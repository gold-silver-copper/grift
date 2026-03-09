use syn::visit::Visit;

/// A singleton entry from `define_singletons!`.
#[derive(Debug, Clone)]
pub struct SingletonEntry {
    pub name: String,
    pub slot: usize,
    pub doc: String,
}

/// Visitor that extracts entries from `define_singletons!`.
pub struct MacroConstExtractor {
    pub singletons: Vec<SingletonEntry>,
}

impl MacroConstExtractor {
    pub fn new() -> Self {
        Self {
            singletons: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for MacroConstExtractor {
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        let path = &node.mac.path;
        let last_seg = path.segments.last();
        if last_seg.is_none_or(|s| s.ident != "define_singletons") {
            return;
        }

        let tokens = &node.mac.tokens;
        self.singletons = parse_singletons(tokens.clone());
    }
}

fn parse_singletons(tokens: proc_macro2::TokenStream) -> Vec<SingletonEntry> {
    use proc_macro2::TokenTree;
    let mut entries = Vec::new();
    let toks: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;

    // Collect doc comments (they appear as #[doc = "..."] which syn
    // already parsed as part of the macro invocation, but in the token
    // stream they're raw tokens). We need to handle:
    //   $(#[$meta:meta])* NAME = $idx:expr ,
    // In the token stream, doc comments appear as:
    //   # [ doc = "..." ] NAME = LITERAL ,

    let mut pending_docs: Vec<String> = Vec::new();

    while i < toks.len() {
        // Check for doc attribute: # [ doc = "..." ]
        if matches!(&toks[i], TokenTree::Punct(p) if p.as_char() == '#') && i + 1 < toks.len() {
            if let TokenTree::Group(group) = &toks[i + 1] {
                let inner: Vec<TokenTree> = group.stream().into_iter().collect();
                // Look for: doc = "..."
                if inner.len() >= 3 {
                    if let TokenTree::Ident(id) = &inner[0] {
                        if id == "doc" {
                            // inner[1] should be '=', inner[2] should be literal
                            if let TokenTree::Literal(lit) = &inner[2] {
                                let raw = lit.to_string();
                                let s = raw.trim_matches('"');
                                let s = s.strip_prefix(' ').unwrap_or(s);
                                pending_docs.push(s.to_string());
                            }
                        }
                    }
                }
                i += 2;
                continue;
            }
        }

        // Check for semicolon (marks end of regular entries, start of FIRST_FREE)
        if matches!(&toks[i], TokenTree::Punct(p) if p.as_char() == ';') {
            break;
        }

        // Look for NAME = LITERAL ,
        if let TokenTree::Ident(name) = &toks[i] {
            let name_str = name.to_string();
            // Next should be '='
            if i + 1 < toks.len()
                && matches!(&toks[i + 1], TokenTree::Punct(p) if p.as_char() == '=')
                && i + 2 < toks.len()
            {
                if let TokenTree::Literal(lit) = &toks[i + 2] {
                    if let Ok(slot) = lit.to_string().parse::<usize>() {
                        let doc = pending_docs.join("\n");
                        entries.push(SingletonEntry {
                            name: name_str,
                            slot,
                            doc,
                        });
                        pending_docs.clear();
                        i += 3;
                        // Skip comma
                        if i < toks.len()
                            && matches!(&toks[i], TokenTree::Punct(p) if p.as_char() == ',')
                        {
                            i += 1;
                        }
                        continue;
                    }
                }
            }
        }

        i += 1;
    }

    entries
}
