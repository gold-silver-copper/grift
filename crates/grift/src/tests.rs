#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::evaluate::Primitive;
use crate::heap::{Bindings, Object};
use crate::{Error, Grift, Interrupter, Operative, OperativeKind, Value, local};

/// The grift definitions of the native primitives.
const MEANINGS: &str = include_str!("natives.grift");

/// The primitives grift can't express, so with no definition in `natives.grift`.
const CORE: &[&str] = &[
    "vau!",
    "wrap",
    "unwrap",
    "evaluate",
    "evaluate-with-cleanup",
    "evaluate-with-handler",
    "raise",
    "context",
    "bind",
    "unbind",
    "names",
    "pair",
    "same?",
    "type-of",
    "add",
    "subtract",
    "multiply",
    "divide",
    "remainder",
    "less-than?",
    "power",
    "square-root",
    "sine",
    "cosine",
    "tangent",
    "exponential",
    "natural-logarithm",
    "floor",
    "ceiling",
    "round",
    "truncate",
    "absolute-value",
    "byte-at",
    "join",
    "string-to-symbol",
    "string-to-float",
    "read-form",
    "value-to-string",
];

impl<T> Grift<T> {
    /// From now on the natives are their grift definitions: each bound in the built-in context,
    /// where the library looks it up, wrapped if the native is, and no library form runs a native
    /// twin. As the library's own code, what this loads is skipped by traces.
    pub(crate) fn run_definitions(&mut self) {
        let here = Object::Context(Some(self.built_in), Bindings::default());
        let here = self.heap.alloc(here).unwrap();
        let forms = self.read_local(MEANINGS, None, true).unwrap();
        self.evaluate_forms(None, forms, here, false).unwrap();
        let name = self.intern("meanings").unwrap();
        let Some(local::Value::Context(meanings)) = self.find(here, name).unwrap() else {
            panic!("natives.grift defines meanings");
        };
        for name in self.names_of(meanings).unwrap() {
            let meaning = self.find(meanings, name).unwrap().unwrap();
            let native = self.find(self.built_in, name).unwrap();
            let bound = match native {
                Some(local::Value::Applicative(_)) => local::Value::Applicative(
                    self.heap.alloc(Object::Applicative(meaning)).unwrap(),
                ),
                _ => meaning,
            };
            self.bind_symbol(self.built_in, name, bound).unwrap();
        }
        for name in ["if!", "sequence!"] {
            let name = self.intern(name).unwrap();
            let Some(local::Value::Operative(form)) = self.find(self.built_in, name).unwrap()
            else {
                panic!("the library defines if! and sequence! as operatives");
            };
            if let Object::Operative(op) = self.heap.get_mut(form).unwrap() {
                op.native = None;
            }
        }
    }

    /// The combiner under `v` if it is an applicative, else `v`.
    fn underlying(&self, v: &Value) -> Value {
        match self.import(v).unwrap() {
            local::Value::Applicative(r) => self.export(self.unwrapped(r).unwrap()),
            _ => v.clone(),
        }
    }

    /// Whether `v` is the definition this interpreter runs for primitive `p`.
    fn is_definition_of(&self, v: &Value, p: Primitive) -> bool {
        let Some(found) = self
            .symbols
            .get(p.name())
            .copied()
            .map(local::Symbol)
            .and_then(|name| self.find(self.built_in, name).unwrap())
        else {
            return false;
        };
        let definition = match found {
            local::Value::Applicative(r) => self.unwrapped(r).unwrap(),
            other => other,
        };
        definition == self.import(v).unwrap()
    }
}

fn grift() -> Grift {
    Grift::new().expect("the library loads")
}

pub(crate) fn show<T>(g: &Grift<T>, v: &Value) -> String {
    let mut s = String::new();
    g.write(v, &mut s).unwrap();
    s
}

pub(crate) fn kind<T>(g: &Grift<T>, e: &Error) -> String {
    g.symbol_name(&e.kind).unwrap().to_string()
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
            Ok(v) => panic!("{src} gave {}", show(g, &v)),
            Err(e) => panic!("{src} failed: {}", kind(g, &e)),
        }
    }
}

/// Each source fails with the given kind of error.
fn fails(g: &mut Grift, cases: &[(&str, &str)]) {
    for (src, want) in cases {
        match g.evaluate(&mut (), src) {
            Ok(v) => panic!("{src} gave {}", show(g, &v)),
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
            "(sequence! (define! e (context ())) (equal? (names e) ()))",
            "(sequence! (define! e (context ())) (bind e (quote! (a (b))) (list 1 (list 2))) \
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
    assert_eq!(g.as_str(&e.values[1]), Ok(Some("two")));
    let e = g.evaluate(&mut (), "(foo bar)").unwrap_err();
    assert_eq!(show(&g, &e.values[0]), "foo");
}

/// An error's values are the values involved and nothing else: no primitive, combiner or name
/// says who failed, primitive, library or parameter tree alike; the trace says where.
#[test]
fn an_error_carries_its_values_and_no_one_who_failed() {
    let mut g = grift();
    for (src, values) in [
        ("(first 1)", "(1)"),
        ("(add 1 1.0)", "(1.0)"),
        ("(add 1)", "((1))"),
        ("(if! 1 2)", "((1 2))"),
        ("((lambda! (x) x))", "(())"),
        ("(byte-at \"a\" 5)", "(\"a\" 5)"),
        ("(divide 1 0)", "(1 0)"),
        ("(string-to-integer \"x\")", "(\"x\")"),
        ("(symbol-to-string 1)", "(1)"),
        ("(power 2 -1)", "(2 -1)"),
        ("(catch-kind! 1 2 e 3)", "(2)"),
        ("(string-split \"a\" \"\")", "(\"\")"),
        ("(minimum 1 1.0)", "(1)"),
    ] {
        let e = g.evaluate(&mut (), src).unwrap_err();
        let mut shown = Vec::new();
        for v in &e.values {
            shown.push(show(&g, v));
        }
        assert_eq!(format!("({})", shown.join(" ")), values, "{src}");
    }
}

/// Strings and bytes share one vocabulary, as integers and floats share add: each word takes
/// either kind and answers that kind, and refuses the two mixed.
#[test]
fn strings_and_bytes_share_one_vocabulary() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            "(equal? (list (byte-length \"\\xe9;\") (byte-length #bytes(1 2 3))) (list 2 3))",
            "(equal? (list (byte-at \"a\" 0) (byte-at #bytes(5) 0)) (list 97 5))",
            "(equal? (list (slice \"abc\" 1 2) (slice #bytes(1 2 3) 1 3)) (list \"b\" #bytes(2 3)))",
            "(equal? (list (join \"a\" \"b\") (join #bytes(1) #bytes(2)) (join)) (list \"ab\" #bytes(1 2) \"\"))",
            "(equal? (list (search \"hello\" \"ll\" 0) (search #bytes(1 2 3) #bytes(2 3) 0)) (list 2 1))",
            // A partial match that overlaps the real one: the search goes on from the next byte.
            "(equal? (list (search \"aab\" \"ab\" 0) (search #bytes(1 1 2) #bytes(1 2) 0)) (list 1 1))",
            "(not? (or!? (binds? (standard-context) (quote! bytes-search))
                         (binds? (standard-context) (quote! string-append))
                         (binds? (standard-context) (quote! bytes-slice))))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(join \"a\" #bytes(1))", "type-error"),
            ("(search \"abc\" #bytes(97) 0)", "type-error"),
            ("(slice \"\\xe9;\" 0 1)", "range-error"),
            ("(byte-length 1)", "type-error"),
        ],
    );
}

/// Whether a combination is the program's comes from who made it: the program's reader and the
/// program's own code make the program's, the library's text and code make the library's, whatever
/// the head of the combination is and whenever it was made.
#[test]
fn a_combination_is_the_program_s_when_the_program_made_it() {
    let mut g = grift();
    let first_place = |g: &mut Grift, src: &str| {
        let e = g.evaluate(&mut (), src).unwrap_err();
        show(g, &e.trace[0])
    };
    // Built by the program around a combiner value: the program's.
    assert_eq!(
        first_place(&mut g, "(evaluate (list first 1) (current-context))"),
        "(#[applicative #[primitive first]] 1)"
    );
    // Built by the library around a combiner value, as map's apply builds it: the library's.
    assert_eq!(
        first_place(&mut g, "(map first (list 1 2))"),
        "(map first (list 1 2))"
    );
    // Built by the program after an argument that ran library code: still the program's.
    assert_eq!(
        first_place(
            &mut g,
            "(evaluate (list first (element-at (list 1) 0)) (current-context))"
        ),
        "(#[applicative #[primitive first]] 1)"
    );
    // Built by a host function after it ran library code, in a nested run or as library text:
    // the host's, so the program's.
    fn first_one(g: &mut Grift) -> Result<Value, Error> {
        let top = g.top_level();
        let name = g.symbol("first")?;
        let first = g.lookup(&top, &name)?.unwrap();
        let one = g.pair(Value::Integer(1), Value::EmptyList)?;
        g.pair(first, one)
    }
    fn after_a_run(g: &mut Grift, s: &mut (), _: &[Value]) -> Result<Value, Error> {
        g.evaluate(s, "(element-at (list 1) 0)")?;
        first_one(g)
    }
    fn after_library_text(g: &mut Grift, s: &mut (), _: &[Value]) -> Result<Value, Error> {
        let top = g.top_level();
        g.evaluate_library_in(s, &top, "prelude.grift", "(element-at (list 1) 0)")?;
        first_one(g)
    }
    define(&mut g, "after-a-run", after_a_run);
    define(&mut g, "after-library-text", after_library_text);
    for made in ["after-a-run", "after-library-text"] {
        assert_eq!(
            first_place(&mut g, &format!("(evaluate ({made}) (current-context))")),
            "(#[applicative #[primitive first]] 1)",
            "{made}"
        );
    }
    // Code the program made stays the program's after a host's own library code has run.
    g.evaluate(&mut (), "(define! kept (list (quote! first) 1))")
        .unwrap();
    let top = g.top_level();
    g.evaluate_library_in(&mut (), &top, "prelude.grift", "(define! helper 1)")
        .unwrap();
    assert_eq!(
        first_place(&mut g, "(evaluate kept (current-context))"),
        "(first 1)"
    );
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
    g.evaluate(&mut (), "(define! here (current-context)) (define! n 0)")
        .unwrap();
    let forms = g
        .read_in(
            "tail.grift",
            "(if! (same? n 0) #inert
        (sequence! (bind here (quote! n) (subtract n 1)) (evaluate again here)))",
        )
        .unwrap();
    let (form, _) = g.as_pair(&forms).unwrap().unwrap();
    g.define("again", form).unwrap();
    let mut high = Vec::new();
    for n in [100, 3000] {
        g.define("n", Value::Integer(n)).unwrap();
        assert_eq!(
            g.evaluate(&mut (), "(evaluate again here)"),
            Ok(Value::Inert)
        );
        high.push(g.machine.frames_high());
    }
    assert_eq!(high[0], high[1], "evaluating named forms kept tail frames");
}

#[test]
fn deep_nesting_needs_no_rust_stack() {
    let mut g = grift();
    let depth = 100_000;
    let nested = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
    let v = g.evaluate(&mut (), &format!("(quote! {nested})")).unwrap();
    assert_eq!(show(&g, &v), nested);
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
    // A large discarded byte payload requests collection without thousands of new objects;
    // a kept payload remains a leaf root through that collection.
    let garbage = g.bytes(&vec![255; 1_000_000]).unwrap();
    let live = g.bytes(&[0, 255]).unwrap();
    let kept = g.keep(live).unwrap();
    assert!(g.heap.due());
    g.evaluate(&mut (), "#inert").unwrap();
    assert_eq!(kind(&g, &g.as_bytes(&garbage).unwrap_err()), "stale-value");
    assert_eq!(
        g.as_bytes(&g.kept(&kept).unwrap()).unwrap(),
        Some(&[0, 255][..])
    );
    g.release(kept).unwrap();
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
        let printed = show(&g, &v);
        let again = g.evaluate(&mut (), &format!("(quote! {printed})")).unwrap();
        let again = show(&g, &again);
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
        show(&g, &f),
        "#[applicative #[operative (x) #ignore (multiply x x)]]"
    );
    // Control characters print as escapes that read back.
    let v = g
        .evaluate(&mut (), "\"\\x1b;[2J\\ta\\x7f;\\x3bb;\"")
        .unwrap();
    let printed = show(&g, &v);
    assert_eq!(printed, "\"\\x1b;[2J\\ta\\x7f;\u{3bb}\"");
    let again = g.evaluate(&mut (), &printed).unwrap();
    assert_eq!(g.as_str(&again), g.as_str(&v));
    // Bytes print as their literal, every byte as the integer byte-at gives, and read back as
    // the same bytes, by the program reader and by read-form.
    let every: Vec<u8> = (0..=255).collect();
    let v = g.bytes(&every).unwrap();
    let printed = show(&g, &v);
    let integers: Vec<String> = every.iter().map(u8::to_string).collect();
    assert_eq!(printed, format!("#bytes({})", integers.join(" ")));
    let again = g.evaluate(&mut (), &printed).unwrap();
    assert_eq!(g.as_bytes(&again).unwrap(), Some(&every[..]));
    g.define("every", v).unwrap();
    holds(
        &mut g,
        &[
            "(same? (first (read-form (value-to-string every) 0)) every)",
            "(equal? (bytes-to-list every) (range 0 256))",
            "(same? (first (read-form \"#bytes( 1 ; one\n 2)\" 0)) (list-to-bytes (list 1 2)))",
            "(same? (value-to-string (list-to-bytes ())) \"#bytes()\")",
        ],
    );
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
    let text = "(define! ran #true)\n\n(add 1\n  (multiply 2 3))\n42\nran\n()";
    let source = "sources/λ.grift";
    let named = g.read_in(source, text).unwrap();
    let unnamed = g.read(text).unwrap();
    assert_eq!(show(&g, &named), show(&g, &unnamed));
    let forms = g.list_items(&named).unwrap();
    assert_eq!(forms.len(), 5);
    let inner = g.list_items(&forms[1]).unwrap();
    // Another named read must not replace these positions. Reading never collects.
    let other = g.read_in("other.grift", "\n\n(list 9)").unwrap();
    let other = g.list_items(&other).unwrap();
    for (form, line) in [(&forms[0], 1), (&forms[1], 3), (&inner[2], 4)] {
        assert_eq!(g.position(form), Ok(Some((source, line))));
    }
    assert_eq!(g.position(&other[0]), Ok(Some(("other.grift", 3))));
    for form in &forms[2..] {
        assert_eq!(g.position(form), Ok(None));
    }
    let unnamed = g.list_items(&unnamed).unwrap();
    assert_eq!(g.position(&unnamed[0]), Ok(None));
    assert_eq!(
        g.read_in("empty.grift", "#! ignored\n ; é"),
        Ok(Value::EmptyList)
    );
    assert!(g.read_in(source, "(define! ran #true)\n(").is_err());
    let ran = g.symbol("ran").unwrap();
    assert_eq!(g.lookup(&g.top_level(), &ran), Ok(None));
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
            e.trace.first().map(|v| show(&g, v)).as_deref(),
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
    assert_eq!(show(&g, &e.trace[0]), "(fail-after-churn)");
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
    assert_eq!(show(&g, &top), "(f g)");
    let built_in = g.evaluate(&mut (), "built-in-names").unwrap();
    let built_in = show(&g, &built_in);
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
    Ok(args[0].clone())
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
    assert_eq!(show(&g, &f), "#[applicative #[host twice]]");
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
    let s = g.as_str(&args[0])?.unwrap().to_owned();
    let (a, b) = (g.string(&s)?, g.string(&s)?);
    g.pair(a, b)
}

fn sum(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    let (mut list, mut n) = (args[0].clone(), 0);
    while let Some((Value::Integer(x), rest)) = g.as_pair(&list)? {
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
    assert_eq!(g.as_pair(&Value::Integer(1)), Ok(None));
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
    let v = g.call(&mut (), &f, &[x, ys]).unwrap();
    assert_eq!(show(&g, &v), "(undefined first 1)");
    let first = g.evaluate(&mut (), "first").unwrap();
    let e = g.call(&mut (), &first, &[Value::Integer(1)]).unwrap_err();
    assert_eq!(kind(&g, &e), "type-error");
}

#[test]
fn call_takes_what_apply_takes() {
    let mut g = grift();
    for f in ["quote!", "(unwrap list)", "1"] {
        let by_apply = g
            .evaluate(&mut (), &format!("(apply {f} (list 1))"))
            .unwrap_err();
        let f = g.evaluate(&mut (), f).unwrap();
        let by_call = g.call(&mut (), &f, &[Value::Integer(1)]).unwrap_err();
        assert_eq!(kind(&g, &by_call), kind(&g, &by_apply));
        assert_eq!(by_call.values, by_apply.values);
    }
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
    assert_eq!(kind(&g, &e), "mine");
    assert_eq!(e.values[0], Value::Integer(7));
    assert_eq!(show(&g, &e.values[1]), "(8)");
    assert_eq!(
        show(&g, &e.trace[0]),
        "(raise (error (quote! mine) 7 (list 8)))"
    );
    let log = g.evaluate(&mut (), "log").unwrap();
    assert_eq!(show(&g, &log), "(4 3 2 1)");
    // A cleanup that fails replaces the error; the outer cleanup still runs.
    let e = g
        .evaluate(&mut (), "(finally! (finally! (first 1) (rest 2)) (note 5))")
        .unwrap_err();
    assert_eq!(show(&g, &e.values[0]), "2");
    assert_eq!(g.evaluate(&mut (), "(first log)"), Ok(Value::Integer(5)));
    // A cleanup that succeeds doesn't lend the error its own location.
    let (protect, here) = (
        g.evaluate(&mut (), "evaluate-with-cleanup").unwrap(),
        g.evaluate(&mut (), "(current-context)").unwrap(),
    );
    let missing = Value::Symbol(g.symbol("missing").unwrap());
    let cleanup = g.evaluate(&mut (), "(lambda! () (list 1))").unwrap();
    let e = g
        .call(&mut (), &protect, &[missing, here, cleanup])
        .unwrap_err();
    assert_eq!(
        (kind(&g, &e), &e.trace[..]),
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
    assert_eq!(show(&g, &e.values[0]), "(1 2)");
}

/// The machine steps taken and the operatives made evaluating `src`.
fn cost(g: &mut Grift, src: &str) -> (u64, usize) {
    (g.machine.steps, g.heap.operatives) = (0, 0);
    g.evaluate(&mut (), src).unwrap();
    (g.machine.steps, g.heap.operatives)
}

#[test]
fn a_running_program_reads_if_and_sequence_as_their_library_definitions() {
    let mut g = grift();
    for (name, definition) in [("if!", "select"), ("sequence!", "body")] {
        let v = g.evaluate(&mut (), name).unwrap();
        assert!(
            show(&g, &v).contains(definition),
            "{name}: {}",
            show(&g, &v)
        );
        let operative = format!("(same? (type-of {name}) (quote! operative))");
        let v = g.evaluate(&mut (), &operative).unwrap();
        assert_eq!(v, Value::Boolean(true), "{name}");
        let e = g
            .evaluate(&mut (), &format!("(unwrap {name})"))
            .unwrap_err();
        assert_eq!(kind(&g, &e), "type-error", "{name}");
    }
    // What a wrong shape raises is the operative's own parameter tree failing to match.
    let e = g.evaluate(&mut (), "(if! #true 1)").unwrap_err();
    assert_eq!(kind(&g, &e), "arity-error");
    assert_eq!(show(&g, &e.values[0]), "(#true 1)");
}

#[test]
fn library_forms_build_no_code() {
    // Each form against the same thing written by hand: a use makes no operative and takes at
    // most six times the steps. They used to build new code each time: let! made two
    // operatives, and conditional! took 29 times the steps. What is written with if! compares with
    // if! as grift, its definition, and what is written with lambda!, which runs no if!, with natives.
    let by_lambda = [
        ("(let! ((x (add n 1))) x)", "((lambda! (x) x) (add n 1))"),
        (
            "(let! ((x (add n 1)) (y x)) y)",
            "((lambda! (x) ((lambda! (y) y) x)) (add n 1))",
        ),
    ];
    let by_if = [
        ("(sequence! 1 2 n)", "(if! #true (if! #true n n) n)"),
        (
            "(conditional! ((same? n 1) 1) ((same? n 2) 2) (#true 3))",
            "(if! (same? n 1) 1 (if! (same? n 2) 2 3))",
        ),
        ("(when! #true n)", "(if! #true n #inert)"),
        (
            "(and!? #true (same? n 3))",
            "(if! #true (same? n 3) #false)",
        ),
        (
            "(or!? #false (same? n 3))",
            "(if! #false #true (same? n 3))",
        ),
    ];
    for (definitions, cases) in [(false, by_lambda.as_slice()), (true, by_if.as_slice())] {
        let mut g = grift();
        if definitions {
            g.run_definitions();
        }
        g.evaluate(&mut (), "(define! n 3) (define! e (current-context))")
            .unwrap();
        for (form, by_hand) in cases {
            let ((steps, made), (by_hand_steps, _)) = (cost(&mut g, form), cost(&mut g, by_hand));
            assert_eq!(made, 0, "{form} made operatives");
            assert!(
                steps <= 6 * by_hand_steps,
                "{form}: {steps} steps against {by_hand_steps}"
            );
        }
    }
}

#[test]
fn an_operative_takes_any_number_of_body_forms_and_prints_them_as_written() {
    let mut g = grift();
    let source = "(define! sequence! 5) (define! n 1)
                  (define! f (vau! (x) #ignore (bind (current-context) (quote! y) x) y))
                  (define! g (lambda! (x) (first x) x))
                  (list (f 2) (same? ((vau! () #ignore)) #inert))";
    let v = g.evaluate(&mut (), source).unwrap();
    assert_eq!(show(&g, &v), "(2 #true)");
    let v = g.evaluate(&mut (), "g").unwrap();
    assert_eq!(
        show(&g, &v),
        "#[applicative #[operative (x) #ignore (first x) x]]"
    );
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
            "(equal? (catch! (evaluate-with-handler (quote! (first 1)) (current-context) (vau! () #ignore 1)) e (evaluate-in! trace e)) \
             (quote! ((evaluate-with-handler (quote! (first 1)) (current-context) (vau! () #ignore 1)))))",
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
            "(same? (catch! (catch! (first 1) #ignore (rest 1)) e (same? (first (evaluate-in! values e)) 1)) #true)",
            // catch! in a loop runs in constant space.
            "(sequence! (define! loop (lambda! (n) (if! (same? n 0) #true (sequence! (catch! (first n) #ignore 0) (loop (subtract n 1)))))) \
             (loop 20000))",
        ],
    );
    fails(
        &mut g,
        &[
            (
                "(evaluate-with-handler 1 (current-context) 1)",
                "type-error",
            ),
            ("(evaluate-with-handler 1 2 first)", "type-error"),
            ("(let! ((x 1 2)) x)", "arity-error"),
            (
                "(catch! (first 1) e (raise (apply error (pair (quote! again) (evaluate-in! values e)))))",
                "again",
            ),
            ("(raise (quote! (kind (1))))", "type-error"),
            ("(raise (context ()))", "type-error"),
            ("(raise (sequence! (define! e (error 1)) e))", "type-error"),
        ],
    );
    // raise passes a caught error on, its trace kept, and adds where it is as any location is.
    let e = g
        .evaluate(&mut (), "(catch! (add 1 (check (quote! a))) e (raise e))")
        .unwrap_err();
    assert_eq!(kind(&g, &e), "not-integer");
    let trace: Vec<String> = e.trace.iter().map(|v| show(&g, v)).collect();
    assert_eq!(
        trace,
        [
            "(raise (error (quote! not-integer) x 2))",
            "(add 1 (check (quote! a)))",
            "(raise e)",
            "(catch! (add 1 (check (quote! a))) e (raise e))"
        ]
    );
    // A new error starts at its raise.
    let trace = g
        .evaluate(
            &mut (),
            "(catch! (catch! (first 1) e (raise e)) e (evaluate-in! trace e))",
        )
        .unwrap();
    assert_eq!(show(&g, &trace), "((first 1) (raise e))");
    let trace = g
        .evaluate(
            &mut (),
            "(catch! (raise (error (quote! mine))) e (evaluate-in! trace e))",
        )
        .unwrap();
    assert_eq!(show(&g, &trace), "((raise (error (quote! mine))))");
    assert!(
        g.heap.len() < 100_000,
        "heap grew to {} slots",
        g.heap.len()
    );
}

fn stop(g: &mut Grift<Interrupter>, _: &mut Interrupter, _: &[Value]) -> Result<Value, Error> {
    g.interrupter().stop();
    Ok(Value::Inert)
}

/// A host function that fails as the program's mistake, or says that memory ran out.
fn fail<T>(g: &mut Grift<T>, _: &mut T, args: &[Value]) -> Result<Value, Error> {
    match args {
        [] => Err(g.error("out-of-memory", &[])),
        _ => Err(g.error("bad-input", args)),
    }
}

#[test]
fn catch_catches_errors_about_the_program_and_never_the_limits() {
    let mut g: Grift<Interrupter> = Grift::new().unwrap();
    let mut s = g.interrupter();
    define(&mut g, "stop", stop);
    define(&mut g, "fail", fail);
    // An error about the program, a host function's included, is caught.
    let caught = g
        .evaluate(&mut s, "(catch! (fail 7) e (evaluate-in! kind e))")
        .unwrap();
    assert_eq!(show(&g, &caught), "bad-input");
    // So is one a host function makes, whatever its kind is called: only the machine can run out.
    let caught = g
        .evaluate(&mut s, "(catch! (fail) e (evaluate-in! kind e))")
        .unwrap();
    assert_eq!(show(&g, &caught), "out-of-memory");
    // A cleanup that fails while a limit passes doesn't turn it into an error catch! catches.
    let e = g
        .evaluate(&mut s, "(catch! (finally! (stop) (first 1)) #ignore 0)")
        .unwrap_err();
    assert_eq!(kind(&g, &e), "interrupted");
    // ... even through a catch! in a loop that would catch anything.
    let e = g
        .evaluate(
            &mut s,
            "(define! spin (lambda! () (catch! (sequence! (stop) (spin)) #ignore (spin)))) (spin)",
        )
        .unwrap_err();
    assert_eq!(kind(&g, &e), "interrupted");
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
                    (not? (binds? (context ()) (quote! first))))",
        ],
    );
    // Both operands are evaluated: a computed symbol is the one looked up, not the name it is
    // held in.
    holds(
        &mut g,
        &[
            "(binds? e (string-to-symbol \"x\"))",
            "(not? (binds? e (string-to-symbol \"nope\")))",
            "(same? (first (catch! (binds? e 1) x (evaluate-in! values x))) 1)",
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
        let at: Vec<String> = e.trace.iter().map(|v| show(&g, v)).collect();
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
            "(equal? (byte-length \"λx\") 3)",
            "(equal? (list (byte-at \"λx\" 0) (byte-at \"λx\" 2)) (quote! (206 120)))",
            "(same? (slice \"λx\" 2 3) \"x\")",
            "(same? (slice \"abc\" 1 1) \"\")",
            "(same? (join \"a\" (code-point-to-string 955) \"\") \"aλ\")",
            "(same? (join) \"\")",
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
            ("(slice \"λ\" 0 1)", "range-error"),
            ("(slice \"abc\" 2 1)", "range-error"),
            ("(slice \"abc\" 0 4)", "range-error"),
            ("(byte-at \"abc\" 3)", "range-error"),
            ("(byte-at \"abc\" -1)", "range-error"),
            ("(code-point-to-string 55296)", "range-error"),
            ("(code-point-to-string -1)", "range-error"),
            ("(byte-length (quote! a))", "type-error"),
            ("(join \"a\" 1)", "type-error"),
            ("(symbol-to-string \"a\")", "type-error"),
            ("(float-to-string 1)", "type-error"),
            ("(integer-to-string \"1\")", "type-error"),
        ],
    );
}

#[test]
fn bytes_by_offsets_and_their_conversions() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            // The string operations' shapes, offsets counting bytes, a slice's end exclusive.
            "(same? (byte-length #bytes(0 255 9)) 3)",
            "(same? (byte-at #bytes(0 255 9) 1) 255)",
            "(same? (slice #bytes(0 255 9) 1 3) #bytes(255 9))",
            "(same? (join #bytes(1) #bytes() #bytes(2 3)) #bytes(1 2 3))",
            "(same? (join) \"\")",
            "(equal? (map (lambda! (start) (search #bytes(1 2 1 2) #bytes(1 2) start)) (range 0 5)) (list 0 2 2 () ()))",
            "(same? (search #bytes(1 2) #bytes() 2) 2)",
            // Text to bytes and back, whatever the text holds, NUL and a byte-order mark too.
            "(same? (string-to-bytes \"\\xe9;\") #bytes(195 169))",
            "(sequence! (define! s (join (code-point-to-string 0) \"\\xfeff;h\\xe9;\\x1f600;\")) (same? (bytes-to-string (string-to-bytes s)) s))",
            "(equal? (bytes-to-list (list-to-bytes (list 0 128 255))) (list 0 128 255))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(byte-at #bytes(1) 1)", "range-error"),
            ("(slice #bytes(1 2) 2 1)", "range-error"),
            ("(search #bytes(1) #bytes(1) 2)", "range-error"),
            // A string is never bytes, nor bytes a string.
            ("(bytes-to-string \"abc\")", "type-error"),
            ("(join #bytes(97) \"b\")", "type-error"),
            ("(join \"a\" #bytes(98))", "type-error"),
        ],
    );
    // What isn't a list of bytes is refused with the value and its place in the list.
    for (list, want) in [
        ("(list 1 256)", "(range-error (256 1))"),
        ("(list -1)", "(range-error (-1 0))"),
        ("(list 1 \"a\")", "(type-error (\"a\" 1))"),
        ("(pair 1 2)", "(type-error (2 1))"),
        ("5", "(type-error (5 0))"),
    ] {
        let src = format!(
            "(catch! (list-to-bytes {list}) e (list (evaluate-in! kind e) (evaluate-in! values e)))"
        );
        let v = g.evaluate(&mut (), &src).unwrap();
        assert_eq!(show(&g, &v), want, "{list}");
    }
    // Bytes that aren't UTF-8 aren't a string: encoding-error, with the bytes
    // themselves and Rust's valid_up_to, the offset of the first sequence that is cut short,
    // overlong, a surrogate, past U+10FFFF or no sequence at all.
    for bytes in [
        &b"a\xff"[..],
        b"hi\xc0\x80",
        b"\xe2\x82",
        b"\xed\xa0\x80",
        b"\xf4\x90\x80\x80",
        b"A\xf0\x9f\x98",
        b"\xc3\xa9\xe2(\xa1",
        b"\0\xef\xbb\xbf\xc3\xa9\x80",
    ] {
        let at = std::str::from_utf8(bytes).unwrap_err().valid_up_to();
        let b = g.bytes(bytes).unwrap();
        g.define("b", b).unwrap();
        let v = g
            .evaluate(
                &mut (),
                "(catch! (bytes-to-string b) e
                   (let! (((original at) (evaluate-in! values e)))
                     (list (evaluate-in! kind e) (same? original b) at)))",
            )
            .unwrap();
        assert_eq!(
            show(&g, &v),
            format!("(encoding-error #true {at})"),
            "{bytes:?}"
        );
    }
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
fn a_token_with_two_signs_is_a_symbol() {
    let mut g = grift();
    for token in ["--5", "+-5", "-+1.5", "++1"] {
        let forms = g.read(token).unwrap();
        assert_eq!(show(&g, &forms), format!("({token})"));
    }
}

#[test]
fn floats_read_back_as_they_print() {
    let mut g = grift();
    for x in FLOATS {
        let printed = show(&g, &Value::Float(x));
        let again = float(&mut g, &printed);
        assert_eq!(again.to_bits(), x.to_bits(), "{printed}");
        // value-to-string gives the printer's text, and string-to-float reads it back (a
        // not-a-number only up to its sign, which these floats don't vary).
        let s = g
            .evaluate(&mut (), &format!("(value-to-string {printed})"))
            .unwrap();
        assert_eq!(g.as_str(&s), Ok(Some(printed.as_str())));
        let again = float(
            &mut g,
            &format!("(string-to-float (value-to-string {printed}))"),
        );
        assert_eq!(again.to_bits(), x.to_bits(), "{printed}");
    }
    // A not-a-number of any sign reads back as a not-a-number.
    let negative = show(&g, &Value::Float(-f64::NAN));
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
                        assert_eq!(kind(&g, &e), want, "({name} {a} {b})");
                    }
                    (rust, got) => panic!("({name} {a} {b}): Rust {rust:?}, grift {got:?}"),
                }
            }
            let less = g.evaluate(&mut (), &format!("(less-than? {a} {b})"));
            assert_eq!(less, Ok(Value::Boolean(a < b)));
        }
        // absolute-value and power take integers too: Rust's checked abs and pow, overflow
        // where they give none, and a negative exponent is a range-error.
        let got = g.evaluate(&mut (), &format!("(absolute-value {a})"));
        match (a.checked_abs(), got) {
            (Some(n), Ok(v)) => assert_eq!(v, Value::Integer(n), "(absolute-value {a})"),
            (None, Err(e)) => assert_eq!(kind(&g, &e), "overflow"),
            (rust, got) => panic!("(absolute-value {a}): Rust {rust:?}, grift {got:?}"),
        }
        for b in [0, 1, 2, 3, 31, 62, 63, 64, -1] {
            let got = g.evaluate(&mut (), &format!("(power {a} {b})"));
            match (u32::try_from(b).map(|b| a.checked_pow(b)), got) {
                (Ok(Some(n)), Ok(v)) => assert_eq!(v, Value::Integer(n), "(power {a} {b})"),
                (Ok(None), Err(e)) => assert_eq!(kind(&g, &e), "overflow", "(power {a} {b})"),
                (Err(_), Err(e)) => assert_eq!(kind(&g, &e), "range-error"),
                (rust, got) => panic!("(power {a} {b}): Rust {rust:?}, grift {got:?}"),
            }
        }
    }
    let floats = FLOATS.map(|x| show(&g, &Value::Float(x)));
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
            ("(less-than? \"a\" #bytes(97))", "type-error"),
            ("(less-than? #bytes(97) \"a\")", "type-error"),
            ("(less-than? \"a\" 1)", "type-error"),
            ("(less-than? 1 \"a\")", "type-error"),
            ("(less-than? #true #false)", "type-error"),
            ("(multiply 2.0 \"a\")", "type-error"),
            ("(square-root 4)", "type-error"),
            ("(absolute-value \"a\")", "type-error"),
            ("(power 2 3.0)", "type-error"),
            ("(power 2.0 3)", "type-error"),
            ("(integer-to-float 1.0)", "type-error"),
            ("(float-to-integer 1)", "type-error"),
        ],
    );
}

#[test]
fn bitwise_operations_are_rusts() {
    let mut g = grift();
    let integers = [0, 1, -1, 6, -7, 0x5555, i64::MAX, i64::MIN];
    for a in integers {
        for b in integers {
            for (name, rust) in [
                ("bitwise-and", a & b),
                ("bitwise-or", a | b),
                ("bitwise-exclusive-or", a ^ b),
            ] {
                let got = g.evaluate(&mut (), &format!("({name} {a} {b})"));
                assert_eq!(got, Ok(Value::Integer(rust)), "({name} {a} {b})");
            }
        }
        // A shift drops the bits it moves out, as Rust's does, and an amount outside 0 to 63,
        // where Rust's checked shift gives none, is overflow.
        for n in [0, 1, 13, 63, 64, -1, i64::MAX] {
            for (name, rust) in [
                (
                    "shift-left",
                    u32::try_from(n).ok().and_then(|n| a.checked_shl(n)),
                ),
                (
                    "shift-right",
                    u32::try_from(n).ok().and_then(|n| a.checked_shr(n)),
                ),
            ] {
                let got = g.evaluate(&mut (), &format!("({name} {a} {n})"));
                match (rust, got) {
                    (Some(r), Ok(v)) => assert_eq!(v, Value::Integer(r), "({name} {a} {n})"),
                    (None, Err(e)) => assert_eq!(kind(&g, &e), "overflow", "({name} {a} {n})"),
                    (rust, got) => panic!("({name} {a} {n}): Rust {rust:?}, grift {got:?}"),
                }
            }
        }
    }
    fails(
        &mut g,
        &[
            ("(bitwise-and 1 1.0)", "type-error"),
            ("(shift-left 1.0 1)", "type-error"),
            ("(bitwise-or 1)", "arity-error"),
        ],
    );
}

#[test]
fn comparisons_agree_with_rust() {
    let mut g = grift();
    let integers = [-1, 0, 1].map(Value::Integer);
    let floats = [-1.0, 0.0, -0.0, 1.0, f64::NAN, f64::INFINITY].map(Value::Float);
    for values in [&integers[..], &floats[..]] {
        for x in values {
            for y in values {
                let (a, b) = (show(&g, x), show(&g, y));
                // Rust's own order of the two, None with not-a-number.
                let order = match (x, y) {
                    (Value::Integer(x), Value::Integer(y)) => x.partial_cmp(y),
                    (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
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
            "(same? \"ab\" (join \"a\" \"b\"))",
            "(not? (same? (list 1) (list 1)))",
            "(not? (same? (context ()) (context ())))",
            "(not? (same? (lambda! () 1) (lambda! () 1)))",
            "(sequence! (define! p (list 1)) (same? p p))",
            "(same? first first)",
            // Bytes, like strings, by their contents.
            "(same? #bytes(97 98) (join #bytes(97) (string-to-bytes \"b\")))",
            "(same? #bytes() (slice #bytes(1) 1 1))",
            "(not? (same? #bytes(1) #bytes(1 0)))",
            "(equal? (list #bytes(1)) (list (list-to-bytes (list 1))))",
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
            "(not? (equal? (context ()) (context ())))",
            // Values of different types are never equal.
            "(not? (equal? 1 1.0))",
            "(not? (same? 1 1.0))",
            "(not? (equal? \"a\" (quote! a)))",
            "(not? (same? \"a\" #bytes(97)))",
            "(not? (equal? (list 97) #bytes(97)))",
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
        "bytes?",
        "context?",
        "operative?",
        "applicative?",
        "inert?",
        "ignore?",
    ];
    let values = [
        "()",
        "#false",
        "1",
        "1.0",
        "(quote! a)",
        "(list 1)",
        "\"s\"",
        "#bytes(115)",
        "(context ())",
        "if!",
        "first",
        "#true",
        "(pair 1 2)",
        "#bytes()",
        "vau!",
        "#inert",
        "#ignore",
    ];
    // Each value's type-of names exactly one predicate, and only that one holds.
    for value in values {
        let kind = g
            .evaluate(&mut (), &format!("(symbol-to-string (type-of {value}))"))
            .unwrap();
        let kind = format!("{}?", g.as_str(&kind).unwrap().unwrap());
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
        "(define! parent (context ())) (bind parent (quote! (b a)) (list 1 2))
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
            // bindings gives a context's own bindings as data, a record's fields.
            "(equal? (bindings child) (quote! ((a 10) (c 3))))",
            "(equal? (second (find-association (quote! values) (bindings (error (quote! e) 1)))) (list 1))",
            "(same? (bindings (context child)) ())",
            // current-context is the context it is called in.
            "(same? ((lambda! (x) (evaluate-in! x (current-context))) 5) 5)",
        ],
    );
    // A context prints as its own names, sorted, never its values.
    let child = g.evaluate(&mut (), "child").unwrap();
    assert_eq!(show(&g, &child), "#[context a c]");
    let empty = g.evaluate(&mut (), "(context ())").unwrap();
    assert_eq!(show(&g, &empty), "#[context]");
    let e = g.evaluate(&mut (), "(error (quote! mine) 1)").unwrap();
    assert_eq!(show(&g, &e), "#[context kind trace values]");
}

/// What evaluating `src` does: machine steps, collections, and references and symbols compared.
/// Counts, so they say the same on a loaded machine.
fn work(g: &mut Grift, src: &str) -> (Result<Value, Error>, u64, usize, u64) {
    (g.machine.steps, g.heap.collections) = (0, 0);
    local::COMPARED.with(|n| n.set(0));
    let value = g.evaluate(&mut (), src);
    let compared = local::COMPARED.with(std::cell::Cell::get);
    (value, g.machine.steps, g.heap.collections, compared)
}

#[test]
fn unwinding_and_checking_parameters_search_nothing() {
    // 300,000 frames, each around a combination made for it, so the trace holds each of them.
    // Through a cleanup per frame, too.
    for around in ["((lambda! (x) x) {})", "(finally! {} #inert)"] {
        let mut g = grift();
        let inner = around.replace("{}", "(deep (evaluate n caller))");
        let deep = format!(
            "(define! here (current-context))
             (define! deep
               (lambda! (n)
                 (if! (same? n 0)
                   (raise (error (quote! bottom)))
                   (evaluate (list (quote! add) 1 (list (quote! wrapped) (subtract n 1))) here))))
             (define! wrapped (vau! (n) caller {inner}))
             (catch! (deep 300000) e (length (evaluate-in! trace e)))"
        );
        // 160 steps a frame, 15 to 19 collections, a third of a comparison a step. A search per
        // frame is 45 billion comparisons, a collection per cleanup 300,000 collections.
        let (traced, steps, collections, compared) = work(&mut g, &deep);
        assert!(steps < 150_000_000, "{steps} steps");
        assert!(collections < 100, "{collections} collections");
        assert!(compared < steps, "{compared} compared in {steps} steps");
        assert!(matches!(traced, Ok(Value::Integer(n)) if n >= 300_000));
    }
    // 300,000 parameters: a search compares 45 billion times, a table a few.
    let mut g = grift();
    let names: Vec<String> = (0..300_000).map(|i| format!("p{i:06}")).collect();
    let good = format!("((wrap vau!) (quote! ({})) #ignore 1)", names.join(" "));
    let (made, _, _, compared) = work(&mut g, &good);
    assert!(made.is_ok());
    assert!(compared < 300_000, "{compared} compared");
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
                      (equal? (length trace) 5)))",
        ],
    );
}

/// `s` as a grift string literal.
fn literal(s: &str) -> String {
    let mut g = grift();
    let v = g.string(s).unwrap();
    show(&g, &v)
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
                   (if! (equal? (first (read-form (slice text start end) 0)) form)
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
        assert_eq!(kind(&g, &whole), "parse-error");
        let one = g
            .evaluate(&mut (), &format!("(read-form {} 0)", literal(bad)))
            .unwrap_err();
        assert_eq!(
            (kind(&g, &one), one.values),
            ("parse-error".into(), whole.values)
        );
    }
    let e = g
        .evaluate(&mut (), "(read-form \"a\\nb\\n)\" 4)")
        .unwrap_err();
    assert_eq!(show(&g, &e.values[0]), "unmatched-parenthesis");
    assert_eq!(e.values[1..], [Value::Integer(3), Value::Integer(1)]);
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
        "(sequence! (define! c (context ())) (bind c (quote! b) 1) (bind c (quote! a) 2) c)",
    ] {
        let v = g.evaluate(&mut (), src).unwrap();
        let printed = show(&g, &v);
        let text = g
            .evaluate(&mut (), &format!("(value-to-string {src})"))
            .unwrap();
        assert_eq!(g.as_str(&text), Ok(Some(printed.as_str())));
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
               (all? (lambda! (part) (empty-list? (search part p 0))) (string-split t p))))",
            "(equal? (string-split \"a,,b\" \",\") (list \"a\" \"\" \"b\"))",
            "(equal? (string-split \"\" \",\") (list \"\"))",
            "(same? (string-join () \",\") \"\")",
            "(same? (string-join (list \"a\") \",\") \"a\")",
            // search: the pattern is at the offset it gives, and it is at none before it.
            "(every-pair (lambda! (t p)
               (let! ((at (search t p 0)) (size (byte-length p)))
                 (if! (empty-list? at)
                   (same? (length (string-split t p)) 1)
                   (and!? (same? (slice t at (add at size)) p)
                          (empty-list? (search (slice t 0 at) p 0)))))))",
            "(same? (search \"abcabc\" \"c\" 3) 5)",
            "(same? (search \"abc\" \"\" 3) 3)",
            "(empty-list? (search \"abc\" \"abcd\" 0))",
            // Every text starts with what it starts with and ends with what it ends with.
            "(every-pair (lambda! (t p)
               (and!? (string-starts-with? (join p t) p)
                      (string-ends-with? (join t p) p))))",
            "(not? (string-starts-with? \"é\" \"a\"))",
            "(not? (string-starts-with? \"a\" \"ab\"))",
            "(not? (string-ends-with? \"é\" \"a\"))",
            "(not? (string-ends-with? \"b\" \"ab\"))",
            // string-trim: nothing blank is left at the ends, nothing else goes, and once is enough.
            "(same? (string-trim \" \\t\\n a b\\xd;\\n \") \"a b\")",
            "(same? (string-trim \"  \\n\") \"\")",
            "(all? (lambda! (t) (same? (string-trim (string-trim t)) (string-trim t))) texts)",
            "(all? (lambda! (t) (same? (string-trim (join \" \" t \"\\n\")) t)) texts)",
            // The pads: at least the width, the text unchanged, the spaces on the side named.
            "(all? (lambda! (t)
               (and!? (same? (byte-length (string-pad-left t 30 \" \")) 30)
                      (string-ends-with? (string-pad-left t 30 \"0\") t)
                      (string-starts-with? (string-pad-right t 30 \".\") t)
                      (same? (string-pad-right t 0 \" \") t)
                      (same? (string-pad-left t 0 \" \") t)))
               texts)",
            "(same? (string-pad-left \"ab\" 4 \" \") \"  ab\")",
            "(same? (string-pad-right \"ab\" 4 \" \") \"ab  \")",
            "(same? (string-pad-left \"7\" 2 \"0\") \"07\")",
            "(same? (string-pad-left \"ab\" -9223372036854775808 \" \") \"ab\")",
            "(same? (string-pad-right \"ab\" -9223372036854775808 \" \") \"ab\")",
            // string-repeat: as long as count copies, each one the string; a count below 1 is none.
            "(same? (string-repeat \"a\" -1) \"\")",
            "(all? (lambda! (t)
               (all? (lambda! (n)
                       (let! ((r (string-repeat t n)))
                         (and!? (same? (byte-length r) (multiply n (byte-length t)))
                                (same? (string-join (string-split r t) \"\") (string-repeat \"\" n)))))
                     (range 0 4)))
               (pair \"ab\" (rest texts)))",
            // string-to-lowercase: only A to Z change, and once is enough.
            "(same? (string-to-lowercase \"ÉCLAIR Ab1 [@]\") \"Éclair ab1 [@]\")",
            "(all? (lambda! (t)
               (let! ((low (string-to-lowercase t)))
                 (and!? (same? (string-to-lowercase low) low)
                        (same? (byte-length low) (byte-length t)))))
               texts)",
        ],
    );
    fails(
        &mut g,
        &[
            ("(string-split \"a\" \"\")", "range-error"),
            ("(search \"é\" \"a\" 1)", "range-error"),
            ("(search \"a\" \"a\" 2)", "range-error"),
            ("(search \"a\" \"a\" -1)", "range-error"),
            ("(search 1 \"a\" 0)", "type-error"),
            ("(search \"a\" 1 0)", "type-error"),
            ("(string-join (list 1 2) \",\")", "type-error"),
            ("(string-starts-with? 1 \"a\")", "type-error"),
            ("(string-ends-with? \"a\" 1)", "type-error"),
            ("(string-trim 1)", "type-error"),
            ("(string-pad-left 1 2 \" \")", "type-error"),
            ("(string-pad-right \"a\" \"b\" \" \")", "type-error"),
            ("(string-pad-left \"a\" 3 \"ab\")", "range-error"),
            ("(string-pad-left \"a\" 3 \"\")", "range-error"),
            ("(string-pad-left \"a\" 3)", "arity-error"),
            ("(string-repeat 1 2)", "type-error"),
            ("(string-repeat \"a\" 1.0)", "type-error"),
            ("(string-to-lowercase 1)", "type-error"),
        ],
    );
}

#[test]
fn a_parse_error_says_why_and_where() {
    let mut g = grift();
    let mut missing_source = Vec::new();
    // Each reason the reader has, and the line and column: of the `(` or `"` left open, wherever
    // the text ends, or else of what didn't read.
    for (text, want) in [
        (
            "(define! f\n  (lambda! (x)\n    (add x 1)\n(f 2)",
            "unclosed-list 2 3",
        ),
        ("(a (b) (c)", "unclosed-list 1 1"),
        ("(a \"bc\n\nd", "unclosed-string 1 4"),
        ("\"ab\\", "unclosed-string 1 1"),
        ("\"\\x41", "unclosed-string 1 1"),
        ("(a))", "unmatched-parenthesis 1 4"),
        ("(a . )", "misplaced-dot 1 6"),
        ("(. a)", "misplaced-dot 1 2"),
        ("(a . b c)", "misplaced-dot 1 8"),
        ("x \"a\\qb\"", "bad-escape 1 5"),
        ("\"\\x110000;\"", "bad-escape 1 2"),
        ("(1.2.3)", "bad-number 1 2"),
        ("99999999999999999999", "bad-number 1 1"),
        ("#nope", "unknown-constant 1 1"),
        ("(define! unread 1)\n(a", "unclosed-list 2 1"),
        ("α )", "unmatched-parenthesis 1 3"),
        // Inside #bytes( only integers 0 to 255, each at its own place.
        ("#bytes(1\n 256)", "bad-byte 2 2"),
        ("#bytes(1x)", "bad-number 1 8"),
        ("#bytes(#foo)", "unknown-constant 1 8"),
        ("#bytes(\"a", "unclosed-string 1 8"),
        ("#bytes(1 \"a\" x)", "bad-byte 1 10"),
        ("(#bytes((1)))", "bad-byte 1 11"),
        ("#bytes((1", "unclosed-list 1 8"),
        ("#bytes(1.0)", "bad-byte 1 8"),
        ("#bytes(1 . 2)", "misplaced-dot 1 10"),
        ("#bytes (1)", "unknown-constant 1 1"),
    ] {
        let e = g.read(text).unwrap_err();
        let values: Vec<String> = e.values.iter().map(|v| show(&g, v)).collect();
        assert_eq!(
            (kind(&g, &e), values.join(" ")),
            ("parse-error".into(), want.into()),
            "{text:?}"
        );
        // Unnamed entries keep the bare reader error. Both named entries add the source after
        // (reason line column), and a trace holds only combinations.
        assert_eq!(g.evaluate(&mut (), text).unwrap_err(), e);
        assert!(e.trace.is_empty());
        for entry in ["read_in", "evaluate_in"] {
            let named = if entry == "read_in" {
                g.read_in("broken.grift", text)
            } else {
                g.evaluate_in(&mut (), &g.top_level(), "broken.grift", text)
            }
            .unwrap_err();
            assert_eq!(named.kind, e.kind);
            let source = Value::Symbol(g.symbol("broken.grift").unwrap());
            let (values, trace) = (named.values, named.trace);
            if values != [e.values.as_slice(), &[source]].concat() || !trace.is_empty() {
                missing_source.push(format!("{entry}: {want}: {values:?} {trace:?}"));
            }
        }
        // More text after an unclosed form changes nothing.
        if want.starts_with("unclosed-list") {
            let more = g.read(&format!("{text}\n(g 3)\n")).unwrap_err();
            assert_eq!(more.values, e.values);
        }
    }
    assert!(missing_source.is_empty(), "{}", missing_source.join("\n"));
    // An unclosed literal of bytes is placed at its `(`; a list after it is no more of it but a
    // byte that isn't one.
    for (text, want) in [
        ("(#bytes(1 2", "unclosed-list 1 8"),
        ("(#bytes(1 2\n(g 3)", "bad-byte 2 5"),
    ] {
        let e = g.read(text).unwrap_err();
        let values: Vec<String> = e.values.iter().map(|v| show(&g, v)).collect();
        assert_eq!(values.join(" "), want, "{text:?}");
    }
}

#[test]
fn an_object_counts_its_storage_towards_the_next_collection() {
    let mut g = grift();
    let text = "x".repeat(1_000_000);
    g.heap.not_due();
    g.string(&text).unwrap();
    assert!(g.heap.due(), "a string");
    g.heap.not_due();
    g.bytes(text.as_bytes()).unwrap();
    assert!(g.heap.due(), "bytes");
}

#[test]
fn a_context_takes_one_operand_its_parent_or_none() {
    let mut g = grift();
    holds(
        &mut g,
        &[
            "(same? (lookup (context ()) (quote! list) #false) #false)",
            "(same? (lookup (context (current-context)) (quote! list) list) list)",
        ],
    );
    fails(
        &mut g,
        &[("(context 1)", "type-error"), ("(context)", "arity-error")],
    );
}

#[test]
fn a_trace_from_the_host_is_checked_as_values_are() {
    let mut g = grift();
    let mut e = g.error("mine", &[]);
    e.trace.push(g.string("reclaimed").unwrap());
    g.heap.request_collection();
    g.evaluate(&mut (), "#inert").unwrap();
    let Err(why) = g.describe(&e, &mut String::new()) else {
        panic!("described a reclaimed value");
    };
    assert_eq!(kind(&g, &why), "stale-value");
}

#[test]
fn a_bind_that_fails_binds_nothing() {
    let mut g = grift();
    g.evaluate(&mut (), "(define! c (context ()))").unwrap();
    fails(
        &mut g,
        &[
            ("(bind c (quote! (a b)) (list 1))", "arity-error"),
            ("(bind c (quote! (a 1)) (list 1 2))", "parameter-error"),
            ("(bind c (quote! (a 1)) (list 2))", "arity-error"),
            ("(bind c (quote! (a a)) (list 1 2))", "parameter-error"),
        ],
    );
    holds(&mut g, &["(same? (names c) ())"]);
}

#[test]
fn a_comment_starts_with_either_mark_wherever_a_token_can() {
    let mut g = grift();
    for src in ["(1 ; x\n 2)", "(1 #!x\n 2)", "#!x\n(1 2)", " #!x\n(1 2)"] {
        let forms = g.read(src).unwrap();
        assert_eq!(show(&g, &forms), "((1 2))", "{src}");
    }
    // Inside a token or a string, it is no comment.
    let forms = g.read("a#!b \"#!c\"").unwrap();
    assert_eq!(show(&g, &forms), "(a#!b \"#!c\")");
}

#[test]
fn named_parse_errors_stay_describable_after_collection() {
    let mut broken = Vec::new();
    for entry in ["read_in", "evaluate_in"] {
        let mut g = grift();
        let e = if entry == "read_in" {
            g.read_in("config.grift", "(a\n  (b")
        } else {
            g.evaluate_in(&mut (), &g.top_level(), "config.grift", "(a\n  (b")
        }
        .unwrap_err();
        let mut before = String::new();
        g.describe(&e, &mut before).unwrap();
        for _ in 0..2 {
            // A second error with the same name must not invalidate the one the host holds.
            let again = g.read_in("config.grift", "(").unwrap_err();
            let garbage = g.string("collect this").unwrap();
            g.heap.request_collection();
            g.evaluate(&mut (), "#inert").unwrap();
            assert_eq!(kind(&g, &g.as_str(&garbage).unwrap_err()), "stale-value");
            for held in [&e, &again] {
                let mut after = String::new();
                let described = g.describe(held, &mut after);
                let position = g.position(&held.values[3]);
                if described.is_err() || position != Ok(None) {
                    broken.push(format!(
                        "{entry}: describe={described:?}, position={position:?}"
                    ));
                }
                if std::ptr::eq(held, &e) && described.is_ok() {
                    assert_eq!(after, before);
                }
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

#[test]
fn lookup_and_unbind() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! outer (current-context))
         (define! x 1)
         (define! inner (context outer))
         (bind inner (quote! (x y)) (list 2 3))",
    )
    .unwrap();
    holds(
        &mut g,
        &[
            // lookup gives what evaluating the symbol gives, or `otherwise` where that fails.
            "(all? (lambda! (s) (equal? (lookup inner s ()) (evaluate s inner))) (quote! (x y first)))",
            "(same? (lookup inner (quote! nope) 7) 7)",
            "(same? (lookup (context ()) (quote! first) 7) 7)",
            // binds? says whether lookup finds it, whatever is bound, otherwise's value included.
            "(sequence! (bind inner (quote! z) 7) (binds? inner (quote! z)))",
            "(not? (binds? inner (quote! nope)))",
            // unbind removes a binding of the context itself, revealing a parent's.
            "(unbind inner (quote! x))",
            "(same? (evaluate (quote! x) inner) 1)",
            "(not? (unbind inner (quote! x)))",
            "(not? (unbind inner (quote! first)))",
            "(same? (lookup outer (quote! x) ()) 1)",
            "(sequence! (unbind inner (quote! y)) (not? (binds? inner (quote! y))))",
            "(equal? (names inner) (quote! (z)))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(lookup 1 (quote! x) 2)", "type-error"),
            ("(lookup inner 1 2)", "type-error"),
            ("(unbind inner \"x\")", "type-error"),
            ("(unbind inner)", "arity-error"),
        ],
    );
    // An unbound value can be reclaimed: the collector frees it, so the host's copy goes stale.
    let churn = "(length (map (lambda! (n) (list n)) (range 0 20000)))";
    let held = g
        .evaluate(
            &mut (),
            "(sequence! (bind inner (quote! held) (join \"kept\" \"?\"))
                        (lookup inner (quote! held) ()))",
        )
        .unwrap();
    // While bound it survives collections; once unbound, nothing holds it.
    g.evaluate(&mut (), churn).unwrap();
    assert_eq!(g.as_str(&held), Ok(Some("kept?")));
    g.evaluate(&mut (), "(unbind inner (quote! held))").unwrap();
    g.evaluate(&mut (), churn).unwrap();
    assert_eq!(kind(&g, &g.as_str(&held).unwrap_err()), "stale-value");
}

#[test]
fn catch_kind_catches_one_kind_and_lets_the_rest_go_on_as_they_were() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! deep (lambda! (x) (add 1 (first x))))
         (define! trace-of
           (lambda! (body) (catch! (evaluate body (current-context)) e (evaluate-in! trace e))))",
    )
    .unwrap();
    holds(
        &mut g,
        &[
            "(same? (catch-kind! (deep 1) type-error e (evaluate-in! kind e)) (quote! type-error))",
            "(same? (catch-kind! (deep (list 1)) type-error e 0) 2)",
            "(same? (catch-kind! (raise (error (quote! found) 4)) found e (first (evaluate-in! values e))) 4)",
            // Another kind goes on, its trace the body's own combinations and the form it passed.
            "(equal? (length (trace-of (quote! (catch-kind! (deep 1) parse-error e 0))))
                     (add 1 (length (trace-of (quote! (deep 1))))))",
            "(equal? (first (trace-of (quote! (catch-kind! (deep 1) parse-error e 0))))
                     (first (trace-of (quote! (deep 1)))))",
            "(equal? (last (trace-of (quote! (catch-kind! (deep 1) parse-error e 0))))
                     (quote! (catch-kind! (deep 1) parse-error e 0)))",
            "(same? (catch-kind! (catch-kind! (deep 1) parse-error e 1) type-error e 2) 2)",
        ],
    );
    fails(
        &mut g,
        &[
            (
                "(catch-kind! (deep 1) (quote! type-error) e 0)",
                "type-error",
            ),
            ("(catch-kind! (deep 1) parse-error e 0)", "type-error"),
        ],
    );
}

#[test]
fn sort_is_ordered_and_stable() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! seed 12345)
         (define! here (current-context))
         (define! next (lambda! () (bind here (quote! seed) (remainder (add (multiply seed 1103515245) 12345) 2147483648)) seed))
         (define! numbers (map (lambda! (#ignore) (remainder (next) 100)) (range 0 10000)))
         (define! by-first (lambda! (a b) (less-than? (first a) (first b))))
         (define! ordered?
           (lambda! (xs before?)
             (if! (or!? (empty-list? xs) (empty-list? (rest xs)))
               #true
               (and!? (not? (before? (second xs) (first xs))) (ordered? (rest xs) before?)))))",
    )
    .unwrap();
    // 14.5 million steps and 109 collections, about 13 comparisons of elements for each of the
    // 10,000. A quadratic sort is thousands of comparisons each: billions of steps.
    let (sorted, steps, collections, _) =
        work(&mut g, "(define! sorted (sort numbers less-than?))");
    assert!(steps < 40_000_000, "{steps} steps");
    assert!(collections < 400, "{collections} collections");
    sorted.unwrap();
    holds(
        &mut g,
        &[
            "(ordered? sorted less-than?)",
            "(same? (length sorted) 10000)",
            "(all? (lambda! (n) (same? (length (filter (lambda! (x) (same? x n)) sorted))
                                       (length (filter (lambda! (x) (same? x n)) numbers))))
                   (range 0 100))",
            // Stable: elements that are equal keep their order, here the order of their index.
            "(let! ((pairs (map pair numbers (range 0 10000))) (s (sort pairs by-first)))
               (ordered? s (lambda! (a b) (if! (same? (first a) (first b))
                                            (less-than? (rest a) (rest b))
                                            (less-than? (first a) (first b))))))",
            "(empty-list? (sort () less-than?))",
            "(equal? (sort (list \"b\" \"ab\" \"a\" \"\") less-than?) (list \"\" \"a\" \"ab\" \"b\"))",
            // The comparisons built on less-than? take text with no change.
            "(greater-than? \"b\" \"a\")",
            "(less-than-or-equal? \"a\" \"a\")",
            "(greater-than-or-equal? #bytes(2) #bytes(1 9))",
            "(same? (minimum \"b\" \"a\") \"a\")",
            "(same? (maximum \"b\" \"a\") \"b\")",
        ],
    );
    fails(&mut g, &[("(sort (list 2 1) add)", "type-error")]);
    // less-than? on strings is Rust's < on str, byte by byte, and on bytes the same order.
    let words = ["", "a", "b", "ab", "abc", "B", "é", "e", "日本", "日"];
    for a in words {
        for b in words {
            let got = g.evaluate(&mut (), &format!("(less-than? {a:?} {b:?})"));
            assert_eq!(got, Ok(Value::Boolean(a < b)), "{a:?} {b:?}");
            let bytes = format!("(less-than? (string-to-bytes {a:?}) (string-to-bytes {b:?}))");
            let got = g.evaluate(&mut (), &bytes);
            assert_eq!(got, Ok(Value::Boolean(a < b)), "{a:?} {b:?}");
        }
    }
}

#[test]
fn list_access_by_index() {
    let mut g = grift();
    g.evaluate(&mut (), "(define! xs (range 10 20))").unwrap();
    holds(
        &mut g,
        &[
            // Element i is the i-th of the list, counting from 0, and take and drop split it there.
            "(all? (lambda! (i) (same? (element-at xs i) (add 10 i))) (range 0 10))",
            "(all? (lambda! (i) (equal? (append (take xs i) (drop xs i)) xs)) (range 0 11))",
            "(all? (lambda! (i) (same? (length (take xs i)) i)) (range 0 11))",
            "(and!? (same? (second xs) 11) (same? (third xs) 12) (same? (last xs) 19))",
            "(same? (last (list 1)) 1)",
            // Out of range: the operands, the list and the index or count, as for a string.
            "(equal? (catch! (element-at xs 10) e (evaluate-in! values e)) (list xs 10))",
            "(equal? (catch! (element-at xs -1) e (evaluate-in! values e)) (list xs -1))",
            "(equal? (catch! (take xs 11) e (evaluate-in! values e)) (list xs 11))",
            "(equal? (catch! (drop xs 12) e (evaluate-in! values e)) (list xs 12))",
            "(equal? (catch! (second (list 1)) e (evaluate-in! values e)) (list (list 1) 1))",
            "(equal? (catch! (third (list 1 2)) e (evaluate-in! values e)) (list (list 1 2) 2))",
            "(equal? (catch! (last ()) e (evaluate-in! values e)) (list ()))",
            "(equal? (catch! (take 2 xs) e (evaluate-in! values e)) (list xs))",
            "(equal? (catch! (element-at xs -9223372036854775808) e (evaluate-in! values e))
                     (list xs -9223372036854775808))",
            "(equal? (catch! (drop xs -9223372036854775808) e (evaluate-in! values e))
                     (list xs -9223372036854775808))",
            "(equal? (catch! (take xs -1) e (evaluate-in! values e)) (list xs -1))",
        ],
    );
    fails(
        &mut g,
        &[
            ("(last ())", "range-error"),
            ("(second (list 1))", "range-error"),
            ("(second ())", "range-error"),
            ("(third (list 1 2))", "range-error"),
            ("(third (list 1))", "range-error"),
            ("(third ())", "range-error"),
            ("(element-at xs 1.0)", "type-error"),
            ("(drop 2 xs)", "type-error"),
        ],
    );
}

/// A small deterministic generator (xorshift*), so the generated programs need no dependency and
/// a failure names its seed.
struct Random(u64);

impl Random {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize % n
    }

    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

/// Values of every kind, by what the natives take: integers at the ends and the shift bounds,
/// floats with signed zero, the infinities and not-a-number, UTF-8 strings and number texts,
/// symbols, contexts, combiners, pairs and the constants.
const INTEGER_TEXTS: &[&str] = &[
    "0",
    "1",
    "-1",
    "2",
    "7",
    "-8",
    "62",
    "63",
    "64",
    "255",
    "1114111",
    "1114112",
    "55296",
    "57343",
    "9223372036854775807",
    "-9223372036854775808",
    "4611686018427387904",
    "-4611686018427387905",
    "-6148914691236517206",
];
const FLOAT_TEXTS: &[&str] = &[
    "0.0",
    "-0.0",
    "1.5",
    "-1.5",
    "0.5",
    "-0.99",
    "#infinity",
    "#-infinity",
    "#not-a-number",
    "9.3e18",
    "-9.3e18",
    "9223372036854775807.0",
    "-9223372036854775808.0",
    "4.5e15",
    "1e300",
];
const STRING_TEXTS: &[&str] = &[
    "\"\"",
    "\"a\"",
    "\"h\\xe9;llo\"",
    "\"\\x3bb;x\\x3bb;\\x1f600;\"",
    "\"a\\rb\\rb\"",
    "\"abcabc\"",
    "\"bcabcab\"",
    "\"bc\"",
    "\"+12\"",
    "\"-0\"",
    "\"0042\"",
    "\"9223372036854775807\"",
    "\"9223372036854775808\"",
    "\"-9223372036854775808\"",
    "\"-9223372036854775809\"",
    "\"12x\"",
    "\"+\"",
    "\"-\"",
    "\" 1\"",
];
const OTHER_TEXTS: &[&str] = &[
    "()",
    "#true",
    "#false",
    "#inert",
    "#ignore",
    "(quote! x)",
    "(quote! first)",
    "here",
    "(context ())",
    "record",
    "(list 1 2)",
    "(pair 1 2)",
    "(quote! (a . b))",
    "first",
    "if!",
    "(unwrap first)",
    "(wrap if!)",
    "sequence!",
    "search",
    "double",
    "note",
    "(malformed 0)",
    "(malformed 1)",
    "(malformed 2)",
    "#bytes(0 255)",
];

/// Each native with what each of its arguments usually is: `i` an integer, `f` a float, `s` a
/// string, `t` a part of one, `o` an offset, `n` a shift amount, `c` a context, `y` a symbol, `p` a
/// pair, `b` a test, `B` a bytes value, `l` a list of bytes, `x` anything.
const NATIVES: &[(&str, &str)] = &[
    ("first", "p"),
    ("rest", "p"),
    ("select", "bxx"),
    ("lookup", "cyx"),
    ("bitwise-and", "ii"),
    ("bitwise-or", "ii"),
    ("bitwise-exclusive-or", "ii"),
    ("shift-left", "in"),
    ("shift-right", "in"),
    ("integer-to-string", "i"),
    ("string-to-integer", "s"),
    ("integer-to-float", "i"),
    ("float-to-integer", "f"),
    ("byte-length", "s"),
    ("slice", "soo"),
    ("code-point-to-string", "i"),
    ("if!", "bxx"),
    ("sequence!", "xxx"),
    ("search", "sto"),
    ("byte-length", "B"),
    ("byte-at", "Bo"),
    ("slice", "Boo"),
    ("join", "BB"),
    ("string-to-bytes", "s"),
    ("bytes-to-string", "B"),
    ("list-to-bytes", "l"),
    ("bytes-to-list", "B"),
];

/// Where a generated call stands: at top level, as an operand, in a body's tail, in a branch
/// taken or not, before or as the last of a sequence, in a handler and a cleanup, applied,
/// unwrapped, wrapped again, and bound and read back.
const PLACES: &[&str] = &[
    "{}",
    "(list {} (note 2))",
    "((lambda! () {}))",
    "(double {})",
    "(if! #true {} (note 0))",
    "(if! #false (note 0) {})",
    "(sequence! {} (note 3))",
    "(sequence! (note 3) {})",
    "(catch! {} e (list (evaluate-in! kind e) (evaluate-in! values e) (evaluate-in! trace e)))",
    "(catch! (raise (error (quote! mine) 1)) #ignore {})",
    "(finally! {} (note 9))",
    "(finally! (note 9) {})",
    "(sequence! (bind record (quote! x) {}) (lookup record (quote! x) 0))",
    "(catch-kind! {} type-error e (evaluate-in! values e))",
    // A form that isn't a combination failing after one that returned through deeper ones: where
    // the error is reported depends on the twin restoring the place.
    "(sequence! (double {}) nope)",
    "((lambda! () (double {}) nope))",
];

/// One generated expression.
fn generated(r: &mut Random, depth: usize) -> String {
    let (name, kinds) = NATIVES[r.below(NATIVES.len())];
    let mut arguments: Vec<String> = kinds.chars().map(|k| argument(r, k, depth)).collect();
    // Missing and excess operands now and then.
    match r.below(12) {
        0 => {
            arguments.pop();
        }
        1 => arguments.push(argument(r, 'x', depth)),
        _ => {}
    }
    let arguments = arguments.join(" ");
    let call = match r.below(14) {
        0 if !name.ends_with('!') => format!("(apply {name} (list {arguments}))"),
        1 => format!("((unwrap {name}) {arguments})"),
        2 => format!("((wrap {name}) {arguments})"),
        3 => format!("({name} . {arguments})"),
        4 => format!(
            "(list (type-of {name}) (operative? {name}) (applicative? {name}) (value-to-string {name}) (same? {name} (unwrap (wrap {name}))))"
        ),
        5 => format!("({name} {arguments} . 1)"),
        _ => format!("({name} {arguments})"),
    };
    r.pick(PLACES).replace("{}", &call)
}

fn argument(r: &mut Random, kind: char, depth: usize) -> String {
    let pool = match (r.below(8), kind) {
        (0, _) => OTHER_TEXTS,
        (1, _) => [INTEGER_TEXTS, FLOAT_TEXTS, STRING_TEXTS, OTHER_TEXTS][r.below(4)],
        (_, 'i' | 'o' | 'n') if r.below(3) == 0 && depth > 0 => {
            return generated(r, depth - 1);
        }
        (_, 'i') => INTEGER_TEXTS,
        (_, 'o') => &["-1", "0", "1", "2", "3", "4", "6", "7", "100"],
        (_, 'n') => &["-1", "0", "1", "2", "13", "62", "63", "64"],
        (_, 'f') => FLOAT_TEXTS,
        (_, 's') => STRING_TEXTS,
        (_, 't') => &[
            "\"\"",
            "\"b\"",
            "\"bc\"",
            "\"ca\"",
            "\"l\"",
            "\"\\x3bb;\"",
            "\"\\rb\"",
            "\"0\"",
        ],
        (_, 'c') => &[
            "here",
            "record",
            "(context ())",
            "(context record)",
            "1",
            "(malformed 0)",
        ],
        (_, 'y') => &[
            "(quote! x)",
            "(quote! y)",
            "(quote! first)",
            "(quote! nope)",
            "nope",
            "\"x\"",
        ],
        (_, 'p') => &[
            "(list 1 2)",
            "(pair 1 2)",
            "(quote! (a . b))",
            "()",
            "1",
            "(list (list))",
            "(malformed 2)",
        ],
        (_, 'b') => &["#true", "#false", "(same? 1 1)", "(note #false)", "1", "()"],
        (_, 'B') => &[
            "#bytes()",
            "#bytes(97)",
            "#bytes(0 98 255 99)",
            "#bytes(104 195 169)",
            "#bytes(240 159 152 128)",
            "#bytes(195)",
            "#bytes(237 160 128)",
            "(string-to-bytes \"bc\")",
            "\"bc\"",
            "(malformed 3)",
        ],
        (_, 'l') => &[
            "()",
            "(list 1 2)",
            "(list 0 255)",
            "(list 256)",
            "(list 1 -1)",
            "(pair 1 2)",
            "(list 1 \"a\")",
            "5",
            "(list (note 7))",
        ],
        _ if depth > 0 && r.below(2) == 0 => return generated(r, depth - 1),
        _ => [INTEGER_TEXTS, FLOAT_TEXTS, STRING_TEXTS, OTHER_TEXTS][r.below(4)],
    };
    r.pick(pool).to_string()
}

/// What a host sees of a value, compared across two interpreters: the same structure, and the
/// same identities, so aliasing is the same too.
struct Comparison<'a> {
    left: &'a Grift<Vec<String>>,
    right: &'a Grift<Vec<String>>,
    matched: Vec<(Value, Value)>,
    /// How each native prints, as itself or as its definition.
    natives: &'a [String],
}

/// How each native primitive of the two interpreters prints.
fn native_texts(left: &mut Grift<Vec<String>>, right: &mut Grift<Vec<String>>) -> Vec<String> {
    let mut texts = Vec::new();
    for p in Primitive::ALL.iter().filter(|p| !CORE.contains(&p.name())) {
        for g in [&mut *left, &mut *right] {
            let bound = g.evaluate(&mut Vec::new(), p.name()).unwrap();
            texts.push(show(g, &g.underlying(&bound)));
        }
    }
    texts
}

/// `text`, with every native in it printed alike.
fn plain(text: &str, natives: &[String]) -> String {
    natives.iter().fold(text.to_string(), |text, native| {
        text.replace(native, "#[native]")
    })
}

impl Comparison<'_> {
    /// Whether two texts differ by more than how a native prints: the one thing a native and its
    /// definition may differ in.
    fn differs(&self, a: &str, b: &str) -> bool {
        a != b && plain(a, self.natives) != plain(b, self.natives)
    }

    fn values(&mut self, a: &Value, b: &Value) -> Result<(), String> {
        let (l, r) = (self.left, self.right);
        let mut todo = vec![(a.clone(), b.clone())];
        while let Some((a, b)) = todo.pop() {
            let differ = || Err(format!("{} against {}", show(l, &a), show(r, &b)));
            match (&a, &b) {
                (Value::Float(x), Value::Float(y)) if x.to_bits() != y.to_bits() => {
                    return differ();
                }
                (Value::Symbol(x), Value::Symbol(y))
                    if l.symbol_name(x).unwrap() != r.symbol_name(y).unwrap() =>
                {
                    return differ();
                }
                (Value::String(_), Value::String(_))
                    if self.differs(
                        l.as_str(&a).unwrap().unwrap(),
                        r.as_str(&b).unwrap().unwrap(),
                    ) =>
                {
                    return differ();
                }
                (Value::Bytes(_), Value::Bytes(_)) if l.as_bytes(&a) != r.as_bytes(&b) => {
                    return differ();
                }
                (
                    Value::Operative(Operative(OperativeKind::Host(..))),
                    Value::Operative(Operative(OperativeKind::Host(..))),
                ) if show(l, &a) != show(r, &b) => return differ(),
                (Value::Float(_), Value::Float(_))
                | (Value::Symbol(_), Value::Symbol(_))
                | (Value::String(_), Value::String(_))
                | (Value::Bytes(_), Value::Bytes(_))
                | (
                    Value::Operative(Operative(OperativeKind::Host(..))),
                    Value::Operative(Operative(OperativeKind::Host(..))),
                ) => {}
                (Value::Pair(_), Value::Pair(_))
                | (Value::Context(_), Value::Context(_))
                | (
                    Value::Operative(Operative(OperativeKind::Compound(_))),
                    Value::Operative(Operative(OperativeKind::Compound(_))),
                )
                | (Value::Applicative(_), Value::Applicative(_)) => {
                    let known = self.matched.iter().find(|(x, y)| *x == a || *y == b);
                    match known {
                        Some((x, y)) if *x == a && *y == b => continue,
                        Some(_) => return Err(format!("aliasing differs at {}", show(l, &a))),
                        None => self.matched.push((a.clone(), b.clone())),
                    }
                    if let (Some((a1, a2)), Some((b1, b2))) =
                        (l.as_pair(&a).unwrap(), r.as_pair(&b).unwrap())
                    {
                        todo.push((a1, b1));
                        todo.push((a2, b2));
                    } else if let Value::Context(_) = a {
                        let (x, y) = (l.names(&a).unwrap(), r.names(&b).unwrap());
                        let names = |g: &Grift<Vec<String>>, n: &[crate::Symbol]| {
                            n.iter()
                                .map(|s| g.symbol_name(s).unwrap().to_string())
                                .collect::<Vec<_>>()
                        };
                        if names(l, &x) != names(r, &y) {
                            return differ();
                        }
                        for (s, t) in x.iter().zip(&y) {
                            todo.push((
                                l.lookup(&a, s).unwrap().unwrap(),
                                r.lookup(&b, t).unwrap().unwrap(),
                            ));
                        }
                    } else if let Value::Applicative(_) = a {
                        todo.push((l.underlying(&a), r.underlying(&b)));
                    } else if self.differs(&show(l, &a), &show(r, &b)) {
                        return differ();
                    }
                }
                // What a native is, its definition is: the comparison's one allowed difference.
                (
                    Value::Operative(Operative(OperativeKind::Primitive(p))),
                    Value::Operative(Operative(OperativeKind::Compound(_))),
                ) if r.is_definition_of(&b, *p) => {}
                _ if std::mem::discriminant(&a) != std::mem::discriminant(&b) || a != b => {
                    return differ();
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn outcomes(
        &mut self,
        a: &Result<Value, Error>,
        b: &Result<Value, Error>,
    ) -> Result<(), String> {
        match (a, b) {
            (Ok(a), Ok(b)) => self.values(a, b),
            (Err(a), Err(b)) => {
                let (l, r) = (self.left, self.right);
                if kind(l, a) != kind(r, b) || a.values.len() != b.values.len() {
                    return Err(format!(
                        "{} {:?} against {} {:?}",
                        kind(l, a),
                        a.values,
                        kind(r, b),
                        b.values
                    ));
                }
                if a.trace.len() != b.trace.len() {
                    return Err(format!("trace {} against {}", a.trace.len(), b.trace.len()));
                }
                for (x, y) in a.trace.iter().zip(&b.trace) {
                    if l.position(x) != r.position(y) {
                        return Err(format!(
                            "{} placed {:?} against {:?}",
                            show(l, x),
                            l.position(x),
                            r.position(y)
                        ));
                    }
                    self.values(x, y)?;
                }
                a.values
                    .iter()
                    .zip(&b.values)
                    .try_for_each(|(x, y)| self.values(x, y))
            }
            _ => Err(String::from("a value against an error")),
        }
    }
}

/// `(note x)`: logs x as it prints, and gives it back: the order and number of evaluations.
fn note(g: &mut Grift<Vec<String>>, log: &mut Vec<String>, args: &[Value]) -> Result<Value, Error> {
    log.push(
        args.iter()
            .map(|v| show(g, v))
            .collect::<Vec<_>>()
            .join(" "),
    );
    Ok(args.first().cloned().unwrap_or(Value::Inert))
}

/// What a host might hand back wrongly: an object's reference in another kind's variant.
fn malformed(
    g: &mut Grift<Vec<String>>,
    _: &mut Vec<String>,
    args: &[Value],
) -> Result<Value, Error> {
    let string = g.string("not a context")?;
    let pair = g.pair(Value::Integer(1), Value::Integer(2))?;
    let (Value::String(s), Value::Pair(p)) = (string, pair) else {
        return Err(g.error("not-made", &[]));
    };
    Ok(match args.first() {
        Some(Value::Integer(0)) => Value::Context(s),
        Some(Value::Integer(1)) => Value::String(p),
        Some(Value::Integer(3)) => Value::Bytes(s),
        _ => Value::Pair(s),
    })
}

/// Two interpreters alike, one running the natives and one their grift definitions.
fn twins() -> [Grift<Vec<String>>; 2] {
    [true, false].map(|natives| {
        let mut g: Grift<Vec<String>> = Grift::new().unwrap();
        if !natives {
            g.run_definitions();
        }
        for (name, f) in [
            ("note", note as crate::HostFunction<Vec<String>>),
            ("malformed", malformed),
        ] {
            let f = g.host_function(name, f).unwrap();
            g.define(name, f).unwrap();
        }
        g.evaluate(
            &mut Vec::new(),
            "(define! here (current-context))
             (define! record (context ()))
             (bind record (quote! (x first)) (list 5 6))
             (define! double (lambda! (x) (list x x)))",
        )
        .unwrap();
        g
    })
}

#[test]
fn natives_are_their_definitions() {
    let [mut native, mut defined] = twins();
    let natives = native_texts(&mut native, &mut defined);
    // Every primitive grift can express has its definition, and no other has.
    for &p in Primitive::ALL {
        let (a, b) = (
            native.evaluate(&mut Vec::new(), p.name()).unwrap(),
            defined.evaluate(&mut Vec::new(), p.name()).unwrap(),
        );
        let same_as_native = show(&native, &a) == show(&defined, &b);
        assert_eq!(same_as_native, CORE.contains(&p.name()), "{}", p.name());
    }
    // A separately read outer form can enter through a native branch or sequence body too.
    // Compare the complete errors and positions, not just the form's returned value.
    for g in [&mut native, &mut defined] {
        let forms = g.read_in("call.grift", "(f 1)").unwrap();
        let (form, _) = g.as_pair(&forms).unwrap().unwrap();
        g.define("form", form).unwrap();
        g.evaluate(&mut Vec::new(), "(define! f (lambda! (x) (first x)))")
            .unwrap();
    }
    for src in [
        "(evaluate (list (quote! if!) #true form #inert) (current-context))",
        "(evaluate (list (quote! if!) #false #inert form) (current-context))",
        "(evaluate (list (quote! sequence!) form #inert) (current-context))",
        "(evaluate (list (quote! sequence!) #inert form) (current-context))",
    ] {
        let a = native.evaluate(&mut Vec::new(), src);
        let b = defined.evaluate(&mut Vec::new(), src);
        let mut compare = Comparison {
            left: &native,
            right: &defined,
            matched: Vec::new(),
            natives: &natives,
        };
        if let Err(why) = compare.outcomes(&a, &b) {
            panic!("{src}: {why}");
        }
        for (g, result) in [(&native, &a), (&defined, &b)] {
            assert!(
                result
                    .as_ref()
                    .unwrap_err()
                    .trace
                    .iter()
                    .any(|at| g.position(at).unwrap() == Some(("call.grift", 1)))
            );
        }
    }
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let mut counts = [0usize; 2];
    let grids = boundary_cases();
    let cases = grids.len() + 2500;
    for case in 0..cases {
        let src = match grids.get(case) {
            Some(src) => src.clone(),
            None => generated(&mut random, 2),
        };
        let place = format!("case-{case}.grift");
        let (mut a_log, mut b_log) = (Vec::new(), Vec::new());
        let a_scope = native.context(Some(&native.top_level())).unwrap();
        let b_scope = defined.context(Some(&defined.top_level())).unwrap();
        let a = native.evaluate_in(&mut a_log, &a_scope, &place, &src);
        let b = defined.evaluate_in(&mut b_log, &b_scope, &place, &src);
        counts[usize::from(a.is_err())] += 1;
        let mut compare = Comparison {
            left: &native,
            right: &defined,
            matched: Vec::new(),
            natives: &natives,
        };
        if let Err(why) = compare.outcomes(&a, &b) {
            panic!("case {case}: {src}\n  {why}");
        }
        // What a case changed persists: the top level and the record, bindings and all.
        let (a_top, b_top) = (native.top_level(), defined.top_level());
        if let Err(why) = compare.values(&a_top, &b_top) {
            panic!("case {case}: {src}\n  afterwards, the top level: {why}");
        }
        let (a_log, b_log) = (a_log.join("\n"), b_log.join("\n"));
        assert!(
            !compare.differs(&a_log, &b_log),
            "case {case}: {src}: what ran"
        );
    }
    // Floats from Rust, with any bits, not-a-number's payloads included, through Rust calls.
    for bits in [
        0,
        1,
        1 << 63,
        0x7ff8_0000_0000_0011,
        0xfff0_0000_0000_0000,
        0x7ff0_0000_0000_0001,
    ] {
        let f = Value::Float(f64::from_bits(bits));
        for (name, arguments) in [
            ("float-to-integer", vec![f.clone()]),
            (
                "select",
                vec![Value::Boolean(true), f.clone(), Value::Integer(0)],
            ),
            (
                "(wrap if!)",
                vec![Value::Boolean(false), Value::Integer(0), f.clone()],
            ),
            ("(wrap sequence!)", vec![Value::Integer(0), f.clone()]),
            ("integer-to-float", vec![f.clone()]),
        ] {
            let (fa, fb) = (
                native.evaluate(&mut Vec::new(), name),
                defined.evaluate(&mut Vec::new(), name),
            );
            let fa = native.call(&mut Vec::new(), &fa.unwrap(), &arguments);
            let fb = defined.call(&mut Vec::new(), &fb.unwrap(), &arguments);
            let mut compare = Comparison {
                left: &native,
                right: &defined,
                matched: Vec::new(),
                natives: &natives,
            };
            if let Err(why) = compare.outcomes(&fa, &fb) {
                panic!("({name} {bits:#x}) from Rust: {why}");
            }
        }
    }
    // Both kinds of outcome are well represented.
    assert!(
        counts.iter().all(|&n| n > 500),
        "values and errors: {counts:?}"
    );
}

/// The edges no generator should have to find: every byte offset pair of a text of one-, two-,
/// four- and one-byte characters and of a bytes value, the scalar values' edges, UTF-8 that is cut
/// short, overlong, a surrogate or past U+10FFFF, lists of bytes that aren't, the bitwise
/// operations on the integers' ends and the shift bounds, and mutations that persist.
fn boundary_cases() -> Vec<String> {
    let mut cases = Vec::new();
    for start in -1..5 {
        for end in -1..5 {
            cases.push(format!("(slice #bytes(1 2 3) {start} {end})"));
        }
        cases.push(format!("(byte-at #bytes(1 2 3) {start})"));
    }
    for utf8 in [
        "0 195 169 255 90",
        "97 255",
        "104 105 192 128",
        "195 169 226 40 161",
        "65 240 159 152",
        "226 130",
        "237 160 128",
        "237 159 191",
        "224 160 128",
        "224 159 191",
        "244 143 191 191",
        "244 144 128 128",
        "248 128",
        "0 239 187 191 195 169 128",
    ] {
        cases.push(format!("(bytes-to-string #bytes({utf8}))"));
    }
    cases.extend([
        "(list-to-bytes (list 0 1 255 256))".into(),
        "(list-to-bytes (pair 1 (pair 2 3)))".into(),
        "(join #bytes(1) (note #bytes()) #bytes(2 3) 4)".into(),
        "(join)".into(),
        "(bytes-to-list (string-to-bytes \"\\xe9;\\x1f600;\"))".into(),
    ]);
    for start in -1..10 {
        for end in -1..10 {
            cases.push(format!("(slice \"\\xe9;\\x1f603;a\" {start} {end})"));
        }
        cases.push(format!("(search \"\\xe9;ab\\xe9;ab\" \"ab\" {start})"));
    }
    for n in [
        -1, 0, 31, 127, 128, 55295, 55296, 57343, 57344, 65535, 65536, 1114111, 1114112,
    ] {
        cases.push(format!("(code-point-to-string {n})"));
    }
    let ends = [i64::MIN, -1, 0, 1, 31, 62, 63, 64, i64::MAX];
    for x in ends {
        for y in ends {
            for p in [
                "bitwise-and",
                "bitwise-or",
                "bitwise-exclusive-or",
                "shift-left",
                "shift-right",
            ] {
                cases.push(format!("({p} {x} {y})"));
            }
        }
    }
    for text in [
        "\"-9223372036854775808\"",
        "\"9223372036854775807\"",
        "\"-0\"",
        "\"+\"",
        "\"0x1\"",
    ] {
        cases.push(format!("(string-to-integer {text})"));
    }
    cases.extend([
        "(sequence! (bind record (quote! answer) (note 17)) (bind record (quote! last) (note 18)) #inert)".into(),
        "(if! #true (bind record (quote! answer) (note 19)) (bind record (quote! answer) (note 99)))".into(),
        // Definitions inside the twins bind where the library's would: in the caller's context.
        "(sequence! (define! made 1) (define! made-too 2) 3) (list made made-too)".into(),
        "(if! #true (define! made 3) 0) made".into(),
        "((lambda! () (sequence! (define! inner 4) inner)))".into(),
        // Errors in each place of a twin, for their traces and lines.
        "(sequence! (first 1) 2)".into(),
        "(sequence! 1 (first 1))".into(),
        "((lambda! () (sequence! (rest 2) 3)))".into(),
        "(if! (first 1) 2 3)".into(),
        "(if! #true (first 1) 3)".into(),
        "(search \"ab\" (first 1) 0)".into(),
        // A partial match that overlaps the real one, and a pattern of the other kind.
        "(search \"aab\" \"ab\" 0)".into(),
        "(search #bytes(1 1 2) #bytes(1 2) 0)".into(),
        "(search \"ab\" #bytes(98) 0)".into(),
        "(search #bytes(97 98) \"b\" 0)".into(),
        "(lookup (malformed 0) (quote! x) 42)".into(),
        "(catch! (lookup (malformed 0) (quote! x) 42) e (evaluate-in! kind e))".into(),
    ]);
    cases
}

#[test]
fn natives_and_their_definitions_run_tail_calls_in_constant_space() {
    for (mut g, natives) in twins().into_iter().zip([true, false]) {
        g.evaluate(
            &mut Vec::new(),
            "(define! loop (lambda! (n) (if! (same? n 0) (first (list n)) (sequence! (search \"ab\" \"b\" 0) (shift-right n 1) (byte-length #bytes(1 2)) (loop (subtract n 1))))))
             (define! count (lambda! (n) (if! (same? n 0) 0 (count (subtract n 1)))))",
        )
        .unwrap();
        let mut high = Vec::new();
        for n in [100, 3000] {
            for f in ["loop", "count"] {
                let r = g.evaluate(&mut Vec::new(), &format!("({f} {n})"));
                assert_eq!(r, Ok(Value::Integer(0)), "({f} {n})");
            }
            high.push(g.machine.frames_high());
        }
        assert_eq!(
            high[0], high[1],
            "frames grew with the loop (natives {natives})",
        );
    }
}

#[test]
fn string_literals_share_the_reader_and_both_writers() {
    let mut g = grift();
    // Every letter grift takes after `\` means what Rust's escape of it does; any other ASCII
    // character there is a bad-escape.
    let rust = [
        ('\\', '\\'),
        ('"', '"'),
        ('n', '\n'),
        ('r', '\r'),
        ('t', '\t'),
    ];
    for c in (32u8..127).map(char::from).filter(|&c| c != 'x') {
        let read = g.evaluate(&mut (), &format!("\"\\{c}\""));
        match rust.iter().find(|(letter, _)| *letter == c) {
            Some(&(_, meant)) => {
                let v = read.unwrap();
                assert_eq!(g.as_str(&v), Ok(Some(meant.to_string().as_str())), "\\{c}");
            }
            None => {
                let e = read.unwrap_err();
                assert_eq!(show(&g, &e.values[0]), "bad-escape", "\\{c}");
            }
        }
    }
    // Every control character prints as an escape that reads back as it.
    let controls: String = (0u8..32).map(char::from).chain(['\u{7f}']).collect();
    let v = g.string(&controls).unwrap();
    let printed = show(&g, &v);
    assert!(printed.contains("\\r") && printed.contains("\\n") && !printed.contains('\r'));
    let again = g.evaluate(&mut (), &printed).unwrap();
    assert_eq!(g.as_str(&again), Ok(Some(controls.as_str())));
    let mut texts = vec![
        String::new(),
        "\"\\;()\n\r\t\0a\u{1}f".into(),
        (0..=0x9f).filter_map(char::from_u32).collect(),
        "é e\u{301} λ 中 🦀 \u{2028}\u{2029}\u{feff}\u{10ffff}".into(),
    ];
    let mut random = Random(0x6d2b_79f5);
    for length in 0..64 {
        texts.push(
            (0..length % 33)
                .filter_map(|_| char::from_u32(random.below(0x11_0000) as u32))
                .collect(),
        );
    }
    let read_form = g.evaluate(&mut (), "read-form").unwrap();
    for text in texts {
        let mut written = String::new();
        crate::print::write_string(&text, &mut written).unwrap();
        assert!(!written.chars().any(char::is_control));
        let value = g.string(&text).unwrap();
        assert_eq!(show(&g, &value), written);
        let input = g.string(&written).unwrap();
        let read = g
            .call(&mut (), &read_form, &[input, Value::Integer(0)])
            .unwrap();
        let fields = g.list_items(&read).unwrap();
        assert_eq!(fields.len(), 3);
        assert_eq!(g.as_str(&fields[0]), Ok(Some(text.as_str())));
        assert_eq!(
            fields[1..],
            [Value::Integer(0), Value::Integer(written.len() as i64)]
        );
    }
    struct RefusesAfter(usize);
    impl std::fmt::Write for RefusesAfter {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0 = self.0.checked_sub(s.len()).ok_or(std::fmt::Error)?;
            Ok(())
        }
    }
    let sample = "a\n\"\\\0λ";
    let mut full = String::new();
    crate::print::write_string(sample, &mut full).unwrap();
    for budget in 0..full.len() {
        assert_eq!(
            crate::print::write_string(sample, &mut RefusesAfter(budget)),
            Err(std::fmt::Error)
        );
    }
}

#[test]
fn positions_name_each_combination_s_source_and_line() {
    let mut g = grift();
    let top = g.top_level();
    let src = "(define! f\n  (lambda! (x)\n    (first x)))\n\n(f 1)";
    let e = g.evaluate_in(&mut (), &top, "f.grift", src).unwrap_err();
    let lines: Vec<_> = e.trace.iter().map(|at| g.position(at).unwrap()).collect();
    assert_eq!(lines, [Some(("f.grift", 3)), Some(("f.grift", 5))]);
    // A loader written in grift evaluates named forms later in the same run. Its trace keeps
    // the same program combinations as evaluate_in, including a nested failing combination.
    let forms = g.read_in("f.grift", src).unwrap();
    g.define("forms", forms).unwrap();
    let later = g
        .evaluate(
            &mut (),
            "(define! here (current-context))
        (for-each (lambda! (form) (evaluate form here)) forms)",
        )
        .unwrap_err();
    assert_eq!(kind(&g, &later), kind(&g, &e));
    let places: Vec<_> = later
        .trace
        .iter()
        .take(2)
        .map(|at| g.position(at).unwrap())
        .collect();
    assert_eq!(places, [Some(("f.grift", 3)), Some(("f.grift", 5))]);
    let nested = g.read_in("nested.grift", "\n(add 1\n  (first 0))").unwrap();
    g.define("forms", nested).unwrap();
    let later = g
        .evaluate(
            &mut (),
            "(for-each (lambda! (form) (evaluate form here)) forms)",
        )
        .unwrap_err();
    assert_eq!(g.position(&later.trace[0]), Ok(Some(("nested.grift", 3))));
    // A named form tail-evaluating another replaces its finished boundary with the new one.
    let target = g.read_in("b.grift", "(f 1)").unwrap();
    let (target, _) = g.as_pair(&target).unwrap().unwrap();
    g.define("target", target).unwrap();
    let forms = g.read_in("a.grift", "(evaluate target here)").unwrap();
    g.define("forms", forms).unwrap();
    let later = g
        .evaluate(
            &mut (),
            "(for-each (lambda! (form) (evaluate form here)) forms)",
        )
        .unwrap_err();
    let places: Vec<_> = later
        .trace
        .iter()
        .map(|at| g.position(at).unwrap())
        .collect();
    assert!(places.contains(&Some(("b.grift", 1))), "{places:?}");
    assert!(!places.contains(&Some(("a.grift", 1))), "{places:?}");
    // The host's entry boundary is distinct and remains when its form tail-evaluates another.
    let later = g
        .evaluate_in(&mut (), &top, "host.grift", "(evaluate target here)")
        .unwrap_err();
    let places: Vec<_> = later
        .trace
        .iter()
        .map(|at| g.position(at).unwrap())
        .collect();
    assert!(places.contains(&Some(("b.grift", 1))), "{places:?}");
    assert_eq!(places.last(), Some(&Some(("host.grift", 1))));
    // Text evaluated with no name, and pairs a program makes, have none.
    let e = g.evaluate(&mut (), "(f 2)").unwrap_err();
    assert_eq!(g.position(&e.trace[1]), Ok(None));
    let made = g.evaluate(&mut (), "(list 1 2)").unwrap();
    assert_eq!(g.position(&made), Ok(None));
    // A slot reused by a pair read later answers with its own line, never the old one.
    for n in 0..30 {
        let src = format!("{}(f {n})", "\n".repeat(n));
        let e = g.evaluate_in(&mut (), &top, "g.grift", &src).unwrap_err();
        assert_eq!(g.position(&e.trace[1]), Ok(Some(("g.grift", n as u32 + 1))));
        g.evaluate(&mut (), "(length (map list (range 0 5000)))")
            .unwrap();
    }
}

/// Asks for a stop of the run in progress, then walks the tree it was given, with no step of the
/// machine between them: each long walk must find the stop itself.
fn walks_with_a_stop_waiting(
    g: &mut Grift<Interrupter>,
    stop: &mut Interrupter,
    args: &[Value],
) -> Result<Value, Error> {
    assert!(stop.stop(), "a run to stop");
    let tree = g.import(&args[0]).unwrap();
    let interrupted = |g: &Grift<Interrupter>, e: local::Error| {
        assert_eq!(kind(g, &g.export_error(e)), "interrupted");
    };
    let checked = g.check_parameters(tree, local::Value::Ignore);
    interrupted(g, checked.unwrap_err());
    let mut bindings = Bindings::default();
    let matched = g.match_tree(&mut bindings, tree, tree);
    interrupted(g, matched.unwrap_err());
    let Err(e) = g.write(&args[0], &mut String::new()) else {
        panic!("printing went on past a stop");
    };
    assert_eq!(kind(g, &e), "interrupted");
    Ok(Value::Inert)
}

#[test]
fn each_long_walk_finds_a_stop_by_itself() {
    let mut g: Grift<Interrupter> = Grift::new().unwrap();
    define(&mut g, "walk", walks_with_a_stop_waiting);
    let mut stop = g.interrupter();
    let r = g.evaluate(
        &mut stop,
        "(define! grow (lambda! (d n) (if! (same? n 0) d (grow (pair d d) (subtract n 1)))))
         (walk (grow () 16))",
    );
    assert_eq!(kind(&g, &r.unwrap_err()), "interrupted");
}

/// As [`walks_with_a_stop_waiting`], for a force, with the stop already acted on: it stays asked,
/// so each walk finds it by itself.
fn walks_with_a_force_waiting(
    g: &mut Grift<Interrupter>,
    stop: &mut Interrupter,
    args: &[Value],
) -> Result<Value, Error> {
    assert!(stop.stop(), "a run to stop");
    g.control.acted();
    assert!(!g.control.asked());
    assert!(stop.force(), "the run this handle stopped");
    let tree = g.import(&args[0]).unwrap();
    assert!(
        g.check_parameters(tree, local::Value::Ignore)
            .unwrap_err()
            .limit
    );
    let mut bindings = Bindings::default();
    assert!(g.match_tree(&mut bindings, tree, tree).unwrap_err().limit);
    let Err(e) = g.write(&args[0], &mut String::new()) else {
        panic!("printing went on past a force");
    };
    assert!(e.limit);
    Ok(Value::Inert)
}

#[test]
fn each_long_walk_finds_a_force_by_itself() {
    let mut g: Grift<Interrupter> = Grift::new().unwrap();
    define(&mut g, "walk", walks_with_a_force_waiting);
    let mut stop = g.interrupter();
    let r = g.evaluate(
        &mut stop,
        "(define! grow (lambda! (d n) (if! (same? n 0) d (grow (pair d d) (subtract n 1)))))
         (walk (grow () 16))",
    );
    assert_eq!(kind(&g, &r.unwrap_err()), "interrupted");
}

#[test]
fn interned_names_keep_their_identity_and_read_back() {
    let mut g = grift();
    let names: Vec<_> = (0..5000)
        .rev()
        .map(|n| format!("name-λ-{n}"))
        .chain(["é", "日本語", "-", "a-b", "a", "ab", "b", "name-"].map(String::from))
        .collect();
    let symbols: Vec<_> = names.iter().map(|name| g.symbol(name).unwrap()).collect();
    assert_eq!(
        symbols
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        names.len()
    );
    for (name, symbol) in names.iter().zip(&symbols).rev() {
        assert_eq!(g.symbol(name).unwrap(), *symbol);
        assert_eq!(g.symbol_name(symbol).unwrap(), name);
    }
    let read = g.read(&names.join("\u{a0}")).unwrap();
    let expected: Vec<_> = symbols.into_iter().map(Value::Symbol).collect();
    assert_eq!(g.list_items(&read).unwrap(), expected);
    for text in [
        "日本語\u{a0}é",
        "日本語 é ; end",
        "日本語\té; é",
        ";\n日本語 ; \n é ;",
    ] {
        let forms = g.read(text).unwrap();
        let want = [
            Value::Symbol(g.symbol("日本語").unwrap()),
            Value::Symbol(g.symbol("é").unwrap()),
        ];
        assert_eq!(g.list_items(&forms).unwrap(), want);
    }
    assert_eq!(g.read("; only a comment"), Ok(Value::EmptyList));
    assert_eq!(g.read("; é"), Ok(Value::EmptyList));
    let forms = g
        .read_in("comments.grift", "; é\n; λ\r\n\u{a0}(list 1)")
        .unwrap();
    assert_eq!(
        g.position(&g.list_items(&forms).unwrap()[0]),
        Ok(Some(("comments.grift", 3)))
    );
}

#[test]
fn a_run_reclaims_discarded_reader_allocations_before_needing_a_slot() {
    // Refuse new slots at a fixed length, without a global allocator. This makes the same
    // exhausted free list as a failed heap-vector reserve; collection can reuse its garbage.
    for (headroom, live) in [(8192, 0), (32, 20000)] {
        for named in [false, true] {
            for entry in ["evaluate", "evaluate_in", "call"] {
                let mut g = grift();
                let retained = g.read(&"() ".repeat(live)).unwrap();
                g.define("retained", retained).unwrap();
                g.evaluate(&mut (), "#inert").unwrap();
                let inputs = g
                    .evaluate(
                        &mut (),
                        "(list (lambda! args 3) (list 2) \"live\"
                    #bytes(3) (context ()) (vau! () #ignore #inert) (lambda! () #inert))",
                    )
                    .unwrap();
                let inputs = g.list_items(&inputs).unwrap();
                let f = &inputs[0];
                let arguments = &inputs[1..];
                let c = g.context(Some(&g.top_level())).unwrap();
                let x = Value::Symbol(g.symbol("x").unwrap());
                g.bind(&c, &x, Value::Integer(2)).unwrap();
                g.heap.slot_limit = g.heap.len() + headroom;
                let text = "(a b c)\n".repeat(32768);
                let error = if named {
                    g.read_in("big.grift", &text)
                } else {
                    g.read(&text)
                }
                .unwrap_err();
                assert_eq!(kind(&g, &error), "out-of-memory");
                let result = match entry {
                    "evaluate" => g.evaluate(&mut (), "(add 1 2)"),
                    "evaluate_in" => g.evaluate_in(&mut (), &c, "small.grift", "(add x 1)"),
                    _ => g.call(&mut (), f, arguments),
                };
                assert_eq!(
                    result,
                    Ok(Value::Integer(3)),
                    "{entry} after named={named}, headroom={headroom}"
                );
            }
        }
    }
}

#[test]
fn out_of_memory_from_a_run_or_host_allocation_allows_the_next_run() {
    let mut refused = Vec::new();
    for origin in ["run", "host"] {
        for entry in ["evaluate", "evaluate_in", "call"] {
            let mut g = grift();
            let retained = g.read(&"() ".repeat(20000)).unwrap();
            g.define("retained", retained).unwrap();
            g.evaluate(&mut (), "#inert").unwrap();
            let f = g.evaluate(&mut (), "(lambda! (x) x)").unwrap();
            let kept = g.keep(f).unwrap();
            let c = g.context(Some(&g.top_level())).unwrap();
            let x = Value::Symbol(g.symbol("x").unwrap());
            g.bind(&c, &x, Value::Integer(2)).unwrap();
            let scope = g.keep(c).unwrap();
            g.evaluate(
                &mut (),
                "(define! churn (lambda! (n)
                (if! (same? n 0) #inert (sequence! (pair n n) (churn (subtract n 1))))))",
            )
            .unwrap();
            g.heap.slot_limit = g.heap.len() + 32;
            let error = match origin {
                "run" => g.evaluate(&mut (), "(churn 100000)").unwrap_err(),
                _ => (0..100000)
                    .try_for_each(|_| g.pair(Value::Inert, Value::Inert).map(drop))
                    .unwrap_err(),
            };
            assert_eq!(kind(&g, &error), "out-of-memory");
            let result = match entry {
                "evaluate" => g.evaluate(&mut (), "(add 1 2)"),
                "evaluate_in" => g.evaluate_in(
                    &mut (),
                    &g.kept(&scope).unwrap(),
                    "small.grift",
                    "(add x 1)",
                ),
                _ => g.call(&mut (), &g.kept(&kept).unwrap(), &[Value::Integer(3)]),
            };
            if result != Ok(Value::Integer(3)) {
                refused.push(format!("{entry} after {origin}: {result:?}"));
            }
        }
    }
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}

#[test]
fn record_is_a_new_context_of_its_names_alone() {
    holds(
        &mut grift(),
        &[
            "(same? (evaluate-in! y (record (quote! (x y)) (list 1 2))) 2)",
            "(equal? (names (record (quote! (x y)) (list 1 2))) (quote! (x y)))",
            "(not? (binds? (record () ()) (quote! list)))",
        ],
    );
}

#[test]
fn a_body_of_several_forms_needs_no_binding_of_the_program() {
    let mut g = grift();
    g.evaluate(&mut (), "(define! sequence! 5)").unwrap();
    holds(
        &mut g,
        &[
            "(same? ((lambda! () 1 2)) 2)",
            "(same? ((evaluate (list lambda! () 1 2) (context ()))) 2)",
        ],
    );
}

#[test]
fn an_overflow_carries_the_operands_of_the_arithmetic_that_overflowed() {
    holds(
        &mut grift(),
        &[
            "(equal? (catch! (absolute-value -9223372036854775808) e (evaluate-in! values e))
                     (list 0 -9223372036854775808))",
            "(same? (catch! (power 2 100) e (evaluate-in! kind e)) (quote! overflow))",
        ],
    );
}

#[test]
fn string_split_searches_at_string_searchs_speed() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! short \"abc\") (define! long (string-repeat \"abc\" 1000))",
    )
    .unwrap();
    let (short, _) = cost(&mut g, "(string-split short \",\")");
    let (long, _) = cost(&mut g, "(string-split long \",\")");
    assert_eq!(short, long);
}

#[test]
fn standard_context_is_a_new_child_of_the_built_in_context() {
    let mut g = grift();
    g.evaluate(
        &mut (),
        "(define! list 0) (define! mine 1) (define! c (standard-context))",
    )
    .unwrap();
    holds(
        &mut g,
        &[
            "(equal? (evaluate (quote! (list 1 2)) c) (quote! (1 2)))",
            "(not? (binds? c (quote! mine)))",
            "(same? (names c) ())",
            "(same? (bind c (quote! map) 5) #inert)",
            "(applicative? (evaluate (quote! map) (standard-context)))",
        ],
    );
}

/// `local::Value`'s tag is a whole word, so a copy loads it at once and never waits on a one-byte
/// write just before (#210: about 16% of every program on x86-64). Its size can't show this: a
/// one-byte tag pads to the same 16 bytes. So: the payload starts a word in.
#[test]
fn a_values_tag_is_a_whole_word() {
    let v = local::Value::Boolean(true);
    let local::Value::Boolean(b) = &v else {
        panic!("a boolean")
    };
    let offset = std::ptr::from_ref(b).addr() - std::ptr::from_ref(&v).addr();
    assert_eq!(offset, size_of::<u64>());
}

/// A collection needs no memory of its own (208's P33): with no slot to be had and its mark stack
/// never growing, it still keeps everything live, however deep, and frees the garbage, so a run
/// that needs that memory gets it.
#[test]
fn a_collection_needs_no_memory_of_its_own() {
    let mut g: Grift = Grift::new().unwrap();
    g.evaluate(
        &mut (),
        "(define! keep (range 0 20000))
         (define! tree (map (lambda! (n) (list n (list n))) (range 0 2000)))
         (length (range 0 50000))",
    )
    .unwrap();
    g.heap.slot_limit = g.heap.len();
    let room = g.heap.mark_room();
    g.heap.request_collection();
    let v = g
        .evaluate(
            &mut (),
            "(list (length (range 0 40000)) (length keep) (first (reverse keep))
                   (first (second (first (reverse tree)))))",
        )
        .unwrap();
    assert_eq!(
        g.list_items(&v).unwrap(),
        [40000, 20000, 19999, 1999].map(Value::Integer)
    );
    assert_eq!(g.heap.mark_room(), room);
}

/// Every slot has its mark stack entry before any collection needs it: with memory refused and no
/// room, a collection would have nowhere to put what it reaches.
#[test]
fn the_mark_stack_has_room_for_every_slot() {
    let mut g: Grift = Grift::new().unwrap();
    assert!(g.heap.mark_room() >= g.heap.len());
    g.evaluate(&mut (), "(length (range 0 50000))").unwrap();
    assert!(g.heap.mark_room() >= g.heap.len());
}

/// Asks for a stop of the run in progress, then searches `(search text pattern 0)` with no step of
/// the machine around it: a long search must find the stop itself, in whichever phase it is.
fn searches_with_a_stop_waiting(
    g: &mut Grift<Interrupter>,
    stop: &mut Interrupter,
    args: &[Value],
) -> Result<Value, Error> {
    assert!(stop.stop(), "a run to stop");
    let (text, pattern) = (g.import(&args[0]).unwrap(), g.import(&args[1]).unwrap());
    match g.search_directly(text, pattern) {
        Err(e) => assert_eq!(kind(g, &g.export_error(e)), "interrupted"),
        Ok(v) => panic!("the search went on past a stop and answered {v:?}"),
    }
    Ok(Value::Inert)
}

/// Bytes and strings, a text the pattern nearly matches everywhere, and an empty text, where only
/// the preparation of the search reads the pattern.
#[test]
fn a_long_search_finds_a_stop_by_itself() {
    let mut pattern = "a".repeat((1 << 15) - 1);
    pattern.push('b');
    for text in ["a".repeat(1 << 16), String::new()] {
        for bytes in [true, false] {
            let mut g: Grift<Interrupter> = Grift::new().unwrap();
            define(&mut g, "walk", searches_with_a_stop_waiting);
            let (hay, wanted) = if bytes {
                (g.bytes(text.as_bytes()), g.bytes(pattern.as_bytes()))
            } else {
                (g.string(&text), g.string(&pattern))
            };
            g.define("hay", hay.unwrap()).unwrap();
            g.define("pattern", wanted.unwrap()).unwrap();
            let mut stop = g.interrupter();
            let r = g.evaluate(&mut stop, "(walk hay pattern)");
            assert_eq!(kind(&g, &r.unwrap_err()), "interrupted");
        }
    }
}

/// The search against the plain definition: every text and pattern of two letters up to a length,
/// which holds every shape of periodic pattern that short, then long periodic ones.
#[test]
fn searching_finds_what_the_plain_definition_finds() {
    let g: Grift = Grift::new().unwrap();
    let check = |text: &[u8], pattern: &[u8]| {
        let plain = match pattern.len() {
            0 => Some(0),
            n => text.windows(n).position(|w| w == pattern),
        };
        assert_eq!(
            g.search(text, pattern).unwrap(),
            plain,
            "{text:?} {pattern:?}"
        );
    };
    let word = |bits: usize, len: usize| -> Vec<u8> {
        (0..len).map(|i| b'a' + (bits >> i & 1) as u8).collect()
    };
    for len in 0..=8 {
        for pattern in (0..1 << len).map(|bits| word(bits, len)) {
            for text_len in 0..=12 {
                for text in (0..1 << text_len).map(|bits| word(bits, text_len)) {
                    check(&text, &pattern);
                }
            }
        }
    }
    let mut x: u32 = 1;
    let mut next = move |below: u32| {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        (x >> 16) % below
    };
    for _ in 0..3000 {
        let period: Vec<u8> = (0..1 + next(5)).map(|_| b'a' + next(3) as u8).collect();
        let pattern: Vec<u8> = period
            .iter()
            .cycle()
            .take(1 + next(40) as usize)
            .copied()
            .collect();
        let mut text: Vec<u8> = period
            .iter()
            .cycle()
            .take(next(300) as usize)
            .copied()
            .collect();
        if !text.is_empty() {
            let at = next(text.len() as u32) as usize;
            text[at] = b'a' + next(3) as u8;
        }
        check(&text, &pattern);
    }
}
