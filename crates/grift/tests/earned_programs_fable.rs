//! A guard for what interning and the reader promise already, stated nowhere else: a symbol
//! stays itself among many, whatever its spelling, and the reader's corners read the same.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use grift::{Grift, Value};
use std::collections::HashSet;

#[test]
fn symbols_stay_themselves_among_many_and_the_reader_s_corners_read_the_same() {
    let mut g: Grift = Grift::new().expect("the library loads");
    let names: Vec<String> = (0..5000)
        .map(|i| format!("name-{i}"))
        .chain(
            ["é", "日本語", "名前-1", "λ", "-", "a-b", "a", "ab", "b", "name-"].map(String::from),
        )
        .collect();
    let symbols: Vec<_> = names.iter().map(|n| g.symbol(n).unwrap()).collect();
    // Each names itself, is the same symbol when interned again, and is no other.
    for (name, s) in names.iter().zip(&symbols) {
        assert_eq!(g.symbol_name(s), Ok(name.as_str()));
        assert_eq!(g.symbol(name).unwrap(), *s);
    }
    let distinct: HashSet<_> = symbols.iter().collect();
    assert_eq!(distinct.len(), symbols.len());
    // The reader gives those symbols for the names, with non-ASCII space between them, and a
    // comment ending the text with no newline, non-ASCII in it or not, reads as nothing.
    for text in ["日本語\u{a0}é", "日本語 é ; end", "日本語\té; é", ";\n日本語 ; \n é ;"] {
        let forms = g.read(text).unwrap();
        let want = [
            Value::Symbol(g.symbol("日本語").unwrap()),
            Value::Symbol(g.symbol("é").unwrap()),
        ];
        assert_eq!(g.list_items(&forms).unwrap(), want, "{text:?}");
    }
    assert_eq!(g.read("; only a comment"), Ok(Value::EmptyList));
    assert_eq!(g.read("; é"), Ok(Value::EmptyList));
}
