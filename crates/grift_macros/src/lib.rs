//! Procedural macros for grift_parser standard library
//!
//! This crate provides the `include_stdlib!` macro which parses a `.scm` file
//! containing function definitions and transforms them into the format expected
//! by the `define_stdlib!` macro.
//!
//! # Example
//!
//! Given a file `stdlib.scm`:
//! ```lisp
//! ;;; (map f lst) - Apply f to each element of lst
//! (define (map f lst) (if (null? lst) '() (cons (f (car lst)) (map f (cdr lst)))))
//! ```
//!
//! The macro:
//! ```text
//! include_stdlib!("stdlib.scm");
//! ```
//!
//! Expands to:
//! ```text
//! define_stdlib! {
//!     /// (map f lst) - Apply f to each element of lst
//!     Map("map", ["f", "lst"], "(if (null? lst) '() (cons (f (car lst)) (map f (cdr lst))))"),
//! }
//! ```

use proc_macro::{TokenStream, TokenTree, Literal, Punct, Spacing, Ident, Span, Group, Delimiter};

/// Include a `.scm` file and transform it into `define_stdlib!` format.
///
/// The Scheme file should contain function definitions in the form:
/// ```lisp
/// ;;; Documentation comment (optional)
/// (define (function-name param1 param2 ...) body)
/// ```
///
/// Each definition is transformed into a `define_stdlib!` entry.
#[proc_macro]
pub fn include_stdlib(input: TokenStream) -> TokenStream {
    // Parse the input to get the file path
    let mut iter = input.into_iter();
    let path_token = match iter.next() {
        Some(TokenTree::Literal(lit)) => lit,
        _ => panic!("include_stdlib! expects a string literal path"),
    };
    
    // Extract the path string from the literal
    let path_str = path_token.to_string();
    // Remove quotes from the literal
    let path = path_str.trim_matches('"');
    
    // Read the file relative to CARGO_MANIFEST_DIR
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    let full_path = std::path::Path::new(&manifest_dir).join(path);
    
    let content = std::fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", full_path.display(), e));
    
    // Parse the lisp file and generate define_stdlib! invocation
    let stdlib_entries = parse_lisp_file(&content);
    
    // Generate the define_stdlib! macro invocation
    let mut tokens = Vec::new();
    
    // define_stdlib!
    tokens.push(TokenTree::Ident(Ident::new("define_stdlib", Span::call_site())));
    tokens.push(TokenTree::Punct(Punct::new('!', Spacing::Alone)));
    
    // Build the body of the macro
    let mut body_tokens = Vec::new();
    
    for entry in stdlib_entries {
        // Add doc comment if present
        if let Some(doc) = &entry.doc {
            // #[doc = "..."]
            body_tokens.push(TokenTree::Punct(Punct::new('#', Spacing::Alone)));
            let mut attr_tokens = Vec::new();
            attr_tokens.push(TokenTree::Ident(Ident::new("doc", Span::call_site())));
            attr_tokens.push(TokenTree::Punct(Punct::new('=', Spacing::Alone)));
            attr_tokens.push(TokenTree::Literal(Literal::string(doc)));
            body_tokens.push(TokenTree::Group(Group::new(
                Delimiter::Bracket,
                attr_tokens.into_iter().collect(),
            )));
        }
        
        // VariantName("name", ["param1", "param2"], "body"),
        body_tokens.push(TokenTree::Ident(Ident::new(&entry.variant_name, Span::call_site())));
        
        let mut args = Vec::new();
        // "name"
        args.push(TokenTree::Literal(Literal::string(&entry.name)));
        args.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        
        // ["param1", "param2"]
        let mut params_tokens = Vec::new();
        for (i, param) in entry.params.iter().enumerate() {
            if i > 0 {
                params_tokens.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
            }
            params_tokens.push(TokenTree::Literal(Literal::string(param)));
        }
        args.push(TokenTree::Group(Group::new(
            Delimiter::Bracket,
            params_tokens.into_iter().collect(),
        )));
        args.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        
        // "body"
        args.push(TokenTree::Literal(Literal::string(&entry.body)));
        
        body_tokens.push(TokenTree::Group(Group::new(
            Delimiter::Parenthesis,
            args.into_iter().collect(),
        )));
        body_tokens.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
    }
    
    tokens.push(TokenTree::Group(Group::new(
        Delimiter::Brace,
        body_tokens.into_iter().collect(),
    )));
    
    tokens.into_iter().collect()
}

/// A parsed stdlib function entry
struct StdlibEntry {
    /// The Rust enum variant name (e.g., "Map" for "map")
    variant_name: String,
    /// The Lisp function name (e.g., "map")
    name: String,
    /// Parameter names
    params: Vec<String>,
    /// The function body as a string
    body: String,
    /// Optional documentation comment
    doc: Option<String>,
}

/// Parse a lisp file and extract function definitions
fn parse_lisp_file(content: &str) -> Vec<StdlibEntry> {
    let mut entries = Vec::new();
    let mut lines = content.lines().peekable();
    
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        
        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }
        
        // Check for documentation comment (;;; ...)
        let doc = if trimmed.starts_with(";;;") {
            // Use strip_prefix for safe string manipulation
            Some(trimmed.strip_prefix(";;;").unwrap_or("").trim().to_string())
        } else {
            None
        };
        
        // If we found a doc comment, the next non-empty line should be the definition
        let def_line = if doc.is_some() {
            // Skip to next non-empty, non-comment line
            loop {
                match lines.next() {
                    Some(l) if l.trim().is_empty() || l.trim().starts_with(";;") => continue,
                    Some(l) => break l,
                    None => break "",
                }
            }
        } else if trimmed.starts_with(";;") {
            // Skip regular comments
            continue;
        } else {
            line
        };
        
        let def_trimmed = def_line.trim();
        
        // Parse (define (name params...) body)
        if def_trimmed.starts_with("(define")
            && let Some(entry) = parse_define(def_trimmed, doc, &mut lines)
        {
            entries.push(entry);
        }
    }
    
    entries
}

/// Parse a (define (name params...) body) expression
fn parse_define<'a, I: Iterator<Item = &'a str>>(
    first_line: &str,
    doc: Option<String>,
    remaining_lines: &mut I,
) -> Option<StdlibEntry> {
    // Collect the full definition (may span multiple lines)
    let mut full_def = first_line.to_string();
    
    // Count parentheses to find the end
    let mut paren_count = 0;
    for c in first_line.chars() {
        match c {
            '(' => paren_count += 1,
            ')' => paren_count -= 1,
            _ => {}
        }
    }
    
    // If not balanced, read more lines
    while paren_count > 0 {
        match remaining_lines.next() {
            Some(line) => {
                full_def.push(' ');
                full_def.push_str(line.trim());
                for c in line.chars() {
                    match c {
                        '(' => paren_count += 1,
                        ')' => paren_count -= 1,
                        _ => {}
                    }
                }
            }
            None => break,
        }
    }
    
    // Now parse the complete definition
    // Format: (define (name param1 param2 ...) body)
    let content = full_def.trim();
    if !content.starts_with("(define") {
        return None;
    }
    
    // Find the function signature: (name param1 param2 ...)
    // Skip "(define " and find the opening paren of the signature
    let after_define = content.strip_prefix("(define").unwrap_or("").trim_start();
    if !after_define.starts_with('(') {
        // Not a function definition (might be (define name value))
        return None;
    }
    
    // Find matching close paren for the signature
    let sig_start = 1; // Skip the '('
    let mut paren_depth = 1;
    let mut sig_end = sig_start;
    let after_define_chars: Vec<char> = after_define.chars().collect();
    
    for (i, &c) in after_define_chars[sig_start..].iter().enumerate() {
        match c {
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth == 0 {
                    sig_end = sig_start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    
    // Extract signature content (between parens)
    let sig_content: String = after_define_chars[sig_start..sig_end].iter().collect();
    let sig_parts: Vec<&str> = sig_content.split_whitespace().collect();
    
    if sig_parts.is_empty() {
        return None;
    }
    
    let name = sig_parts[0].to_string();
    let params: Vec<String> = sig_parts[1..].iter().map(|s| s.to_string()).collect();
    
    // Extract the body (everything after the signature, before final paren)
    // The body starts after the signature closing paren and ends at the last paren
    let body_start = sig_end + 1; // After the signature ')'
    let after_sig: String = after_define_chars[body_start..].iter().collect();
    let body_trimmed = after_sig.trim();
    
    // Remove the trailing ')' that closes the define
    // Use strip_suffix for safe string manipulation
    let body = body_trimmed.strip_suffix(')').unwrap_or(body_trimmed).trim().to_string();
    
    // Generate variant name from function name
    let variant_name = to_pascal_case(&name);
    
    Some(StdlibEntry {
        variant_name,
        name,
        params,
        body,
        doc,
    })
}

/// Convert a lisp-style name to PascalCase
/// e.g., "map" -> "Map", "set-car!" -> "SetCar", "null?" -> "NullP"
fn to_pascal_case(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    
    for c in name.chars() {
        match c {
            '-' | '_' => {
                capitalize_next = true;
            }
            '!' => {
                // Skip exclamation marks
            }
            '?' => {
                // Replace with 'P' (predicate convention)
                result.push('P');
                capitalize_next = true;
            }
            '=' => {
                // Replace with 'Eq' for equality operators
                result.push_str("Eq");
                capitalize_next = true;
            }
            '>' => {
                // Replace with 'Gt' for greater-than
                result.push_str("Gt");
                capitalize_next = true;
            }
            '<' => {
                // Replace with 'Lt' for less-than
                result.push_str("Lt");
                capitalize_next = true;
            }
            _ => {
                if capitalize_next {
                    result.extend(c.to_uppercase());
                    capitalize_next = false;
                } else {
                    result.push(c);
                }
            }
        }
    }
    
    result
}
