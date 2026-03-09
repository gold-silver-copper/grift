use crate::model::DocModel;

use super::{
    builtins::render_builtin_entry,
    md_writer::{slugify, MdWriter, TocItem},
    PageSpec,
};

pub fn render_special_forms(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Special Forms");
    md.paragraph(
        "Operatives receive unevaluated arguments. Entries and signatures are derived from the builtin declarations in source.",
    );

    let toc_items: Vec<_> = model
        .operatives
        .iter()
        .map(|entry| TocItem {
            title: entry.lisp_name.as_str(),
            anchor: slugify(&entry.rust_method),
        })
        .collect();
    md.toc("Contents", &toc_items);

    for entry in &model.operatives {
        render_builtin_entry(&mut md, entry, model, "Operative");
    }

    md.finish()
}
