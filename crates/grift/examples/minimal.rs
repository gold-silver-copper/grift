//! Minimal Grift example demonstrating basic usage.
//!
//! Run with: `cargo run --example minimal`

use grift::{Lisp, Evaluator, Value};

fn main() {
    // Create a Lisp interpreter with a 10,000-cell arena
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).expect("Failed to create evaluator");

    println!("Grift Minimal Example");
    println!("=====================\n");

    // Basic arithmetic
    let result = eval.eval_str("(+ 1 2 3 4 5)").unwrap();
    print_result(&lisp, result, "(+ 1 2 3 4 5)");

    // Define a function
    eval.eval_str("(define (square x) (* x x))").unwrap();
    println!("Defined: (define (square x) (* x x))");
    
    let result = eval.eval_str("(square 7)").unwrap();
    print_result(&lisp, result, "(square 7)");

    // Factorial with recursion
    eval.eval_str("(define (factorial n) (if (= n 0) 1 (* n (factorial (- n 1)))))").unwrap();
    println!("Defined: (define (factorial n) ...)");
    
    let result = eval.eval_str("(factorial 10)").unwrap();
    print_result(&lisp, result, "(factorial 10)");

    // Higher-order functions
    let result = eval.eval_str("(map square '(1 2 3 4 5))").unwrap();
    print_result(&lisp, result, "(map square '(1 2 3 4 5))");

    // Closures
    eval.eval_str("(define (make-adder n) (lambda (x) (+ x n)))").unwrap();
    eval.eval_str("(define add10 (make-adder 10))").unwrap();
    println!("Defined: (make-adder n) and (add10)");
    
    let result = eval.eval_str("(add10 5)").unwrap();
    print_result(&lisp, result, "(add10 5)");

    // Arena statistics
    let result = eval.eval_str("(arena-stats)").unwrap();
    print_result(&lisp, result, "(arena-stats) ; (capacity allocated free usage%)");

    println!("\n✓ All examples completed successfully!");
}

fn print_result<const N: usize>(lisp: &Lisp<N>, result: grift::ArenaIndex, expr: &str) {
    let _value = lisp.get(result).unwrap();
    let formatted = format_value(lisp, result);
    println!("{} => {}", expr, formatted);
}

fn format_value<const N: usize>(lisp: &Lisp<N>, idx: grift::ArenaIndex) -> String {
    let mut buf = String::new();
    format_value_impl(lisp, idx, &mut buf, 0);
    buf
}

fn format_value_impl<const N: usize>(lisp: &Lisp<N>, idx: grift::ArenaIndex, buf: &mut String, depth: usize) {
    if depth > 50 {
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
        Ok(Value::Float(f)) => {
            use std::fmt::Write;
            write!(buf, "{}", f).unwrap();
        }
        Ok(Value::Char(c)) => {
            buf.push_str("#\\");
            buf.push(c);
        }
        Ok(Value::Cons { .. }) => {
            buf.push('(');
            format_list(lisp, idx, buf, depth + 1);
            buf.push(')');
        }
        Ok(Value::Lambda(_)) => buf.push_str("#<lambda>"),
        Ok(Value::Builtin(b)) => {
            buf.push_str("#<builtin:");
            buf.push_str(b.name());
            buf.push('>');
        }
        Ok(Value::StdLib(func)) => {
            buf.push_str("#<stdlib:");
            buf.push_str(func.name());
            buf.push('>');
        }
        _ => buf.push_str("#<value>"),
    }
}

fn format_list<const N: usize>(lisp: &Lisp<N>, mut idx: grift::ArenaIndex, buf: &mut String, depth: usize) {
    let mut first = true;
    loop {
        match lisp.get(idx) {
            Ok(Value::Nil) => break,
            Ok(Value::Cons { car, cdr }) => {
                if !first { buf.push(' '); }
                first = false;
                format_value_impl(lisp, car, buf, depth);
                idx = cdr;
            }
            _ => {
                buf.push_str(" . ");
                format_value_impl(lisp, idx, buf, depth);
                break;
            }
        }
    }
}
