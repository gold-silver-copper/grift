#![doc = include_str!("../../../README.md")]

mod evaluate;
mod heap;
mod interrupt;
mod local;
mod print;
mod read;
#[cfg(test)]
mod tests;

use heap::{Bindings, Heap, IdHasher, Object};
use interrupt::Control;
use local::{K, KEPT, boxed, push};
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::{Arc, Weak};

pub use evaluate::Primitive;
pub use interrupt::Interrupter;

/// The library, evaluated in the built-in context by [`Grift::new`].
const LIBRARY: &str = include_str!("library.grift");

/// A grift value. Immediate values are plain data; the rest belong to the [`Grift`] that made
/// them, and every operation checks that: another interpreter's value is the error
/// `foreign-value`, never read as one of this one's.
///
/// A value returned to the host stays valid until the next call to [`Grift::evaluate`] or
/// [`Grift::call`]; the collector may then reclaim it, and using it is the error `stale-value`,
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
    /// A context.
    Context(Reference),
    /// A compound operative made by `vau!`.
    Operative(Reference),
    /// An applicative: a wrapped combiner.
    Applicative(Reference),
    /// A Rust primitive, as an operative (the library binds most of them wrapped).
    Primitive(Primitive),
    /// A host function, as an operative ([`Grift::host_function`] wraps it).
    Host(Host),
}

/// An interned symbol of one [`Grift`]; its name is [`Grift::symbol_name`].
#[derive(Clone, Debug)]
pub struct Symbol {
    owner: Weak<Control>,
    index: u32,
}

/// An object in the heap of one [`Grift`]: a slot and its generation.
#[derive(Clone, Debug)]
pub struct Reference {
    owner: Weak<Control>,
    local: heap::Reference,
}

/// A host function registered with one [`Grift`].
#[derive(Clone, Debug)]
pub struct Host {
    owner: Weak<Control>,
    index: u32,
}

/// Interpreter identities are the same allocation, so values from two interpreters are never
/// equal, even after one is dropped: a value keeps the allocation's address from being reused.
fn same_owner(a: &Weak<Control>, b: &Weak<Control>) -> bool {
    Weak::ptr_eq(a, b)
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Symbol) -> bool {
        self.index == other.index && same_owner(&self.owner, &other.owner)
    }
}

impl Eq for Symbol {}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl PartialEq for Reference {
    fn eq(&self, other: &Reference) -> bool {
        self.local == other.local && same_owner(&self.owner, &other.owner)
    }
}

impl PartialEq for Host {
    fn eq(&self, other: &Host) -> bool {
        self.index == other.index && same_owner(&self.owner, &other.owner)
    }
}

/// A host function. It receives the interpreter, the host's state (the one passed to
/// [`Grift::evaluate`] or [`Grift::call`]) and the evaluated arguments.
pub type HostFunction<T> = fn(&mut Grift<T>, &mut T, &[Value]) -> Result<Value, Error>;

/// What went wrong: a symbol naming the kind of error, the values involved, and where.
/// [`Grift::describe`] writes it for a person.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    /// The kind, such as `type-error` or a kind a program gave `error`; [`Grift::is_kind`] asks.
    pub kind: Symbol,
    /// The values involved, valid as long as a returned [`Value`].
    pub values: Vec<Value>,
    /// Where, read with [`Error::trace`].
    trace: Vec<Value>,
}

impl Error {
    /// Where: the combinations of the program that were being evaluated, innermost first, each
    /// once, ending with the top-level form. The library's own code is skipped, so `(if! 1 2 3)`
    /// fails at `(if! 1 2 3)`; a call in tail position has finished, so it isn't listed.
    /// [`Grift::position`] gives each one's source and line.
    pub fn trace(&self) -> &[Value] {
        &self.trace
    }
}

/// Why [`Grift::write`] or [`Grift::describe`] stopped.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteError {
    /// A value can't be written: `stale-value`, `foreign-value`, or `out-of-memory` when the
    /// printer's own stack can't grow.
    Value(Error),
    /// The output refused the text.
    Output,
}

/// A value the host keeps across evaluations: a binding in a context the interpreter owns, under a
/// name no program can make. Read it with [`Grift::kept`]; [`Grift::release`] unbinds it. Neither
/// `Copy` nor `Clone`, so it is released at most once and never used after; one dropped without
/// a release keeps its value until the interpreter is dropped (there are no finalizers).
///
/// ```
/// let mut g: grift::Grift = grift::Grift::new()?;
/// let handler = g.evaluate(&mut (), "(lambda! (key) (string-append \"got \" key))")?;
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
    owner: Weak<Control>,
    name: u32,
}

/// An interpreter: a heap, the built-in context holding the primitives and the library, and the
/// top-level context, a child of the built-in one, where programs define things. `T` is the
/// host's state, which the host lends to each [`Grift::evaluate`] and [`Grift::call`] and which
/// they pass on to host functions.
pub struct Grift<T = ()> {
    heap: Heap,
    names: Vec<Box<str>>,
    /// Symbol ids sorted by name, for interning.
    sorted: Vec<u32>,
    hosts: Vec<(local::Symbol, HostFunction<T>)>,
    built_in: heap::Reference,
    top: heap::Reference,
    /// The kept values' context, a root no program reaches, and the names free for the next.
    kept: heap::Reference,
    kept_free: Vec<u32>,
    kept_next: u32,
    machine: evaluate::Machine,
    /// Shared with the interrupters; also this interpreter's identity.
    control: Arc<Control>,
    /// The generation of the outer run in progress, 0 for none, and the last one given out.
    run: u64,
    runs: u64,
    /// Whether the run in progress was interrupted already: it is once, and its cleanups run.
    stopped: bool,
    /// Where each combination read under a name is: its slot's index -> (the slot's generation,
    /// the source's name, the line of its `(`).
    positions: HashMap<u32, (u32, local::Symbol, u32), BuildHasherDefault<IdHasher>>,
    /// Off in tests: native primitives and library forms then run their grift definitions.
    #[cfg(test)]
    pub(crate) natives: bool,
    #[cfg(test)]
    pub(crate) definitions: heap::Reference,
}

impl<T> Grift<T> {
    /// Builds the built-in context, evaluates the library in it and makes an empty top level.
    pub fn new() -> Result<Grift<T>, Error> {
        let mut g = Grift {
            heap: Heap::default(),
            names: Vec::new(),
            sorted: Vec::new(),
            hosts: Vec::new(),
            built_in: heap::Reference::NONE,
            top: heap::Reference::NONE,
            kept: heap::Reference::NONE,
            kept_free: Vec::new(),
            kept_next: 0,
            machine: evaluate::Machine::default(),
            control: Arc::new(Control::default()),
            run: 0,
            runs: 0,
            stopped: false,
            positions: HashMap::default(),
            #[cfg(test)]
            natives: true,
            #[cfg(test)]
            definitions: heap::Reference::NONE,
        };
        match g.load() {
            Ok(()) => Ok(g),
            Err(e) => Err(g.export_error(e)),
        }
    }

    fn load(&mut self) -> Result<(), local::Error> {
        let context =
            |heap: &mut Heap, parent| heap.alloc(Object::Context(parent, Bindings::default()));
        self.built_in = context(&mut self.heap, None)?;
        self.top = context(&mut self.heap, Some(self.built_in))?;
        self.kept = context(&mut self.heap, None)?;
        for name in local::K_NAMES {
            self.intern(name)?;
        }
        for &p in Primitive::ALL {
            let s = self.intern(p.name())?;
            let v = match p {
                Primitive::Vau => local::Value::Primitive(p),
                _ => local::Value::Applicative(
                    self.heap
                        .alloc(Object::Applicative(local::Value::Primitive(p)))?,
                ),
            };
            self.bind_symbol(self.built_in, s, v)?;
        }
        let forms = self.read_local(LIBRARY, None)?;
        self.evaluate_forms(None, forms, self.built_in)?;
        self.install_natives()?;
        #[cfg(test)]
        self.load_definitions()?;
        // Everything in the heap now is the library's; errors skip its code.
        self.heap.seal_library();
        Ok(())
    }

    /// Reads every form in `src`, then evaluates them in order in the top-level context.
    /// Returns the last form's value (`#inert` for no forms). Nothing is evaluated if `src`
    /// doesn't read. Host functions get `state`. [`Grift::evaluate_in`] names the source.
    pub fn evaluate(&mut self, state: &mut T, src: &str) -> Result<Value, Error> {
        let forms = self.read_local(src, None);
        let done = forms.and_then(|forms| self.evaluate_forms(Some(state), forms, self.top));
        self.exported(done)
    }

    /// Reads every form in `src`, the text of `source` (a file's path, say), then evaluates them
    /// in order in `context`, as `evaluate` does at top level. Each combination read keeps its
    /// line, so an error's trace says where: [`Grift::position`], [`Grift::describe`].
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let keys = g.context(Some(&g.top_level()))?;
    /// let e = g.evaluate_in(&mut (), &keys, "keys.grift", "(define! a 1)\n(first a)").unwrap_err();
    /// assert_eq!(g.position(&e.trace()[0]), Some(("keys.grift", 2)));
    /// let mut text = String::new();
    /// g.describe(&e, &mut text).unwrap();
    /// assert_eq!(text, "type-error #[primitive first] 1 in (first a) at keys.grift:2");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn evaluate_in(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
    ) -> Result<Value, Error> {
        let done = self.context_of(context, Primitive::Evaluate).and_then(|c| {
            let source = self.intern(source)?;
            let forms = self.read_local(src, Some(source))?;
            self.evaluate_forms(Some(state), forms, c)
        });
        self.exported(done)
    }

    /// Calls combiner `f` with `arguments` as Kernel's `apply` does: an applicative's
    /// underlying combiner gets the arguments as its operands, in a new empty context; an
    /// operative gets them as they are. Host functions get `state`.
    pub fn call(&mut self, state: &mut T, f: &Value, arguments: &[Value]) -> Result<Value, Error> {
        let done = self.import(f).and_then(|f| {
            let mut local = Vec::new();
            local
                .try_reserve_exact(arguments.len())
                .map_err(|_| local::Error::oom())?;
            for a in arguments {
                local.push(self.import(a)?);
            }
            self.call_local(Some(state), f, &local)
        });
        self.exported(done)
    }

    /// An applicative calling `f`, bound nowhere: the host binds it with [`Grift::define`], or
    /// hands it to grift code that keeps it private.
    pub fn host_function(&mut self, name: &str, f: HostFunction<T>) -> Result<Value, Error> {
        let made = self.intern(name).and_then(|s| {
            let id = u32::try_from(self.hosts.len()).map_err(|_| local::Error::oom())?;
            push(&mut self.hosts, (s, f))?;
            let host = Object::Applicative(local::Value::Host(local::Host(id)));
            Ok(local::Value::Applicative(self.heap.alloc(host)?))
        });
        self.exported(made)
    }

    /// A new empty context, `(context)` or `(context parent)`: with no parent, a record, as
    /// `error` makes; with the top level as its parent, a place for a host's own definitions.
    pub fn context(&mut self, parent: Option<&Value>) -> Result<Value, Error> {
        let made = parent
            .map(|p| self.context_of(p, Primitive::Context))
            .transpose()
            .and_then(|parent| {
                self.heap
                    .alloc(Object::Context(parent, Bindings::default()))
            });
        self.exported(made.map(local::Value::Context))
    }

    /// A new empty context whose parent is the built-in one: a private top level. Grift code
    /// evaluated there sees the primitives and the library, never what a program defines.
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
        self.exported(done.map(|()| local::Value::Inert)).map(drop)
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
        let done = self.context_of(context, Primitive::Bind).and_then(|c| {
            let (parameters, value) = (self.import(parameters)?, self.import(&value)?);
            self.bind_tree(c, parameters, value)
        });
        self.exported(done.map(|()| local::Value::Inert)).map(drop)
    }

    /// `(lookup context name otherwise)` without the default: the value bound to `name` in
    /// `context` or its nearest parent that binds it, `None` if none does.
    pub fn lookup(&self, context: &Value, name: &Symbol) -> Result<Option<Value>, Error> {
        let found = self
            .context_of(context, Primitive::Lookup)
            .and_then(|c| self.find(c, self.import_symbol(name)?));
        match found {
            Ok(v) => Ok(v.map(|v| self.export(v))),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// `(names context)`: the symbols bound in `context` itself, sorted by name.
    pub fn names(&self, context: &Value) -> Result<Vec<Symbol>, Error> {
        let names = self
            .context_of(context, Primitive::Names)
            .and_then(|c| self.names_of(c));
        let names = names.and_then(|names| {
            let mut out = Vec::new();
            out.try_reserve_exact(names.len())
                .map_err(|_| local::Error::oom())?;
            out.extend(names.into_iter().filter_map(|n| match n {
                local::Value::Symbol(s) => Some(self.export_symbol(s)),
                _ => None,
            }));
            Ok(out)
        });
        names.map_err(|e| self.export_error(e))
    }

    /// From now on everything in the heap counts as library code, as grift's own library does:
    /// an error's trace skips its combinations and the code it built. A host calls this after
    /// evaluating its prelude and before any user code; data live at that moment, if later
    /// evaluated as code, is skipped too.
    pub fn seal_library(&mut self) {
        self.heap.seal_library();
    }

    /// Interns `name`.
    pub fn symbol(&mut self, name: &str) -> Result<Symbol, Error> {
        match self.intern(name) {
            Ok(s) => Ok(self.export_symbol(s)),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// The name of a symbol.
    pub fn symbol_name(&self, symbol: &Symbol) -> Result<&str, Error> {
        match self.import_symbol(symbol) {
            Ok(s) => Ok(self.name(s)),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// An error of kind `kind` with a copy of `values`, for a host function to return.
    pub fn error(&mut self, kind: &str, values: &[Value]) -> Error {
        let kind = match self.symbol(kind) {
            Ok(kind) => kind,
            Err(e) => return e,
        };
        let mut copy = Vec::new();
        if copy.try_reserve_exact(values.len()).is_err() {
            return self.export_error(local::Error::oom());
        }
        copy.extend_from_slice(values);
        Error {
            kind,
            values: copy,
            trace: Vec::new(),
        }
    }

    /// Whether `error` is of the kind named `name`.
    pub fn is_kind(&self, error: &Error, name: &str) -> Result<bool, Error> {
        match self.import_symbol(&error.kind) {
            // A name never interned is no error's kind.
            Ok(kind) => Ok(self.interned(name) == Some(kind)),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// A new string holding a copy of `s`.
    pub fn string(&mut self, s: &str) -> Result<Value, Error> {
        let made = boxed(s).and_then(|s| self.heap.alloc(Object::String(s)));
        self.exported(made.map(local::Value::String))
    }

    /// The contents of a string value; `None` for anything else.
    pub fn as_str(&self, v: &Value) -> Result<Option<&str>, Error> {
        match self.import(v) {
            Ok(local::Value::String(r)) => match self.heap.get(r) {
                Ok(Object::String(s)) => Ok(Some(s)),
                _ => Err(self.export_error(local::Error::new(K::StaleValue, &[]))),
            },
            Ok(_) => Ok(None),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// A copy of a string value's contents, which the host can hold while it calls `&mut self`
    /// methods; `None` for anything else.
    pub fn as_string(&self, v: &Value) -> Result<Option<String>, Error> {
        let Some(s) = self.as_str(v)? else {
            return Ok(None);
        };
        let mut copy = String::new();
        match copy.try_reserve_exact(s.len()) {
            Ok(()) => {
                copy.push_str(s);
                Ok(Some(copy))
            }
            Err(_) => Err(self.export_error(local::Error::oom())),
        }
    }

    /// The new pair `(a . b)`.
    pub fn pair(&mut self, a: Value, b: Value) -> Result<Value, Error> {
        let made = self
            .import(&a)
            .and_then(|a| Ok((a, self.import(&b)?)))
            .and_then(|(a, b)| self.pair_local(a, b));
        self.exported(made)
    }

    /// The first and the rest of a pair; `None` for anything else.
    pub fn as_pair(&self, v: &Value) -> Result<Option<(Value, Value)>, Error> {
        match self.import(v) {
            Ok(local::Value::Pair(r)) => match self.pair_at(r) {
                Ok((a, b)) => Ok(Some((self.export(a), self.export(b)))),
                Err(e) => Err(self.export_error(e)),
            },
            Ok(_) => Ok(None),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// A new list of `values`.
    pub fn list(&mut self, values: &[Value]) -> Result<Value, Error> {
        let made = values
            .iter()
            .rev()
            .try_fold(local::Value::EmptyList, |list, v| {
                let v = self.import(v)?;
                self.pair_local(v, list)
            });
        self.exported(made)
    }

    /// The elements of a proper list; a `type-error` carrying anything else.
    pub fn list_items(&self, list: &Value) -> Result<Vec<Value>, Error> {
        let items = self
            .import(list)
            .and_then(|l| self.items(l, || local::Error::new(K::TypeError, &[l])));
        let items = items.and_then(|items| {
            let mut out = Vec::new();
            out.try_reserve_exact(items.len())
                .map_err(|_| local::Error::oom())?;
            out.extend(items.into_iter().map(|v| self.export(v)));
            Ok(out)
        });
        items.map_err(|e| self.export_error(e))
    }

    /// Reads every form in `src` into a list, evaluating nothing. A `parse-error` carries why and
    /// the line and column: of the `(` or `"` left open, or of what didn't read.
    pub fn read(&mut self, src: &str) -> Result<Value, Error> {
        let forms = self.read_local(src, None);
        self.exported(forms)
    }

    /// Writes `v` as grift would read it back, where it can be read back. Combiners show their
    /// parameters and body: `#[operative (x) #ignore x]`, `#[applicative #[primitive first]]`;
    /// a context shows its own names, sorted: `#[context a b]`.
    pub fn write(&self, v: &Value, out: &mut dyn fmt::Write) -> Result<(), WriteError> {
        let v = self
            .import(v)
            .map_err(|e| WriteError::Value(self.export_error(e)))?;
        let mut out = Watched {
            out,
            refused: false,
        };
        self.print(v, &mut out)
            .map_err(|_| self.write_failed(out.refused))
    }

    fn write_failed(&self, refused: bool) -> WriteError {
        match refused {
            true => WriteError::Output,
            // The printer stops by itself only when its own stack can't grow.
            false => WriteError::Value(self.export_error(local::Error::oom())),
        }
    }

    /// Writes an error for a person: its kind and values, then each combination of its trace,
    /// with its source and line where it has one: `type-error #[primitive first] 1 in (first a)
    /// at keys.grift:2`, then `\n  from (f) at keys.grift:3` for each one around it.
    pub fn describe(&self, error: &Error, out: &mut dyn fmt::Write) -> Result<(), WriteError> {
        let e = self
            .import_error(error)
            .map_err(|e| WriteError::Value(self.export_error(e)))?;
        let mut out = Watched {
            out,
            refused: false,
        };
        self.write_error(&e, &mut out)
            .map_err(|_| self.write_failed(out.refused))
    }

    fn write_error(&self, e: &local::Error, out: &mut dyn fmt::Write) -> fmt::Result {
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

    /// The source and line of a combination read by [`Grift::evaluate_in`], such as an entry of an
    /// error's trace; `None` for anything else.
    pub fn position(&self, combination: &Value) -> Option<(&str, u32)> {
        self.import(combination)
            .ok()
            .and_then(|c| self.position_local(c))
    }

    fn position_local(&self, combination: local::Value) -> Option<(&str, u32)> {
        let local::Value::Pair(r) = combination else {
            return None;
        };
        let &(generation, source, line) = self.positions.get(&r.idx)?;
        (generation == r.generation).then(|| (self.name(source), line))
    }

    /// A handle that stops this interpreter's run in progress, from another thread.
    pub fn interrupter(&self) -> Interrupter {
        Interrupter::new(&self.control)
    }

    /// Keeps `v` alive across evaluations, until [`Grift::release`].
    pub fn keep(&mut self, v: Value) -> Result<Kept, Error> {
        let kept = self.import(&v).and_then(|v| {
            // A released name first, so keeping on every event never runs out of names.
            let name = match self.kept_free.last() {
                Some(&name) => name,
                None if self.kept_next < KEPT => KEPT | self.kept_next,
                None => return Err(local::Error::oom()),
            };
            self.bind_symbol(self.kept, local::Symbol(name), v)?;
            if self.kept_free.pop().is_none() {
                self.kept_next += 1;
            }
            Ok(name)
        });
        match kept {
            Ok(name) => Ok(Kept {
                owner: Arc::downgrade(&self.control),
                name,
            }),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// The value `k` keeps.
    pub fn kept(&self, k: &Kept) -> Result<Value, Error> {
        let found = match self.owns(&k.owner) {
            true => self.find(self.kept, local::Symbol(k.name)),
            false => Err(local::Error::new(K::ForeignValue, &[])),
        };
        match found {
            Ok(Some(v)) => Ok(self.export(v)),
            Ok(None) => Err(self.export_error(local::Error::new(K::StaleValue, &[]))),
            Err(e) => Err(self.export_error(e)),
        }
    }

    /// Unbinds the value `k` keeps, so it can be collected. Another interpreter's `Kept` is the
    /// error `foreign-value`, and comes back with it, to be released in its own.
    pub fn release(&mut self, k: Kept) -> Result<(), (Error, Kept)> {
        if !self.owns(&k.owner) {
            return Err((
                self.export_error(local::Error::new(K::ForeignValue, &[])),
                k,
            ));
        }
        if self.kept_free.try_reserve(1).is_err() {
            return Err((self.export_error(local::Error::oom()), k));
        }
        if let Ok(bindings) = self.bindings_mut(self.kept) {
            bindings.remove(&local::Symbol(k.name));
        }
        self.kept_free.push(k.name);
        Ok(())
    }

    /// Interns `name`. The ids from [`KEPT`] up are the kept values' names, so no text gets one.
    pub(crate) fn intern(&mut self, name: &str) -> Result<local::Symbol, local::Error> {
        let names = &self.names;
        let at = self.sorted.binary_search_by(|&i| {
            names
                .get(i as usize)
                .map(|n| (**n).cmp(name))
                .unwrap_or(std::cmp::Ordering::Less)
        });
        match at {
            Ok(i) => Ok(local::Symbol(
                self.sorted.get(i).copied().unwrap_or_default(),
            )),
            Err(i) => {
                let id = u32::try_from(self.names.len())
                    .ok()
                    .filter(|&id| id < KEPT)
                    .ok_or_else(local::Error::oom)?;
                push(&mut self.names, boxed(name)?)?;
                self.sorted
                    .try_reserve(1)
                    .map_err(|_| local::Error::oom())?;
                self.sorted.insert(i, id);
                Ok(local::Symbol(id))
            }
        }
    }

    /// The symbol named `name`, if it was ever interned.
    pub(crate) fn interned(&self, name: &str) -> Option<local::Symbol> {
        let at = self
            .sorted
            .binary_search_by(|&i| self.name(local::Symbol(i)).cmp(name));
        at.ok()
            .and_then(|i| self.sorted.get(i))
            .map(|&i| local::Symbol(i))
    }

    /// The name of symbol `s`.
    pub(crate) fn name(&self, s: local::Symbol) -> &str {
        self.names.get(s.0 as usize).map_or("", |n| n)
    }

    /// Whether the run in progress was asked to stop; it stops once.
    #[inline(always)]
    pub(crate) fn interrupted(&mut self) -> bool {
        !self.stopped && self.control.stopped(self.run) && {
            self.stopped = true;
            true
        }
    }

    /// Starts an outer run, unless one is in progress: the next generation, published for
    /// interrupters. Whether it started one, which [`Grift::end_run`] then ends.
    pub(crate) fn begin_run(&mut self) -> Result<bool, local::Error> {
        if self.run != 0 {
            return Ok(false);
        }
        // Generations never repeat, so a late stop can't match a later run.
        self.runs = self
            .runs
            .checked_add(1)
            .ok_or_else(|| local::Error::new(K::Overflow, &[]))?;
        (self.run, self.stopped) = (self.runs, false);
        self.control.begin(self.run);
        Ok(true)
    }

    pub(crate) fn end_run(&mut self) {
        self.control.end();
        self.run = 0;
    }

    /// The context `v` is, or a `type-error` naming `p`.
    fn context_of(&self, v: &Value, p: Primitive) -> Result<heap::Reference, local::Error> {
        match self.import(v)? {
            local::Value::Context(c) => Ok(c),
            v => Err(evaluate::type_error(p, v)),
        }
    }

    fn owns(&self, owner: &Weak<Control>) -> bool {
        std::ptr::eq(owner.as_ptr(), Arc::as_ptr(&self.control))
    }

    /// `v` as this interpreter's own: `foreign-value` if another made it, `stale-value` if it was
    /// reclaimed. Every value the host hands in comes through here, so nothing stale or foreign
    /// is ever stored, and a live object holds only live objects.
    pub(crate) fn import(&self, v: &Value) -> Result<local::Value, local::Error> {
        use local::Value as L;
        let live = |r: &Reference| match self.owns(&r.owner) {
            true => self.heap.get(r.local).map(|_| r.local),
            false => Err(local::Error::new(K::ForeignValue, &[])),
        };
        Ok(match v {
            Value::EmptyList => L::EmptyList,
            Value::Boolean(b) => L::Boolean(*b),
            Value::Inert => L::Inert,
            Value::Ignore => L::Ignore,
            Value::Integer(n) => L::Integer(*n),
            Value::Float(f) => L::Float(*f),
            Value::Primitive(p) => L::Primitive(*p),
            Value::Symbol(s) => L::Symbol(self.import_symbol(s)?),
            Value::Pair(r) => L::Pair(live(r)?),
            Value::String(r) => L::String(live(r)?),
            Value::Context(r) => L::Context(live(r)?),
            Value::Operative(r) => L::Operative(live(r)?),
            Value::Applicative(r) => L::Applicative(live(r)?),
            Value::Host(h) if self.owns(&h.owner) => L::Host(local::Host(h.index)),
            Value::Host(_) => return Err(local::Error::new(K::ForeignValue, &[])),
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
        let mut values = Vec::new();
        let mut trace = Vec::new();
        let mut seen = heap::Seen::default();
        values
            .try_reserve_exact(e.values.len())
            .map_err(|_| local::Error::oom())?;
        trace
            .try_reserve_exact(e.trace.len())
            .map_err(|_| local::Error::oom())?;
        seen.try_reserve(e.trace.len())
            .map_err(|_| local::Error::oom())?;
        for v in &e.values {
            values.push(self.import(v)?);
        }
        for v in &e.trace {
            let v = self.import(v)?;
            if let local::Value::Pair(r) = v {
                seen.insert(r);
            }
            trace.push(v);
        }
        Ok(local::Error {
            kind: self.import_symbol(&e.kind)?,
            values,
            trace,
            seen,
        })
    }

    pub(crate) fn export(&self, v: local::Value) -> Value {
        use local::Value as L;
        let reference = |local| Reference {
            owner: Arc::downgrade(&self.control),
            local,
        };
        match v {
            L::EmptyList => Value::EmptyList,
            L::Boolean(b) => Value::Boolean(b),
            L::Inert => Value::Inert,
            L::Ignore => Value::Ignore,
            L::Integer(n) => Value::Integer(n),
            L::Float(f) => Value::Float(f),
            L::Primitive(p) => Value::Primitive(p),
            L::Symbol(s) => Value::Symbol(self.export_symbol(s)),
            L::Pair(r) => Value::Pair(reference(r)),
            L::String(r) => Value::String(reference(r)),
            L::Context(r) => Value::Context(reference(r)),
            L::Operative(r) => Value::Operative(reference(r)),
            L::Applicative(r) => Value::Applicative(reference(r)),
            L::Host(h) => Value::Host(Host {
                owner: Arc::downgrade(&self.control),
                index: h.0,
            }),
        }
    }

    fn export_symbol(&self, s: local::Symbol) -> Symbol {
        Symbol {
            owner: Arc::downgrade(&self.control),
            index: s.0,
        }
    }

    pub(crate) fn export_error(&self, e: local::Error) -> Error {
        let mut values = Vec::new();
        let mut trace = Vec::new();
        let (kind, all) = match (
            values.try_reserve_exact(e.values.len()),
            trace.try_reserve_exact(e.trace.len()),
        ) {
            (Ok(()), Ok(())) => (e.kind, true),
            _ => (K::OutOfMemory.into(), false),
        };
        if all {
            values.extend(e.values.iter().map(|&v| self.export(v)));
            trace.extend(e.trace.iter().map(|&v| self.export(v)));
        }
        Error {
            kind: self.export_symbol(kind),
            values,
            trace,
        }
    }

    fn exported(&self, r: Result<local::Value, local::Error>) -> Result<Value, Error> {
        match r {
            Ok(v) => Ok(self.export(v)),
            Err(e) => Err(self.export_error(e)),
        }
    }
}

/// A sink that notes whether it refused text, to tell that from the printer's own failure.
struct Watched<'a> {
    out: &'a mut dyn fmt::Write,
    refused: bool,
}

impl fmt::Write for Watched<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.out.write_str(s).inspect_err(|_| self.refused = true)
    }
}
