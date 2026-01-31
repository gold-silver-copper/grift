#![forbid(unsafe_code)]

//! # Grift REPL
//!
//! A Read-Eval-Print-Loop for the grift Lisp interpreter.
//!
//! ## Features
//!
//! - Rich error display with stack traces
//! - GC commands and statistics
//! - Help system
//!
//! ## Usage
//!
//! ```rust
//! use grift_repl::run_repl;
//!
//! run_repl::<10000>();
//! ```

use std::io::{self, BufRead, Write};

pub use grift_eval::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, StdLib, Lisp, ParseError, ParseErrorKind, SourceLoc, parse,
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
        Ok(Value::True) => buf.push_str("#t"),
        Ok(Value::False) => buf.push_str("#f"),
        Ok(Value::Number(n)) => {
            use std::fmt::Write;
            write!(buf, "{}", n).unwrap();
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
        Ok(Value::Symbol { name, .. }) => {
            format_symbol(lisp, name, buf);
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
                let car = lisp.car(idx).unwrap_or(ArenaIndex::NIL);
                let cdr = lisp.cdr(idx).unwrap_or(ArenaIndex::NIL);
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
            if let (Some(expected), Some(got)) = (err.expected_args, err.got_args) {
                use std::fmt::Write;
                write!(buf, ": expected {} arguments, got {}", expected, got).unwrap();
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
                    ParseErrorKind::VectorLiteralTooLarge => buf.push_str("vector literal exceeds 256 elements"),
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
    let msg = err.message.as_str();
    if !msg.is_empty() {
        buf.push_str("\n  ");
        buf.push_str(msg);
    }
    
    // Stack trace
    if err.backtrace_len > 0 {
        buf.push_str("\n\nStack trace (most recent call first):");
        for i in (0..err.backtrace_len).rev() {
            let frame = &err.backtrace[i];
            buf.push_str("\n  ");
            use std::fmt::Write;
            write!(buf, "{}: ", err.backtrace_len - i).unwrap();
            
            if !frame.func.is_nil() {
                let mut func_buf = String::new();
                format_value(lisp, frame.func, &mut func_buf);
                if func_buf.len() > 40 {
                    buf.push_str(&func_buf[..37]);
                    buf.push_str("...");
                } else {
                    buf.push_str(&func_buf);
                }
            } else if !frame.expr.is_nil() {
                let mut expr_buf = String::new();
                format_value(lisp, frame.expr, &mut expr_buf);
                if expr_buf.len() > 40 {
                    buf.push_str(&expr_buf[..37]);
                    buf.push_str("...");
                } else {
                    buf.push_str(&expr_buf);
                }
            } else {
                buf.push_str("<unknown>");
            }
        }
    }
    
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

/// Run a REPL session
pub fn run_repl<const N: usize>() {
    let lisp: Lisp<N> = Lisp::new();
    let mut eval = match Evaluator::new(&lisp) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialize evaluator: {}", format_error(&lisp, &e));
            return;
        }
    };
    
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    
    println!("Grift Lisp (pwn_arena)");
    println!("========================");
    println!("Features: TCO, strict (call-by-value), full mutation, rich errors");
    println!("Truthiness: only #f is false (nil/'() are truthy!)");
    println!("Type :help for commands, Ctrl+D to exit.");
    println!("Arena capacity: {} cells", N);
    println!();
    
    let mut input_buffer = String::new();
    let mut continuation = false;
    
    loop {
        if continuation {
            print!("... ");
        } else {
            print!("> ");
        }
        stdout.flush().unwrap();
        
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF
                println!("\nGoodbye!");
                break;
            }
            Ok(_) => {
                if continuation {
                    input_buffer.push_str(&line);
                } else {
                    input_buffer = line;
                }
                
                let input = input_buffer.trim();
                if input.is_empty() {
                    continuation = false;
                    input_buffer.clear();
                    continue;
                }
                
                // Check for unbalanced parens (simple continuation)
                let open = input.chars().filter(|&c| c == '(').count();
                let close = input.chars().filter(|&c| c == ')').count();
                if open > close {
                    continuation = true;
                    continue;
                }
                
                continuation = false;
                
                // Special commands
                if input.starts_with(':') {
                    if handle_command(input, &lisp, &mut eval) {
                        input_buffer.clear();
                        continue;
                    }
                    // If handle_command returns false, it's :quit
                    break;
                }
                
                // Evaluate
                match eval.eval_str(input) {
                    Ok(result) => {
                        println!("{}", value_to_string(&lisp, result));
                    }
                    Err(e) => {
                        println!("{}", format_error(&lisp, &e));
                    }
                }
                
                input_buffer.clear();
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
            println!("File loading not implemented in this version");
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
    println!("Grift Lisp Help");
    println!("=================");
    println!();
    println!("Truthiness:");
    println!("  Only #f is false. Everything else is truthy, including:");
    println!("  - nil / '() (empty list)");
    println!("  - 0 (zero)");
    println!();
    println!("Literals:");
    println!("  #t, #f      - Boolean true and false");
    println!("  42, -10     - Numbers");
    println!("  'symbol     - Quoted symbol");
    println!("  '(1 2 3)    - Quoted list");
    println!();
    println!("Special Forms:");
    println!("  (quote x) or 'x       - Return x unevaluated");
    println!("  (if cond then else)   - Conditional (TCO in branches)");
    println!("  (cond (c1 e1)...)     - Multi-way conditional");
    println!("  (case key ((d1) e1)...)-Pattern matching");
    println!("  (lambda (args) body)  - Create closure");
    println!("  (define name val)     - Define variable");
    println!("  (define (f x) body)   - Define function");
    println!("  (let ((x v)...) body) - Parallel local bindings");
    println!("  (let* ((x v)...) body)- Sequential local bindings");
    println!("  (begin e1 e2...)      - Sequence");
    println!("  (and e1 e2...)        - Short-circuit and");
    println!("  (or e1 e2...)         - Short-circuit or");
    println!("  (do ((v i s)...) (t r) b) - Iteration loop");
    println!("  (quasiquote ...)      - Template with unquote");
    println!("  (eval expr)           - Evaluate at runtime");
    println!("  (apply f args)        - Apply function to list");
    println!("  (values v1 v2...)     - Multiple return values");
    println!();
    println!("Built-in Functions:");
    println!("  List:   car, cdr, cons, list");
    println!("  Pred:   eq?, eqv?, equal?, null?, pair?, number?, boolean?");
    println!("          symbol?, procedure?");
    println!("  Bool:   not");
    println!("  Math:   +, -, *, /, modulo, remainder");
    println!("  Cmp:    <, >, <=, >=, =");
    println!("  I/O:    print, display, newline");
    println!("  Err:    error");
    println!("  Mut:    set-car!, set-cdr!");
    println!("  GC:     gc, gc-enable, gc-disable, gc-enabled?, arena-stats");
    println!();
    println!("Standard Library Functions:");
    println!("  atom, map, filter, fold, length, append, reverse");
    println!("  nth, take, drop, zip, member, assoc, range");
    println!("  compose, identity, constantly, flip, curry");
    println!("  cadr, caddr, cddr");
    println!();
    println!("Mutation:");
    println!("  (set! name value)    - Mutate variable binding");
    println!("  (set-car! pair val)  - Mutate car of a pair");
    println!("  (set-cdr! pair val)  - Mutate cdr of a pair");
    println!();
    println!("Memory Management:");
    println!("  (gc)             - Trigger GC, returns (marked collected before)");
    println!("  (gc-enable)      - Enable automatic GC");
    println!("  (gc-disable)     - Disable automatic GC");
    println!("  (gc-enabled?)    - Check if GC is enabled");
    println!("  (arena-stats)    - Returns (capacity allocated free usage%)");
    println!();
    println!("NOTE: This is Scheme-like with STRICT EVALUATION and MUTATION!");
    println!("      - All arguments are evaluated before function application");
    println!("      - Full tail-call optimization (TCO) for deep recursion");
    println!("      - Mutation: set!, set-car!, set-cdr! available");
    println!();
    println!("REPL Commands:");
    println!("  :help, :h, :?  - Show this help");
    println!("  :gc            - Run garbage collection");
    println!("  :stats         - Show arena statistics");
    println!("  :env           - Show environment size");
    println!("  :quit, :q      - Exit");
    println!();
    println!("Examples:");
    println!("  (define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))");
    println!("  (fact 5)");
    println!();
    println!("  ; Tail-recursive sum");
    println!("  (define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))");
    println!("  (sum 1000 0)  ; => 500500 (no stack overflow)");
    println!();
    println!("  ; Pattern matching with case");
    println!("  (case 'b ((a) 1) ((b c) 2) (else 3))  ; => 2");
    println!();
    println!("  ; Iteration with do");
    println!("  (do ((i 1 (+ i 1)) (sum 0 (+ sum i)))");
    println!("      ((> i 5) sum))  ; => 15");
    println!();
}

/// Evaluate a string and return the result as a string
pub fn eval_to_string<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> Result<String, EvalError> {
    let result = eval.eval_str(input)?;
    // In strict evaluation, values are already fully evaluated
    Ok(value_to_string(lisp, result))
}

// ============================================================================
