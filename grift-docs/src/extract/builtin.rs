use syn::visit::Visit;

use super::methods::MethodIndex;

/// Kind of builtin combiner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    Operative,
    Applicative,
}

/// A single builtin entry extracted from `define_builtins!`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BuiltinEntry {
    pub lisp_name: String,
    pub kind: BuiltinKind,
    pub rust_method: String,
    pub doc: String,
    pub signature: String,
    pub is_tco: bool,
}

/// Visitor that finds `define_builtins!` macro calls and extracts entries.
pub struct BuiltinExtractor {
    pub operatives: Vec<BuiltinEntry>,
    pub applicatives: Vec<BuiltinEntry>,
}

impl BuiltinExtractor {
    pub fn new() -> Self {
        Self {
            operatives: Vec::new(),
            applicatives: Vec::new(),
        }
    }

    /// After extraction, attach docs from the MethodIndex and detect TCO.
    pub fn attach_docs(&mut self, methods: &MethodIndex, file_contents: &str) {
        for entry in self
            .operatives
            .iter_mut()
            .chain(self.applicatives.iter_mut())
        {
            if let Some(mdoc) = methods.get(&entry.rust_method) {
                entry.doc = mdoc.doc.clone();
                // Extract signature from first backtick span in doc
                entry.signature = extract_signature(&mdoc.doc, &entry.lisp_name);
            }
            // Default signature if not extracted from docs
            if entry.signature.is_empty() {
                entry.signature = format!("({} ...)", entry.lisp_name);
            }
            // Detect TCO: check if method body contains tail_continue! or TailAction::Continue
            entry.is_tco = detect_tco(&entry.rust_method, file_contents);
        }
    }
}

fn extract_signature(doc: &str, lisp_name: &str) -> String {
    // Look for `(name ...)` pattern in the doc
    if let Some(start) = doc.find('`') {
        if let Some(end) = doc[start + 1..].find('`') {
            let sig = &doc[start + 1..start + 1 + end];
            if sig.starts_with('(') {
                return sig.to_string();
            }
        }
    }
    format!("({lisp_name} ...)")
}

fn detect_tco(method_name: &str, file_contents: &str) -> bool {
    // Look for the method definition with a word boundary check
    // (followed by a non-alphanumeric character like '(' or whitespace)
    let search = format!("fn {method_name}(");
    let search_alt = format!("fn {method_name} ");
    let pos = file_contents
        .find(&search)
        .or_else(|| file_contents.find(&search_alt));
    if let Some(pos) = pos {
        let rest = &file_contents[pos..];
        // Look within a reasonable range (next 2000 chars)
        let range = &rest[..rest.len().min(2000)];
        return range.contains("tail_continue!") || range.contains("TailAction::Continue");
    }
    false
}

impl<'ast> Visit<'ast> for BuiltinExtractor {
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        let path = &node.mac.path;
        let last_seg = path.segments.last();
        if last_seg.is_none_or(|s| s.ident != "define_builtins") {
            return;
        }

        let tokens = &node.mac.tokens;
        if let Ok(parsed) = parse_define_builtins(tokens.clone()) {
            self.operatives = parsed.0;
            self.applicatives = parsed.1;
        }
    }
}

fn parse_define_builtins(
    tokens: proc_macro2::TokenStream,
) -> syn::Result<(Vec<BuiltinEntry>, Vec<BuiltinEntry>)> {
    use proc_macro2::TokenTree;

    let mut iter = tokens.into_iter().peekable();
    let mut operatives = Vec::new();
    let mut applicatives = Vec::new();

    // Parse: operatives { ... } applicatives { ... }
    while let Some(tt) = iter.next() {
        if let TokenTree::Ident(ident) = &tt {
            let kind = if ident == "operatives" {
                BuiltinKind::Operative
            } else if ident == "applicatives" {
                BuiltinKind::Applicative
            } else {
                continue;
            };

            // Next should be a Group (braces)
            if let Some(TokenTree::Group(group)) = iter.next() {
                let entries = parse_entries(group.stream(), kind);
                match kind {
                    BuiltinKind::Operative => operatives = entries,
                    BuiltinKind::Applicative => applicatives = entries,
                }
            }
        }
    }

    Ok((operatives, applicatives))
}

fn parse_entries(tokens: proc_macro2::TokenStream, kind: BuiltinKind) -> Vec<BuiltinEntry> {
    use proc_macro2::TokenTree;

    let mut entries = Vec::new();
    let toks: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;

    while i < toks.len() {
        // Expect: "name" => CONST => method ,
        // `=>` is two puncts: `=` `>`
        // So: LitStr = > Ident = > Ident ,
        //     i      i1 i2 i3  i4 i5 i6  i7
        if let TokenTree::Literal(lit) = &toks[i] {
            let raw = lit.to_string();
            let lisp_name = raw.trim_matches('"').to_string();

            let mut rust_method = String::new();

            // i+1 = '=', i+2 = '>', i+3 = CONST (unused but skipped)
            // i+4 = '=', i+5 = '>', i+6 = method
            if i + 6 < toks.len() {
                if let TokenTree::Ident(id) = &toks[i + 6] {
                    rust_method = id.to_string();
                }
            }

            entries.push(BuiltinEntry {
                lisp_name,
                kind,
                rust_method,
                doc: String::new(),
                signature: String::new(),
                is_tco: false,
            });

            i += 7; // skip: name = > const = > method
                    // Skip comma if present
            if i < toks.len() {
                if let TokenTree::Punct(p) = &toks[i] {
                    if p.as_char() == ',' {
                        i += 1;
                    }
                }
            }
        } else {
            i += 1;
        }
    }

    entries
}
