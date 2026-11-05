//! Symbol identity survives more names being interned, including names read from text.

#![allow(clippy::unwrap_used)]

use grift::{Grift, Value};
use std::collections::HashSet;

#[test]
fn interned_names_stay_distinct_and_read_back_after_more_interns() {
    let mut g: Grift = Grift::new().unwrap();
    let names: Vec<_> = (0..256).rev().map(|n| format!("name-λ-{n:03}")).collect();
    let symbols: Vec<_> = names.iter().map(|name| g.symbol(name).unwrap()).collect();
    assert_eq!(symbols.iter().collect::<HashSet<_>>().len(), names.len());

    for (name, symbol) in names.iter().zip(&symbols).rev() {
        assert_eq!(g.symbol(name).unwrap(), *symbol, "{name}");
        assert_eq!(g.symbol_name(symbol).unwrap(), name, "{name}");
        let forms = g.read(name).unwrap();
        assert_eq!(
            g.list_items(&forms).unwrap(),
            [Value::Symbol(symbol.clone())],
            "{name}"
        );
    }
}
