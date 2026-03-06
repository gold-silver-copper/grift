use crate::model::DocModel;

use super::md_writer::{find_errors_for_method, split_doc_examples, MdWriter};

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
            let (prose, examples) = split_doc_examples(&entry.doc);
            md.paragraph(&prose);

            for ex in &examples {
                md.code_block("scheme", ex);
            }

            if examples.is_empty() {
                md.code_block("scheme", &format!("({} ...)", entry.lisp_name));
            }
        }

        let related_errors = find_errors_for_method(&entry.rust_method, &model.error_sites);
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
