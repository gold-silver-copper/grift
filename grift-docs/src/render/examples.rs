use crate::model::DocModel;

use super::md_writer::{path_label, slugify, MdWriter, TocItem};
use super::PageSpec;

pub fn render_examples(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Examples");
    md.paragraph("Example content discovered from source files and project examples.");

    let mut toc_items = Vec::new();
    for entry in &model.prelude {
        toc_items.push(TocItem {
            title: entry.lisp_name.as_str(),
            anchor: slugify(&entry.lisp_name),
        });
    }
    for example in &model.examples {
        toc_items.push(TocItem {
            title: example.title.as_str(),
            anchor: slugify(&example.title),
        });
    }
    md.toc("Contents", &toc_items);

    if !model.prelude.is_empty() {
        md.h2("Prelude Entries");
        for entry in &model.prelude {
            md.h3(&entry.lisp_name);
            if let Some(source) = &entry.source {
                md.code_block("scheme", source);
            }
            md.related_links(&[
                ("Built-ins", "builtins.md".to_string()),
                ("Special Forms", "special-forms.md".to_string()),
            ]);
        }
    }

    if !model.examples.is_empty() {
        md.h2("Discovered Example Files");
        for example in &model.examples {
            md.h3(&example.title);
            md.paragraph(&format!("**Source:** `{}`", path_label(&example.origin)));
            if let Some(summary) = &example.summary {
                md.paragraph(summary);
            }
            md.code_block(example.language.as_str(), &example.source);
            md.related_links(&[
                ("Built-ins", "builtins.md".to_string()),
                ("Types", "types.md".to_string()),
                ("Errors", "errors.md".to_string()),
            ]);
        }
    }

    md.finish()
}
