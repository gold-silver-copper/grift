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
    assert!(run(&mut g, "(eq? (cons 1 2) (cons 1 2))").starts_with("ERR type-error"));
    assert_eq!(run(&mut g, "(eq? (cons 1 2) 3)"), "#f");
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

#[test]
fn gc_keeps_heap_bounded_under_sustained_allocation() {
    let mut g = Grift::new();
    g.eval(
        "($define! churn
           ($lambda (n)
             ($if (=? n 0)
                  #inert
                  ($sequence (list n n n n n n n n) (churn (- n 1))))))",
    )
    .unwrap();
    g.eval("(churn 200000)").unwrap();
    let after_first = g.heap_stats();
    g.eval("(churn 200000)").unwrap();
    let after_second = g.heap_stats();
    assert!(after_first.collections > 0);
    // Garbage is reclaimed: a second run of the same loop does not grow
    // the heap.
    assert_eq!(
        after_first.capacity, after_second.capacity,
        "{after_first:?} {after_second:?}"
    );
    assert!(after_second.capacity < 400_000, "{after_second:?}");
}

#[test]
fn gc_reclaims_unreachable_data_and_keeps_pinned_values() {
    let mut g = Grift::new();
    g.eval("($define! build ($lambda (n acc) ($if (=? n 0) acc (build (- n 1) (cons n acc)))))")
        .unwrap();
    let big = g.eval("(build 100000 ())").unwrap();
    g.pin(big);
    g.collect_garbage();
    let pinned = g.heap_stats().live;
    // The pinned list survives collection intact.
    let len = g.eval("(length (build 3 ()))").unwrap();
    assert_eq!(g.write(len), "3");
    assert!(g.write(big).starts_with("(1 2 3 4 5"));
    g.unpin(big);
    g.collect_garbage();
    let unpinned = g.heap_stats().live;
    assert!(unpinned + 99_000 < pinned, "{pinned} -> {unpinned}");
}

#[test]
fn natives_and_errors_from_natives() {
    fn double(ctx: &mut grift::Ctx, args: &[grift::Value]) -> Result<grift::Value, grift::Error> {
        match args {
            [grift::Value::Int(n)] => Ok(grift::Value::Int(n * 2)),
            [v] => Err(grift::Error::new(
                "type-error",
                format!("double: expected an integer, got {}", ctx.write(*v)),
            )),
            _ => Err(grift::Error::new(
                "arity-error",
                "double expects 1 argument",
            )),
        }
    }
    fn shout(ctx: &mut grift::Ctx, args: &[grift::Value]) -> Result<grift::Value, grift::Error> {
        let s = args
            .first()
            .and_then(|v| ctx.as_str(*v))
            .ok_or_else(|| grift::Error::new("type-error", "shout: expected a string"))?;
        let up = s.to_uppercase();
        Ok(ctx.string(&up))
    }
    let mut g = Grift::new();
    g.register_native("double", double).unwrap();
    g.register_native("shout", shout).unwrap();
    assert!(g.register_native("$bad", double).is_err());
    assert_eq!(run(&mut g, "(double 21)"), "42");
    assert_eq!(run(&mut g, "(map double (list 1 2 3))"), "(2 4 6)");
    assert_eq!(run(&mut g, r#"(shout "hi")"#), r#""HI""#);
    assert_eq!(
        run(&mut g, r#"(double "x")"#),
        r#"ERR type-error: double: expected an integer, got "x""#
    );
    assert_eq!(run(&mut g, "(applicative? double)"), "#t");
}

#[test]
fn top_level_declarations_thread_the_environment() {
    let mut g = Grift::new();
    assert_eq!(
        run(&mut g, "($define! x 1) ($define! y (+ x 1)) (list x y)"),
        "(1 2)"
    );
    // Redefinition shadows; closures made earlier keep the old binding.
    assert_eq!(
        run(
            &mut g,
            "($define! get-x ($lambda () x)) ($define! x 10) (list x (get-x))"
        ),
        "(10 1)"
    );
    // A failing form keeps the declarations before it.
    assert!(run(&mut g, "($define! z 5) (car 1) ($define! w 6)").starts_with("ERR"));
    assert_eq!(run(&mut g, "z"), "5");
    assert!(run(&mut g, "w").starts_with("ERR unbound-symbol"));
    assert_eq!(run(&mut g, ""), "#inert");
    assert_eq!(run(&mut g, "($define! q 1)"), "#inert");
}

#[test]
fn cloning_a_booted_interpreter_is_independent() {
    let mut a = Grift::new();
    a.eval("($define! x 1)").unwrap();
    let mut b = a.clone();
    b.eval("($define! x 2)").unwrap();
    assert_eq!(run(&mut a, "x"), "1");
    assert_eq!(run(&mut b, "x"), "2");
}
