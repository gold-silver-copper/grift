//! What two earned needs promise a host, through the public API alone:
//! - `Grift::read_in` reads a named text into its forms, evaluating nothing, and each
//!   combination keeps its source and line, so a loader written in grift that evaluates them
//!   later gets its errors placed there; a text that doesn't read is `read`'s `parse-error`;
//! - `grift::write_string` spells any text as the grift string literal `read-form` reads back,
//!   the one `Grift::write` prints, with no interpreter.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use grift::{Error, Grift, Value};

fn grift() -> Grift {
    Grift::new().expect("the library loads")
}

fn kind(g: &Grift, e: &Error) -> String {
    g.symbol_name(&e.kind).unwrap().to_string()
}

fn show(g: &Grift, v: &Value) -> String {
    let mut s = String::new();
    g.write(v, &mut s).unwrap();
    s
}

#[test]
fn read_in_gives_each_form_unevaluated_at_its_source_and_line() {
    let mut g = grift();
    let text = "(define! ran #true)\n\n(add 1\n  (multiply 2\n    3)) ; end\nran";
    let forms = g.read_in("loader.grift", text).unwrap();
    // The forms `read` gives, and none of them evaluated: the define! defined nothing.
    let unnamed = g.read(text).unwrap();
    assert_eq!(show(&g, &forms), show(&g, &unnamed));
    let ran = g.symbol("ran").unwrap();
    assert_eq!(g.lookup(&g.top_level(), &ran), Ok(None));
    // Each combination's source and the line of its `(`, inner ones too; an atom has none, and
    // nor does a form read with no name.
    let items = g.list_items(&forms).unwrap();
    assert_eq!(g.position(&items[0]), Ok(Some(("loader.grift", 1))));
    assert_eq!(g.position(&items[1]), Ok(Some(("loader.grift", 3))));
    let inner = g.list_items(&items[1]).unwrap();
    assert_eq!(g.position(&inner[2]), Ok(Some(("loader.grift", 4))));
    assert_eq!(g.position(&items[2]), Ok(None));
    let unnamed = g.list_items(&unnamed).unwrap();
    assert_eq!(g.position(&unnamed[0]), Ok(None));
}

#[test]
fn a_form_read_in_and_evaluated_later_fails_at_its_source_and_line() {
    let mut g = grift();
    let text = "(define! f\n  (lambda! (x)\n    (add 1 (first x))))\n\n(list 0\n  (f 1))";
    let forms = g.read_in("f.grift", text).unwrap();
    g.define("forms", forms).unwrap();
    // A loader in grift: each form evaluated in the program's own top level, in the program's
    // own run. The error is placed as evaluate_in would place it.
    let e = g
        .evaluate(
            &mut (),
            "(define! here (current-context))
             (for-each (lambda! (form) (evaluate form here)) forms)",
        )
        .unwrap_err();
    assert_eq!(kind(&g, &e), "type-error");
    let places: Vec<_> = e.trace().iter().map(|at| g.position(at).unwrap()).collect();
    assert_eq!(
        places[..3],
        [
            Some(("f.grift", 3)),
            Some(("f.grift", 3)),
            Some(("f.grift", 5))
        ],
        "{places:?}"
    );
}

#[test]
fn read_in_of_a_text_that_does_not_read_is_read_s_parse_error() {
    let mut g = grift();
    for text in [
        "(define! a 1)\n(a",
        "(define! a 1) \"ab\\q\"",
        "(a))",
        "\n\n  1.2.3",
        "(a . )",
        "#nope",
    ] {
        let want = g.read(text).unwrap_err();
        let e = g.read_in("bad.grift", text).unwrap_err();
        assert_eq!(kind(&g, &e), "parse-error", "{text:?}");
        // The values `read` gives for it: the reason, then the line and column.
        assert_eq!(e.values, want.values, "{text:?}");
        assert_eq!(e.values.len(), 3, "{text:?}");
    }
    // Nothing before the fault was evaluated.
    let a = g.symbol("a").unwrap();
    assert_eq!(g.lookup(&g.top_level(), &a), Ok(None));
}

/// A small fixed-seed set of texts: every escape the printer has, control characters, non-ASCII
/// space and letters, and the reader's delimiters, in strings of up to twelve characters.
fn texts() -> Vec<String> {
    let alphabet = [
        'a', 'Z', '0', ' ', '"', '\\', 'x', ';', '(', ')', '#', '.', '\n', '\r', '\t', '\u{0}',
        '\u{7}', '\u{1b}', '\u{7f}', '\u{80}', '\u{85}', '\u{9f}', '\u{a0}', 'é', 'λ', '日', '😀',
        '\u{200b}', '\u{feff}',
    ];
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut texts: Vec<String> = ["", "\\", "\"", "\\x41;", "\\n", "a\u{0}b", "\"\\\"\\\\\""]
        .map(String::from)
        .to_vec();
    for _ in 0..64 {
        let length = (next() % 13) as usize;
        texts.push(
            (0..length)
                .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
                .collect(),
        );
    }
    texts
}

#[test]
fn write_string_spells_any_text_as_read_form_reads_it_and_write_prints_it() {
    // Spelled with no interpreter: a host streams its own text as grift data.
    let spelled: Vec<(String, String)> = texts()
        .into_iter()
        .map(|text| {
            let mut written = String::new();
            grift::write_string(&text, &mut written).unwrap();
            (text, written)
        })
        .collect();
    let mut g = grift();
    for (text, written) in &spelled {
        // One literal, no control character raw in it, which read-form reads back as that
        // text, the whole of the literal being the string.
        assert!(
            !written.chars().any(char::is_control),
            "{text:?} as {written:?}"
        );
        let literal = g.string(written).unwrap();
        g.define("written", literal).unwrap();
        let read = g.evaluate(&mut (), "(read-form written 0)").unwrap();
        let (form, _start, end) = match g.list_items(&read).unwrap().as_slice() {
            [form, start, end] => (form.clone(), start.clone(), end.clone()),
            other => panic!("{text:?} as {written:?} read as {other:?}"),
        };
        assert_eq!(g.as_str(&form), Ok(Some(text.as_str())), "{written:?}");
        assert_eq!(end, Value::Integer(written.len() as i64), "{written:?}");
        // The same spelling Grift::write prints for that string.
        let string = g.string(text).unwrap();
        assert_eq!(&show(&g, &string), written);
    }
}
