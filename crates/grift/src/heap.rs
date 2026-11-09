//! The heap: a growable vector of slots, reused through a free list and reclaimed by mark and
//! sweep. A new slot first reserves its entry on the mark stack, so a collection needs no memory of
//! its own.

use crate::evaluate::Native;
use crate::local::{Error, K, Symbol, Value, push};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

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

/// The slot numbers of a trace's combinations, for checking in constant time whether one was met
/// before. The combinations in a trace are live, so a slot number identifies one.
pub(crate) type Seen = HashSet<u32, BuildHasherDefault<IdHasher>>;

/// Hashes a symbol or a slot by its number, a small integer: one multiplication spreads it over
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

/// A collection pays for itself: the next one starts once as much was allocated as the largest
/// part of what this one walked, at least this much. It walks what survived, the roots outside the
/// heap (the running machine, the kept table and its parked runs) and every slot, so the next waits
/// for the largest of those three, the slots counted as half the free ones: that many take free
/// slots without the heap growing, and as the lowest go first the top stays free. The heap stays
/// within about twice the larger of the live data, those roots and the slots it already has; a
/// free third at the top of the slots is given back, so a past peak leaves no walking behind. Its
/// memory goes back only down to the highest slot still alive: objects never move, so one
/// long-lived object made after a peak keeps the slots below it, never more than the peak. A
/// retired slot counts as live. A string, bytes value or context counts as many objects as its
/// storage would fill slots, so large chunks dropped one after another bring the next collection
/// nearer as their size, not their number, says.
const GC_MIN: usize = 4096;

/// How many objects `obj` counts as, for when to collect.
#[inline]
fn weight(obj: &Object) -> usize {
    match obj {
        Object::String(s) => 1 + s.len().div_ceil(size_of::<Slot>()),
        Object::Bytes(b) => 1 + b.capacity().div_ceil(size_of::<Slot>()),
        Object::Context(_, b) => {
            1 + (b.capacity() * size_of::<(Symbol, Value)>()).div_ceil(size_of::<Slot>())
        }
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
    /// The generation a new slot starts at: past every generation of a slot given back, so no
    /// reference to one that was there matches the slot made in its place.
    fresh: u32,
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
    /// Weight allocated, and slots and roots outside the heap that collections walked, so far.
    #[cfg(test)]
    pub work: (usize, usize),
}

impl Default for Heap {
    fn default() -> Heap {
        Heap {
            slots: Vec::new(),
            free: NONE,
            until_gc: Cell::new(GC_MIN),
            marks: Vec::new(),
            fresh: 0,
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
            #[cfg(test)]
            work: (0, 0),
        }
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
            self.work.0 += weight(&obj);
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
        let generation = self.fresh;
        let slot = Slot {
            generation,
            marked: Cell::new(false),
            obj,
        };
        push(&mut self.slots, slot).map_err(|_| self.refused())?;
        Ok(Reference { idx, generation })
    }

    /// A new empty context.
    #[inline]
    pub fn context(&mut self, parent: Option<Reference>) -> Result<Reference, Error> {
        self.alloc(Object::Context(parent, Bindings::default()))
    }

    /// No room for an object: collection comes next, at the machine's next safe point or as a
    /// host's next run begins.
    #[cold]
    fn refused(&self) -> Error {
        self.request_collection();
        Error::oom()
    }

    #[inline]
    pub fn get(&self, r: Reference) -> Result<&Object, Error> {
        match self.slots.get(r.idx as usize) {
            Some(s) if s.generation == r.generation => Ok(&s.obj),
            _ => Err(Error::new(K::StaleValue, &[])),
        }
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
    /// reserved its entry as it was made. `walked` is how much outside the heap the collection
    /// walks too. The free list is made again from the top down, so the lowest slots are reused
    /// first and the top empties, to be given back.
    pub fn collect(&mut self, roots: impl Iterator<Item = Value>, walked: usize) {
        #[cfg(test)]
        {
            self.collections += 1;
            self.work.1 += self.slots.len() + walked;
        }
        let slots = &self.slots;
        let marks = &mut self.marks;
        #[cfg(test)]
        let room = marks.capacity();
        roots.for_each(|v| mark(slots, marks, v));
        while let Some(idx) = marks.pop() {
            if let Some(s) = slots.get(idx as usize) {
                children(&s.obj, |v| mark(slots, marks, v));
            }
        }
        #[cfg(test)]
        assert_eq!(marks.capacity(), room, "marking grew the mark stack");
        let (mut live, mut free, mut top) = (0usize, 0, None);
        self.free = NONE;
        for (idx, s) in (0..NONE).zip(self.slots.iter_mut()).rev() {
            if s.marked.take() {
                live = live.saturating_add(weight(&s.obj));
                top = top.or(Some(idx));
                continue;
            }
            if !matches!(s.obj, Object::Free(_)) {
                // A slot whose generations run out is retired, never reused: its last one, which
                // no reference has, means free for good.
                s.generation = s.generation.saturating_add(1);
            }
            if s.generation == u32::MAX {
                // Swept every time and never reused, it counts as live.
                s.obj = Object::Free(NONE);
                top = top.or(Some(idx));
                live = live.saturating_add(1);
            } else {
                s.obj = Object::Free(self.free);
                self.free = idx;
                free += 1;
            }
        }
        let used = top.map_or(0, |i| i as usize + 1);
        free -= self.give_back(used).min(free);
        *self.until_gc.get_mut() = live.max(walked).max(free / 2).max(GC_MIN);
        #[cfg(test)]
        {
            self.live = live.saturating_add(walked);
        }
    }

    /// Gives back the free slots above `used`, when they are at least a third of all, and says
    /// how many it gave: what survived was made last, so about as high as half the free slots.
    /// The free list runs from the bottom up, so they are its end. A slot made in their place
    /// starts past their generations, unless those ran so far that it couldn't.
    fn give_back(&mut self, used: usize) -> usize {
        let top = self.slots.get(used..).unwrap_or_default();
        let past = top.iter().map(|s| s.generation).max().unwrap_or(0);
        if top.len() < GC_MIN || top.len() < self.slots.len() / 3 || past >= u32::MAX - 1 {
            return 0;
        }
        let given = top.len();
        self.fresh = self.fresh.max(past);
        self.slots.truncate(used);
        // The last free slot below now ends the list.
        if let Some(Slot {
            obj: Object::Free(next),
            ..
        }) = self.slots.iter_mut().rev().find(|s| match s.obj {
            Object::Free(next) => next != NONE && next as usize >= used,
            _ => false,
        }) {
            *next = NONE;
        }
        shrink(&mut self.slots);
        // The mark stack is empty between collections, and needs room for every slot.
        let mut marks = Vec::new();
        if marks.try_reserve_exact(used + 1).is_ok() {
            self.marks = marks;
        }
        given
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

/// Moves `v` into storage of its own length, if memory allows; else leaves it as it is.
pub(crate) fn shrink<X>(v: &mut Vec<X>) {
    let mut smaller = Vec::new();
    if v.capacity() > v.len() && smaller.try_reserve_exact(v.len()).is_ok() {
        smaller.append(v);
        *v = smaller;
    }
}

/// Puts `v`'s object on the mark stack, unless a collection already reached it.
fn mark(slots: &[Slot], marks: &mut Vec<u32>, v: Value) {
    let r = match v {
        Value::Pair(r)
        | Value::String(r)
        | Value::Bytes(r)
        | Value::Context(r)
        | Value::Operative(r)
        | Value::Applicative(r) => r,
        _ => return,
    };
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
