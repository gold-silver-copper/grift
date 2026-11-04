#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use grift::{Grift, Value};

fn evaluate(g: &mut Grift, program: &str) -> Value {
    g.evaluate(&mut (), program).unwrap_or_else(|error| {
        let mut message = String::new();
        g.describe(&error, &mut message).unwrap();
        panic!("{message}");
    })
}

#[test]
fn every_byte_survives_printing_and_reading() {
    let mut g = Grift::new().unwrap();
    let decimal = (0..=255)
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let program = format!(
        r##"
        (define! original #bytes({decimal}))
        (define! printed (value-to-string original))
        (define! (restored start end) (read-form printed 0))
        (and!?
          (same? (type-of original) (quote! bytes))
          (same? (type-of restored) (quote! bytes))
          (string-starts-with? printed "#bytes(")
          (string-ends-with? printed ")")
          (same? end (string-byte-length printed))
          (same? restored original)
          (equal? (bytes-to-list original) (range 0 256))
          (equal? (bytes-to-list restored) (range 0 256)))
        "##
    );
    assert_eq!(evaluate(&mut g, &program), Value::Boolean(true));
}

#[test]
fn invalid_utf8_reports_the_original_bytes_and_byte_offset() {
    // Multibyte prefixes distinguish byte offsets from character counts; incomplete and
    // malformed sequences must name their start, not the later byte that exposed the error.
    let cases: &[&[u8]] = &[
        b"\0\xc3\xa9\xffZ",
        b"\xc3\xa9\xe2(\xa1",
        b"A\xf0\x9f\x98",
        b"\xed\xa0\x80",
    ];
    for input in cases {
        let offset = std::str::from_utf8(input).unwrap_err().valid_up_to();
        let decimal = input
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        let mut g = Grift::new().unwrap();
        let error = g
            .evaluate(
                &mut (),
                &format!("(bytes-to-string (list-to-bytes (quote! ({decimal}))))"),
            )
            .expect_err("bytes-to-string must refuse invalid UTF-8");
        let mut message = String::new();
        g.describe(&error, &mut message).unwrap();
        assert!(
            g.is_kind(&error, "encoding-error").unwrap(),
            "bytes-to-string must report encoding-error: {message}"
        );

        let values = g.list(&error.values).unwrap();
        g.define("failure-values", values).unwrap();
        let property = format!(
            "(and!?
               (same? (length failure-values) 3)
               (same? (first failure-values) (unwrap bytes-to-string))
               (same? (type-of (second failure-values)) (quote! bytes))
               (equal? (bytes-to-list (second failure-values)) (quote! ({decimal})))
               (same? (third failure-values) {offset}))"
        );
        assert_eq!(
            evaluate(&mut g, &property),
            Value::Boolean(true),
            "wrong encoding-error values for {input:?}: {message}"
        );
    }
}
