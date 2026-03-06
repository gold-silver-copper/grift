use crate::model::DocModel;

use super::md_writer::MdWriter;

pub fn render_gc(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Garbage Collection", 7);
    md.h1("Garbage Collection");

    // File doc from arena.rs (lib.rs for grift-arena module docs)
    if let Some(intro) = model.file_doc.get("arena") {
        md.paragraph(intro);
    } else if let Some(intro) = model.file_doc.get("lib") {
        md.paragraph(intro);
    }

    // GC_ROOTS singleton
    if let Some(gc_roots) = model.singletons.iter().find(|s| s.name == "GC_ROOTS") {
        md.h2("GC Root Stack");
        md.paragraph(&format!(
            "**Singleton slot:** `{}` (slot {})",
            gc_roots.name, gc_roots.slot
        ));
        if !gc_roots.doc.is_empty() {
            md.paragraph(&gc_roots.doc);
        }
    }

    md.h2("GC Pipeline");

    let gc_methods = [
        "push_root",
        "initialize_roots",
        "process_mark_stack",
        "sweep_unmarked",
        "pop_roots",
    ];

    for name in &gc_methods {
        if let Some(mdoc) = model.method_index.get(*name) {
            md.h3(&format!("`{}`", mdoc.name));
            md.paragraph(&mdoc.doc);
        }
    }

    // Also include collect_garbage
    if let Some(mdoc) = model.method_index.get("collect_garbage") {
        md.h3(&format!("`{}`", mdoc.name));
        md.paragraph(&mdoc.doc);
    }

    md.finish()
}
