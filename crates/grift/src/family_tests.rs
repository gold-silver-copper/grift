//! Every primitive, classed in one `match` with no catch-all arm, and three sweeps over the classes:
//! a stop ends each walk by itself, deep or long data needs no Rust stack, and memory refused at
//! each allocation is refused cleanly. A new primitive doesn't compile here until it is classed,
//! and once classed it is in every sweep.

#![allow(clippy::unwrap_used, clippy::panic)]

use crate::evaluate::Primitive;
use crate::{Answer, Error, Grift, Interrupter, Value, local};

/// How much work a primitive does in one step.
#[derive(Clone, Copy, PartialEq)]
enum Family {
    /// Constant work, whatever its operands.
    Fixed,
    /// One copy, check or parse of its operand, at memory speed: no item-by-item walk.
    Copies,
    /// Item by item along a list, a text or a tree: a stop must end it by itself.
    Walks,
    /// Hands work to the machine, whose loop checks for stops.
    Machine,
}

/// Each primitive's class and operands. In operands, `L` is a long list of `0`s, `T` a long text
/// of `a`s, `B` its bytes, `R` the text of a long list, `C` a context binding many names, and `D` a
/// tree: shared and wide for the stop sweep, nested deep for the stack sweep.
fn family(p: Primitive) -> (Family, &'static str) {
    use Family::*;
    use Primitive as P;
    match p {
        P::Vau => (Walks, "D #ignore"),
        P::Wrap | P::Unwrap => (Fixed, "add"),
        P::Evaluate => (Machine, "1 (context ())"),
        P::EvaluateWithCleanup => (Machine, "1 (context ()) (wrap (vau! () #ignore 0))"),
        P::EvaluateWithHandler => (Machine, "1 (context ()) (wrap (vau! (e) #ignore 0))"),
        P::Raise => (
            Walks,
            "(record (quote! (kind values trace)) (list (quote! k) L ()))",
        ),
        P::Context => (Fixed, "()"),
        P::Bind => (Walks, "(context ()) D D"),
        P::Unbind => (Fixed, "(context ()) (quote! x)"),
        P::Names => (Walks, "C"),
        P::Pair | P::Same => (Fixed, "D D"),
        P::TypeOf => (Fixed, "D"),
        P::Add
        | P::Subtract
        | P::Multiply
        | P::Divide
        | P::Remainder
        | P::LessThan
        | P::Power
        | P::BitwiseAnd
        | P::BitwiseOr
        | P::BitwiseExclusiveOr
        | P::ShiftLeft
        | P::ShiftRight => (Fixed, "6 3"),
        P::SquareRoot
        | P::Sine
        | P::Cosine
        | P::Tangent
        | P::Exponential
        | P::NaturalLogarithm
        | P::Floor
        | P::Ceiling
        | P::Round
        | P::Truncate
        | P::AbsoluteValue
        | P::FloatToInteger => (Fixed, "2.5"),
        P::IntegerToFloat | P::IntegerToString | P::CodePointToString => (Fixed, "65"),
        P::ByteAt => (Fixed, "T 0"),
        P::ByteLength => (Fixed, "T"),
        P::First | P::Rest => (Fixed, "L"),
        P::Select => (Fixed, "#true 1 2"),
        P::Lookup => (Fixed, "(context ()) (quote! x) 0"),
        P::Join => (Copies, "T T"),
        P::Slice => (Copies, "T 1 1000"),
        P::StringToSymbol | P::StringToFloat | P::StringToInteger | P::StringToBytes => {
            (Copies, "T")
        }
        P::BytesToString => (Copies, "B"),
        P::ReadForm => (Walks, "R 0"),
        P::ValueToString => (Walks, "D"),
        P::Search => (Walks, "T \"b\" 0"),
        P::ListToBytes => (Walks, "L"),
        P::BytesToList => (Walks, "B"),
    }
}

/// Definitions of the operands, with lists `n` long and trees `depth` deep.
fn operands(n: usize, deep: bool, depth: usize) -> String {
    let names: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
    let tree = if deep {
        format!("(quote! {}{})", "(".repeat(depth), ")".repeat(depth))
    } else {
        format!("(grow () {depth})")
    };
    format!(
        "(define! grow (lambda! (d n) (if! (same? n 0) d (grow (pair d d) (subtract n 1)))))
         (define! zeros (lambda! (n acc) (if! (same? n 0) acc (zeros (subtract n 1) (pair 0 acc)))))
         (define! L (zeros {n} ()))
         (define! T \"{text}\")
         (define! B (string-to-bytes T))
         (define! R \"({list})\")
         (define! C (context ()))
         (bind C (quote! ({names})) L)
         (define! D {tree})",
        text = "a".repeat(n),
        list = "0 ".repeat(n),
        names = names.join(" "),
    )
}

fn kind<T>(g: &Grift<T>, e: &Error) -> String {
    super::tests::kind(g, e)
}

struct Probe {
    stop: Interrupter,
    p: Primitive,
    /// How the primitive ended: its error's kind, or `done`.
    said: String,
}

/// Asks for a stop, then runs the primitive on its operands with no machine step between them.
fn probe(g: &mut Grift<Probe>, s: &mut Probe, args: &[Value]) -> Result<Answer, Error> {
    assert!(s.stop.stop(), "a run to stop");
    let args: Vec<local::Value> = args.iter().map(|a| g.import(a).unwrap()).collect();
    s.said = match g.call_primitive(s.p, &args, g.built_in) {
        Err(e) => kind(g, &g.export_error(e)),
        Ok(_) => "done".into(),
    };
    Ok(Answer::Now(Value::Inert))
}

#[test]
fn a_stop_ends_every_walking_primitive_by_itself() {
    let mut missed = Vec::new();
    for &p in Primitive::ALL {
        let (Family::Walks, args) = family(p) else {
            continue;
        };
        let mut g: Grift<Probe> = Grift::new().unwrap();
        let mut s = Probe {
            stop: g.interrupter(),
            p,
            said: String::new(),
        };
        g.evaluate(&mut s, &operands(1 << 16, false, 20)).unwrap();
        // A handle taken between runs reaches the next one.
        s.stop = g.interrupter();
        let f = g.host_function("probe", probe).unwrap();
        g.define("probe", f).unwrap();
        // The run itself ends `interrupted` either way, at its next step: what counts is how
        // the primitive ended.
        let _ = g.evaluate(&mut s, &format!("(probe {args})"));
        if s.said != "interrupted" {
            missed.push(format!("{}: {}", p.name(), s.said));
        }
    }
    assert!(missed.is_empty(), "went on past a stop: {missed:?}");
}

#[test]
fn deep_and_long_operands_need_no_rust_stack() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut g = Grift::new().unwrap();
            g.evaluate(&mut (), &operands(100_000, true, 100_000))
                .unwrap();
            for &p in Primitive::ALL {
                let (_, args) = family(p);
                // Any answer will do: the run must end, not overflow the stack.
                let _ = g.evaluate(&mut (), &format!("({} {args})", p.name()));
            }
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn memory_refused_at_each_allocation_is_refused_cleanly() {
    let mut g = Grift::new().unwrap();
    g.evaluate(&mut (), &operands(5, false, 3)).unwrap();
    for &p in Primitive::ALL {
        let (_, args) = family(p);
        let src = format!("({} {args})", p.name());
        let shown = |g: &Grift, r: Result<Value, Error>| match r {
            Ok(v) => Ok(super::tests::show(g, &v)),
            Err(e) => Err(kind(g, &e)),
        };
        let r = g.evaluate(&mut (), &src);
        let whole = shown(&g, r);
        for n in 0..64 {
            g.heap.refuse_in = Some(n);
            let r = g.evaluate(&mut (), &src);
            g.heap.refuse_in = None;
            let r = shown(&g, r);
            match (&r, &whole) {
                (Err(k), _) if k == "out-of-memory" => {}
                _ => assert_eq!(r, whole, "{src} with allocation {n} refused"),
            }
            assert_eq!(g.evaluate(&mut (), "(add 1 1)"), Ok(Value::Integer(2)));
        }
    }
}
