//! Generated programs, for laws and differential checks: well-formed programs across the language
//! (arithmetic, lists, strings, contexts, operatives, recursion and tail loops, errors raised by
//! the program and by the host, handlers, cleanups, calls a host function hands back, waits),
//! from a seed, and shrinking: a failing program is cut down by putting one of a form's parts, or
//! `0`, in its place while it still fails.
//!
//! The words are the slice's natives and library with [`PRELUDE`](crate::PRELUDE), the harness's
//! [`Host`](crate::Host) functions and the laws' `note` and `cleanup-note`
//! ([`laws::setup`](crate::laws::setup)).

/// A small deterministic generator (xorshift*), so a failure names its seed.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    /// A number below `n` (0 for `n` = 0).
    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize)
            .checked_rem(n)
            .unwrap_or(0)
    }

    pub fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs.get(self.below(xs.len())).copied().unwrap_or("0")
    }
}

/// A generated program: text whose `{}` holes are its parts.
#[derive(Clone, Debug)]
pub struct Node {
    form: String,
    parts: Vec<Node>,
}

impl Node {
    pub fn leaf(text: &str) -> Node {
        Node {
            form: text.to_string(),
            parts: Vec::new(),
        }
    }

    /// `form` with its `{}` holes filled by `parts`, in order.
    pub fn of(form: String, parts: Vec<Node>) -> Node {
        Node { form, parts }
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        let mut pieces = self.form.split("{}");
        out.push_str(pieces.next().unwrap_or(""));
        for (part, piece) in self.parts.iter().zip(pieces) {
            out.push_str(&part.text());
            out.push_str(piece);
        }
        out
    }

    /// How many nodes it has, itself included.
    pub fn size(&self) -> usize {
        1 + self.parts.iter().map(Node::size).sum::<usize>()
    }

    /// The `i`th node in preorder.
    pub fn at(&mut self, mut i: usize) -> Option<&mut Node> {
        if i == 0 {
            return Some(self);
        }
        i -= 1;
        for part in &mut self.parts {
            if i < part.size() {
                return part.at(i);
            }
            i -= part.size();
        }
        None
    }

    /// The node with its `i`th node wrapped as `form`'s one part (`(stop-here {})`, say).
    pub fn wrapped_at(&self, i: usize, form: &str) -> Node {
        let mut q = self.clone();
        if let Some(target) = q.at(i) {
            *target = Node::of(form.to_string(), vec![target.clone()]);
        }
        q
    }
}

/// What a program may contain.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Whether it may wait (and park, in a run that may).
    pub waits: bool,
}

/// What the generator knows while it builds: names in scope, a counter for fresh ones.
struct Gen<'a> {
    r: &'a mut Rng,
    o: Options,
    scope: Vec<String>,
    fresh: usize,
    /// Inside a cleanup's own code, where notes say so, so a log tells what ran after a stop.
    cleaning: bool,
}

const ATOMS: &[&str] = &[
    "0",
    "1",
    "-3",
    "7",
    "40",
    "9223372036854775807",
    "\"\"",
    "\"ab\"",
    "\"h\u{e9}\"",
    "#true",
    "#false",
    "()",
    "#inert",
    "(quote! x)",
    "(quote! (y . 1))",
    "1.5",
];

impl Gen<'_> {
    fn name(&mut self, stem: &str) -> String {
        self.fresh += 1;
        format!("{stem}{}", self.fresh)
    }

    /// `f` builds with `names` in scope.
    fn with<R>(&mut self, names: &[&String], f: impl FnOnce(&mut Self) -> R) -> R {
        let before = self.scope.len();
        self.scope.extend(names.iter().map(|n| n.to_string()));
        let r = f(self);
        self.scope.truncate(before);
        r
    }

    /// The host function that logs, as cleanup code or not.
    fn note(&self) -> &'static str {
        if self.cleaning {
            "cleanup-note"
        } else {
            "note"
        }
    }

    /// The context a form is evaluated in.
    fn here(&self) -> &'static str {
        "((vau! () c c))"
    }

    fn atom(&mut self) -> Node {
        let i = self.r.below(self.scope.len() * 3);
        match self.scope.get(i) {
            Some(name) => Node::leaf(name),
            None => Node::leaf(self.r.pick(ATOMS)),
        }
    }

    /// An integer, or a name in scope.
    fn number(&mut self) -> Node {
        match self.r.below(4) {
            0 => self.atom(),
            _ => Node::leaf(self.r.pick(&["0", "1", "-3", "7", "40"])),
        }
    }

    fn expr(&mut self, depth: usize) -> Node {
        if depth == 0 || self.r.below(5) == 0 {
            return self.atom();
        }
        let n = |me: &mut Self| me.expr(depth - 1);
        match self.r.below(23) {
            0 | 1 => {
                let op = self.r.pick(&[
                    "add",
                    "subtract",
                    "multiply",
                    "less-than?",
                    "same?",
                    "pair",
                    "list",
                    "join",
                ]);
                Node::of(format!("({op} {{}} {{}})"), vec![n(self), n(self)])
            }
            2 => {
                let op = self.r.pick(&[
                    "first",
                    "rest",
                    "value-to-string",
                    "byte-length",
                    "type-of",
                    "string-to-symbol",
                ]);
                Node::of(format!("({op} {{}})"), vec![n(self)])
            }
            3 | 4 => {
                let test = match self.r.below(3) {
                    0 => n(self),
                    _ => Node::of(
                        "(less-than? {} {})".into(),
                        vec![self.number(), self.number()],
                    ),
                };
                Node::of("(if! {} {} {})".into(), vec![test, n(self), n(self)])
            }
            5 => Node::of(
                format!("(sequence! ({} {{}}) {{}})", self.note()),
                vec![n(self), n(self)],
            ),
            6 => {
                let v = self.name("v");
                let value = n(self);
                let body = self.with(&[&v], n);
                Node::of(format!("(let! (({v} {{}})) {{}})"), vec![value, body])
            }
            7 => {
                let v = self.name("v");
                let body = self.with(&[&v], n);
                Node::of(format!("((lambda! ({v}) {{}}) {{}})"), vec![body, n(self)])
            }
            8 => {
                let k = self.r.pick(&[
                    "mine",
                    "other",
                    "type-error",
                    "out-of-memory",
                    "interrupted",
                ]);
                let form = match self.r.below(2) {
                    0 => format!("(raise (make-error (quote! {k}) (list {{}})))"),
                    _ => format!("(fail (quote! {k}) {{}})"),
                };
                Node::of(form, vec![n(self)])
            }
            9 | 10 => self.catch(depth),
            11 => self.cleanup(depth),
            12 => {
                let (f, i, acc) = (self.name("f"), self.name("i"), self.name("acc"));
                let k = self.r.pick(&["0", "3", "20"]);
                let step = self.with(&[&i, &acc], n);
                Node::of(
                    format!(
                        "(let! (({f} (lambda! ({i} {acc}) (if! (less-than? {i} 1) {acc} ({f} (subtract {i} 1) {{}}))))) ({f} {k} {{}}))"
                    ),
                    vec![step, n(self)],
                )
            }
            13 => {
                let (f, i) = (self.name("f"), self.name("i"));
                let k = self.r.pick(&["0", "2", "12"]);
                let base = self.with(&[&i], n);
                Node::of(
                    format!(
                        "(let! (({f} (lambda! ({i}) (if! (less-than? {i} 1) {{}} (list {i} ({f} (subtract {i} 1))))))) ({f} {k}))"
                    ),
                    vec![base],
                )
            }
            14 => {
                let v = self.name("v");
                let body = self.with(&[&v], n);
                Node::of(
                    format!("(map (lambda! ({v}) {{}}) (list {{}} {{}}))"),
                    vec![body, n(self), n(self)],
                )
            }
            15 => Node::of(
                format!("(evaluate (quote! {{}}) {})", self.here()),
                vec![n(self)],
            ),
            16 => {
                let (v, env) = (self.name("v"), self.name("env"));
                Node::of(
                    format!("((vau! ({v}) {env} (evaluate {v} {env})) {{}})"),
                    vec![n(self)],
                )
            }
            17 => {
                let (v, w) = (self.name("v"), self.name("w"));
                let body = self.with(&[&v, &w], n);
                let form = format!("((lambda! ({v} {w}) {{}}) {{}} {{}})");
                Node::of(form, vec![body, n(self), n(self)])
            }
            18 => {
                let v = self.name("v");
                let body = self.with(&[&v], n);
                let form = match self.r.below(2) {
                    0 => format!("(call-back (lambda! ({v}) {{}}) {{}})"),
                    _ => format!("(call-then (lambda! ({v}) {{}}) {{}} (lambda! (r) (echo r)))"),
                };
                Node::of(form, vec![body, n(self)])
            }
            19 => match self.r.below(5) {
                0 => Node::of(
                    "(first (read-form (value-to-string {}) 0))".into(),
                    vec![n(self)],
                ),
                1 => Node::of(
                    "(slice \"abc\u{3bb}d\" {} {})".into(),
                    vec![self.number(), self.number()],
                ),
                2 => Node::of("(search {} \"b\" 0)".into(), vec![n(self)]),
                3 => {
                    let c = self.name("c");
                    let here = self.here();
                    Node::of(
                        format!(
                            "(let! (({c} (context {here}))) (sequence! (bind {c} (quote! (x . y)) {{}}) (list (names {c}) (lookup {c} (quote! y) 0) (evaluate (quote! x) {c}))))"
                        ),
                        vec![n(self)],
                    )
                }
                _ => Node::of(
                    "(string-to-symbol (value-to-string {}))".into(),
                    vec![n(self)],
                ),
            },
            _ if self.o.waits => Node::of("(wait {})".into(), vec![n(self)]),
            _ => Node::of(format!("({} {{}})", self.note()), vec![n(self)]),
        }
    }

    /// A handler around a part: its value, or what the handler makes of the error.
    fn catch(&mut self, depth: usize) -> Node {
        let e = self.name("e");
        let body = self.expr(depth - 1);
        let field = |f: &str| format!("(lookup {e} (quote! {f}) ())");
        let handler = match self.r.below(4) {
            0 => Node::leaf(&field("values")),
            1 => Node::leaf(&format!("(list {} {})", field("kind"), field("values"))),
            2 => Node::leaf(&format!("(raise {e})")),
            _ => self.expr(depth - 1),
        };
        // The draw keeps every seed's program as it was when another vocabulary used it.
        let _ = self.r.below(3);
        let form = format!(
            "(evaluate-with-handler (quote! {{}}) {} (lambda! ({e}) {{}}))",
            self.here()
        );
        Node::of(form, vec![body, handler])
    }

    /// A cleanup around a part, which notes it was entered, so that a cleanup owed can be told
    /// from one never installed. The cleanup notes itself, and may raise, wait or run code of
    /// its own.
    fn cleanup(&mut self, depth: usize) -> Node {
        let c = self.name("cleanup");
        let body = self.expr(depth - 1);
        let mut parts = vec![body];
        let after = match self.r.below(5) {
            0 => format!(" (raise (make-error (quote! {c}) ()))"),
            1 if self.o.waits => " (wait 0)".to_string(),
            2 => {
                let cleaning = std::mem::replace(&mut self.cleaning, true);
                parts.push(self.expr(depth - 1));
                self.cleaning = cleaning;
                " {}".to_string()
            }
            _ => String::new(),
        };
        let (entered, note, plain) = (c.replace("cleanup", "entered"), self.note(), "note");
        let form = format!(
            "(evaluate-with-cleanup (quote! (sequence! ({note} (quote! {entered})) {{}})) {} (lambda! () ({plain} (quote! {c})){after}))",
            self.here()
        );
        Node::of(form, parts)
    }
}

/// A program for `seed`.
pub fn program(seed: u64, o: Options) -> Node {
    let mut r = Rng::new(seed);
    let depth = 2 + r.below(4);
    Gen {
        r: &mut r,
        o,
        scope: Vec::new(),
        fresh: 0,
        cleaning: false,
    }
    .expr(depth)
}

/// The text of every form the generator writes in `o`'s words, for a check that each appears.
pub fn forms(o: Options) -> Vec<&'static str> {
    let mut all = vec![
        "(add ",
        "(first ",
        "(if! ",
        "(sequence! (",
        "(let! ((v",
        "((lambda! (v",
        "(raise ",
        "(fail ",
        "acc",
        "(list i",
        "(map (lambda!",
        "(evaluate (quote!",
        "((vau! (v",
        "(call-back",
        "(read-form",
        "(slice",
        "(search",
        "(names",
    ];
    all.extend([
        "(evaluate-with-handler ",
        "(evaluate-with-cleanup ",
        "(call-then",
    ]);
    if o.waits {
        all.push("(wait ");
    }
    all
}

/// `law` on `p`, a panic in it a failure too, so that it shrinks as well.
pub fn guarded(law: &impl Fn(&Node) -> Result<(), String>, p: &Node) -> Result<(), String> {
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| law(p)));
    caught.unwrap_or_else(|panic| {
        let text = panic.downcast_ref::<String>().cloned();
        let text = text.or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()));
        Err(format!("panicked: {}", text.unwrap_or_default()))
    })
}

/// Shrinking, a little: put a part of a node, or `0`, in its place while the law still fails.
pub fn shrink(
    mut p: Node,
    mut why: String,
    law: impl Fn(&Node) -> Result<(), String>,
) -> (Node, String) {
    let mut tries = 0;
    'again: while tries < 500 {
        for i in 0..p.size() {
            let Some(node) = p.clone().at(i).map(|n| n.clone()) else {
                continue;
            };
            let mut smaller = node.parts.clone();
            if node.form != "0" {
                smaller.push(Node::leaf("0"));
            }
            for s in smaller {
                tries += 1;
                let mut q = p.clone();
                if let Some(at) = q.at(i) {
                    *at = s;
                }
                if let Err(w) = guarded(&law, &q) {
                    (p, why) = (q, w);
                    continue 'again;
                }
            }
        }
        break;
    }
    (p, why)
}

/// Runs `law` on the programs of seeds `first..first + cases`; a failure is the seed, the smallest
/// program found that still fails, and why.
pub fn check(
    name: &str,
    first: u64,
    cases: u64,
    o: Options,
    law: impl Fn(&Node, u64) -> Result<(), String>,
) -> Result<(), String> {
    for seed in first..first + cases {
        let p = program(seed, o);
        if let Err(why) = guarded(&|p: &Node| law(p, seed), &p) {
            let (small, why) = shrink(p, why, |p| law(p, seed));
            return Err(format!(
                "law {name} fails for seed {seed}:\n{}\n{why}",
                small.text()
            ));
        }
    }
    Ok(())
}
