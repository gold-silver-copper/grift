//! `core::fmt::Display` implementation for Lisp values.
//!
//! Provides [`DisplayValue`], a wrapper that carries the arena context needed
//! to format a value. This enables standard formatting via `write!` and
//! `format!` (when `alloc` or `std` is available) and works in `no_std`
//! environments where only `core::fmt::Write` is needed.
//!
//! # Example
//!
//! ```
//! use grift_core::{Lisp, DisplayValue};
//!
//! let lisp = Lisp::<1000>::new();
//! let val = lisp.number(42).unwrap();
//! let dv = DisplayValue::new(val, &lisp);
//! ```

use core::fmt;
use grift_arena::ArenaIndex;
use crate::value::Value;
use crate::lisp::Lisp;

/// Maximum formatting depth to prevent infinite recursion on cyclic structures.
const MAX_DISPLAY_DEPTH: usize = 100;

/// Maximum list elements to display before truncating.
const MAX_LIST_ELEMENTS: usize = 100;

/// A wrapper that enables `core::fmt::Display` for Lisp values.
///
/// Because formatting a value requires access to the arena (to follow
/// cons-cell chains, read strings, etc.), this wrapper carries both the
/// value index and a reference to the [`Lisp`] context.
///
/// # Example
///
/// ```
/// use grift_core::{Lisp, DisplayValue};
///
/// let lisp = Lisp::<1000>::new();
/// let nil = lisp.nil().unwrap();
/// let dv = DisplayValue::new(nil, &lisp);
/// // In a no_std context, use core::fmt::Write:
/// // write!(some_writer, "{}", dv).ok();
/// ```
pub struct DisplayValue<'a, const N: usize> {
    value: ArenaIndex,
    lisp: &'a Lisp<N>,
}

impl<'a, const N: usize> DisplayValue<'a, N> {
    /// Create a new `DisplayValue` wrapper.
    #[inline]
    pub fn new(value: ArenaIndex, lisp: &'a Lisp<N>) -> Self {
        DisplayValue { value, lisp }
    }
}

impl<const N: usize> fmt::Display for DisplayValue<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_value(self.lisp, self.value, f, 0)
    }
}

// ============================================================================
// Formatting implementation
// ============================================================================

fn format_value<const N: usize>(
    lisp: &Lisp<N>,
    idx: ArenaIndex,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    if depth > MAX_DISPLAY_DEPTH {
        return f.write_str("...");
    }
    
    match lisp.get(idx) {
        Ok(Value::Nil) => f.write_str("()"),
        Ok(Value::Void) => f.write_str("#<void>"),
        Ok(Value::True) => f.write_str("#t"),
        Ok(Value::False) => f.write_str("#f"),
        Ok(Value::Number(n)) => write!(f, "{}", n),
        Ok(Value::Char(c)) => {
            f.write_str("#\\")?;
            match c {
                ' ' => f.write_str("space"),
                '\n' => f.write_str("newline"),
                '\t' => f.write_str("tab"),
                _ => write!(f, "{}", c),
            }
        }
        Ok(Value::Symbol(chars)) => format_symbol(lisp, chars, f),
        Ok(Value::Cons { .. }) => {
            f.write_str("(")?;
            format_list_contents(lisp, idx, f, depth + 1)?;
            f.write_str(")")
        }
        Ok(Value::Lambda { .. }) => f.write_str("#<lambda>"),
        Ok(Value::Builtin(b)) => {
            f.write_str("#<builtin:")?;
            f.write_str(b.name())?;
            f.write_str(">")
        }
        Ok(Value::StdLib(s)) => {
            f.write_str("#<stdlib:")?;
            f.write_str(s.name())?;
            f.write_str(">")
        }
        Ok(Value::Array { .. }) => {
            f.write_str("#(")?;
            let len = lisp.array_len(idx).unwrap_or(0);
            for i in 0..len {
                if i > 0 {
                    f.write_str(" ")?;
                }
                if let Ok(elem_idx) = lisp.array_get(idx, i) {
                    format_value(lisp, elem_idx, f, depth + 1)?;
                }
            }
            f.write_str(")")
        }
        Ok(Value::Bytevector { .. }) => {
            f.write_str("#u8(")?;
            let len = lisp.bytevector_len(idx).unwrap_or(0);
            for i in 0..len {
                if i > 0 {
                    f.write_str(" ")?;
                }
                if let Ok(elem_idx) = lisp.bytevector_get(idx, i) {
                    format_value(lisp, elem_idx, f, depth + 1)?;
                }
            }
            f.write_str(")")
        }
        Ok(Value::String { .. }) => {
            f.write_str("\"")?;
            let len = lisp.string_len(idx).unwrap_or(0);
            for i in 0..len {
                if let Ok(c) = lisp.string_char_at(idx, i) {
                    match c {
                        '"' => f.write_str("\\\"")?,
                        '\\' => f.write_str("\\\\")?,
                        '\n' => f.write_str("\\n")?,
                        '\t' => f.write_str("\\t")?,
                        _ => write!(f, "{}", c)?,
                    }
                }
            }
            f.write_str("\"")
        }
        Ok(Value::Native { .. }) => {
            let id = lisp.native_id(idx).unwrap_or(0);
            write!(f, "#<native:{}>", id)
        }
        Ok(Value::Ref(r)) => write!(f, "#<ref:{}>", r.raw()),
        Ok(Value::Usize(n)) => write!(f, "#<usize:{}>", n),
        Ok(Value::Syntax { expr, .. }) => {
            f.write_str("#<syntax:")?;
            format_value(lisp, expr, f, depth + 1)?;
            f.write_str(">")
        }
        Ok(Value::ContFrame { .. }) => f.write_str("#<cont-frame>"),
        Ok(Value::Continuation { .. }) => f.write_str("#<continuation>"),
        Err(_) => f.write_str("#<error>"),
    }
}

fn format_list_contents<const N: usize>(
    lisp: &Lisp<N>,
    mut idx: ArenaIndex,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    let mut first = true;
    let mut count = 0;
    
    loop {
        if count > MAX_LIST_ELEMENTS {
            return f.write_str(" ...");
        }
        
        match lisp.get(idx) {
            Ok(Value::Nil) => break,
            Ok(Value::Cons { .. }) => {
                if !first {
                    f.write_str(" ")?;
                }
                first = false;
                let (car, cdr) = lisp.car_cdr(idx).unwrap_or((ArenaIndex::NIL, ArenaIndex::NIL));
                format_value(lisp, car, f, depth)?;
                idx = cdr;
                count += 1;
            }
            Ok(_) => {
                // Improper list (dotted pair)
                f.write_str(" . ")?;
                return format_value(lisp, idx, f, depth);
            }
            Err(_) => {
                return f.write_str(" . #<error>");
            }
        }
    }
    Ok(())
}

fn format_symbol<const N: usize>(
    lisp: &Lisp<N>,
    chars: ArenaIndex,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let len = lisp.string_len(chars).unwrap_or(0);
    for i in 0..len {
        if i > 64 {
            return f.write_str("...");
        }
        if let Ok(c) = lisp.string_char_at(chars, i) {
            write!(f, "{}", c)?;
        }
    }
    Ok(())
}
