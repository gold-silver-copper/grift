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
    /// Shared with the interrupters; its allocation is this interpreter's identity, which every
    /// value handed out holds.
    control: Arc<Control>,
    identity: Weak<Control>,
    /// The generation of the outer run in progress, 0 for none, and the last one given out.
    run: u64,
    runs: u64,
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
            identity: Weak::new(),
            control: Arc::new(Control::default()),
            run: 0,
            runs: 0,
            positions: HashMap::default(),
            #[cfg(test)]
            natives: true,
            #[cfg(test)]
            definitions: heap::Reference::NONE,
        };
        g.identity = Arc::downgrade(&g.control);
        let loaded = g.load();
        g.out(loaded).map(|()| g)
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
    /// assert_eq!(g.position(&e.trace()[0])?, Some(("keys.grift", 2)));
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
            let arguments = self.imported(arguments)?;
            self.call_local(Some(state), f, &arguments)
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
            let c = context_kind(c, Primitive::Bind)?;
            self.bind_tree(c, parameters, value)
        });
        self.out(done)
    }

    /// `(lookup context name otherwise)` without the default: the value bound to `name` in
    /// `context` or its nearest parent that binds it, `None` if none does.
    pub fn lookup(&self, context: &Value, name: &Symbol) -> Result<Option<Value>, Error> {
        let found = self.import(context).and_then(|c| {
            let name = self.import_symbol(name)?;
            self.find(context_kind(c, Primitive::Lookup)?, name)
        });
        self.out(found.map(|v| v.map(|v| self.export(v))))
    }

    /// `(names context)`: the symbols bound in `context` itself, sorted by name.
    pub fn names(&self, context: &Value) -> Result<Vec<Symbol>, Error> {
        let names = self
            .context_of(context, Primitive::Names)
            .and_then(|c| self.names_of(c));
        let names = names.and_then(|names| {
            collected(names.into_iter().map(|n| match n {
                local::Value::Symbol(s) => self.export_symbol(s),
                _ => self.export_symbol(K::Symbol.into()),
            }))
        });
        self.out(names)
    }

    /// From now on everything in the heap counts as library code, as grift's own library does:
    /// an error's trace skips its combinations and the code it built. A host calls this after
    /// evaluating its prelude and before any user code; data live at that moment, if later
    /// evaluated as code, is skipped too.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let top = g.top_level();
    /// g.evaluate_in(&mut (), &top, "fux.grift", "(define! split (lambda! (pane) (first pane)))")?;
    /// g.seal_library();
    /// let e = g.evaluate_in(&mut (), &top, "config.grift", "\n(split 9)").unwrap_err();
    /// let mut text = String::new();
    /// g.describe(&e, &mut text).unwrap();
    /// // The prelude's (first pane) is skipped: the error is the config's line.
    /// assert_eq!(text, "type-error #[primitive first] 9 in (split 9) at config.grift:2");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn seal_library(&mut self) {
        self.heap.seal_library();
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
            })
        });
        made.unwrap_or_else(|e| self.export_error(e))
    }

    /// Whether `error` is of the kind named `name`.
    pub fn is_kind(&self, error: &Error, name: &str) -> Result<bool, Error> {
        // A name never interned is no error's kind.
        let kind = self.import_symbol(&error.kind);
        self.out(kind.map(|kind| self.interned(name) == Some(kind)))
    }

    /// A new string holding a copy of `s`.
    pub fn string(&mut self, s: &str) -> Result<Value, Error> {
        let made = boxed(s).and_then(|s| self.heap.alloc(Object::String(s)));
        self.exported(made.map(local::Value::String))
    }

    /// The contents of a string value; `None` for anything else.
    pub fn as_str(&self, v: &Value) -> Result<Option<&str>, Error> {
        let s = self.import(v).and_then(|v| match v {
            local::Value::String(_) => self.text(Primitive::StringAppend, v).map(Some),
            _ => Ok(None),
        });
        self.out(s)
    }

    /// A copy of a string value's contents, which the host can hold while it calls `&mut self`
    /// methods; `None` for anything else.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let path = g.string("notes.txt")?;
    /// let path = g.as_string(&path)?.unwrap(); // owned: g is free again
    /// let e = g.error("file-error", &[]);
    /// assert_eq!((path.as_str(), g.is_kind(&e, "file-error")?), ("notes.txt", true));
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn as_string(&self, v: &Value) -> Result<Option<String>, Error> {
        let Some(s) = self.as_str(v)? else {
            return Ok(None);
        };
        let mut copy = String::new();
        let reserved = copy
            .try_reserve_exact(s.len())
            .map_err(|_| local::Error::oom());
        self.out(reserved).map(|()| Some(copy + s))
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
        let p = self.import(v).and_then(|v| match v {
            local::Value::Pair(r) => self.pair_at(r).map(Some),
            _ => Ok(None),
        });
        self.out(p.map(|p| p.map(|(a, b)| (self.export(a), self.export(b)))))
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
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let keys = g.evaluate(&mut (), "(list \"C-b\" \"v\")")?;
    /// let keys: Vec<String> = g.list_items(&keys)?.iter().filter_map(|k| g.as_string(k).ok()?).collect();
    /// assert_eq!(keys, ["C-b", "v"]);
    /// let dotted = g.evaluate(&mut (), "(pair 1 2)")?;
    /// assert!(g.is_kind(&g.list_items(&dotted).unwrap_err(), "type-error")?);
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn list_items(&self, list: &Value) -> Result<Vec<Value>, Error> {
        let items = self
            .import(list)
            .and_then(|l| self.items(l, || local::Error::new(K::TypeError, &[l])));
        let items = items.and_then(|items| collected(items.into_iter().map(|v| self.export(v))));
        self.out(items)
    }

    /// Reads every form in `src` into a list, evaluating nothing. A `parse-error` carries why and
    /// the line and column: of the `(` or `"` left open, or of what didn't read.
    pub fn read(&mut self, src: &str) -> Result<Value, Error> {
        let forms = self.read_local(src, None);
        self.exported(forms)
    }

    /// Writes `v` as grift would read it back, where it can be read back. Combiners show their
    /// parameters and body: `#[operative (x) #ignore x]`, `#[applicative #[primitive first]]`;
    /// a context shows its own names, sorted: `#[context a b]`. A reclaimed value is
    /// [`WriteError::Value`] with `stale-value`, never text.
    ///
    /// ```
    /// let mut g: grift::Grift = grift::Grift::new()?;
    /// let v = g.evaluate(&mut (), "(list 1.0 \"a\\rb\" (quote! x))")?;
    /// let mut text = String::new();
    /// g.write(&v, &mut text).unwrap();
    /// assert_eq!(text, "(1.0 \"a\\rb\" x)");
    /// g.evaluate(&mut (), "(length (map list (range 0 100000)))")?; // collects v
    /// assert!(matches!(g.write(&v, &mut text), Err(grift::WriteError::Value(_))));
    /// # Ok::<(), grift::Error>(())
    /// ```
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
    /// error's trace; `None` for any other value. A reclaimed value is `stale-value`, another
    /// interpreter's `foreign-value`.
    pub fn position(&self, combination: &Value) -> Result<Option<(&str, u32)>, Error> {
        let c = self.import(combination);
        self.out(c.map(|c| self.position_local(c)))
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
                owner: self.identity.clone(),
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

    /// Whether a stop of the run in progress came since the last one acted on: each stops it
    /// once, so its cleanups run, and a later one stops it again.
    #[inline(always)]
    pub(crate) fn interrupted(&self) -> bool {
        self.control.stopped(self.run)
    }

    /// Starts an outer run, unless one is in progress: the next generation, published for
    /// interrupters. Whether it started one, which [`Grift::end_run`] then ends. A run this
    /// interpreter holds that its control no longer publishes was abandoned (a host function
    /// replaced the interpreter, or panicked): its machine state goes, and this run is an outer one.
    pub(crate) fn begin_run(&mut self) -> Result<bool, local::Error> {
        if self.run != 0 {
            if self.control.active() == self.run {
                return Ok(false);
            }
            self.machine = evaluate::Machine::default();
        }
        // Generations never repeat, so a late stop can't match a later run.
        self.runs = self
            .runs
            .checked_add(1)
            .ok_or_else(|| local::Error::new(K::Overflow, &[]))?;
        self.run = self.runs;
        self.control.begin(self.run);
        Ok(true)
    }

    pub(crate) fn end_run(&mut self) {
        self.control.end();
        self.run = 0;
    }

    /// Gives up the run in progress and the machine's state: what a host function left behind
    /// when it replaced or restored the interpreter, or panicked.
    pub(crate) fn abandon(&mut self) {
        self.machine = evaluate::Machine::default();
        if self.run != 0 {
            self.end_run();
        }
    }

    /// The context `v` is, or a `type-error` naming `p`.
    fn context_of(&self, v: &Value, p: Primitive) -> Result<heap::Reference, local::Error> {
        context_kind(self.import(v)?, p)
    }

    fn owns(&self, owner: &Weak<Control>) -> bool {
        std::ptr::eq(owner.as_ptr(), Arc::as_ptr(&self.control))
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
            Value::Primitive(p) => L::Primitive(*p),
            Value::Symbol(s) => L::Symbol(self.import_symbol(s)?),
            Value::Pair(r) => L::Pair(live(r, |o| matches!(o, Object::Pair(..)))?),
            Value::String(r) => L::String(live(r, |o| matches!(o, Object::String(_)))?),
            Value::Context(r) => L::Context(live(r, |o| matches!(o, Object::Context(..)))?),
            Value::Operative(r) => L::Operative(live(r, |o| matches!(o, Object::Operative(_)))?),
            Value::Applicative(r) => {
                L::Applicative(live(r, |o| matches!(o, Object::Applicative(_)))?)
            }
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
        let (values, trace) = (self.imported(&e.values)?, self.imported(&e.trace)?);
        let mut seen = heap::Seen::default();
        seen.try_reserve(trace.len())
            .map_err(|_| local::Error::oom())?;
        seen.extend(trace.iter().filter_map(|v| match v {
            local::Value::Pair(r) => Some(*r),
            _ => None,
        }));
        let kind = self.import_symbol(&e.kind)?;
        Ok(local::Error {
            kind,
            values,
            trace,
            seen,
        })
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
            owner: self.identity.clone(),
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
                owner: self.identity.clone(),
                index: h.0,
            }),
        }
    }

    fn export_symbol(&self, s: local::Symbol) -> Symbol {
        Symbol {
            owner: self.identity.clone(),
            index: s.0,
        }
    }

    pub(crate) fn export_error(&self, e: local::Error) -> Error {
        let exported = |vs: &[local::Value]| collected(vs.iter().map(|&v| self.export(v)));
        let (kind, values, trace) = match (exported(&e.values), exported(&e.trace)) {
            (Ok(values), Ok(trace)) => (e.kind, values, trace),
            _ => (K::OutOfMemory.into(), Vec::new(), Vec::new()),
        };
        Error {
            kind: self.export_symbol(kind),
            values,
            trace,
        }
    }

    fn exported(&self, r: Result<local::Value, local::Error>) -> Result<Value, Error> {
        self.out(r.map(|v| self.export(v)))
    }

    fn out<R>(&self, r: Result<R, local::Error>) -> Result<R, Error> {
        r.map_err(|e| self.export_error(e))
    }
}

/// The context `v` is, or a `type-error` naming `p`.
fn context_kind(v: local::Value, p: Primitive) -> Result<heap::Reference, local::Error> {
    match v {
        local::Value::Context(c) => Ok(c),
        v => Err(evaluate::type_error(p, v)),
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
