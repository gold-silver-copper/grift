use crate::model::DocModel;

use super::md_writer::MdWriter;

pub fn render_environments(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Environments", 5);
    md.h1("Environments");

    // File doc from lisp.rs
    if let Some(intro) = model.file_doc.get("lisp") {
        md.paragraph(intro);
    }

    md.h2("Environment Chain");

    md.code_block("", "\
Ground Env (slot 5) — immutable, builtins only
     ↓ parent
Global Env (slot 7) — prelude + user define!s
     ↓ parent
User Env   (dynamic) — created per lambda / let");

    md.h2("Environment Methods");

    let env_methods = [
        "env_define",
        "env_set",
        "env_lookup",
        "make_env",
        "make_child_env",
    ];

    for name in &env_methods {
        if let Some(mdoc) = model.method_index.get(*name) {
            md.h3(&format!("`{}`", mdoc.name));
            md.paragraph(&mdoc.doc);
        }
    }

    md.finish()
}
