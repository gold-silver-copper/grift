use crate::model::DocModel;

use super::md_writer::{first_sentence, MdWriter};

pub fn render_types(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Types", 2);
    md.h1("Value Types");

    // Intro from file doc
    if let Some(intro) = model.file_doc.get("value") {
        md.paragraph(intro);
    }

    // Summary table
    md.h2("Summary");

    let mut rows = Vec::new();
    for v in &model.value_enum.variants {
        let self_eval = if v.name != "Symbol" && v.name != "Cons" {
            "Yes"
        } else {
            "No"
        };

        let predicate = match v.name.as_str() {
            "Nil" => "`null?`",
            "Boolean" => "`boolean?`",
            "Number" => "`number?`",
            "Symbol" => "`symbol?`",
            "Cons" | "CharPair" => "`pair?`",
            "Environment" => "`environment?`",
            "Operative" | "Builtin" => "`operative?`",
            "Applicative" | "Prelude" | "Native" => "`applicative?`",
            "Inert" => "`inert?`",
            "Ignore" => "`ignore?`",
            _ => "—",
        };

        let written_as = match v.name.as_str() {
            "Nil" => "`()`",
            "Boolean" => "`#t` / `#f`",
            "Number" => "`42`",
            "Symbol" => "`hello`",
            "Cons" => "`(a . b)`",
            "CharPair" => "`\"hello\"`",
            "Operative" => "`(vau ...)`",
            "Applicative" => "`(wrap ...)`",
            "Builtin" => "`<builtin>`",
            "Environment" => "`<environment>`",
            "Inert" => "`#inert`",
            "Ignore" => "`#ignore`",
            "Prelude" => "`<prelude:name>`",
            "Native" => "`<native>`",
            _ => "—",
        };

        let desc = first_sentence(&v.doc);

        rows.push(vec![
            format!("`{}`", v.name),
            written_as.to_string(),
            self_eval.to_string(),
            predicate.to_string(),
            if desc.is_empty() {
                "—".to_string()
            } else {
                desc
            },
        ]);
    }

    md.table(
        &["Type", "Written as", "Self-evaluating", "Predicate", "Description"],
        &rows,
    );

    // Per-variant sections
    for v in &model.value_enum.variants {
        md.h3(&format!("`{}`", v.name));

        if v.doc.is_empty() {
            md.missing_doc_warning();
        } else {
            md.paragraph(&v.doc);
        }

        // Field table for named fields
        let named_fields: Vec<_> = v.fields.iter().filter(|f| f.name.is_some()).collect();
        if !named_fields.is_empty() {
            let field_rows: Vec<Vec<String>> = named_fields
                .iter()
                .map(|f| {
                    vec![
                        format!("`{}`", f.name.as_deref().unwrap_or("_")),
                        format!("`{}`", f.ty),
                        if f.doc.is_empty() {
                            "—".to_string()
                        } else {
                            first_sentence(&f.doc)
                        },
                    ]
                })
                .collect();
            md.table(&["Field", "Type", "Description"], &field_rows);
        }

        // Singleton cross-reference
        let singleton_name = match v.name.as_str() {
            "Nil" => Some("NIL"),
            "Boolean" => Some("TRUE"),
            "Inert" => Some("INERT"),
            "Ignore" => Some("IGNORE"),
            _ => None,
        };
        if let Some(sname) = singleton_name {
            if let Some(s) = model.singletons.iter().find(|s| s.name == sname) {
                md.paragraph(&format!(
                    "**Singleton slot:** `{}` (slot {})",
                    s.name, s.slot
                ));
            }
        }

        // Synthesized example
        let example = match v.name.as_str() {
            "Nil" => Some("`()`"),
            "Boolean" => Some("`#t`"),
            "Number" => Some("`42`"),
            "Symbol" => Some("`hello`"),
            "Cons" => Some("`(cons 1 2)`"),
            "CharPair" => Some("`\"hi\"`"),
            "Operative" => Some("`(vau (x) #ignore x)`"),
            "Applicative" => Some("`(wrap (vau (x) #ignore x))`"),
            "Environment" => Some("`(current-environment)`"),
            "Inert" => Some("`(define! x 1) ; → #inert`"),
            "Ignore" => Some("`(vau (#ignore) e #inert)`"),
            "Builtin" => {
                if !model.operatives.is_empty() {
                    None // handled below
                } else {
                    Some("`<builtin>`")
                }
            }
            "Prelude" => None, // handled below
            "Native" => None,
            _ => None,
        };

        if v.name == "Builtin" {
            if let Some(first) = model.operatives.first() {
                md.paragraph(&format!("**Example:** `{}`", first.lisp_name));
            }
        } else if v.name == "Prelude" {
            if let Some(first) = model.prelude.first() {
                md.paragraph(&format!("**Example:** `{}`", first.lisp_name));
            }
        } else if v.name == "Native" {
            md.paragraph("**Note:** Registered via `register_native` from Rust code.");
        } else if let Some(ex) = example {
            md.paragraph(&format!("**Example:** {ex}"));
        }
    }

    md.finish()
}
