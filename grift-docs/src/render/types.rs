use crate::model::DocModel;

use super::{
    md_writer::{first_sentence, slugify, MdWriter, TocItem},
    PageSpec,
};

pub fn render_types(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Value Types");

    if let Some(intro) = model.file_doc.get("value") {
        md.paragraph(intro);
    } else if !model.value_enum.doc.is_empty() {
        md.paragraph(&model.value_enum.doc);
    }

    md.h2("Summary");
    let rows: Vec<Vec<String>> = model
        .value_enum
        .variants
        .iter()
        .map(|variant| {
            vec![
                format!("`{}`", variant.name),
                field_summary(variant.fields.len()),
                fallback_dash(first_sentence(&variant.doc)),
            ]
        })
        .collect();
    md.table(&["Type", "Fields", "Description"], &rows);

    let toc_items: Vec<_> = model
        .value_enum
        .variants
        .iter()
        .map(|variant| TocItem {
            title: variant.name.as_str(),
            anchor: slugify(&variant.name),
        })
        .collect();
    md.toc("Contents", &toc_items);

    for variant in &model.value_enum.variants {
        md.h3(&variant.name);
        if variant.doc.is_empty() {
            md.missing_doc_warning();
        } else {
            md.paragraph(&variant.doc);
        }

        if !variant.fields.is_empty() {
            let field_rows: Vec<Vec<String>> = variant
                .fields
                .iter()
                .map(|field| {
                    vec![
                        format!("`{}`", field.name.as_deref().unwrap_or("_")),
                        format!("`{}`", field.ty),
                        fallback_dash(first_sentence(&field.doc)),
                    ]
                })
                .collect();
            md.table(&["Field", "Type", "Description"], &field_rows);
        }

        let related = [
            ("Built-ins", "builtins.md".to_string()),
            ("Special Forms", "special-forms.md".to_string()),
            ("Errors", "errors.md".to_string()),
        ];
        md.related_links(&related);
    }

    md.finish()
}

fn field_summary(count: usize) -> String {
    match count {
        0 => "None".to_string(),
        1 => "1 field".to_string(),
        _ => format!("{count} fields"),
    }
}

fn fallback_dash(value: String) -> String {
    if value.is_empty() {
        "—".to_string()
    } else {
        value
    }
}
