//! The unit tests, which take the machine's and the heap's insides.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod ending;
mod family;
mod parked;

use crate::evaluate::{NATIVES, Native, Primitive};
use crate::heap::Bindings;
use crate::{Answer, Error, Grift, HostCall, Interrupter, Operative, OperativeKind, Value, local};

/// The grift definitions of the native primitives.
const MEANINGS: &str = include_str!("../natives.grift");

/// Whether primitive `p` is native, its meaning its definition in `natives.grift`, and what a
/// generated call of it passes: one signature for each kind of operands it takes, each argument a
/// letter of `argument`'s. Every other primitive is core, what grift can't express, and the
/// generator leaves it alone; `natives_are_their_definitions` checks the class against the
/// definitions, so a native left out here fails it.
fn class(p: Primitive) -> (bool, &'static [&'static str]) {
    use Primitive as P;
    match p {
        P::First | P::Rest => (true, &["p"]),
        P::Select => (true, &["bxx"]),
        P::Lookup => (true, &["cyx"]),
        P::BitwiseAnd | P::BitwiseOr | P::BitwiseExclusiveOr => (true, &["ii"]),
        P::ShiftLeft | P::ShiftRight => (true, &["in"]),
        P::IntegerToString | P::IntegerToFloat | P::CodePointToString => (true, &["i"]),
        P::StringToInteger | P::StringToBytes => (true, &["s"]),
        P::FloatToInteger | P::Truncate | P::Floor | P::Ceiling | P::Round | P::SquareRoot => {
            (true, &["f"])
        }
        P::AbsoluteValue => (true, &["i", "f"]),
        P::Remainder | P::Power => (true, &["ii", "ff"]),
        P::LessThan => (true, &["ii", "ff", "ss", "BB"]),
        P::ByteLength => (true, &["s", "B"]),
        P::Slice => (true, &["soo", "Boo"]),
        P::Search => (true, &["sto"]),
        P::BytesToString | P::BytesToList => (true, &["B"]),
        P::ListToBytes => (true, &["l"]),
        // Core, called on bytes values, as the definitions call them.
        P::ByteAt => (false, &["Bo"]),
        P::Join => (false, &["BB"]),
        // Core, listed so a new primitive is classed here or this doesn't compile.
        P::Vau
        | P::Wrap
        | P::Unwrap
        | P::Evaluate
        | P::EvaluateWithCleanup
        | P::EvaluateWithHandler
        | P::Raise
        | P::Context
        | P::Bind
        | P::Unbind
        | P::Names
        | P::Pair
        | P::Same
        | P::TypeOf
        | P::Add
        | P::Subtract
        | P::Multiply
        | P::Divide
        | P::Sine
        | P::Cosine
        | P::Tangent
        | P::Exponential
        | P::NaturalLogarithm
        | P::StringToSymbol
        | P::StringToFloat
        | P::ReadForm
        | P::ValueToString => (false, &[]),
    }
}

impl<T> Grift<T> {
    /// From now on the natives are their grift definitions: each bound in the built-in context,
    /// where the library looks it up, wrapped if the native is, and no library form runs a native
    /// twin. As the library's own code, what this loads is skipped by traces.
    pub(crate) fn run_definitions(&mut self) {
        let here = (self.heap.make((Some(self.built_in), Bindings::default()))).unwrap();
        self.run_text(None, here, None, MEANINGS, true, None)
            .unwrap();
        let name = self.heap.intern("meanings").unwrap();
        let Some(local::Value::Context(meanings)) = self.find(here, name).unwrap() else {
            panic!("natives.grift defines meanings");
        };
        for name in self.names_of(meanings).unwrap() {
            let meaning = self.find(meanings, name).unwrap().unwrap();
            let native = self.find(self.built_in, name).unwrap();
            let bound = match native {
                Some(local::Value::Applicative(_)) => {
                    local::Value::Applicative(self.heap.make(meaning).unwrap())
                }
                _ => meaning,
            };
            self.heap.bind(self.built_in, name, bound).unwrap();
        }
        for (_, name) in NATIVES {
            let name = self.heap.intern(name).unwrap();
            let Some(local::Value::Operative(form)) = self.find(self.built_in, name).unwrap()
            else {
                panic!("the library defines each twin as an operative");
            };
            self.heap.install(form, None).unwrap();
        }
    }

    /// The combiner under `v` if it is an applicative, else `v`.
    fn underlying(&self, v: &Value) -> Value {
        match self.import(v).unwrap() {
            local::Value::Applicative(r) => self.export(*self.heap.read(r).unwrap()),
            _ => *v,
        }
    }

    /// Whether `v` is the definition this interpreter runs for primitive `p`.
    fn is_definition_of(&self, v: &Value, p: Primitive) -> bool {
        let Some(found) = self
            .heap
            .names()
            .get(p.name())
            .copied()
            .map(local::Symbol)
            .and_then(|name| self.find(self.built_in, name).unwrap())
        else {
            return false;
        };
        let definition = match found {
            local::Value::Applicative(r) => *self.heap.read(r).unwrap(),
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
    // A new error starts at its raise. A program reads a trace newest first.
    let trace = g
        .evaluate(
            &mut (),
            "(catch! (catch! (first 1) e (raise e)) e (evaluate-in! trace e))",
        )
        .unwrap();
    assert_eq!(show(&g, &trace), "((raise e) (first 1))");
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
    // Every not-a-number is one value: which bits a machine's arithmetic leaves in one (x86_64's
    // `(divide 0.0 0.0)` has its sign set, aarch64's hasn't) is not something a program sees.
    let same = g.evaluate(&mut (), "same?").unwrap();
    for bits in [
        0x7ff0_0000_0000_0001,
        0xfff8_0000_0000_0000,
        0x7ff8_0000_0000_0011,
    ] {
        let other = Value::Float(f64::from_bits(bits));
        let answer = g.call(&mut (), &same, &[other, Value::Float(f64::NAN)]);
        assert!(matches!(answer, Ok(Value::Boolean(true))), "{bits:#x}");
    }
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
        ],
    );
    fails(
        &mut g,
        &[
            ("1e", "parse-error"),
            ("1.2.3", "parse-error"),
            ("#nan", "parse-error"),
            ("(string-to-float \"one\")", "parse-error"),
            ("(string-to-float 1.0)", "type-error"),
        ],
    );
}

#[test]
fn a_token_with_two_signs_is_a_symbol() {
    let mut g = grift();
    for token in ["--5", "+-5", "-+1.5", "++1"] {
        let forms = g.read(token).unwrap();
        assert_eq!(show(&g, &forms), format!("({token})"));
    }
}

/// What evaluating `src` does: machine steps, collections, and references and symbols compared.
/// Counts, so they say the same on a loaded machine.
fn work(g: &mut Grift, src: &str) -> (Result<Value, Error>, u64, usize, u64) {
    (g.heap.probe.steps, g.heap.probe.collections) = (0, 0);
    local::COMPARED.with(|n| n.set(0));
    let value = g.evaluate(&mut (), src);
    let compared = local::COMPARED.with(std::cell::Cell::get);
    (
        value,
        g.heap.probe.steps,
        g.heap.probe.collections,
        compared,
    )
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
fn a_catch_and_raise_at_every_level_copies_no_trace() {
    // 3,000 levels, each a combination of its own, each catching the error and raising it again:
    // the trace each handler gets is the last one's with the places since on its front. A copy at
    // each level would make 4.5 million pairs, a collection every few thousand.
    let mut g = grift();
    let mut src = String::from("(first 1)");
    for _ in 0..3000 {
        src = format!("(catch! {src} e (raise e))");
    }
    let (e, _, collections, _) = work(&mut g, &src);
    let e = e.unwrap_err();
    assert_eq!(e.trace.len(), 3002);
    assert_eq!(show(&g, &e.trace[0]), "(first 1)");
    assert!(collections < 20, "{collections} collections");
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
            "(same? (string-to-lowercase \"ÉCLAIR AZb1 [@]\") \"Éclair azb1 [@]\")",
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
    // A context's bindings, as they grow, though binding allocates nothing else.
    let c = g.evaluate(&mut (), "(record () ())").unwrap();
    let names: Vec<Value> = (0..20_000)
        .map(|i| Value::Symbol(g.symbol(&format!("n{i}")).unwrap()))
        .collect();
    g.heap.not_due();
    names
        .iter()
        .for_each(|n| g.bind(&c, n, Value::Inert).unwrap());
    assert!(g.heap.due(), "a context");
}

/// Collection walks a constant factor of what was allocated, deep in a stack of frames that make
/// little and after a host kept and released many values: `Heap::collect` checks each one. The
/// generator's programs run neither so deep nor beside so many kept values. And the next run
/// after a dropped peak gives its slots back, which the generator's runs, ended by collections
/// asked for, can't tell.
#[test]
fn collection_pays_for_what_it_walks() {
    // Kept values and contexts' bindings, which every collection walks, count too.
    let mut g = grift();
    g.heap.request_collection();
    g.evaluate(&mut (), "#inert").unwrap();
    let kept: Vec<_> = (0..200_000)
        .map(|_| g.keep(Value::Inert).unwrap())
        .collect();
    g.evaluate(&mut (), "(length (range 0 100000))").unwrap();
    kept.into_iter().for_each(|k| g.release(k).unwrap());
    let mut g = grift();
    let names: Vec<Value> = (0..300)
        .map(|i| Value::Symbol(g.symbol(&format!("n{i}")).unwrap()))
        .collect();
    let contexts: Vec<_> = (0..300)
        .map(|_| {
            let c = g.evaluate(&mut (), "(record () ())").unwrap();
            names
                .iter()
                .for_each(|n| g.bind(&c, n, Value::Inert).unwrap());
            g.keep(c).unwrap()
        })
        .collect();
    for _ in 0..3 {
        g.evaluate(&mut (), "(length (range 0 100000))").unwrap();
    }
    contexts
        .into_iter()
        .for_each(|k| assert!(g.release(k).is_ok(), "released"));
    let mut g = grift();
    for src in [
        "(define! big (range 0 100000))",
        "(define! big 0)",
        "(length (range 0 50000))",
    ] {
        g.evaluate(&mut (), src).unwrap();
    }
    g.evaluate(&mut (), "#inert").unwrap();
    assert!(g.heap.len() < 20_000, "kept {} slots", g.heap.len());
    let deep = "(define! spin (lambda! (n) (if! (same? n 0) 0 (sequence! (pair 1 2) (spin (subtract n 1))))))
        (define! c (current-context)) (define! d 0)
        (define! body (quote! (if! (same? d 50000) (spin 100000)
          (add 1 (sequence! (bind c (quote! d) (add d 1)) (evaluate body c))))))
        (evaluate body c)";
    g.evaluate(&mut (), deep).unwrap();
    // The stacks give back room as collections come, and a cleanup still starts: its call goes
    // into the slot its popped Cleanup frame left. A deep stack, gone when the cleanup is
    // installed, and a collection just before the cleanup starts.
    define(&mut g, "collect-always", |g, _, _| {
        g.0.heap.probe.stress = true;
        Ok(crate::Answer::Now(Value::Inert))
    });
    let cleanup = "(define! down (lambda! (n) (if! (same? n 0) 0 (add 1 (down (subtract n 1))))))
        (sequence! (down 30000) (evaluate-with-cleanup (quote! (collect-always)) (current-context) (lambda! () 0)))";
    g.evaluate(&mut (), cleanup).unwrap();
    g.heap.probe.stress = false;
    let kept: Vec<_> = (0..50_000).map(|_| g.keep(Value::Inert).unwrap()).collect();
    kept.into_iter().for_each(|k| g.release(k).unwrap());
    g.evaluate(&mut (), "(spin 100000)").unwrap();
}

/// Every collection walks the host-function table, so a run that allocates little beside a large
/// one collects as rarely as the table says: here each collection's walk of it is paid for.
#[test]
fn collection_pays_for_the_host_functions_it_walks() {
    let mut g = grift();
    let hosts = 100_000;
    for _ in 0..hosts {
        g.host_function("h", |_, _, _| Ok(crate::Answer::Now(Value::Inert)))
            .unwrap();
    }
    let count = "(define! down (lambda! (n) (if! (same? n 0) 0 (down (subtract n 1)))))";
    g.evaluate(&mut (), count).unwrap();
    let (collections, allocated) = (g.heap.probe.collections, g.heap.probe.work.0);
    g.evaluate(&mut (), "(down 100000)").unwrap();
    let collections = g.heap.probe.collections - collections;
    let allocated = g.heap.probe.work.0 - allocated;
    assert!(
        collections * hosts <= 8 * allocated,
        "{collections} collections walked {hosts} host functions each for {allocated} allocated"
    );
}

#[test]
fn sort_is_ordered_and_stable() {
    // 600 pairs (i mod 10 . i) sorted by key: stable, so each key's i stay rising. A merge sort
    // takes 980,000 steps here, sorting and checking, and a quadratic insertion sort 9.9 million.
    let (same, steps, _, _) = work(
        &mut grift(),
        "(equal? (sort (map (lambda! (i) (pair (remainder i 10) i)) (range 0 600))
                       (lambda! (a b) (less-than? (first a) (first b))))
                 (map (lambda! (j) (pair (divide j 60) (add (multiply (remainder j 60) 10) (divide j 60))))
                      (range 0 600)))",
    );
    assert_eq!(same, Ok(Value::Boolean(true)));
    assert!(steps < 2_000_000, "{steps} steps");
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

/// What the generator calls, with what each of its arguments usually is: `i` an integer, `f` a
/// float, `s` a string, `t` a part of one, `o` an offset, `n` a shift amount, `c` a context, `y` a
/// symbol, `p` a pair, `b` a test, `B` a bytes value, `l` a list of bytes, `x` anything. The
/// primitives' come from [`class`], the twins' from [`NATIVES`].
static CALLED: std::sync::LazyLock<Vec<(&str, &str)>> = std::sync::LazyLock::new(|| {
    let primitives = Primitive::ALL
        .iter()
        .flat_map(|&p| class(p).1.iter().map(move |k| (p.name(), *k)));
    primitives
        .chain(NATIVES.map(|(n, name)| match n {
            Native::If => (name, "bxx"),
            Native::Sequence => (name, "xxx"),
        }))
        .collect()
});

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
    let (name, kinds) = CALLED[r.below(CALLED.len())];
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
    for p in Primitive::ALL.iter().filter(|&&p| class(p).0) {
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
        let mut todo = vec![(*a, *b)];
        while let Some((a, b)) = todo.pop() {
            let differ = || Err(format!("{} against {}", show(l, &a), show(r, &b)));
            match (&a, &b) {
                // As `same?` sees them: every not-a-number is one.
                (Value::Float(x), Value::Float(y))
                    if x.to_bits() != y.to_bits() && !(x.is_nan() && y.is_nan()) =>
                {
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
                        None => self.matched.push((a, b)),
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
fn note(
    g: &mut HostCall<Vec<String>>,
    log: &mut Vec<String>,
    args: &[Value],
) -> Result<Answer, Error> {
    log.push(
        args.iter()
            .map(|v| show(g, v))
            .collect::<Vec<_>>()
            .join(" "),
    );
    Ok(Answer::Now(args.first().cloned().unwrap_or(Value::Inert)))
}

/// What a host might hand back wrongly: an object's reference in another kind's variant.
fn malformed(
    g: &mut HostCall<Vec<String>>,
    _: &mut Vec<String>,
    args: &[Value],
) -> Result<Answer, Error> {
    let string = g.string("not a context")?;
    let pair = g.pair(Value::Integer(1), Value::Integer(2))?;
    let (Value::String(s), Value::Pair(p)) = (string, pair) else {
        return Err(g.error("not-made", &[]));
    };
    Ok(Answer::Now(match args.first() {
        Some(Value::Integer(0)) => Value::Context(s),
        Some(Value::Integer(1)) => Value::String(p),
        Some(Value::Integer(3)) => Value::Bytes(s),
        _ => Value::Pair(s),
    }))
}

/// Two interpreters alike, one running the natives and one their grift definitions.
fn twins() -> [Grift<Vec<String>>; 2] {
    [true, false].map(|natives| {
        let mut g: Grift<Vec<String>> = Grift::new().unwrap();
        // A memory limit far over what any case holds: a collector that stops keeping up runs
        // out here, in seconds, instead of growing until the machine does.
        g.heap.probe.slot_limit = Some(1 << 18);
        if !natives {
            g.run_definitions();
        }
        for (name, f) in [
            ("note", note as crate::HostFunction<Vec<String>>),
            ("malformed", malformed),
        ] {
            define(&mut g, name, f);
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
        assert_eq!(same_as_native, !class(p).0, "{}", p.name());
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
    // Floats from Rust, with any bits, not-a-number's payloads included, through Rust calls: a
    // payload is the machine's (aarch64's `trunc` quietens a signalling one, x86_64's doesn't).
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
            ("float-to-integer", vec![f]),
            ("select", vec![Value::Boolean(true), f, Value::Integer(0)]),
            (
                "(wrap if!)",
                vec![Value::Boolean(false), Value::Integer(0), f],
            ),
            ("(wrap sequence!)", vec![Value::Integer(0), f]),
            ("integer-to-float", vec![f]),
            ("truncate", vec![f]),
            ("remainder", vec![f, Value::Float(1.5)]),
            ("square-root", vec![f]),
            ("absolute-value", vec![f]),
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
        cases.push(format!("(search \"ab\" \"\" {start})"));
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
    // Rounding at halves, just under them and at 2^52, the edges of not-a-number and zero's sign,
    // and the operands that aren't floats.
    let floats = [
        "2.5",
        "-2.5",
        "0.49999999999999994",
        "-0.49999999999999994",
        "4503599627370495.5",
        "-4503599627370495.5",
        "4503599627370497.0",
        "-0.3",
        "5e-324",
        "-5e-324",
        "(divide 0.0 0.0)",
        "(subtract 0.0 (divide 0.0 0.0))",
        "2.0",
        "1.7976931348623157e308",
        "1",
        "\"a\"",
    ];
    for f in FLOAT_TEXTS.iter().chain(&floats) {
        for p in ["truncate", "floor", "ceiling", "round", "square-root"] {
            cases.push(format!("({p} {f})"));
        }
    }
    for p in [
        "truncate",
        "floor",
        "ceiling",
        "round",
        "remainder",
        "absolute-value",
        "power",
        "square-root",
        "less-than?",
    ] {
        cases.push(format!("({p})"));
        cases.push(format!("({p} 1.5 2.5 3.5)"));
    }
    for x in
        INTEGER_TEXTS
            .iter()
            .chain(&["-1.5", "7.25", "#not-a-number", "\"a\"", "1e300", "-0.0"])
    {
        cases.push(format!("(absolute-value {x})"));
        for y in INTEGER_TEXTS.iter().chain(&[
            "0.0",
            "-2.0",
            "#infinity",
            "(quote! y)",
            "5e-324",
            "-0.75",
        ]) {
            cases.push(format!("(remainder {x} {y})"));
            cases.push(format!("(power {x} {y})"));
        }
    }
    let texts = [
        "\"\"",
        "\"a\"",
        "\"ab\"",
        "\"\\xe9;\"",
        "#bytes(97)",
        "#bytes(255)",
        "(malformed 3)",
        "1",
    ];
    for x in texts {
        for y in texts {
            cases.push(format!("(less-than? {x} {y})"));
        }
    }
    cases.extend([
        // A handler's values are made where the handler is called, wherever the error was: the
        // native raises in the program, its twin in the library, and the values, evaluated as
        // code, are traced alike.
        "((wrap list) (catch-kind! (first 1) type-error e (evaluate-in! values e)))".into(),
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

/// Asks for a stop of the run in progress, then searches `(search text pattern 0)` with no step of
/// the machine around it: a long search must find the stop itself, in whichever phase it is.
fn searches_with_a_stop_waiting(
    g: &mut HostCall<Interrupter>,
    stop: &mut Interrupter,
    args: &[Value],
) -> Result<Answer, Error> {
    assert!(stop.stop(), "a run to stop");
    let (text, pattern) = (g.import(&args[0]).unwrap(), g.import(&args[1]).unwrap());
    match g.0.search_directly(text, pattern) {
        Err(e) => assert_eq!(kind(g, &g.export_error(e)), "interrupted"),
        Ok(v) => panic!("the search went on past a stop and answered {v:?}"),
    }
    Ok(Answer::Now(Value::Inert))
}

/// Bytes and strings: a long text a short pattern nearly matches everywhere, where the search is
/// all comparison, and an empty text, where only the preparation reads the long pattern.
#[test]
fn a_long_search_finds_a_stop_by_itself() {
    let pattern = format!("{}b", "a".repeat((1 << 15) - 1));
    for (text, pattern) in [("a".repeat(1 << 16), "ab".into()), (String::new(), pattern)] {
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
/// Asks for a stop of the run in progress, then writes the value it was given: the printer must
/// find the stop by what it has written, however few items that took.
fn writes_with_a_stop_waiting(
    g: &mut HostCall<Interrupter>,
    stop: &mut Interrupter,
    args: &[Value],
) -> Result<Answer, Error> {
    assert!(stop.stop(), "a run to stop");
    let Err(e) = g.write(&args[0], &mut String::new()) else {
        panic!("printing went on past a stop");
    };
    assert_eq!(kind(g, &e), "interrupted");
    Ok(Answer::Now(Value::Inert))
}

/// A few items can be a great deal of text: 64 references to one 64 KiB string, or one 64 KiB
/// bytes value, which prints as one item.
#[test]
fn printing_finds_a_stop_by_the_bytes_it_writes() {
    for value in [
        "(map (lambda! (n) big) (range 0 64))",
        "(string-to-bytes big)",
    ] {
        let mut g: Grift<Interrupter> = Grift::new().unwrap();
        define(&mut g, "walk", writes_with_a_stop_waiting);
        let mut stop = g.interrupter();
        let src = format!("(define! big (string-repeat \"x\" 65536)) (walk {value})");
        let r = g.evaluate(&mut stop, &src);
        assert_eq!(kind(&g, &r.unwrap_err()), "interrupted", "{value}");
    }
}

fn report(g: &Grift, e: &Error, bytes: usize) -> (crate::ReportStatus, String) {
    let mut text = crate::TextBuffer::new();
    let status = g.describe(e, &mut text, bytes).unwrap();
    (status, text.as_str().to_owned())
}

/// A report is never longer than its budget, and a cut one still begins with the kind and the
/// innermost place, each in at most a quarter of it, whatever the program made long: its nesting,
/// its kind, its source's name, or a value that shares itself. Each line writes the line before's
/// combination as `...`, so each piece of source is printed once, and only grift starts a line.
#[test]
fn a_report_fits_its_budget_and_begins_with_kind_and_place() {
    use crate::ReportStatus::{Complete, Truncated};
    let mut g = grift();
    let top = g.top_level();
    let e = g
        .evaluate_in(&mut (), &top, "keys.grift", "(define! a 1)\n(first a)")
        .unwrap_err();
    let complete = (
        Complete,
        "type-error at keys.grift:2 1 in (first a)".to_owned(),
    );
    assert_eq!(report(&g, &e, 4096), complete);
    let nested = |depth| format!("{}(first 1){}", "(add 1 ".repeat(depth), ")".repeat(depth));
    let e = g
        .evaluate_in(&mut (), &top, "deep.grift", &nested(300))
        .unwrap_err();
    let (status, text) = report(&g, &e, 1 << 20);
    assert_eq!((status, text.matches("(add 1 ").count()), (Complete, 300));
    assert!(text.starts_with("type-error at deep.grift:1 1 in (first 1)\n  from (add 1 ...) at"));
    let long_kind = format!(
        "(raise (error (string-to-symbol \"{}\") 1))",
        "k".repeat(5000)
    );
    let long_source = "s".repeat(5000);
    g.evaluate(&mut (), "(define! x 1)").unwrap();
    for _ in 0..40 {
        g.evaluate(&mut (), "(define! x (pair x x))").unwrap();
    }
    for (source, program, begins) in [
        (
            "deep.grift",
            nested(2000),
            "type-error at deep.grift:2 1 in (first 1)\n",
        ),
        ("long.grift", long_kind, "kkk"),
        (
            long_source.as_str(),
            "(first 1)".to_owned(),
            "type-error at sss",
        ),
        (
            "shared.grift",
            "(raise (error (quote! mine) x))".to_owned(),
            "mine at shared.grift:2 (((",
        ),
    ] {
        let e = g
            .evaluate_in(&mut (), &top, source, &format!("\n{program}"))
            .unwrap_err();
        let (status, text) = report(&g, &e, 4096);
        let place = text.find(" at ").unwrap();
        assert!(
            status == Truncated
                && text.len() <= 4096
                && text.starts_with(begins)
                && place <= 1024
                && text[place..].contains(":2"),
            "{status:?} {} bytes: {:?}",
            text.len(),
            &text[..text.len().min(80)]
        );
    }
    let e = g
        .evaluate(
            &mut (),
            "(raise (error (string-to-symbol \"a\\nb\") (quote! c)))",
        )
        .unwrap_err();
    let (_, text) = report(&g, &e, 4096);
    assert_eq!(text.lines().count(), 1, "{text:?}");
    // A name is as wide as its escaped text: whole when that fits its quarter, else cut so the
    // place still begins within it. Below 30 bytes the kind comes first, cut only by the budget.
    for n in [25, 30] {
        let kind = "\\x1;".repeat(n);
        let raise =
            format!("\n(raise (error (string-to-symbol \"{kind}\") (string-repeat \"v\" 999)))");
        let e = g
            .evaluate_in(&mut (), &top, "keys.grift", &raise)
            .unwrap_err();
        for bytes in [20, 400] {
            let (_, text) = report(&g, &e, bytes);
            let at = text.find(" at keys.grift:2").unwrap_or(bytes);
            let fits = match bytes {
                ..30 => !text.is_empty() && kind.starts_with(&text),
                _ if kind.len() <= bytes / 4 => text.starts_with(&kind) && at == kind.len(),
                _ => at <= bytes / 4 && text[..at].ends_with("..."),
            };
            assert!(fits && text.len() <= bytes, "{n} in {bytes}: {text:?}");
        }
    }
}

/// A sink that takes everything and keeps nothing.
struct Discard;

impl std::fmt::Write for Discard {
    fn write_str(&mut self, _: &str) -> std::fmt::Result {
        Ok(())
    }
}

/// Forces its run, then describes into no room at all an error whose values and trace a report
/// must check before it writes: the checking is work like any other, which the force ends.
fn checks_with_a_force_waiting(
    g: &mut HostCall<()>,
    _: &mut (),
    _: &[Value],
) -> Result<Answer, Error> {
    let mut e = g.error("example", &vec![Value::Inert; 8_192]);
    e.trace = vec![Value::Inert; 8_192];
    assert!(g.interrupter().force());
    let failure = g.describe(&e, &mut Discard, 0).unwrap_err();
    assert_eq!(
        (kind(g, &failure).as_str(), failure.limit),
        ("interrupted", true)
    );
    Ok(Answer::Now(Value::Inert))
}

#[test]
fn a_report_checking_many_values_finds_a_force() {
    let mut g = grift();
    define(&mut g, "report", checks_with_a_force_waiting);
    let e = g.evaluate(&mut (), "(report)").unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
}

#[test]
fn a_name_nothing_holds_is_collected_and_the_host_s_symbol_of_it_goes_stale() {
    let mut g: Grift = Grift::new().unwrap();
    let held = g.symbol("only-the-host-holds-this").unwrap();
    g.evaluate(&mut (), "(define! bound-name 42)").unwrap();
    let bound = g.symbol("bound-name").unwrap();
    let before = g.heap.names().len();
    g.evaluate(
        &mut (),
        "(define! dead (lambda! (n) (if! (same? n 0) #inert (dead-step n))))
         (define! dead-step (lambda! (n) (string-to-symbol (join \"dead-\" (integer-to-string n))) (dead (subtract n 1))))
         (dead 20000)",
    )
    .unwrap();
    g.heap.request_collection();
    g.evaluate(&mut (), "0").unwrap();
    // Only the two definitions' names and `n` were added; the 20,000 and the host's are gone.
    assert!(
        g.heap.names().len() <= before + 3,
        "{} {before}",
        g.heap.names().len()
    );
    assert!(
        g.heap.names().capacity() < 4096,
        "{}",
        g.heap.names().capacity()
    );
    let e = g.symbol_name(&held).unwrap_err();
    assert_eq!(kind(&g, &e), "stale-value");
    let again = g.symbol("only-the-host-holds-this").unwrap();
    assert_ne!(again, held);
    assert_eq!(g.symbol_name(&again).unwrap(), "only-the-host-holds-this");
    let top = g.top_level();
    let e = g.lookup(&top, &held).unwrap_err();
    assert_eq!(kind(&g, &e), "stale-value");
    // A bound name keeps its identity through the collection.
    assert_eq!(g.symbol("bound-name").unwrap(), bound);
    assert_eq!(g.lookup(&top, &bound).unwrap(), Some(Value::Integer(42)));
    // An error's kind made as the program runs is held by the error alone while a cleanup runs.
    g.heap.probe.stress = true;
    let e = g
        .evaluate(
            &mut (),
            "(finally! (raise (error (string-to-symbol (join \"pending-\" \"kind\")))) (list 1 2 3))",
        )
        .unwrap_err();
    g.heap.probe.stress = false;
    assert_eq!(g.symbol_name(&e.kind).unwrap(), "pending-kind");
}

/// The reader makes a list's pairs as the list closes, and they are steps of its walk as the bytes
/// are: in a text too short for its bytes alone to reach a check, they reach one, and the stop ends
/// the read.
#[test]
fn a_stop_ends_the_pairs_of_a_list_being_read() {
    fn reads(g: &mut HostCall<()>, _: &mut (), _: &[Value]) -> Result<Answer, Error> {
        const ITEMS: usize = crate::WALK_CHECK * 2 / 5;
        assert!(g.interrupter().stop(), "a run to stop");
        let read = g.read(&format!("({})", "a ".repeat(ITEMS)));
        assert_eq!(g.symbol_name(&read.unwrap_err().kind)?, "interrupted");
        Ok(Answer::Now(Value::Inert))
    }
    let mut g = Grift::new().unwrap();
    define(&mut g, "reads", reads);
    let _ = g.evaluate(&mut (), "(reads)");
}
