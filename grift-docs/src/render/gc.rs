use crate::model::DocModel;

use super::md_writer::{slugify, MdWriter, TocItem};
use super::PageSpec;

pub fn render_gc(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Garbage Collection");

    if let Some(intro) = model
        .file_doc
        .get("arena")
        .or_else(|| model.file_doc.get("eval"))
        .or_else(|| model.file_doc.get("lib"))
    {
        md.paragraph(intro);
    }

    let gc_singletons: Vec<_> = model
        .singletons
        .iter()
        .filter(|singleton| singleton.name.to_lowercase().contains("gc"))
        .collect();
    if !gc_singletons.is_empty() {
        md.h2("Singletons");
        for singleton in gc_singletons {
            md.h3(&format!("`{}`", singleton.name));
            md.paragraph(&format!("Slot `{}`", singleton.slot));
            if !singleton.doc.is_empty() {
                md.paragraph(&singleton.doc);
            }
        }
    }

    let methods = model.method_docs_matching(|method| {
        let doc = method.doc.to_lowercase();
        method.name.contains("root")
            || method.name.contains("garbage")
            || method.name.contains("mark")
            || method.name.contains("sweep")
            || doc.contains("garbage collection")
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
                ("Errors", "errors.md".to_string()),
                ("Types", "types.md".to_string()),
                ("Examples", "examples.md".to_string()),
            ]);
        }
    }

    md.related_links(&[
        ("Errors", "errors.md".to_string()),
        ("Types", "types.md".to_string()),
        ("Examples", "examples.md".to_string()),
    ]);

    md.finish()
}
