use crate::model::DocModel;

use super::md_writer::{slugify, MdWriter, TocItem};
use super::PageSpec;

pub fn render_environments(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Environments");

    if let Some(intro) = model
        .file_doc
        .get("lisp")
        .or_else(|| model.file_doc.get("eval"))
    {
        md.paragraph(intro);
    }

    let methods = model.method_docs_matching(|method| {
        method.name.starts_with("env_")
            || method.name.contains("environment")
            || method.name.contains("_env")
    });

    if methods.is_empty() {
        md.missing_doc_warning();
    } else {
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
            let related = [
                ("Built-ins", "builtins.md".to_string()),
                ("Types", "types.md".to_string()),
                ("Errors", "errors.md".to_string()),
            ];
            md.related_links(&related);
        }
    }

    let builtins = model.builtins_matching(|entry| entry.lisp_name.contains("environment"));
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
        ("Built-ins", "builtins.md".to_string()),
        ("Special Forms", "special-forms.md".to_string()),
        ("Types", "types.md".to_string()),
    ]);

    md.finish()
}
