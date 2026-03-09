use std::path::{Path, PathBuf};

use anyhow::Result;
use indexmap::IndexMap;
use syn::visit::Visit;
use walkdir::WalkDir;

use crate::extract::builtin::{BuiltinEntry, BuiltinExtractor};
use crate::extract::enums::{EnumDoc, EnumExtractor};
use crate::extract::error_sites::ErrorSiteExtractor;
use crate::extract::methods::{ImplMethodExtractor, MethodDoc, MethodIndex};
use crate::extract::prelude::{PreludeEntry, PreludeExtractor};
use crate::extract::singletons::{MacroConstExtractor, SingletonEntry};
use crate::extract::SourceMap;

/// Extracted example snippet from the project tree.
#[derive(Debug, Clone)]
pub struct ExampleSnippet {
    pub title: String,
    pub language: String,
    pub source: String,
    pub summary: Option<String>,
    pub origin: PathBuf,
}

/// Assembled documentation model.
pub struct DocModel {
    pub project_name: String,
    pub value_enum: EnumDoc,
    pub arena_error: EnumDoc,
    pub error_sites: IndexMap<String, Vec<String>>,
    pub singletons: Vec<SingletonEntry>,
    pub operatives: Vec<BuiltinEntry>,
    pub applicatives: Vec<BuiltinEntry>,
    pub prelude: Vec<PreludeEntry>,
    pub method_index: MethodIndex,
    /// Module-level `//!` comments, keyed by filename stem.
    pub file_doc: IndexMap<String, String>,
    pub examples: Vec<ExampleSnippet>,
}

impl DocModel {
    pub fn build(map: &SourceMap) -> Result<Self> {
        let project_name = infer_project_name(&map.root);

        let mut enum_extractor = EnumExtractor::new(&["Value", "ArenaError"]);
        for file in map.files.values() {
            enum_extractor.visit_file(file);
        }

        let value_enum = enum_extractor
            .find("Value")
            .cloned()
            .unwrap_or_else(|| EnumDoc {
                name: "Value".to_string(),
                doc: String::new(),
                variants: Vec::new(),
            });

        let arena_error = enum_extractor
            .find("ArenaError")
            .cloned()
            .unwrap_or_else(|| EnumDoc {
                name: "ArenaError".to_string(),
                doc: String::new(),
                variants: Vec::new(),
            });

        let mut method_extractor = ImplMethodExtractor::new();
        for file in map.files.values() {
            method_extractor.visit_file(file);
        }
        let method_index = method_extractor.methods;

        let mut builtin_extractor = BuiltinExtractor::new();
        let eval_content = map
            .sources
            .iter()
            .find_map(|(path, content)| (path_stem(path) == "eval").then_some(content.clone()))
            .unwrap_or_default();
        for file in map.files.values() {
            builtin_extractor.visit_file(file);
        }
        builtin_extractor.attach_docs(&method_index, &eval_content);

        let mut singleton_extractor = MacroConstExtractor::new();
        for file in map.files.values() {
            singleton_extractor.visit_file(file);
        }

        let mut error_extractor = ErrorSiteExtractor::new();
        for file in map.files.values() {
            error_extractor.visit_file(file);
        }
        for fns in error_extractor.sites.values_mut() {
            fns.sort();
            fns.dedup();
        }

        let mut prelude_extractor = PreludeExtractor::new();
        for file in map.files.values() {
            prelude_extractor.visit_file(file);
        }
        let prelude = discover_prelude(map, prelude_extractor.entries);

        let mut file_doc = IndexMap::new();
        for (path, file) in &map.files {
            let stem = path_stem(path);
            let doc = crate::extract::extract_inner_doc(file);
            if !doc.is_empty() {
                file_doc.insert(stem, doc);
            }
        }

        let examples = discover_examples(map)?;

        Ok(DocModel {
            project_name,
            value_enum,
            arena_error,
            error_sites: error_extractor.sites,
            singletons: singleton_extractor.singletons,
            operatives: builtin_extractor.operatives,
            applicatives: builtin_extractor.applicatives,
            prelude,
            method_index,
            file_doc,
            examples,
        })
    }

    pub fn intro_doc(&self) -> Option<&str> {
        for key in ["lib", "main", "lisp", "eval"] {
            if let Some(doc) = self.file_doc.get(key) {
                return Some(doc);
            }
        }
        None
    }

    pub fn method_docs_matching(&self, predicate: impl Fn(&MethodDoc) -> bool) -> Vec<&MethodDoc> {
        self.method_index
            .values()
            .filter(|method| predicate(method))
            .collect()
    }

    pub fn builtins_matching(
        &self,
        predicate: impl Fn(&BuiltinEntry) -> bool,
    ) -> Vec<&BuiltinEntry> {
        let mut matches: Vec<_> = self
            .operatives
            .iter()
            .chain(self.applicatives.iter())
            .filter(|entry| predicate(entry))
            .collect();
        matches.sort_by(|a, b| a.lisp_name.cmp(&b.lisp_name));
        matches
    }
}

fn discover_prelude(map: &SourceMap, mut entries: Vec<PreludeEntry>) -> Vec<PreludeEntry> {
    if !entries.is_empty() {
        return entries;
    }

    for candidate in prelude_candidates(&map.root) {
        if let Ok(content) = std::fs::read_to_string(&candidate) {
            entries = crate::extract::prelude::extract_from_grift(&content);
            if !entries.is_empty() {
                break;
            }
        }
    }

    entries
}

fn prelude_candidates(root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut current = Some(root);
    while let Some(dir) = current {
        candidates.push(dir.join("prelude.grift"));
        current = dir.parent();
    }
    candidates
}

fn discover_examples(map: &SourceMap) -> Result<Vec<ExampleSnippet>> {
    let mut examples = Vec::new();
    let project_root = map.root.parent().unwrap_or(&map.root);

    let examples_dir = project_root.join("examples");
    if examples_dir.exists() {
        for entry in WalkDir::new(&examples_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            if path.extension().is_some_and(|ext| ext == "rs") {
                let content = std::fs::read_to_string(&path)?;
                examples.extend(extract_doc_examples_from_rust(&path, &content));
            }
        }
    }

    for candidate in prelude_candidates(&map.root) {
        if candidate.exists() {
            let source = std::fs::read_to_string(&candidate)?;
            examples.push(ExampleSnippet {
                title: humanize_stem(&candidate),
                language: "scheme".to_string(),
                source: source.trim().to_string(),
                summary: Some("Discovered from project `.grift` source.".to_string()),
                origin: candidate,
            });
            break;
        }
    }

    examples.sort_by(|a, b| a.title.cmp(&b.title));
    examples.dedup_by(|a, b| a.title == b.title && a.source == b.source);
    Ok(examples)
}

fn extract_doc_examples_from_rust(path: &Path, content: &str) -> Vec<ExampleSnippet> {
    let mut summary_lines = Vec::new();
    let mut snippets = Vec::new();
    let mut in_code = false;
    let mut language = String::new();
    let mut code = String::new();

    for line in content.lines() {
        let doc_line = line
            .trim_start()
            .strip_prefix("//!")
            .or_else(|| line.trim_start().strip_prefix("///"));

        let Some(doc_line) = doc_line else {
            if !in_code && !summary_lines.is_empty() {
                break;
            }
            continue;
        };

        let doc_line = doc_line.trim_start();
        if let Some(info) = doc_line.strip_prefix("```") {
            if in_code {
                snippets.push(ExampleSnippet {
                    title: humanize_stem(path),
                    language: language.clone(),
                    source: code.trim().to_string(),
                    summary: joined_summary(&summary_lines),
                    origin: path.to_path_buf(),
                });
                in_code = false;
                language.clear();
                code.clear();
            } else {
                in_code = true;
                language = info.trim().to_string();
            }
            continue;
        }

        if in_code {
            if !code.is_empty() {
                code.push('\n');
            }
            code.push_str(doc_line);
            continue;
        }

        if !doc_line.is_empty() {
            summary_lines.push(doc_line.to_string());
        }
    }

    snippets.retain(|snippet| !snippet.source.is_empty());
    snippets
}

fn joined_summary(lines: &[String]) -> Option<String> {
    let summary = lines.join(" ").trim().to_string();
    (!summary.is_empty()).then_some(summary)
}

fn infer_project_name(root: &Path) -> String {
    let candidate = root
        .file_name()
        .filter(|name| *name != "src")
        .or_else(|| root.parent().and_then(|parent| parent.file_name()));

    candidate
        .and_then(|name| name.to_str())
        .map(title_case)
        .unwrap_or_else(|| "Documentation".to_string())
}

fn humanize_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(title_case)
        .unwrap_or_else(|| path.display().to_string())
}

fn title_case(raw: &str) -> String {
    raw.split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut title = first.to_uppercase().collect::<String>();
                    title.push_str(&chars.as_str().to_lowercase());
                    title
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn path_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}
