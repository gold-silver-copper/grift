use crate::model::DocModel;

use super::md_writer::{slugify, MdWriter, TocItem};
use super::PageSpec;

pub fn render_strings(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Strings");

    if let Some(variant) = model
        .value_enum
        .variants
        .iter()
        .find(|variant| variant.name == "CharPair")
    {
        md.paragraph(&variant.doc);
    }

    let methods = model.method_docs_matching(|method| {
        method.name.contains("string")
            || method.name.contains("char")
            || method.doc.to_lowercase().contains("string")
    });
    if !methods.is_empty() {
        md.h2("Methods");
        let toc_items: Vec<_> = methods
            .iter()
            .map(|method| TocItem {
                title: method.name.as_str(),
                anchor: slugify(&method.name),
            })
            .collect();
        md.toc("Contents", &toc_items);
        for method in methods {
            md.h3(&method.name);
            md.paragraph(&method.doc);
            md.related_links(&[
                ("Types", "types.md#charpair".to_string()),
                ("Built-ins", "builtins.md".to_string()),
                ("Examples", "examples.md".to_string()),
            ]);
        }
    }

    let builtins = model.builtins_matching(|entry| entry.lisp_name.contains("string"));
    if !builtins.is_empty() {
        md.h2("Builtins");
        for builtin in builtins {
            md.h3(&builtin.rust_method);
            md.paragraph(&format!("**Name:** `{}`", builtin.lisp_name));
            md.paragraph(&format!("**Signature:** `{}`", builtin.signature));
            md.paragraph(&builtin.doc);
        }
    }

    md.related_links(&[
        ("Types", "types.md#charpair".to_string()),
        ("Built-ins", "builtins.md".to_string()),
        ("Examples", "examples.md".to_string()),
    ]);

    md.finish()
}
