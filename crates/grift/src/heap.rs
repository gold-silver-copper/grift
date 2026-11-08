//! The heap: a growable vector of slots, reused through a free list and reclaimed by mark and
//! sweep. A new slot first reserves its entry on the mark stack, so a collection needs no memory of
//! its own.

use crate::evaluate::Native;
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
    Pair(Value, Value, Origin),
    String(Box<str>),
    /// An optional parent and the bindings.
    Context(Option<Reference>, Bindings),
    Operative(Operative),
    Applicative(Value),
    Bytes(Vec<u8>),
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
    /// Set as a collection first reaches the slot, so it goes on the mark stack once at most.
    marked: Cell<bool>,
    obj: Object,
}

// A pair's origin fits in the room an operative already needs.
const _: () = assert!(size_of::<Slot>() == 72);

/// Where a pair came from: the source and line it was read at, if under a name; whether the
/// library made it, by reading its text or by running its code; and whether it is one of its
/// source's outer forms.
#[derive(Clone, Copy)]
pub(crate) struct Origin {
    pub place: Option<(Symbol, u32)>,
    pub library: bool,
    pub outer: bool,
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
        _ => 1,
    }
}

pub(crate) struct Heap {
    slots: Vec<Slot>,
    free: u32,
    /// How much may be allocated before the next collection: collection sets it, allocation
    /// spends it, and a refusal empties it.
    until_gc: Cell<usize>,
    /// The mark stack, empty between collections: every slot reserved its entry here when it was
    /// made, so a collection never grows it.
    marks: Vec<u32>,
    /// What the last collection found alive, by weight, for tests of what a run keeps.
    #[cfg(test)]
    live: usize,
    /// Operatives made, for tests of what a form costs.
    #[cfg(test)]
    pub operatives: usize,
    /// Refuse new slots past this length, to test recovery without replacing the allocator.
    #[cfg(test)]
    pub slot_limit: usize,
    /// Refuse the kept table room past this length, to test a park with no memory to spare.
    #[cfg(test)]
    pub kept_limit: usize,
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
            until_gc: Cell::new(GC_MIN),
            marks: Vec::new(),
            #[cfg(test)]
            live: 0,
            #[cfg(test)]
            operatives: 0,
            #[cfg(test)]
            slot_limit: usize::MAX,
            #[cfg(test)]
            kept_limit: usize::MAX,
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
        let until_gc = self.until_gc.get_mut();
        *until_gc = until_gc.saturating_sub(weight(&obj));
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
        self.marks
            .try_reserve(self.slots.len() + 1)
            .map_err(|_| self.refused())?;
        let slot = Slot {
            generation: 0,
            marked: Cell::new(false),
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
        self.until_gc.set(self.live.max(GC_MIN));
    }

    /// Whether collection is due at a machine safe point or before a host's run needs slots.
    #[inline]
    pub fn due(&self) -> bool {
        #[cfg(test)]
        if self.stress {
            return true;
        }
        self.until_gc.get() == 0
    }

    /// The next safe point collects.
    pub fn request_collection(&self) {
        self.until_gc.set(0);
    }

    /// Marks everything reachable from `roots` and frees the rest, with no memory of its own: a
    /// slot goes on the mark stack only as it is first marked, so once at most, and every slot
    /// reserved its entry as it was made. `parked` is the weight of what
    /// outside the heap the next collection walks too, counted as live.
    pub fn collect(&mut self, roots: impl Iterator<Item = Value>, parked: usize) {
        #[cfg(test)]
        {
            self.collections += 1;
        }
        let slots = &self.slots;
        let marks = &mut self.marks;
        roots.for_each(|v| mark(slots, marks, v));
        while let Some(idx) = marks.pop() {
            if let Some(s) = slots.get(idx as usize) {
                children(&s.obj, |v| mark(slots, marks, v));
            }
        }
        let mut live = parked;
        for (idx, s) in (0..).zip(self.slots.iter_mut()) {
            if s.marked.take() {
                live = live.saturating_add(weight(&s.obj));
            } else if !matches!(s.obj, Object::Free(_)) {
                s.obj = Object::Free(self.free);
                // A slot whose generations run out is retired, never reused: its last one, which
                // no reference has, means free for good.
                s.generation = s.generation.saturating_add(1);
                if s.generation != u32::MAX {
                    self.free = idx;
                }
            }
        }
        *self.until_gc.get_mut() = live.max(GC_MIN);
        #[cfg(test)]
        {
            self.live = live;
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// The mark stack's room, for tests that every slot has an entry.
    #[cfg(test)]
    pub fn mark_room(&self) -> usize {
        self.marks.capacity()
    }
}

/// Puts `v`'s object on the mark stack, unless a collection already reached it.
fn mark(slots: &[Slot], marks: &mut Vec<u32>, v: Value) {
    let Some(r) = reference(v) else { return };
    if let Some(s) = slots.get(r.idx as usize)
        && s.generation == r.generation
        && !s.marked.replace(true)
    {
        marks.push(r.idx);
    }
}

/// What `obj` refers to.
fn children(obj: &Object, mut f: impl FnMut(Value)) {
    match obj {
        Object::Free(_) | Object::String(_) | Object::Bytes(_) => {}
        Object::Pair(a, d, _) => {
            f(*a);
            f(*d);
        }
        Object::Context(parent, bindings) => {
            if let Some(p) = parent {
                f(Value::Context(*p));
            }
            bindings.values().copied().for_each(f);
        }
        Object::Operative(op) => {
            f(op.parameters);
            f(op.body);
            f(Value::Context(op.context));
        }
        Object::Applicative(c) => f(*c),
    }
}
