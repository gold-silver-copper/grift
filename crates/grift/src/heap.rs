//! The heap: a growable vector of slots, reused through a free list and reclaimed by mark and
//! sweep.

use crate::{Error, K, Symbol, Value, push};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A reference to a heap slot. The generation tells a live object from a reclaimed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ref {
    idx: u32,
    generation: u32,
}

/// A compound operative: formals tree, environment formal, body, and the static environment.
#[derive(Clone, Copy)]
pub(crate) struct Op {
    pub formals: Value,
    pub eformal: Value,
    pub body: Value,
    pub env: Ref,
}

pub(crate) enum Obj {
    /// A free slot and the next free one.
    Free(u32),
    Pair(Value, Value),
    Str(Box<str>),
    /// An optional parent and the bindings.
    Env(Option<Ref>, Binds),
    Op(Op),
    App(Value),
}

/// An environment's bindings.
pub(crate) type Binds = HashMap<Symbol, Value, BuildHasherDefault<SymbolHasher>>;

/// Hashes a symbol by its id, a small integer: one multiplication spreads it over the bits the
/// table looks at.
#[derive(Default)]
pub(crate) struct SymbolHasher(u64);

impl Hasher for SymbolHasher {
    fn finish(&self) -> u64 {
        self.0.wrapping_mul(0x9e37_79b9_7f4a_7c15)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(b);
        }
    }

    fn write_u32(&mut self, n: u32) {
        self.0 = u64::from(n);
    }
}

struct Slot {
    generation: u32,
    marked: bool,
    /// Live when the library had been loaded: its code, and the code it built.
    library: bool,
    obj: Obj,
}

const NONE: u32 = u32::MAX;

/// Collections start once this many objects were allocated since the last one, or as many as
/// survived it if that is more, so the heap stays within about twice the live data.
const GC_MIN: usize = 4096;

pub(crate) struct Heap {
    slots: Vec<Slot>,
    free: u32,
    since_gc: usize,
    live: usize,
    marks: Vec<Ref>,
    /// Operatives made, for tests of what a form costs.
    #[cfg(test)]
    pub operatives: usize,
}

impl Default for Heap {
    fn default() -> Heap {
        Heap {
            slots: Vec::new(),
            free: NONE,
            since_gc: 0,
            live: 0,
            marks: Vec::new(),
            #[cfg(test)]
            operatives: 0,
        }
    }
}

fn reference(v: Value) -> Option<Ref> {
    match v {
        Value::Pair(r) | Value::Str(r) | Value::Env(r) | Value::Op(r) | Value::App(r) => Some(r),
        _ => None,
    }
}

impl Ref {
    /// No object: a reference no slot has.
    pub(crate) const NONE: Ref = Ref {
        idx: NONE,
        generation: 0,
    };
}

impl Heap {
    #[inline]
    pub fn alloc(&mut self, obj: Obj) -> Result<Ref, Error> {
        self.since_gc = self.since_gc.saturating_add(1);
        #[cfg(test)]
        {
            self.operatives += usize::from(matches!(obj, Obj::Op(_)));
        }
        let idx = self.free;
        if let Some(slot) = self.slots.get_mut(idx as usize)
            && let Obj::Free(next) = slot.obj
        {
            self.free = next;
            slot.obj = obj;
            return Ok(Ref {
                idx,
                generation: slot.generation,
            });
        }
        let idx = u32::try_from(self.slots.len())
            .ok()
            .filter(|&i| i != NONE)
            .ok_or_else(Error::oom)?;
        push(
            &mut self.slots,
            Slot {
                generation: 0,
                marked: false,
                library: false,
                obj,
            },
        )?;
        Ok(Ref { idx, generation: 0 })
    }

    /// Marks every object now in the heap as the library's. A slot loses the mark when it is
    /// freed.
    pub fn seal_library(&mut self) {
        for s in &mut self.slots {
            s.library = !matches!(s.obj, Obj::Free(_));
        }
    }

    /// The object, and whether it is the library's: its code, or code it built while it was
    /// loaded.
    #[inline]
    pub fn get_code(&self, r: Ref) -> Result<(&Obj, bool), Error> {
        self.slot(r)
            .map(|s| (&s.obj, s.library))
            .ok_or_else(|| Error::new(K::StaleValue, &[]))
    }

    #[inline]
    fn slot(&self, r: Ref) -> Option<&Slot> {
        self.slots
            .get(r.idx as usize)
            .filter(|s| s.generation == r.generation)
    }

    #[inline]
    pub fn get(&self, r: Ref) -> Result<&Obj, Error> {
        self.slot(r)
            .map(|s| &s.obj)
            .ok_or_else(|| Error::new(K::StaleValue, &[]))
    }

    #[inline]
    pub fn get_mut(&mut self, r: Ref) -> Result<&mut Obj, Error> {
        match self.slots.get_mut(r.idx as usize) {
            Some(s) if s.generation == r.generation => Ok(&mut s.obj),
            _ => Err(Error::new(K::StaleValue, &[])),
        }
    }

    /// Whether a collection is due. The machine asks only at its safe point.
    #[inline]
    pub fn due(&self) -> bool {
        self.since_gc >= self.live.max(GC_MIN)
    }

    /// Marks everything reachable from `roots` and frees the rest. If the mark stack can't grow,
    /// nothing is freed and the error is `out-of-memory`.
    pub fn collect(&mut self, roots: impl Iterator<Item = Value>) -> Result<(), Error> {
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
                self.live += 1;
            } else if !matches!(s.obj, Obj::Free(_)) {
                s.obj = Obj::Free(self.free);
                s.library = false;
                s.generation = s.generation.wrapping_add(1);
                self.free = idx;
            }
        }
        self.since_gc = 0;
        Ok(())
    }

    fn mark(
        &mut self,
        roots: impl Iterator<Item = Value>,
        marks: &mut Vec<Ref>,
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
                    Obj::Free(_) | Obj::Str(_) => {}
                    Obj::Pair(a, d) => {
                        child(*a)?;
                        child(*d)?;
                    }
                    Obj::Env(parent, binds) => {
                        if let Some(p) = parent {
                            child(Value::Env(*p))?;
                        }
                        for v in binds.values() {
                            child(*v)?;
                        }
                    }
                    Obj::Op(op) => {
                        child(op.formals)?;
                        child(op.body)?;
                        child(Value::Env(op.env))?;
                    }
                    Obj::App(c) => child(*c)?,
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
