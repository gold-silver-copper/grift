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
use crate::{LIMB_BITS, MAX_LIMBS};

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
    /// When true, uses Scheme `display` semantics (no quotes around strings,
    /// characters printed as-is). When false, uses `write` semantics (the default).
    display_mode: bool,
}

impl<'a, const N: usize> DisplayValue<'a, N> {
    /// Create a new `DisplayValue` wrapper (uses `write` semantics by default).
    pub fn new(value: ArenaIndex, lisp: &'a Lisp<N>) -> Self {
        DisplayValue { value, lisp, display_mode: false }
    }

    /// Create a new `DisplayValue` wrapper using Scheme `display` semantics.
    ///
    /// In display mode, strings are printed without surrounding quotes and
    /// without escape sequences, and characters are printed as-is without the
    /// `#\` prefix (per R7RS §6.13.3).
    pub fn new_display(value: ArenaIndex, lisp: &'a Lisp<N>) -> Self {
        DisplayValue { value, lisp, display_mode: true }
    }
}

impl<const N: usize> fmt::Display for DisplayValue<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_value(self.lisp, self.value, f, 0, self.display_mode)
    }
}

// ============================================================================
// Formatting implementation
// ============================================================================

fn format_sequence<const N: usize, F>(
    lisp: &Lisp<N>,
    prefix: &str,
    len: usize,
    get_elem: F,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
    display_mode: bool,
) -> fmt::Result
where
    F: Fn(usize) -> Result<ArenaIndex, grift_arena::ArenaError>,
{
    f.write_str(prefix)?;
    for i in 0..len {
        if i > 0 {
            f.write_str(" ")?;
        }
        if let Ok(elem_idx) = get_elem(i) {
            format_value(lisp, elem_idx, f, depth + 1, display_mode)?;
        }
    }
    f.write_str(")")
}

fn format_value<const N: usize>(
    lisp: &Lisp<N>,
    idx: ArenaIndex,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
    display_mode: bool,
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
        Ok(Value::Float(fl)) => {
            // R7RS: inexact numbers display with decimal point
            if fl.is_nan() {
                f.write_str("+nan.0")
            } else if fl.is_infinite() {
                if fl > 0.0 {
                    f.write_str("+inf.0")
                } else {
                    f.write_str("-inf.0")
                }
            } else if fl.is_finite() && fl == (fl as isize as crate::fsize) {
                // Whole number float: display with ".0"
                write!(f, "{:.1}", fl)
            } else {
                write!(f, "{}", fl)
            }
        }
        Ok(Value::Rational { num, denom }) => write!(f, "{}/{}", num, denom),
        Ok(Value::Complex { real, imag }) => {
            // Format real part with R7RS conventions
            fn fmt_fsize(f: &mut core::fmt::Formatter<'_>, v: crate::fsize) -> core::fmt::Result {
                if v.is_nan() {
                    f.write_str("+nan.0")
                } else if v.is_infinite() {
                    if v > 0.0 { f.write_str("+inf.0") } else { f.write_str("-inf.0") }
                } else if v.is_finite() && v == (v as isize as crate::fsize) {
                    write!(f, "{:.1}", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            fmt_fsize(f, real)?;
            // Add '+' only for finite non-negative values; inf/nan already include sign
            if !imag.is_nan() && !imag.is_infinite() && imag >= 0.0 {
                f.write_str("+")?;
            }
            fmt_fsize(f, imag)?;
            f.write_str("i")
        }
        Ok(Value::BigNum { len, data, negative }) => {
            // Display bignum as decimal
            if len == 0 {
                return f.write_str("0");
            }
            // Collect limbs from arena
            let mut limbs = [0usize; MAX_LIMBS];
            let limb_count = if len > MAX_LIMBS { MAX_LIMBS } else { len };
            for i in 0..limb_count {
                if let Ok(limb_idx) = lisp.arena_index_at_offset(data, i) {
                    if let Ok(Value::Usize(v)) = lisp.get(limb_idx) {
                        limbs[i] = v;
                    }
                }
            }
            if negative {
                f.write_str("-")?;
            }
            // Convert to decimal using repeated division
            // Work with a copy of limbs
            let mut work = [0usize; MAX_LIMBS];
            work[..limb_count].copy_from_slice(&limbs[..limb_count]);
            let mut work_len = limb_count;
            // Trim leading zeros
            while work_len > 0 && work[work_len - 1] == 0 {
                work_len -= 1;
            }
            if work_len == 0 {
                return f.write_str("0");
            }
            // Collect decimal digits in reverse
            let mut digits = [0u8; 1300]; // enough for 2^4096
            let mut dlen = 0;
            while work_len > 0 {
                // Divide work by 10, get remainder
                let mut rem: u128 = 0;
                for i in (0..work_len).rev() {
                    let cur = rem * (1u128 << LIMB_BITS) + work[i] as u128;
                    work[i] = (cur / 10) as usize;
                    rem = cur % 10;
                }
                digits[dlen] = rem as u8;
                dlen += 1;
                // Trim leading zeros
                while work_len > 0 && work[work_len - 1] == 0 {
                    work_len -= 1;
                }
            }
            // Write digits in correct order (most significant first)
            for i in (0..dlen).rev() {
                let c = (b'0' + digits[i]) as char;
                write!(f, "{}", c)?;
            }
            Ok(())
        }
        Ok(Value::Char(c)) => {
            if display_mode {
                // display mode: print character as-is (R7RS §6.13.3)
                write!(f, "{}", c)
            } else {
                // write mode: use #\ prefix with named chars
                f.write_str("#\\")?;
                match c {
                    ' ' => f.write_str("space"),
                    '\n' => f.write_str("newline"),
                    '\t' => f.write_str("tab"),
                    _ => write!(f, "{}", c),
                }
            }
        }
        Ok(Value::Symbol(chars)) => {
            if display_mode {
                format_symbol(lisp, chars, f)
            } else {
                format_symbol_write(lisp, chars, f)
            }
        }
        Ok(Value::Cons { .. }) => {
            f.write_str("(")?;
            format_list_contents(lisp, idx, f, depth + 1, display_mode)?;
            f.write_str(")")
        }
        Ok(Value::Lambda { .. }) => f.write_str("#<lambda>"),
        Ok(Value::Builtin(b)) => write!(f, "#<builtin:{}>", b.name()),
        Ok(Value::StdLib(s)) => write!(f, "#<stdlib:{}>", s.name()),
        Ok(Value::Array { .. }) => {
            let len = lisp.array_len(idx).unwrap_or(0);
            format_sequence(lisp, "#(", len, |i| lisp.array_get(idx, i), f, depth, display_mode)
        }
        Ok(Value::Bytevector { .. }) => {
            let len = lisp.bytevector_len(idx).unwrap_or(0);
            format_sequence(lisp, "#u8(", len, |i| lisp.bytevector_get(idx, i), f, depth, display_mode)
        }
        Ok(Value::String { .. }) => {
            if display_mode {
                // display mode: print string contents without quotes or escaping (R7RS §6.13.3)
                let len = lisp.string_len(idx).unwrap_or(0);
                for i in 0..len {
                    if let Ok(c) = lisp.string_char_at(idx, i) {
                        write!(f, "{}", c)?;
                    }
                }
                Ok(())
            } else {
                // write mode: print with quotes and escaping
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
        }
        Ok(Value::Native { .. }) => {
            let id = lisp.native_id(idx).unwrap_or(0);
            write!(f, "#<native:{}>", id)
        }
        Ok(Value::Ref(r)) => write!(f, "#<ref:{}>", r.raw()),
        Ok(Value::Usize(n)) => write!(f, "#<usize:{}>", n),
        Ok(Value::Syntax { expr, .. }) => {
            f.write_str("#<syntax:")?;
            format_value(lisp, expr, f, depth + 1, display_mode)?;
            f.write_str(">")
        }
        Ok(Value::ContFrame { .. }) => f.write_str("#<cont-frame>"),
        Ok(Value::ContType(_)) => f.write_str("#<cont-type>"),
        Ok(Value::Continuation { .. }) => f.write_str("#<continuation>"),
        Ok(Value::ErrorObject { .. }) => f.write_str("#<error-object>"),
        Ok(Value::Port(port_id)) => write!(f, "#<port:{}>", port_id.0),
        Ok(Value::Eof) => f.write_str("#<eof>"),
        Ok(Value::Environment { .. }) => f.write_str("#<environment>"),
        Err(_) => f.write_str("#<error>"),
    }
}

fn format_list_contents<const N: usize>(
    lisp: &Lisp<N>,
    mut idx: ArenaIndex,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
    display_mode: bool,
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
                format_value(lisp, car, f, depth, display_mode)?;
                idx = cdr;
                count += 1;
            }
            Ok(_) => {
                // Improper list (dotted pair)
                f.write_str(" . ")?;
                return format_value(lisp, idx, f, depth, display_mode);
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

/// Format a symbol in `write` mode (R7RS §7.1.1).
/// Symbols that need escaping are written with |...| delimiters.
fn format_symbol_write<const N: usize>(
    lisp: &Lisp<N>,
    chars: ArenaIndex,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let len = lisp.string_len(chars).unwrap_or(0);

    // Empty symbol
    if len == 0 {
        return f.write_str("||");
    }

    // Check if symbol needs quoting
    let needs_quoting = symbol_needs_quoting(lisp, chars, len);

    if !needs_quoting {
        // Output directly without delimiters
        return format_symbol(lisp, chars, f);
    }

    // Output with |...| delimiters, escaping | and \ within
    f.write_str("|")?;
    for i in 0..len {
        if i > 64 {
            f.write_str("...")?;
            break;
        }
        if let Ok(c) = lisp.string_char_at(chars, i) {
            match c {
                '|' => f.write_str("\\|")?,
                '\\' => f.write_str("\\\\")?,
                _ => write!(f, "{}", c)?,
            }
        }
    }
    f.write_str("|")
}

/// Check if a symbol name needs |...| quoting in write mode.
fn symbol_needs_quoting<const N: usize>(
    lisp: &Lisp<N>,
    chars: ArenaIndex,
    len: usize,
) -> bool {
    if len == 0 {
        return true;
    }

    // Check first character
    if let Ok(first) = lisp.string_char_at(chars, 0) {
        // Symbols starting with digits need quoting
        if first.is_ascii_digit() {
            return true;
        }
        // Special initial characters that look like numbers
        if (first == '+' || first == '-' || first == '.') && len > 1 {
            if let Ok(second) = lisp.string_char_at(chars, 1) {
                if second.is_ascii_digit() || second == 'i' || second == 'n'
                    || second == 'I' || second == 'N'
                {
                    return true;
                }
            }
        }
        // A lone dot needs quoting since . is special in S-expressions
        if first == '.' && len == 1 {
            return true;
        }
    }

    // Check all characters for special characters needing quoting
    for i in 0..len {
        if let Ok(c) = lisp.string_char_at(chars, i) {
            match c {
                ' ' | '\t' | '\n' | '\r' | '(' | ')' | '[' | ']' | '{' | '}' |
                '"' | ',' | '\'' | '`' | ';' | '#' | '|' | '\\' => return true,
                _ => {}
            }
        }
    }

    false
}
