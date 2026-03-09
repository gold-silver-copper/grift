use crate::model::DocModel;

use super::{md_writer::MdWriter, PageSpec};

pub fn render_index(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1(&format!("{} Documentation", model.project_name));

    if let Some(intro) = model.intro_doc() {
        md.paragraph(intro);
    } else {
        md.paragraph("Generated documentation derived from the source tree.");
    }

    md.h2("Overview");
    md.paragraph(&format!(
        "This build discovered **{} value types**, **{} special forms**, **{} built-in functions**, **{} error variants**, and **{} prelude entries**.",
        model.value_enum.variants.len(),
        model.operatives.len(),
        model.applicatives.len(),
        model.arena_error.variants.len(),
        model.prelude.len(),
    ));

    md.h2("Pages");
    for linked_page in super::pages()
        .iter()
        .filter(|linked_page| linked_page.file_name != page.file_name)
    {
        md.raw(&format!(
            "- [{}]({})\n",
            linked_page.title, linked_page.file_name
        ));
    }
    md.raw("\n");

    md.finish()
}
