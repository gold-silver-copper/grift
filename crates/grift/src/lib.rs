#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod evaluate;
mod heap;
mod interrupt;
mod local;
mod parked;
mod print;
mod read;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "a test may abort when memory is refused"
)]
mod tests;

use heap::{Bindings, Heap, Ref, kind};
use interrupt::Control;
use local::{K, push};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub use interrupt::Interrupter;
pub use local::Kind;
pub use parked::{Parked, Ran, ResumeError};
pub use print::{ReportStatus, TextBuffer, write_string};

/// The library, evaluated in the built-in context by [`Grift::new`].
const LIBRARY: &str = include_str!("library.grift");

/// How many items a walk inside one step takes between looks for a stop: a power of two.
const WALK_CHECK: usize = 4096;
const _: () = assert!(WALK_CHECK.is_power_of_two());

/// A grift value. Immediate values are plain data; the rest belong to the [`Grift`] that made
/// them, and every operation checks that: another interpreter's value is the error
/// `foreign-value`, never read as one of this one's.
///
/// A value returned to the host stays valid until the next call to [`Grift::evaluate`],
/// [`Grift::evaluate_in`], [`Grift::call`] or [`Grift::resume`], or the like; the collector may then reclaim it, and using it is the error `stale-value`,
/// never another object. To keep one longer, bind it in a context or [`Grift::keep`] it.
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Operative(OperativeKind);

/// How an operative was made.
#[derive(Clone, Copy, Debug, PartialEq)]
enum OperativeKind {
    /// By `vau!`: an object in the heap of one [`Grift`].
    Compound(Reference),
    /// A Rust primitive, the same in every interpreter (the library binds most of them wrapped).
    Primitive(evaluate::Primitive),
    /// A host function registered with one [`Grift`], which [`Grift::host_function`] wraps.
    Host(Owner, u32),
}

/// An interned symbol of one [`Grift`]; its name is [`Grift::symbol_name`]. A name is collected
/// like any value once nothing in the interpreter refers to it, so a symbol handed to the host
/// lasts as a [`Value`] does: until the next run, unless something the program holds, such as a
/// binding, a list or a [`Kept`] value, has it. A reclaimed one is `stale-value`, never another
/// name, and interning its spelling again gives a new symbol, unequal to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Symbol {
    owner: Owner,
    index: u32,
    generation: u32,
}

/// An object in the heap of one [`Grift`]: a slot and its generation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reference {
    owner: Owner,
    local: Ref,
}

/// The interpreter a value belongs to: a number no other interpreter in the process has, counted
/// from one static. Counting one a nanosecond, it would take 584 years to come round.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Owner(u64);

static OWNERS: AtomicU64 = AtomicU64::new(0);

/// A host function. It receives a [`HostCall`] on the interpreter, the host's state (the one
/// passed to [`Grift::evaluate`], [`Grift::call`] or the like) and the evaluated arguments, and
/// gives its [`Answer`]. It begins no run of its own: a `HostCall` has the values, not `evaluate`,
/// `call` or any other way in, for a run there would stack the program's depth on this function's
/// Rust frame. To have grift call something, it answers [`Answer::Call`]. One that panics ends the
/// run it was called from: the panic goes on to the host, no cleanup runs, and the interpreter
/// starts its next evaluation afresh.
///
/// ```compile_fail
/// // E0596: a HostCall lends the Grift only as `&Grift`, and `call` needs `&mut`.
/// let mut g: grift::Grift = grift::Grift::new()?;
/// g.host_function("f", |g, s, args| g.call(s, &args[0], &[]).map(grift::Answer::Now))?;
/// # Ok::<(), grift::Error>(())
/// ```
pub type HostFunction<T> = fn(&mut HostCall<'_, T>, &mut T, &[Value]) -> Result<Answer, Error>;

/// What a [`HostFunction`] is given: the interpreter it was called from, without a way to begin a
/// run. It reads as a [`Grift`] does, and makes, binds and keeps values with the methods below,
/// each the `Grift` method of its name.
pub struct HostCall<'a, T>(&'a mut Grift<T>);

/// One outside a run, for a helper the host shares with its host functions.
impl<'a, T> From<&'a mut Grift<T>> for HostCall<'a, T> {
    fn from(g: &'a mut Grift<T>) -> HostCall<'a, T> {
        HostCall(g)
    }
}

impl<T> std::ops::Deref for HostCall<'_, T> {
    type Target = Grift<T>;
    fn deref(&self) -> &Grift<T> {
        self.0
    }
}

/// The `Grift` methods a [`HostCall`] has that change the interpreter.
macro_rules! host_call {
    ($(fn $name:ident(&mut self $(, $a:ident: $t:ty)*) -> $r:ty;)*) => {
        impl<T> HostCall<'_, T> {
            $(
                #[doc = concat!("[`Grift::", stringify!($name), "`].")]
                pub fn $name(&mut self $(, $a: $t)*) -> $r {
                    self.0.$name($($a),*)
                }
            )*
        }
    };
}

host_call! {
    fn context(&mut self, parent: Option<&Value>) -> Result<Value, Error>;
    fn private_context(&mut self) -> Result<Value, Error>;
    fn define(&mut self, name: &str, v: Value) -> Result<(), Error>;
    fn bind(&mut self, context: &Value, parameters: &Value, value: Value) -> Result<(), Error>;
    fn symbol(&mut self, name: &str) -> Result<Symbol, Error>;
    fn error(&mut self, kind: impl AsRef<str>, values: &[Value]) -> Error;
    fn limit(&mut self, kind: impl AsRef<str>, values: &[Value]) -> Error;
    fn string(&mut self, s: &str) -> Result<Value, Error>;
    fn bytes(&mut self, b: &[u8]) -> Result<Value, Error>;
    fn pair(&mut self, a: Value, b: Value) -> Result<Value, Error>;
    fn list(&mut self, values: &[Value]) -> Result<Value, Error>;
    fn read(&mut self, src: &str) -> Result<Value, Error>;
    fn read_in(&mut self, source: &str, text: &str) -> Result<Value, Error>;
    fn keep(&mut self, v: Value) -> Result<Kept, Error>;
    fn release(&mut self, k: Kept) -> Result<(), (Error, Kept)>;
}

impl<T> HostCall<'_, T> {
    /// The arguments of a host function that takes exactly `N`, or else [`HostCall::arity_error`].
    pub fn exactly<'v, const N: usize>(
        &mut self,
        args: &'v [Value],
    ) -> Result<&'v [Value; N], Error> {
        args.try_into().map_err(|_| self.arity_error(args))
    }

    /// The `arity-error` a primitive gives: one value, the arguments as a list. With no memory for
    /// the list, `out-of-memory`.
    pub fn arity_error(&mut self, args: &[Value]) -> Error {
        match self.0.list(args) {
            Ok(list) => self.0.error(Kind::ArityError, &[list]),
            Err(e) => e,
        }
    }

    /// The `type-error` a primitive gives: one value, the one of the wrong type.
    pub fn type_error(&mut self, value: Value) -> Error {
        self.0.error(Kind::TypeError, &[value])
    }
}

/// What a [`HostFunction`] answers.
#[derive(Debug, PartialEq)]
pub enum Answer {
    /// This value.
    Now(Value),
    /// Not yet: the run is set aside, and [`Grift::resume`] gives this call its value or error.
    /// The value says what is being waited for, and [`Parked::request`] hands it to the host's
    /// loop: do the work there, once the run has really parked, for it may not. In a run that
    /// can't park ([`Grift::can_park`]) this call fails with `cannot-park`, naming the function; a
    /// stop that reached the run fails it with `interrupted`; and with no memory to park,
    /// `out-of-memory`.
    Later(Value),
    /// Call `function` with `arguments`, as [`Grift::call`] would, once this host call has
    /// returned: the machine makes the call on its own frames, so it costs no Rust stack, a
    /// program can recurse through a host function as deep as memory goes, and a wait inside it
    /// parks the run as any other does. With no `then`, the call's ending is this call's, in tail
    /// position. With `then`, an applicative and values saved for it, the call's value goes to
    /// `then` after those values, and what `then` gives is this call's; a host function as `then`
    /// may answer `Call` again, which is how a host function loops. An error passes `then` by, as
    /// a `?` would. The vectors are the host's to make: `Vec::new()` allocates nothing, and
    /// `try_reserve` makes room that can be refused instead of aborting.
    ///
    /// ```
    /// use grift::{Answer, Grift, Value};
    /// let mut g: Grift = Grift::new()?;
    /// // `(twice f x)` is `(f (f x))`: the first call's value goes to `again`, saved with `f`.
    /// let twice = g.host_function("twice", |_, _, args| {
    ///     Ok(Answer::Call {
    ///         function: args[0],
    ///         arguments: vec![args[1]],
    ///         then: Some((args[2], vec![args[0]])),
    ///     })
    /// })?;
    /// g.define("twice", twice)?;
    /// let again = g.host_function("again", |_, _, args| {
    ///     Ok(Answer::Call { function: args[0], arguments: vec![args[1]], then: None })
    /// })?;
    /// g.define("again", again)?;
    /// let v = g.evaluate(&mut (), "(twice (lambda! (n) (multiply n 3)) 7 again)")?;
    /// assert_eq!(v, Value::Integer(63));
    /// # Ok::<(), grift::Error>(())
    /// ```
    Call {
        /// An applicative, as for [`Grift::call`].
        function: Value,
        /// What it is called with.
        arguments: Vec<Value>,
        /// An applicative for the call's value, and the values it gets before that value.
        then: Option<(Value, Vec<Value>)>,
    },
}

/// What went wrong: a symbol naming the kind of error, the values involved, and where.
/// [`Grift::describe`] writes it for a person.
#[derive(Debug, PartialEq)]
pub struct Error {
    /// The kind, such as `type-error` or a kind a program gave `error`; [`Grift::symbol_name`]
    /// reads it, and [`Error::grift_kind`] says which of grift's own it is.
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
    /// host function stays what it was. Its kind then says which limit it is, or,
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
    /// use grift::{Grift, Value};
    /// let mut g: Grift = Grift::new()?;
    /// let quit = g.host_function("quit", |g, _, args| Err(g.limit("quit", args)))?;
    /// g.define("quit", quit)?;
    /// let e = g.evaluate(&mut (), "(catch! (quit 3) e 0) (add 1 2)").unwrap_err();
    /// assert!(g.symbol_name(&e.kind)? == "quit" && e.values == [Value::Integer(3)]);
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub limit: bool,
}

impl Error {
    /// Which of grift's own kinds this error is, if it is one of them, whoever raised it.
    pub fn grift_kind(&self) -> Option<Kind> {
        let index = usize::try_from(self.kind.index).ok()?;
        local::KINDS.get(index).copied()
    }
}

/// An error has no `clone`, which would abort when memory is refused: a host keeps the one it was
/// given, or makes another with [`Grift::error`].
///
/// ```compile_fail
/// fn copied(e: &grift::Error) -> grift::Error {
///     e.clone()
/// }
/// ```
#[cfg(doctest)]
struct ErrorIsNotClone;

/// Nor has an answer, whose call holds vectors.
///
/// ```compile_fail
/// fn copied(a: &grift::Answer) -> grift::Answer {
///     a.clone()
/// }
/// ```
#[cfg(doctest)]
struct AnswerIsNotClone;

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
            // Nothing, but passing it is a step of the walk, which an immediate counts.
            Entry::Free(_) => (Some(local::Value::Inert), &[][..]),
            Entry::Value(v) => (Some(*v), &[][..]),
            Entry::Run(room) => (None, room.as_slice()),
        };
        value
            .into_iter()
            .chain(runs.iter().flat_map(evaluate::Machine::roots))
    }
}

/// Room held back for when memory is refused, given up as the refusal unwinds: for the trace the
/// unwinding builds, the program's combinations it passes, and after it for a report of the
/// error, whose printer needs about 2 KiB whatever the error holds, beside the host's own text.
/// It is no promise past that: a deep trace, or a host's text longer than what is left, can
/// still find memory refused.
const SPARE: usize = 64 * 1024;

/// An interpreter: a heap, the built-in context holding the primitives and the library, and the
/// top-level context, a child of the built-in one, where programs define things. `T` is the
/// host's state, which the host lends to each [`Grift::evaluate`] and [`Grift::call`] and which
/// they pass on to host functions.
pub struct Grift<T = ()> {
    heap: Heap,
    hosts: Vec<(local::Symbol, HostFunction<T>)>,
    built_in: Ref<kind::Context>,
    top: Ref<kind::Context>,
    /// The kept values and parked runs, and the first free entry.
    kept: Vec<Entry>,
    kept_free: Option<u32>,
    /// The kept name of the run that has just parked, for [`Grift::started`] alone to read.
    parked_as: Option<u32>,
    machine: evaluate::Machine,
    /// Room held back for an `out-of-memory` to name where it happened: given up as a refusal
    /// unwinds, when small allocations may have only scraps left, and taken again by the next run.
    spare: Vec<u8>,
    /// Shared with the interrupters and parked runs' handles.
    control: Arc<Control>,
    /// This interpreter's identity, which every value handed out holds.
    owner: Owner,
}

impl<T> Grift<T> {
    /// Builds the built-in context, evaluates the library in it and makes an empty top level.
    /// Memory refused is `out-of-memory`, but for one allocation, the first: the interpreter's
    /// shared control block, which its interrupters point to, is an `Arc::new`, and so
    /// aborts the process if refused. It becomes `Arc::try_new` once that is stable.
    pub fn new() -> Result<Grift<T>, Error> {
        let (heap, made) = Heap::new();
        #[expect(
            clippy::disallowed_methods,
            reason = "Arc::try_new is unstable: the one abort `new` documents"
        )]
        let mut g = Grift {
            heap,
            hosts: Vec::new(),
            built_in: Ref::NONE,
            top: Ref::NONE,
            kept: Vec::new(),
            kept_free: None,
            parked_as: None,
            machine: evaluate::Machine::default(),
            spare: Vec::new(),
            control: Arc::new(Control::default()),
            owner: Owner(OWNERS.fetch_add(1, Ordering::Relaxed)),
        };
        let loaded = made.and_then(|()| g.load());
        g.out(loaded).map(|()| g)
    }

    fn load(&mut self) -> Result<(), local::Error> {
        self.built_in = self.heap.make((None, Bindings::default()))?;
        self.top = self.heap.make((Some(self.built_in), Bindings::default()))?;
        for &p in evaluate::Primitive::ALL {
            let s = self.heap.intern(p.name())?;
            let wrapped = self.heap.make(local::Value::Primitive(p))?;
            let v = local::Value::Applicative(wrapped);
            self.heap.bind(self.built_in, [(s, v)])?;
        }
        self.run_text(None, self.built_in, None, LIBRARY, true, None)?;
        self.install_natives()
    }

    /// Reads every form in `src`, then evaluates them in order in the top-level context.
    /// Returns the last form's value (`#inert` for no forms). Nothing is evaluated if `src`
    /// doesn't read. Host functions get `state`. [`Grift::evaluate_in`] names the source.
    pub fn evaluate(&mut self, state: &mut T, src: &str) -> Result<Value, Error> {
        let done = self.run_text(Some(state), self.top, None, src, false, None);
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
    /// let mut text = grift::TextBuffer::new();
    /// g.describe(&e, &mut text, 4096)?;
    /// assert_eq!(text.as_str(), "type-error at keys.grift:2 1 in (first a)");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn evaluate_in(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
    ) -> Result<Value, Error> {
        let done = self
            .context_of(context)
            .and_then(|c| self.run_text(Some(state), c, Some(source), src, false, None));
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
    /// let mut text = grift::TextBuffer::new();
    /// g.describe(&e, &mut text, 4096)?;
    /// // The prelude's (first xs) is skipped: the error is the config's line.
    /// assert_eq!(text.as_str(), "type-error at config.grift:2 9 in (head 9)");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn evaluate_library_in(
        &mut self,
        state: &mut T,
        context: &Value,
        source: &str,
        src: &str,
    ) -> Result<Value, Error> {
        let done = self
            .context_of(context)
            .and_then(|c| self.run_text(Some(state), c, Some(source), src, true, None));
        self.exported(done)
    }

    /// Reads `src`, the text of `source` if it has a name, and evaluates its forms in `context`;
    /// `library` marks it as a host's own grift code, and `parks` is the run's right to park.
    fn run_text(
        &mut self,
        state: Option<&mut T>,
        context: Ref<kind::Context>,
        source: Option<&str>,
        src: &str,
        library: bool,
        parks: Option<parked::MayPark>,
    ) -> Result<local::Value, local::Error> {
        let ready = self.prepare(std::iter::once(local::Value::Context(context)), []);
        let source = source.map(|s| self.heap.intern(s)).transpose()?;
        let first = evaluate::First::Text {
            context,
            source,
            src,
            library,
        };
        self.run_on_machine(ready, parks, state, first)
    }

    /// Calls applicative `f` with `arguments` as `apply` does: its underlying combiner gets the
    /// arguments as its operands, in a new empty context; anything else is apply's `type-error`.
    /// Host functions get `state`; one that wants a call answers [`Answer::Call`] instead.
    pub fn call(&mut self, state: &mut T, f: &Value, arguments: &[Value]) -> Result<Value, Error> {
        let done = self.run_call(state, f, arguments, None);
        self.exported(done)
    }

    /// Calls `f` as [`Grift::call`] does; `parks` is the run's right to park.
    fn run_call(
        &mut self,
        state: &mut T,
        f: &Value,
        arguments: &[Value],
        parks: Option<parked::MayPark>,
    ) -> Result<local::Value, local::Error> {
        let (f, ready) = self.prepare_call(f, arguments)?;
        let arguments = self.imported(arguments)?;
        let first = evaluate::First::Call {
            f,
            arguments: &arguments,
        };
        self.run_on_machine(ready, parks, Some(state), first)
    }

    /// An applicative calling `f`, bound nowhere: the host binds it with [`Grift::define`], or
    /// hands it to grift code that keeps it private.
    pub fn host_function(&mut self, name: &str, f: HostFunction<T>) -> Result<Value, Error> {
        let made = self.heap.intern(name).and_then(|s| {
            let id = u32::try_from(self.hosts.len()).map_err(|_| local::Error::oom())?;
            let host = self.heap.make(local::Value::Host(local::Host(id)))?;
            push(&mut self.hosts, (s, f))?;
            Ok(local::Value::Applicative(host))
        });
        self.exported(made)
    }

    /// A new empty context, `(context parent)` with `()` for none: with no parent, a record, as
    /// `error` makes; with the top level as its parent, a place for a host's own definitions.
    pub fn context(&mut self, parent: Option<&Value>) -> Result<Value, Error> {
        let made = parent
            .map(|p| self.context_of(p))
            .transpose()
            .and_then(|parent| self.heap.make((parent, Bindings::default())));
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
        let made = self
            .heap
            .make((Some(self.built_in), Bindings::default()))
            .map(local::Value::Context);
        self.exported(made)
    }

    /// The top-level context, where programs define things.
    pub fn top_level(&self) -> Value {
        self.export(local::Value::Context(self.top))
    }

    /// Binds `name` to `v` in the top-level context.
    pub fn define(&mut self, name: &str, v: Value) -> Result<(), Error> {
        let done = self.import(&v).and_then(|v| {
            let s = self.heap.intern(name)?;
            self.heap.bind(self.top, [(s, v)])
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
        let done = Control::host_run(&self.control).and_then(|_run| {
            let c = self.import(context)?;
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
        let s = self.heap.intern(name);
        self.out(s.map(|s| self.export_symbol(s)))
    }

    /// The name of a symbol.
    pub fn symbol_name(&self, symbol: &Symbol) -> Result<&str, Error> {
        self.out(self.import_symbol(symbol).map(|s| self.name(s)))
    }

    /// An error of kind `kind`, a [`Kind`] or a host's own, with a copy of `values`.
    pub fn error(&mut self, kind: impl AsRef<str>, values: &[Value]) -> Error {
        let made = self.heap.intern(kind.as_ref()).and_then(|kind| {
            Ok(Error {
                kind: self.export_symbol(kind),
                values: collected(values.iter().cloned())?,
                trace: Vec::new(),
                limit: false,
            })
        });
        made.unwrap_or_else(|e| self.export_error(e))
    }

    /// [`Grift::error`] as a limit, which no handler catches ([`Error::limit`]).
    pub fn limit(&mut self, kind: impl AsRef<str>, values: &[Value]) -> Error {
        let mut e = self.error(kind, values);
        e.limit = true;
        e
    }

    /// A new string holding a copy of `s`.
    pub fn string(&mut self, s: &str) -> Result<Value, Error> {
        let made = self.string_local(s);
        self.exported(made)
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
            #[expect(clippy::disallowed_methods, reason = "one pair for the host")]
            local::Value::Pair(r) => self.heap.pair_at(r).map(Some),
            _ => Ok(None),
        });
        self.out(p.map(|p| p.map(|(a, b, _)| (self.export(a), self.export(b)))))
    }

    /// A new list of `values`, as `pair` builds one from its end.
    pub fn list(&mut self, values: &[Value]) -> Result<Value, Error> {
        let made = self.pairs(values, local::Value::EmptyList);
        self.exported(made)
    }

    /// `heads` in front of `tail`, built by the host, so the pairs are the program's.
    fn pairs(&mut self, heads: &[Value], tail: local::Value) -> Result<local::Value, local::Error> {
        heads.iter().rev().try_fold(tail, |rest, v| {
            let v = self.import(v)?;
            self.pair_local(v, rest, false)
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
        let items = self.import(list).and_then(|l| self.items(l, l));
        let items = items.and_then(|items| collected(items.into_iter().map(|v| self.export(v))));
        self.out(items)
    }

    /// Reads every form in `src` into a list, evaluating nothing. The values of a `parse-error`
    /// are why and the line and column: of the `(` or `"` left open, or of what didn't read.
    /// Reading never collects: after `out-of-memory`, further reads may fail until an evaluation
    /// or call gives collection its next safe point.
    pub fn read(&mut self, src: &str) -> Result<Value, Error> {
        self.read_text(None, src)
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
        self.read_text(Some(source), text)
    }

    /// [`Grift::read_in`], or [`Grift::read`] with no `source`.
    fn read_text(&mut self, source: Option<&str>, text: &str) -> Result<Value, Error> {
        let forms = Control::host_run(&self.control).and_then(|_run| {
            let source = source.map(|s| self.heap.intern(s)).transpose()?;
            self.read_local(text, source, false)
        });
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
    /// let mut text = grift::TextBuffer::new(); // grows fallibly, where a String aborts
    /// g.write(&v, &mut text)?;
    /// assert_eq!(text.as_str(), "(1.0 \"a\\rb\" x)");
    /// g.evaluate(&mut (), "(length (map list (range 0 100000)))")?; // collects v
    /// let e = g.write(&v, &mut text).unwrap_err();
    /// assert_eq!(g.symbol_name(&e.kind)?, "stale-value");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn write(&self, v: &Value, out: &mut dyn fmt::Write) -> Result<(), Error> {
        let written = Control::host_run(&self.control)
            .and_then(|_run| self.import(v).and_then(|v| self.print(v, out)));
        self.out(written)
    }

    /// Writes an error for a person, in at most `bytes` bytes of `out`: its kind and the innermost
    /// place its trace has, `type-error at keys.grift:2`, then its values, then each combination
    /// of its trace, the first after ` in ` and each one around it on a line of its own, `\n  from
    /// (f ...) at keys.grift:3`, where `...` is the line before's combination, so each piece of
    /// source is written once. A report that fits `bytes` is written whole. One that doesn't gives
    /// the kind and the source's name at most a quarter of `bytes` each, cut with `...`, so it
    /// begins with both from 30 bytes up, and below that with as much of the kind as fits. A
    /// context is written as its count of names, `#[context 3 names]`, where [`Grift::write`]
    /// lists them, and a name's control characters are escaped as in a string literal, so only
    /// the report begins a line. Data nested more than 32 deep is written as `...` too. Nothing
    /// is copied: past the text the sink holds, a report needs about 2 KiB for its printer,
    /// whatever the error holds, so after `out-of-memory` a host needs room for its text and
    /// that; a [`TextBuffer`] may hold up to twice its text while it grows.
    ///
    /// [`ReportStatus::Truncated`] says the report was cut. A [`TextBuffer`] holds the text
    /// fallibly. A value of the error that is stale or another interpreter's fails it before
    /// anything is written, a stop that reaches it is `interrupted`, and an `out` that refuses the
    /// text `output-error`.
    ///
    /// ```
    /// use grift::{Grift, ReportStatus, TextBuffer};
    /// let mut g: Grift = Grift::new()?;
    /// let top = g.top_level();
    /// let e = g.evaluate_in(&mut (), &top, "keys.grift", "(define! f (lambda! (x) (first x)))\n(f 1)").unwrap_err();
    /// let mut text = TextBuffer::new();
    /// assert_eq!(g.describe(&e, &mut text, 4096)?, ReportStatus::Complete);
    /// assert_eq!(text.as_str(), "type-error at keys.grift:1 1 in (first x)\n  from (f 1) at keys.grift:2");
    /// let mut text = TextBuffer::new();
    /// assert_eq!(g.describe(&e, &mut text, 40)?, ReportStatus::Truncated);
    /// assert_eq!(text.as_str(), "type-error at keys.grift:1 1 in (first x");
    /// # Ok::<(), grift::Error>(())
    /// ```
    pub fn describe(
        &self,
        error: &Error,
        out: &mut dyn fmt::Write,
        bytes: usize,
    ) -> Result<ReportStatus, Error> {
        let run = Control::host_run(&self.control);
        self.out(run.and_then(|_run| self.report(error, out, bytes)))
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
        #[expect(clippy::disallowed_methods, reason = "one pair's origin")]
        let (source, line) = self.heap.pair_at(r).ok()?.2.place?;
        Some((self.name(source), line))
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
        self.out(kept.map(|name| Kept {
            owner: self.owner,
            name,
        }))
    }

    /// The first free entry's name, a released one first so keeping on every event never runs out
    /// of names, one made if there is none. Filling it can't fail, so a caller that may yet turn
    /// back has nothing to undo.
    pub(crate) fn free_name(&mut self) -> Result<u32, local::Error> {
        if let Some(name) = self.kept_free {
            return Ok(name);
        }
        let name = u32::try_from(self.kept.len()).map_err(|_| local::Error::oom())?;
        push(&mut self.kept, Entry::Free(None))?;
        // Every collection walks the kept table, so a new entry brings the next one nearer.
        self.heap.charge(1);
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

    /// Gives back the free entries at the end of the kept table, and room it no longer uses (as
    /// [`heap::tidy`] says), and makes the free list again from the bottom up, so the lowest names
    /// are reused first and the end empties. Each collection walks the whole table, and this once
    /// more.
    pub(crate) fn tidy_kept(&mut self) {
        let used = self.kept.iter().rposition(|e| !matches!(e, Entry::Free(_)));
        self.kept.truncate(used.map_or(0, |i| i + 1));
        heap::tidy(&mut self.kept);
        self.kept_free = None;
        for (name, entry) in (0..u32::MAX).zip(self.kept.iter_mut()).rev() {
            if let Entry::Free(next) = entry {
                *next = self.kept_free;
                self.kept_free = Some(name);
            }
        }
    }

    /// The value `k` keeps.
    pub fn kept(&self, k: &Kept) -> Result<Value, Error> {
        let found = self
            .owns(&k.owner)
            .and_then(|()| match self.kept.get(k.name as usize) {
                Some(Entry::Value(v)) => Ok(*v),
                _ => Err(local::Error::stale()),
            });
        self.exported(found)
    }

    /// Clears the value `k` keeps, so it can be collected. Another interpreter's `Kept` is the
    /// error `foreign-value`, and comes back with it, to be released in its own.
    pub fn release(&mut self, k: Kept) -> Result<(), (Error, Kept)> {
        if let Err(e) = self.owns(&k.owner) {
            return Err((self.export_error(e), k));
        }
        self.clear_kept(k.name);
        Ok(())
    }

    /// The name of symbol `s`.
    pub(crate) fn name(&self, s: local::Symbol) -> &str {
        self.heap.symbol(s.0).map_or("", |(name, _)| name)
    }

    /// The context `v` is, or a `type-error`.
    fn context_of(&self, v: &Value) -> Result<Ref<kind::Context>, local::Error> {
        context_kind(self.import(v)?)
    }

    /// The one check that something is this interpreter's: `foreign-value` if it isn't.
    fn owns(&self, owner: &Owner) -> Result<(), local::Error> {
        if *owner == self.owner {
            Ok(())
        } else {
            Err(local::Error::new(K::ForeignValue, &[]))
        }
    }

    /// `v` as this interpreter's own: `foreign-value` if another made it, `stale-value` if it was
    /// reclaimed. Every value the host hands in comes through here, so nothing stale or foreign
    /// is ever stored, and a live object holds only live objects.
    pub(crate) fn import(&self, v: &Value) -> Result<local::Value, local::Error> {
        use local::Value as L;
        Ok(match v {
            Value::EmptyList => L::EmptyList,
            Value::Boolean(b) => L::Boolean(*b),
            Value::Inert => L::Inert,
            Value::Ignore => L::Ignore,
            Value::Integer(n) => L::Integer(*n),
            Value::Float(f) => L::Float(*f),
            Value::Symbol(s) => L::Symbol(self.import_symbol(s)?),
            Value::Pair(r) => L::Pair(self.live(r)?),
            Value::String(r) => L::String(self.live(r)?),
            Value::Bytes(r) => L::Bytes(self.live(r)?),
            Value::Context(r) => L::Context(self.live(r)?),
            Value::Operative(Operative(kind)) => match kind {
                OperativeKind::Compound(r) => L::Operative(self.live(r)?),
                OperativeKind::Primitive(p) => L::Primitive(*p),
                OperativeKind::Host(owner, index) => {
                    self.owns(owner)?;
                    L::Host(local::Host(*index))
                }
            },
            Value::Applicative(r) => L::Applicative(self.live(r)?),
        })
    }

    /// `r` as this interpreter's, naming a live object of the kind the variant it came in says: a
    /// reference put in another variant is no value of this interpreter's.
    fn live<O: heap::Kind>(&self, r: &Reference) -> Result<Ref<O>, local::Error> {
        self.owns(&r.owner)?;
        self.heap.kind(r.local)
    }

    fn import_symbol(&self, s: &Symbol) -> Result<local::Symbol, local::Error> {
        self.owns(&s.owner)?;
        match self.heap.symbol(s.index) {
            Some((_, generation)) if generation == s.generation => Ok(local::Symbol(s.index)),
            _ => Err(local::Error::stale()),
        }
    }

    /// An error a host function returned, as this interpreter's, a program's.
    fn import_error(&self, e: &Error) -> Result<local::Error, local::Error> {
        let (values, trace) = (self.imported(&e.values)?, self.imported(&e.trace)?);
        let (kind, trace) = (self.import_symbol(&e.kind)?, local::Trace::given(trace)?);
        Ok(local::Error::from_parts(kind, values, trace))
    }

    /// An error the host hands in, as the reason a run unwinds. One that can't come in, its values
    /// reclaimed or another interpreter's, is that error instead. Either is a limit if it was one.
    pub(crate) fn incoming_error(&self, e: &Error) -> local::Error {
        let (Ok(imported) | Err(imported)) = self.import_error(e);
        if e.limit {
            imported.into_limit()
        } else {
            imported
        }
    }

    /// Validate and root a host call before its first allocation. A call of what isn't an
    /// applicative is refused here, before its run begins.
    fn prepare_call(
        &mut self,
        f: &Value,
        arguments: &[Value],
    ) -> Result<(local::Value, evaluate::Prepared), local::Error> {
        let f = self.import(f)?;
        arguments
            .iter()
            .try_for_each(|v| self.import(v).map(drop))?;
        self.combiner_of(f)?;
        // Validation allocates nothing. Root every checked reference before the argument
        // buffer needs memory; marking follows the reference, whatever its object's kind.
        let held =
            arguments.iter().filter_map(|v| match v {
                Value::Pair(r)
                | Value::String(r)
                | Value::Bytes(r)
                | Value::Context(r)
                | Value::Operative(Operative(OperativeKind::Compound(r)))
                | Value::Applicative(r) => Some(r.local),
                // Listed, not a catch-all: a new kind of value is classed here or doesn't compile.
                Value::EmptyList
                | Value::Boolean(_)
                | Value::Inert
                | Value::Ignore
                | Value::Integer(_)
                | Value::Float(_)
                | Value::Symbol(_)
                | Value::Operative(Operative(
                    OperativeKind::Primitive(_) | OperativeKind::Host(..),
                )) => None,
            });
        let names = arguments.iter().filter_map(|v| match v {
            Value::Symbol(s) => Some(local::Value::Symbol(local::Symbol(s.index))),
            _ => None,
        });
        Ok((f, self.prepare(std::iter::once(f).chain(names), held)))
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
            owner: self.owner,
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
            L::Pair(r) => Value::Pair(reference(r.untyped())),
            L::String(r) => Value::String(reference(r.untyped())),
            L::Bytes(r) => Value::Bytes(reference(r.untyped())),
            L::Context(r) => Value::Context(reference(r.untyped())),
            L::Operative(r) => operative(OperativeKind::Compound(reference(r.untyped()))),
            L::Applicative(r) => Value::Applicative(reference(r.untyped())),
            L::Host(h) => operative(OperativeKind::Host(self.owner, h.0)),
        }
    }

    fn export_symbol(&self, s: local::Symbol) -> Symbol {
        Symbol {
            owner: self.owner,
            index: s.0,
            generation: self
                .heap
                .symbol(s.0)
                .map_or(0, |(_, generation)| generation),
        }
    }

    /// An error as the host's. Memory refused here, not on its way through a run, asks for its
    /// collection now: the host's next try has that, as a run's cleanups do.
    pub(crate) fn export_error(&self, mut e: local::Error) -> Error {
        if e.take_refusal() {
            self.heap.request_collection();
        }
        let exported = |vs: &[local::Value]| collected(vs.iter().map(|&v| self.export(v)));
        // The host's trace is innermost first: the shared list's places reversed, then the rest.
        let (shared, added) = e.trace.parts();
        let trace = self.items(shared, shared).and_then(|mut v| {
            v.reverse();
            v.try_reserve_exact(added.len())?;
            #[expect(clippy::disallowed_methods, reason = "reserved added.len() above")]
            v.extend_from_slice(added);
            exported(&v)
        });
        match (exported(&e.values), trace) {
            (Ok(values), Ok(trace)) => Error {
                kind: self.export_symbol(e.kind),
                values,
                trace,
                limit: e.limit(),
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
fn context_kind(v: local::Value) -> Result<Ref<kind::Context>, local::Error> {
    match v {
        local::Value::Context(c) => Ok(c),
        v => Err(evaluate::type_error(v)),
    }
}

/// The items in a new vector, reporting `out-of-memory` instead of aborting.
fn collected<X>(items: impl ExactSizeIterator<Item = X>) -> Result<Vec<X>, local::Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(items.len())?;
    #[expect(clippy::disallowed_methods, reason = "reserved exactly above")]
    v.extend(items);
    Ok(v)
}
