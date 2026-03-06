use crate::model::DocModel;

use super::md_writer::MdWriter;

pub fn render_examples(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Examples", 9);
    md.h1("Examples");

    md.paragraph(
        "Example programs and benchmarks from the grift source tree.",
    );

    // Prelude functions
    if !model.prelude.is_empty() {
        md.h2("Prelude Functions");
        md.paragraph(
            "The following functions are available in the standard prelude, \
             parsed on demand from static source text.",
        );

        for entry in &model.prelude {
            md.h3(&format!("`{}`", entry.lisp_name));
            if let Some(source) = &entry.source {
                md.code_block("scheme", source);
            }
        }
    }

    md.h2("Fibonacci Benchmark");
    md.code_block("scheme", "\
(fn! fib (n)
  (if (<= n 1) n
    (+ (fib (- n 1)) (fib (- n 2)))))

(fib 20)");

    md.h2("Basic Usage");
    md.code_block("scheme", "\
;; Arithmetic
(+ 1 2 3)       ; → 6
(* 2 3 4)       ; → 24

;; Lists
(define! xs (list 1 2 3))
(car xs)         ; → 1
(cdr xs)         ; → (2 3)

;; Lambda
(define! double (lambda (x) (* x 2)))
(double 21)      ; → 42

;; Higher-order functions (prelude)
(map double (list 1 2 3))  ; → (2 4 6)
(filter (lambda (x) (> x 2)) (list 1 2 3 4))  ; → (3 4)");

    md.finish()
}
