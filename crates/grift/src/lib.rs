#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! # grift
//!
//! A minimal, pure, type-strict Lisp built on John Shutt's vau calculus.
//!
//! The core is a handful of Rust primitives with exactly one primitive
//! operative, `$vau`. Everything else — `$if`, `$define!`, `$lambda`,
//! `$let`, `$cond`, lists, n-ary arithmetic, error handling, dynamic
//! binding, effect handlers, contracts — is written in grift itself and
//! loaded from the bundled library at startup.
//!
//! No value ever changes once observed: pairs, strings and environments are
//! immutable, definitions extend environments by creating children, and
//! control effects come from multi-prompt delimited continuations.
//!
//! ```
//! use grift::Grift;
//!
//! let mut g = Grift::new();
//! g.eval("($define! square ($lambda (x) (* x x)))").unwrap();
//! let v = g.eval("(square 12)").unwrap();
//! assert_eq!(g.write(v), "144");
//! ```

extern crate alloc;

mod ctx;
mod env;
mod error;
mod heap;
mod machine;
mod prims;
mod print;
mod read;
mod symbol;
mod value;

use alloc::string::String;

pub use ctx::Ctx;
pub use error::Error;
pub use heap::HeapStats;
pub use value::{ObjId, PrimId, SymId, Value};

use heap::Object;
use machine::{Ctl, Machine};

/// A native function: receives the store and its evaluated arguments.
///
/// Errors are delivered to grift code through the error protocol, exactly
/// like errors raised by primitives.
pub type NativeFn = fn(&mut Ctx, &[Value]) -> Result<Value, Error>;

/// The bundled library, loaded in order into the ground environment.
const LIBRARY: &[(&str, &str)] = &[
    ("00-boot.grift", include_str!("../lib/00-boot.grift")),
    ("10-control.grift", include_str!("../lib/10-control.grift")),
    ("20-env.grift", include_str!("../lib/20-env.grift")),
    ("30-list.grift", include_str!("../lib/30-list.grift")),
    ("40-integer.grift", include_str!("../lib/40-integer.grift")),
];

/// A grift interpreter.
///
/// Cloning a booted interpreter is much cheaper than booting a new one.
#[derive(Clone, Debug)]
pub struct Grift {
    m: Machine,
}

/// Index of the current top-level environment in `Machine::env_roots`.
const TOP: usize = 0;

impl Default for Grift {
    fn default() -> Self {
        Self::new()
    }
}

impl Grift {
    /// Boot an interpreter: load the library into the ground environment and
    /// open a standard environment for user code.
    ///
    /// # Panics
    ///
    /// Panics if the bundled library fails to load, which is a bug.
    pub fn new() -> Self {
        let mut m = Machine::new();
        let ground = m.ctx.extend(m.prim_env, []);
        m.env_roots.push(ground);
        let mut g = Grift { m };
        for (name, src) in LIBRARY {
            if let Err(e) = g.eval(src) {
                panic!("library file {name} failed to load: {e}");
            }
        }
        let ground = g.m.env_roots[TOP];
        let standard = g.m.ctx.extend(ground, []);
        g.m.env_roots[TOP] = standard;
        g
    }

    /// Evaluate every form in `src` in the current top-level environment.
    ///
    /// Forms are evaluated in order. When a form evaluates to a
    /// declaration (e.g. `($define! x 1)`), the declaration is applied and
    /// the environment it returns becomes the top-level environment for the
    /// following forms and later calls. Returns the value of the last form
    /// that is not a declaration, or `#inert`.
    ///
    /// Values returned to the host stay valid until the next call that
    /// evaluates code, unless pinned with [`Grift::pin`].
    pub fn eval(&mut self, src: &str) -> Result<Value, Error> {
        let forms = self
            .m
            .ctx
            .read_all(src)
            .map_err(|f| self.m.fault_to_error(&f))?;
        let pinned = self.m.pinned.len();
        self.m.pinned.extend(forms.iter().copied());
        let result = self.fold(&forms);
        self.m.pinned.truncate(pinned);
        result
    }

    fn fold(&mut self, forms: &[Value]) -> Result<Value, Error> {
        let mut last = Value::Inert;
        for &form in forms {
            let env = self.m.env_roots[TOP];
            let v = self.m.run(Ctl::Eval(form, env))?;
            if let Some(Object::Declaration(_)) = self.m.ctx.obj(v) {
                // The bare primitive treats its operand list as arguments.
                let args = self.m.ctx.list(&[v, Value::Obj(env)]);
                let apply = Value::Prim(prims::APPLY_DECLARATION);
                let new_env = self.m.run(Ctl::Apply(apply, args, env))?;
                self.m.env_roots[TOP] = new_env
                    .as_obj()
                    .expect("apply-declaration returns an environment");
                last = Value::Inert;
            } else {
                last = v;
            }
        }
        Ok(last)
    }

    /// The written representation of a value (as `write` produces it).
    pub fn write(&self, v: Value) -> String {
        self.m.ctx.write(v)
    }

    /// The displayed representation of a value (as `display` produces it).
    pub fn display(&self, v: Value) -> String {
        self.m.ctx.display(v)
    }

    /// Bind a native function, as an applicative, in a new child of the
    /// top-level environment. Its name must not start with `$` (natives are
    /// applicatives, and the sigil rule reserves `$` for operatives).
    pub fn register_native(&mut self, name: &str, f: NativeFn) -> Result<(), Error> {
        if name.starts_with('$') {
            return Err(Error::new(
                "sigil-violation",
                "native functions are applicatives; their names must not start with $",
            ));
        }
        let sym = self.m.ctx.sym(name);
        let native = self
            .m
            .ctx
            .heap
            .alloc(Object::Native(heap::Native { name: sym, f }));
        let app = self
            .m
            .ctx
            .heap
            .alloc(Object::Applicative(Value::Obj(native)));
        let env = self.m.env_roots[TOP];
        self.m.env_roots[TOP] = self.m.ctx.extend(env, [(sym, Some(Value::Obj(app)))]);
        Ok(())
    }

    /// Keep `v` alive across evaluations until it is unpinned.
    pub fn pin(&mut self, v: Value) {
        self.m.pinned.push(v);
    }

    /// Release one pin of `v`.
    pub fn unpin(&mut self, v: Value) {
        if let Some(i) = self.m.pinned.iter().rposition(|&p| p == v) {
            self.m.pinned.remove(i);
        }
    }

    /// Run a full garbage collection now.
    pub fn collect_garbage(&mut self) {
        self.m.ctx.heap.since_gc = usize::MAX;
        let _ = self.m.run(Ctl::Ret(Value::Inert));
    }

    /// Number of machine steps taken so far (always 0 unless the `stats`
    /// feature is enabled).
    pub fn steps(&self) -> u64 {
        self.m.steps
    }

    /// Heap occupancy statistics.
    pub fn heap_stats(&self) -> HeapStats {
        self.m.ctx.heap.stats()
    }

    /// The store, for building values to pass to natives or inspecting
    /// results.
    pub fn ctx(&mut self) -> &mut Ctx {
        &mut self.m.ctx
    }
}
