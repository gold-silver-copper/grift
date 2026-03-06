use crate::model::DocModel;

use super::md_writer::MdWriter;

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

    // Error cross-references
    let related_errors = find_errors_for_method(&entry.rust_method, model);
    if !related_errors.is_empty() {
        md.raw("#### Errors\n\n");
        for variant in &related_errors {
            md.raw(&format!("- `ArenaError::{variant}`\n"));
        }
        md.raw("\n");
    }
}

fn find_errors_for_method(method: &str, model: &DocModel) -> Vec<String> {
    let mut variants = Vec::new();
    for (variant, fns) in &model.error_sites {
        if fns.iter().any(|f| f == method) {
            variants.push(variant.clone());
        }
    }
    variants
}

fn split_doc_examples(doc: &str) -> (String, Vec<String>) {
    let mut prose = String::new();
    let mut examples = Vec::new();
    let mut in_code = false;
    let mut current_example = String::new();
    let mut is_scheme = false;

    for line in doc.lines() {
        if line.starts_with("```") {
            if in_code {
                if is_scheme {
                    examples.push(current_example.clone());
                }
                current_example.clear();
                in_code = false;
                is_scheme = false;
            } else {
                in_code = true;
                is_scheme = line.contains("scheme") || line.contains("lisp");
            }
        } else if in_code {
            if !current_example.is_empty() {
                current_example.push('\n');
            }
            current_example.push_str(line);
        } else {
            if !prose.is_empty() {
                prose.push('\n');
            }
            prose.push_str(line);
        }
    }

    (prose, examples)
}
