//! The bytes value, through programs given to the interpreter as text:
//! - a bytes value read back from its printed form is the same bytes, for all 256 byte values;
//! - converting bytes that aren't UTF-8 to a string is refused, with the original bytes and the
//!   place (`str::from_utf8`'s `valid_up_to`).
//!
//! These compile on the base and fail at run time: the base has no bytes value, so `list-to-bytes`
//! is an `unbound-symbol` and `#bytes(` is `parse-error unknown-constant`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use grift::{Error, Grift};

/// Runs `src` and gives what it evaluates to, written, or its error described.
fn run(g: &mut Grift, src: &str) -> Result<String, String> {
    let mut text = String::new();
    match g.evaluate(&mut (), src) {
        Ok(v) => {
            g.write(&v, &mut text).unwrap();
            Ok(text)
        }
        Err(e) => {
            g.describe(&e, &mut text).unwrap();
            Err(text)
        }
    }
}

/// The result of `src`, which must evaluate without an error.
fn ok(g: &mut Grift, src: &str) -> String {
    run(g, src).unwrap_or_else(|e| panic!("{src}\n  failed: {e}"))
}

#[test]
fn a_bytes_value_reads_back_from_its_printed_form() {
    let mut g = Grift::new().unwrap();
    let all: Vec<String> = (0..=255).map(|b| b.to_string()).collect();
    let literal = format!("#bytes({})", all.join(" "));
    // Made from the list of every byte value, printed, and read back by the program reader: the
    // same bytes, of the bytes kind, and the same value the literal in a program's text reads as.
    let program = format!(
        "(define! all (list-to-bytes (range 0 256)))
         (define! text (value-to-string all))
         (define! back (first (read-form text 0)))
         (list (same? back all)
               (same? (bytes-to-list back) (range 0 256))
               (type-of back)
               (same? {literal} all)
               (same? (first (read-form \"#bytes()\" 0)) (list-to-bytes ()))
               (string-starts-with? text \"#bytes(\"))"
    );
    assert_eq!(
        ok(&mut g, &program),
        "(#true #true bytes #true #true #true)"
    );
    // Printed and read back twice more, still the same bytes: the printed form is the literal.
    let again = "(define! twice (first (read-form (value-to-string (first (read-form (value-to-string all) 0))) 0)))
                 (same? twice all)";
    assert_eq!(ok(&mut g, again), "#true");
}

#[test]
fn bytes_that_are_not_utf8_are_refused_as_a_string_with_the_place() {
    let mut g = Grift::new().unwrap();
    // Each, as a list of integers, with what Rust says of it.
    let inputs: [&[u8]; 6] = [
        &[97, 255],
        &[104, 105, 0xC0, 0x80],
        &[0xE2, 0x82],
        &[0xED, 0xA0, 0x80],
        &[0xF4, 0x90, 0x80, 0x80],
        &[0, 0xEF, 0xBB, 0xBF, 0xC3, 0xA9, 0x80],
    ];
    for bytes in inputs {
        let at = std::str::from_utf8(bytes).unwrap_err().valid_up_to();
        let list: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
        let list = list.join(" ");
        // The error is encoding-error, naming the primitive, carrying the original bytes (the same
        // value) and the offset of the first byte that doesn't start a valid sequence.
        let program = format!(
            "(define! b (list-to-bytes (list {list})))
             (catch! (bytes-to-string b) e
               (let! ((vs (evaluate-in! values e)))
                 (list (evaluate-in! kind e)
                       (length vs)
                       (same? (first vs) (unwrap bytes-to-string))
                       (same? (second vs) b)
                       (third vs))))"
        );
        assert_eq!(
            ok(&mut g, &program),
            format!("(encoding-error 3 #true #true {at})"),
            "for the bytes {list}"
        );
    }
    // Valid UTF-8 goes both ways, as text, whatever it holds: NUL and a byte-order mark too.
    let program = "(define! s (string-append (code-point-to-string 0) (code-point-to-string 65279) \"h\\xe9;llo \\x1F600;\"))
                   (list (same? (bytes-to-string (string-to-bytes s)) s)
                         (same? (bytes-to-list (string-to-bytes \"\\xe9;\")) (list 195 169))
                         (same? (bytes-to-string (list-to-bytes ())) \"\"))";
    assert_eq!(ok(&mut g, program), "(#true #true #true)");
    // A string is never bytes: converting it is a type-error, never a silent reading of its text.
    let r = g.evaluate(&mut (), "(bytes-to-string \"abc\")");
    let kind = |g: &Grift, e: &Error| g.symbol_name(&e.kind).unwrap().to_string();
    match r {
        Err(e) => assert_eq!(kind(&g, &e), "type-error"),
        Ok(v) => panic!("a string converted as bytes: {v:?}"),
    }
}
