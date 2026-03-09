mod extract;
mod model;
mod render;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use model::DocModel;

/// Static-analysis documentation generator for the grift Lisp interpreter.
#[derive(Parser)]
struct Args {
    /// Output directory for Markdown files. Defaults to `../docs/grift`.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let src = grift_source_dir()?;
    let out = output_dir(args.out)?;
    let map = extract::SourceMap::load(&src)?;
    let model = DocModel::build(&map)?;
    std::fs::create_dir_all(&out)?;

    for page in render::pages() {
        std::fs::write(out.join(page.file_name), (page.render)(&model, page))?;
        println!("  wrote {}", page.file_name);
    }

    println!(
        "\n{}: {} value types, {} operatives, {} applicatives, \
         {} errors, {} prelude entries",
        model.project_name,
        model.value_enum.variants.len(),
        model.operatives.len(),
        model.applicatives.len(),
        model.arena_error.variants.len(),
        model.prelude.len(),
    );
    Ok(())
}

fn grift_source_dir() -> Result<PathBuf> {
    let docs_dir = manifest_dir();
    let repo_root = docs_dir
        .parent()
        .context("grift-docs crate is expected to live directly under the repo root")?;
    let src = repo_root.join("crates/grift/src");
    anyhow::ensure!(
        src.is_dir(),
        "expected Grift source directory at {}",
        src.display()
    );
    Ok(src)
}

fn output_dir(cli_out: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = cli_out {
        return Ok(path);
    }

    let docs_dir = manifest_dir();
    let repo_root = docs_dir
        .parent()
        .context("grift-docs crate is expected to live directly under the repo root")?;
    Ok(repo_root.join("docs/grift"))
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
