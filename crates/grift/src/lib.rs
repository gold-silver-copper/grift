#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! # grift
//!
//! A minimal, pure, type-strict Lisp built on John Shutt's vau calculus.

extern crate alloc;

mod ctx;
mod error;
mod heap;
mod print;
mod read;
mod symbol;
mod value;

pub use ctx::Ctx;
pub use error::Error;
pub use heap::HeapStats;
pub use value::{ObjId, PrimId, SymId, Value};

/// A native function: receives the store and its evaluated arguments.
pub type NativeFn = fn(&mut Ctx, &[Value]) -> Result<Value, Error>;

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn roundtrip(src: &str) -> alloc::string::String {
        let mut ctx = Ctx::default();
        let forms = ctx.read_all(src).map_err(|f| f.message).unwrap();
        forms
            .iter()
            .map(|&v| ctx.write(v))
            .collect::<alloc::vec::Vec<_>>()
            .join(" ")
    }

    #[test]
    fn reader_printer_roundtrip() {
        assert_eq!(
            roundtrip("(a b . c) () #t #f #inert #ignore -12 +7"),
            "(a b . c) () #t #f #inert #ignore -12 7"
        );
        assert_eq!(
            roundtrip(r#""a\"b\n" #\a #\space #\x41 #\("#),
            r#""a\"b\n" #\a #\space #\A #\("#
        );
        assert_eq!(
            roundtrip("; c\n #| nested #| x |# |# (1 #;(skip me) 2)"),
            "(1 2)"
        );
        assert_eq!(roundtrip("$vau foo-bar? <=?"), "$vau foo-bar? <=?");
    }

    #[test]
    fn reader_errors() {
        let mut ctx = Ctx::default();
        for bad in [
            "'x",
            "(1 2",
            ")",
            "#q",
            "(. a)",
            "99999999999999999999",
            "\"abc",
        ] {
            let e = ctx.read_all(bad).unwrap_err();
            assert_eq!(e.kind, "parse-error", "{bad}");
        }
    }
}
