#![doc = include_str!("../README.md")]

#[cfg(test)]
mod ending_tests;
mod evaluate;
mod heap;
mod interrupt;
mod local;
mod parked;
#[cfg(test)]
mod parked_tests;
mod print;
mod read;
#[cfg(test)]
mod tests;

use heap::{Bindings, Heap, IdHasher, Object};
use interrupt::Control;
use local::{K, boxed, push};
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::{Arc, Weak};

pub use interrupt::Interrupter;
pub use parked::{Answer, Parked, ParkingFunction, Ran, ResumeError};
pub use print::write_string;

/// The library, evaluated in the built-in context by [`Grift::new`].
const LIBRARY: &str = include_str!("library.grift");

/// How many items a walk inside one step takes between looks for a stop: a power of two.
const WALK_CHECK: usize = 4096;

/// A grift value. Immediate values are plain data; the rest belong to the [`Grift`] that made
/// them, and every operation checks that: another interpreter's value is the error
/// `foreign-value`, never read as one of this one's.
///
/// A value returned to the host stays valid until the next call to [`Grift::evaluate`],
/// [`Grift::evaluate_in`], [`Grift::call`] or [`Grift::resume`], or the like; the collector may then reclaim it, and using it is the error `stale-value`,
/// never another object. To keep one longer, bind it in a context or [`Grift::keep`] it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// The empty list `()`.
    EmptyList,
    /// `#true` or `#false`.
    Boolean(bool),
    /// `#inert`, the value of a form with nothing useful to return.
    Inert,
    /// `#ignore`, the parameter that binds nothing.
    Ignore,
    /// A 64-bit integer; arithmetic is checked.
    Integer(i64),
    /// A 64-bit IEEE float.
    Float(f64),
    /// An interned symbol.
    Symbol(Symbol),
    /// A pair.
    Pair(Reference),
    /// An immutable string.
    String(Reference),
    /// An immutable sequence of bytes, any bytes: what isn't text.
    Bytes(Reference),
    /// A context.
    Context(Reference),
    /// An operative, however it was made.
    Operative(Operative),
    /// An applicative: a wrapped combiner.
    Applicative(Reference),
}

/// An operative, opaque: a host holds one, compares it and writes it, which shows how it was made
/// (`#[operative (x) #ignore x]`, `#[primitive add]`, `#[host name]`), but can't look inside or
/// make one, so adding or moving a primitive changes no host's code.
#[derive(Clone, Debug, PartialEq)]
pub struct Operative(OperativeKind);

/// How an operative was made.
#[derive(Clone, Debug, PartialEq)]
enum OperativeKind {
    /// By `vau!`: an object in the heap of one [`Grift`].
    Compound(Reference),
    /// A Rust primitive, the same in every interpreter (the library binds most of them wrapped).
    Primitive(evaluate::Primitive),
    /// A host function registered with one [`Grift`], which [`Grift::host_function`] wraps.
    Host(Owner, u32),
}

/// An interned symbol of one [`Grift`]; its name is [`Grift::symbol_name`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Symbol {
    owner: Owner,
    index: u32,
}

/// An object in the heap of one [`Grift`]: a slot and its generation.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    owner: Owner,
    local: heap::Reference,
}

/// The interpreter a value belongs to: its control's allocation. Two are the same only if the
/// allocation is, even after one interpreter is dropped: a value keeps the address from reuse.
#[derive(Clone, Debug)]
struct Owner(Weak<Control>);

impl PartialEq for Owner {
    fn eq(&self, other: &Owner) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Owner {}

impl Hash for Owner {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

/// A host function. It receives the interpreter, the host's state (the one passed to
/// [`Grift::evaluate`] or [`Grift::call`]) and the evaluated arguments. One that panics ends the
/// run it was called from: the panic goes on to the host and no cleanup runs. One that replaces
/// the interpreter it was given ends the run too: the interpreter answers `foreign-value`. Either
/// way the interpreter starts its next evaluation afresh.
pub type HostFunction<T> = fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Value, Error>;

/// What went wrong: a symbol naming the kind of error, the values involved, and where.
/// [`Grift::describe`] writes it for a person.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    /// The kind, such as `type-error` or a kind a program gave `error`; [`Grift::symbol_name`]
    /// reads it.
    pub kind: Symbol,
    /// The values involved, valid as long as a returned [`Value`].
    pub values: Vec<Value>,
    /// Where: the combinations of the program that were being evaluated, innermost first, each
    /// once, ending with the top-level form. The library's code, read or built, is skipped, so
    /// `(if! 1 2 3)` fails at `(if! 1 2 3)`; a call in tail position has finished, so it isn't
    /// listed.
    /// [`Grift::position`] gives each one's source and line. Checked as the values are when the
    /// error comes in.
    pub trace: Vec<Value>,
    /// Whether the host (a stop, a force, a host function that sets this) or the machine (out of
    /// memory) began this, so that no handler could catch it. Never so for what a program raises or
    /// a host function makes with [`Grift::error`], whatever its kind is called (unless memory is
    /// refused to make it: that is the limit `out-of-memory`), and an error passing through a
    /// nested run or a host function stays what it was. Its kind then says which limit it is, or,
    /// if the host handed one in after its values were reclaimed, `stale-value`.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let forged = g.evaluate(&mut (), "(raise (error (quote! interrupted)))").unwrap_err();
    /// assert!(g.symbol_name(&forged.kind)? == "interrupted" && !forged.limit);
    /// # Ok::<(), grift::Error>(())
    /// ```
    ///
    /// A host function that returns an error with this set ends the run, past every handler,
    /// through every cleanup, as a stop does, and the error keeps its kind and values for the host
    /// to read when the run has ended.
    ///
    /// ```
    /// use grift::{Error, Grift, Value};
    /// let mut g: Grift = Grift::new()?;
    /// let quit = g.host_function("quit", |g, _, args| Err(Error { limit: true, ..g.error("quit", args) }))?;
    /// g.define("quit", quit)?;
    /// let e = g.evaluate(&mut (), "(catch! (quit 3) e 0) (add 1 2)").unwrap_err();
    /// assert!(g.symbol_name(&e.kind)? == "quit" && e.values == [Value::Integer(3)]);
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub limit: bool,
}

/// A value the host keeps across evaluations: an entry in a table the collector marks and no
/// program reaches. Read it with [`Grift::kept`]; [`Grift::release`] clears it. Neither
/// `Copy` nor `Clone`, so it is released at most once and never used after; one dropped without
/// a release keeps its value until the interpreter is dropped (there are no finalizers).
///
/// ```
/// let mut g: grift::Grift = grift::Grift::new()?;
/// let handler = g.evaluate(&mut (), "(lambda! (key) (join \"got \" key))")?;
/// let kept = g.keep(handler)?;
/// g.evaluate(&mut (), "(length (map list (range 0 100000)))")?; // collects
/// let handler = g.kept(&kept)?;
/// let key = g.string("C-b")?;
/// let answer = g.call(&mut (), &handler, &[key])?;
/// assert_eq!(g.as_str(&answer)?, Some("got C-b"));
/// g.release(kept).map_err(|(e, _)| e)?;
/// # Ok::<(), grift::Error>(())
/// ```
#[must_use]
#[derive(Debug)]
pub struct Kept {
    owner: Owner,
    name: u32,
}

/// An entry of the kept table, the collector's roots no program reaches, named by its place.
pub(crate) enum Entry {
    /// Nothing, and the next free entry.
    Free(Option<u32>),
    /// A value a [`Kept`] names.
    Value(local::Value),
    /// A parked run, which a [`Parked`] names: one machine, in a vector so that making room for it
    /// can report failure and the table stays small.
    Run(Vec<evaluate::Machine>),
}

const _: () = assert!(size_of::<Entry>() == 24);

impl Entry {
    /// What this entry holds for the collector.
    pub(crate) fn roots(&self) -> impl Iterator<Item = local::Value> {
        let (value, runs) = match self {
            Entry::Free(_) => (None, &[][..]),
            Entry::Value(v) => (Some(*v), &[][..]),
            Entry::Run(room) => (None, room.as_slice()),
        };
        let machines = runs.iter().flat_map(evaluate::Machine::roots);
        value.into_iter().chain(machines)
    }

    /// How much of the next collection's walk this entry is, beyond its value's heap objects.
    pub(crate) fn weight(&self) -> usize {
        match self {
            Entry::Run(room) => room.iter().map(evaluate::Machine::weight).sum(),
            _ => 0,
        }
    }
}

/// Where a combination was read, and whether it is one of the source's outer forms.
struct Position {
    generation: u32,
    source: local::Symbol,
    line: u32,
    outer: bool,
}

/// An interpreter: a heap, the built-in context holding the primitives and the library, and the
/// top-level context, a child of the built-in one, where programs define things. `T` is the
/// host's state, which the host lends to each [`Grift::evaluate`] and [`Grift::call`] and which
/// they pass on to host functions.
pub struct Grift<T = ()> {
    heap: Heap,
    names: Vec<Box<str>>,
    /// Names to symbol ids, for interning; `names` gives the spelling by id.
    symbols: HashMap<Box<str>, u32>,
    hosts: Vec<(local::Symbol, parked::Hosted<T>)>,
    built_in: heap::Reference,
    top: heap::Reference,
    /// The kept values and parked runs, and the first free entry.
    kept: Vec<Entry>,
    kept_free: Option<u32>,
    /// The kept name of the run that has just parked, for whoever began or resumed it.
    parked_as: Option<u32>,
    machine: evaluate::Machine,
    /// The machines of the runs under a nested run, whose host functions are waiting for it.
    suspended: Vec<evaluate::Machine>,
    /// Shared with the interrupters; its allocation is this interpreter's identity, which every
    /// value handed out holds.
    control: Arc<Control>,
    /// Where each combination read under a name is, by its slot's index.
    positions: HashMap<u32, Position, BuildHasherDefault<IdHasher>>,
}

impl<T> Grift<T> {
    /// Builds the built-in context, evaluates the library in it and makes an empty top level.
    pub fn new() -> Result<Grift<T>, Error> {
        let mut g = Grift {
            heap: Heap::default(),
            names: Vec::new(),
            symbols: HashMap::new(),
            hosts: Vec::new(),
            built_in: heap::Reference::NONE,
            top: heap::Reference::NONE,
            kept: Vec::new(),
            kept_free: None,
            parked_as: None,
            machine: evaluate::Machine::default(),
            suspended: Vec::new(),
            control: Arc::new(Control::default()),
            positions: HashMap::default(),
        };
        let loaded = g.load();
        g.out(loaded).map(|()| g)
    }

    fn load(&mut self) -> Result<(), local::Error> {
        let context =
            |heap: &mut Heap, parent| heap.alloc(Object::Context(parent, Bindings::default()));
        self.built_in = context(&mut self.heap, None)?;
        self.top = context(&mut self.heap, Some(self.built_in))?;
        for name in local::K_NAMES {
            self.intern(name)?;
        }
        for &p in evaluate::Primitive::ALL {
            let s = self.intern(p.name())?;
            let wrapped = Object::Applicative(local::Value::Primitive(p));
            let v = local::Value::Applicative(self.heap.alloc(wrapped)?);
            self.bind_symbol(self.built_in, s, v)?;
        }
        let forms = self.read_local(LIBRARY, None, true)?;
        self.evaluate_forms(None, forms, self.built_in, false)?;
        self.install_natives()
    }

    /// Reads every form in `src`, then evaluates them in order in the top-level context.
    /// Returns the last form's value (`#inert` for no forms). Nothing is evaluated if `src`
    /// doesn't read. Host functions get `state`. [`Grift::evaluate_in`] names the source.
    pub fn evaluate(&mut self, state: &mut T, src: &str) -> Result<Value, Error> {
        let forms = self
            .prepare(std::iter::empty())
            .and_then(|()| self.read_local(src, None, false));
        let done = forms.and_then(|forms| self.evaluate_forms(Some(state), forms, self.top, false));
        self.exported(done)
    }

    /// Reads every form in `src`, the text of `source` (a file's path, say), then evaluates them
    /// in order in `context`, as `evaluate` does at top level. Each combination read keeps its
    /// line, so an error's trace says where: [`Grift::position`], [`Grift::describe`]. If `src`
    /// doesn't read, the values of `parse-error` are `(reason line column source)`, as for
    /// [`Grift::read_in`], with `source` a symbol.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let keys = g.context(Some(&g.top_level()))?;
    /// let e = g.evaluate_in(&mut (), &keys, "keys.grift", "(define! a 1)\n(first a)").unwrap_err();
    /// assert_eq!(g.position(&e.trace[0])?, Some(("keys.grift", 2)));
    /// let mut text = String::new();
    /// g.describe(&e, &mut text).unwrap();
    /// assert_eq!(text, "type-error 1 in (first a) at keys.grift:2");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn evaluate_in(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
    ) -> Result<Value, Error> {
        let done = self.run_text(state, context, source, src, false, false);
        self.exported(done)
    }

    /// [`Grift::evaluate_in`], for a host's own grift code, such as a prelude: it counts as library
    /// code, as grift's own library does, so an error's trace skips its combinations and the code
    /// they build, and lists the program's.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let top = g.top_level();
    /// g.evaluate_library_in(&mut (), &top, "prelude.grift", "(define! head (lambda! (xs) (first xs)))")?;
    /// let e = g.evaluate_in(&mut (), &top, "config.grift", "\n(head 9)").unwrap_err();
    /// let mut text = String::new();
    /// g.describe(&e, &mut text).unwrap();
    /// // The prelude's (first xs) is skipped: the error is the config's line.
    /// assert_eq!(text, "type-error 9 in (head 9) at config.grift:2");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn evaluate_library_in(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
    ) -> Result<Value, Error> {
        let done = self.run_text(state, context, source, src, true, false);
        self.exported(done)
    }

    /// Reads `src` and evaluates its forms in `context`; `library` marks it as a host's own grift
    /// code, and `parks` says whether the run may park.
    fn run_text(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
        library: bool,
        parks: bool,
    ) -> Result<local::Value, local::Error> {
        let c = self.context_of(context)?;
        self.prepare(std::iter::once(local::Value::Context(c)))?;
        let source = self.intern(source)?;
        let forms = self.read_local(src, Some(source), library)?;
        self.evaluate_forms(Some(state), forms, c, parks)
    }

    /// Calls applicative `f` with `arguments` as `apply` does: its underlying combiner gets the
    /// arguments as its operands, in a new empty context; anything else is apply's `type-error`.
    /// Host functions get `state`.
    pub fn call(&mut self, state: &mut T, f: &Value, arguments: &[Value]) -> Result<Value, Error> {
        let done = self.run_call(state, f, arguments, false);
        self.exported(done)
    }

    /// Calls `f` as [`Grift::call`] does; `parks` says whether the run may park.
    fn run_call(
        &mut self,
        state: &mut T,
        f: &Value,
        arguments: &[Value],
        parks: bool,
    ) -> Result<local::Value, local::Error> {
        let f = self.prepare_call(f, arguments)?;
        let arguments = self.imported(arguments)?;
        self.call_local(Some(state), f, &arguments, parks)
    }

    /// An applicative calling `f`, bound nowhere: the host binds it with [`Grift::define`], or
    /// hands it to grift code that keeps it private.
    pub fn host_function(&mut self, name: &str, f: HostFunction<T>) -> Result<Value, Error> {
        self.hosted(name, parked::Hosted::Plain(f))
    }

    fn hosted(&mut self, name: &str, f: parked::Hosted<T>) -> Result<Value, Error> {
        let made = self.intern(name).and_then(|s| {
            let id = u32::try_from(self.hosts.len()).map_err(|_| local::Error::oom())?;
            push(&mut self.hosts, (s, f))?;
            let host = Object::Applicative(local::Value::Host(local::Host(id)));
            Ok(local::Value::Applicative(self.heap.alloc(host)?))
        });
        self.exported(made)
    }

    /// A new empty context, `(context parent)` with `()` for none: with no parent, a record, as
    /// `error` makes; with the top level as its parent, a place for a host's own definitions.
    pub fn context(&mut self, parent: Option<&Value>) -> Result<Value, Error> {
        let made = parent
            .map(|p| self.context_of(p))
            .transpose()
            .and_then(|parent| {
                self.heap
                    .alloc(Object::Context(parent, Bindings::default()))
            });
        self.exported(made.map(local::Value::Context))
    }

    /// A new empty context whose parent is the built-in one: a private top level. Grift code
    /// evaluated there sees the primitives and the library, never what a program defines.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// g.evaluate(&mut (), "(define! list 0)")?; // a program redefines list
    /// let private = g.private_context()?;
    /// let made = g.evaluate_in(&mut (), &private, "prelude.grift", "(list 1 2)")?;
    /// assert_eq!(g.list_items(&made)?, [grift::Value::Integer(1), grift::Value::Integer(2)]);
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn private_context(&mut self) -> Result<Value, Error> {
        let c = Object::Context(Some(self.built_in), Bindings::default());
        let made = self.heap.alloc(c).map(local::Value::Context);
        self.exported(made)
    }

    /// The top-level context, where programs define things.
    pub fn top_level(&self) -> Value {
        self.export(local::Value::Context(self.top))
    }

    /// Binds `name` to `v` in the top-level context.
    pub fn define(&mut self, name: &str, v: Value) -> Result<(), Error> {
        let done = self.import(&v).and_then(|v| {
            let s = self.intern(name)?;
            self.bind_symbol(self.top, s, v)
        });
        self.out(done)
    }

    /// `(bind context parameters value)`: matches the parameter tree to the value and binds its
    /// symbols in `context` itself, as the primitive does; one name is `Value::Symbol`.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let record = g.context(None)?;
    /// let fields = g.read("(id title)")?; // a list of one form
    /// let (fields, _) = g.as_pair(&fields)?.unwrap();
    /// let title = g.string("sh")?;
    /// let values = g.list(&[grift::Value::Integer(3), title])?;
    /// g.bind(&record, &fields, values)?;
    /// let id = g.symbol("id")?;
    /// assert_eq!(g.lookup(&record, &id)?, Some(grift::Value::Integer(3)));
    /// let names: Vec<String> = g.names(&record)?.iter().map(|s| g.symbol_name(s).map(String::from)).collect::<Result<_, _>>()?;
    /// assert_eq!(names, ["id", "title"]);
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn bind(&mut self, context: &Value, parameters: &Value, value: Value) -> Result<(), Error> {
        // Every value is checked before the context's kind, as the primitive's arguments are.
        let done = self.import(context).and_then(|c| {
            let (parameters, value) = (self.import(parameters)?, self.import(&value)?);
            let c = context_kind(c)?;
            self.bind_tree(c, parameters, value)
        });
        self.out(done)
    }

    /// `(lookup context name otherwise)` without the default: the value bound to `name` in
    /// `context` or its nearest parent that binds it, `None` if none does.
    pub fn lookup(&self, context: &Value, name: &Symbol) -> Result<Option<Value>, Error> {
        let found = self.import(context).and_then(|c| {
            let name = self.import_symbol(name)?;
            self.find(context_kind(c)?, name)
        });
        self.out(found.map(|v| v.map(|v| self.export(v))))
    }

    /// `(names context)`: the symbols bound in `context` itself, sorted by name.
    pub fn names(&self, context: &Value) -> Result<Vec<Symbol>, Error> {
        let names = self.context_of(context).and_then(|c| self.names_of(c));
        let names =
            names.and_then(|names| collected(names.into_iter().map(|s| self.export_symbol(s))));
        self.out(names)
    }

    /// Interns `name`.
    pub fn symbol(&mut self, name: &str) -> Result<Symbol, Error> {
        let s = self.intern(name);
        self.out(s.map(|s| self.export_symbol(s)))
    }

    /// The name of a symbol.
    pub fn symbol_name(&self, symbol: &Symbol) -> Result<&str, Error> {
        self.out(self.import_symbol(symbol).map(|s| self.name(s)))
    }

    /// An error of kind `kind` with a copy of `values`, for a host function to return.
    pub fn error(&mut self, kind: &str, values: &[Value]) -> Error {
        let made = self.intern(kind).and_then(|kind| {
            Ok(Error {
                kind: self.export_symbol(kind),
                values: collected(values.iter().cloned())?,
                trace: Vec::new(),
                limit: false,
            })
        });
        made.unwrap_or_else(|e| self.export_error(e))
    }

    /// A new string holding a copy of `s`.
    pub fn string(&mut self, s: &str) -> Result<Value, Error> {
        let made = boxed(s).and_then(|s| self.heap.alloc(Object::String(s)));
        self.exported(made.map(local::Value::String))
    }

    /// The contents of a string value; `None` for anything else. To hold them while it calls
    /// `&mut self` methods a host copies them, as it does a bytes value's.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let path = g.string("notes.txt")?;
    /// let path = g.as_str(&path)?.map(str::to_owned); // owned: g is free again
    /// let e = g.error("file-error", &[]);
    /// assert_eq!(path.as_deref(), Some("notes.txt"));
    /// assert_eq!(g.symbol_name(&e.kind)?, "file-error");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn as_str(&self, v: &Value) -> Result<Option<&str>, Error> {
        let s = self.import(v).and_then(|v| match v {
            local::Value::String(_) => self.text(v).map(Some),
            _ => Ok(None),
        });
        self.out(s)
    }

    /// A new bytes value holding a copy of `b`.
    pub fn bytes(&mut self, b: &[u8]) -> Result<Value, Error> {
        let made = collected(b.iter().copied()).and_then(|b| self.bytes_local(b));
        self.exported(made)
    }

    /// The contents of a bytes value; `None` for anything else.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let chunk = g.bytes(&[104, 105, 255])?;
    /// g.define("chunk", chunk)?;
    /// let head = g.evaluate(&mut (), "(slice chunk 0 2)")?;
    /// assert_eq!(g.as_bytes(&head)?, Some(&b"hi"[..]));
    /// let text = g.evaluate(&mut (), "(bytes-to-string (slice chunk 0 2))")?;
    /// assert_eq!((g.as_bytes(&text)?, g.as_str(&text)?), (None, Some("hi")));
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn as_bytes(&self, v: &Value) -> Result<Option<&[u8]>, Error> {
        let b = self.import(v).and_then(|v| match v {
            local::Value::Bytes(_) => self.bytes_of(v).map(Some),
            _ => Ok(None),
        });
        self.out(b)
    }

    /// The new pair `(a . b)`.
    pub fn pair(&mut self, a: Value, b: Value) -> Result<Value, Error> {
        let made = self
            .import(&b)
            .and_then(|b| self.pairs(std::slice::from_ref(&a), b));
        self.exported(made)
    }

    /// The first and the rest of a pair; `None` for anything else.
    pub fn as_pair(&self, v: &Value) -> Result<Option<(Value, Value)>, Error> {
        let p = self.import(v).and_then(|v| match v {
            local::Value::Pair(r) => self.pair_at(r).map(Some),
            _ => Ok(None),
        });
        self.out(p.map(|p| p.map(|(a, b)| (self.export(a), self.export(b)))))
    }

    /// A new list of `values`, as `pair` builds one from its end.
    pub fn list(&mut self, values: &[Value]) -> Result<Value, Error> {
        let made = self.pairs(values, local::Value::EmptyList);
        self.exported(made)
    }

    /// `heads` in front of `tail`, built by the host: what a run a host function left behind
    /// marked is over first, so the pairs are the program's.
    fn pairs(&mut self, heads: &[Value], tail: local::Value) -> Result<local::Value, local::Error> {
        self.settled();
        heads.iter().rev().try_fold(tail, |rest, v| {
            let v = self.import(v)?;
            self.pair_local(v, rest)
        })
    }

    /// The elements of a proper list, as `as_pair` reads them; a `type-error` carrying anything else.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let keys = g.evaluate(&mut (), "(list \"C-b\" \"v\")")?;
    /// let keys: Vec<String> = g.list_items(&keys)?.iter().filter_map(|k| g.as_str(k).ok()?.map(String::from)).collect();
    /// assert_eq!(keys, ["C-b", "v"]);
    /// let dotted = g.evaluate(&mut (), "(pair 1 2)")?;
    /// let e = g.list_items(&dotted).unwrap_err();
    /// assert_eq!(g.symbol_name(&e.kind)?, "type-error");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn list_items(&self, list: &Value) -> Result<Vec<Value>, Error> {
        let items = self
            .import(list)
            .and_then(|l| self.items(l, || local::Error::new(K::TypeError, &[l])));
        let items = items.and_then(|items| collected(items.into_iter().map(|v| self.export(v))));
        self.out(items)
    }

    /// Reads every form in `src` into a list, evaluating nothing. The values of a `parse-error`
    /// are why and the line and column: of the `(` or `"` left open, or of what didn't read.
    /// Reading never collects: after `out-of-memory`, further reads may fail until an evaluation
    /// or call gives collection its next safe point.
    pub fn read(&mut self, src: &str) -> Result<Value, Error> {
        let forms = self.read_local(src, None, false);
        self.exported(forms)
    }

    /// Reads every form in `text` into a list, evaluating nothing, as [`Grift::read`] does.
    /// Each combination keeps `source` and the line of its `(` for [`Grift::position`] and
    /// traces when evaluated later, including entry through native control flow. A tail
    /// evaluation of another named outer form replaces the finished form's trace boundary;
    /// the host's entry boundary stays. No forms gives `()`.
    /// If `text` doesn't read, the values of `parse-error` are `(reason line column source)`, as
    /// for [`Grift::evaluate_in`], with `source` a symbol. Allocation failure is `out-of-memory`.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let forms = g.read_in("calculation.grift", "\n(add 1 2)")?;
    /// let (form, _) = g.as_pair(&forms)?.unwrap();
    /// assert_eq!(g.position(&form)?, Some(("calculation.grift", 2)));
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn read_in(&mut self, source: &str, text: &str) -> Result<Value, Error> {
        let forms = self
            .intern(source)
            .and_then(|source| self.read_local(text, Some(source), false));
        self.exported(forms)
    }

    /// Writes `v` as grift would read it back, where it can be read back. Combiners show their
    /// parameters and body: `#[operative (x) #ignore x]`, `#[applicative #[primitive first]]`;
    /// a context shows its own names, sorted: `#[context a b]`. A reclaimed value is
    /// `stale-value`, never text; another interpreter's is `foreign-value`. A stop that reaches a
    /// long print is `interrupted`, no memory for the printer's own stack `out-of-memory`, and an
    /// `out` that refuses the text `output-error`.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let v = g.evaluate(&mut (), "(list 1.0 \"a\\rb\" (quote! x))")?;
    /// let mut text = String::new();
    /// g.write(&v, &mut text).unwrap();
    /// assert_eq!(text, "(1.0 \"a\\rb\" x)");
    /// g.evaluate(&mut (), "(length (map list (range 0 100000)))")?; // collects v
    /// let e = g.write(&v, &mut text).unwrap_err();
    /// assert_eq!(g.symbol_name(&e.kind)?, "stale-value");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn write(&self, v: &Value, out: &mut dyn fmt::Write) -> Result<(), Error> {
        let written = self.import(v).and_then(|v| self.print(v, out));
        self.out(written)
    }

    /// Writes an error for a person: its kind and values, then each combination of its trace,
    /// with its source and line where it has one: `type-error 1 in (first a)
    /// at keys.grift:2`, then `\n  from (f) at keys.grift:3` for each one around it. It fails as
    /// [`Grift::write`] does.
    pub fn describe(&self, error: &Error, out: &mut dyn fmt::Write) -> Result<(), Error> {
        let written = self
            .import_error(error)
            .and_then(|e| self.write_error(&e, out));
        self.out(written)
    }

    fn write_error(&self, e: &local::Error, out: &mut dyn fmt::Write) -> Result<(), local::Error> {
        self.print(local::Value::Symbol(e.kind), out)?;
        for &v in &e.values {
            out.write_char(' ')?;
            self.print(v, out)?;
        }
        for (i, &at) in e.trace.iter().enumerate() {
            out.write_str(if i == 0 { " in " } else { "\n  from " })?;
            self.print(at, out)?;
            if let Some((source, line)) = self.position_local(at) {
                write!(out, " at {source}:{line}")?;
            }
        }
        Ok(())
    }

    /// The source and line of a combination read by [`Grift::read_in`] or [`Grift::evaluate_in`],
    /// such as an entry of an error's trace; `None` for any other value. A reclaimed value is
    /// `stale-value`, another interpreter's `foreign-value`.
    pub fn position(&self, combination: &Value) -> Result<Option<(&str, u32)>, Error> {
        let c = self.import(combination);
        self.out(c.map(|c| self.position_local(c)))
    }

    fn position_local(&self, combination: local::Value) -> Option<(&str, u32)> {
        let local::Value::Pair(r) = combination else {
            return None;
        };
        let position = self.positions.get(&r.idx)?;
        (position.generation == r.generation).then(|| (self.name(position.source), position.line))
    }

    /// A handle that stops one run of this interpreter from another thread: the run in progress,
    /// or, if none is, the next to begin. It never reaches another.
    pub fn interrupter(&self) -> Interrupter {
        Interrupter::new(&self.control)
    }

    /// Keeps `v` alive across evaluations, until [`Grift::release`].
    pub fn keep(&mut self, v: Value) -> Result<Kept, Error> {
        let kept = self.import(&v).and_then(|v| {
            let name = self.free_name()?;
            self.fill_kept(name, Entry::Value(v));
            Ok(name)
        });
        match kept {
            Ok(name) => Ok(Kept {
                owner: self.owner(),
                name,
            }),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// The first free entry's name, a released one first so keeping on every event never runs out
    /// of names, one made if there is none. Filling it can't fail, so a caller that may yet turn
    /// back has nothing to undo.
    pub(crate) fn free_name(&mut self) -> Result<u32, local::Error> {
        if let Some(name) = self.kept_free {
            return Ok(name);
        }
        #[cfg(test)]
        if self.kept.len() >= self.heap.kept_limit {
            return Err(local::Error::oom());
        }
        let name = u32::try_from(self.kept.len()).map_err(|_| local::Error::oom())?;
        push(&mut self.kept, Entry::Free(None))?;
        self.kept_free = Some(name);
        Ok(name)
    }

    /// Fills the entry [`Grift::free_name`] gave.
    pub(crate) fn fill_kept(&mut self, name: u32, filled: Entry) {
        if let Some(entry) = self.kept.get_mut(name as usize) {
            if let Entry::Free(next) = *entry {
                self.kept_free = next;
            }
            *entry = filled;
        }
    }

    /// Frees the entry `name`, for the next, and says what it held.
    pub(crate) fn clear_kept(&mut self, name: u32) -> Option<Entry> {
        let entry = self.kept.get_mut(name as usize)?;
        let held = std::mem::replace(entry, Entry::Free(self.kept_free));
        self.kept_free = Some(name);
        Some(held)
    }

    /// The value `k` keeps.
    pub fn kept(&self, k: &Kept) -> Result<Value, Error> {
        let found = match self.owns(&k.owner) {
            true => match self.kept.get(k.name as usize) {
                Some(Entry::Value(v)) => Ok(*v),
                _ => Err(local::Error::new(K::StaleValue, &[])),
            },
            false => Err(local::Error::new(K::ForeignValue, &[])),
        };
        self.exported(found)
    }

    /// Clears the value `k` keeps, so it can be collected. Another interpreter's `Kept` is the
    /// error `foreign-value`, and comes back with it, to be released in its own.
    pub fn release(&mut self, k: Kept) -> Result<(), (Error, Kept)> {
        if !self.owns(&k.owner) {
            return Err((
                self.export_error(local::Error::new(K::ForeignValue, &[])),
                k,
            ));
        }
        self.clear_kept(k.name);
        Ok(())
    }

    /// Interns `name`.
    pub(crate) fn intern(&mut self, name: &str) -> Result<local::Symbol, local::Error> {
        if let Some(symbol) = self.interned(name) {
            return Ok(symbol);
        }
        let id = u32::try_from(self.names.len()).map_err(|_| local::Error::oom())?;
        let (spelling, key) = (boxed(name)?, boxed(name)?);
        self.names.try_reserve(1).map_err(|_| local::Error::oom())?;
        self.symbols
            .try_reserve(1)
            .map_err(|_| local::Error::oom())?;
        // Neither table changes until both spellings and both collections have room.
        self.names.push(spelling);
        self.symbols.insert(key, id);
        Ok(local::Symbol(id))
    }

    /// The symbol named `name`, if it was ever interned.
    pub(crate) fn interned(&self, name: &str) -> Option<local::Symbol> {
        self.symbols.get(name).copied().map(local::Symbol)
    }

    /// The name of symbol `s`.
    pub(crate) fn name(&self, s: local::Symbol) -> &str {
        self.names.get(s.0 as usize).map_or("", |n| n)
    }

    /// Whether a walk inside one step, now `walked` items long, must give up for a stop. Pairs
    /// can share, so a few objects can be a tree no walk finishes; the failed step then acts on the
    /// stop. A walk shorter than [`WALK_CHECK`] items never asks, so it can't be cut.
    #[inline(always)]
    pub(crate) fn walk_stopped(&self, walked: &mut usize) -> bool {
        *walked = walked.wrapping_add(1);
        *walked & (WALK_CHECK - 1) == 0 && self.control.asked()
    }

    /// Starts an outer run, unless one is in progress; whether it did, which `Control::end` then
    /// ends. What a host function left behind when it panicked or replaced the interpreter is
    /// over: its machines go here.
    pub(crate) fn begin_run(&mut self) -> bool {
        let outer = self.settled();
        if outer {
            self.control.begin();
        }
        outer
    }

    /// Whether no run is in progress; if none is, what a host function left on the machines when
    /// it panicked or replaced the interpreter is over, so nothing the host makes now takes its
    /// mark. A machine is empty between runs, and none is suspended.
    pub(crate) fn settled(&mut self) -> bool {
        let none = !self.control.running();
        if none {
            self.machine.clear();
            self.suspended.clear();
        }
        none
    }

    /// The context `v` is, or a `type-error`.
    fn context_of(&self, v: &Value) -> Result<heap::Reference, local::Error> {
        context_kind(self.import(v)?)
    }

    /// What a value handed out holds: this interpreter's identity.
    fn owner(&self) -> Owner {
        Owner(Arc::downgrade(&self.control))
    }

    fn owns(&self, owner: &Owner) -> bool {
        std::ptr::eq(owner.0.as_ptr(), Arc::as_ptr(&self.control))
    }

    /// `v` as this interpreter's own: `foreign-value` if another made it, `stale-value` if it was
    /// reclaimed. Every value the host hands in comes through here, so nothing stale or foreign
    /// is ever stored, and a live object holds only live objects.
    pub(crate) fn import(&self, v: &Value) -> Result<local::Value, local::Error> {
        use local::Value as L;
        // The object must be live and of the kind the variant says: a reference put in another
        // variant is no value of this interpreter's.
        let live = |r: &Reference, kind: fn(&Object) -> bool| match self.owns(&r.owner) {
            true => match self.heap.get(r.local)? {
                o if kind(o) => Ok(r.local),
                _ => Err(local::Error::new(K::StaleValue, &[])),
            },
            false => Err(local::Error::new(K::ForeignValue, &[])),
        };
        Ok(match v {
            Value::EmptyList => L::EmptyList,
            Value::Boolean(b) => L::Boolean(*b),
            Value::Inert => L::Inert,
            Value::Ignore => L::Ignore,
            Value::Integer(n) => L::Integer(*n),
            Value::Float(f) => L::Float(*f),
            Value::Symbol(s) => L::Symbol(self.import_symbol(s)?),
            Value::Pair(r) => L::Pair(live(r, |o| matches!(o, Object::Pair(..)))?),
            Value::String(r) => L::String(live(r, |o| matches!(o, Object::String(_)))?),
            Value::Bytes(r) => L::Bytes(live(r, |o| matches!(o, Object::Bytes(_)))?),
            Value::Context(r) => L::Context(live(r, |o| matches!(o, Object::Context(..)))?),
            Value::Operative(Operative(kind)) => match kind {
                OperativeKind::Compound(r) => {
                    L::Operative(live(r, |o| matches!(o, Object::Operative(_)))?)
                }
                OperativeKind::Primitive(p) => L::Primitive(*p),
                OperativeKind::Host(owner, index) if self.owns(owner) => {
                    L::Host(local::Host(*index))
                }
                OperativeKind::Host(..) => return Err(local::Error::new(K::ForeignValue, &[])),
            },
            Value::Applicative(r) => {
                L::Applicative(live(r, |o| matches!(o, Object::Applicative(_)))?)
            }
        })
    }

    fn import_symbol(&self, s: &Symbol) -> Result<local::Symbol, local::Error> {
        match self.owns(&s.owner) {
            true => Ok(local::Symbol(s.index)),
            false => Err(local::Error::new(K::ForeignValue, &[])),
        }
    }

    /// An error a host function returned, as this interpreter's.
    pub(crate) fn import_error(&self, e: &Error) -> Result<local::Error, local::Error> {
        let (values, trace) = (self.imported(&e.values)?, self.imported(&e.trace)?);
        let kind = self.import_symbol(&e.kind)?;
        let imported = local::Error::from_parts(kind, values, trace)?;
        Ok(local::Error {
            limit: e.limit,
            ..imported
        })
    }

    /// An error the host hands in, as the reason a run unwinds. One that can't come in, its values
    /// reclaimed or another interpreter's, is that error instead, and stays a limit if it was one.
    pub(crate) fn incoming_error(&self, e: &Error) -> local::Error {
        self.import_error(e).unwrap_or_else(|mut failed| {
            failed.limit |= e.limit;
            failed
        })
    }

    /// Validate and root a host call before its first allocation. Return before running it, so
    /// validation and root-iterator temporaries do not stay on a suspended host's stack.
    fn prepare_call(
        &mut self,
        f: &Value,
        arguments: &[Value],
    ) -> Result<local::Value, local::Error> {
        let f = self.import(f)?;
        arguments
            .iter()
            .try_for_each(|v| self.import(v).map(drop))?;
        // Validation allocates nothing. Root every checked reference before the argument
        // buffer needs memory; marking follows the reference, independent of its value kind.
        let roots = arguments.iter().filter_map(|v| match v {
            Value::Pair(r)
            | Value::String(r)
            | Value::Bytes(r)
            | Value::Context(r)
            | Value::Operative(Operative(OperativeKind::Compound(r)))
            | Value::Applicative(r) => Some(local::Value::Pair(r.local)),
            _ => None,
        });
        self.prepare(std::iter::once(f).chain(roots))?;
        Ok(f)
    }

    fn imported(&self, values: &[Value]) -> Result<Vec<local::Value>, local::Error> {
        let mut out = collected(std::iter::repeat_n(local::Value::Inert, values.len()))?;
        for (to, v) in out.iter_mut().zip(values) {
            *to = self.import(v)?;
        }
        Ok(out)
    }

    pub(crate) fn export(&self, v: local::Value) -> Value {
        use local::Value as L;
        let reference = |local| Reference {
            owner: self.owner(),
            local,
        };
        let operative = |kind| Value::Operative(Operative(kind));
        match v {
            L::EmptyList => Value::EmptyList,
            L::Boolean(b) => Value::Boolean(b),
            L::Inert => Value::Inert,
            L::Ignore => Value::Ignore,
            L::Integer(n) => Value::Integer(n),
            L::Float(f) => Value::Float(f),
            L::Primitive(p) => operative(OperativeKind::Primitive(p)),
            L::Symbol(s) => Value::Symbol(self.export_symbol(s)),
            L::Pair(r) => Value::Pair(reference(r)),
            L::String(r) => Value::String(reference(r)),
            L::Bytes(r) => Value::Bytes(reference(r)),
            L::Context(r) => Value::Context(reference(r)),
            L::Operative(r) => operative(OperativeKind::Compound(reference(r))),
            L::Applicative(r) => Value::Applicative(reference(r)),
            L::Host(h) => operative(OperativeKind::Host(self.owner(), h.0)),
        }
    }

    fn export_symbol(&self, s: local::Symbol) -> Symbol {
        Symbol {
            owner: self.owner(),
            index: s.0,
        }
    }

    /// An error as the host's. Memory refused here, not on its way through a run, asks for its
    /// collection now: the host's next try has that, as a run's cleanups do.
    pub(crate) fn export_error(&self, e: local::Error) -> Error {
        if e.refused {
            self.heap.request_collection();
        }
        let exported = |vs: &[local::Value]| collected(vs.iter().map(|&v| self.export(v)));
        match (exported(&e.values), exported(&e.trace)) {
            (Ok(values), Ok(trace)) => Error {
                kind: self.export_symbol(e.kind),
                values,
                trace,
                limit: e.limit,
            },
            _ => self.export_error(local::Error::oom()),
        }
    }

    fn exported(&self, r: Result<local::Value, local::Error>) -> Result<Value, Error> {
        self.out(r.map(|v| self.export(v)))
    }

    fn out<R>(&self, r: Result<R, local::Error>) -> Result<R, Error> {
        r.map_err(|e| self.export_error(e))
    }
}

/// The context `v` is, or a `type-error`.
fn context_kind(v: local::Value) -> Result<heap::Reference, local::Error> {
    match v {
        local::Value::Context(c) => Ok(c),
        v => Err(evaluate::type_error(v)),
    }
}

/// The items in a new vector, reporting `out-of-memory` instead of aborting.
fn collected<X>(items: impl ExactSizeIterator<Item = X>) -> Result<Vec<X>, local::Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(items.len())
        .map_err(|_| local::Error::oom())?;
    v.extend(items);
    Ok(v)
}
