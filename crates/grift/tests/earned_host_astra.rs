//! Named reading preserves forms and their places; host strings use the language's spelling.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use grift::{Grift, Value};

#[test]
fn read_in_returns_all_forms_without_evaluating_them() {
    let mut g: Grift = Grift::new().unwrap();
    let equal = g.evaluate(&mut (), "equal?").unwrap();
    g.define("changed", Value::Integer(0)).unwrap();
    let changed = g.symbol("changed").unwrap();
    let text = "(define! changed 1)\n42\nunbound\n() ; last form";

    let forms = g.read_in("definitions.grift", text).unwrap();
    assert_eq!(g.list_items(&forms).unwrap().len(), 4);
    assert_eq!(
        g.lookup(&g.top_level(), &changed).unwrap(),
        Some(Value::Integer(0)),
        "reading must not perform the definition"
    );
    let unnamed = g.read(text).unwrap();
    assert_eq!(
        g.call(&mut (), &equal, &[forms, unnamed]).unwrap(),
        Value::Boolean(true),
        "naming the text must not change its forms"
    );
    assert_eq!(
        g.read_in("empty.grift", " ; no forms").unwrap(),
        Value::EmptyList
    );
}

#[test]
fn read_in_places_each_combination_at_its_source_and_opening_line() {
    let mut g: Grift = Grift::new().unwrap();
    let source = "sources/λ.grift";
    let forms = g
        .read_in(source, "; heading\n(list\n  (add 1 2))\n\n(first 0)")
        .unwrap();
    let forms = g.list_items(&forms).unwrap();
    assert_eq!(forms.len(), 2);
    let outer = g.list_items(&forms[0]).unwrap();

    // A later named read must not replace the earlier forms' source.
    let other = g.read_in("other.grift", "\n\n(list 9)").unwrap();
    let other = g.list_items(&other).unwrap();
    for (form, line) in [(&forms[0], 2), (&outer[1], 3), (&forms[1], 5)] {
        assert_eq!(g.position(form).unwrap(), Some((source, line)));
    }
    assert_eq!(g.position(&other[0]).unwrap(), Some(("other.grift", 3)));
}

#[test]
fn evaluating_a_read_in_form_places_its_error_at_the_original_source() {
    let mut g: Grift = Grift::new().unwrap();
    let evaluate = g.evaluate(&mut (), "evaluate").unwrap();
    let source = "calculation.grift";
    let forms = g.read_in(source, "\n(add 1\n  (first 0))").unwrap();
    let forms = g.list_items(&forms).unwrap();

    let error = g
        .call(&mut (), &evaluate, &[forms[0].clone(), g.top_level()])
        .unwrap_err();
    assert!(g.is_kind(&error, "type-error").unwrap());
    let at = error.trace().first().unwrap();
    assert_eq!(g.position(at).unwrap(), Some((source, 3)));
}

#[test]
fn read_in_preserves_reads_parse_error_kind_reason_line_and_column() {
    let mut g: Grift = Grift::new().unwrap();
    for text in [
        "; heading\n(a\n  (b",
        "α\n  \"unfinished",
        "α )",
        "\n(a . )",
        "\n\"a\\qb\"",
        "\n1.2.3",
        "\n#unknown",
        "\n#bytes(256)",
    ] {
        let unnamed = g.read(text).unwrap_err();
        let named = g.read_in("broken.grift", text).unwrap_err();
        assert!(g.is_kind(&unnamed, "parse-error").unwrap(), "{text:?}");
        assert!(g.is_kind(&named, "parse-error").unwrap(), "{text:?}");
        assert_eq!(named.values, unnamed.values, "{text:?}");
    }
}

fn strings() -> Vec<String> {
    let mut texts = vec![
        String::new(),
        "\"\\;()\n\r\t\0a\u{1}f".into(),
        (0..=0x9f).filter_map(char::from_u32).collect(),
        "é e\u{301} λ 中 🦀 \u{2028}\u{2029}\u{feff}\u{10ffff}".into(),
    ];
    let mut seed = 0x6d2b_79f5_u32;
    for length in 0..64 {
        let mut text = String::new();
        for _ in 0..length % 33 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            if let Some(c) = char::from_u32(seed % 0x11_0000) {
                text.push(c);
            }
        }
        texts.push(text);
    }
    texts
}

#[test]
fn write_string_round_trips_through_read_form() {
    let mut g: Grift = Grift::new().unwrap();
    let read_form = g.evaluate(&mut (), "read-form").unwrap();
    for text in strings() {
        let mut written = String::new();
        grift::write_string(&text, &mut written).unwrap();
        let input = g.string(&written).unwrap();
        let read = g
            .call(&mut (), &read_form, &[input, Value::Integer(0)])
            .unwrap();
        let fields = g.list_items(&read).unwrap();
        assert_eq!(fields.len(), 3, "{text:?}");
        assert_eq!(
            g.as_str(&fields[0]).unwrap(),
            Some(text.as_str()),
            "{text:?}"
        );
        assert_eq!(fields[1], Value::Integer(0), "{text:?}");
        assert_eq!(
            fields[2],
            Value::Integer(i64::try_from(written.len()).unwrap()),
            "the whole output must be one string literal: {text:?}"
        );
    }
}

#[test]
fn write_string_agrees_with_writing_the_same_interpreter_string() {
    let mut g: Grift = Grift::new().unwrap();
    for text in strings() {
        let mut written = String::new();
        grift::write_string(&text, &mut written).unwrap();
        let value = g.string(&text).unwrap();
        let mut from_value = String::new();
        g.write(&value, &mut from_value).unwrap();
        assert_eq!(written, from_value, "{text:?}");
    }
}
