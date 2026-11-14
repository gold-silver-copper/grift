//! The heap: slots in chunks of a fixed size, reused through a free list and reclaimed by mark and
//! sweep. It grows a chunk at a time and gives a free chunk back by dropping it, so it never
//! copies itself. A slot holds its own link of the mark stack, so a collection needs no memory.

use crate::evaluate::Native;
use crate::local::{Error, K_NAMES, Symbol, Value, boxed};
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

/// Sealed: only this file says which types are kinds, and which kinds are read whole.
mod sealed {
    pub trait Kind {}
    pub trait Whole {}
}

/// A kind of object: what its variant of [`Object`] holds.
pub(crate) trait Kind: Copy + sealed::Kind {
    type Holds;
    fn object(holds: Self::Holds) -> Object;
    fn of(obj: &Object) -> Option<&Self::Holds>;
    fn of_mut(obj: &mut Object) -> Option<&mut Self::Holds>;
}

/// A kind read whole: every kind but a pair, which only [`Heap::pair_at`] reads.
pub(crate) trait Whole: Kind + sealed::Whole {}

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
                impl sealed::Kind for $kind {}
                $(
                    impl sealed::$whole for $kind {}
                    impl $whole for $kind {}
                )?
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
    /// refers to it, and is collected like any value when nothing does. [`Heap::intern`] makes
    /// one and the sweep frees one, each with its entry in the heap's index of names.
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
    /// [`UNMARKED`] until a collection first reaches the slot, then the slot under it on the mark
    /// stack, or [`END`]: so it goes on once at most, and the stack is the slots' own.
    gray: Cell<u32>,
    obj: Object,
}

/// How many slots a chunk holds: 64 KiB.
const CHUNK: usize = 1024;

/// Slots that live and die together. A slot's number is its chunk's place in the heap's list
/// times [`CHUNK`], plus its place in the chunk.
type Chunk = Box<[Slot; CHUNK]>;

/// A slot no collection has reached, and the bottom of the mark stack: neither is a slot's number.
const UNMARKED: u32 = NONE;
const END: u32 = NONE - 1;

/// Room held back for when memory is refused, given up at the refusal: for the trace the
/// unwinding builds, the program's combinations it passes, and after it for a report of the
/// error, whose printer needs about 2 KiB whatever the error holds, beside the host's own text.
/// It is no promise past that: a deep trace, or a host's text longer than what is left, can
/// still find memory refused.
const SPARE: usize = 64 * 1024;

/// Room held back for the machine's own growth, a chunk's size: what tells the heap it is at the
/// allocator's limit before anything is refused. When a chunk, a call's bindings or a machine
/// stack is refused memory, this is given up and the same request made once more, so the step goes
/// on, and the next safe point collects. Nothing is run again: only the request is repeated.
const SLACK: usize = CHUNK * size_of::<Slot>();

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

const NONE: u32 = u32::MAX;

/// A collection pays for itself: the next one starts once as much was allocated as the largest part
/// of what this one walked, at least this much. It walks what survived, each root outside the heap
/// (the running machine's values, the kept table's entries and its parked runs', the host
/// functions' and known names, and the references a host's call holds) and every slot, so the next
/// waits for the largest of those three, the slots counted as half the free ones, so it walks at
/// most four
/// times what was allocated. The heap stays within about twice the larger of the live data and
/// those roots. A chunk with every slot free is given back once the free slots below it are the
/// room the next collection waits for. New objects take the lowest free slots, so the newest live
/// ones, such as a running loop's, sit as high as what was allocated since the last collection,
/// and each keeps its chunk. So after a dropped peak, when the free slots are what the next
/// collection would wait for, the next run to begin collects first, once: what the runs before
/// made is garbage by then, so it gives back the rest, and the two walk at most eight times what
/// was allocated. Objects never move, so one long-lived object keeps its chunk, never more than
/// the peak. A retired slot counts as live, and keeps its chunk too. A string,
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

/// The bytes `obj` holds outside its slot, as the memory limit counts them: a string's or bytes
/// value's storage, a name's spelling twice and its entry in the index of names, and a context's
/// bindings at their capacity. Not the allocator's own bookkeeping, nor a table's control bytes.
#[inline]
fn payload(obj: &Object) -> usize {
    match obj {
        Object::String(s) => s.len(),
        Object::Symbol(s) => 2 * s.len() + size_of::<(Box<str>, u32)>(),
        Object::Bytes(b) => b.capacity(),
        Object::Context((_, b)) => b.capacity() * size_of::<(Symbol, Value)>(),
        _ => 0,
    }
}

pub(crate) struct Heap {
    /// The chunks, each at the place its slots' numbers say, and `None` where one was given back:
    /// the list is never closed up, so a number means one slot for as long as its chunk is there.
    chunks: Vec<Option<Chunk>>,
    /// How many places in the list have no chunk.
    holes: usize,
    free: u32,
    /// Every live name's spelling to its slot: an entry goes in as [`Heap::intern`] makes the
    /// name's slot and out as the sweep frees it, so no other code can make the two disagree.
    names: HashMap<Box<str>, u32>,
    /// How much may be allocated before the next collection: collection sets it, allocation
    /// spends it, and a refusal empties it.
    until_gc: Cell<usize>,
    /// The generation a new slot starts at: past every generation of a slot given back, so no
    /// reference to one that was there matches the slot made in its place.
    fresh: u32,
    /// How many slots ran out of generations: each counts as live.
    retired: usize,
    /// The [`SPARE`] room, or none after a refusal until the next run begins.
    spare: Cell<Vec<u8>>,
    /// The [`SLACK`] room, or none from when the machine's growth needed it until a collection
    /// can take it again.
    slack: Cell<Vec<u8>>,
    /// Whether the next run to begin collects first, once: the last paced collection left most
    /// slots free.
    early: bool,
    /// Whether the next collection was asked for, not paced: it brings no early one.
    asked: Cell<bool>,
    /// Whether memory was refused since a run last asked: a run that ends so collects as it ends,
    /// whatever collections its cleanups had on the way.
    refused: Cell<bool>,
    /// The most bytes [`Heap::held`] may reach, or `usize::MAX` for no limit: the host's to set.
    limit: usize,
    /// The bytes the interpreter holds, as the limit counts them: what the last collection found
    /// (the chunks, the live objects' [`payload`], the spare and slack room, and the machine
    /// stacks and tables outside the heap) and what was taken since. Garbage is held until a
    /// collection frees it, so it counts until then.
    held: Cell<usize>,
    #[cfg(test)]
    pub probe: probe::Probe,
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

impl Heap {
    /// A heap holding grift's known names, made first so each one's slot is its number, as
    /// `Symbol::from(K)` says, and live for good: [`Heap::collect`] marks them itself. With no
    /// memory for them all, the heap is only good for reporting that, and the refusal says so.
    pub fn new() -> (Heap, Result<(), Error>) {
        let mut heap = Heap {
            chunks: Vec::new(),
            holes: 0,
            free: NONE,
            names: HashMap::new(),
            until_gc: Cell::new(GC_MIN),
            fresh: 0,
            retired: 0,
            spare: Cell::new(Vec::new()),
            slack: Cell::new(Vec::new()),
            early: false,
            asked: Cell::new(false),
            refused: Cell::new(false),
            limit: usize::MAX,
            held: Cell::new(0),
            #[cfg(test)]
            probe: probe::Probe::default(),
        };
        heap.hold_spare();
        heap.hold_slack();
        let made = K_NAMES
            .iter()
            .try_for_each(|name| heap.intern(name).map(|_| ()));
        (heap, made)
    }

    /// Takes the spare room if it isn't held: as the heap is made, and as a run begins, after the
    /// collection a refusal asked for. Refused, the heap goes without: an `out-of-memory` trace
    /// can then miss places.
    pub fn hold_spare(&mut self) {
        held_back(self.spare.get_mut(), SPARE, self.limit, &self.held);
    }

    /// Takes the slack room if it isn't held: whether it is held now.
    fn hold_slack(&mut self) -> bool {
        held_back(self.slack.get_mut(), SLACK, self.limit, &self.held)
    }

    /// The memory limit, if the host set one.
    pub fn limit(&self) -> Option<usize> {
        Some(self.limit).filter(|&limit| limit != usize::MAX)
    }

    /// Sets the memory limit, or none. The next run collects before it begins, so the heap paces
    /// itself by the new limit from there; what is held past a lower limit stays held, and
    /// nothing more is given until a collection has brought the count under it.
    pub fn set_limit(&mut self, limit: Option<usize>) {
        self.limit = limit.unwrap_or(usize::MAX);
        self.request_collection();
    }

    /// The bytes the limit counts.
    pub fn held(&self) -> usize {
        self.held.get()
    }

    /// Whether the limit allows `bytes` more. If only the slack room is in the way it is given
    /// up, as when the allocator refuses, and the next safe point collects.
    #[inline]
    fn allows(&self, bytes: usize) -> bool {
        let over = || self.held.get().saturating_add(bytes) > self.limit;
        !over() || (give_slack(&self.slack, &self.asked, &self.until_gc, &self.held) && !over())
    }

    /// Room under the limit for `bytes` about to be asked of the allocator, for a value whose
    /// size a program chooses: asked first, the allocator is never given a request the limit
    /// would refuse. The bytes are counted when the value becomes an object.
    pub fn fits(&self, bytes: usize) -> Result<(), Error> {
        if self.allows(bytes) {
            Ok(())
        } else {
            Err(self.refused())
        }
    }

    /// Room for `more` on `stack`, one of a machine's: under the limit, and the slack room stands
    /// behind it as it does behind a chunk. What it grows by is counted.
    #[inline]
    pub fn room<X>(&self, stack: &mut Vec<X>, more: usize) -> Result<(), Error> {
        if stack.capacity() - stack.len() >= more {
            return Ok(());
        }
        self.grown(stack, more)
    }

    #[cold]
    fn grown<X>(&self, stack: &mut Vec<X>, more: usize) -> Result<(), Error> {
        let before = stack.capacity();
        // A vector grows to twice its size at least.
        self.fits(before.max(more).saturating_mul(size_of::<X>()))?;
        self.reserving(|| stack.try_reserve(more))?;
        let grown = (stack.capacity() - before).saturating_mul(size_of::<X>());
        self.held.set(self.held.get().saturating_add(grown));
        Ok(())
    }

    /// Makes `attempt`, a request for memory the machine's growth needs, and if it is refused
    /// gives up the slack room and makes it once more.
    #[inline]
    pub fn reserving<E>(&self, mut attempt: impl FnMut() -> Result<(), E>) -> Result<(), E> {
        match attempt() {
            Ok(()) => Ok(()),
            Err(refused) if give_slack(&self.slack, &self.asked, &self.until_gc, &self.held) => {
                drop(refused);
                attempt()
            }
            Err(refused) => Err(refused),
        }
    }

    /// Makes a chunk of free slots, at the first place in the list without one, each linked to
    /// the next: its first slot, or `None` if memory was refused. Its storage is made the size it
    /// stays, so nothing is ever copied to grow, and its slots start past the generations of any
    /// that were there before.
    fn add_chunk(&mut self) -> Option<u32> {
        #[cfg(test)]
        if (self.probe.slot_limit).is_some_and(|limit| self.len() + CHUNK > limit) {
            return None;
        }
        if self.held.get().saturating_add(SLACK) > self.limit {
            return None;
        }
        let holes = if self.holes > 0 { &*self.chunks } else { &[] };
        let at = (holes.iter().position(Option::is_none)).unwrap_or(self.chunks.len());
        // Every slot's number is below `END`.
        let first = (at.checked_mul(CHUNK).and_then(|n| u32::try_from(n).ok()))
            .filter(|n| END - n >= CHUNK as u32)?;
        let mut slots = Vec::new();
        slots.try_reserve_exact(CHUNK).ok()?;
        if at == self.chunks.len() {
            self.chunks.try_reserve(1).ok()?;
        }
        let (generation, last) = (self.fresh, first + (CHUNK as u32 - 1));
        #[expect(clippy::disallowed_methods, reason = "reserved CHUNK above")]
        slots.extend((first..=last).map(|idx| Slot {
            generation,
            gray: Cell::new(UNMARKED),
            obj: Object::Free(if idx == last { NONE } else { idx + 1 }),
        }));
        // As long as it was reserved, so the box is the vector's own storage.
        let chunk = Chunk::try_from(slots.into_boxed_slice()).ok()?;
        match self.chunks.get_mut(at) {
            Some(hole) => {
                *hole = Some(chunk);
                self.holes = self.holes.saturating_sub(1);
            }
            #[expect(clippy::disallowed_methods, reason = "reserved 1 above")]
            None => self.chunks.push(Some(chunk)),
        }
        *self.held.get_mut() += SLACK;
        Some(first)
    }

    /// The free list ran out: adds a chunk, and says whether memory allowed.
    #[cold]
    fn grow(&mut self) -> bool {
        let first = self.add_chunk().or_else(|| {
            give_slack(&self.slack, &self.asked, &self.until_gc, &self.held)
                .then(|| self.add_chunk())
                .flatten()
        });
        self.free = first.unwrap_or(NONE);
        first.is_some()
    }

    /// A new slot for `obj`, which is of kind `T`: the makers below say so, each for its kind.
    #[inline]
    fn alloc<T>(&mut self, obj: Object) -> Result<Ref<T>, Error> {
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
        // What an object holds outside its slot is counted as it comes in, and refused past the
        // limit: the one place every string, bytes value, name and context passes.
        let outside = payload(&obj);
        if outside > 0 && !self.allows(outside) {
            return Err(self.refused());
        }
        if self.free == NONE && !self.grow() {
            return Err(self.refused());
        }
        let idx = self.free;
        if let Some(slot) = slot_mut(&mut self.chunks, idx)
            && let Object::Free(next) = slot.obj
        {
            slot.obj = obj;
            self.free = next;
            *self.held.get_mut() += outside;
            return Ok(Ref {
                idx,
                generation: slot.generation,
                kind: PhantomData,
            });
        }
        Err(self.refused())
    }

    /// A new object of kind `T`.
    #[inline]
    pub fn make<T: Kind>(&mut self, holds: T::Holds) -> Result<Ref<T>, Error> {
        self.alloc(T::object(holds))
    }

    /// The live symbol spelled `spelling`, or a new one. The index has room before the slot is
    /// made, so it changes only once the name has its slot, and a refusal leaves both as they were.
    pub fn intern(&mut self, spelling: &str) -> Result<Symbol, Error> {
        if let Some(&id) = self.names.get(spelling) {
            return Ok(Symbol(id));
        }
        let (name, key) = (boxed(spelling)?, boxed(spelling)?);
        self.names.try_reserve(1)?;
        let id = self.alloc::<()>(Object::Symbol(name))?.idx;
        #[expect(clippy::disallowed_methods, reason = "reserved 1 above")]
        self.names.insert(key, id);
        Ok(Symbol(id))
    }

    /// Binds each of `new` in context `r` itself, replacing bindings there: all of them, or with
    /// no room, none.
    pub fn bind<I>(&mut self, r: Ref<kind::Context>, new: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = (Symbol, Value), IntoIter: ExactSizeIterator>,
    {
        let new = new.into_iter();
        let bindings = self.room_in(r, new.len())?;
        #[expect(clippy::disallowed_methods, reason = "room_in reserved new.len()")]
        bindings.extend(new);
        Ok(())
    }

    /// Takes `s`'s binding out of context `r` itself: whether it had one.
    pub fn unbind(&mut self, r: Ref<kind::Context>, s: Symbol) -> Result<bool, Error> {
        Ok(self.room_in(r, 0)?.remove(&s).is_some())
    }

    /// Context `r`'s bindings, with room for `more` new ones. Storage they grow by brings the
    /// next collection nearer, as a new object's weight does.
    fn room_in(&mut self, r: Ref<kind::Context>, more: usize) -> Result<&mut Bindings, Error> {
        let (_, bindings) = self.get(r)?;
        let (before, held) = (bindings_weight(bindings), bindings.capacity());
        // A table that must grow grows to twice its size at least: under the limit, or not at all.
        if held - bindings.len() < more {
            self.fits(held.max(more).saturating_mul(size_of::<(Symbol, Value)>()))?;
        }
        let (_, bindings) = live_mut(&mut self.chunks, r)?;
        if bindings.try_reserve(more).is_err() {
            if !give_slack(&self.slack, &self.asked, &self.until_gc, &self.held) {
                return Err(Error::oom());
            }
            bindings.try_reserve(more)?;
        }
        let taken = (bindings.capacity() - held).saturating_mul(size_of::<(Symbol, Value)>());
        *self.held.get_mut() += taken;
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

    /// Memory was refused, to the heap or to anything else: the next safe point collects, and the
    /// spare room is given up for what reports the refusal.
    pub fn refusal(&self) {
        self.request_collection();
        let spare = self.spare.take().capacity();
        self.held.set(self.held.get().saturating_sub(spare));
        self.refused.set(true);
    }

    /// Whether memory was refused since this was last asked.
    pub fn take_refused(&self) -> bool {
        self.refused.take()
    }

    /// What the object `r` names holds, if its slot still holds one of its kind.
    #[inline]
    fn get<T: Kind>(&self, r: Ref<T>) -> Result<&T::Holds, Error> {
        if let Some(s) = slot(&self.chunks, r.idx)
            && s.generation == r.generation
            && let Some(holds) = T::of(&s.obj)
        {
            return Ok(holds);
        }
        Err(Error::stale())
    }

    /// The binding of `s` in `context` or the nearest parent that has one. The walk holds the
    /// list of chunks once, where a read at each context would load it again.
    #[inline]
    pub fn find(&self, mut context: Ref<kind::Context>, s: Symbol) -> Result<Option<Value>, Error> {
        let chunks = &*self.chunks;
        loop {
            let (parent, bindings) = match slot(chunks, context.idx) {
                Some(Slot {
                    generation,
                    obj: Object::Context(holds),
                    ..
                }) if *generation == context.generation => holds,
                _ => return Err(Error::stale()),
            };
            if let Some(&v) = bindings.get(&s) {
                return Ok(Some(v));
            }
            let Some(parent) = parent else {
                return Ok(None);
            };
            context = *parent;
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

    /// Flags a library operative as having a native twin, or as having none.
    pub fn install(
        &mut self,
        r: Ref<kind::Operative>,
        native: Option<Native>,
    ) -> Result<(), Error> {
        live_mut(&mut self.chunks, r)?.native = native;
        Ok(())
    }

    /// The name in symbol slot `id` and its generation, if the slot holds a name.
    #[inline]
    pub fn symbol(&self, id: u32) -> Option<(&str, u32)> {
        match slot(&self.chunks, id) {
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

    /// Marks everything reachable from `roots`, the known names and the host's `held`, and frees
    /// the rest, with no memory of its own: a slot goes on the mark stack only as it is first
    /// marked, so once at most, and the stack's links are the slots' own. Each root, known
    /// name and held reference is a step of the walk outside the heap, counted as it is marked, so
    /// what pacing pays for is what was walked. The free list is made again from the bottom up, so
    /// the lowest slots are reused first and the chunks above empty, to be given back: dropping
    /// one needs no memory and can't be refused.
    ///
    /// The bytes the limit counts are counted again here, from what is left: `outside` is what
    /// the interpreter holds outside the heap, its machine stacks and tables.
    pub fn collect(
        &mut self,
        roots: impl Iterator<Item = Value>,
        held: impl Iterator<Item = Ref>,
        outside: usize,
    ) {
        let asked = self.asked.take();
        let early = self.early && !asked && self.until_gc.get() > 0;
        let mut marking = Marking {
            chunks: &self.chunks,
            top: END,
            live: 0,
            bytes: 0,
        };
        let known = (0..).zip(K_NAMES).map(|(id, _)| Value::Symbol(Symbol(id)));
        let walked = (roots.chain(known)).map(|v| marking.mark(v)).count()
            + held.map(|r| marking.hold(r)).count();
        while let Some(s) = marking.pop() {
            children(&s.obj, |v| marking.mark(v));
        }
        let live = marking.live.saturating_add(self.retired);
        let payloads = marking.bytes;
        #[cfg(test)]
        {
            self.probe.collections += 1;
            // Marking walks each context's bindings too.
            let bindings: usize = (self.chunks.iter().flatten())
                .flat_map(|chunk| chunk.iter())
                .map(|s| match &s.obj {
                    Object::Context((_, b)) => bindings_weight(b),
                    _ => 0,
                })
                .sum();
            let walks = self.len() + walked + bindings;
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
        let paced = live.max(walked).max(GC_MIN);
        // Each free slot is linked to from the one below it, so the list runs from the bottom up.
        let mut free = 0usize;
        let mut tail = &mut self.free;
        let names = &mut self.names;
        for (first, chunk) in (0..).step_by(CHUNK).zip(self.chunks.iter_mut()) {
            // The chunk's free slots are linked among themselves first: `lowest` is the first of
            // them, `highest` the place of the last, and `count` how many.
            let (mut lowest, mut highest, mut count) = (NONE, 0, 0usize);
            // The highest generation among them, and whether a slot in the chunk is in use.
            let (mut past, mut used) = (0, false);
            let mut link = &mut lowest;
            for (at, s) in chunk
                .iter_mut()
                .flat_map(|slots| slots.iter_mut().enumerate())
            {
                if s.gray.replace(UNMARKED) != UNMARKED {
                    used = true;
                    continue;
                }
                if !matches!(s.obj, Object::Free(_)) {
                    // A slot whose generations run out is retired, never reused: its last one,
                    // which no reference has, means free for good.
                    s.generation = s.generation.saturating_add(1);
                    if s.generation == u32::MAX {
                        self.retired = self.retired.saturating_add(1);
                    }
                    if let Object::Symbol(name) = std::mem::replace(&mut s.obj, Object::Free(NONE))
                    {
                        names.remove(&name);
                    }
                }
                if s.generation == u32::MAX {
                    used = true;
                } else if let Object::Free(next) = &mut s.obj {
                    *link = first + at as u32;
                    link = next;
                    (highest, count, past) = (at, count + 1, past.max(s.generation));
                }
            }
            if count == 0 {
                continue;
            }
            // An empty chunk is given back once the free slots below it are the room the next
            // collection waits for, which given back the heap would only grow into again. A
            // chunk made in its place starts past its generations. New slots all start past
            // those, so one slot reused billions of times would make every new one retire soon:
            // past half their generations, the chunk stays.
            if !used && free >= paced && past < u32::MAX / 2 {
                self.fresh = self.fresh.max(past);
                *chunk = None;
                self.holes += 1;
            } else if let Some(Slot {
                obj: Object::Free(next),
                ..
            }) = chunk.as_mut().and_then(|slots| slots.get_mut(highest))
            {
                *tail = lowest;
                tail = next;
                free += count;
            }
        }
        *tail = NONE;
        // The list ends at its last chunk.
        while let Some(None) = self.chunks.last() {
            self.chunks.pop();
            self.holes = self.holes.saturating_sub(1);
        }
        // The index gives back its storage once under a quarter used, if memory allows.
        if names.capacity() / 4 > names.len() {
            let mut smaller = HashMap::new();
            if smaller.try_reserve(names.len()).is_ok() {
                #[expect(clippy::disallowed_methods, reason = "reserved names.len() above")]
                smaller.extend(names.drain());
                *names = smaller;
            }
        }
        self.early = free / 2 > paced && !early && !asked;
        *self.until_gc.get_mut() = paced.max(free / 2);
        // What is held now: the chunks left, what the live objects hold outside them, what the
        // interpreter holds outside the heap, and the spare and slack room if they are held.
        let chunks = (self.chunks.len() - self.holes).saturating_mul(SLACK);
        let back = self.spare.get_mut().capacity() + self.slack.get_mut().capacity();
        *self.held.get_mut() = (chunks.saturating_add(payloads))
            .saturating_add(outside)
            .saturating_add(back);
        // The slack given up since the last collection is taken again, now that the chunks this
        // one emptied are given back. While it can't be, the heap is at the allocator's limit and
        // collects sooner: after an eighth of what it would wait for, at least half a chunk's
        // worth. So what is refused then, a collection could have made little room for, and a
        // large heap is still walked in proportion to what is allocated.
        let sooner = (paced / 8).max(CHUNK / 2);
        if !self.hold_slack() {
            self.asked.set(true);
            *self.until_gc.get_mut() = sooner;
        } else if self.limit.saturating_sub(self.held.get()) / size_of::<Slot>()
            < self.until_gc.get()
        {
            // Under a limit the next collection comes before what is allocated could reach it,
            // though no sooner than at the limit itself: so garbage is collected before a
            // request larger than the slack is refused for it.
            let room = self.limit.saturating_sub(self.held.get()) / size_of::<Slot>();
            self.asked.set(true);
            *self.until_gc.get_mut() = room.max(sooner);
        }
        #[cfg(test)]
        {
            self.probe.live = live.saturating_add(walked);
            // A collection stress mode forced is not paced, as one asked for isn't.
            self.early &= !self.probe.stress;
        }
    }
}

/// Gives up the slack room, if it is held, and asks for a collection at the next safe point:
/// whether there was any to give. It takes the fields alone, so a caller may hold a slot.
#[cold]
fn give_slack(
    slack: &Cell<Vec<u8>>,
    asked: &Cell<bool>,
    until_gc: &Cell<usize>,
    counted: &Cell<usize>,
) -> bool {
    let given = slack.take().capacity();
    if given > 0 {
        asked.set(true);
        until_gc.set(0);
        counted.set(counted.get().saturating_sub(given));
    }
    given > 0
}

/// Takes `bytes` of room to hold back in `room`, if it isn't held and the limit allows it:
/// whether it is held now. Counted as it is taken.
fn held_back(room: &mut Vec<u8>, bytes: usize, limit: usize, counted: &Cell<usize>) -> bool {
    if room.capacity() == 0
        && counted.get().saturating_add(bytes) <= limit
        && room.try_reserve_exact(bytes).is_ok()
    {
        counted.set(counted.get().saturating_add(room.capacity()));
    }
    room.capacity() > 0
}

/// The slot numbered `idx`, if its chunk is there.
#[inline]
fn slot(chunks: &[Option<Chunk>], idx: u32) -> Option<&Slot> {
    (chunks.get(idx as usize / CHUNK)?.as_ref()?).get(idx as usize % CHUNK)
}

#[inline]
fn slot_mut(chunks: &mut [Option<Chunk>], idx: u32) -> Option<&mut Slot> {
    (chunks.get_mut(idx as usize / CHUNK)?.as_mut()?).get_mut(idx as usize % CHUNK)
}

/// What the object `r` names holds, to change, if its slot still holds one of its kind. It takes
/// the chunks alone, so the heap's pacing stays free beside what it gives.
fn live_mut<T: Kind>(chunks: &mut [Option<Chunk>], r: Ref<T>) -> Result<&mut T::Holds, Error> {
    match slot_mut(chunks, r.idx) {
        Some(s) if s.generation == r.generation => T::of_mut(&mut s.obj).ok_or_else(Error::stale),
        _ => Err(Error::stale()),
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
/// entries, keeping twice its length and at least that: a deep stack or table costs nothing after.
/// It moves into smaller storage if memory allows, else stays as it is.
pub(crate) fn tidy<X>(v: &mut Vec<X>) {
    let room = v.len().saturating_mul(2).max(GC_MIN);
    let mut smaller = Vec::new();
    if v.capacity() / 4 > v.len().max(GC_MIN) && smaller.try_reserve_exact(room).is_ok() {
        #[expect(clippy::disallowed_methods, reason = "reserved at least v.len() above")]
        smaller.append(v);
        *v = smaller;
    }
}

/// A collection's marking: the mark stack, which runs through the slots on it, and what it has
/// reached so far, by weight.
struct Marking<'a> {
    chunks: &'a [Option<Chunk>],
    top: u32,
    live: usize,
    /// What the objects reached hold outside their slots.
    bytes: usize,
}

impl<'a> Marking<'a> {
    /// Puts `v`'s object on the mark stack, unless the collection already reached it.
    fn mark(&mut self, v: Value) {
        let r = match v {
            // A name refers to nothing, so it is marked where it is.
            Value::Symbol(id) => {
                if let Some(s) = slot(self.chunks, id.0)
                    && matches!(s.obj, Object::Symbol(_))
                    && s.gray.get() == UNMARKED
                {
                    s.gray.set(END);
                    self.live = self.live.saturating_add(weight(&s.obj));
                    self.bytes = self.bytes.saturating_add(payload(&s.obj));
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
        self.hold(r);
    }

    /// Puts the object `r` names on the mark stack, unless the collection already reached it: a
    /// value's, or one a host holds, whose kind marking needn't know.
    fn hold(&mut self, r: Ref) {
        if let Some(s) = slot(self.chunks, r.idx)
            && s.generation == r.generation
            && s.gray.get() == UNMARKED
        {
            s.gray.set(self.top);
            self.top = r.idx;
            self.live = self.live.saturating_add(weight(&s.obj));
            self.bytes = self.bytes.saturating_add(payload(&s.obj));
        }
    }

    /// Takes the top slot off the mark stack. It stays marked: its link is never [`UNMARKED`].
    fn pop(&mut self) -> Option<&'a Slot> {
        let s = slot(self.chunks, self.top)?;
        self.top = s.gray.get();
        Some(s)
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

impl<T> crate::Grift<T> {
    /// Lends string `r`'s text to `f`, with the interpreter, so it needs no copy: the string is
    /// empty while `f` runs and has its text back as `f` returns. Here, beside the heap's raw
    /// slots, so no other code can take a string's text out or put any back.
    pub(crate) fn lend_text<R>(
        &mut self,
        r: Ref<kind::String>,
        f: impl FnOnce(&mut Self, &str) -> R,
    ) -> Result<R, Error> {
        let text = std::mem::take(live_mut(&mut self.heap.chunks, r)?);
        let done = f(self, &text);
        *live_mut(&mut self.heap.chunks, r)? = text;
        Ok(done)
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
        /// Refuse new chunks past this many slots, to test recovery without replacing the allocator.
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
        /// Steps the machine took, for tests of what a form costs.
        pub steps: u64,
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
            (self.chunks.len() - self.holes) * CHUNK
        }

        /// The index of live names.
        pub fn names(&self) -> &HashMap<Box<str>, u32> {
            &self.names
        }
    }
}
