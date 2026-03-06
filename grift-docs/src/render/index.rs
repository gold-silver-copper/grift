use crate::model::DocModel;

use super::md_writer::MdWriter;

pub fn render_index(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Grift Documentation", 1);
    md.h1("Grift Documentation");

    md.paragraph("Welcome to the Grift documentation — a minimalistic \
        `no_std`, `no_alloc` Lisp interpreter built on a fixed-size arena \
        allocator, implementing Kernel-style vau calculus.");

    md.h2("Quick Start");

    md.code_block("rust", "\
use grift::{Lisp, Value};

let lisp: Lisp<20000> = Lisp::new();
let result = lisp.eval(\"(+ 1 2)\");
assert_eq!(result, Ok(Value::Number(3)));");

    md.h2("Overview");

    md.paragraph(&format!(
        "The interpreter supports **{} value types**, \
         **{} special forms** (operatives), \
         **{} built-in functions** (applicatives), \
         **{} error types**, and \
         **{} prelude entries**.",
        model.value_enum.variants.len(),
        model.operatives.len(),
        model.applicatives.len(),
        model.arena_error.variants.len(),
        model.prelude.len(),
    ));

    md.h2("Table of Contents");

    md.raw("- [Types](types.md) — Value types and their representation\n");
    md.raw("- [Special Forms](special-forms.md) — Operative special forms\n");
    md.raw("- [Builtins](builtins.md) — Built-in applicative functions\n");
    md.raw("- [Environments](environments.md) — Environment model\n");
    md.raw("- [Strings](strings.md) — String representation\n");
    md.raw("- [GC](gc.md) — Garbage collection\n");
    md.raw("- [Errors](errors.md) — Error types and handling\n");
    md.raw("- [Examples](examples.md) — Example programs\n\n");

    md.finish()
}
