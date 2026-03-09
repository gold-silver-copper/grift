use crate::model::DocModel;

use super::md_writer::MdWriter;

pub fn render_strings(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Strings", 6);
    md.h1("String Representation");

    // CharPair variant doc
    if let Some(v) = model.value_enum.variants.iter().find(|v| v.name == "CharPair") {
        md.paragraph(&v.doc);
    }

    md.h2("String Operations");

    let methods = [
        "alloc_string",
        "builtin_cons",
        "prepend_char",
        "walk_chars",
        "strings_equal",
    ];

    for name in &methods {
        if let Some(mdoc) = model.method_index.get(*name) {
            md.h3(&format!("`{}`", mdoc.name));
            md.paragraph(&mdoc.doc);
        }
    }

    md.h2("String Builtins");

    let string_builtins = ["raw-read-string", "raw-display-to-string", "raw-write-to-string"];
    for lisp_name in &string_builtins {
        if let Some(entry) = model
            .applicatives
            .iter()
            .find(|e| e.lisp_name == *lisp_name)
        {
            md.h3(&format!("`{}`", entry.signature));
            if entry.doc.is_empty() {
                md.missing_doc_warning();
            } else {
                md.paragraph(&entry.doc);
            }
        }
    }

    md.finish()
}
