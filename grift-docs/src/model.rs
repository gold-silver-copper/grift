use std::path::Path;

use anyhow::Result;
use indexmap::IndexMap;
use syn::visit::Visit;

use crate::extract::SourceMap;
use crate::extract::builtin::BuiltinExtractor;
use crate::extract::enums::{EnumDoc, EnumExtractor};
use crate::extract::methods::{ImplMethodExtractor, MethodIndex};
use crate::extract::singletons::{MacroConstExtractor, SingletonEntry};
use crate::extract::prelude::{PreludeExtractor, PreludeEntry};
use crate::extract::builtin::BuiltinEntry;
use crate::extract::error_sites::ErrorSiteExtractor;

/// Assembled documentation model.
pub struct DocModel {
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
}

impl DocModel {
    pub fn build(map: &SourceMap) -> Result<Self> {
        // Phase 2a: Extract enums
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

        // Phase 2c: Extract methods (do this before builtins so we can attach docs)
        let mut method_extractor = ImplMethodExtractor::new();
        for file in map.files.values() {
            method_extractor.visit_file(file);
        }
        let method_index = method_extractor.methods;

        // Phase 2b: Extract builtins
        let mut builtin_extractor = BuiltinExtractor::new();
        let mut eval_content = String::new();
        for (path, file) in &map.files {
            builtin_extractor.visit_file(file);
            if path_stem(path) == "eval" {
                eval_content = std::fs::read_to_string(path).unwrap_or_default();
            }
        }
        builtin_extractor.attach_docs(&method_index, &eval_content);

        // Phase 2d: Extract singletons
        let mut singleton_extractor = MacroConstExtractor::new();
        for file in map.files.values() {
            singleton_extractor.visit_file(file);
        }

        // Phase 2e: Extract error sites
        let mut error_extractor = ErrorSiteExtractor::new();
        for file in map.files.values() {
            error_extractor.visit_file(file);
        }
        // Deduplicate function names per variant
        for fns in error_extractor.sites.values_mut() {
            fns.sort();
            fns.dedup();
        }

        // Phase 2f: Extract prelude
        let mut prelude_extractor = PreludeExtractor::new();
        for file in map.files.values() {
            prelude_extractor.visit_file(file);
        }
        let mut prelude = prelude_extractor.entries;

        // Also try to extract from .grift files
        for (path, _) in &map.files {
            // Check for prelude.grift next to src files
            if let Some(parent) = path.parent() {
                let grift_path = parent.join("../prelude.grift");
                if grift_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&grift_path) {
                        let grift_entries = crate::extract::prelude::extract_from_grift(&content);
                        if !grift_entries.is_empty() && prelude.is_empty() {
                            prelude = grift_entries;
                        }
                    }
                }
            }
        }
        // Also check the src root for prelude.grift
        if prelude.is_empty() {
            // Walk up to find prelude.grift
            for path in map.files.keys() {
                let mut dir = path.parent();
                while let Some(d) = dir {
                    let candidate = d.join("prelude.grift");
                    if candidate.exists() {
                        if let Ok(content) = std::fs::read_to_string(&candidate) {
                            prelude = crate::extract::prelude::extract_from_grift(&content);
                            if !prelude.is_empty() {
                                break;
                            }
                        }
                    }
                    dir = d.parent();
                }
                if !prelude.is_empty() {
                    break;
                }
            }
        }

        // File-level //! comments
        let mut file_doc = IndexMap::new();
        for (path, file) in &map.files {
            let stem = path_stem(path);
            let doc = crate::extract::extract_inner_doc(file);
            if !doc.is_empty() {
                file_doc.insert(stem, doc);
            }
        }

        Ok(DocModel {
            value_enum,
            arena_error,
            error_sites: error_extractor.sites,
            singletons: singleton_extractor.singletons,
            operatives: builtin_extractor.operatives,
            applicatives: builtin_extractor.applicatives,
            prelude,
            method_index,
            file_doc,
        })
    }
}

fn path_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}
