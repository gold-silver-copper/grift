#![allow(
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::single_match_else,
    clippy::module_name_repetitions,
    clippy::inline_always,
    clippy::needless_pass_by_value,
    clippy::wildcard_imports,
    clippy::iter_without_into_iter,
    clippy::iter_filter_is_ok
)]

//! Growable, allocator-backed arena storage.
//!
//! Runtime objects occupy slots in an [`alloc::vec::Vec`]. Stable
//! [`ArenaIndex`] values address those slots, so growth and vector relocation do
//! not invalidate language references. Freed slots form a singly linked free
//! list and are reused before the vector grows.
//!
//! The arena remains `no_std` and safe Rust. It requires the embedding program
//! to provide a global allocator through Rust's `alloc` crate.

use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

// ============================================================================
// Core Types
// ============================================================================

/// Opaque index into an [`Arena`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArenaIndex(usize);

/// Define every well-known runtime singleton index from one source of truth.
macro_rules! define_singletons {
    (
        $(
            $(#[$meta:meta])*
            $name:ident = $idx:expr,
        )*
        ; FIRST_FREE = $first_free:expr
    ) => {
        impl ArenaIndex {
            $(
                $(#[$meta])*
                pub const $name: ArenaIndex = ArenaIndex($idx);
            )*

            /// First slot after the fixed runtime singleton layout.
            pub const FIRST_FREE: ArenaIndex = ArenaIndex($first_free);

            /// Singleton slots retained by every collection.
            pub const ROOTS: &'static [ArenaIndex] = &[
                $(ArenaIndex($idx),)*
            ];
        }
    };
}

define_singletons! {
    /// Empty list / nil singleton.
    NIL = 0,
    /// Boolean true singleton.
    TRUE = 1,
    /// Boolean false singleton.
    FALSE = 2,
    /// Inert singleton.
    INERT = 3,
    /// Ignore singleton.
    IGNORE = 4,
    /// Ground environment singleton.
    GROUND_ENV = 5,
    /// Global environment singleton. Slot 6 stores its parent list.
    GLOBAL_ENV = 7,
    /// Evaluator GC-root stack anchor.
    GC_ROOTS = 8,
    /// Symbol intern-list anchor.
    INTERN_LIST = 9,
    ; FIRST_FREE = 10
}

impl ArenaIndex {
    /// Return the singleton index for a Rust boolean.
    #[inline]
    pub const fn from_bool(value: bool) -> Self {
        if value { Self::TRUE } else { Self::FALSE }
    }

    /// Construct a low-level slot index.
    ///
    /// Fabricated indices remain memory-safe but fail arena validation unless
    /// they identify a currently occupied slot.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Return the underlying slot number.
    #[inline]
    pub const fn raw(self) -> usize {
        self.0
    }

    /// Return whether this is the nil singleton index.
    #[inline]
    pub const fn is_nil(self) -> bool {
        self.0 == 0
    }
}

impl Default for ArenaIndex {
    fn default() -> Self {
        Self::NIL
    }
}

impl core::fmt::Display for ArenaIndex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "@{}", self.0)
    }
}

/// Errors produced by arena and language operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaError {
    /// Arena storage reservation failed because of allocator exhaustion or
    /// a capacity overflow.
    OutOfMemory,
    /// A slot index is outside the current logical slot range.
    IndexOutOfBounds,
    /// A slot exists but is currently vacant.
    IndexNotAllocated,
    /// Internal storage was borrowed incompatibly, or a mutating traversal
    /// callback changed the arena before its copied value could be written back.
    BorrowConflict,
    /// An argument was invalid.
    InvalidArgument,
    /// A form or builtin received the wrong number of operands.
    ArityError,
    /// Garbage-collector tracing failed.
    TraceError,
    /// A cycle was detected where cycles are invalid.
    Cyclic,
    /// A runtime value had the wrong type.
    TypeError,
    /// Source parsing failed at the given one-based location.
    ParseError {
        /// One-based line number.
        line: u32,
        /// One-based column number.
        col: u32,
    },
    /// Checked arithmetic overflowed.
    ArithmeticOverflow,
    /// Division or modulo by zero was requested.
    DivisionByZero,
    /// No requested variable binding was found.
    UnboundVariable,
    /// A non-callable value was applied.
    NotCallable,
    /// Mutation of an immutable environment was requested.
    ImmutableEnvironment,
    /// A same-frame binding already exists.
    AlreadyDefined,
}

impl ArenaError {
    /// Return a compact human-readable description.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::OutOfMemory => "Arena storage reservation failed",
            Self::IndexOutOfBounds => "Index out of bounds",
            Self::IndexNotAllocated => "Index not allocated",
            Self::BorrowConflict => "Conflicting arena mutation or storage borrow",
            Self::InvalidArgument => "Invalid argument",
            Self::ArityError => "Arity error",
            Self::TraceError => "Error during GC tracing",
            Self::Cyclic => "Cycle detected in evaluation",
            Self::TypeError => "Type error",
            Self::ParseError { .. } => "Parse error",
            Self::ArithmeticOverflow => "Arithmetic overflow",
            Self::DivisionByZero => "Division by zero",
            Self::UnboundVariable => "Unbound variable",
            Self::NotCallable => "Not callable",
            Self::ImmutableEnvironment => "Attempt to mutate immutable environment",
            Self::AlreadyDefined => "Variable already defined",
        }
    }

    /// Return whether allocation failed.
    pub const fn is_out_of_memory(&self) -> bool {
        matches!(self, Self::OutOfMemory)
    }

    /// Return whether an index was invalid or vacant.
    pub const fn is_invalid_index(&self) -> bool {
        matches!(self, Self::IndexOutOfBounds | Self::IndexNotAllocated)
    }

    /// Return whether tracing failed.
    pub const fn is_trace_error(&self) -> bool {
        matches!(self, Self::TraceError)
    }
}

impl core::fmt::Display for ArenaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ParseError { line, col } => {
                write!(f, "Parse error at line {line}, column {col}")
            }
            other => f.write_str(other.as_str()),
        }
    }
}

/// Result type for arena operations.
pub type ArenaResult<T> = Result<T, ArenaError>;

#[derive(Clone, Copy)]
enum Slot<T: Copy> {
    Free { next_free: Option<usize> },
    Occupied { value: T },
}

// ============================================================================
// Statistics
// ============================================================================

/// Statistics returned by garbage collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GcStats {
    /// Number of reachable objects marked.
    pub marked: usize,
    /// Number of unreachable objects reclaimed.
    pub collected: usize,
    /// Number of live objects before collection.
    pub total_before: usize,
}

impl GcStats {
    /// Return whether collection reclaimed anything.
    pub const fn did_collect(&self) -> bool {
        self.collected > 0
    }

    /// Return the number of live objects after collection.
    pub const fn remaining(&self) -> usize {
        self.total_before - self.collected
    }

    /// Return the fraction of objects reclaimed.
    pub fn collection_ratio(&self) -> f32 {
        if self.total_before == 0 {
            0.0
        } else {
            self.collected as f32 / self.total_before as f32
        }
    }

    /// Return the fraction of objects retained.
    pub fn survival_ratio(&self) -> f32 {
        if self.total_before == 0 {
            1.0
        } else {
            self.marked as f32 / self.total_before as f32
        }
    }
}

/// Snapshot of growable arena storage usage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArenaStats {
    /// Logical number of slots, both occupied and vacant.
    pub slot_count: usize,
    /// Number of currently occupied slots.
    pub allocated: usize,
    /// Number of vacant slots immediately available for reuse.
    pub vacant: usize,
    /// Storage currently reserved by the backing vector.
    pub reserved_capacity: usize,
    /// Ratio of discontiguous vacant runs to logical slots.
    pub fragmentation: f32,
}

impl ArenaStats {
    /// Return occupied logical slots as a percentage.
    pub fn usage_percent(&self) -> f32 {
        if self.slot_count == 0 {
            0.0
        } else {
            (self.allocated as f32 / self.slot_count as f32) * 100.0
        }
    }

    /// Return vacant logical slots as a percentage.
    pub fn free_percent(&self) -> f32 {
        if self.slot_count == 0 {
            100.0
        } else {
            (self.vacant as f32 / self.slot_count as f32) * 100.0
        }
    }

    /// Return whether no objects are allocated.
    pub const fn is_empty(&self) -> bool {
        self.allocated == 0
    }

    /// Return whether fragmentation exceeds `threshold`.
    pub fn is_fragmented(&self, threshold: f32) -> bool {
        self.fragmentation > threshold
    }
}

// ============================================================================
// Traits
// ============================================================================

/// Recursively delete children owned by an arena value.
pub trait ArenaDelete<T: Copy> {
    /// Delete children referenced by this value.
    fn delete_recursive(&self, arena: &Arena<T>) -> ArenaResult<()>;
}

/// Deep-copy children owned by an arena value.
pub trait ArenaCopy<T: Copy> {
    /// Return a copy whose child indices refer to newly allocated values.
    fn copy_deep(&self, arena: &Arena<T>) -> ArenaResult<T>;
}

/// Enumerate arena indices reachable from a value.
pub trait Trace<T: Copy> {
    /// Call `tracer` once for every directly referenced arena value.
    fn trace<F: FnMut(ArenaIndex)>(&self, tracer: F);

    /// Trace with read access to the arena when metadata is stored indirectly.
    fn trace_with_arena<F: FnMut(ArenaIndex)>(&self, _arena: &Arena<T>, tracer: F) {
        self.trace(tracer);
    }
}

// ============================================================================
// Iterator
// ============================================================================

/// Copying iterator over the logical slot range that exists when iteration begins.
///
/// Occupancy and values are observed when each slot is reached. Slots appended
/// after iterator construction are not visited; slots freed before visitation
/// are skipped; and a formerly vacant slot allocated before visitation is
/// yielded. No `RefCell` guard escapes.
pub struct ArenaIterator<'a, T: Copy> {
    arena: &'a Arena<T>,
    current: usize,
    end: usize,
}

impl<T: Copy> Iterator for ArenaIterator<'_, T> {
    type Item = (ArenaIndex, T);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < self.end {
            let index = ArenaIndex::new(self.current);
            self.current += 1;
            if let Ok(value) = self.arena.get(index) {
                return Some((index, value));
            }
        }
        None
    }
}

// ============================================================================
// Arena
// ============================================================================

/// Growable, index-addressed arena with free-list slot reuse.
pub struct Arena<T: Copy> {
    slots: RefCell<Vec<Slot<T>>>,
    free_head: Cell<Option<usize>>,
    len: Cell<usize>,
    mutation_epoch: Cell<usize>,
}

impl<T: Copy> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy> Arena<T> {
    /// Construct an empty arena with no allocation.
    pub const fn new() -> Self {
        Self {
            slots: RefCell::new(Vec::new()),
            free_head: Cell::new(None),
            len: Cell::new(0),
            mutation_epoch: Cell::new(0),
        }
    }

    fn record_mutation(&self) {
        self.mutation_epoch
            .set(self.mutation_epoch.get().wrapping_add(1));
    }

    /// Return the number of logical slots, including reusable vacancies.
    pub fn slot_count(&self) -> usize {
        self.slots.borrow().len()
    }

    /// Return the backing vector's reserved storage capacity.
    pub fn reserved_capacity(&self) -> usize {
        self.slots.borrow().capacity()
    }

    /// Return the number of occupied slots.
    #[inline]
    pub fn len(&self) -> usize {
        self.len.get()
    }

    /// Return whether no slots are occupied.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the number of vacant logical slots available for reuse.
    pub fn vacant(&self) -> usize {
        self.slot_count() - self.len()
    }

    /// Ensure `additional` allocations can succeed without growing storage.
    pub fn reserve(&self, additional: usize) -> ArenaResult<()> {
        let reusable = self.vacant();
        let needed = additional.saturating_sub(reusable);
        if needed == 0 {
            return Ok(());
        }
        self.slots
            .try_borrow_mut()
            .map_err(|_| ArenaError::BorrowConflict)?
            .try_reserve(needed)
            .map_err(|_| ArenaError::OutOfMemory)
    }

    /// Allocate a value, reusing a vacant slot before growing the vector.
    pub fn alloc(&self, value: T) -> ArenaResult<ArenaIndex> {
        let new_len = self
            .len
            .get()
            .checked_add(1)
            .ok_or(ArenaError::OutOfMemory)?;
        let mut slots = self
            .slots
            .try_borrow_mut()
            .map_err(|_| ArenaError::BorrowConflict)?;

        let index = if let Some(index) = self.free_head.get() {
            let next_free = match slots.get(index).copied() {
                Some(Slot::Free { next_free }) => next_free,
                _ => return Err(ArenaError::InvalidArgument),
            };
            slots[index] = Slot::Occupied { value };
            self.free_head.set(next_free);
            index
        } else {
            slots.try_reserve(1).map_err(|_| ArenaError::OutOfMemory)?;
            let index = slots.len();
            slots.push(Slot::Occupied { value });
            index
        };

        self.len.set(new_len);
        self.record_mutation();
        Ok(ArenaIndex::new(index))
    }

    fn validate_index(&self, index: ArenaIndex) -> ArenaResult<usize> {
        let slots = self
            .slots
            .try_borrow()
            .map_err(|_| ArenaError::BorrowConflict)?;
        match slots.get(index.raw()) {
            None => Err(ArenaError::IndexOutOfBounds),
            Some(Slot::Free { .. }) => Err(ArenaError::IndexNotAllocated),
            Some(Slot::Occupied { .. }) => Ok(index.raw()),
        }
    }

    /// Return a copy of an occupied value.
    pub fn get(&self, index: ArenaIndex) -> ArenaResult<T> {
        let slots = self
            .slots
            .try_borrow()
            .map_err(|_| ArenaError::BorrowConflict)?;
        match slots.get(index.raw()).copied() {
            None => Err(ArenaError::IndexOutOfBounds),
            Some(Slot::Free { .. }) => Err(ArenaError::IndexNotAllocated),
            Some(Slot::Occupied { value }) => Ok(value),
        }
    }

    /// Replace an occupied value.
    pub fn set(&self, index: ArenaIndex, value: T) -> ArenaResult<()> {
        let mut slots = self
            .slots
            .try_borrow_mut()
            .map_err(|_| ArenaError::BorrowConflict)?;
        match slots.get_mut(index.raw()) {
            None => Err(ArenaError::IndexOutOfBounds),
            Some(Slot::Free { .. }) => Err(ArenaError::IndexNotAllocated),
            Some(slot @ Slot::Occupied { .. }) => {
                *slot = Slot::Occupied { value };
                self.record_mutation();
                Ok(())
            }
        }
    }

    /// Modify a copied value and write it back into the occupied slot.
    ///
    /// No arena borrow is held while `f` runs. Reentrant reads are supported,
    /// but if `f` mutates the arena, this method returns
    /// [`ArenaError::BorrowConflict`] rather than risk overwriting a slot that
    /// was freed and reused during the callback.
    pub fn modify<F>(&self, index: ArenaIndex, f: F) -> ArenaResult<()>
    where
        F: FnOnce(&mut T),
    {
        let mut value = self.get(index)?;
        let epoch = self.mutation_epoch.get();
        f(&mut value);
        if self.mutation_epoch.get() != epoch {
            return Err(ArenaError::BorrowConflict);
        }
        self.set(index, value)
    }

    /// Return a value when the index is occupied.
    pub fn try_get(&self, index: ArenaIndex) -> Option<T> {
        self.get(index).ok()
    }

    /// Swap two occupied slots.
    pub fn swap(&self, a: ArenaIndex, b: ArenaIndex) -> ArenaResult<()> {
        let mut slots = self
            .slots
            .try_borrow_mut()
            .map_err(|_| ArenaError::BorrowConflict)?;
        for index in [a.raw(), b.raw()] {
            match slots.get(index) {
                None => return Err(ArenaError::IndexOutOfBounds),
                Some(Slot::Free { .. }) => return Err(ArenaError::IndexNotAllocated),
                Some(Slot::Occupied { .. }) => {}
            }
        }
        slots.swap(a.raw(), b.raw());
        self.record_mutation();
        Ok(())
    }

    /// Replace an occupied value and return its previous value.
    pub fn replace(&self, index: ArenaIndex, value: T) -> ArenaResult<T> {
        let mut slots = self
            .slots
            .try_borrow_mut()
            .map_err(|_| ArenaError::BorrowConflict)?;
        match slots.get_mut(index.raw()) {
            None => Err(ArenaError::IndexOutOfBounds),
            Some(Slot::Free { .. }) => Err(ArenaError::IndexNotAllocated),
            Some(slot @ Slot::Occupied { .. }) => {
                let Slot::Occupied { value: old } =
                    core::mem::replace(slot, Slot::Occupied { value })
                else {
                    unreachable!()
                };
                self.record_mutation();
                Ok(old)
            }
        }
    }

    /// Mark an occupied slot vacant and add it to the free list.
    pub fn free(&self, index: ArenaIndex) -> ArenaResult<()> {
        let mut slots = self
            .slots
            .try_borrow_mut()
            .map_err(|_| ArenaError::BorrowConflict)?;
        match slots.get_mut(index.raw()) {
            None => Err(ArenaError::IndexOutOfBounds),
            Some(Slot::Free { .. }) => Err(ArenaError::IndexNotAllocated),
            Some(slot @ Slot::Occupied { .. }) => {
                *slot = Slot::Free {
                    next_free: self.free_head.get(),
                };
                self.free_head.set(Some(index.raw()));
                self.len.set(self.len.get() - 1);
                self.record_mutation();
                Ok(())
            }
        }
    }

    /// Return whether an index identifies an occupied slot.
    pub fn is_allocated(&self, index: ArenaIndex) -> bool {
        self.validate_index(index).is_ok()
    }

    /// Remove all logical slots while retaining vector reservation.
    pub fn clear(&self) -> ArenaResult<()> {
        self.slots
            .try_borrow_mut()
            .map_err(|_| ArenaError::BorrowConflict)?
            .clear();
        self.free_head.set(None);
        self.len.set(0);
        self.record_mutation();
        Ok(())
    }

    /// Iterate over copied occupied values in slot order.
    pub fn iter(&self) -> ArenaIterator<'_, T> {
        ArenaIterator {
            arena: self,
            current: 0,
            end: self.slot_count(),
        }
    }

    /// Return a usage snapshot.
    pub fn stats(&self) -> ArenaStats {
        let slots = self.slots.borrow();
        let slot_count = slots.len();
        let allocated = self.len();
        ArenaStats {
            slot_count,
            allocated,
            vacant: slot_count - allocated,
            reserved_capacity: slots.capacity(),
            fragmentation: Self::fragmentation(&slots),
        }
    }

    fn fragmentation(slots: &[Slot<T>]) -> f32 {
        let (fragments, _) = slots
            .iter()
            .fold((0usize, false), |(count, was_free), slot| {
                let is_free = matches!(slot, Slot::Free { .. });
                (count + usize::from(is_free && !was_free), is_free)
            });
        if slots.is_empty() {
            0.0
        } else {
            fragments as f32 / slots.len() as f32
        }
    }

    /// Validate live counts, slot kinds, and free-list reachability.
    pub fn validate(&self) -> bool {
        let Ok(slots) = self.slots.try_borrow() else {
            return false;
        };
        let occupied = slots
            .iter()
            .filter(|slot| matches!(slot, Slot::Occupied { .. }))
            .count();
        if occupied != self.len() {
            return false;
        }

        let mut visited = Vec::new();
        if visited.try_reserve_exact(slots.len()).is_err() {
            return false;
        }
        visited.resize(slots.len(), false);

        let mut free_count = 0usize;
        let mut current = self.free_head.get();
        while let Some(index) = current {
            let Some(slot) = slots.get(index) else {
                return false;
            };
            if visited[index] {
                return false;
            }
            visited[index] = true;
            match slot {
                Slot::Free { next_free } => {
                    free_count += 1;
                    current = *next_free;
                }
                Slot::Occupied { .. } => return false,
            }
        }

        if free_count + occupied != slots.len() {
            return false;
        }
        slots
            .iter()
            .enumerate()
            .all(|(index, slot)| !matches!(slot, Slot::Free { .. }) || visited[index])
    }

    /// Return whether a raw slot number is occupied.
    pub fn is_slot_occupied(&self, slot_index: usize) -> bool {
        self.slots
            .try_borrow()
            .ok()
            .and_then(|slots| slots.get(slot_index).copied())
            .is_some_and(|slot| matches!(slot, Slot::Occupied { .. }))
    }

    /// Return all occupied indices in slot order.
    pub fn allocated_indices(&self) -> ArenaResult<Vec<ArenaIndex>> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.len())
            .map_err(|_| ArenaError::OutOfMemory)?;
        result.extend(self.iter().map(|(index, _)| index));
        Ok(result)
    }

    /// Visit copied occupied values without retaining an arena borrow.
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(ArenaIndex, &T),
    {
        self.iter().for_each(|(index, value)| f(index, &value));
    }

    /// Visit copied occupied values and write each result back.
    ///
    /// Reentrant reads are supported. If `f` mutates the arena, traversal
    /// stops with [`ArenaError::BorrowConflict`] before writing back the copied
    /// value for that iteration.
    pub fn for_each_mut<F>(&self, mut f: F) -> ArenaResult<()>
    where
        F: FnMut(ArenaIndex, &mut T),
    {
        let end = self.slot_count();
        for raw in 0..end {
            let index = ArenaIndex::new(raw);
            if let Ok(mut value) = self.get(index) {
                let epoch = self.mutation_epoch.get();
                f(index, &mut value);
                if self.mutation_epoch.get() != epoch {
                    return Err(ArenaError::BorrowConflict);
                }
                self.set(index, value)?;
            }
        }
        Ok(())
    }

    /// Count occupied values matching a predicate.
    pub fn count_where<F>(&self, predicate: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        self.iter().filter(|(_, value)| predicate(value)).count()
    }

    /// Find the first occupied value matching a predicate.
    pub fn find<F>(&self, predicate: F) -> Option<(ArenaIndex, T)>
    where
        F: Fn(&T) -> bool,
    {
        self.iter().find(|(_, value)| predicate(value))
    }

    /// Return whether any occupied value matches a predicate.
    pub fn any<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter().any(|(_, value)| predicate(&value))
    }

    /// Return whether every occupied value matches a predicate.
    pub fn all<F>(&self, predicate: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter().all(|(_, value)| predicate(&value))
    }

    /// Recursively delete a value and its owned children.
    pub fn delete_recursive(&self, index: ArenaIndex) -> ArenaResult<()>
    where
        T: ArenaDelete<T>,
    {
        let value = self.get(index)?;
        value.delete_recursive(self)?;
        self.free(index)
    }

    /// Deep-copy a value and its owned children.
    pub fn copy_deep(&self, index: ArenaIndex) -> ArenaResult<ArenaIndex>
    where
        T: ArenaCopy<T>,
    {
        let copied = self.get(index)?.copy_deep(self)?;
        self.alloc(copied)
    }

    /// Perform mark-and-sweep collection from one root set.
    pub fn collect_garbage(&self, roots: &[ArenaIndex]) -> ArenaResult<GcStats>
    where
        T: Trace<T>,
    {
        self.collect_garbage_multi(&[roots])
    }

    /// Perform mark-and-sweep collection from multiple root sets.
    pub fn collect_garbage_multi(&self, root_sets: &[&[ArenaIndex]]) -> ArenaResult<GcStats>
    where
        T: Trace<T>,
    {
        let slot_count = self.slot_count();
        let total_before = self.len();

        let mut marked = Vec::new();
        marked
            .try_reserve_exact(slot_count)
            .map_err(|_| ArenaError::OutOfMemory)?;
        marked.resize(slot_count, false);

        let mut mark_stack = Vec::new();
        mark_stack
            .try_reserve_exact(slot_count)
            .map_err(|_| ArenaError::OutOfMemory)?;

        for roots in root_sets {
            for &root in *roots {
                let index = root.raw();
                if index < slot_count && !marked[index] && self.is_allocated(root) {
                    marked[index] = true;
                    mark_stack.push(index);
                }
            }
        }

        while let Some(raw) = mark_stack.pop() {
            let Ok(value) = self.get(ArenaIndex::new(raw)) else {
                continue;
            };
            value.trace_with_arena(self, |child| {
                let index = child.raw();
                if index < slot_count && !marked[index] && self.is_allocated(child) {
                    marked[index] = true;
                    mark_stack.push(index);
                }
            });
        }

        let marked_count = marked.iter().filter(|&&value| value).count();
        let mut collected = 0usize;
        for (raw, is_marked) in marked.iter().copied().enumerate() {
            let index = ArenaIndex::new(raw);
            if !is_marked && self.is_allocated(index) {
                self.free(index)?;
                collected += 1;
            }
        }

        Ok(GcStats {
            marked: marked_count,
            collected,
            total_before,
        })
    }
}
