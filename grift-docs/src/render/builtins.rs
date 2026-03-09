use indexmap::IndexMap;

use crate::extract::BuiltinEntry;
use crate::model::DocModel;

use super::{
    md_writer::{find_errors_for_method, slugify, split_doc_examples, MdWriter, TocItem},
    PageSpec,
};

pub fn render_builtins(model: &DocModel, page: &PageSpec) -> String {
    let mut md = MdWriter::new();
    md.page_front_matter(page);
    md.h1("Built-in Functions");
    md.paragraph(
        "Applicatives evaluate arguments before dispatch. The grouping below is derived from the builtin names present in the source tree.",
    );

    let grouped = group_by_prefix(&model.applicatives);
    let toc_items: Vec<_> = grouped
        .values()
        .flat_map(|entries| {
            entries.iter().map(|entry| TocItem {
                title: entry.lisp_name.as_str(),
                anchor: builtin_anchor(entry),
            })
        })
        .collect();
    md.toc("Contents", &toc_items);

    for (group, entries) in grouped {
        md.h2(&group);
        for entry in entries {
            render_builtin_entry(&mut md, entry, model, "Applicative");
        }
    }

    md.finish()
}

pub(crate) fn render_builtin_entry(
    md: &mut MdWriter,
    entry: &BuiltinEntry,
    model: &DocModel,
    kind_label: &str,
) {
    md.h3(&entry.rust_method);
    md.paragraph(&format!("**Name:** `{}`", entry.lisp_name));
    md.paragraph(&format!("**Signature:** `{}`", entry.signature));
    md.paragraph(&format!("**Kind:** {kind_label}"));
    md.paragraph(&format!("**Dispatch method:** `{}`", entry.rust_method));
    md.paragraph(&format!(
        "**Tail-call optimized:** {}",
        yes_no(entry.is_tco)
    ));

    if entry.doc.is_empty() {
        md.missing_doc_warning();
    } else {
        let (prose, examples) = split_doc_examples(&entry.doc);
        md.paragraph(&prose);
        for example in examples {
            md.code_block("scheme", &example);
        }
    }

    let related_errors = find_errors_for_method(&entry.rust_method, &model.error_sites);
    if !related_errors.is_empty() {
        md.raw("#### Errors\n\n");
        for variant in &related_errors {
            md.raw(&format!("- `ArenaError::{variant}`\n"));
        }
        md.raw("\n");
    }

    let mut related = vec![("Types", "types.md".to_string())];
    if let Some(first_error) = related_errors.first() {
        related.push(("Error", format!("errors.md#{}", slugify(first_error))));
    } else {
        related.push(("Errors", "errors.md".to_string()));
    }
    related.push(("Examples", "examples.md".to_string()));
    md.related_links(&related);
}

fn group_by_prefix<'a>(entries: &'a [BuiltinEntry]) -> IndexMap<String, Vec<&'a BuiltinEntry>> {
    let mut grouped: IndexMap<String, Vec<&BuiltinEntry>> = IndexMap::new();
    let mut ordered: Vec<_> = entries.iter().collect();
    ordered.sort_by(|a, b| a.lisp_name.cmp(&b.lisp_name));

    for entry in ordered {
        let label = category_label(&entry.lisp_name);
        grouped.entry(label).or_default().push(entry);
    }

    grouped
}

fn category_label(name: &str) -> String {
    if let Some(prefix) = name
        .split(['-', '?'])
        .next()
        .filter(|prefix| !prefix.is_empty())
    {
        let mut chars = prefix.chars();
        if let Some(first) = chars.next() {
            let mut label = first.to_uppercase().collect::<String>();
            label.push_str(chars.as_str());
            return label;
        }
    }
    "Other".to_string()
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "Yes"
    } else {
        "No"
    }
}

fn builtin_anchor(entry: &BuiltinEntry) -> String {
    slugify(&entry.rust_method)
}
