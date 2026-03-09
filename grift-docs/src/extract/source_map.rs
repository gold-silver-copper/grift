use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use walkdir::WalkDir;

/// All parsed source files, keyed by path relative to the source root.
pub struct SourceMap {
    pub files: IndexMap<PathBuf, syn::File>,
}

impl SourceMap {
    /// Walk `src_dir` recursively, parse every `*.rs` file with `syn`.
    pub fn load(src_dir: &Path) -> Result<Self> {
        let mut files = IndexMap::new();
        for entry in WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().is_some_and(|ext| ext == "rs")
            })
        {
            let path = entry.path().to_owned();
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            match syn::parse_file(&content) {
                Ok(parsed) => {
                    files.insert(path, parsed);
                }
                Err(e) => {
                    eprintln!("  warning: failed to parse {}: {e}", entry.path().display());
                }
            }
        }
        Ok(SourceMap { files })
    }
}
