#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

extern crate std;

use crate::{Error, Grift, Value};
use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

fn grift() -> Grift {
    Grift::new().expect("the library loads")
}

fn show<T>(g: &Grift<T>, v: Value) -> String {
    let mut s = String::new();
    g.write(v, &mut s).unwrap();
    s
}

fn code<T>(g: &mut Grift<T>, e: &Error) -> String {
    show(g, Value::Sym(e.code))
}

/// Each source evaluates to `#t`.
fn holds(g: &mut Grift, cases: &[&str]) {
    for src in cases {
        match g.eval(&mut (), src) {
            Ok(Value::True) => {}
            Ok(v) => panic!("{src} gave {}", show(g, v)),
            Err(e) => panic!("{src} failed: {}", code(g, &e)),
        }
    }
}

/// Each source fails with the given code.
fn fails(g: &mut Grift, cases: &[(&str, &str)]) {
    for (src, want) in cases {
        match g.eval(&mut (), src) {
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
            "(eq? (eval ($quote (+ 1 2)) (make-environment (get-current-environment))) 3)",
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
fn library_basics() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            "($and? (null? ()) (not? (null? (list 1))) (pair? (cons 1 2)) (not? (pair? ())))",
            "($and? (symbol? ($quote a)) (string? \"a\") (integer? 1) (not? (integer? \"1\")) \
             (environment? (get-current-environment)))",
            "($and? (equal? (reverse (list 1 2 3)) ($quote (3 2 1))) (null? (reverse ())))",
            "($and? (equal? (assoc 2 ($quote ((1 a) (2 b)))) ($quote (2 b))) (null? (assoc 3 ())))",
            "($and? (member? (list 1) (list 2 (list 1))) (not? (member? 3 (list 1 2))))",
            "($and? (=? (min 1 2) 1) (=? (max 1 2) 2))",
            // mod is what div leaves, for every sign.
            "(equal? (map ($lambda ((a b)) (=? a (+ (* b (div a b)) (mod a b)))) \
                       ($quote ((7 2) (-7 2) (7 -2) (-7 -2)))) (list #t #t #t #t))",
            // ... and fits wherever its result does.
            "(equal? (list (mod -9223372036854775808 3) (mod 9223372036854775807 -2) \
                           (mod -9223372036854775808 -1)) ($quote (1 -1 0)))",
            "($sequence ($define! seen ()) ($define! here (get-current-environment)) \
             (for-each ($lambda (x) ($set! here seen (cons x seen))) (list 1 2 3)) \
             (equal? seen ($quote (3 2 1))))",
            "($and? (eq? ($when #f (car ())) #inert) (eq? ($when #t 1 2) 2))",
            "(equal? ($let* ((x 1) (y (+ x 1))) (list x y)) ($quote (1 2)))",
            "(eq? ($let* () 5) 5)",
            // $set! changes the binding where it is, even from inside a function.
            "($sequence ($define! n 0) ($define! e (get-current-environment)) \
             (($lambda () ($set! e n (+ n 1)))) (=? n 1))",
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
    let e = g
        .eval(&mut (), "(error ($quote my-code) 1 \"two\")")
        .unwrap_err();
    assert_eq!(e.values.len(), 2);
    assert_eq!(e.values[0], Value::Int(1));
    assert_eq!(g.string(e.values[1]), Some("two"));
    let e = g.eval(&mut (), "(car\n  (cons 1 2)").unwrap_err();
    assert_eq!(e.values, [Value::Int(2), Value::Int(13)]);
    let e = g.eval(&mut (), "(foo bar)").unwrap_err();
    assert_eq!(show(&g, e.values[0]), "foo");
}

#[test]
fn tail_calls_run_in_constant_space() {
    let mut g = grift();
    g.eval(
        &mut (),
        "($define! loop ($lambda (n) ($if (eq? n 0) #t ($sequence (cons n n) (loop (- n 1))))))
         ($define! count ($lambda (n) ($cond ((eq? n 0) #t) (#t ($let ((m (- n 1))) (count m))))))",
    )
    .unwrap();
    assert_eq!(g.eval(&mut (), "(loop 1000000)"), Ok(Value::True));
    assert_eq!(g.eval(&mut (), "(count 100000)"), Ok(Value::True));
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
    let v = g.eval(&mut (), &format!("($quote {nested})")).unwrap();
    assert_eq!(show(&g, v), nested);
    let sum = format!("{}0{}", "(+ 1 ".repeat(depth), ")".repeat(depth));
    assert_eq!(g.eval(&mut (), &sum), Ok(Value::Int(100_000)));
    g.eval(&mut (), &format!("($define! deep ($quote {nested}))"))
        .unwrap();
    assert_eq!(g.eval(&mut (), "(equal? deep deep)"), Ok(Value::True));
}

#[test]
fn collection_reclaims_garbage_and_keeps_live_data() {
    let mut g = grift();
    g.eval(
        &mut (),
        "($define! build ($lambda (n acc) ($if (eq? n 0) acc (build (- n 1) (cons n acc)))))
         ($define! keep (build 1000 ()))
         ($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (build 100 ()) (churn (- n 1))))))",
    )
    .unwrap();
    assert_eq!(g.eval(&mut (), "(churn 5000)"), Ok(Value::True));
    assert!(
        g.heap.len() < 100_000,
        "heap grew to {} slots",
        g.heap.len()
    );
    assert_eq!(g.eval(&mut (), "(length keep)"), Ok(Value::Int(1000)));
}

#[test]
fn printing_reads_back() {
    let mut g = grift();
    for src in [
        "(1 -2 (a . b) \"q\\\"\\\\\\n\" #t #f #inert #ignore ())",
        "(())",
        "x",
    ] {
        let v = g.eval(&mut (), &format!("($quote {src})")).unwrap();
        let printed = show(&g, v);
        let again = g.eval(&mut (), &format!("($quote {printed})")).unwrap();
        let same = g
            .eval(
                &mut (),
                &format!("(equal? ($quote {printed}) ($quote {src}))"),
            )
            .unwrap();
        assert_eq!((show(&g, again), same), (printed, Value::True));
    }
    let f = g.eval(&mut (), "($lambda (x) (* x x))").unwrap();
    assert_eq!(
        show(&g, f),
        "#[applicative #[operative (x) #ignore (* x x)]]"
    );
    // A body of several forms prints as written, not as $sequence's own source.
    let f = g.eval(&mut (), "($lambda (x) (f x) x)").unwrap();
    assert_eq!(
        show(&g, f),
        "#[applicative #[operative (x) #ignore ($sequence (f x) x)]]"
    );
    // Control characters print as escapes that read back.
    let v = g.eval(&mut (), "\"\\x1b;[2J\\ta\\x7f;\\x3bb;\"").unwrap();
    let printed = show(&g, v);
    assert_eq!(printed, "\"\\x1b;[2J\\ta\\x7f;\u{3bb}\"");
    let again = g.eval(&mut (), &printed).unwrap();
    assert_eq!(g.string(again), g.string(v));
    fails(
        &mut g,
        &[
            ("\"\\x;\"", "parse-error"),
            ("\"\\x110000;\"", "parse-error"),
        ],
    );
}

#[test]
fn reading_evaluates_nothing() {
    let mut g = grift();
    let forms = g.read("(car 1) undefined").unwrap();
    assert_eq!(show(&g, forms), "((car 1) undefined)");
}

#[test]
fn errors_name_the_program_combination() {
    let mut g = grift();
    g.eval(&mut (), "($define! f ($lambda (x) (car x)))")
        .unwrap();
    // The innermost combination of the program, never the library's code.
    for (src, at) in [
        ("($if 1 2 3)", "($if 1 2 3)"),
        ("(f 1)", "(car x)"),
        ("(map car (list 1 2))", "(map car (list 1 2))"),
        ("($let (((a b) (list 1))) a)", "($let (((a b) (list 1))) a)"),
        ("($cond ((car 1) 2))", "(car 1)"),
        ("(+ 1 (foo))", "(foo)"),
        ("(length 1)", "(length 1)"),
        ("(reverse 1)", "(reverse 1)"),
        ("(error ($quote mine))", "(error ($quote mine))"),
    ] {
        let e = g.eval(&mut (), src).unwrap_err();
        assert_eq!(e.at.map(|v| show(&g, v)).as_deref(), Some(at), "{src}");
    }
    assert_eq!(g.eval(&mut (), "(car").unwrap_err().at, None);
    // A host function's location survives collections in an evaluation it runs.
    g.eval(
        &mut (),
        "($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (list 1 2 3) (churn (- n 1))))))",
    )
    .unwrap();
    g.define("fail-after-churn", |g, state, _| {
        g.eval(state, "(churn 20000)")?;
        Err(g.error("mine", &[]))
    })
    .unwrap();
    let e = g.eval(&mut (), "(fail-after-churn)").unwrap_err();
    assert_eq!(show(&g, e.at.unwrap()), "(fail-after-churn)");
}

#[test]
fn redefinition_and_bindings() {
    let mut g = grift();
    g.eval(
        &mut (),
        "($define! f ($lambda (x) (+ x 1))) ($define! g ($lambda (x) (f x)))",
    )
    .unwrap();
    assert_eq!(g.eval(&mut (), "(g 1)"), Ok(Value::Int(2)));
    g.eval(&mut (), "($define! f ($lambda (x) (* x 10)))")
        .unwrap();
    assert_eq!(g.eval(&mut (), "(g 1)"), Ok(Value::Int(10)));
    let top = g.eval(&mut (), "(bindings (($vau () e e)))").unwrap();
    assert_eq!(show(&g, top), "(f g)");
    let ground = g.eval(&mut (), "ground-names").unwrap();
    let ground = show(&g, ground);
    for name in [
        "$vau",
        "car",
        "error",
        "$define!",
        "$lambda",
        "equal?",
        "ground-names",
    ] {
        assert!(
            ground.split([' ', '(', ')']).any(|n| n == name),
            "{name} missing from {ground}"
        );
    }
    // A top-level definition shadows the library without changing it, and the ground
    // can't be reached to change it there.
    g.eval(&mut (), "($define! list 5) ($define! $vau 6)")
        .unwrap();
    assert_eq!(
        g.eval(
            &mut (),
            "(equal? (map car ($quote ((1) (2)))) ($quote (1 2)))"
        ),
        Ok(Value::True)
    );
    fails(&mut g, &[("ground-environment", "unbound-symbol")]);
}

fn twice(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Int(n)] => Ok(Value::Int(n * 2)),
        _ => Err(g.error("type-error", args)),
    }
}

fn reenter(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    g.eval(&mut (), "(churn 2000)")?;
    Ok(args[0])
}

#[test]
fn host_functions() {
    let mut g = grift();
    g.define("twice", twice).unwrap();
    g.define("reenter", reenter).unwrap();
    assert_eq!(g.eval(&mut (), "(twice (+ 1 2))"), Ok(Value::Int(6)));
    let f = g.eval(&mut (), "twice").unwrap();
    assert_eq!(show(&g, f), "#[applicative #[host twice]]");
    fails(&mut g, &[("(twice 1 2)", "type-error")]);
    // A host function may evaluate; collections inside keep the caller's values.
    g.eval(&mut (), "($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (list 1 2 3) (churn (- n 1))))))")
        .unwrap();
    assert_eq!(
        g.eval(&mut (), "(length (reenter (list 1 2 3)))"),
        Ok(Value::Int(3))
    );
}

/// `(halves "ab")` is `("ab" . "ab")`, made from Rust; `(sum (1 2 3))` walks a list from Rust.
fn halves(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    let s = String::from(g.string(args[0]).unwrap());
    let (a, b) = (g.make_string(&s)?, g.make_string(&s)?);
    g.cons(a, b)
}

fn sum(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    let (mut list, mut n) = (args[0], 0);
    while let Some((Value::Int(x), rest)) = g.uncons(list) {
        (list, n) = (rest, n + x);
    }
    Ok(Value::Int(n))
}

#[test]
fn host_functions_make_and_read_data() {
    let mut g = grift();
    g.define("halves", halves).unwrap();
    g.define("sum", sum).unwrap();
    holds(
        &mut g,
        &[
            "(equal? (halves \"ab\") (cons \"ab\" \"ab\"))",
            "(=? (sum (list 1 2 3)) 6)",
        ],
    );
    assert_eq!(g.uncons(Value::Int(1)), None);
}

/// Counts its calls in the host's state, and calls back into grift with it.
fn count(g: &mut Grift<u32>, calls: &mut u32, args: &[Value]) -> Result<Value, Error> {
    *calls += 1;
    match args {
        [] => Ok(Value::Int(i64::from(*calls))),
        _ => g.eval(calls, "(count)"),
    }
}

#[test]
fn host_functions_get_the_host_state() {
    let mut g: Grift<u32> = Grift::new().unwrap();
    g.define("count", count).unwrap();
    let mut calls = 0;
    assert_eq!(g.eval(&mut calls, "(count) (count 1)"), Ok(Value::Int(3)));
    assert_eq!(calls, 3);
}

#[test]
fn the_host_calls_combiners_with_values() {
    let mut g = grift();
    let f = g.eval(&mut (), "($lambda (x ys) (cons x ys))").unwrap();
    // The arguments are values: a symbol and a list are not evaluated again.
    let x = g.eval(&mut (), "($quote undefined)").unwrap();
    let ys = g.eval(&mut (), "($quote (car 1))").unwrap();
    let v = g.call(&mut (), f, &[x, ys]).unwrap();
    assert_eq!(show(&g, v), "(undefined car 1)");
    let car = g.eval(&mut (), "car").unwrap();
    let e = g.call(&mut (), car, &[Value::Int(1)]).unwrap_err();
    assert_eq!(code(&mut g, &e), "type-error");
    let quote = g.eval(&mut (), "$quote").unwrap();
    assert_eq!(g.call(&mut (), quote, &[Value::Int(1)]), Ok(Value::Int(1)));
}

static STOP: AtomicBool = AtomicBool::new(false);

#[test]
fn the_host_interrupts_a_runaway_evaluation() {
    let mut g = grift();
    g.set_interrupt_flag(&STOP);
    g.eval(
        &mut (),
        "($define! log ()) ($define! here (get-current-environment))",
    )
    .unwrap();
    g.eval(&mut (), "($define! spin ($lambda () (spin)))")
        .unwrap();
    let setter = std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        STOP.store(true, Ordering::Relaxed);
    });
    let e = g
        .eval(
            &mut (),
            "($unwind-protect (spin) ($set! here log (cons 1 log)))",
        )
        .unwrap_err();
    setter.join().unwrap();
    assert_eq!(code(&mut g, &e), "interrupted");
    // The cleanup ran, the flag is clear again and the interpreter goes on.
    assert!(!STOP.load(Ordering::Relaxed));
    assert_eq!(g.eval(&mut (), "(length log)"), Ok(Value::Int(1)));
}

#[test]
fn cleanup_runs_and_the_error_goes_on() {
    let mut g = grift();
    g.eval(
        &mut (),
        "($define! log ()) ($define! here (get-current-environment))",
    )
    .unwrap();
    g.eval(
        &mut (),
        "($define! note ($lambda (x) ($set! here log (cons x log))))",
    )
    .unwrap();
    assert_eq!(
        g.eval(&mut (), "($unwind-protect 5 (note 1) (note 2))"),
        Ok(Value::Int(5))
    );
    // The error leaves through both cleanups, innermost first, unchanged.
    let e = g
        .eval(
            &mut (),
            "($unwind-protect ($unwind-protect (error ($quote mine) 7 (list 8)) (note 3)) (note 4))",
        )
        .unwrap_err();
    assert_eq!(code(&mut g, &e), "mine");
    assert_eq!(e.values[0], Value::Int(7));
    assert_eq!(show(&g, e.values[1]), "(8)");
    assert_eq!(show(&g, e.at.unwrap()), "(error ($quote mine) 7 (list 8))");
    let log = g.eval(&mut (), "log").unwrap();
    assert_eq!(show(&g, log), "(4 3 2 1)");
    // A cleanup that fails replaces the error; the outer cleanup still runs.
    let e = g
        .eval(
            &mut (),
            "($unwind-protect ($unwind-protect (car 1) (cdr 2)) (note 5))",
        )
        .unwrap_err();
    assert_eq!(show(&g, e.values[0]), "#[primitive cdr]");
    assert_eq!(g.eval(&mut (), "(car log)"), Ok(Value::Int(5)));
    // A cleanup that succeeds doesn't lend the error its own location.
    let (protect, env) = (
        g.eval(&mut (), "eval-protected").unwrap(),
        g.eval(&mut (), "(get-current-environment)").unwrap(),
    );
    let missing = Value::Sym(g.symbol("missing").unwrap());
    let forms = g.read("(list 1)").unwrap();
    let cleanup = g.uncons(forms).unwrap().0;
    let e = g
        .call(&mut (), protect, &[missing, env, cleanup])
        .unwrap_err();
    assert_eq!(
        (code(&mut g, &e), e.at),
        (String::from("unbound-symbol"), None)
    );
    // The waiting error's values survive collections in the cleanup.
    g.eval(&mut (), "($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (list 1 2 3) (churn (- n 1))))))")
        .unwrap();
    let e = g
        .eval(
            &mut (),
            "($unwind-protect (error ($quote mine) (list 1 2)) (churn 20000))",
        )
        .unwrap_err();
    assert_eq!(show(&g, e.values[0]), "(1 2)");
}

#[test]
fn stale_values_are_refused() {
    let mut g = grift();
    let old = g.eval(&mut (), "(list 1 2)").unwrap();
    g.eval(&mut (), "($define! churn ($lambda (n) ($if (eq? n 0) #t ($sequence (list 1 2 3) (churn (- n 1))))))")
        .unwrap();
    g.eval(&mut (), "(churn 10000)").unwrap();
    assert_eq!(show(&g, old), "#[stale]");
    assert_eq!(g.string(old), None);
    let Value::Pair(r) = old else {
        panic!("a pair")
    };
    let e = g.pair(r).unwrap_err();
    assert_eq!(code(&mut g, &e), "stale-value");
}
