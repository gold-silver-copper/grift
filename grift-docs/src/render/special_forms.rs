use crate::model::DocModel;

use super::md_writer::MdWriter;

pub fn render_special_forms(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Special Forms", 3);
    md.h1("Special Forms (Operatives)");

    md.paragraph(
        "Operatives receive their arguments **unevaluated**, along with \
         the caller's dynamic environment. They are the fundamental \
         combiner type in Kernel-style vau calculus.",
    );

    for entry in &model.operatives {
        md.h3(&format!("`{}`", entry.signature));

        md.paragraph("**Kind:** Operative — arguments are NOT evaluated before dispatch.");

        let tco = if entry.is_tco { "Yes" } else { "No" };
        md.paragraph(&format!("**TCO:** {tco}"));

        if entry.doc.is_empty() {
            md.missing_doc_warning();
        } else {
            // Re-emit doc, extracting any ```scheme blocks
            let (prose, examples) = split_doc_examples(&entry.doc);
            md.paragraph(&prose);

            for ex in &examples {
                md.code_block("scheme", ex);
            }

            if examples.is_empty() {
                // Synthesize minimal example
                md.code_block("scheme", &format!("({} ...)", entry.lisp_name));
            }
        }

        // Error cross-references
        if let Some(errors) = model.error_sites.get(&entry.rust_method) {
            let _ = errors; // checked via find_errors_for_method below
        }
        let related_errors = find_errors_for_method(&entry.rust_method, model);
        if !related_errors.is_empty() {
            md.raw("#### Errors\n\n");
            for variant in &related_errors {
                md.raw(&format!("- `ArenaError::{variant}`\n"));
            }
            md.raw("\n");
        }
    }

    md.finish()
}

fn find_errors_for_method(method: &str, model: &DocModel) -> Vec<String> {
    let mut variants = Vec::new();
    for (variant, fns) in &model.error_sites {
        if fns.iter().any(|f| f == method) {
            variants.push(variant.clone());
        }
    }
    variants
}

/// Split doc string into prose and code examples.
fn split_doc_examples(doc: &str) -> (String, Vec<String>) {
    let mut prose = String::new();
    let mut examples = Vec::new();
    let mut in_code = false;
    let mut current_example = String::new();
    let mut is_scheme = false;

    for line in doc.lines() {
        if line.starts_with("```") {
            if in_code {
                if is_scheme {
                    examples.push(current_example.clone());
                }
                current_example.clear();
                in_code = false;
                is_scheme = false;
            } else {
                in_code = true;
                is_scheme = line.contains("scheme") || line.contains("lisp");
            }
        } else if in_code {
            if !current_example.is_empty() {
                current_example.push('\n');
            }
            current_example.push_str(line);
        } else {
            if !prose.is_empty() {
                prose.push('\n');
            }
            prose.push_str(line);
        }
    }

    (prose, examples)
}
