mod extract;
mod model;
mod render;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use model::DocModel;

/// Static-analysis documentation generator for the grift Lisp interpreter.
#[derive(Parser)]
struct Args {
    /// Path to the grift source tree (e.g. `./crates/grift`).
    #[arg(long)]
    src: PathBuf,

    /// Output directory for Markdown files.
    #[arg(long)]
    out: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let map = extract::SourceMap::load(&args.src)?;
    let model = DocModel::build(&map)?;
    std::fs::create_dir_all(&args.out)?;

    type PageFn = fn(&DocModel) -> String;
    let pages: &[(&str, PageFn)] = &[
        ("index.md", render::render_index),
        ("types.md", render::render_types),
        ("special-forms.md", render::render_special_forms),
        ("builtins.md", render::render_builtins),
        ("environments.md", render::render_environments),
        ("strings.md", render::render_strings),
        ("gc.md", render::render_gc),
        ("errors.md", render::render_errors),
        ("examples.md", render::render_examples),
    ];

    for (name, render_fn) in pages {
        std::fs::write(args.out.join(name), render_fn(&model))?;
        println!("  wrote {name}");
    }

    println!(
        "\n{} value types, {} operatives, {} applicatives, \
         {} errors, {} prelude entries",
        model.value_enum.variants.len(),
        model.operatives.len(),
        model.applicatives.len(),
        model.arena_error.variants.len(),
        model.prelude.len(),
    );
    Ok(())
}
