use proc_macro::TokenStream;

#[proc_macro]
pub fn include_stdlib(input: TokenStream) -> TokenStream {
    let path_token = input
        .into_iter()
        .next()
        .expect("include_stdlib! expects a string literal path");
    let path_str = path_token.to_string();
    let path = path_str.trim_matches('"');

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let full_path = std::path::Path::new(&manifest_dir).join(path);
    let content = std::fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", full_path.display(), e));

    let forms = split_top_level_forms(&content);
    let mut fn_entries: Vec<FnEntry> = Vec::new();
    let mut constants: Vec<ConstEntry> = Vec::new();

    for form in &forms {
        if let Some(entry) = try_extract_fn_define(form) {
            fn_entries.push(entry);
        } else if let Some(entry) = try_extract_constant(form) {
            constants.push(entry);
        }
    }

    let mut code = String::new();

    // Emit static StdLibEntry for each function
    for (i, entry) in fn_entries.iter().enumerate() {
        let escaped = entry.source.replace('\\', "\\\\").replace('"', "\\\"");
        code.push_str(&format!(
            "static _STDLIB_{i}: StdLibEntry = StdLibEntry {{ name: \"{}\", source: \"{}\" }};\n",
            entry.name, escaped
        ));
    }

    // Emit STDLIB_ALL array
    code.push_str("pub static STDLIB_ALL: &[StdLib] = &[\n");
    for i in 0..fn_entries.len() {
        code.push_str(&format!("    StdLib::new(&_STDLIB_{i}),\n"));
    }
    code.push_str("];\n\n");

    // Emit init_stdlib_constants function
    code.push_str("pub(crate) fn init_stdlib_constants<const N: usize>(lisp: &Lisp<N>) {\n");
    for c in &constants {
        match &c.value {
            ConstValue::Number(n) => {
                code.push_str(&format!(
                    "    {{ let s = lisp.symbol(\"{}\").unwrap(); let v = lisp.number({}).unwrap(); let _ = lisp.env_define(ArenaIndex::GLOBAL_ENV, s, v); }}\n",
                    c.name, n
                ));
            }
            ConstValue::Bool(b) => {
                let idx = if *b {
                    "ArenaIndex::TRUE"
                } else {
                    "ArenaIndex::FALSE"
                };
                code.push_str(&format!(
                    "    {{ let s = lisp.symbol(\"{}\").unwrap(); let _ = lisp.env_define(ArenaIndex::GLOBAL_ENV, s, {}); }}\n",
                    c.name, idx
                ));
            }
            ConstValue::String(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                code.push_str(&format!(
                    "    {{ let s = lisp.symbol(\"{}\").unwrap(); let v = lisp.alloc_string(\"{}\").unwrap(); let _ = lisp.env_define(ArenaIndex::GLOBAL_ENV, s, v); }}\n",
                    c.name, escaped
                ));
            }
        }
    }
    code.push_str("}\n");

    code.parse().expect("Failed to parse generated stdlib code")
}

struct FnEntry {
    name: String,
    source: String,
}

struct ConstEntry {
    name: String,
    value: ConstValue,
}

enum ConstValue {
    Number(isize),
    Bool(bool),
    String(String),
}

fn normalize_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_comment = false;
    let mut in_string = false;
    let mut escape_next = false;
    let mut last_was_space = false;

    for c in s.chars() {
        if escape_next {
            result.push(c);
            escape_next = false;
            last_was_space = false;
            continue;
        }
        if in_comment {
            if c == '\n' {
                in_comment = false;
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            continue;
        }
        if in_string {
            if c == '\\' {
                escape_next = true;
                result.push(c);
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            result.push(c);
            last_was_space = false;
            continue;
        }
        match c {
            ';' => in_comment = true,
            '"' => {
                in_string = true;
                result.push(c);
                last_was_space = false;
            }
            c if c.is_whitespace() => {
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            _ => {
                result.push(c);
                last_was_space = false;
            }
        }
    }
    result
}

fn split_top_level_forms(source: &str) -> Vec<&str> {
    let mut forms = Vec::new();
    let mut depth = 0i32;
    let mut start = None;
    let mut in_comment = false;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in source.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
            continue;
        }
        if in_string {
            if c == '\\' {
                escape_next = true;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            ';' => in_comment = true,
            '"' => in_string = true,
            '(' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        forms.push(&source[s..=i]);
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }
    forms
}

fn try_extract_fn_define(form: &str) -> Option<FnEntry> {
    let s = normalize_whitespace(form);
    let s = s.trim();

    if !s.starts_with("(define!") {
        return None;
    }
    let rest = s["(define!".len()..].trim_start();
    if !rest.starts_with("(fn ") {
        return None;
    }

    // Find matching close paren for (fn name params...)
    let mut depth = 0i32;
    let mut sig_end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    sig_end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let sig = &rest[1..sig_end]; // "fn name p1 p2 ..."
    let tokens: Vec<&str> = sig.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }

    let name = tokens[1].to_string();
    let params: Vec<&str> = tokens[2..].to_vec();

    // Body: after sig close paren, before final )
    let after_sig = rest[sig_end + 1..].trim();
    let body = after_sig.strip_suffix(')')?.trim();

    // Multi-expression body gets wrapped in begin
    let body_forms = split_top_level_forms(body);
    let body_str = if body_forms.len() > 1 || body_forms.is_empty() {
        format!("(begin {})", body)
    } else {
        body.to_string()
    };

    let source = format!("(lambda ({}) {})", params.join(" "), body_str);
    Some(FnEntry { name, source })
}

fn try_extract_constant(form: &str) -> Option<ConstEntry> {
    let s = normalize_whitespace(form);
    let s = s.trim();

    if !s.starts_with("(define!") {
        return None;
    }
    let rest = s["(define!".len()..].trim_start();
    // Skip if it's a fn define or any sub-expression
    if rest.starts_with('(') {
        return None;
    }

    // Should be: name value)
    let inner = rest.strip_suffix(')')?;
    let mut parts = inner.split_whitespace();
    let name = parts.next()?.to_string();
    let value_str = parts.next()?;

    // Ensure no extra tokens (simple constant only)
    if parts.next().is_some() {
        return None;
    }

    // Parse value
    let value = if let Ok(n) = value_str.parse::<isize>() {
        ConstValue::Number(n)
    } else if value_str == "#t" {
        ConstValue::Bool(true)
    } else if value_str == "#f" {
        ConstValue::Bool(false)
    } else if value_str.starts_with('"') && value_str.ends_with('"') {
        ConstValue::String(value_str[1..value_str.len() - 1].to_string())
    } else {
        return None;
    };

    Some(ConstEntry { name, value })
}
