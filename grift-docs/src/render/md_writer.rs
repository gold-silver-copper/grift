/// Markdown writer helper wrapping a String.
pub struct MdWriter {
    buf: String,
}

impl MdWriter {
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
        }
    }

    pub fn front_matter(&mut self, title: &str, order: u32) {
        self.buf.push_str("---\n");
        self.buf.push_str(&format!("title: \"{title}\"\n"));
        self.buf.push_str(&format!("order: {order}\n"));
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

    pub fn missing_doc_warning(&mut self) {
        self.blockquote("⚠️ No documentation found.");
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
