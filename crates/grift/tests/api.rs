//! Rust-level tests of the embedding API and the bare primitives.

use grift::Grift;

fn run(g: &mut Grift, src: &str) -> String {
    match g.eval(src) {
        Ok(v) => g.write(v),
        Err(e) => format!("ERR {e}"),
    }
}

#[test]
fn bare_primitives() {
    let mut g = Grift::new();
    assert_eq!(run(&mut g, "(($vau (x) #ignore x) foo)"), "foo");
    assert_eq!(run(&mut g, "(+ 1 2)"), "3");
    assert_eq!(
        run(&mut g, "((wrap ($vau (x y) #ignore (cons x y))) 1 (+ 1 1))"),
        "(1 . 2)"
    );
    assert_eq!(
        run(
            &mut g,
            "(eval (cons + (cons 1 (cons 2 ()))) (make-environment))"
        ),
        "3"
    );
    assert_eq!(
        run(
            &mut g,
            "(make-declaration (wrap ($vau (e) #ignore (environment-bind e (($vau (x) #ignore x) answer) 42))))"
        ),
        "#inert"
    );
    assert_eq!(run(&mut g, "answer"), "42");
    assert_eq!(run(&mut g, "(select #t 1 2)"), "1");
    assert_eq!(
        run(&mut g, "(select 0 1 2)"),
        "ERR type-error: expected a boolean, got integer 0"
    );
    assert!(run(&mut g, "($car 1)").starts_with("ERR unbound-symbol"));
    assert!(run(&mut g, "(eq? (cons 1 2) 3)").starts_with("ERR type-error"));
    assert!(run(&mut g, "(+ 9223372036854775807 1)").starts_with("ERR overflow"));
}

#[test]
fn sigil_rule_at_call_sites() {
    let mut g = Grift::new();
    g.eval("(make-declaration (wrap ($vau (e) #ignore (environment-bind e (($vau (x) #ignore x) quote) ($vau (x) #ignore x)))))").unwrap();
    assert!(run(&mut g, "(quote a)").starts_with("ERR sigil-violation"));
    g.eval("(make-declaration (wrap ($vau (e) #ignore (environment-bind e (($vau (x) #ignore x) $plus) +))))").unwrap();
    assert!(run(&mut g, "($plus 1 2)").starts_with("ERR sigil-violation"));
}

#[test]
fn recursive_binding_and_deep_tail_loop() {
    let mut g = Grift::new();
    let def = "(make-declaration (wrap ($vau (e) #ignore
        (environment-bind-recursive e (($vau (x) #ignore x) count)
          (($vau (x) #ignore x)
            (wrap ($vau (n acc) #ignore
              (eval (select (<? n 1) acc (cons count (cons (- n 1) (cons (+ acc 1) ())))) (make-environment)))))))))";
    assert_eq!(run(&mut g, def), "#inert");
    assert_eq!(run(&mut g, "(count 1000000 0)"), "1000000");
    let stats = g.heap_stats();
    assert!(stats.collections > 0, "{stats:?}");
}
