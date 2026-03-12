//! Lazy prelude support backed by the bundled source file.
//!
//! The bundled `prelude.grift` source is included as static text. Startup uses
//! the real runtime reader to walk top-level forms and binds lazy prelude
//! entries from those forms directly.

/// Bundled prelude source text.
pub const PRELUDE_SOURCE: &str = include_str!("../prelude.grift");

/// A lazy prelude binding backed by the bundled source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prelude {
    source: &'static str,
}

impl Prelude {
    /// Create a lazy prelude binding.
    pub const fn new(source: &'static str) -> Self {
        Self { source }
    }

    /// The Lisp-visible binding name.
    pub fn name(&self) -> &'static str {
        extract_binding_name(self.source).unwrap_or("<prelude>")
    }

    /// The source text for the full top-level defining form.
    pub const fn source(&self) -> &'static str {
        self.source
    }

    /// True when the bound value is a raw operative.
    pub(crate) fn is_operative(&self) -> bool {
        let mut i = skip_ws(self.source, 0);
        if self.source.as_bytes().get(i).copied().map(char::from) != Some('(') {
            return false;
        }
        i = skip_ws(self.source, i + 1);
        let Some((head, next)) = take_atom(self.source, i) else {
            return false;
        };
        if head == "fn!" {
            return false;
        }
        if head != "define!" {
            return false;
        }
        i = skip_ws(self.source, next);
        let Some((_, next)) = take_atom(self.source, i) else {
            return false;
        };
        i = skip_ws(self.source, next);
        self.source[i..].starts_with("(vau")
    }
}

fn skip_ws(s: &'static str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    while i < bytes.len() {
        match bytes[i] as char {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            ';' => {
                i += 1;
                while i < bytes.len() && bytes[i] as char != '\n' {
                    i += 1;
                }
            }
            _ => break,
        }
    }
    i
}

fn take_atom(s: &'static str, start: usize) -> Option<(&'static str, usize)> {
    let bytes = s.as_bytes();
    if start >= bytes.len() {
        return None;
    }
    let mut end = start;
    while end < bytes.len() {
        match bytes[end] as char {
            ' ' | '\t' | '\n' | '\r' | '(' | ')' | '"' | ';' => break,
            _ => end += 1,
        }
    }
    (end > start).then_some((&s[start..end], end))
}

/// Extract the symbol name introduced by a top-level `(fn! ...)` or
/// `(define! ...)` form.
pub(crate) fn extract_binding_name(form: &'static str) -> Option<&'static str> {
    let mut i = skip_ws(form, 0);
    if form.as_bytes().get(i).copied()? as char != '(' {
        return None;
    }
    i = skip_ws(form, i + 1);
    let (head, next) = take_atom(form, i)?;
    if head != "fn!" && head != "define!" {
        return None;
    }
    i = skip_ws(form, next);
    let (name, _) = take_atom(form, i)?;
    Some(name)
}
