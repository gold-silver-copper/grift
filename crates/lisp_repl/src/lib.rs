#![forbid(unsafe_code)]

//! # Lisp REPL
//!
//! A Read-Eval-Print-Loop for the classic Lisp interpreter.
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
//! use lisp_repl::run_repl;
//!
//! run_repl::<10000>();
//! ```

use std::io::{self, BufRead, Write};

pub use lisp_eval::{
    Arena, ArenaIndex, ArenaError, ArenaResult, Trace, GcStats,
    Value, Builtin, StdLib, Lisp, ParseError, ParseErrorKind, SourceLoc, parse,
    EvalError, EvalResult, Evaluator, ErrorKind, StackFrame,
    ThunkData, LambdaData,
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
        Ok(Value::Symbol { chars }) => {
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
        Ok(Value::LambdaDataVal(_)) => {
            buf.push_str("#<lambda-data>");
        }
        Ok(Value::Thunk { data }) => {
            // Check if thunk has been forced by looking at cached field
            if let Ok(Value::ThunkDataVal(thunk_data)) = lisp.get(data) {
                if thunk_data.cached.is_null() {
                    buf.push_str("#<promise>");
                } else {
                    buf.push_str("#<promise:forced>");
                }
            } else {
                buf.push_str("#<promise>");
            }
        }
        Ok(Value::ThunkDataVal(_)) => {
            buf.push_str("#<thunk-data>");
        }
        Ok(Value::Builtin(b)) => {
            buf.push_str("#<builtin:");
            buf.push_str(b.name());
            buf.push('>');
        }
        Ok(Value::StdLib { func: s, .. }) => {
            buf.push_str("#<stdlib:");
            buf.push_str(s.name());
            buf.push('>');
        }
        Ok(Value::Memo { .. }) => {
            buf.push_str("#<memoized>");
        }
        Ok(Value::Array { len, .. }) => {
            use std::fmt::Write;
            write!(buf, "#<array:{}>", len).unwrap();
        }
        Ok(Value::String { len, .. }) => {
            // Format string like in many Lisps: "..."
            buf.push('"');
            for i in 0..(len as usize) {
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
        Ok(Value::Native { id, .. }) => {
            use std::fmt::Write;
            write!(buf, "#<native:{}>", id).unwrap();
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
            Ok(Value::Cons { car, cdr }) => {
                if !first {
                    buf.push(' ');
                }
                first = false;
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
            if !err.expr.is_null() {
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
                }
            }
        }
        ErrorKind::UserError => {
            if !err.expr.is_null() {
                buf.push_str(": ");
                format_value(lisp, err.expr, &mut buf);
            }
        }
        _ => {
            // Include expression if available
            if !err.expr.is_null() && !matches!(err.kind, ErrorKind::OutOfMemory | ErrorKind::StackOverflow) {
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
            
            if !frame.func.is_null() {
                let mut func_buf = String::new();
                format_value(lisp, frame.func, &mut func_buf);
                if func_buf.len() > 40 {
                    buf.push_str(&func_buf[..37]);
                    buf.push_str("...");
                } else {
                    buf.push_str(&func_buf);
                }
            } else if !frame.expr.is_null() {
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
    
    println!("Classic Lisp (pwn_arena)");
    println!("========================");
    println!("Features: TCO, call-by-need, full mutation, rich errors");
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
            Ok(Value::Cons { cdr, .. }) => {
                count += 1;
                env = cdr;
            }
            _ => return count,
        }
    }
}

fn print_help() {
    println!("Classic Lisp Help");
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
    println!("Macros:");
    println!("  (defmacro name (params) body) - Define a macro");
    println!("  (gensym)              - Generate unique symbol");
    println!();
    println!("Built-in Functions:");
    println!("  List:   car, cdr, cons, list");
    println!("  Pred:   atom, eq, null?, pair?, number?, boolean?");
    println!("          symbol?, procedure?");
    println!("  Bool:   not");
    println!("  Math:   +, -, *, /, mod");
    println!("  Cmp:    <, >, <=, >=, =");
    println!("  I/O:    print, display, newline");
    println!("  Err:    error");
    println!("  Memo:   memoize");
    println!("  Mut:    set-car!, set-cdr!");
    println!("  GC:     gc, gc-enable, gc-disable, gc-enabled?, arena-stats");
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
    println!("NOTE: This is a Lisp with HYBRID EVALUATION and MUTATION!");
    println!("      - Tail calls: STRICT (enables proper TCO)");
    println!("      - Builtins: LAZY (infinite data structures work)");
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
    println!("  ; Infinite stream of ones");
    println!("  (define (ones) (cons 1 (ones)))");
    println!("  (car (ones))       ; => 1");
    println!("  (car (cdr (ones))) ; => 1");
    println!();
    println!("  ; Pattern matching with case");
    println!("  (case 'b ((a) 1) ((b c) 2) (else 3))  ; => 2");
    println!();
    println!("  ; Iteration with do");
    println!("  (do ((i 1 (+ i 1)) (sum 0 (+ sum i)))");
    println!("      ((> i 5) sum))  ; => 15");
    println!();
    println!("  ; Macros");
    println!("  (defmacro unless (c t e) (list 'if c e t))");
    println!("  (unless #f 'yes 'no)  ; => yes");
    println!();
}

/// Evaluate a string and return the result as a string
/// This deeply forces the result for display (lazy values are evaluated)
pub fn eval_to_string<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> Result<String, EvalError> {
    let result = eval.eval_str(input)?;
    // Deep force for display
    let forced = deep_force(lisp, eval, result)?;
    Ok(value_to_string(lisp, forced))
}

/// Deeply force a value for display
/// Forces all nested thunks in cons cells (up to a depth limit)
fn deep_force<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, idx: ArenaIndex) -> Result<ArenaIndex, EvalError> {
    deep_force_impl(lisp, eval, idx, 100) // Limit depth to prevent infinite loops
}

fn deep_force_impl<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, idx: ArenaIndex, depth: usize) -> Result<ArenaIndex, EvalError> {
    if depth == 0 {
        return Ok(idx); // Stop at depth limit
    }
    
    // First, force this value to WHNF
    let forced = force_whnf(lisp, eval, idx)?;
    
    // Then recursively force cons cells
    match lisp.get(forced)? {
        Value::Cons { car, cdr } => {
            let forced_car = deep_force_impl(lisp, eval, car, depth - 1)?;
            let forced_cdr = deep_force_impl(lisp, eval, cdr, depth - 1)?;
            // Return a new cons with forced values
            lisp.cons(forced_car, forced_cdr).map_err(Into::into)
        }
        _ => Ok(forced),
    }
}

/// Force a value to WHNF (similar to evaluator's force)
fn force_whnf<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, mut idx: ArenaIndex) -> Result<ArenaIndex, EvalError> {
    loop {
        match lisp.get(idx)? {
            Value::Thunk { data } => {
                let thunk_data = match lisp.get(data)? {
                    Value::ThunkDataVal(td) => td,
                    _ => return Ok(idx), // Corrupted thunk, just return as-is
                };
                if !thunk_data.cached.is_null() {
                    idx = thunk_data.cached;
                    continue;
                }
                // Force the thunk using the evaluator
                let result = eval.eval_in_env(thunk_data.expr, thunk_data.env)?;
                lisp.set(data, Value::ThunkDataVal(ThunkData { 
                    expr: thunk_data.expr, 
                    env: thunk_data.env, 
                    cached: result 
                }))?;
                idx = result;
                continue;
            }
            _ => return Ok(idx),
        }
    }
}

// ============================================================================
