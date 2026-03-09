use crate::model::DocModel;

use super::md_writer::{first_sentence, slugify, MdWriter, TocItem};
use super::PageSpec;

pub fn render_errors(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Error Types");

    if !model.arena_error.doc.is_empty() {
        md.paragraph(&model.arena_error.doc);
    }

    // Summary table
    md.h2("Summary");

    let mut rows = Vec::new();
    for v in &model.arena_error.variants {
        let desc = first_sentence(&v.doc);
        let raised_in = model
            .error_sites
            .get(&v.name)
            .map(|fns| {
                fns.iter()
                    .take(5)
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "—".to_string());

        rows.push(vec![
            format!("`{}`", v.name),
            if desc.is_empty() {
                "—".to_string()
            } else {
                desc
            },
            raised_in,
        ]);
    }

    md.table(&["Variant", "Raised when", "Raised in"], &rows);
    let toc_items: Vec<_> = model
        .arena_error
        .variants
        .iter()
        .map(|variant| TocItem {
            title: variant.name.as_str(),
            anchor: slugify(&variant.name),
        })
        .collect();
    md.toc("Contents", &toc_items);

    for v in &model.arena_error.variants {
        md.h3(&v.name);

        if v.doc.is_empty() {
            md.missing_doc_warning();
        } else {
            md.paragraph(&v.doc);
        }

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

        if let Some(fns) = model.error_sites.get(&v.name) {
            md.raw("**Raised in:**\n\n");
            for f in fns {
                md.raw(&format!("- `{f}`\n"));
            }
            md.raw("\n");
        } else {
            md.blockquote("No call-sites found for this error variant.");
        }

        let related = [
            ("Built-ins", "builtins.md".to_string()),
            ("Special Forms", "special-forms.md".to_string()),
            ("Types", "types.md".to_string()),
        ];
        md.related_links(&related);
    }

    md.finish()
}
