mod builtins;
mod environments;
mod errors;
mod examples;
mod gc;
mod index;
mod md_writer;
mod special_forms;
mod strings;
mod types;

use crate::model::DocModel;

pub struct PageSpec {
    pub file_name: &'static str,
    pub title: &'static str,
    pub order: u32,
    pub render: fn(&DocModel, &PageSpec) -> String,
}

const PAGES: &[PageSpec] = &[
    PageSpec {
        file_name: "index.md",
        title: "Overview",
        order: 1,
        render: index::render_index,
    },
    PageSpec {
        file_name: "types.md",
        title: "Types",
        order: 2,
        render: types::render_types,
    },
    PageSpec {
        file_name: "special-forms.md",
        title: "Special Forms",
        order: 3,
        render: special_forms::render_special_forms,
    },
    PageSpec {
        file_name: "builtins.md",
        title: "Built-in Functions",
        order: 4,
        render: builtins::render_builtins,
    },
    PageSpec {
        file_name: "environments.md",
        title: "Environments",
        order: 5,
        render: environments::render_environments,
    },
    PageSpec {
        file_name: "strings.md",
        title: "Strings",
        order: 6,
        render: strings::render_strings,
    },
    PageSpec {
        file_name: "gc.md",
        title: "Garbage Collection",
        order: 7,
        render: gc::render_gc,
    },
    PageSpec {
        file_name: "errors.md",
        title: "Error Types",
        order: 8,
        render: errors::render_errors,
    },
    PageSpec {
        file_name: "examples.md",
        title: "Examples",
        order: 9,
        render: examples::render_examples,
    },
];

pub fn pages() -> &'static [PageSpec] {
    PAGES
}
