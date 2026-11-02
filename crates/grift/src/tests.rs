#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::{Error, Grift, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn grift() -> Grift {
    Grift::new().expect("the library loads")
}

fn show<T>(g: &Grift<T>, v: Value) -> String {
    let mut s = String::new();
    g.write(v, &mut s).unwrap();
    s
}

fn kind<T>(g: &mut Grift<T>, e: &Error) -> String {
    show(g, Value::Symbol(e.kind))
}

/// Binds a host function in the top-level context.
fn define<T>(g: &mut Grift<T>, name: &str, f: crate::HostFunction<T>) {
    let f = g.host_function(name, f).unwrap();
    g.define(name, f).unwrap();
}

/// Each source evaluates to `#true`.
fn holds(g: &mut Grift, cases: &[&str]) {
    for src in cases {
        match g.evaluate(&mut (), src) {
            Ok(Value::Boolean(true)) => {}
            Ok(v) => panic!("{src} gave {}", show(g, v)),
            Err(e) => panic!("{src} failed: {}", kind(g, &e)),
        }
    }
}

/// Each source fails with the given kind of error.
fn fails(g: &mut Grift, cases: &[(&str, &str)]) {
    for (src, want) in cases {
        match g.evaluate(&mut (), src) {
            Ok(v) => panic!("{src} gave {}", show(g, v)),
            Err(e) => assert_eq!(kind(g, &e), *want, "{src}"),
        }
    }
}

#[test]
fn primitives() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            "(equal? ((vau! (x) #ignore x) (a b)) (first (quote! ((a b)))))",
            "(equal? ((vau! (x . y) e (list x y)) 1 2 3) (quote! (1 (2 3))))",
            "(same? (type-of ((vau! () e e))) (quote! context))",
            "(same? ((wrap (vau! (x) #ignore x)) (add 1 2)) 3)",
            "(equal? ((unwrap list) (add 1 2)) (quote! ((add 1 2))))",
            "(same? (evaluate (quote! (add 1 2)) (context (current-context))) 3)",
            "(sequence! (define! e (context)) (equal? (names e) ()))",
            "(sequence! (define! e (context)) (bind e (quote! (a (b))) (list 1 (list 2))) \
             (equal? (list (evaluate (quote! a) e) (names e)) (quote! (1 (a b)))))",
            "(equal? (pair 1 2) (quote! (1 . 2)))",
            "(same? (first (pair 1 2)) 1)",
            "(same? (rest (pair 1 2)) 2)",
            "(same? \"ab\" \"ab\")",
            "(not? (same? (pair 1 2) (pair 1 2)))",
            "(same? (select #true 1 2) 1)",
            "(same? (select #false 1 2) 2)",
            "(equal? (map type-of (list () #true #inert #ignore 1 (quote! a) (pair 1 2) \"s\" first if!)) \
             (quote! (empty-list boolean inert ignore integer symbol pair string applicative operative)))",
            "(same? (add 2 3) 5)",
            "(same? (subtract 2 3) -1)",
            "(same? (multiply -4 3) -12)",
            "(equal? (list (divide 7 2) (divide -7 2) (divide 7 -2) (divide -7 -2)) (quote! (3 -3 -3 3)))",
            "(and!? (less-than? 1 2) (not? (less-than? 2 1)) (not? (less-than? 1 1)))",
        ],
    );
}

#[test]
fn library() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            "(same? (quote! a) (quote! a))",
            "(equal? (list 1 (add 1 1)) (quote! (1 2)))",
            "(sequence! (define! (a . b) (list 1 2)) (equal? (list a b) (quote! (1 (2)))))",
            "(same? (if! #true 1 (first ())) 1)",
            "(same? (sequence!) #inert)",
            "(same? (sequence! 1 2 3) 3)",
            "(same? ((lambda! (x y) (multiply x y)) 6 7) 42)",
            "(same? ((lambda! (x) (define! y (add x 1)) (multiply y 2)) 1) 4)",
            "(same? (let! ((x 2) ((y z) (list 3 4))) (add x (multiply y z))) 14)",
            "(same? (conditional! (#false 1) ((same? 1 1) 2 3) (#true (first ()))) 3)",
            "(same? (conditional! (#false 1)) #inert)",
            "(and!? (and!?) (not? (and!? #true #false)) (and!? #true #true) (not? (and!? #false (first ()))))",
            "(and!? (not? (or!?)) (or!? #false #true) (not? (or!? #false #false)) (or!? #true (first ())))",
            "(and!? (not? #false) (not? (not? #true)))",
            "(and!? (equal? 3 3) (not? (equal? 3 4)))",
            "(equal? (apply list (list 1 2)) (quote! (1 2)))",
            "(equal? (apply (lambda! (x) (multiply x x)) (list 5)) 25)",
            "(equal? (map (lambda! (x) (multiply x x)) (list 1 2 3)) (quote! (1 4 9)))",
            "(equal? (append (list 1) () (list 2 3) 4) (quote! (1 2 3 . 4)))",
            "(same? (append) ())",
            "(and!? (same? (length ()) 0) (same? (length (list 1 2 3)) 3))",
            "(and!? (equal? \"a\" \"a\") (equal? (list 1 (list 2)) (list 1 (list 2))) \
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
            "(and!? (empty-list? ()) (not? (empty-list? (list 1))) (pair? (pair 1 2)) (not? (pair? ())))",
            "(and!? (symbol? (quote! a)) (string? \"a\") (integer? 1) (not? (integer? \"1\")) \
             (context? (current-context)))",
            "(and!? (equal? (reverse (list 1 2 3)) (quote! (3 2 1))) (empty-list? (reverse ())))",
            "(and!? (equal? (find-association 2 (quote! ((1 a) (2 b)))) (quote! (2 b))) (empty-list? (find-association 3 ())))",
            "(and!? (member? (list 1) (list 2 (list 1))) (not? (member? 3 (list 1 2))))",
            "(and!? (equal? (minimum 1 2) 1) (equal? (maximum 1 2) 2))",
            // remainder is what divide leaves, for every sign, as in Rust.
            "(equal? (map (lambda! ((a b)) (equal? a (add (multiply b (divide a b)) (remainder a b)))) \
                       (quote! ((7 2) (-7 2) (7 -2) (-7 -2)))) (list #true #true #true #true))",
            // ... with the sign of the dividend.
            "(equal? (list (remainder -9223372036854775808 3) (remainder 9223372036854775807 -2)) \
                     (quote! (-2 1)))",
            "(sequence! (define! seen ()) (define! here (current-context)) \
             (for-each (lambda! (x) (bind here (quote! seen) (pair x seen))) (list 1 2 3)) \
             (equal? seen (quote! (3 2 1))))",
            "(and!? (same? (when! #false (first ())) #inert) (same? (when! #true 1) 1))",
            "(equal? (let! ((x 1) (y (add x 1))) (list x y)) (quote! (1 2)))",
            "(same? (let! () 5) 5)",
            // bind changes the binding where it is, even from inside a function.
            "(sequence! (define! n 0) (define! e (current-context)) \
             ((lambda! () (bind e (quote! n) (add n 1)))) (equal? n 1))",
        ],
    );
}

#[test]
fn error_kinds() {
    let mut g = grift();
    fails(
        &mut g,
        &[
            ("nope", "unbound-symbol"),
            ("(first 1)", "type-error"),
            ("(add 1 \"a\")", "type-error"),
            ("(if! 1 2 3)", "type-error"),
            ("(evaluate 1)", "arity-error"),
            ("(if! #true 1 2 3)", "arity-error"),
            ("(if! #true 1)", "arity-error"),
            ("(if! #true)", "arity-error"),
            ("((lambda! (x) x))", "arity-error"),
            ("(1 2)", "not-a-combiner"),
            ("(vau! (x x) #ignore x)", "parameter-error"),
            ("(vau! (x) x x)", "parameter-error"),
            ("(define! 1 2)", "parameter-error"),
            ("(multiply 9223372036854775807 2)", "overflow"),
            ("(divide -9223372036854775808 -1)", "overflow"),
            ("(remainder -9223372036854775808 -1)", "overflow"),
            ("(divide 1 0)", "division-by-zero"),
            ("(remainder 1 0)", "division-by-zero"),
            ("(first", "parse-error"),
            (")", "parse-error"),
            ("(1 . )", "parse-error"),
            ("#x", "parse-error"),
            ("99999999999999999999", "parse-error"),
            ("(raise (error (quote! my-kind) 1 2))", "my-kind"),
        ],
    );
    let e = g
        .evaluate(&mut (), "(raise (error (quote! my-kind) 1 \"two\"))")
        .unwrap_err();
    assert_eq!(e.values.len(), 2);
    assert_eq!(e.values[0], Value::Integer(1));
    assert_eq!(g.as_str(e.values[1]), Some("two"));
    let e = g.evaluate(&mut (), "(first\n  (pair 1 2)").unwrap_err();
    assert_eq!(e.values, [Value::Integer(2), Value::Integer(13)]);
    let e = g.evaluate(&mut (), "(foo bar)").unwrap_err();
    assert_eq!(show(&g, e.values[0]), "foo");
}

#[test]
fn tail_calls_run_in_constant_space() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! loop (lambda! (n) (if! (same? n 0) #true (sequence! (pair n n) (loop (subtract n 1))))))
         (define! count (lambda! (n) (conditional! ((same? n 0) #true) (#true (let! ((m (subtract n 1))) (count m))))))",
    )
    .unwrap();
    assert_eq!(
        g.evaluate(&mut (), "(loop 200000)"),
        Ok(Value::Boolean(true))
    );
    assert_eq!(
        g.evaluate(&mut (), "(count 50000)"),
        Ok(Value::Boolean(true))
    );
    // when!'s last body form is a tail call too.
    g.evaluate(
        &mut (),
        "(define! down (lambda! (n) (when! (less-than? 0 n) (pair n n) (down (subtract n 1)))))",
    )
    .unwrap();
    assert_eq!(g.evaluate(&mut (), "(down 1000000)"), Ok(Value::Inert));
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
    let v = g.evaluate(&mut (), &format!("(quote! {nested})")).unwrap();
    assert_eq!(show(&g, v), nested);
    let sum = format!("{}0{}", "(add 1 ".repeat(depth), ")".repeat(depth));
    assert_eq!(g.evaluate(&mut (), &sum), Ok(Value::Integer(100_000)));
    // equal? is library code, so it needs no Rust stack either; a shallower tree keeps the
    // test quick.
    let nested = format!("{}{}", "(".repeat(depth / 10), ")".repeat(depth / 10));
    g.evaluate(&mut (), &format!("(define! deep (quote! {nested}))"))
        .unwrap();
    let copy = format!("(equal? deep (quote! {nested}))");
    assert_eq!(g.evaluate(&mut (), &copy), Ok(Value::Boolean(true)));
}

#[test]
fn collection_reclaims_garbage_and_keeps_live_data() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! build (lambda! (n acc) (if! (same? n 0) acc (build (subtract n 1) (pair n acc)))))
         (define! keep (build 1000 ()))
         (define! churn (lambda! (n) (if! (same? n 0) #true (sequence! (build 100 ()) (churn (subtract n 1))))))",
    )
    .unwrap();
    assert_eq!(
        g.evaluate(&mut (), "(churn 5000)"),
        Ok(Value::Boolean(true))
    );
    assert!(
        g.heap.len() < 100_000,
        "heap grew to {} slots",
        g.heap.len()
    );
    assert_eq!(
        g.evaluate(&mut (), "(length keep)"),
        Ok(Value::Integer(1000))
    );
}

#[test]
fn printing_reads_back() {
    let mut g = grift();
    for src in [
        "(1 -2 (a . b) \"q\\\"\\\\\\n\" #true #false #inert #ignore ())",
        "(())",
        "x",
    ] {
        let v = g.evaluate(&mut (), &format!("(quote! {src})")).unwrap();
        let printed = show(&g, v);
        let again = g.evaluate(&mut (), &format!("(quote! {printed})")).unwrap();
        let again = show(&g, again);
        let same = g
            .evaluate(
                &mut (),
                &format!("(equal? (quote! {printed}) (quote! {src}))"),
            )
            .unwrap();
        assert_eq!((again, same), (printed, Value::Boolean(true)));
    }
    let f = g.evaluate(&mut (), "(lambda! (x) (multiply x x))").unwrap();
    assert_eq!(
        show(&g, f),
        "#[applicative #[operative (x) #ignore (multiply x x)]]"
    );
    // A body of several forms prints as written, not as sequence!'s own source.
    let f = g.evaluate(&mut (), "(lambda! (x) (f x) x)").unwrap();
    assert_eq!(
        show(&g, f),
        "#[applicative #[operative (x) #ignore (sequence! (f x) x)]]"
    );
    // Control characters print as escapes that read back.
    let v = g
        .evaluate(&mut (), "\"\\x1b;[2J\\ta\\x7f;\\x3bb;\"")
        .unwrap();
    let printed = show(&g, v);
    assert_eq!(printed, "\"\\x1b;[2J\\ta\\x7f;\u{3bb}\"");
    let again = g.evaluate(&mut (), &printed).unwrap();
    assert_eq!(g.as_str(again), g.as_str(v));
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
    let forms = g.read("(first 1) undefined").unwrap();
    assert_eq!(show(&g, forms), "((first 1) undefined)");
}

#[test]
fn errors_name_the_program_combination() {
    let mut g = grift();
    g.evaluate(&mut (), "(define! f (lambda! (x) (first x)))")
        .unwrap();
    // The innermost combination of the program, never the library's code.
    for (src, at) in [
        ("(if! 1 2 3)", "(if! 1 2 3)"),
        ("(f 1)", "(first x)"),
        ("(map first (list 1 2))", "(map first (list 1 2))"),
        ("(let! (((a b) (list 1))) a)", "(let! (((a b) (list 1))) a)"),
        ("(conditional! ((first 1) 2))", "(first 1)"),
        ("(conditional! 1)", "(conditional! 1)"),
        ("(when! 1 2)", "(when! 1 2)"),
        ("(add 1 (foo))", "(foo)"),
        ("(length 1)", "(length 1)"),
        ("(reverse 1)", "(reverse 1)"),
        (
            "(raise (error (quote! mine)))",
            "(raise (error (quote! mine)))",
        ),
    ] {
        let e = g.evaluate(&mut (), src).unwrap_err();
        assert_eq!(
            e.trace().first().map(|&v| show(&g, v)).as_deref(),
            Some(at),
            "{src}"
        );
    }
    assert_eq!(g.evaluate(&mut (), "(first").unwrap_err().trace, []);
    // A host function's location survives collections in an evaluation it runs.
    g.evaluate(
        &mut (),
        "(define! churn (lambda! (n) (if! (same? n 0) #true (sequence! (list 1 2 3) (churn (subtract n 1))))))",
    )
    .unwrap();
    define(&mut g, "fail-after-churn", |g, state, _| {
        g.evaluate(state, "(churn 20000)")?;
        Err(g.error("mine", &[]))
    });
    let e = g.evaluate(&mut (), "(fail-after-churn)").unwrap_err();
    assert_eq!(show(&g, e.trace()[0]), "(fail-after-churn)");
}

#[test]
fn redefinition_and_names() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! f (lambda! (x) (add x 1))) (define! g (lambda! (x) (f x)))",
    )
    .unwrap();
    assert_eq!(g.evaluate(&mut (), "(g 1)"), Ok(Value::Integer(2)));
    g.evaluate(&mut (), "(define! f (lambda! (x) (multiply x 10)))")
        .unwrap();
    assert_eq!(g.evaluate(&mut (), "(g 1)"), Ok(Value::Integer(10)));
    let top = g.evaluate(&mut (), "(names ((vau! () e e)))").unwrap();
    assert_eq!(show(&g, top), "(f g)");
    let built_in = g.evaluate(&mut (), "built-in-names").unwrap();
    let built_in = show(&g, built_in);
    for name in [
        "vau!",
        "first",
        "error",
        "define!",
        "lambda!",
        "equal?",
        "built-in-names",
    ] {
        assert!(
            built_in.split([' ', '(', ')']).any(|n| n == name),
            "{name} missing from {built_in}"
        );
    }
    // A top-level definition shadows the library without changing it, and the built-in
    // context can't be reached to change it there.
    g.evaluate(&mut (), "(define! list 5) (define! vau! 6)")
        .unwrap();
    assert_eq!(
        g.evaluate(
            &mut (),
            "(equal? (map first (quote! ((1) (2)))) (quote! (1 2)))"
        ),
        Ok(Value::Boolean(true))
    );
    fails(&mut g, &[("built-in-context", "unbound-symbol")]);
}

fn twice(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Integer(n)] => Ok(Value::Integer(n * 2)),
        _ => Err(g.error("type-error", args)),
    }
}

fn reenter(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    g.evaluate(&mut (), "(churn 2000)")?;
    Ok(args[0])
}

#[test]
fn host_functions() {
    let mut g = grift();
    define(&mut g, "twice", twice);
    define(&mut g, "reenter", reenter);
    assert_eq!(
        g.evaluate(&mut (), "(twice (add 1 2))"),
        Ok(Value::Integer(6))
    );
    let f = g.evaluate(&mut (), "twice").unwrap();
    assert_eq!(show(&g, f), "#[applicative #[host twice]]");
    fails(&mut g, &[("(twice 1 2)", "type-error")]);
    // A host function may evaluate; collections inside keep the caller's values.
    g.evaluate(&mut (), "(define! churn (lambda! (n) (if! (same? n 0) #true (sequence! (list 1 2 3) (churn (subtract n 1))))))")
        .unwrap();
    assert_eq!(
        g.evaluate(&mut (), "(length (reenter (list 1 2 3)))"),
        Ok(Value::Integer(3))
    );
}

/// `(halves "ab")` is `("ab" . "ab")`, made from Rust; `(sum (1 2 3))` walks a list from Rust.
fn halves(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    let s = String::from(g.as_str(args[0]).unwrap());
    let (a, b) = (g.string(&s)?, g.string(&s)?);
    g.pair(a, b)
}

fn sum(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    let (mut list, mut n) = (args[0], 0);
    while let Some((Value::Integer(x), rest)) = g.as_pair(list) {
        (list, n) = (rest, n + x);
    }
    Ok(Value::Integer(n))
}

#[test]
fn host_functions_make_and_read_data() {
    let mut g = grift();
    define(&mut g, "halves", halves);
    define(&mut g, "sum", sum);
    holds(
        &mut g,
        &[
            "(equal? (halves \"ab\") (pair \"ab\" \"ab\"))",
            "(equal? (sum (list 1 2 3)) 6)",
        ],
    );
    assert_eq!(g.as_pair(Value::Integer(1)), None);
}

/// Counts its calls in the host's state, and calls back into grift with it.
fn count(g: &mut Grift<u32>, calls: &mut u32, args: &[Value]) -> Result<Value, Error> {
    *calls += 1;
    match args {
        [] => Ok(Value::Integer(i64::from(*calls))),
        _ => g.evaluate(calls, "(count)"),
    }
}

#[test]
fn host_functions_get_the_host_state() {
    let mut g: Grift<u32> = Grift::new().unwrap();
    define(&mut g, "count", count);
    let mut calls = 0;
    assert_eq!(
        g.evaluate(&mut calls, "(count) (count 1)"),
        Ok(Value::Integer(3))
    );
    assert_eq!(calls, 3);
}

#[test]
fn the_host_calls_combiners_with_values() {
    let mut g = grift();
    let f = g.evaluate(&mut (), "(lambda! (x ys) (pair x ys))").unwrap();
    // The arguments are values: a symbol and a list are not evaluated again.
    let x = g.evaluate(&mut (), "(quote! undefined)").unwrap();
    let ys = g.evaluate(&mut (), "(quote! (first 1))").unwrap();
    let v = g.call(&mut (), f, &[x, ys]).unwrap();
    assert_eq!(show(&g, v), "(undefined first 1)");
    let first = g.evaluate(&mut (), "first").unwrap();
    let e = g.call(&mut (), first, &[Value::Integer(1)]).unwrap_err();
    assert_eq!(kind(&mut g, &e), "type-error");
    let quote = g.evaluate(&mut (), "quote!").unwrap();
    assert_eq!(
        g.call(&mut (), quote, &[Value::Integer(1)]),
        Ok(Value::Integer(1))
    );
}

static STOP: AtomicBool = AtomicBool::new(false);

#[test]
fn the_host_interrupts_a_runaway_evaluation() {
    let mut g = grift();
    g.set_interrupt_flag(&STOP);
    g.evaluate(&mut (), "(define! log ()) (define! here (current-context))")
        .unwrap();
    g.evaluate(&mut (), "(define! spin (lambda! () (spin)))")
        .unwrap();
    let setter = std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        STOP.store(true, Ordering::Relaxed);
    });
    let e = g
        .evaluate(
            &mut (),
            "(finally! (spin) (bind here (quote! log) (pair 1 log)))",
        )
        .unwrap_err();
    setter.join().unwrap();
    assert_eq!(kind(&mut g, &e), "interrupted");
    // The cleanup ran, the flag is clear again and the interpreter goes on.
    assert!(!STOP.load(Ordering::Relaxed));
    assert_eq!(g.evaluate(&mut (), "(length log)"), Ok(Value::Integer(1)));
}

#[test]
fn cleanup_runs_and_the_error_goes_on() {
    let mut g = grift();
    g.evaluate(&mut (), "(define! log ()) (define! here (current-context))")
        .unwrap();
    g.evaluate(
        &mut (),
        "(define! note (lambda! (x) (bind here (quote! log) (pair x log))))",
    )
    .unwrap();
    assert_eq!(
        g.evaluate(&mut (), "(finally! 5 (note 1) (note 2))"),
        Ok(Value::Integer(5))
    );
    // The error leaves through both cleanups, innermost first, unchanged.
    let e = g
        .evaluate(
            &mut (),
            "(finally! (finally! (raise (error (quote! mine) 7 (list 8))) (note 3)) (note 4))",
        )
        .unwrap_err();
    assert_eq!(kind(&mut g, &e), "mine");
    assert_eq!(e.values[0], Value::Integer(7));
    assert_eq!(show(&g, e.values[1]), "(8)");
    assert_eq!(
        show(&g, e.trace()[0]),
        "(raise (error (quote! mine) 7 (list 8)))"
    );
    let log = g.evaluate(&mut (), "log").unwrap();
    assert_eq!(show(&g, log), "(4 3 2 1)");
    // A cleanup that fails replaces the error; the outer cleanup still runs.
    let e = g
        .evaluate(&mut (), "(finally! (finally! (first 1) (rest 2)) (note 5))")
        .unwrap_err();
    assert_eq!(show(&g, e.values[0]), "#[primitive rest]");
    assert_eq!(g.evaluate(&mut (), "(first log)"), Ok(Value::Integer(5)));
    // A cleanup that succeeds doesn't lend the error its own location.
    let (protect, here) = (
        g.evaluate(&mut (), "evaluate-with-cleanup").unwrap(),
        g.evaluate(&mut (), "(current-context)").unwrap(),
    );
    let missing = Value::Symbol(g.symbol("missing").unwrap());
    let cleanup = g.evaluate(&mut (), "(lambda! () (list 1))").unwrap();
    let e = g
        .call(&mut (), protect, &[missing, here, cleanup])
        .unwrap_err();
    assert_eq!(
        (kind(&mut g, &e), e.trace()),
        (String::from("unbound-symbol"), &[][..])
    );
    // The waiting error's values survive collections in the cleanup.
    g.evaluate(&mut (), "(define! churn (lambda! (n) (if! (same? n 0) #true (sequence! (list 1 2 3) (churn (subtract n 1))))))")
        .unwrap();
    let e = g
        .evaluate(
            &mut (),
            "(finally! (raise (error (quote! mine) (list 1 2))) (churn 20000))",
        )
        .unwrap_err();
    assert_eq!(show(&g, e.values[0]), "(1 2)");
}

#[test]
fn stale_values_are_refused() {
    let mut g = grift();
    let old = g.evaluate(&mut (), "(list 1 2)").unwrap();
    g.evaluate(&mut (), "(define! churn (lambda! (n) (if! (same? n 0) #true (sequence! (list 1 2 3) (churn (subtract n 1))))))")
        .unwrap();
    g.evaluate(&mut (), "(churn 10000)").unwrap();
    assert_eq!(show(&g, old), "#[stale]");
    assert_eq!(g.as_str(old), None);
    let Value::Pair(r) = old else {
        panic!("a pair")
    };
    let e = g.pair_at(r).unwrap_err();
    assert_eq!(kind(&mut g, &e), "stale-value");
}

/// The machine steps taken and the operatives made evaluating `src`.
fn cost(g: &mut Grift, src: &str) -> (u64, usize) {
    (g.machine.steps, g.heap.operatives) = (0, 0);
    g.evaluate(&mut (), src).unwrap();
    (g.machine.steps, g.heap.operatives)
}

#[test]
fn library_forms_build_no_code() {
    // Each form against the same thing written by hand: a use makes no operative and takes at
    // most six times the steps. They used to build new code each time: let! made two
    // operatives, and conditional! took 29 times the steps.
    let mut g = grift();
    g.evaluate(&mut (), "(define! n 3) (define! e (current-context))")
        .unwrap();
    for (form, by_hand) in [
        (
            "(conditional! ((same? n 1) 1) ((same? n 2) 2) (#true 3))",
            "(if! (same? n 1) 1 (if! (same? n 2) 2 3))",
        ),
        ("(let! ((x (add n 1))) x)", "((lambda! (x) x) (add n 1))"),
        (
            "(let! ((x (add n 1)) (y x)) y)",
            "((lambda! (x) ((lambda! (y) y) x)) (add n 1))",
        ),
        ("(when! #true n)", "(if! #true n #inert)"),
        (
            "(sequence! 1 2 n)",
            "((lambda! (#ignore #ignore x) x) 1 2 n)",
        ),
        (
            "(and!? #true (same? n 3))",
            "(if! #true (same? n 3) #false)",
        ),
        (
            "(or!? #false (same? n 3))",
            "(if! #false #true (same? n 3))",
        ),
    ] {
        let ((steps, made), (by_hand_steps, _)) = (cost(&mut g, form), cost(&mut g, by_hand));
        assert_eq!(made, 0, "{form} made operatives");
        assert!(
            steps <= 6 * by_hand_steps,
            "{form}: {steps} steps against {by_hand_steps}"
        );
    }
}

#[test]
fn catch_catches_errors_as_contexts() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! log ()) (define! here (current-context))
         (define! note (lambda! (x) (bind here (quote! log) (pair x log))))
         (define! check (lambda! (x) (if! (integer? x) x (raise (error (quote! not-integer) x 2)))))",
    )
    .unwrap();
    holds(
        &mut g,
        &[
            // A value passes through; a caught error is a context with no parent binding kind,
            // values and trace.
            "(same? (catch! 5 e (first 1)) 5)",
            "(equal? (catch! (add 1 (check (quote! a))) e (list (evaluate-in! kind e) (evaluate-in! values e))) \
             (quote! (not-integer (a 2))))",
            "(equal? (catch! (first 1) e (evaluate-in! trace e)) (quote! ((first 1))))",
            // A handler that fails on being called still says where.
            "(equal? (catch! (call-with-handler (lambda! () (first 1)) (vau! () #ignore 1)) e (evaluate-in! trace e)) \
             (quote! ((call-with-handler (lambda! () (first 1)) (vau! () #ignore 1)))))",
            "(equal? (catch! (first 1) e (names e)) (quote! (kind trace values)))",
            "(catch! (first 1) e (not? (binds? e (quote! list))))",
            // error makes the same kind of context, with an empty trace, and raises nothing.
            "(sequence! (define! made (error (quote! mine) 1)) \
                        (and!? (equal? (names made) (quote! (kind trace values))) \
                               (equal? (evaluate-in! trace made) ()) (not? (binds? made (quote! list)))))",
            // The handler forms run in a child of the caller's context; the last one's value is
            // catch!'s.
            "(sequence! (define! y 1) (equal? (catch! (first 1) e (define! y 2) (add y 1)) 3))",
            "(equal? y 1)",
            // Early exit is catch! and the program's own kind of error.
            "(same? (catch! (sequence! (for-each (lambda! (x) (when! (same? x 3) (raise (error (quote! found) x)))) \
                                                (list 1 2 3 4)) \
                                      #false) \
                            e (first (evaluate-in! values e))) 3)",
            // Cleanups inside run first, innermost first.
            "(same? (catch! (finally! (finally! (first 1) (note 1)) (note 2)) #ignore (note 3) 7) 7)",
            "(equal? log (quote! (3 2 1)))",
            // catch! nests; an error in a handler goes to the catch! around it.
            "(same? (catch! (catch! (first 1) #ignore (rest 1)) e (same? (first (evaluate-in! values e)) (unwrap rest))) #true)",
            // catch! in a loop runs in constant space.
            "(sequence! (define! loop (lambda! (n) (if! (same? n 0) #true (sequence! (catch! (first n) #ignore 0) (loop (subtract n 1)))))) \
             (loop 20000))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(call-with-handler 1 first)", "type-error"),
            ("(let! ((x 1 2)) x)", "arity-error"),
            (
                "(catch! (first 1) e (raise (apply error (pair (quote! again) (evaluate-in! values e)))))",
                "again",
            ),
            ("(raise (quote! (kind (1))))", "type-error"),
            ("(raise (context))", "type-error"),
            ("(raise (sequence! (define! e (error 1)) e))", "type-error"),
        ],
    );
    // raise passes a caught error on as it was, its trace included.
    let e = g
        .evaluate(&mut (), "(catch! (add 1 (check (quote! a))) e (raise e))")
        .unwrap_err();
    assert_eq!(kind(&mut g, &e), "not-integer");
    let trace: Vec<String> = e.trace().iter().map(|&v| show(&g, v)).collect();
    assert_eq!(
        trace,
        [
            "(raise (error (quote! not-integer) x 2))",
            "(add 1 (check (quote! a)))",
            "(catch! (add 1 (check (quote! a))) e (raise e))"
        ]
    );
    // ... the raise itself not among them; a new error starts at its raise.
    let trace = g
        .evaluate(
            &mut (),
            "(catch! (catch! (first 1) e (raise e)) e (evaluate-in! trace e))",
        )
        .unwrap();
    assert_eq!(show(&g, trace), "((first 1))");
    let trace = g
        .evaluate(
            &mut (),
            "(catch! (raise (error (quote! mine))) e (evaluate-in! trace e))",
        )
        .unwrap();
    assert_eq!(show(&g, trace), "((raise (error (quote! mine))))");
    assert!(
        g.heap.len() < 100_000,
        "heap grew to {} slots",
        g.heap.len()
    );
}

fn stop(_: &mut Grift, _: &mut (), _: &[Value]) -> Result<Value, Error> {
    STOP.store(true, Ordering::Relaxed);
    Ok(Value::Inert)
}

/// A host function that fails as the program's mistake, or reports that memory ran out.
fn fail(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    match args {
        [] => Err(g.error("out-of-memory", &[])),
        _ => Err(g.error("bad-input", args)),
    }
}

#[test]
fn catch_catches_errors_about_the_program_and_never_the_limits() {
    let mut g = grift();
    g.set_interrupt_flag(&STOP);
    define(&mut g, "stop", stop);
    define(&mut g, "fail", fail);
    // An error about the program, a host function's included, is caught.
    let caught = g
        .evaluate(&mut (), "(catch! (fail 7) e (evaluate-in! kind e))")
        .unwrap();
    assert_eq!(show(&g, caught), "bad-input");
    // The allocator's limit and the host's go through every catch! to the host.
    let e = g
        .evaluate(&mut (), "(catch! (fail) #ignore 0)")
        .unwrap_err();
    assert_eq!(kind(&mut g, &e), "out-of-memory");
    // A cleanup that fails while a limit passes doesn't turn it into an error catch! catches.
    let e = g
        .evaluate(&mut (), "(catch! (finally! (stop) (first 1)) #ignore 0)")
        .unwrap_err();
    assert_eq!(kind(&mut g, &e), "interrupted");
    // ... even through a catch! in a loop that would catch anything.
    let e = g
        .evaluate(
            &mut (),
            "(define! spin (lambda! () (catch! (sequence! (stop) (spin)) #ignore (spin)))) (spin)",
        )
        .unwrap_err();
    assert_eq!(kind(&mut g, &e), "interrupted");
}

#[test]
fn binds_looks_through_parents() {
    let mut g = grift();
    g.evaluate(&mut (), "(define! x 1) (define! e (current-context))")
        .unwrap();
    holds(
        &mut g,
        &[
            "(and!? (binds? e (quote! x)) (binds? e (quote! first)) (not? (binds? e (quote! nope))) \
                    (not? (binds? (context) (quote! first))))",
        ],
    );
    // Both operands are evaluated: a computed symbol is the one looked up, not the name it is
    // held in.
    holds(
        &mut g,
        &[
            "(binds? e (string-to-symbol \"x\"))",
            "(not? (binds? e (string-to-symbol \"nope\")))",
            "(equal? (catch! (binds? e 1) x (evaluate-in! values x)) (quote! (binds? 1)))",
            "(sequence! (define! s (quote! nope)) (not? (binds? e s)))",
            "(binds? (context e) (quote! first))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(binds? 1 (quote! x))", "type-error"),
            ("(binds? e 1)", "type-error"),
            ("(binds? e)", "arity-error"),
        ],
    );
}

#[test]
fn errors_name_each_program_combination_around_them() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! check (lambda! (x) (if! (integer? x) x (raise (error (quote! not-int) x)))))
         (define! run (lambda! (x) (add 1 (check x))))
         (define! loop (lambda! (n) (if! (same? n 0) (check (quote! a)) (loop (subtract n 1)))))
         (define! deep (lambda! (n) (if! (same? n 0) (check (quote! a)) (add 1 (deep (subtract n 1))))))",
    )
    .unwrap();
    // A helper's error names the caller, out to the top-level form; a call in tail position has
    // finished, so it isn't listed, and each combination is listed once, however deep.
    for (src, want) in [
        (
            "(run (quote! a))",
            "(raise (error (quote! not-int) x)) (add 1 (check x)) (run (quote! a))",
        ),
        (
            "(loop 100000)",
            "(raise (error (quote! not-int) x)) (loop 100000)",
        ),
        (
            "(deep 10000)",
            "(raise (error (quote! not-int) x)) (add 1 (deep (subtract n 1))) (deep 10000)",
        ),
        ("(first 1)", "(first 1)"),
    ] {
        let e = g.evaluate(&mut (), src).unwrap_err();
        let at: Vec<String> = e.trace().iter().map(|&v| show(&g, v)).collect();
        assert_eq!(at.join(" "), want, "{src}");
    }
}

#[test]
fn strings_by_byte_offsets() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            // Lengths and offsets count bytes: "λx" is three.
            "(equal? (string-byte-length \"λx\") 3)",
            "(equal? (list (string-byte \"λx\" 0) (string-byte \"λx\" 2)) (quote! (206 120)))",
            "(same? (byte-substring \"λx\" 2 3) \"x\")",
            "(same? (byte-substring \"abc\" 1 1) \"\")",
            "(same? (string-append \"a\" (code-point-to-string 955) \"\") \"aλ\")",
            "(same? (string-append) \"\")",
            // Symbols and integers to strings and back.
            "(same? (string-to-symbol (symbol-to-string (quote! foo?))) (quote! foo?))",
            "(same? (symbol-to-string (quote! a)) \"a\")",
            "(equal? (map integer-to-string (list 0 -9223372036854775808)) (list \"0\" \"-9223372036854775808\"))",
        ],
    );
    fails(
        &mut g,
        &[
            // Off a character boundary, out of range, or not a Unicode scalar value.
            ("(byte-substring \"λ\" 0 1)", "range-error"),
            ("(byte-substring \"abc\" 2 1)", "range-error"),
            ("(byte-substring \"abc\" 0 4)", "range-error"),
            ("(string-byte \"abc\" 3)", "range-error"),
            ("(string-byte \"abc\" -1)", "range-error"),
            ("(code-point-to-string 55296)", "range-error"),
            ("(code-point-to-string -1)", "range-error"),
            ("(string-byte-length (quote! a))", "type-error"),
            ("(string-append \"a\" 1)", "type-error"),
            ("(symbol-to-string \"a\")", "type-error"),
            ("(float-to-string 1)", "type-error"),
            ("(integer-to-string \"1\")", "type-error"),
        ],
    );
}

/// The value of `src`, which must be a float.
fn float(g: &mut Grift, src: &str) -> f64 {
    match g.evaluate(&mut (), src) {
        Ok(Value::Float(f)) => f,
        other => panic!("{src} gave {other:?}"),
    }
}

const FLOATS: [f64; 12] = [
    0.0,
    -0.0,
    0.1,
    -2.5,
    1.0,
    1e300,
    5e-324,
    123456.789,
    f64::MAX,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
];

#[test]
fn floats_read_back_as_they_print() {
    let mut g = grift();
    for x in FLOATS {
        let printed = show(&g, Value::Float(x));
        let again = float(&mut g, &printed);
        assert_eq!(again.to_bits(), x.to_bits(), "{printed}");
        // value-to-string gives the printer's text, and string-to-float reads it back (a
        // not-a-number only up to its sign, which these floats don't vary).
        let s = g
            .evaluate(&mut (), &format!("(value-to-string {printed})"))
            .unwrap();
        assert_eq!(g.as_str(s), Some(printed.as_str()));
        let again = float(
            &mut g,
            &format!("(string-to-float (value-to-string {printed}))"),
        );
        assert_eq!(again.to_bits(), x.to_bits(), "{printed}");
    }
    // A not-a-number of any sign reads back as a not-a-number.
    let negative = show(&g, Value::Float(-f64::NAN));
    assert!(float(&mut g, &negative).is_nan());
    holds(
        &mut g,
        &[
            "(float? 1.5)",
            "(float? 1e3)",
            "(integer? 1000)",
            "(same? (string-to-float \"1\") 1.0)",
            "(same? (value-to-string 1.0) \"1.0\")",
            "(same? (float-to-string 1.0) \"1.0\")",
            "(same? (float-to-string -2.5e-7) (value-to-string -2.5e-7))",
            "(same? (string-to-float \"inf\") #infinity)",
            "(same? (string-to-integer \"-42\") -42)",
            "(same? (integer-to-string -42) \"-42\")",
        ],
    );
    fails(
        &mut g,
        &[
            ("1e", "parse-error"),
            ("1.2.3", "parse-error"),
            ("#nan", "parse-error"),
            ("(string-to-integer \"4x\")", "parse-error"),
            (
                "(string-to-integer \"99999999999999999999\")",
                "parse-error",
            ),
            ("(string-to-integer \" 1\")", "parse-error"),
            ("(string-to-float \"one\")", "parse-error"),
            ("(string-to-float 1.0)", "type-error"),
        ],
    );
}

#[test]
fn arithmetic_is_rusts() {
    let mut g = grift();
    let integers = [0, 1, -1, 7, -7, 2, -2, i64::MAX, i64::MIN];
    for a in integers {
        for b in integers {
            for (name, rust) in [
                ("add", a.checked_add(b)),
                ("subtract", a.checked_sub(b)),
                ("multiply", a.checked_mul(b)),
                ("divide", a.checked_div(b)),
                ("remainder", a.checked_rem(b)),
            ] {
                let got = g.evaluate(&mut (), &format!("({name} {a} {b})"));
                match (rust, got) {
                    (Some(n), Ok(v)) => assert_eq!(v, Value::Integer(n), "({name} {a} {b})"),
                    (None, Err(e)) => {
                        let want = if b == 0 && name.starts_with(['d', 'r']) {
                            "division-by-zero"
                        } else {
                            "overflow"
                        };
                        assert_eq!(kind(&mut g, &e), want, "({name} {a} {b})");
                    }
                    (rust, got) => panic!("({name} {a} {b}): Rust {rust:?}, grift {got:?}"),
                }
            }
            let less = g.evaluate(&mut (), &format!("(less-than? {a} {b})"));
            assert_eq!(less, Ok(Value::Boolean(a < b)));
        }
    }
    let floats = FLOATS.map(|x| show(&g, Value::Float(x)));
    for (x, a) in FLOATS.iter().zip(&floats) {
        for (y, b) in FLOATS.iter().zip(&floats) {
            for (name, rust) in [
                ("add", x + y),
                ("subtract", x - y),
                ("multiply", x * y),
                ("divide", x / y),
                ("remainder", x % y),
                ("power", x.powf(*y)),
            ] {
                let got = float(&mut g, &format!("({name} {a} {b})"));
                assert_eq!(got.to_bits(), rust.to_bits(), "({name} {a} {b})");
            }
        }
        for (name, rust) in [
            ("square-root", x.sqrt()),
            ("sine", x.sin()),
            ("cosine", x.cos()),
            ("tangent", x.tan()),
            ("exponential", x.exp()),
            ("natural-logarithm", x.ln()),
            ("floor", x.floor()),
            ("ceiling", x.ceil()),
            ("round", x.round()),
            ("truncate", x.trunc()),
            ("absolute-value", x.abs()),
        ] {
            let got = float(&mut g, &format!("({name} {a})"));
            assert_eq!(got.to_bits(), rust.to_bits(), "({name} {a})");
        }
        // float-to-integer is Rust's `as`: saturating, and not-a-number is 0.
        let got = g.evaluate(&mut (), &format!("(float-to-integer {a})"));
        assert_eq!(got, Ok(Value::Integer(*x as i64)), "(float-to-integer {a})");
    }
    for n in [0, -3, i64::MAX, i64::MIN] {
        let got = float(&mut g, &format!("(integer-to-float {n})"));
        assert_eq!(got.to_bits(), (n as f64).to_bits());
    }
    // Integers and floats never mix, and the float functions take floats only.
    fails(
        &mut g,
        &[
            ("(add 1 1.5)", "type-error"),
            ("(add 1.5 1)", "type-error"),
            ("(less-than? 1 1.5)", "type-error"),
            ("(multiply 2.0 \"a\")", "type-error"),
            ("(square-root 4)", "type-error"),
            ("(absolute-value -1)", "type-error"),
            ("(power 2 3)", "type-error"),
            ("(integer-to-float 1.0)", "type-error"),
            ("(float-to-integer 1)", "type-error"),
        ],
    );
}

#[test]
fn comparisons_agree_with_rust() {
    let mut g = grift();
    let integers = [-1, 0, 1].map(Value::Integer);
    let floats = [-1.0, 0.0, -0.0, 1.0, f64::NAN, f64::INFINITY].map(Value::Float);
    for values in [&integers[..], &floats[..]] {
        for &x in values {
            for &y in values {
                let (a, b) = (show(&g, x), show(&g, y));
                // Rust's own order of the two, None with not-a-number.
                let order = match (x, y) {
                    (Value::Integer(x), Value::Integer(y)) => x.partial_cmp(&y),
                    (Value::Float(x), Value::Float(y)) => x.partial_cmp(&y),
                    _ => panic!("two integers or two floats"),
                };
                let (less, equal, greater) = (
                    order == Some(std::cmp::Ordering::Less),
                    order == Some(std::cmp::Ordering::Equal),
                    order == Some(std::cmp::Ordering::Greater),
                );
                for (name, want) in [
                    ("less-than?", less),
                    ("greater-than?", greater),
                    ("less-than-or-equal?", less || equal),
                    ("greater-than-or-equal?", greater || equal),
                    ("equal?", equal),
                ] {
                    let got = g.evaluate(&mut (), &format!("({name} {a} {b})"));
                    let want = Value::Boolean(want);
                    assert_eq!(got, Ok(want), "({name} {a} {b})");
                }
            }
        }
    }
    holds(
        &mut g,
        &[
            "(and!? (same? (minimum 1 2) 1) (same? (maximum 1 2) 2))",
            "(and!? (same? (minimum 1.5 -2.0) -2.0) (same? (maximum 1.5 -2.0) 1.5))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(greater-than-or-equal? 1 1.0)", "type-error"),
            ("(minimum 1 1.0)", "type-error"),
        ],
    );
}

#[test]
fn same_and_equal() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            // same? is identity for pairs, contexts and combiners, and equality of value for
            // integers, symbols and strings.
            "(same? 1 1)",
            "(same? (quote! a) (quote! a))",
            "(same? \"ab\" (string-append \"a\" \"b\"))",
            "(not? (same? (list 1) (list 1)))",
            "(not? (same? (context) (context)))",
            "(not? (same? (lambda! () 1) (lambda! () 1)))",
            "(sequence! (define! p (list 1)) (same? p p))",
            "(same? first first)",
            // Floats: same? compares the stored bits, equal? as IEEE does.
            "(not? (same? 0.0 -0.0))",
            "(equal? 0.0 -0.0)",
            "(same? #not-a-number #not-a-number)",
            "(not? (equal? #not-a-number #not-a-number))",
            "(not? (equal? #not-a-number 1.0))",
            "(equal? #infinity #infinity)",
            // equal? compares pairs by structure, and everything else as same? does.
            "(equal? (list 1 (list 2.0 \"x\")) (list 1 (list 2.0 \"x\")))",
            "(not? (equal? (list 1 2) (list 1 3)))",
            "(not? (equal? (context) (context)))",
            // Values of different types are never equal.
            "(not? (equal? 1 1.0))",
            "(not? (same? 1 1.0))",
            "(not? (equal? \"a\" (quote! a)))",
            "(not? (equal? () #false))",
            "(not? (equal? (list 1.0) (list 1)))",
        ],
    );
}

#[test]
fn a_predicate_for_every_type() {
    let mut g = grift();
    let predicates = [
        "empty-list?",
        "boolean?",
        "integer?",
        "float?",
        "symbol?",
        "pair?",
        "string?",
        "context?",
        "operative?",
        "applicative?",
    ];
    let values = [
        "()",
        "#false",
        "1",
        "1.0",
        "(quote! a)",
        "(list 1)",
        "\"s\"",
        "(context)",
        "if!",
        "first",
    ];
    // Each value's type-of names exactly one predicate, and only that one holds. #inert and #ignore,
    // each the only value of its type, have none: `same?` says it.
    for value in values {
        let kind = g
            .evaluate(&mut (), &format!("(symbol-to-string (type-of {value}))"))
            .unwrap();
        let kind = format!("{}?", g.as_str(kind).unwrap());
        assert!(predicates.contains(&kind.as_str()), "{value}: {kind}");
        for p in predicates {
            let want = Value::Boolean(p == kind);
            assert_eq!(
                g.evaluate(&mut (), &format!("({p} {value})")),
                Ok(want),
                "({p} {value})"
            );
        }
    }
    // A primitive and a host function are operatives too.
    holds(
        &mut g,
        &["(and!? (operative? vau!) (operative? (unwrap first)) (applicative? list))"],
    );
}

#[test]
fn if_and_let() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            // if! has both branches and evaluates one; when! has one branch, a body of several
            // forms evaluated in order, the last one's value its own, and #inert on a false test.
            "(same? (if! #true 1 (first ())) 1)",
            "(same? (if! #false (first ()) 2) 2)",
            "(same? (when! #true 1) 1)",
            "(same? (when! #false (first ())) #inert)",
            "(same? (when! #true) #inert)",
            "(sequence! (define! seen ()) (define! here (current-context)) \
             (equal? (list (when! #true (bind here (quote! seen) (pair 1 seen)) (bind here (quote! seen) (pair 2 seen)) 3) seen) \
                     (quote! (3 (2 1)))))",
            // conditional!: the body of the first clause whose test is #true, its forms in order;
            // #inert if there is none or the body is empty; later tests aren't evaluated.
            "(same? (conditional! (#false (first ())) (#true 1) (#true 2) ((first ()) 3)) 1)",
            "(sequence! (define! seen ()) (define! here (current-context)) \
             (equal? (list (conditional! (#false 0) (#true (bind here (quote! seen) (pair 1 seen)) (bind here (quote! seen) (pair 2 seen)) 3)) seen) \
                     (quote! (3 (2 1)))))",
            "(and!? (same? #inert (conditional!)) (same? #inert (conditional! (#false 1))) (same? #inert (conditional! (#true))))",
            // let! binds in order in one new context: each binding sees the ones before it, and a
            // combiner it binds can call itself and the ones after it.
            "(same? (let! ((x 1) (y (add x 1))) y) 2)",
            "(same? (let! ((even? (lambda! (n) (if! (same? n 0) #true (odd? (subtract n 1))))) \
                           (odd? (lambda! (n) (if! (same? n 0) #false (even? (subtract n 1)))))) \
                      (even? 100)) #true)",
            "(equal? (let! ((x 1) (y 2)) (names (current-context))) (quote! (x y)))",
            "(sequence! (define! x 10) (same? (let! ((x (add x 1))) x) 11))",
            "(equal? x 10)",
            // ... and each binding's parameters are a tree.
            "(same? (let! (((a b) (list 1 2)) (c (add a b))) c) 3)",
        ],
    );
    fails(
        &mut g,
        &[
            ("(if! #false 1 2 3)", "arity-error"),
            ("(when!)", "arity-error"),
            ("(when! 1 2)", "type-error"),
            ("(when! (quote! a))", "type-error"),
            ("(unless! #false 1)", "unbound-symbol"),
            ("(conditional! 1)", "arity-error"),
            ("(conditional! (#false 1) ())", "arity-error"),
            ("(let*! ((x 1)) x)", "unbound-symbol"),
            ("(let! ((x)) x)", "arity-error"),
            ("(let! (x) x)", "arity-error"),
        ],
    );
}

#[test]
fn no_truthiness() {
    let mut g = grift();
    // Every test must be a boolean: anything else is a type-error, never true or false.
    fails(
        &mut g,
        &[
            ("(if! 1 2 3)", "type-error"),
            ("(if! () 2 3)", "type-error"),
            ("(when! () 2)", "type-error"),
            ("(conditional! (1 2))", "type-error"),
            ("(conditional! (#false 1) ((quote! a) 2))", "type-error"),
            ("(and!? 1)", "type-error"),
            ("(and!? #true 5)", "type-error"),
            ("(or!? #false ())", "type-error"),
            ("(or!? 0 #true)", "type-error"),
            ("(not? 0)", "type-error"),
            ("(select \"\" 1 2)", "type-error"),
        ],
    );
}

#[test]
fn contexts() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! parent (context)) (bind parent (quote! (b a)) (list 1 2))
         (define! child (context parent)) (bind child (quote! c) 3)",
    )
    .unwrap();
    holds(
        &mut g,
        &[
            // names lists a context's own names, sorted; built-in-names the built-in context's.
            "(equal? (names child) (quote! (c)))",
            "(equal? (names parent) (quote! (a b)))",
            "(member? (quote! names) built-in-names)",
            "(not? (member? (quote! parent) built-in-names))",
            // evaluate-in! evaluates in the context given, through its parents, and only there.
            "(and!? (same? (evaluate-in! a child) 2) (same? (evaluate-in! c child) 3))",
            "(catch! (evaluate-in! (add a c) child) e (equal? (evaluate-in! values e) (quote! (add))))",
            // binds? looks through parents.
            "(and!? (binds? child (quote! a)) (binds? child (quote! c)) (not? (binds? parent (quote! c))))",
            // bind binds in the context given, never in its parent.
            "(sequence! (bind child (quote! a) 10) (and!? (same? (evaluate-in! a child) 10) \
                                                       (same? (evaluate-in! a parent) 2)))",
            "(equal? (names child) (quote! (a c)))",
            // current-context is the context it is called in.
            "(same? ((lambda! (x) (evaluate-in! x (current-context))) 5) 5)",
        ],
    );
    // A context prints as its own names, sorted, never its values.
    let child = g.evaluate(&mut (), "child").unwrap();
    assert_eq!(show(&g, child), "#[context a c]");
    let empty = g.evaluate(&mut (), "(context)").unwrap();
    assert_eq!(show(&g, empty), "#[context]");
    let e = g.evaluate(&mut (), "(error (quote! mine) 1)").unwrap();
    assert_eq!(show(&g, e), "#[context kind trace values]");
}

/// Whether a test program finishes within this: about 5 times what it takes (2 s) and less than
/// a search per frame takes (14 s), in a release build at a load of about 8 on 32 cores.
const LINEAR: Duration = Duration::from_secs(10);

#[test]
fn unwinding_and_checking_parameters_search_nothing() {
    // 300,000 frames, each around a combination made for it, so the trace holds each of them.
    // Through a cleanup per frame, too.
    for around in ["x", "(finally! x #inert)"] {
        let mut g = grift();
        let deep = format!(
            "(define! here (current-context))
             (define! deep
               (lambda! (n)
                 (if! (same? n 0)
                   (raise (error (quote! bottom)))
                   (evaluate (list (quote! add) 1 (list (quote! wrapped) (subtract n 1))) here))))
             (define! wrapped (vau! (n) caller ((lambda! (x) {around}) (deep (evaluate n caller)))))
             (catch! (deep 300000) e (length (evaluate-in! trace e)))"
        );
        let start = Instant::now();
        let traced = g.evaluate(&mut (), &deep).unwrap();
        assert!(start.elapsed() < LINEAR, "{:?}", start.elapsed());
        assert!(matches!(traced, Value::Integer(n) if n >= 300_000));
    }
    // 300,000 parameters, in the order they intern fastest: 9 s with a search, 0.05 s without.
    let mut g = grift();
    let names: Vec<String> = (0..300_000).map(|i| format!("p{i:06}")).collect();
    let start = Instant::now();
    let good = format!("((wrap vau!) (quote! ({})) #ignore 1)", names.join(" "));
    assert!(g.evaluate(&mut (), &good).is_ok());
    assert!(start.elapsed() < LINEAR, "{:?}", start.elapsed());
    let twice = format!(
        "((wrap vau!) (quote! ({} p000000)) #ignore 1)",
        names.join(" ")
    );
    fails(&mut g, &[(twice.as_str(), "parameter-error")]);
}

#[test]
fn a_trace_keeps_each_combination_once_and_a_given_trace_as_it_is() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            // A recursion passes one combination many times.
            "(sequence!
               (define! down (lambda! (n) (if! (same? n 0) (raise (error (quote! bottom))) (add 1 (down (subtract n 1))))))
               (define! trace (catch! (down 100) e (evaluate-in! trace e)))
               (equal? (length trace) (length (fold (lambda! (seen x) (if! (member? x seen) seen (pair x seen))) () trace))))",
            // A caught error raised again passes the same combinations again, at every level.
            "(sequence!
               (define! up
                 (lambda! (n)
                   (if! (same? n 0)
                     (raise (error (quote! bottom)))
                     (catch! (add 1 (up (subtract n 1))) e (raise e)))))
               (define! trace (catch! (up 20) e (evaluate-in! trace e)))
               (equal? (length trace) (length (fold (lambda! (seen x) (if! (member? x seen) seen (pair x seen))) () trace))))",
            // What a raise is given stays, repeats and all, and what it passes is added once.
            "(sequence!
               (define! e (error (quote! mine)))
               (define! at (quote! (a b)))
               (bind e (quote! trace) (list 1 at at))
               (define! trace (catch! (add 1 (raise e)) caught (evaluate-in! trace caught)))
               (and!? (equal? (first trace) 1)
                      (same? (first (rest trace)) at)
                      (same? (first (rest (rest trace))) at)
                      (equal? (length trace) 4)))",
        ],
    );
}

/// `s` as a grift string literal.
fn literal(s: &str) -> String {
    let mut g = grift();
    let v = g.string(s).unwrap();
    show(&g, v)
}

#[test]
fn read_form_is_the_program_reader() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! forms
           (lambda! (text at)
             (let! ((read (read-form text at)))
               (if! (empty-list? read)
                 ()
                 (let! (((form start end) read))
                   ; The text between its offsets is the form's, and reads back as the form.
                   (if! (equal? (first (read-form (byte-substring text start end) 0)) form)
                     (pair form (forms text end))
                     (raise (error (quote! not-the-form) form))))))))",
    )
    .unwrap();
    // Form by form, it reads what the program reader reads.
    for text in [
        "",
        "  ; only a comment\n",
        "1 -2.5 1e3 #true #inert () (a . b) (a (b \"c\\n\" . d) e) \"é\" sym ; end",
        "(first\n  ; between\n  (quote! (x)))\n\"\\x41;\"",
        "a(b)c\"d\"e",
    ] {
        let t = literal(text);
        holds(
            &mut g,
            &[&format!("(equal? (forms {t} 0) (quote! ({text}\n)))")],
        );
    }
    holds(
        &mut g,
        &[
            // The form, where it starts past space and comments, and the offset right after it; a
            // form that is () from the end of the text.
            "(equal? (read-form \"  (add 1 2.5) x\" 0) (quote! ((add 1 2.5) 2 13)))",
            "(equal? (read-form \"  (add 1 2.5) x\" 13) (quote! (x 14 15)))",
            "(equal? (read-form \"é x\" 2) (quote! (x 3 4)))",
            "(equal? (read-form \"; é\\n\\xc;\\xa0; ()\" 0) (quote! (() 9 11)))",
            "(empty-list? (read-form \"() \" 2))",
            "(empty-list? (read-form \"\" 0))",
            // It evaluates nothing, and what it reads evaluates as written.
            "(same? (evaluate (first (read-form \"(add 1 2)\" 0)) (current-context)) 3)",
            "(pair? (first (read-form \"(no-such-thing)\" 0)))",
        ],
    );
    // A parse-error says the line and column in the whole text, as the program reader's does.
    for bad in [
        ")",
        "(a",
        "\n  (a . )",
        "\"é\n",
        "(a\n#nope)",
        "1.2.3",
        "(. a)",
        "\"\\q\"",
    ] {
        let whole = g.read(bad).unwrap_err();
        assert_eq!(kind(&mut g, &whole), "parse-error");
        let one = g
            .evaluate(&mut (), &format!("(read-form {} 0)", literal(bad)))
            .unwrap_err();
        assert_eq!(
            (kind(&mut g, &one), one.values),
            ("parse-error".into(), whole.values)
        );
    }
    let e = g
        .evaluate(&mut (), "(read-form \"a\\nb\\n)\" 4)")
        .unwrap_err();
    assert_eq!(e.values, [Value::Integer(3), Value::Integer(1)]);
    fails(
        &mut g,
        &[
            ("(read-form 1 0)", "type-error"),
            ("(read-form \"a\" 1.0)", "type-error"),
            ("(read-form \"a\" 2)", "range-error"),
            ("(read-form \"a\" -1)", "range-error"),
            ("(read-form \"é\" 1)", "range-error"),
        ],
    );
    // The text is all there afterwards, whatever the outcome.
    holds(
        &mut g,
        &["(sequence!
               (define! t \"(a) (\")
               (read-form t 0)
               (catch! (read-form t 3) #ignore #inert)
               (catch! (read-form t 9) #ignore #inert)
               (same? t \"(a) (\"))"],
    );
}

#[test]
fn value_to_string_is_the_printer() {
    let mut g = grift();
    for src in [
        "(list 1 1.0 -0.5 #infinity \"a\\n\\\"b\" (quote! (s . t)) () #true #inert #ignore)",
        "first",
        "vau!",
        "(lambda! (x) (add x 1))",
        "(vau! (x . y) caller (x y))",
        "(sequence! (define! c (context)) (bind c (quote! b) 1) (bind c (quote! a) 2) c)",
    ] {
        let v = g.evaluate(&mut (), src).unwrap();
        let printed = show(&g, v);
        let text = g
            .evaluate(&mut (), &format!("(value-to-string {src})"))
            .unwrap();
        assert_eq!(g.as_str(text), Some(printed.as_str()));
    }
    // What can be read back reads back, by read-form.
    holds(
        &mut g,
        &[
            "(sequence!
               (define! v (quote! (1 -2.5 \"s\\t\" (a . b) () #false)))
               (equal? (first (read-form (value-to-string v) 0)) v))",
            "(equal? (read-form \"#!/x\\n1\" 0) (quote! (1 5 6)))",
            "(float? (first (read-form (value-to-string 1.0) 0)))",
        ],
    );
}

#[test]
fn member_and_find_association_cost_what_a_loop_by_hand_does() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! xs (quote! (1 2 3 4)))
         (define! ps (quote! ((1 . a) (2 . b) (3 . c) (4 . d))))
         (define! member-by-hand
           (lambda! (x xs)
             (if! (same? xs ()) #false (if! (equal? x (first xs)) #true (member-by-hand x (rest xs))))))
         (define! find-by-hand
           (lambda! (x ps)
             (if! (same? ps ())
               ()
               (if! (equal? x (first (first ps))) (first ps) (find-by-hand x (rest ps))))))",
    )
    .unwrap();
    for (form, by_hand) in [
        ("(member? 9 xs)", "(member-by-hand 9 xs)"),
        ("(find-association 9 ps)", "(find-by-hand 9 ps)"),
    ] {
        let ((steps, _), (by_hand_steps, _)) = (cost(&mut g, form), cost(&mut g, by_hand));
        assert!(
            steps <= by_hand_steps,
            "{form}: {steps} against {by_hand_steps}"
        );
    }
    holds(
        &mut g,
        &[
            "(member? 4 xs)",
            "(not? (member? 9 xs))",
            "(member? (list 1) (list 2 (list 1)))",
            "(member? \"a\" (list \"a\"))",
            // equal?'s meaning: not-a-number is equal to nothing, and -0.0 to 0.0.
            "(not? (member? #not-a-number (list #not-a-number)))",
            "(member? 0.0 (list -0.0))",
            "(equal? (find-association 2 ps) (quote! (2 . b)))",
            "(empty-list? (find-association 9 ps))",
            "(equal? (find-association 0.0 (list (pair -0.0 1))) (pair -0.0 1))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(member? 1 2)", "type-error"),
            ("(find-association 1 (list 2))", "type-error"),
        ],
    );
}

#[test]
fn list_helpers() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! xs (range -3 4))
         (define! even? (lambda! (x) (same? (remainder x 2) 0)))
         (define! calls 0)
         (define! counting (lambda! (x) (bind top (quote! calls) (add calls 1)) (even? x)))
         (define! top (current-context))",
    )
    .unwrap();
    holds(
        &mut g,
        &[
            // range: as many as the distance, each one more than the last.
            "(equal? xs (list -3 -2 -1 0 1 2 3))",
            "(same? (length (range 5 105)) 100)",
            "(empty-list? (range 3 3))",
            "(empty-list? (range 3 2))",
            // filter: the elements that pass, in order, and the others are the rest of the list.
            "(all? even? (filter even? xs))",
            "(same? (add (length (filter even? xs))
                         (length (filter (lambda! (x) (not? (even? x))) xs)))
                    (length xs))",
            "(equal? (filter even? xs) (list -2 0 2))",
            // fold: from the left, the sum of a range by its formula.
            "(same? (fold add 0 (range 0 101)) 5050)",
            "(equal? (fold pair () (list 1 2)) (pair (pair () 1) 2))",
            "(same? (fold add 7 ()) 7)",
            // any? and all? negate each other, and stop at the answer.
            "(same? (any? even? xs) (not? (all? (lambda! (x) (not? (even? x))) xs)))",
            "(not? (any? even? ()))",
            "(all? even? ())",
            "(sequence! (any? counting (list 2 1 3)) (same? calls 1))",
            "(sequence! (bind top (quote! calls) 0) (all? counting (list 1 2 4)) (same? calls 1))",
            // map: with one list as before; with several, element by element.
            "(equal? (map even? (list 1 2)) (list #false #true))",
            "(equal? (map add (list 1 2) (list 10 20)) (list 11 22))",
            "(equal? (map list (list 1) (list 2) (list 3)) (list (list 1 2 3)))",
            "(empty-list? (map add () ()))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(filter add (list 1))", "arity-error"),
            ("(filter first (list (list 1)))", "type-error"),
            ("(any? first (list (list 1)))", "type-error"),
            ("(all? first (list (list 1)))", "type-error"),
            ("(range 1 1.5)", "type-error"),
            ("(range 1.0 2.0)", "type-error"),
            ("(map add (list 1) (list 2 3))", "type-error"),
            ("(map add (list 1 2) (list 3))", "type-error"),
            ("(map add () (list 3))", "type-error"),
        ],
    );
}

#[test]
fn string_helpers() {
    let mut g = grift();
    // Each case holds for every text and pattern here, UTF-8 included.
    g.evaluate(
        &mut (),
        "(define! texts (list \"\" \"a\" \"a,b,,c\" \",\" \",,\" \"é,ü\" \"日本語,日本\" \"Hello, World\"))
         (define! patterns (list \",\" \"b\" \"é\" \"日本\" \", \" \"xyz\"))
         (define! every-pair
           (lambda! (test)
             (all? (lambda! (t) (all? (lambda! (p) (test t p)) patterns)) texts)))",
    )
    .unwrap();
    holds(
        &mut g,
        &[
            // string-split and string-join undo each other, and no part holds the separator.
            "(every-pair (lambda! (t p) (same? (string-join (string-split t p) p) t)))",
            "(every-pair (lambda! (t p)
               (all? (lambda! (part) (empty-list? (string-search part p 0))) (string-split t p))))",
            "(equal? (string-split \"a,,b\" \",\") (list \"a\" \"\" \"b\"))",
            "(equal? (string-split \"\" \",\") (list \"\"))",
            "(same? (string-join () \",\") \"\")",
            "(same? (string-join (list \"a\") \",\") \"a\")",
            // string-search: the pattern is at the offset it gives, and it is at none before it.
            "(every-pair (lambda! (t p)
               (let! ((at (string-search t p 0)) (size (string-byte-length p)))
                 (if! (empty-list? at)
                   (same? (length (string-split t p)) 1)
                   (and!? (same? (byte-substring t at (add at size)) p)
                          (empty-list? (string-search (byte-substring t 0 at) p 0)))))))",
            "(same? (string-search \"abcabc\" \"c\" 3) 5)",
            "(same? (string-search \"abc\" \"\" 3) 3)",
            "(empty-list? (string-search \"abc\" \"abcd\" 0))",
            // Every text starts with what it starts with and ends with what it ends with.
            "(every-pair (lambda! (t p)
               (and!? (string-starts-with? (string-append p t) p)
                      (string-ends-with? (string-append t p) p))))",
            "(not? (string-starts-with? \"é\" \"a\"))",
            "(not? (string-starts-with? \"a\" \"ab\"))",
            "(not? (string-ends-with? \"é\" \"a\"))",
            "(not? (string-ends-with? \"b\" \"ab\"))",
            // string-trim: nothing blank is left at the ends, nothing else goes, and once is enough.
            "(same? (string-trim \" \\t\\n a b\\xd;\\n \") \"a b\")",
            "(same? (string-trim \"  \\n\") \"\")",
            "(all? (lambda! (t) (same? (string-trim (string-trim t)) (string-trim t))) texts)",
            "(all? (lambda! (t) (same? (string-trim (string-append \" \" t \"\\n\")) t)) texts)",
            // The pads: at least the width, the text unchanged, the spaces on the side named.
            "(all? (lambda! (t)
               (and!? (same? (string-byte-length (string-pad-left t 30)) 30)
                      (string-ends-with? (string-pad-left t 30) t)
                      (string-starts-with? (string-pad-right t 30) t)
                      (same? (string-pad-right t 0) t)
                      (same? (string-pad-left t 0) t)))
               texts)",
            "(same? (string-pad-left \"ab\" 4) \"  ab\")",
            "(same? (string-pad-right \"ab\" 4) \"ab  \")",
            "(same? (string-pad-left \"ab\" -9223372036854775808) \"ab\")",
            "(same? (string-pad-right \"ab\" -9223372036854775808) \"ab\")",
            // string-to-lowercase: only A to Z change, and once is enough.
            "(same? (string-to-lowercase \"ÉCLAIR Ab1 [@]\") \"Éclair ab1 [@]\")",
            "(all? (lambda! (t)
               (let! ((low (string-to-lowercase t)))
                 (and!? (same? (string-to-lowercase low) low)
                        (same? (string-byte-length low) (string-byte-length t)))))
               texts)",
        ],
    );
    fails(
        &mut g,
        &[
            ("(string-split \"a\" \"\")", "range-error"),
            ("(string-search \"é\" \"a\" 1)", "range-error"),
            ("(string-search \"a\" \"a\" 2)", "range-error"),
            ("(string-search \"a\" \"a\" -1)", "range-error"),
            ("(string-search 1 \"a\" 0)", "type-error"),
            ("(string-search \"a\" 1 0)", "type-error"),
            ("(string-join (list 1 2) \",\")", "type-error"),
            ("(string-starts-with? 1 \"a\")", "type-error"),
            ("(string-ends-with? \"a\" 1)", "type-error"),
            ("(string-trim 1)", "type-error"),
            ("(string-pad-left 1 2)", "type-error"),
            ("(string-pad-right \"a\" \"b\")", "type-error"),
            ("(string-to-lowercase 1)", "type-error"),
        ],
    );
}
