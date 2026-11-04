//! The bytes value from the host's side, through `Grift::bytes` and `Grift::as_bytes`: a
//! reclaimed bytes value is `stale-value`, another interpreter's is `foreign-value`, and a live
//! reference of another kind under `Value::Bytes` is `stale-value`, as G-132 has it for strings.
//!
//! This uses the new methods, so on the base it fails to compile: `Grift` has no method named
//! `bytes` or `as_bytes`, and `Value` has no variant `Bytes`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use grift::{Error, Grift, Value};

fn kind(g: &Grift, e: &Error) -> String {
    g.symbol_name(&e.kind).unwrap().to_string()
}

/// The error kind `g` refuses `v` with, through `as_bytes` and as an argument of `bytes-length`.
fn refused(g: &mut Grift, v: &Value) -> Vec<String> {
    let through_as_bytes = match g.as_bytes(v) {
        Ok(b) => format!("ok {b:?}"),
        Err(e) => kind(g, &e),
    };
    let length = g.evaluate(&mut (), "bytes-length").unwrap();
    let through_a_primitive = match g.call(&mut (), &length, std::slice::from_ref(v)) {
        Ok(n) => format!("ok {n:?}"),
        Err(e) => kind(g, &e),
    };
    vec![through_as_bytes, through_a_primitive]
}

/// Runs the machine through several collections.
fn churn(g: &mut Grift) {
    g.evaluate(
        &mut (),
        "(length (map (lambda! (n) (list n)) (range 0 20000)))",
    )
    .unwrap();
}

#[test]
fn a_bytes_value_goes_in_and_out_and_is_refused_once_stale_or_foreign() {
    let mut g = Grift::new().unwrap();
    let all: Vec<u8> = (0..=255).collect();
    // In from a slice, out as the same slice, and the program sees the bytes kind.
    let v = g.bytes(&all).unwrap();
    assert_eq!(g.as_bytes(&v).unwrap(), Some(&all[..]));
    let empty = g.bytes(&[]).unwrap();
    assert_eq!(g.as_bytes(&empty).unwrap(), Some(&[][..]));
    g.define("all", v.clone()).unwrap();
    let seen = g
        .evaluate(
            &mut (),
            "(list (type-of all) (same? (bytes-to-list all) (range 0 256)))",
        )
        .unwrap();
    let mut text = String::new();
    g.write(&seen, &mut text).unwrap();
    assert_eq!(text, "(bytes #true)");
    // A value of another kind is no bytes: None, not an error.
    let s = g.string("abc").unwrap();
    assert_eq!(g.as_bytes(&s).unwrap(), None);
    assert_eq!(g.as_bytes(&Value::Integer(1)).unwrap(), None);
    // Held by nothing in the interpreter, then reclaimed: stale-value at every way in.
    g.define("all", Value::Inert).unwrap();
    churn(&mut g);
    assert_eq!(refused(&mut g, &v), ["stale-value", "stale-value"]);
    // Another interpreter's bytes value: foreign-value, even though its slot and generation match
    // one of ours made the same way.
    let mut theirs = Grift::new().unwrap();
    let foreign = theirs.bytes(&all).unwrap();
    let _ours = g.bytes(&all).unwrap();
    assert_eq!(
        refused(&mut g, &foreign),
        ["foreign-value", "foreign-value"]
    );
    assert_eq!(theirs.as_bytes(&foreign).unwrap(), Some(&all[..]));
    // A live string's reference under Value::Bytes, and a live bytes value's under Value::String:
    // no value of the interpreter's, so stale-value, never the other kind's storage.
    let live = g.bytes(&all).unwrap();
    let string = g.string("abc").unwrap();
    let (Value::Bytes(b), Value::String(s)) = (&live, &string) else {
        panic!("a bytes value is Value::Bytes and a string Value::String");
    };
    assert_eq!(
        refused(&mut g, &Value::Bytes(s.clone())),
        ["stale-value", "stale-value"]
    );
    assert_eq!(
        kind(&g, &g.as_str(&Value::String(b.clone())).unwrap_err()),
        "stale-value"
    );
    assert_eq!(g.as_bytes(&live).unwrap(), Some(&all[..]));
    assert_eq!(g.as_str(&string).unwrap(), Some("abc"));
}
