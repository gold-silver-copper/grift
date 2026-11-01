#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::{Error, Grift, Value};
use alloc::format;
use alloc::string::String;

fn grift() -> Grift {
    Grift::new().expect("the library loads")
}

fn show(g: &Grift, v: Value) -> String {
    let mut s = String::new();
    g.write(v, &mut s).unwrap();
    s
}

fn code(g: &mut Grift, e: &Error) -> String {
    show(g, Value::Sym(e.code))
}

/// Each source evaluates to `#t`.
fn holds(g: &mut Grift, cases: &[&str]) {
    for src in cases {
        match g.eval(src) {
            Ok(Value::True) => {}
            Ok(v) => panic!("{src} gave {}", show(g, v)),
            Err(e) => panic!("{src} failed: {}", code(g, &e)),
        }
    }
}

/// Each source fails with the given code.
fn fails(g: &mut Grift, cases: &[(&str, &str)]) {
    for (src, want) in cases {
        match g.eval(src) {
            Ok(v) => panic!("{src} gave {}", show(g, v)),
            Err(e) => assert_eq!(code(g, &e), *want, "{src}"),
        }
    }
}

#[test]
fn primitives() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            "(equal? (($vau (x) #ignore x) (a b)) (car ($quote ((a b)))))",
            "(equal? (($vau (x . y) e (list x y)) 1 2 3) ($quote (1 (2 3))))",
            "(eq? (type-of (($vau () e e))) ($quote environment))",
            "(eq? ((wrap ($vau (x) #ignore x)) (+ 1 2)) 3)",
            "(equal? ((unwrap list) (+ 1 2)) ($quote ((+ 1 2))))",
            "(eq? (eval ($quote (+ 1 2)) (make-environment ground-environment)) 3)",
            "($sequence ($define! e (make-environment)) (equal? (bindings e) ()))",
            "($sequence ($define! e (make-environment)) (bind! e ($quote (a (b))) (list 1 (list 2))) \
             (equal? (list (eval ($quote a) e) (bindings e)) ($quote (1 (a b)))))",
            "(equal? (cons 1 2) ($quote (1 . 2)))",
            "(eq? (car (cons 1 2)) 1)",
            "(eq? (cdr (cons 1 2)) 2)",
            "(eq? \"ab\" \"ab\")",
            "(not? (eq? (cons 1 2) (cons 1 2)))",
            "(eq? (select #t 1 2) 1)",
            "(eq? (select #f 1 2) 2)",
            "(equal? (map type-of (list () #t #inert #ignore 1 ($quote a) (cons 1 2) \"s\" car $if)) \
             ($quote (null boolean inert ignore integer symbol pair string applicative operative)))",
            "(eq? (+ 2 3) 5)",
            "(eq? (- 2 3) -1)",
            "(eq? (* -4 3) -12)",
            "(equal? (list (div 7 2) (div -7 2) (div 7 -2) (div -7 -2)) ($quote (3 -4 -4 3)))",
            "($and? (<? 1 2) (not? (<? 2 1)) (not? (<? 1 1)))",
        ],
    );
}

#[test]
fn library() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            "(eq? ($quote a) ($quote a))",
            "(equal? (list 1 (+ 1 1)) ($quote (1 2)))",
            "($sequence ($define! (a . b) (list 1 2)) (equal? (list a b) ($quote (1 (2)))))",
            "(eq? ($if #t 1 (car ())) 1)",
            "(eq? ($sequence) #inert)",
            "(eq? ($sequence 1 2 3) 3)",
            "(eq? (($lambda (x y) (* x y)) 6 7) 42)",
            "(eq? (($lambda (x) ($define! y (+ x 1)) (* y 2)) 1) 4)",
            "(eq? ($let ((x 2) ((y z) (list 3 4))) (+ x (* y z))) 14)",
            "(eq? ($cond (#f 1) ((eq? 1 1) 2 3) (#t (car ()))) 3)",
            "(eq? ($cond (#f 1)) #inert)",
            "($and? ($and?) (not? ($and? #t #f)) (eq? ($and? #t 5) 5) (not? ($and? #f (car ()))))",
            "($and? (not? ($or?)) ($or? #f #t) (eq? ($or? #f 5) 5) ($or? #t (car ())))",
            "($and? (not? #f) (not? (not? #t)))",
            "($and? (=? 3 3) (not? (=? 3 4)))",
            "(equal? (apply list (list 1 2)) ($quote (1 2)))",
            "(equal? (apply ($lambda (x) (* x x)) (list 5)) 25)",
            "(equal? (map ($lambda (x) (* x x)) (list 1 2 3)) ($quote (1 4 9)))",
            "(equal? (append (list 1) () (list 2 3) 4) ($quote (1 2 3 . 4)))",
            "(eq? (append) ())",
            "($and? (eq? (length ()) 0) (eq? (length (list 1 2 3)) 3))",
            "($and? (equal? \"a\" \"a\") (equal? (list 1 (list 2)) (list 1 (list 2))) \
             (not? (equal? (list 1) (list 2))))",
        ],
    );
}

#[test]
fn error_codes() {
    let mut g = grift();
    fails(
        &mut g,
        &[
            ("nope", "unbound-symbol"),
            ("(car 1)", "type-error"),
            ("(+ 1 \"a\")", "type-error"),
            ("($if 1 2 3)", "type-error"),
            ("(eval 1)", "arity-error"),
            ("($if #t 1)", "arity-error"),
            ("(($lambda (x) x))", "arity-error"),
            ("(1 2)", "not-a-combiner"),
            ("($vau (x x) #ignore x)", "bad-formals"),
            ("($vau (x) x x)", "bad-formals"),
            ("($define! 1 2)", "bad-formals"),
            ("(* 9223372036854775807 2)", "overflow"),
            ("(div -9223372036854775808 -1)", "overflow"),
            ("(div 1 0)", "division-by-zero"),
            ("(car", "parse-error"),
            (")", "parse-error"),
            ("(1 . )", "parse-error"),
            ("#x", "parse-error"),
            ("99999999999999999999", "parse-error"),
            ("(error ($quote my-code) 1 2)", "my-code"),
        ],
    );
    let e = g.eval("(error ($quote my-code) 1 \"two\")").unwrap_err();
    assert_eq!(e.values.len(), 2);
    assert_eq!(e.values[0], Value::Int(1));
    assert_eq!(g.string(e.values[1]), Some("two"));
    let e = g.eval("(car\n  (cons 1 2)").unwrap_err();
    assert_eq!(e.values, [Value::Int(2), Value::Int(13)]);
    let e = g.eval("(foo bar)").unwrap_err();
    assert_eq!(show(&g, e.values[0]), "foo");
}

#[test]
fn tail_calls_run_in_constant_space() {
    let mut g = grift();
    g.eval(
        "($define! loop ($lambda (n) ($if (eq? n 0) #t ($sequence (cons n n) (loop (- n 1))))))
         ($define! count ($lambda (n) ($cond ((eq? n 0) #t) (#t ($let ((m (- n 1))) (count m))))))",
    )
    .unwrap();
    assert_eq!(g.eval("(loop 1000000)"), Ok(Value::True));
    assert_eq!(g.eval("(count 100000)"), Ok(Value::True));
    assert!(
        g.heap.len() < 100_000,
        "heap grew to {} slots",
        g.heap.len()
    );
}

#[test]
fn deep_nesting_needs_no_rust_stack() {
    let mut g = grift();
    let depth = 100_000;
    let nested = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
    let v = g.eval(&format!("($quote {nested})")).unwrap();
    assert_eq!(show(&g, v), nested);
    let sum = format!("{}0{}", "(+ 1 ".repeat(depth), ")".repeat(depth));
    assert_eq!(g.eval(&sum), Ok(Value::Int(100_000)));
    g.eval(&format!("($define! deep ($quote {nested}))"))
        .unwrap();
    assert_eq!(g.eval("(equal? deep deep)"), Ok(Value::True));
}

#[test]
fn collection_reclaims_garbage_and_keeps_live_data() {
    let mut g = grift();
    g.eval(
        "($define! build ($lambda (n acc) ($if (eq? n 0) acc (build (- n 1) (cons n acc)))))
         ($define! keep (build 1000 ()))
         ($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (build 100 ()) (churn (- n 1))))))",
    )
    .unwrap();
    assert_eq!(g.eval("(churn 5000)"), Ok(Value::True));
    assert!(
        g.heap.len() < 100_000,
        "heap grew to {} slots",
        g.heap.len()
    );
    assert_eq!(g.eval("(length keep)"), Ok(Value::Int(1000)));
}

#[test]
fn printing_reads_back() {
    let mut g = grift();
    for src in [
        "(1 -2 (a . b) \"q\\\"\\\\\\n\" #t #f #inert #ignore ())",
        "(())",
        "x",
    ] {
        let v = g.eval(&format!("($quote {src})")).unwrap();
        let printed = show(&g, v);
        let again = g.eval(&format!("($quote {printed})")).unwrap();
        let same = g
            .eval(&format!("(equal? ($quote {printed}) ($quote {src}))"))
            .unwrap();
        assert_eq!((show(&g, again), same), (printed, Value::True));
    }
    let f = g.eval("($lambda (x) (* x x))").unwrap();
    assert_eq!(
        show(&g, f),
        "#[applicative #[operative (x) #ignore (* x x)]]"
    );
}

#[test]
fn redefinition_and_bindings() {
    let mut g = grift();
    g.eval("($define! f ($lambda (x) (+ x 1))) ($define! g ($lambda (x) (f x)))")
        .unwrap();
    assert_eq!(g.eval("(g 1)"), Ok(Value::Int(2)));
    g.eval("($define! f ($lambda (x) (* x 10)))").unwrap();
    assert_eq!(g.eval("(g 1)"), Ok(Value::Int(10)));
    let top = g.eval("(bindings (($vau () e e)))").unwrap();
    assert_eq!(show(&g, top), "(f g)");
    let ground = g.eval("(bindings ground-environment)").unwrap();
    let ground = show(&g, ground);
    for name in [
        "$vau",
        "car",
        "error",
        "$define!",
        "$lambda",
        "equal?",
        "ground-environment",
    ] {
        assert!(
            ground.split([' ', '(', ')']).any(|n| n == name),
            "{name} missing from {ground}"
        );
    }
    // A top-level definition shadows the library without changing it.
    g.eval("($define! list 5)").unwrap();
    assert_eq!(
        g.eval("(equal? (map car ($quote ((1) (2)))) ($quote (1 2)))"),
        Ok(Value::True)
    );
}

fn twice(_: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Int(n)] => Ok(Value::Int(n * 2)),
        _ => Err(Error {
            code: crate::K::TypeError.into(),
            values: args.to_vec(),
        }),
    }
}

fn reenter(g: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    g.eval("(churn 2000)")?;
    Ok(args[0])
}

#[test]
fn host_functions() {
    let mut g = grift();
    g.define("twice", twice).unwrap();
    g.define("reenter", reenter).unwrap();
    assert_eq!(g.eval("(twice (+ 1 2))"), Ok(Value::Int(6)));
    let f = g.eval("twice").unwrap();
    assert_eq!(show(&g, f), "#[applicative #[host twice]]");
    fails(&mut g, &[("(twice 1 2)", "type-error")]);
    // A host function may evaluate; collections inside keep the caller's values.
    g.eval("($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (list 1 2 3) (churn (- n 1))))))")
        .unwrap();
    assert_eq!(g.eval("(length (reenter (list 1 2 3)))"), Ok(Value::Int(3)));
}

#[test]
fn stale_values_are_refused() {
    let mut g = grift();
    let old = g.eval("(list 1 2)").unwrap();
    g.eval("($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (list 1 2 3) (churn (- n 1))))))")
        .unwrap();
    g.eval("(churn 10000)").unwrap();
    assert_eq!(show(&g, old), "#[stale]");
    assert_eq!(g.string(old), None);
    let Value::Pair(r) = old else {
        panic!("a pair")
    };
    let e = g.pair(r).unwrap_err();
    assert_eq!(code(&mut g, &e), "stale-value");
}
