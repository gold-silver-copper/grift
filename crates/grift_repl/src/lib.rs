#![forbid(unsafe_code)]

//! # Grift REPL
//!
//! A Read-Eval-Print-Loop for the Grift R7RS-compliant Scheme interpreter.
//!
//! ## Features
//!
//! - Rich error display with stack traces
//! - Line editing with arrow keys (via rustyline)
//! - GC commands and statistics
//! - Help system
//! - Output callback for side effects during macro expansion
//! - Integrated `StdIoProvider` from `grift_std` for port-based I/O
//!   (stdin, stdout, stderr, and dynamic string ports)
//!
//! ## Usage
//!
//! ```rust
//! use grift_repl::run_repl;
//!
//! run_repl::<10000>();
//! ```

use rustyline::error::ReadlineError;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter};

pub use grift_eval::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, StdLib, Lisp, DisplayValue, ParseError, ParseErrorKind, SourceLoc, parse,
    EvalError, EvalResult, Evaluator, ErrorKind, StackFrame,
};

// ============================================================================
// Value Formatting
// ============================================================================

/// Format a Lisp value as a string
pub fn format_value<const N: usize>(lisp: &Lisp<N>, idx: ArenaIndex, buf: &mut String) {
    format_value_impl(lisp, idx, buf, 0)
}

/// Format with depth limit for protection against cycles
fn format_value_impl<const N: usize>(
    lisp: &Lisp<N>, 
    idx: ArenaIndex, 
    buf: &mut String,
    depth: usize,
) {
    if depth > 100 {
        buf.push_str("...");
        return;
    }
    
    match lisp.get(idx) {
        Ok(Value::Nil) => buf.push_str("()"),
        Ok(Value::Void) => buf.push_str("#<void>"),
        Ok(Value::True) => buf.push_str("#t"),
        Ok(Value::False) => buf.push_str("#f"),
        Ok(Value::Number(n)) => {
            use std::fmt::Write;
            write!(buf, "{}", n).unwrap();
        }
        Ok(Value::Float(f)) => {
            use std::fmt::Write;
            if f.is_nan() {
                buf.push_str("+nan.0");
            } else if f.is_infinite() {
                if f > 0.0 { buf.push_str("+inf.0"); } else { buf.push_str("-inf.0"); }
            } else if f.is_finite() && f == (f as isize as grift_eval::fsize) {
                write!(buf, "{:.1}", f).unwrap();
            } else {
                write!(buf, "{}", f).unwrap();
            }
        }
        Ok(Value::Rational { num, denom }) => {
            use std::fmt::Write;
            write!(buf, "{}/{}", num, denom).unwrap();
        }
        Ok(Value::Complex { real, imag }) => {
            use std::fmt::Write;
            if imag >= 0.0 {
                write!(buf, "{}+{}i", real, imag).unwrap();
            } else {
                write!(buf, "{}{}i", real, imag).unwrap();
            }
        }
        Ok(Value::Char(c)) => {
            buf.push_str("#\\");
            match c {
                ' ' => buf.push_str("space"),
                '\n' => buf.push_str("newline"),
                '\t' => buf.push_str("tab"),
                _ => buf.push(c),
            }
        }
        Ok(Value::Symbol(chars)) => {
            format_symbol(lisp, chars, buf);
        }
        Ok(Value::Cons { .. }) => {
            buf.push('(');
            format_list_contents(lisp, idx, buf, depth + 1);
            buf.push(')');
        }
        Ok(Value::Lambda { .. }) => {
            buf.push_str("#<lambda>");
        }
        Ok(Value::Builtin(b)) => {
            buf.push_str("#<builtin:");
            buf.push_str(b.name());
            buf.push('>');
        }
        Ok(Value::StdLib(s)) => {
            buf.push_str("#<stdlib:");
            buf.push_str(s.name());
            buf.push('>');
        }
        Ok(Value::Array { .. }) => {
            // Format as R7RS vector literal: #(elem1 elem2 ...)
            buf.push_str("#(");
            let len = lisp.array_len(idx).unwrap_or(0);
            for i in 0..len {
                if i > 0 {
                    buf.push(' ');
                }
                if let Ok(elem_idx) = lisp.array_get(idx, i) {
                    format_value(lisp, elem_idx, buf);
                }
            }
            buf.push(')');
        }
        Ok(Value::Bytevector { .. }) => {
            // Format as R7RS bytevector literal: #u8(byte ...)
            buf.push_str("#u8(");
            let len = lisp.bytevector_len(idx).unwrap_or(0);
            for i in 0..len {
                if i > 0 {
                    buf.push(' ');
                }
                if let Ok(elem_idx) = lisp.bytevector_get(idx, i) {
                    format_value(lisp, elem_idx, buf);
                }
            }
            buf.push(')');
        }
        Ok(Value::String { .. }) => {
            // Format string like in many Lisps: "..."
            buf.push('"');
            let len = lisp.string_len(idx).unwrap_or(0);
            for i in 0..len {
                if let Ok(c) = lisp.string_char_at(idx, i) {
                    match c {
                        '"' => buf.push_str("\\\""),
                        '\\' => buf.push_str("\\\\"),
                        '\n' => buf.push_str("\\n"),
                        '\t' => buf.push_str("\\t"),
                        _ => buf.push(c),
                    }
                }
            }
            buf.push('"');
        }
        Ok(Value::Native { .. }) => {
            use std::fmt::Write;
            let id = lisp.native_id(idx).unwrap_or(0);
            write!(buf, "#<native:{}>", id).unwrap();
        }
        Ok(Value::Ref(idx)) => {
            use std::fmt::Write;
            write!(buf, "#<ref:{}>", idx.raw()).unwrap();
        }
        Ok(Value::Usize(n)) => {
            use std::fmt::Write;
            write!(buf, "#<usize:{}>", n).unwrap();
        }
        Ok(Value::Syntax { expr, .. }) => {
            buf.push_str("#<syntax:");
            format_value(lisp, expr, buf);
            buf.push('>');
        }
        Ok(Value::ContFrame { .. }) => {
            buf.push_str("#<cont-frame>");
        }
        Ok(Value::Continuation { .. }) => {
            buf.push_str("#<continuation>");
        }
        Ok(Value::ErrorObject { .. }) => {
            buf.push_str("#<error-object>");
        }
        Ok(Value::Port(port_id)) => {
            use std::fmt::Write;
            write!(buf, "#<port:{}>", port_id.0).unwrap();
        }
        Ok(Value::Eof) => {
            buf.push_str("#<eof>");
        }
        Ok(Value::Environment { .. }) => {
            buf.push_str("#<environment>");
        }
        Err(_) => buf.push_str("#<error>"),
    }
}

/// Format the contents of a list (without outer parens)
fn format_list_contents<const N: usize>(
    lisp: &Lisp<N>, 
    mut idx: ArenaIndex, 
    buf: &mut String,
    depth: usize,
) {
    let mut first = true;
    let mut count = 0;
    
    loop {
        if count > 100 {
            buf.push_str(" ...");
            break;
        }
        
        match lisp.get(idx) {
            Ok(Value::Nil) => break,
            Ok(Value::Cons { .. }) => {
                if !first {
                    buf.push(' ');
                }
                first = false;
                let (car, cdr) = lisp.car_cdr(idx).unwrap_or((ArenaIndex::NIL, ArenaIndex::NIL));
                format_value_impl(lisp, car, buf, depth);
                idx = cdr;
                count += 1;
            }
            Ok(_) => {
                // Improper list (dotted pair)
                buf.push_str(" . ");
                format_value_impl(lisp, idx, buf, depth);
                break;
            }
            Err(_) => {
                buf.push_str(" . #<error>");
                break;
            }
        }
    }
}

/// Format a symbol name.
fn format_symbol<const N: usize>(lisp: &Lisp<N>, chars: ArenaIndex, buf: &mut String) {
    // Get the length from the String value
    let len = lisp.string_len(chars).unwrap_or(0);
    for i in 0..len {
        if i > 64 {
            buf.push_str("...");
            break;
        }
        if let Ok(c) = lisp.string_char_at(chars, i) {
            buf.push(c);
        }
    }
}

/// Convert a Lisp value to a string
pub fn value_to_string<const N: usize>(lisp: &Lisp<N>, idx: ArenaIndex) -> String {
    let mut buf = String::new();
    format_value(lisp, idx, &mut buf);
    buf
}

/// Output callback for display/newline during evaluation.
/// 
/// This function is called by the evaluator when display or newline is executed,
/// including during macro expansion phases. This allows output to be visible
/// during both runtime evaluation and macro expansion.
/// 
/// We check if the value is nil (which represents a newline) or an actual value.
fn output_callback<const N: usize>(lisp: &Lisp<N>, val: ArenaIndex) {
    use std::io::{self, Write};
    let mut stdout = io::stdout();
    
    // Check if this is a newline (represented by nil value)
    // We distinguish by checking the actual pointer - if it's the NIL constant
    if val.is_nil() {
        let _ = stdout.write_all(b"\n");
    } else {
        // Display the value
        let s = value_to_string(lisp, val);
        let _ = stdout.write_all(s.as_bytes());
    }
    let _ = stdout.flush();
}

// ============================================================================
// Error Formatting
// ============================================================================

/// Format an evaluation error with full context
pub fn format_error<const N: usize>(lisp: &Lisp<N>, err: &EvalError) -> String {
    let mut buf = String::new();
    
    // Main error message
    buf.push_str("Error: ");
    buf.push_str(err.kind.as_str());
    
    // Additional context based on error type
    match err.kind {
        ErrorKind::UnboundVariable => {
            if !err.expr.is_nil() {
                buf.push_str(": ");
                format_value(lisp, err.expr, &mut buf);
            }
        }
        ErrorKind::TypeError => {
            if let (Some(expected), Some(got)) = (err.expected, err.got) {
                use std::fmt::Write;
                write!(buf, ": expected {}, got {}", expected, got).unwrap();
            }
        }
        ErrorKind::WrongArgCount => {
            if let Some(info) = &err.arg_info {
                use std::fmt::Write;
                write!(buf, ": expected {} arguments, got {}", info.expected, info.got).unwrap();
            }
        }
        ErrorKind::Parse => {
            if let Some(ref pe) = err.parse_error {
                use std::fmt::Write;
                write!(buf, " at line {}, column {}: ", pe.loc.line, pe.loc.column).unwrap();
                match pe.kind {
                    ParseErrorKind::UnexpectedEof => buf.push_str("unexpected end of input"),
                    ParseErrorKind::UnexpectedChar(c) => {
                        write!(buf, "unexpected character '{}'", c).unwrap();
                    }
                    ParseErrorKind::UnmatchedParen => buf.push_str("unmatched parenthesis"),
                    ParseErrorKind::NumberOverflow => buf.push_str("number too large"),
                    ParseErrorKind::OutOfMemory => buf.push_str("out of memory"),
                    ParseErrorKind::InvalidHashLiteral => buf.push_str("invalid # literal"),
                    ParseErrorKind::InvalidCharLiteral => buf.push_str("invalid character literal"),
                    ParseErrorKind::InvalidEscapeSequence => buf.push_str("invalid escape sequence"),
                    ParseErrorKind::UnterminatedString => buf.push_str("unterminated string"),
                }
            }
        }
        ErrorKind::SyntaxError => {
            // syntax-error stores (message arg ...) in expr
            if !err.expr.is_nil() {
                if let Ok(Value::Cons { car, cdr }) = lisp.get(err.expr) {
                    // First element is the message string
                    buf.push_str(": ");
                    format_value(lisp, car, &mut buf);
                    // Remaining elements are additional args
                    let mut rest = cdr;
                    while let Ok(Value::Cons { car: arg, cdr: next }) = lisp.get(rest) {
                        buf.push(' ');
                        format_value(lisp, arg, &mut buf);
                        rest = next;
                    }
                }
            }
        }
        ErrorKind::UserError => {
            if !err.expr.is_nil() {
                buf.push_str(": ");
                format_value(lisp, err.expr, &mut buf);
            }
        }
        _ => {
            // Include expression if available
            if !err.expr.is_nil() && !matches!(err.kind, ErrorKind::OutOfMemory | ErrorKind::StackOverflow) {
                buf.push_str(" in: ");
                let mut expr_buf = String::new();
                format_value(lisp, err.expr, &mut expr_buf);
                // Truncate long expressions
                if expr_buf.len() > 60 {
                    buf.push_str(&expr_buf[..57]);
                    buf.push_str("...");
                } else {
                    buf.push_str(&expr_buf);
                }
            }
        }
    }
    
    // Custom message if present
    let msg = err.message;
    if !msg.is_empty() {
        buf.push_str("\n  ");
        buf.push_str(msg);
    }
    
    // Note: Stack traces are no longer embedded in EvalError for efficiency.
    // The evaluator maintains its own call stack which can be accessed separately.
    
    buf
}

// ============================================================================
// REPL
// ============================================================================

/// The REPL structure (for API convenience)
pub struct Repl<const N: usize> {
    lisp: Lisp<N>,
}

impl<const N: usize> Repl<N> {
    /// Create a new REPL
    pub fn new() -> Self {
        Repl {
            lisp: Lisp::new(),
        }
    }
    
    /// Get the Lisp context
    pub fn lisp(&self) -> &Lisp<N> {
        &self.lisp
    }
}

impl<const N: usize> Default for Repl<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a string contains only comments and whitespace.
///
/// Returns true if the input contains no actual Scheme expressions,
/// only whitespace, line comments (`;`), and block comments (`#| ... |#`).
pub fn is_only_comments_and_whitespace(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b';' => {
                // Skip to end of line
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'#' if i + 1 < bytes.len() && bytes[i + 1] == b'|' => {
                // Skip nestable block comment #| ... |#
                i += 2;
                let mut depth: usize = 1;
                while i + 1 < bytes.len() && depth > 0 {
                    if bytes[i] == b'#' && bytes[i + 1] == b'|' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'|' && bytes[i + 1] == b'#' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                // Handle last byte if only one left
                if depth > 0 && i < bytes.len() {
                    i += 1;
                }
            }
            _ => return false,
        }
    }
    true
}

/// A rustyline helper that validates Scheme parentheses for multiline input.
///
/// When parentheses are unbalanced (more opens than closes), rustyline will
/// prompt for continuation lines instead of submitting. This also enables
/// proper multiline paste support — pasted text with unbalanced parens is
/// accumulated until balanced.
#[derive(Completer, Helper, Highlighter, Hinter)]
struct SchemeValidator;

impl Validator for SchemeValidator {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

        // Empty or comment-only input is valid (submit immediately)
        if input.trim().is_empty() || is_only_comments_and_whitespace(input) {
            return Ok(ValidationResult::Valid(None));
        }

        // Count parentheses, skipping strings and comments
        let mut depth: i32 = 0;
        let mut in_string = false;
        let mut in_line_comment = false;
        let mut escape = false;
        for c in input.chars() {
            if in_line_comment {
                if c == '\n' {
                    in_line_comment = false;
                }
                continue;
            }
            if escape {
                escape = false;
                continue;
            }
            if c == '\\' && in_string {
                escape = true;
                continue;
            }
            if c == '"' {
                in_string = !in_string;
                continue;
            }
            if !in_string {
                match c {
                    ';' => { in_line_comment = true; }
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
            }
        }

        if depth > 0 || in_string {
            Ok(ValidationResult::Incomplete)
        } else {
            Ok(ValidationResult::Valid(None))
        }
    }
}

/// Run a REPL session
pub fn run_repl<const N: usize>() {
    let lisp: Lisp<N> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = match Evaluator::new(&lisp) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialize evaluator: {}", format_error(&lisp, &e));
            return;
        }
    };
    
    // Set output callback for display/newline to enable side effects during macro expansion
    eval.set_output_callback(Some(output_callback));
    // Set I/O provider for port operations
    eval.set_io_provider(&mut io);
    
    let mut line_editor = Editor::new().expect("Failed to create line editor");
    line_editor.set_helper(Some(SchemeValidator));
    
    println!("Grift Lisp");
    println!("========================");
    println!("Features: TCO, strict (call-by-value), full mutation, rich errors");
    println!("Truthiness: only #f is false (nil/'() are truthy!)");
    println!("Type :help for commands, Ctrl+D to exit.");
    println!("Arena capacity: {} cells", N);
    println!();
    
    loop {
        match line_editor.readline("Λ ") {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() || is_only_comments_and_whitespace(input) {
                    continue;
                }
                
                // Add to history
                let _ = line_editor.add_history_entry(input);
                
                // Special commands
                if input.starts_with(':') {
                    if handle_command(input, &lisp, &mut eval) {
                        continue;
                    }
                    // If handle_command returns false, it's :quit
                    break;
                }
                
                // Evaluate
                match eval.eval_str(input) {
                    Ok(result) => {
                        // Don't print anything for void values (from define, set!, display, etc.)
                        if !matches!(lisp.get(result), Ok(Value::Void)) {
                            println!("{}", value_to_string(&lisp, result));
                        }
                    }
                    Err(e) => {
                        println!("{}", format_error(&lisp, &e));
                    }
                }
            }
            Err(ReadlineError::Eof) => {
                println!("\nGoodbye!");
                break;
            }
            Err(ReadlineError::Interrupted) => {
                // Cancel current input (Ctrl+C)
                continue;
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }
    }
}

/// Handle REPL commands. Returns true to continue, false to quit.
fn handle_command<const N: usize>(input: &str, lisp: &Lisp<N>, eval: &mut Evaluator<N>) -> bool {
    let cmd = input.trim();
    
    match cmd {
        ":q" | ":quit" | ":exit" => {
            println!("Goodbye!");
            return false;
        }
        ":gc" => {
            let stats = eval.gc();
            println!("GC complete:");
            println!("  Marked:    {}", stats.marked);
            println!("  Collected: {}", stats.collected);
            println!("  Before:    {}", stats.total_before);
        }
        ":stats" | ":stat" => {
            let stats = lisp.stats();
            println!("Arena statistics:");
            println!("  Capacity:      {}", stats.capacity);
            println!("  Allocated:     {}", stats.allocated);
            println!("  Free:          {}", stats.capacity - stats.allocated);
            println!("  Usage:         {:.1}%", stats.usage_percent());
            println!("  Fragmentation: {:.2}", stats.fragmentation);
        }
        ":help" | ":h" | ":?" => {
            print_help();
        }
        ":env" => {
            println!("Global environment has {} bindings", 
                     count_env(lisp, eval.global_env()));
        }
        _ if cmd.starts_with(":load ") => {
            let path = cmd.trim_start_matches(":load ").trim();
            match std::fs::read_to_string(path) {
                Ok(contents) => {
                    // Wrap in (begin ...) to evaluate all top-level forms
                    let mut wrapped = String::with_capacity(contents.len() + 9);
                    wrapped.push_str("(begin ");
                    wrapped.push_str(&contents);
                    wrapped.push(')');
                    match eval.eval_str(&wrapped) {
                        Ok(result) => {
                            if !matches!(lisp.get(result), Ok(Value::Void)) {
                                println!("{}", value_to_string(lisp, result));
                            }
                            println!("Loaded: {}", path);
                        }
                        Err(e) => {
                            println!("Error loading {}: {}", path, format_error(lisp, &e));
                        }
                    }
                }
                Err(e) => {
                    println!("Cannot read file '{}': {}", path, e);
                }
            }
        }
        _ => {
            println!("Unknown command: {}", cmd);
            println!("Type :help for available commands");
        }
    }
    
    true
}

fn count_env<const N: usize>(lisp: &Lisp<N>, mut env: ArenaIndex) -> usize {
    let mut count = 0;
    loop {
        match lisp.get(env) {
            Ok(Value::Nil) => return count,
            Ok(Value::Cons { .. }) => {
                count += 1;
                env = lisp.cdr(env).unwrap_or(ArenaIndex::NIL);
            }
            _ => return count,
        }
    }
}

fn print_help() {
    print!("\
Grift Lisp Help
=================

Truthiness:
  Only #f is false. Everything else is truthy, including:
  - nil / '() (empty list)
  - 0 (zero)

Literals:
  #t, #f      - Boolean true and false
  42, -10     - Numbers
  'symbol     - Quoted symbol
  '(1 2 3)    - Quoted list

Special Forms:
  (quote x) or 'x       - Return x unevaluated
  (if cond then else)   - Conditional (TCO in branches)
  (cond (c1 e1)...)     - Multi-way conditional
  (case key ((d1) e1)...)-Pattern matching
  (lambda (args) body)  - Create closure
  (define name val)     - Define variable
  (define (f x) body)   - Define function
  (let ((x v)...) body) - Parallel local bindings
  (let* ((x v)...) body)- Sequential local bindings
  (begin e1 e2...)      - Sequence
  (and e1 e2...)        - Short-circuit and
  (or e1 e2...)         - Short-circuit or
  (do ((v i s)...) (t r) b) - Iteration loop
  (quasiquote ...)      - Template with unquote
  (eval expr)           - Evaluate at runtime
  (apply f args)        - Apply function to list
  (values v1 v2...)     - Multiple return values

Built-in Functions:
  List:   car, cdr, cons, list
  Pred:   eq?, eqv?, equal?, null?, pair?, number?, boolean?
          symbol?, procedure?
  Bool:   not
  Math:   +, -, *, /, modulo, remainder
  Cmp:    <, >, <=, >=, =
  I/O:    print, display, newline
  Err:    error
  Mut:    set-car!, set-cdr!
  GC:     gc, gc-enable, gc-disable, gc-enabled?, arena-stats

Standard Library Functions:
  atom, map, filter, fold, length, append, reverse
  nth, take, drop, zip, member, assoc, range
  compose, identity, constantly, flip, curry
  cadr, caddr, cddr

Mutation:
  (set! name value)    - Mutate variable binding
  (set-car! pair val)  - Mutate car of a pair
  (set-cdr! pair val)  - Mutate cdr of a pair

Memory Management:
  (gc)             - Trigger GC, returns (marked collected before)
  (gc-enable)      - Enable automatic GC
  (gc-disable)     - Disable automatic GC
  (gc-enabled?)    - Check if GC is enabled
  (arena-stats)    - Returns (capacity allocated free usage%)

NOTE: This is Scheme-like with STRICT EVALUATION and MUTATION!
      - All arguments are evaluated before function application
      - Full tail-call optimization (TCO) for deep recursion
      - Mutation: set!, set-car!, set-cdr! available

REPL Commands:
  :help, :h, :?     - Show this help
  :gc               - Run garbage collection
  :stats            - Show arena statistics
  :env              - Show environment size
  :load <file>      - Load and evaluate a .scm file
  :quit, :q         - Exit

Examples:
  (define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))
  (fact 5)

  ; Tail-recursive sum
  (define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))
  (sum 1000 0)  ; => 500500 (no stack overflow)

  ; Pattern matching with case
  (case 'b ((a) 1) ((b c) 2) (else 3))  ; => 2

  ; Iteration with do
  (do ((i 1 (+ i 1)) (sum 0 (+ sum i)))
      ((> i 5) sum))  ; => 15
");
}

/// Evaluate a string and return the result as a string
pub fn eval_to_string<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> Result<String, EvalError> {
    let result = eval.eval_str(input)?;
    // In strict evaluation, values are already fully evaluated
    Ok(value_to_string(lisp, result))
}

// ============================================================================
