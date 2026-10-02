//! The heap: a `Vec` of object slots with a free list and mark-and-sweep GC.
//!
//! Every object is immutable once it is observable by a program. The only
//! internal write is the initialization of a recursive binding (see
//! [`Log`]), which the evaluator guards so that no program can observe a
//! binding changing value.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::NativeFn;
use crate::value::{ObjId, PrimId, SymId, Value};

/// Logs with at least this many entries get a symbol index.
const INDEX_THRESHOLD: usize = 16;

/// Marks "no position" in a [`LogIndex`].
const NONE: u32 = u32::MAX;

/// An index over a large log: for each symbol the position of its latest
/// entry, and for each entry the position of the previous entry for the
/// same symbol. Finding the latest entry visible in a view walks the
/// `prev` chain past entries the view cannot see, which is short because
/// symbols are rarely rebound.
#[derive(Clone, Debug, Default)]
pub(crate) struct LogIndex {
    last: Vec<u32>,
    prev: Vec<u32>,
}

impl LogIndex {
    fn push(&mut self, sym: SymId, pos: u32) {
        let s = sym.0 as usize;
        if s >= self.last.len() {
            self.last.resize(s + 1, NONE);
        }
        self.prev.push(self.last[s]);
        self.last[s] = pos;
    }

    fn find(&self, sym: SymId, len: u32) -> Option<usize> {
        let mut p = *self.last.get(sym.0 as usize)?;
        while p != NONE && p >= len {
            p = self.prev[p as usize];
        }
        (p != NONE).then_some(p as usize)
    }
}

/// An append-only log of bindings shared by a family of environments.
///
/// An environment is a *view* `(log, len)` that sees the first `len`
/// entries of a log plus its parent environment. Extending an environment
/// whose view ends at the end of its log appends to the log, which leaves
/// every existing view unchanged; extending any other environment starts a
/// new log. This keeps environments immutable while making long chains of
/// top-level definitions cheap to look up.
#[derive(Clone, Debug)]
pub(crate) struct Log {
    /// The environment this log's bindings are layered on.
    pub(crate) parent: Option<ObjId>,
    /// Bindings in definition order. `None` marks a recursive binding that
    /// has not been initialized yet.
    pub(crate) entries: Vec<(SymId, Option<Value>)>,
    /// Built once the log is large.
    pub(crate) index: Option<Box<LogIndex>>,
}

impl Log {
    /// Create a log holding `entries`, layered on `parent`.
    pub(crate) fn new(parent: Option<ObjId>, entries: Vec<(SymId, Option<Value>)>) -> Self {
        let mut log = Log {
            parent,
            entries,
            index: None,
        };
        log.maybe_index();
        log
    }

    /// Append entries to the log.
    pub(crate) fn append(&mut self, entries: impl IntoIterator<Item = (SymId, Option<Value>)>) {
        for (sym, val) in entries {
            let pos = u32::try_from(self.entries.len()).expect("environment too large");
            self.entries.push((sym, val));
            if let Some(index) = &mut self.index {
                index.push(sym, pos);
            }
        }
        self.maybe_index();
    }

    pub(crate) fn maybe_index(&mut self) {
        if self.index.is_none() && self.entries.len() >= INDEX_THRESHOLD {
            let mut index = LogIndex::default();
            for (pos, (sym, _)) in self.entries.iter().enumerate() {
                index.push(*sym, pos as u32);
            }
            self.index = Some(Box::new(index));
        }
    }

    /// Find the latest entry for `sym` among the first `len` entries.
    #[inline]
    pub(crate) fn find(&self, sym: SymId, len: u32) -> Option<usize> {
        if let Some(index) = &self.index {
            return index.find(sym, len);
        }
        self.entries[..len as usize]
            .iter()
            .rposition(|(s, _)| *s == sym)
    }
}

/// An environment: a view of the first `len` entries of a [`Log`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Env {
    pub(crate) log: ObjId,
    pub(crate) len: u32,
}

/// Maximum number of parameter bindings a [`SmallEnv`] holds inline.
pub(crate) const SMALL_ENV: usize = 3;

/// The environment of an operative call, with its bindings held inline:
/// up to [`SMALL_ENV`] parameter bindings plus the environment parameter.
/// Most calls bind this few symbols, so a call allocates one object and a
/// lookup in it touches a single heap slot.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SmallEnv {
    pub(crate) parent: ObjId,
    /// The environment parameter and the caller's environment, if bound.
    pub(crate) eparam: Option<(SymId, ObjId)>,
    pub(crate) len: u8,
    pub(crate) syms: [SymId; SMALL_ENV],
    pub(crate) vals: [Value; SMALL_ENV],
}

impl SmallEnv {
    /// The value bound to `sym` in this frame, if any.
    #[inline]
    pub(crate) fn find(&self, sym: SymId) -> Option<Value> {
        for i in 0..self.len as usize {
            if self.syms[i] == sym {
                return Some(self.vals[i]);
            }
        }
        match self.eparam {
            Some((s, e)) if s == sym => Some(Value::Obj(e)),
            _ => None,
        }
    }
}

/// A compound operative created by `$vau`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Operative {
    pub(crate) ptree: Value,
    /// A symbol, or `#ignore`.
    pub(crate) eparam: Value,
    pub(crate) body: Value,
    /// The static environment (an `Env` object).
    pub(crate) env: ObjId,
}

/// A host-registered native function.
#[derive(Clone, Copy)]
pub(crate) struct Native {
    pub(crate) name: SymId,
    pub(crate) f: NativeFn,
}

impl core::fmt::Debug for Native {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Native({:?})", self.name)
    }
}

/// A captured subcontinuation: the frames from `top` down to, but not
/// including, the prompt frame `bottom`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SubCont {
    pub(crate) top: ObjId,
    pub(crate) bottom: ObjId,
}

/// A continuation frame. Frames are immutable and linked by parent
/// pointers, so a frame chain can be shared by any number of captured
/// subcontinuations.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub(crate) parent: Option<ObjId>,
    pub(crate) kind: FrameKind,
}

/// What a frame does with the value returned to it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameKind {
    /// The head of a combination (itself a combination) is being
    /// evaluated; apply the result to `operands`.
    Combine { operands: Value, env: ObjId },
    /// Operands of an applicative are being evaluated, left to right.
    /// `acc` holds the values so far in reverse order; `rest` holds the
    /// operands still to evaluate; `comb` is the unwrapped combiner.
    Args {
        comb: Value,
        rest: Value,
        acc: Value,
        env: ObjId,
    },
    /// The initializer of `environment-bind-recursive` is being evaluated.
    InitRec { env: ObjId, ptree: Value },
    /// A delimiter installed by `push-prompt`. `host` marks the error
    /// prompt installed by the host around every top-level evaluation.
    Prompt { prompt: ObjId, host: bool },
    /// The value returned here is an error payload for the host.
    HostError,
    /// The value returned here must be an environment (the result of a
    /// declaration's transformer).
    CheckEnv,
}

/// A heap object.
#[derive(Clone, Debug)]
pub(crate) enum Object {
    /// An unused slot; holds the next free slot.
    Free(Option<u32>),
    Pair(Value, Value),
    Str(Box<str>),
    Log(Log),
    Env(Env),
    SmallEnv(SmallEnv),
    Operative(Operative),
    /// An applicative wrapping the given combiner.
    Applicative(Value),
    /// The operative core of a primitive that closes over data (e.g. the
    /// constructor of an encapsulation type).
    PrimData(PrimId, Value),
    Native(Native),
    /// An encapsulated value of the encapsulation type with the given tag.
    Encap(u32, Value),
    /// A declaration wrapping an applicative `env -> env`.
    Declaration(Value),
    Prompt,
    SubCont(SubCont),
    Frame(Frame),
}

/// Heap occupancy statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeapStats {
    /// Number of slots in the heap (live and free).
    pub capacity: usize,
    /// Number of live objects.
    pub live: usize,
    /// Number of collections performed so far.
    pub collections: usize,
}

/// The object store.
#[derive(Clone, Debug)]
pub(crate) struct Heap {
    slots: Vec<Object>,
    marks: Vec<bool>,
    free: Option<u32>,
    live: usize,
    /// Allocations since the last collection.
    pub(crate) since_gc: usize,
    /// Collect once `since_gc` exceeds this.
    pub(crate) threshold: usize,
    collections: usize,
    /// Entry vectors of collected logs, kept for reuse.
    pool: Vec<Vec<(SymId, Option<Value>)>>,
}

/// Maximum number of entry vectors kept for reuse.
const POOL_LIMIT: usize = 4096;

/// Minimum number of allocations between collections.
const MIN_THRESHOLD: usize = 1 << 16;

impl Default for Heap {
    fn default() -> Self {
        Heap {
            slots: Vec::new(),
            marks: Vec::new(),
            free: None,
            live: 0,
            since_gc: 0,
            threshold: MIN_THRESHOLD,
            collections: 0,
            pool: Vec::new(),
        }
    }
}

impl Heap {
    /// An empty entry vector, recycled from a collected log when possible.
    #[inline]
    pub(crate) fn entry_vec(&mut self) -> Vec<(SymId, Option<Value>)> {
        self.pool.pop().unwrap_or_default()
    }

    /// Allocate an object.
    pub(crate) fn alloc(&mut self, obj: Object) -> ObjId {
        self.live += 1;
        self.since_gc += 1;
        if let Some(i) = self.free {
            let Object::Free(next) = self.slots[i as usize] else {
                unreachable!("free list points at a live slot")
            };
            self.free = next;
            self.slots[i as usize] = obj;
            ObjId(i)
        } else {
            let i = u32::try_from(self.slots.len()).expect("heap exhausted");
            self.slots.push(obj);
            self.marks.push(false);
            ObjId(i)
        }
    }

    #[inline]
    pub(crate) fn get(&self, id: ObjId) -> &Object {
        &self.slots[id.index()]
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, id: ObjId) -> &mut Object {
        &mut self.slots[id.index()]
    }

    /// The log behind an `Env` object, and the view length.
    pub(crate) fn env(&self, id: ObjId) -> Env {
        match self.get(id) {
            Object::Env(e) => *e,
            other => unreachable!("expected environment, found {other:?}"),
        }
    }

    pub(crate) fn log(&self, id: ObjId) -> &Log {
        match self.get(id) {
            Object::Log(l) => l,
            other => unreachable!("expected log, found {other:?}"),
        }
    }

    pub(crate) fn log_mut(&mut self, id: ObjId) -> &mut Log {
        match self.get_mut(id) {
            Object::Log(l) => l,
            other => unreachable!("expected log, found {other:?}"),
        }
    }

    pub(crate) fn frame(&self, id: ObjId) -> Frame {
        match self.get(id) {
            Object::Frame(f) => *f,
            other => unreachable!("expected frame, found {other:?}"),
        }
    }

    pub(crate) fn stats(&self) -> HeapStats {
        HeapStats {
            capacity: self.slots.len(),
            live: self.live,
            collections: self.collections,
        }
    }

    /// Mark everything reachable from `roots` and free everything else.
    ///
    /// A log's entries are traced only up to the longest view of it that is
    /// still live; entries beyond that can never be seen again, so they are
    /// dropped. This reclaims bindings appended by environments that have
    /// since become garbage.
    pub(crate) fn collect(&mut self, roots: impl IntoIterator<Item = Value>) {
        let mut work: Vec<ObjId> = Vec::new();
        for r in roots {
            if let Value::Obj(id) = r {
                work.push(id);
            }
        }
        // For each log slot, how many of its entries have been traced.
        let mut traced: Vec<u32> = alloc::vec![0; self.slots.len()];
        while let Some(id) = work.pop() {
            let i = id.index();
            if let Object::Env(e) = self.slots[i] {
                // Trace the part of the log this view can see, even if the
                // view itself was already marked through another path.
                let l = e.log.index();
                if !self.marks[l] {
                    self.marks[l] = true;
                    if let Object::Log(Log {
                        parent: Some(p), ..
                    }) = &self.slots[l]
                    {
                        work.push(*p);
                    }
                }
                if traced[l] < e.len {
                    if let Object::Log(log) = &self.slots[l] {
                        for (_, v) in &log.entries[traced[l] as usize..e.len as usize] {
                            if let Some(Value::Obj(id)) = v {
                                work.push(*id);
                            }
                        }
                    }
                    traced[l] = e.len;
                }
                self.marks[i] = true;
                continue;
            }
            if self.marks[i] {
                continue;
            }
            self.marks[i] = true;
            trace(&self.slots[i], &mut work);
        }
        for (i, &upto) in traced.iter().enumerate() {
            if !self.marks[i] {
                continue;
            }
            if let Object::Log(log) = &mut self.slots[i] {
                if (upto as usize) < log.entries.len() {
                    log.entries.truncate(upto as usize);
                    log.index = None;
                    log.maybe_index();
                }
            }
        }
        let mut live = 0;
        let mut free = None;
        for i in (0..self.slots.len()).rev() {
            if self.marks[i] {
                self.marks[i] = false;
                live += 1;
            } else {
                let old = core::mem::replace(&mut self.slots[i], Object::Free(free));
                if let Object::Log(log) = old {
                    if self.pool.len() < POOL_LIMIT && log.entries.capacity() <= 16 {
                        let mut v = log.entries;
                        v.clear();
                        self.pool.push(v);
                    }
                }
                free = Some(i as u32);
            }
        }
        self.free = free;
        self.live = live;
        self.since_gc = 0;
        // Collect again once the live data could have doubled, but never
        // before the free slots are used up: a sweep costs time proportional
        // to the whole heap, so this keeps its cost per allocation bounded.
        self.threshold = MIN_THRESHOLD.max(live * 2).max(self.slots.len() - live);
        self.collections += 1;
    }
}

/// Push the heap children of `obj` onto `work`.
fn trace(obj: &Object, work: &mut Vec<ObjId>) {
    fn push_value(work: &mut Vec<ObjId>, v: Value) {
        if let Value::Obj(id) = v {
            work.push(id);
        }
    }
    macro_rules! push {
        ($v:expr) => {
            push_value(work, $v)
        };
    }
    match obj {
        Object::Free(_) | Object::Str(_) | Object::Native(_) | Object::Prompt => {}
        Object::Pair(a, b) => {
            push!(*a);
            push!(*b);
        }
        // Logs and environment views are traced by `Heap::collect` itself.
        Object::Log(_) | Object::Env(_) => {}
        Object::SmallEnv(e) => {
            work.push(e.parent);
            if let Some((_, caller)) = e.eparam {
                work.push(caller);
            }
            for v in &e.vals[..e.len as usize] {
                push!(*v);
            }
        }
        Object::Operative(op) => {
            push!(op.ptree);
            push!(op.eparam);
            push!(op.body);
            work.push(op.env);
        }
        Object::Applicative(v)
        | Object::PrimData(_, v)
        | Object::Encap(_, v)
        | Object::Declaration(v) => push!(*v),
        Object::SubCont(k) => {
            work.push(k.top);
            work.push(k.bottom);
        }
        Object::Frame(f) => {
            if let Some(p) = f.parent {
                work.push(p);
            }
            match f.kind {
                FrameKind::Combine { operands, env } => {
                    push!(operands);
                    work.push(env);
                }
                FrameKind::Args {
                    comb,
                    rest,
                    acc,
                    env,
                } => {
                    push!(comb);
                    push!(rest);
                    push!(acc);
                    work.push(env);
                }
                FrameKind::InitRec { env, ptree } => {
                    work.push(env);
                    push!(ptree);
                }
                FrameKind::Prompt { prompt, .. } => work.push(prompt),
                FrameKind::HostError | FrameKind::CheckEnv => {}
            }
        }
    }
}
