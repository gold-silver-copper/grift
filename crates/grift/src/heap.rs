//! The heap: a growable vector of slots, reused through a free list, reclaimed by mark and sweep
//! and given back from the top. A new slot first reserves its entry on the mark stack, so a
//! collection needs no memory of its own.

use crate::evaluate::Native;
use crate::local::{Error, K, Symbol, Value, push};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::marker::PhantomData;

/// A reference to a heap slot: the slot, the generation that tells a live object from a reclaimed
/// one, and the kind of object it names, one of [`kind`]'s, or `()` for one a host holds, whose
/// kind [`Heap::kind`] checks as it comes in. Only the heap makes one of a kind, from an object of
/// it, so a value's tag and its object agree, and a reader's argument says which kind it reads.
#[derive(Clone, Copy, Debug, Eq)]
#[cfg_attr(not(test), derive(PartialEq))]
pub(crate) struct Ref<T = ()> {
    idx: u32,
    generation: u32,
    kind: PhantomData<T>,
}

#[cfg(test)]
impl<T> PartialEq for Ref<T> {
    fn eq(&self, other: &Ref<T>) -> bool {
        crate::local::COMPARED.with(|n| n.set(n.get() + 1));
        self.idx == other.idx && self.generation == other.generation
    }
}

/// A kind of object: what its variant of [`Object`] holds.
pub(crate) trait Kind: Copy {
    type Holds;
    fn object(holds: Self::Holds) -> Object;
    fn of(obj: &Object) -> Option<&Self::Holds>;
    fn of_mut(obj: &mut Object) -> Option<&mut Self::Holds>;
}

/// A kind read whole: every kind but a pair, which only [`Heap::pair_at`] reads.
pub(crate) trait Whole: Kind {}

macro_rules! kinds {
    ($($kind:ident($holds:ty) $($whole:ident)?;)*) => {
        /// One type per kind of object, named for its variant of [`Object`] and of `Value`.
        pub(crate) mod kind {
            use super::*;
            $(
                #[derive(Clone, Copy, Debug, PartialEq, Eq)]
                pub(crate) enum $kind {}

                impl Kind for $kind {
                    type Holds = $holds;
                    fn object(holds: $holds) -> Object {
                        Object::$kind(holds)
                    }
                    fn of(obj: &Object) -> Option<&$holds> {
                        match obj {
                            Object::$kind(holds) => Some(holds),
                            _ => None,
                        }
                    }
                    fn of_mut(obj: &mut Object) -> Option<&mut $holds> {
                        match obj {
                            Object::$kind(holds) => Some(holds),
                            _ => None,
                        }
                    }
                }
                $(impl $whole for $kind {})?
            )*
        }
    };
}

kinds! {
    Pair((Value, Value, Origin));
    String(Box<str>) Whole;
    Bytes(Vec<u8>) Whole;
    Context((Option<Ref<Context>>, Bindings)) Whole;
    // Inside `kind`, where `Operative` is the kind.
    Operative(super::Operative) Whole;
    Applicative(Value) Whole;
}

/// A pair the reader is still making: until it lets go, its rest and its origin may change. Only
/// [`Heap::open`] makes one, and nothing turns a pair's [`Ref`] into one, so no pair changes once
/// anything else has seen it.
#[derive(Clone, Copy)]
pub(crate) struct Open(Ref<kind::Pair>);

impl Open {
    pub(crate) fn pair(self) -> Ref<kind::Pair> {
        self.0
    }
}

/// A compound operative: its parameter tree, its context parameter, its body's forms, the context it
/// was made in, how many bindings a call makes, and the native twin that runs it, for the few
/// library forms that have one.
#[derive(Clone, Copy)]
pub(crate) struct Operative {
    pub parameters: Value,
    pub context_parameter: Option<Symbol>,
    pub body: Value,
    pub context: Ref<kind::Context>,
    pub size: u32,
    pub native: Option<Native>,
}

pub(crate) enum Object {
    /// A free slot and the next free one.
    Free(u32),
    Pair((Value, Value, Origin)),
    String(Box<str>),
    /// A symbol's name. Symbols are compared by slot, so a name lives here once while anything
    /// refers to it, and is collected like any value when nothing does.
    Symbol(Box<str>),
    /// An optional parent and the bindings.
    Context((Option<Ref<kind::Context>>, Bindings)),
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
const _: () = assert!(size_of::<Slot>() == 64);

/// Where a pair came from: the source and line it was read at, if under a name; whether the
/// library made it, by reading its text or by running its code; and whether it is one of its
/// source's outer forms.
#[derive(Clone, Copy)]
pub(crate) struct Origin {
    pub place: Option<(Symbol, u32)>,
    pub library: bool,
    pub outer: bool,
}

impl Origin {
    /// A pair made where no source says, the library's if `library`.
    pub(crate) fn new(library: bool) -> Origin {
        Origin {
            place: None,
            library,
            outer: false,
        }
    }
}

const NONE: u32 = u32::MAX;

/// A collection pays for itself: the next one starts once as much was allocated as the largest part
/// of what this one walked, at least this much. It walks what survived, the roots outside the heap
/// (the running machine, the kept table and its parked runs) and every slot, so the next waits for
/// the largest of those three, the slots counted as half the free ones, so it walks at most four
/// times what was allocated. The heap stays within about twice the larger of the live data and
/// those roots. Free slots at the top, past the room the next collection waits for, are given back
/// when they are a third of all. New objects take the lowest free slots, so the newest live ones,
/// such as a running loop's, sit as high as what was allocated since the last collection, and keep
/// the slots below them, as a run's source read then does. So after a dropped peak, when the free
/// slots are what the next collection would wait for, the next run to begin collects first, once:
/// what the runs before made is garbage by then, so it gives back the rest, and the two walk at
/// most eight times what was allocated. Objects never move, so one long-lived object made high
/// keeps the slots below it, never more than the peak. A retired slot counts as live. A string,
/// bytes value or name counts as many objects as its storage would fill slots, and a context as
/// many as its bindings' storage would fill, at least one, counted as they grow, so large chunks
/// dropped one after another bring the next collection nearer as their size, not their number,
/// says.
const GC_MIN: usize = 4096;

/// How many objects `obj` counts as, for when to collect.
#[inline]
fn weight(obj: &Object) -> usize {
    match obj {
        Object::String(s) | Object::Symbol(s) => 1 + s.len().div_ceil(size_of::<Slot>()),
        Object::Bytes(b) => 1 + b.capacity().div_ceil(size_of::<Slot>()),
        Object::Context((_, b)) => 1 + bindings_weight(b),
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
    /// Whether the next run to begin collects first, once: the last paced collection left most
    /// slots free.
    early: bool,
    /// Whether the next collection was asked for, not paced: it brings no early one.
    asked: Cell<bool>,
    #[cfg(test)]
    pub probe: probe::Probe,
}

impl Default for Heap {
    fn default() -> Heap {
        Heap {
            slots: Vec::new(),
            free: NONE,
            until_gc: Cell::new(GC_MIN),
            marks: Vec::new(),
            fresh: 0,
            early: false,
            asked: Cell::new(false),
            #[cfg(test)]
            probe: probe::Probe::default(),
        }
    }
}

impl<T> Ref<T> {
    /// No object: a reference no slot has.
    pub(crate) const NONE: Ref<T> = Ref {
        idx: NONE,
        generation: 0,
        kind: PhantomData,
    };

    /// Its slot's number, which tells one live object from another.
    pub(crate) fn slot(self) -> u32 {
        self.idx
    }

    /// The same reference, as of kind `U`: only the heap says which kind a slot holds.
    fn cast<U>(self) -> Ref<U> {
        Ref {
            idx: self.idx,
            generation: self.generation,
            kind: PhantomData,
        }
    }

    /// The same reference, of no kind.
    pub(crate) fn untyped(self) -> Ref {
        self.cast()
    }
}

/// A slot that holds no object of the kind asked for: reclaimed, or never one. One answer, the
/// host's `stale-value`, with no values: the value would name what isn't there.
fn stale() -> Error {
    Error::new(K::StaleValue, &[])
}

impl Heap {
    /// A new slot for `obj`, which is of kind `T`: the makers below say so, each for its kind.
    #[inline]
    fn alloc<T>(&mut self, obj: Object) -> Result<Ref<T>, Error> {
        debug_assert!(weight(&obj) > 0, "an object weighs at least its own slot");
        let until_gc = self.until_gc.get_mut();
        *until_gc = until_gc.saturating_sub(weight(&obj));
        #[cfg(test)]
        {
            self.probe.work.0 += weight(&obj);
            match self.probe.refuse_in {
                Some(0) => {
                    self.probe.refused += 1;
                    if self.probe.refuse_once {
                        self.probe.refuse_in = None;
                    }
                    return Err(self.refused());
                }
                Some(n) => self.probe.refuse_in = Some(n - 1),
                None => {}
            }
        }
        let idx = self.free;
        if let Some(slot) = self.slots.get_mut(idx as usize)
            && let Object::Free(next) = slot.obj
        {
            self.free = next;
            slot.obj = obj;
            return Ok(Ref {
                idx,
                generation: slot.generation,
                kind: PhantomData,
            });
        }
        #[cfg(test)]
        if (self.probe.slot_limit).is_some_and(|limit| self.slots.len() >= limit) {
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
        Ok(Ref {
            idx,
            generation,
            kind: PhantomData,
        })
    }

    /// A new object of kind `T`.
    #[inline]
    pub fn make<T: Kind>(&mut self, holds: T::Holds) -> Result<Ref<T>, Error> {
        self.alloc(T::object(holds))
    }

    /// A new pair for the reader to finish.
    pub fn open(&mut self, a: Value, origin: Origin) -> Result<Open, Error> {
        self.make((a, Value::EmptyList, origin)).map(Open)
    }

    /// A new symbol spelled `spelling`.
    pub fn name(&mut self, spelling: Box<str>) -> Result<Symbol, Error> {
        self.alloc::<()>(Object::Symbol(spelling))
            .map(|r| Symbol(r.idx))
    }

    /// Context `r`'s bindings, with room for `more` new ones. Storage they grow by brings the
    /// next collection nearer, as a new object's weight does.
    pub fn room_in(&mut self, r: Ref<kind::Context>, more: usize) -> Result<&mut Bindings, Error> {
        let bindings = match self.slots.get_mut(r.idx as usize) {
            Some(Slot {
                generation,
                obj: Object::Context((_, bindings)),
                ..
            }) if *generation == r.generation => bindings,
            _ => return Err(stale()),
        };
        let before = bindings_weight(bindings);
        bindings.try_reserve(more).map_err(|_| Error::oom())?;
        let grown = bindings_weight(bindings).saturating_sub(before);
        let until_gc = self.until_gc.get_mut();
        *until_gc = until_gc.saturating_sub(grown);
        #[cfg(test)]
        {
            self.probe.work.0 += grown;
        }
        Ok(bindings)
    }

    /// Brings the next collection `n` objects' worth nearer: storage outside a new object that a
    /// collection walks.
    pub fn charge(&mut self, n: usize) {
        let until_gc = self.until_gc.get_mut();
        *until_gc = until_gc.saturating_sub(n);
        #[cfg(test)]
        {
            self.probe.work.0 += n;
        }
    }

    /// No room for an object: collection comes next, at the machine's next safe point or as a
    /// host's next run begins.
    #[cold]
    fn refused(&self) -> Error {
        self.request_collection();
        Error::oom()
    }

    /// What the object `r` names holds, if its slot still holds one of its kind.
    #[inline]
    fn get<T: Kind>(&self, r: Ref<T>) -> Result<&T::Holds, Error> {
        match self.slots.get(r.idx as usize) {
            Some(s) if s.generation == r.generation => T::of(&s.obj).ok_or_else(stale),
            _ => Err(stale()),
        }
    }

    fn get_mut<T: Kind>(&mut self, r: Ref<T>) -> Result<&mut T::Holds, Error> {
        match self.slots.get_mut(r.idx as usize) {
            Some(s) if s.generation == r.generation => T::of_mut(&mut s.obj).ok_or_else(stale),
            _ => Err(stale()),
        }
    }

    /// A host's reference as one to an object of kind `T`, if it still names one.
    pub fn kind<T: Kind>(&self, r: Ref) -> Result<Ref<T>, Error> {
        self.get(r.cast::<T>()).map(|_| r.cast())
    }

    /// What an object of any kind but a pair holds.
    #[inline]
    pub fn read<T: Whole>(&self, r: Ref<T>) -> Result<&T::Holds, Error> {
        self.get(r)
    }

    /// A pair's first, its rest and its origin. A walk along a list goes through `Items`, which a
    /// stop ends (`clippy.toml`).
    #[inline]
    pub fn pair_at(&self, r: Ref<kind::Pair>) -> Result<(Value, Value, Origin), Error> {
        self.get(r).copied()
    }

    /// Sets the rest of a pair the reader is making.
    pub fn set_rest(&mut self, p: Open, v: Value) -> Result<(), Error> {
        self.get_mut(p.0)?.1 = v;
        Ok(())
    }

    /// Says where the reader read a list whose first pair it is making, and whether it is an outer
    /// form.
    pub fn place(&mut self, p: Open, place: (Symbol, u32), outer: bool) -> Result<(), Error> {
        let (_, _, origin) = self.get_mut(p.0)?;
        (origin.place, origin.outer) = (Some(place), outer);
        Ok(())
    }

    /// Flags a library operative as having a native twin, or as having none.
    pub fn install(
        &mut self,
        r: Ref<kind::Operative>,
        native: Option<Native>,
    ) -> Result<(), Error> {
        self.get_mut(r)?.native = native;
        Ok(())
    }

    /// Takes a string's text out, leaving it empty until [`Heap::put_text`] puts it back: lent so,
    /// it needs no copy.
    pub fn take_text(&mut self, r: Ref<kind::String>) -> Result<Box<str>, Error> {
        self.get_mut(r).map(std::mem::take)
    }

    pub fn put_text(&mut self, r: Ref<kind::String>, text: Box<str>) -> Result<(), Error> {
        *self.get_mut(r)? = text;
        Ok(())
    }

    /// The name in symbol slot `id` and its generation, if the slot holds a name.
    #[inline]
    pub fn symbol(&self, id: u32) -> Option<(&str, u32)> {
        match self.slots.get(id as usize) {
            Some(Slot {
                obj: Object::Symbol(name),
                generation,
                ..
            }) => Some((name, *generation)),
            _ => None,
        }
    }

    /// Whether collection is due at a machine safe point or before a host's run needs slots.
    #[inline]
    pub fn due(&self) -> bool {
        #[cfg(test)]
        if self.probe.stress {
            return true;
        }
        self.until_gc.get() == 0
    }

    /// Whether a run about to begin collects first: when due, or once after a peak.
    #[inline]
    pub fn due_before_run(&self) -> bool {
        self.early || self.due()
    }

    /// The next safe point collects.
    pub fn request_collection(&self) {
        self.asked.set(true);
        self.until_gc.set(0);
    }

    /// Marks everything reachable from `roots` and the host's `held`, and frees the rest, with no
    /// memory of its own: a slot goes on the mark stack only as it is first marked, so once at
    /// most, and every slot reserved its entry as it was made. `walked` is how much outside the
    /// heap the collection walks too. The free list is made again from the bottom up, so the
    /// lowest slots are reused first and the top empties, to be given back.
    pub fn collect(
        &mut self,
        roots: impl Iterator<Item = Value>,
        held: impl Iterator<Item = Ref>,
        walked: usize,
    ) {
        let asked = self.asked.take();
        let early = self.early && !asked && self.until_gc.get() > 0;
        #[cfg(test)]
        {
            self.probe.collections += 1;
            // Marking walks each context's bindings too.
            let bindings: usize = (self.slots.iter())
                .map(|s| match &s.obj {
                    Object::Context((_, b)) => bindings_weight(b),
                    _ => 0,
                })
                .sum();
            let walks = self.slots.len() + walked + bindings;
            self.probe.work.1 += walks;
            // A collection that came when pacing said walks a constant factor of what was
            // allocated since the last one; an early one, with the one before it, of what was
            // allocated since the one before that.
            let ([before, last], walked_last) = &self.probe.paced;
            let (since, before_too) = if early {
                (*before, walked_last)
            } else {
                (*last, &0)
            };
            let allocated = self.probe.work.0 - since;
            let paced = early || self.until_gc.get() == 0;
            // Every collection is asked for, forced, or paced: one that came sooner is unpaid.
            assert!(
                asked || self.probe.stress || paced,
                "a collection came before it was due"
            );
            if !self.probe.stress && !asked && paced {
                assert!(
                    walks + before_too <= 8 * allocated,
                    "a paced collection walked {walks} (and {before_too} before) for {allocated} allocated"
                );
            }
            self.probe.paced = ([*last, self.probe.work.0], walks);
        }
        let slots = &self.slots;
        let marks = &mut self.marks;
        #[cfg(test)]
        let room = marks.capacity();
        roots.for_each(|v| mark(slots, marks, v));
        held.for_each(|r| hold(slots, marks, r));
        while let Some(idx) = marks.pop() {
            if let Some(s) = slots.get(idx as usize) {
                children(&s.obj, |v| mark(slots, marks, v));
            }
        }
        #[cfg(test)]
        assert_eq!(marks.capacity(), room, "marking grew the mark stack");
        let (mut live, mut free, mut top) = (0usize, 0usize, None);
        // Each free slot is linked to from the one below it, so the list runs from the bottom up.
        let mut tail = &mut self.free;
        for (idx, s) in (0..NONE).zip(self.slots.iter_mut()) {
            if s.marked.take() {
                live = live.saturating_add(weight(&s.obj));
                top = Some(idx);
                continue;
            }
            if !matches!(s.obj, Object::Free(_)) {
                // A slot whose generations run out is retired, never reused: its last one, which
                // no reference has, means free for good.
                s.generation = s.generation.saturating_add(1);
                s.obj = Object::Free(NONE);
            }
            if s.generation == u32::MAX {
                // Swept every time and never reused, it counts as live.
                top = Some(idx);
                live = live.saturating_add(1);
            } else if let Object::Free(next) = &mut s.obj {
                *tail = idx;
                tail = next;
                free += 1;
            }
        }
        *tail = NONE;
        let used = top.map_or(0, |i| i as usize + 1);
        let paced = live.max(walked).max(GC_MIN);
        // What is allocated before the next collection fills the free slots below `used` first.
        let below = free.saturating_sub(self.slots.len() - used);
        free -= self.give_back(used, paced.saturating_sub(below)).min(free);
        self.early = free / 2 > paced && !early && !asked;
        *self.until_gc.get_mut() = paced.max(free / 2);
        #[cfg(test)]
        {
            self.probe.live = live.saturating_add(walked);
            // A collection stress mode forced is not paced, as one asked for isn't.
            self.early &= !self.probe.stress;
        }
    }

    /// When the free slots above `used` are at least a third of all, gives back those past the
    /// first `room` of them, and says how many it gave: `room` is what the next collection waits
    /// for, which given back the heap would only grow into again. The free list runs from the
    /// bottom up, so they are its end. A slot made in their place starts past their generations,
    /// unless those ran so far that it couldn't.
    fn give_back(&mut self, used: usize, room: usize) -> usize {
        let above = self.slots.len().saturating_sub(used);
        if above < GC_MIN || above < self.slots.len() / 3 || above <= room {
            return 0;
        }
        let from = used.saturating_add(room);
        let top = self.slots.get(from..).unwrap_or_default();
        // New slots all start past this, so one slot reused billions of times would make every
        // new one retire soon: past half their generations, nothing is given back.
        let past = top.iter().map(|s| s.generation).max().unwrap_or(0);
        if past >= u32::MAX / 2 {
            return 0;
        }
        let given = top.len();
        self.fresh = self.fresh.max(past);
        self.slots.truncate(from);
        // The last free slot below now ends the list, or the list is empty.
        if self.free as usize >= from {
            self.free = NONE;
        }
        if let Some(Slot {
            obj: Object::Free(next),
            ..
        }) = self.slots.iter_mut().rev().find(|s| match s.obj {
            Object::Free(next) => next != NONE && next as usize >= from,
            _ => false,
        }) {
            *next = NONE;
        }
        shrink(&mut self.slots, 0);
        // The mark stack is empty between collections, and needs room for every slot.
        let mut marks = Vec::new();
        if marks.try_reserve_exact(from + 1).is_ok() {
            self.marks = marks;
        }
        given
    }
}

/// How many objects a context's bindings count as besides the context: the slots their storage
/// would fill past the first, which the context's own count stands for. So a call's few bindings
/// cost nothing more, and a context with many counts as the slots they fill.
fn bindings_weight(b: &Bindings) -> usize {
    (b.capacity() * size_of::<(Symbol, Value)>())
        .div_ceil(size_of::<Slot>())
        .saturating_sub(1)
}

/// Gives back `v`'s room once it holds under a quarter of it and that is more than [`GC_MIN`]
/// entries: a stack or table that was deep once costs nothing after. It keeps twice its length,
/// and at least [`GC_MIN`], so room set aside for a later step (a cleanup's start) stays.
pub(crate) fn tidy<X>(v: &mut Vec<X>) {
    if v.capacity() / 4 > v.len().max(GC_MIN) {
        shrink(v, v.len().saturating_mul(2).max(GC_MIN));
    }
}

/// Moves `v` into storage for `room` entries, at least its length, if that is less than it has and
/// memory allows; else leaves it as it is.
pub(crate) fn shrink<X>(v: &mut Vec<X>, room: usize) {
    let room = room.max(v.len());
    let mut smaller = Vec::new();
    if v.capacity() > room && smaller.try_reserve_exact(room).is_ok() {
        #[expect(clippy::disallowed_methods, reason = "reserved at least v.len() above")]
        smaller.append(v);
        *v = smaller;
    }
}

/// Puts `v`'s object on the mark stack, unless a collection already reached it.
fn mark(slots: &[Slot], marks: &mut Vec<u32>, v: Value) {
    let r = match v {
        // A name refers to nothing, so it is marked where it is.
        Value::Symbol(id) => {
            if let Some(s) = slots.get(id.0 as usize)
                && matches!(s.obj, Object::Symbol(_))
            {
                s.marked.set(true);
            }
            return;
        }
        Value::Pair(r) => r.untyped(),
        Value::String(r) => r.untyped(),
        Value::Bytes(r) => r.untyped(),
        Value::Context(r) => r.untyped(),
        Value::Operative(r) => r.untyped(),
        Value::Applicative(r) => r.untyped(),
        // Listed, not a catch-all: a new kind of value is classed here or doesn't compile.
        Value::EmptyList
        | Value::Boolean(_)
        | Value::Inert
        | Value::Ignore
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Primitive(_)
        | Value::Host(_) => return,
    };
    hold(slots, marks, r);
}

/// Puts the object `r` names on the mark stack, unless a collection already reached it: a value's,
/// or one a host holds, whose kind marking needn't know.
fn hold(slots: &[Slot], marks: &mut Vec<u32>, r: Ref) {
    if let Some(s) = slots.get(r.idx as usize)
        && s.generation == r.generation
        && !s.marked.replace(true)
    {
        #[expect(
            clippy::disallowed_methods,
            reason = "alloc keeps one mark-stack entry per slot"
        )]
        marks.push(r.idx);
    }
}

/// What `obj` refers to.
fn children(obj: &Object, mut f: impl FnMut(Value)) {
    match obj {
        Object::Free(_) | Object::String(_) | Object::Symbol(_) | Object::Bytes(_) => {}
        Object::Pair((a, d, origin)) => {
            if let Some((source, _)) = origin.place {
                f(Value::Symbol(source));
            }
            f(*a);
            f(*d);
        }
        Object::Context((parent, bindings)) => {
            if let Some(p) = parent {
                f(Value::Context(*p));
            }
            for (&name, &value) in bindings {
                f(Value::Symbol(name));
                f(value);
            }
        }
        Object::Operative(op) => {
            f(op.parameters);
            if let Some(s) = op.context_parameter {
                f(Value::Symbol(s));
            }
            f(op.body);
            f(Value::Context(op.context));
        }
        Object::Applicative(c) => f(*c),
    }
}

/// What tests ask of the heap and how they steer it, kept out of the heap's own fields.
#[cfg(test)]
mod probe {
    use super::*;

    #[derive(Default)]
    pub(crate) struct Probe {
        /// What the last collection found alive, by weight, for tests of what a run keeps.
        pub live: usize,
        /// Refuse new slots past this length, to test recovery without replacing the allocator.
        pub slot_limit: Option<usize>,
        /// Collect at every safe point, to test what a collection could lose.
        pub stress: bool,
        /// Refuse a slot after this many more, to choose where memory runs out; and how many were
        /// refused. `refuse_once` lets the next one through.
        pub refuse_in: Option<usize>,
        pub refuse_once: bool,
        pub refused: usize,
        /// Collections so far.
        pub collections: usize,
        /// Weight allocated, and slots and roots outside the heap that collections walked, so far.
        pub work: (usize, usize),
        /// `work.0` at the last two collections, and what the last walked.
        pub paced: ([usize; 2], usize),
    }

    impl Heap {
        /// What the last collection found alive, by weight.
        pub fn live(&self) -> usize {
            self.probe.live
        }

        /// Puts the next collection off, as if the last had just been.
        pub fn not_due(&self) {
            self.until_gc.set(self.probe.live.max(GC_MIN));
        }

        pub fn len(&self) -> usize {
            self.slots.len()
        }
    }
}
