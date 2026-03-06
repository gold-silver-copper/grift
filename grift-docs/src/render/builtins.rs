use crate::model::DocModel;

use super::md_writer::{find_errors_for_method, split_doc_examples, MdWriter};

/// Builtin group classification.
struct BuiltinGroup {
    name: &'static str,
    members: &'static [&'static str],
}

const GROUPS: &[BuiltinGroup] = &[
    BuiltinGroup {
        name: "Arithmetic",
        members: &["+", "-", "*", "/"],
    },
    BuiltinGroup {
        name: "Comparison",
        members: &["=", "<", ">", "<=", ">="],
    },
    BuiltinGroup {
        name: "Pairs & Lists",
        members: &["cons", "car", "cdr", "list", "null?", "pair?"],
    },
    BuiltinGroup {
        name: "Strings",
        members: &[
            "raw-read-string",
            "raw-display-to-string",
            "raw-write-to-string",
        ],
    },
    BuiltinGroup {
        name: "Type Predicates",
        members: &[
            "number?",
            "symbol?",
            "boolean?",
            "inert?",
            "ignore?",
            "operative?",
            "applicative?",
            "environment?",
        ],
    },
    BuiltinGroup {
        name: "Logic",
        members: &["not"],
    },
    BuiltinGroup {
        name: "Equality",
        members: &["eq?", "equal?"],
    },
    BuiltinGroup {
        name: "Combiners",
        members: &["eval", "wrap", "unwrap", "apply"],
    },
    BuiltinGroup {
        name: "Environments",
        members: &["make-environment", "make-empty-environment"],
    },
    BuiltinGroup {
        name: "GC & Control",
        members: &["gc-collect", "error"],
    },
];

pub fn render_builtins(model: &DocModel) -> String {
    let mut md = MdWriter::new();
    md.front_matter("Built-in Functions", 4);
    md.h1("Built-in Functions (Applicatives)");

    md.paragraph(
        "Applicatives evaluate their arguments before dispatch. They are \
         created by wrapping an operative with `wrap`.",
    );

    // Group the applicatives
    let mut used: Vec<bool> = vec![false; model.applicatives.len()];

    for group in GROUPS {
        let group_entries: Vec<(usize, _)> = model
            .applicatives
            .iter()
            .enumerate()
            .filter(|(_, e)| group.members.contains(&e.lisp_name.as_str()))
            .collect();

        if group_entries.is_empty() {
            continue;
        }

        md.h2(group.name);

        for (idx, entry) in &group_entries {
            used[*idx] = true;
            render_builtin_entry(&mut md, entry, model);
        }
    }

    // "Other" section for ungrouped builtins
    let other_entries: Vec<_> = model
        .applicatives
        .iter()
        .enumerate()
        .filter(|(idx, _)| !used[*idx])
        .collect();

    if !other_entries.is_empty() {
        md.h2("Other");
        for (_, entry) in &other_entries {
            render_builtin_entry(&mut md, entry, model);
        }
    }

    md.finish()
}

fn render_builtin_entry(
    md: &mut MdWriter,
    entry: &crate::extract::BuiltinEntry,
    model: &DocModel,
) {
    md.h3(&format!("`{}`", entry.signature));

    md.paragraph(
        "**Kind:** Applicative — arguments are evaluated before dispatch.",
    );

    let tco = if entry.is_tco { "Yes" } else { "No" };
    md.paragraph(&format!("**TCO:** {tco}"));

    if entry.doc.is_empty() {
        md.missing_doc_warning();
    } else {
        let (prose, examples) = split_doc_examples(&entry.doc);
        md.paragraph(&prose);

        for ex in &examples {
            md.code_block("scheme", ex);
        }

        if examples.is_empty() {
            md.code_block("scheme", &format!("({} ...)", entry.lisp_name));
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
}
