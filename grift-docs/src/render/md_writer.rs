use std::path::Path;

use crate::render::PageSpec;

/// Markdown writer helper wrapping a String.
pub struct MdWriter {
    buf: String,
}

pub struct TocItem<'a> {
    pub title: &'a str,
    pub anchor: String,
}

impl MdWriter {
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
        }
    }

    pub fn page_front_matter(&mut self, page: &PageSpec) {
        self.buf.push_str("---\n");
        self.buf.push_str(&format!("title: \"{}\"\n", page.title));
        self.buf.push_str(&format!("order: {}\n", page.order));
        self.buf.push_str("---\n\n");
    }

    pub fn h1(&mut self, s: &str) {
        self.buf.push_str(&format!("# {s}\n\n"));
    }

    pub fn h2(&mut self, s: &str) {
        self.buf.push_str(&format!("## {s}\n\n"));
    }

    pub fn h3(&mut self, s: &str) {
        self.buf.push_str(&format!("### {s}\n\n"));
    }

    pub fn paragraph(&mut self, s: &str) {
        if !s.is_empty() {
            self.buf.push_str(s);
            self.buf.push_str("\n\n");
        }
    }

    pub fn code_block(&mut self, lang: &str, s: &str) {
        self.buf.push_str(&format!("```{lang}\n"));
        self.buf.push_str(s);
        if !s.ends_with('\n') {
            self.buf.push('\n');
        }
        self.buf.push_str("```\n\n");
    }

    pub fn table(&mut self, headers: &[&str], rows: &[Vec<String>]) {
        // Header row
        self.buf.push_str("| ");
        self.buf.push_str(&headers.join(" | "));
        self.buf.push_str(" |\n");

        // Separator
        self.buf.push_str("| ");
        let seps: Vec<&str> = headers.iter().map(|_| "---").collect();
        self.buf.push_str(&seps.join(" | "));
        self.buf.push_str(" |\n");

        // Data rows
        for row in rows {
            self.buf.push_str("| ");
            self.buf.push_str(&row.join(" | "));
            self.buf.push_str(" |\n");
        }
        self.buf.push('\n');
    }

    pub fn blockquote(&mut self, s: &str) {
        for line in s.lines() {
            self.buf.push_str(&format!("> {line}\n"));
        }
        self.buf.push('\n');
    }

    pub fn raw(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    pub fn bullet_link(&mut self, label: &str, target: &str) {
        self.buf.push_str(&format!("- [{}]({target})\n", label));
    }

    pub fn toc(&mut self, title: &str, items: &[TocItem<'_>]) {
        if items.is_empty() {
            return;
        }
        self.h2(title);
        for item in items {
            self.bullet_link(item.title, &format!("#{}", item.anchor));
        }
        self.buf.push('\n');
    }

    pub fn related_links(&mut self, links: &[(&str, String)]) {
        if links.is_empty() {
            return;
        }
        self.raw("**Related:** ");
        for (idx, (label, target)) in links.iter().enumerate() {
            if idx > 0 {
                self.raw(" | ");
            }
            self.raw(&format!("[{label}]({target})"));
        }
        self.raw("\n\n");
    }

    pub fn missing_doc_warning(&mut self) {
        self.blockquote("No documentation found.");
    }

    pub fn finish(self) -> String {
        self.buf
    }
}

/// Get the first sentence of a doc string.
pub fn first_sentence(doc: &str) -> String {
    let s = doc.trim();
    if s.is_empty() {
        return String::new();
    }
    // Find first period followed by whitespace or end
    if let Some(pos) = s.find(". ") {
        return s[..=pos].to_string();
    }
    if let Some(pos) = s.find(".\n") {
        return s[..=pos].to_string();
    }
    // Return first line if no period
    s.lines().next().unwrap_or(s).to_string()
}

/// Split doc string into prose and code examples.
pub fn split_doc_examples(doc: &str) -> (String, Vec<String>) {
    let mut prose = String::new();
    let mut examples = Vec::new();
    let mut in_code = false;
    let mut current_example = String::new();
    let mut is_scheme = false;

    for line in doc.lines() {
        if line.starts_with("```") {
            if in_code {
                if is_scheme {
                    examples.push(current_example.clone());
                }
                current_example.clear();
                in_code = false;
                is_scheme = false;
            } else {
                in_code = true;
                is_scheme = line.contains("scheme") || line.contains("lisp");
            }
        } else if in_code {
            if !current_example.is_empty() {
                current_example.push('\n');
            }
            current_example.push_str(line);
        } else {
            if !prose.is_empty() {
                prose.push('\n');
            }
            prose.push_str(line);
        }
    }

    (prose, examples)
}

/// Find error variants raised in a given method.
pub fn find_errors_for_method(
    method: &str,
    error_sites: &indexmap::IndexMap<String, Vec<String>>,
) -> Vec<String> {
    let mut variants = Vec::new();
    for (variant, fns) in error_sites {
        if fns.iter().any(|f| f == method) {
            variants.push(variant.clone());
        }
    }
    variants
}

pub fn path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

pub fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in value.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            '+' => Some('p'),
            '*' => Some('x'),
            '=' => Some('e'),
            '<' => Some('l'),
            '>' => Some('g'),
            '?' => None,
            _ => Some('-'),
        };

        match mapped {
            Some('-') => {
                if !slug.is_empty() && !prev_dash {
                    slug.push('-');
                    prev_dash = true;
                }
            }
            Some(value) => {
                slug.push(value);
                prev_dash = false;
            }
            None => {}
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}
