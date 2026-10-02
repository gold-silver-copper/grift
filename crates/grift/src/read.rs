//! The reader: source text to data.
//!
//! There is no quotation shorthand: `'`, `` ` `` and `,` are errors, as in
//! Kernel.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ctx::Ctx;
use crate::error::{Fault, GResult};
use crate::print::CHAR_NAMES;
use crate::value::Value;

struct Reader {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';' | '\'' | '`' | ',')
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn error(&self, msg: &str) -> Fault {
        Fault::new(
            "parse-error",
            format!("line {}, column {}: {msg}", self.line, self.col),
        )
        .with(Value::Int(self.line as i64))
        .with(Value::Int(self.col as i64))
    }

    /// Skip whitespace and comments (including `#;` datum comments).
    fn skip_atmosphere(&mut self, ctx: &mut Ctx) -> GResult<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some(';') => {
                    while let Some(c) = self.bump() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                Some('#') if self.peek2() == Some('|') => {
                    self.bump();
                    self.bump();
                    let mut depth = 1;
                    while depth > 0 {
                        match self.bump() {
                            None => return Err(self.error("unterminated block comment")),
                            Some('|') if self.peek() == Some('#') => {
                                self.bump();
                                depth -= 1;
                            }
                            Some('#') if self.peek() == Some('|') => {
                                self.bump();
                                depth += 1;
                            }
                            Some(_) => {}
                        }
                    }
                }
                Some('#') if self.peek2() == Some(';') => {
                    self.bump();
                    self.bump();
                    self.skip_atmosphere(ctx)?;
                    if self.peek().is_none() {
                        return Err(self.error("datum comment without a datum"));
                    }
                    self.datum(ctx)?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn datum(&mut self, ctx: &mut Ctx) -> GResult<Value> {
        self.skip_atmosphere(ctx)?;
        match self.peek() {
            None => Err(self.error("unexpected end of input")),
            Some('(') => {
                self.bump();
                self.list(ctx)
            }
            Some(')') => Err(self.error("unexpected ')'")),
            Some('"') => {
                self.bump();
                self.string(ctx)
            }
            Some('\'' | '`' | ',') => Err(self
                .error("quotation shorthand is not supported (grift, like Kernel, has no quote)")),
            Some('#') => self.hash(),
            Some(_) => self.atom(ctx),
        }
    }

    fn list(&mut self, ctx: &mut Ctx) -> GResult<Value> {
        let mut items = Vec::new();
        let mut tail = Value::Nil;
        loop {
            self.skip_atmosphere(ctx)?;
            match self.peek() {
                None => return Err(self.error("unterminated list")),
                Some(')') => {
                    self.bump();
                    break;
                }
                Some('.') if self.peek2().is_none_or(is_delimiter) => {
                    if items.is_empty() {
                        return Err(self.error("'.' at the start of a list"));
                    }
                    self.bump();
                    tail = self.datum(ctx)?;
                    self.skip_atmosphere(ctx)?;
                    if self.bump() != Some(')') {
                        return Err(self.error("expected ')' after dotted tail"));
                    }
                    break;
                }
                Some(_) => items.push(self.datum(ctx)?),
            }
        }
        Ok(items
            .into_iter()
            .rev()
            .fold(tail, |acc, item| ctx.cons(item, acc)))
    }

    fn string(&mut self, ctx: &mut Ctx) -> GResult<Value> {
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(self.error("unterminated string")),
                Some('"') => break,
                Some('\\') => {
                    let c = match self.bump() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('a') => '\u{7}',
                        Some('b') => '\u{8}',
                        Some('0') => '\0',
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('x') => {
                            let mut hex = String::new();
                            loop {
                                match self.bump() {
                                    Some(';') => break,
                                    Some(c) if c.is_ascii_hexdigit() => hex.push(c),
                                    _ => return Err(self.error("bad \\x escape in string")),
                                }
                            }
                            u32::from_str_radix(&hex, 16)
                                .ok()
                                .and_then(char::from_u32)
                                .ok_or_else(|| self.error("bad \\x escape in string"))?
                        }
                        _ => return Err(self.error("unknown string escape")),
                    };
                    s.push(c);
                }
                Some(c) => s.push(c),
            }
        }
        Ok(ctx.string(&s))
    }

    fn hash(&mut self) -> GResult<Value> {
        self.bump();
        if self.peek() == Some('\\') {
            self.bump();
            let Some(first) = self.bump() else {
                return Err(self.error("unexpected end of input in character"));
            };
            let mut name = String::new();
            name.push(first);
            while let Some(c) = self.peek() {
                if is_delimiter(c) {
                    break;
                }
                name.push(c);
                self.bump();
            }
            if name.chars().count() == 1 {
                return Ok(Value::Char(first));
            }
            if let Some((_, c)) = CHAR_NAMES.iter().find(|(n, _)| *n == name) {
                return Ok(Value::Char(*c));
            }
            if let Some(hex) = name.strip_prefix('x')
                && let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            {
                return Ok(Value::Char(c));
            }
            return Err(self.error(&format!("unknown character name #\\{name}")));
        }
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if is_delimiter(c) {
                break;
            }
            word.push(c);
            self.bump();
        }
        match word.as_str() {
            "t" | "true" => Ok(Value::Bool(true)),
            "f" | "false" => Ok(Value::Bool(false)),
            "inert" => Ok(Value::Inert),
            "ignore" => Ok(Value::Ignore),
            _ => Err(self.error(&format!("unknown syntax #{word}"))),
        }
    }

    fn atom(&mut self, ctx: &mut Ctx) -> GResult<Value> {
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if is_delimiter(c) {
                break;
            }
            word.push(c);
            self.bump();
        }
        let digits = word.strip_prefix(['+', '-']).unwrap_or(&word);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return word
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| self.error("integer literal out of range"));
        }
        if word == "." {
            return Err(self.error("unexpected '.'"));
        }
        Ok(ctx.symbol(&word))
    }
}

impl Ctx {
    /// Read every datum in `src`.
    pub(crate) fn read_all(&mut self, src: &str) -> GResult<Vec<Value>> {
        let mut r = Reader {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        };
        let mut out = Vec::new();
        loop {
            r.skip_atmosphere(self)?;
            if r.peek().is_none() {
                return Ok(out);
            }
            out.push(r.datum(self)?);
        }
    }
}
