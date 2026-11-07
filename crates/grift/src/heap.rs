//! The heap: a growable vector of slots, reused through a free list and reclaimed by mark and
//! sweep.

use crate::evaluate::{Machine, Native};
use crate::local::{Error, K, Symbol, Value, push};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hash, Hasher};

/// A reference to a heap slot. The generation tells a live object from a reclaimed one.
#[derive(Clone, Copy, Debug, Eq)]
#[cfg_attr(not(test), derive(PartialEq))]
pub(crate) struct Reference {
    pub idx: u32,
    pub generation: u32,
}

#[cfg(test)]
impl PartialEq for Reference {
    fn eq(&self, other: &Reference) -> bool {
        crate::local::COMPARED.with(|n| n.set(n.get() + 1));
        self.idx == other.idx && self.generation == other.generation
    }
}

impl Hash for Reference {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(self.idx);
    }
}

/// A compound operative: its parameter tree, its context parameter, its body's forms, the context it
/// was made in, how many bindings a call makes, and the native twin that runs it, for the few
/// library forms that have one.
#[derive(Clone, Copy)]
pub(crate) struct Operative {
    pub parameters: Value,
    pub context_parameter: Value,
    pub body: Value,
    pub context: Reference,
    pub size: u32,
    pub native: Option<Native>,
}

pub(crate) enum Object {
    /// A free slot and the next free one.
    Free(u32),
    Pair(Value, Value),
    String(Box<str>),
    /// An optional parent and the bindings.
    Context(Option<Reference>, Bindings),
    Operative(Operative),
    Applicative(Value),
    Bytes(Vec<u8>),
    /// A parked run's machine, none once it is taken up again: a vector, not a box, so that making
    /// room for it can report failure. Only a kept entry refers to one, under a pair's reference
    /// (marking follows a reference whatever its kind says): no program or host value ever holds it.
    Fiber(Vec<Machine>),
}

/// A context's bindings.
pub(crate) type Bindings = HashMap<Symbol, Value, BuildHasherDefault<IdHasher>>;

/// A set of references, for checking in constant time whether one was met before.
pub(crate) type Seen = HashSet<Reference, BuildHasherDefault<IdHasher>>;

/// Hashes a symbol or a reference by its id, a small integer: one multiplication spreads it over
/// the bits the table looks at.
#[derive(Default)]
pub(crate) struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0.wrapping_mul(0x9e37_79b9_7f4a_7c15)
    }

    /// Never called: both kinds of id hash with `write_u32`.
    fn write(&mut self, _: &[u8]) {}

    fn write_u32(&mut self, n: u32) {
        self.0 = u64::from(n);
    }
}

struct Slot {
    generation: u32,
    marked: bool,
    /// Live when the library had been loaded: its code, and the code it built.
    library: bool,
    obj: Object,
}

const NONE: u32 = u32::MAX;

/// Collections start once this many objects were allocated since the last one, or as many as
/// survived it if that is more, so the heap stays within about twice the live data. A string or
/// bytes value counts as many objects as its storage would fill slots, so large chunks dropped one
/// after another bring the next collection nearer as their size, not their number, says.
const GC_MIN: usize = 4096;

/// How many objects `obj` counts as, for when to collect.
#[inline]
fn weight(obj: &Object) -> usize {
    match obj {
        Object::String(s) => 1 + s.len().div_ceil(size_of::<Slot>()),
        Object::Bytes(b) => 1 + b.capacity().div_ceil(size_of::<Slot>()),
        Object::Fiber(room) => 1 + room.iter().map(Machine::weight).sum::<usize>(),
        _ => 1,
    }
}

pub(crate) struct Heap {
    slots: Vec<Slot>,
    free: u32,
    since_gc: Cell<usize>,
    live: usize,
    marks: Vec<Reference>,
    /// Operatives made, for tests of what a form costs.
    #[cfg(test)]
    pub operatives: usize,
    /// Refuse new slots past this length, to test recovery without replacing the allocator.
    #[cfg(test)]
    pub slot_limit: usize,
    /// Collect at every safe point, to test what a collection could lose.
    #[cfg(test)]
    pub stress: bool,
    /// Refuse a slot after this many more, to choose where memory runs out; and how many were
    /// refused. `refuse_once` lets the next one through.
    #[cfg(test)]
    pub refuse_in: Option<usize>,
    #[cfg(test)]
    pub refuse_once: bool,
    #[cfg(test)]
    pub refused: usize,
    /// Collections so far.
    #[cfg(test)]
    pub collections: usize,
}

impl Default for Heap {
    fn default() -> Heap {
        Heap {
            slots: Vec::new(),
            free: NONE,
            since_gc: Cell::new(0),
            live: 0,
            marks: Vec::new(),
            #[cfg(test)]
            operatives: 0,
            #[cfg(test)]
            slot_limit: usize::MAX,
            #[cfg(test)]
            stress: false,
            #[cfg(test)]
            refuse_in: None,
            #[cfg(test)]
            refuse_once: false,
            #[cfg(test)]
            refused: 0,
            #[cfg(test)]
            collections: 0,
        }
    }
}

fn reference(v: Value) -> Option<Reference> {
    match v {
        Value::Pair(r)
        | Value::String(r)
        | Value::Bytes(r)
        | Value::Context(r)
        | Value::Operative(r)
        | Value::Applicative(r) => Some(r),
        _ => None,
    }
}

impl Reference {
    /// No object: a reference no slot has.
    pub(crate) const NONE: Reference = Reference {
        idx: NONE,
        generation: 0,
    };
}

impl Heap {
    #[inline]
    pub fn alloc(&mut self, obj: Object) -> Result<Reference, Error> {
        let since_gc = self.since_gc.get_mut();
        *since_gc = since_gc.saturating_add(weight(&obj));
        #[cfg(test)]
        {
            self.operatives += usize::from(matches!(obj, Object::Operative(_)));
            match self.refuse_in {
                Some(0) => {
                    self.refused += 1;
                    if self.refuse_once {
                        self.refuse_in = None;
                    }
                    return Err(self.refused());
                }
                Some(n) => self.refuse_in = Some(n - 1),
                None => {}
            }
        }
        let idx = self.free;
        if let Some(slot) = self.slots.get_mut(idx as usize)
            && let Object::Free(next) = slot.obj
        {
            self.free = next;
            slot.obj = obj;
            return Ok(Reference {
                idx,
                generation: slot.generation,
            });
        }
        #[cfg(test)]
        if self.slots.len() >= self.slot_limit {
            return Err(self.refused());
        }
        let idx = u32::try_from(self.slots.len())
            .ok()
            .filter(|&i| i != NONE)
            .ok_or_else(|| self.refused())?;
        let slot = Slot {
            generation: 0,
            marked: false,
            library: false,
            obj,
        };
        push(&mut self.slots, slot).map_err(|_| self.refused())?;
        Ok(Reference { idx, generation: 0 })
    }

    /// No room for an object: collection comes next, at the machine's next safe point or as a
    /// host's next run begins.
    #[cold]
    fn refused(&self) -> Error {
        self.request_collection();
        Error::oom()
    }

    /// Marks every object now in the heap as the library's. A slot loses the mark when it is
    /// freed.
    pub fn seal_library(&mut self) {
        for s in &mut self.slots {
            s.library = !matches!(s.obj, Object::Free(_));
        }
    }

    /// The object, and whether it is the library's: its code, or code it built while it was
    /// loaded.
    #[inline]
    pub fn get_code(&self, r: Reference) -> Result<(&Object, bool), Error> {
        self.slot(r)
            .map(|s| (&s.obj, s.library))
            .ok_or_else(|| Error::new(K::StaleValue, &[]))
    }

    /// The slot `r` refers to, if it still holds that object: a slot freed since has another
    /// generation.
    #[inline]
    fn slot(&self, r: Reference) -> Option<&Slot> {
        self.slots
            .get(r.idx as usize)
            .filter(|s| s.generation == r.generation)
    }

    #[inline]
    pub fn get(&self, r: Reference) -> Result<&Object, Error> {
        self.slot(r)
            .map(|s| &s.obj)
            .ok_or_else(|| Error::new(K::StaleValue, &[]))
    }

    #[inline]
    pub fn get_mut(&mut self, r: Reference) -> Result<&mut Object, Error> {
        match self.slots.get_mut(r.idx as usize) {
            Some(s) if s.generation == r.generation => Ok(&mut s.obj),
            _ => Err(Error::new(K::StaleValue, &[])),
        }
    }

    /// What the last collection found alive, by weight.
    #[cfg(test)]
    pub fn live(&self) -> usize {
        self.live
    }

    /// Puts the next collection off, as if the last had just been.
    #[cfg(test)]
    pub fn not_due(&self) {
        self.since_gc.set(0);
    }

    /// Whether collection is due at a machine safe point or before a host's run needs slots.
    #[inline]
    pub fn due(&self) -> bool {
        #[cfg(test)]
        if self.stress {
            return true;
        }
        self.since_gc.get() >= self.live.max(GC_MIN)
    }

    /// The next safe point collects.
    pub fn request_collection(&self) {
        self.since_gc.set(usize::MAX);
    }

    /// Marks everything reachable from `roots` and frees the rest. If the mark stack can't grow,
    /// nothing is freed and the error is `out-of-memory`.
    pub fn collect(&mut self, roots: impl Iterator<Item = Value>) -> Result<(), Error> {
        #[cfg(test)]
        {
            self.collections += 1;
        }
        let mut marks = std::mem::take(&mut self.marks);
        marks.clear();
        let marked = self.mark(roots, &mut marks);
        self.marks = marks;
        if let Err(e) = marked {
            self.slots.iter_mut().for_each(|s| s.marked = false);
            return Err(e);
        }
        self.live = 0;
        for (idx, s) in (0..).zip(self.slots.iter_mut()) {
            if s.marked {
                s.marked = false;
                self.live = self.live.saturating_add(weight(&s.obj));
            } else if !matches!(s.obj, Object::Free(_)) {
                s.obj = Object::Free(self.free);
                s.library = false;
                // A slot whose generations run out is retired, never reused: its last one, which
                // no reference has, means free for good.
                s.generation = s.generation.saturating_add(1);
                if s.generation != u32::MAX {
                    self.free = idx;
                }
            }
        }
        *self.since_gc.get_mut() = 0;
        Ok(())
    }

    fn mark(
        &mut self,
        roots: impl Iterator<Item = Value>,
        marks: &mut Vec<Reference>,
    ) -> Result<(), Error> {
        for root in roots {
            let Some(r) = reference(root) else { continue };
            push(marks, r)?;
            while let Some(r) = marks.pop() {
                let Some(s) = self.slots.get_mut(r.idx as usize) else {
                    continue;
                };
                if s.generation != r.generation || s.marked {
                    continue;
                }
                s.marked = true;
                let mut child = |v: Value| match reference(v) {
                    Some(r) => push(marks, r),
                    None => Ok(()),
                };
                match &s.obj {
                    Object::Free(_) | Object::String(_) | Object::Bytes(_) => {}
                    Object::Pair(a, d) => {
                        child(*a)?;
                        child(*d)?;
                    }
                    Object::Context(parent, bindings) => {
                        if let Some(p) = parent {
                            child(Value::Context(*p))?;
                        }
                        for v in bindings.values() {
                            child(*v)?;
                        }
                    }
                    Object::Operative(op) => {
                        child(op.parameters)?;
                        child(op.body)?;
                        child(Value::Context(op.context))?;
                    }
                    Object::Applicative(c) => child(*c)?,
                    Object::Fiber(room) => {
                        for v in room.iter().flat_map(Machine::roots) {
                            child(v)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }
}
