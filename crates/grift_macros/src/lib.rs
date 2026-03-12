use proc_macro::TokenStream;

#[proc_macro]
pub fn include_prelude(input: TokenStream) -> TokenStream {
    let path_token = input
        .into_iter()
        .next()
        .expect("include_prelude! expects a string literal path");
    let path_str = path_token.to_string();
    let path = path_str.trim_matches('"');

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let full_path = std::path::Path::new(&manifest_dir).join(path);
    let content = std::fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", full_path.display(), e));

    let forms = split_top_level_forms(&content);
    let mut entries: Vec<PreludeEntrySpec> = Vec::new();
    let mut constants: Vec<ConstEntry> = Vec::new();

    for form in &forms {
        if let Some(entry) = try_extract_prelude_entry(form) {
            entries.push(entry);
        } else if let Some(entry) = try_extract_constant(form) {
            constants.push(entry);
        } else {
            panic!("Unsupported top-level prelude form: {}", form.trim());
        }
    }

    let mut code = String::new();

    // Emit static PreludeEntry for each lazy prelude binding.
    for (i, entry) in entries.iter().enumerate() {
        code.push_str(&format!(
            "static _PRELUDE_{i}: PreludeEntry = PreludeEntry {{ name: {:?}, source: {:?} }};\n",
            entry.name, entry.source
        ));
    }

    // Emit PRELUDE_ALL array
    code.push_str("pub static PRELUDE_ALL: &[Prelude] = &[\n");
    for i in 0..entries.len() {
        code.push_str(&format!("    Prelude::new(&_PRELUDE_{i}),\n"));
    }
    code.push_str("];\n\n");

    // Emit init_prelude_constants function
    code.push_str("pub(crate) fn init_prelude_constants(lisp: &dyn LispOps) {\n");
    for c in &constants {
        match &c.value {
            ConstValue::Number(n) => {
                code.push_str(&format!(
                    "    {{ let s = lisp.symbol({:?}).unwrap(); let v = lisp.number({}).unwrap(); let _ = lisp.define_global(s, v); }}\n",
                    c.name, n
                ));
            }
            ConstValue::Bool(b) => {
                code.push_str(&format!(
                    "    {{ let s = lisp.symbol({:?}).unwrap(); let _ = lisp.define_global(s, lisp.boolean({})); }}\n",
                    c.name, b
                ));
            }
            ConstValue::String(s) => {
                code.push_str(&format!(
                    "    {{ let s = lisp.symbol({:?}).unwrap(); let v = lisp.alloc_string({:?}).unwrap(); let _ = lisp.define_global(s, v); }}\n",
                    c.name, s
                ));
            }
        }
    }
    code.push_str("}\n");

    code.parse()
        .expect("Failed to parse generated prelude code")
}

struct PreludeEntrySpec {
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

fn try_extract_prelude_entry(form: &str) -> Option<PreludeEntrySpec> {
    if let Some(entry) = try_extract_fn_define(form) {
        return Some(entry);
    }

    let (name, source) = extract_named_define(form)?;
    if source.starts_with("(vau") || source.starts_with("(lambda") {
        Some(PreludeEntrySpec { name, source })
    } else {
        None
    }
}

fn try_extract_fn_define(form: &str) -> Option<PreludeEntrySpec> {
    let s = normalize_whitespace(form);
    let s = s.trim();

    if !s.starts_with("(fn!") {
        return None;
    }
    let rest = s["(fn!".len()..].trim_start();

    // Parse: name (params...) body...
    // First token is the function name
    let name_end = rest
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(rest.len());
    let name = rest[..name_end].to_string();
    let after_name = rest[name_end..].trim_start();

    // Next is the params list (...)
    if !after_name.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    let mut params_end = 0;
    for (i, c) in after_name.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    params_end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let params_str = &after_name[1..params_end]; // contents inside parens
    let params: Vec<&str> = params_str.split_whitespace().collect();

    // Body: after params close paren, before final )
    let after_params = after_name[params_end + 1..].trim();
    let body = after_params.strip_suffix(')')?.trim();

    // Multi-expression body gets wrapped in begin
    let body_forms = split_top_level_forms(body);
    let body_str = if body_forms.len() > 1 || body_forms.is_empty() {
        format!("(begin {})", body)
    } else {
        body.to_string()
    };

    let source = format!("(lambda ({}) {})", params.join(" "), body_str);
    Some(PreludeEntrySpec { name, source })
}

fn try_extract_constant(form: &str) -> Option<ConstEntry> {
    let (name, value_str) = extract_named_define(form)?;

    // Ensure no extra tokens (simple constant only)
    let mut parts = value_str.split_whitespace();
    let value_str = parts.next()?;
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

fn extract_named_define(form: &str) -> Option<(String, String)> {
    let s = normalize_whitespace(form);
    let s = s.trim();

    if !s.starts_with("(define!") {
        return None;
    }
    let rest = s["(define!".len()..].trim_start();
    if rest.starts_with('(') {
        return None;
    }

    let name_end = rest
        .find(|c: char| c.is_whitespace() || c == ')')
        .unwrap_or(rest.len());
    let name = rest[..name_end].to_string();
    let rhs = rest[name_end..]
        .trim_start()
        .strip_suffix(')')?
        .trim()
        .to_string();
    Some((name, rhs))
}
