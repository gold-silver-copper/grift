//! Procedural macros for grift_parser standard library
//!
//! This crate provides the `include_stdlib!` macro which parses a `.scm` file
//! containing function definitions and generates a static array of `StdLib` values.
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
//! pub const STDLIB_ALL: &[StdLib] = &[
//!     StdLib::new("map", &["f", "lst"], "(begin (if (null? lst) ...))"),
//! ];
//! ```

use proc_macro::{TokenStream, TokenTree, Literal, Punct, Spacing, Ident, Span, Group, Delimiter};

/// Include a `.scm` file and transform it into a static `STDLIB_ALL` array of `StdLib` values.
///
/// The Scheme file should contain function definitions in the form:
/// ```lisp
/// ;;; Documentation comment (optional)
/// (define (function-name param1 param2 ...) body)
/// ```
///
/// Each definition is transformed into a `StdLib::new(...)` entry in the static array.
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
    
    // Parse the lisp file and generate StdLibEntry statics + STDLIB_ALL array
    let stdlib_entries = parse_lisp_file(&content);
    
    let mut tokens = Vec::new();
    
    // Generate a static StdLibEntry for each function, then a STDLIB_ALL array referencing them.
    for (i, entry) in stdlib_entries.iter().enumerate() {
        let entry_name = format!("_STDLIB_ENTRY_{}", i);
        
        // static _STDLIB_ENTRY_N: StdLibEntry = StdLibEntry { name: "...", params: &[...], body: "..." };
        tokens.push(TokenTree::Ident(Ident::new("static", Span::call_site())));
        tokens.push(TokenTree::Ident(Ident::new(&entry_name, Span::call_site())));
        tokens.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
        tokens.push(TokenTree::Ident(Ident::new("StdLibEntry", Span::call_site())));
        tokens.push(TokenTree::Punct(Punct::new('=', Spacing::Alone)));
        
        // StdLibEntry { name: "...", params: &[...], body: "..." }
        let mut fields = Vec::new();
        
        // name: "..."
        fields.push(TokenTree::Ident(Ident::new("name", Span::call_site())));
        fields.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
        fields.push(TokenTree::Literal(Literal::string(&entry.name)));
        fields.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        
        // params: &[...]
        fields.push(TokenTree::Ident(Ident::new("params", Span::call_site())));
        fields.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
        fields.push(TokenTree::Punct(Punct::new('&', Spacing::Alone)));
        let mut params_tokens = Vec::new();
        for (j, param) in entry.params.iter().enumerate() {
            if j > 0 {
                params_tokens.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
            }
            params_tokens.push(TokenTree::Literal(Literal::string(param)));
        }
        fields.push(TokenTree::Group(Group::new(
            Delimiter::Bracket,
            params_tokens.into_iter().collect(),
        )));
        fields.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        
        // body: "..."
        fields.push(TokenTree::Ident(Ident::new("body", Span::call_site())));
        fields.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
        fields.push(TokenTree::Literal(Literal::string(&entry.body)));
        fields.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        
        tokens.push(TokenTree::Ident(Ident::new("StdLibEntry", Span::call_site())));
        tokens.push(TokenTree::Group(Group::new(
            Delimiter::Brace,
            fields.into_iter().collect(),
        )));
        tokens.push(TokenTree::Punct(Punct::new(';', Spacing::Alone)));
    }
    
    // pub const STDLIB_ALL: &[StdLib] = &[StdLib::new(&_STDLIB_ENTRY_0), ...];
    tokens.push(TokenTree::Ident(Ident::new("pub", Span::call_site())));
    tokens.push(TokenTree::Ident(Ident::new("const", Span::call_site())));
    tokens.push(TokenTree::Ident(Ident::new("STDLIB_ALL", Span::call_site())));
    tokens.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
    tokens.push(TokenTree::Punct(Punct::new('&', Spacing::Alone)));
    let slice_type = vec![
        TokenTree::Ident(Ident::new("StdLib", Span::call_site())),
    ];
    tokens.push(TokenTree::Group(Group::new(
        Delimiter::Bracket,
        slice_type.into_iter().collect(),
    )));
    tokens.push(TokenTree::Punct(Punct::new('=', Spacing::Alone)));
    tokens.push(TokenTree::Punct(Punct::new('&', Spacing::Alone)));
    
    let mut array_tokens = Vec::new();
    for i in 0..stdlib_entries.len() {
        let entry_name = format!("_STDLIB_ENTRY_{}", i);
        // StdLib::new(&_STDLIB_ENTRY_N),
        array_tokens.push(TokenTree::Ident(Ident::new("StdLib", Span::call_site())));
        array_tokens.push(TokenTree::Punct(Punct::new(':', Spacing::Joint)));
        array_tokens.push(TokenTree::Punct(Punct::new(':', Spacing::Alone)));
        array_tokens.push(TokenTree::Ident(Ident::new("new", Span::call_site())));
        
        let mut args = Vec::new();
        args.push(TokenTree::Punct(Punct::new('&', Spacing::Alone)));
        args.push(TokenTree::Ident(Ident::new(&entry_name, Span::call_site())));
        
        array_tokens.push(TokenTree::Group(Group::new(
            Delimiter::Parenthesis,
            args.into_iter().collect(),
        )));
        array_tokens.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
    }
    
    tokens.push(TokenTree::Group(Group::new(
        Delimiter::Bracket,
        array_tokens.into_iter().collect(),
    )));
    tokens.push(TokenTree::Punct(Punct::new(';', Spacing::Alone)));
    
    tokens.into_iter().collect()
}

/// A parsed stdlib function entry
struct StdlibEntry {
    /// The Lisp function name (e.g., "map")
    name: String,
    /// Parameter names
    params: Vec<String>,
    /// The function body as a string
    body: String,
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
        
        // Skip to definition if doc comment (;;; ...)
        let def_line = if trimmed.starts_with(";;;") {
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
            && let Some(entry) = parse_define(def_trimmed, &mut lines)
        {
            entries.push(entry);
        }
    }
    
    entries
}

/// Strip inline comments from a line while preserving string literals.
/// Returns the line with comments removed, but preserves content inside strings.
fn strip_inline_comment(line: &str) -> String {
    let mut result = String::new();
    let mut in_string = false;
    let mut escape_next = false;
    let chars = line.chars().peekable();
    
    for c in chars {
        if escape_next {
            // If we're escaping, add the character and continue
            result.push(c);
            escape_next = false;
            continue;
        }
        
        match c {
            '\\' if in_string => {
                // Start of escape sequence in string
                result.push(c);
                escape_next = true;
            }
            '"' => {
                // Toggle string state
                in_string = !in_string;
                result.push(c);
            }
            ';' if !in_string => {
                // Start of comment outside of string - stop processing this line
                break;
            }
            _ => {
                result.push(c);
            }
        }
    }
    
    result
}

/// Parse a (define (name params...) body) expression
fn parse_define<'a, I: Iterator<Item = &'a str>>(
    first_line: &str,
    remaining_lines: &mut I,
) -> Option<StdlibEntry> {
    // Collect the full definition (may span multiple lines)
    // Strip inline comments from the first line
    let mut full_def = strip_inline_comment(first_line);
    
    // Count parentheses to find the end
    let mut paren_count = 0;
    for c in full_def.chars() {
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
                // Strip inline comments from this line before adding it
                let stripped = strip_inline_comment(line.trim());
                full_def.push(' ');
                full_def.push_str(&stripped);
                for c in stripped.chars() {
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
    let raw_body = body_trimmed.strip_suffix(')').unwrap_or(body_trimmed).trim();

    // Wrap body in (begin ...) to handle multiple expressions properly.
    // Scheme lambda bodies with internal defines need an implicit begin.
    // e.g., (define (f x) (define y 1) (+ x y)) has two expressions that need begin.
    let body = format!("(begin {})", raw_body);
    
    Some(StdlibEntry {
        name,
        params,
        body,
    })
}
