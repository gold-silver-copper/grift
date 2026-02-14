use grift::{Evaluator, Lisp};

fn eval_expr(expr: &str) -> String {
    let lisp = Box::new(Lisp::<100000>::new());
    let lisp = Box::leak(lisp);
    let mut eval = Evaluator::new(lisp).unwrap();
    match eval.eval_str(expr) {
        Ok(idx) => format!("{}", lisp.display(idx)),
        Err(e) => format!("ERROR: {:?}", e),
    }
}

fn eval_multi(exprs: &[&str]) -> String {
    let lisp = Box::new(Lisp::<100000>::new());
    let lisp = Box::leak(lisp);
    let mut eval = Evaluator::new(lisp).unwrap();
    let mut last = String::new();
    for expr in exprs {
        match eval.eval_str(expr) {
            Ok(idx) => last = format!("{}", lisp.display(idx)),
            Err(e) => return format!("ERROR: {:?}", e),
        }
    }
    last
}

fn main() {
    // Test #103: let-syntax hygiene - should be 'outer
    println!("Test #103: {}", eval_expr(
        "(let ((x 'outer)) (let-syntax ((m (syntax-rules () ((m) x)))) (let ((x 'inner)) (m))))"
    ));

    // Test #104: letrec-syntax my-or 
    println!("Test #104: {}", eval_expr(
        "(letrec-syntax ((my-or (syntax-rules () ((my-or) #f) ((my-or e) e) ((my-or e1 e2 ...) (let ((temp e1)) (if temp temp (my-or e2 ...))))))) (let ((x #f) (y 7) (temp 8) (let odd?) (if even?)) (my-or x (let temp) (if y) y)))"
    ));

    // Test #113: part-2x with improper tail
    println!("Test #113: {}", eval_multi(&[
        "(define-syntax part-2x (syntax-rules () ((_ (a b (m n) ... x y . rest)) (vector (list a b) (list m ...) (list n ...) (list x y) (cons \"rest:\" 'rest))) ((_ . rest) 'error)))",
        "(part-2x (10 (+ 21 22) (31 32) (41 42) (51 52) (+ 61 2) 77 . \"tail\"))"
    ]));

    // Test #118: jabberwocky
    println!("Test #118: {}", eval_multi(&[
        "(define-syntax jabberwocky (syntax-rules () ((_ hatter) (begin (define march-hare 42) (define-syntax hatter (syntax-rules () ((_) march-hare)))))))",
        "(jabberwocky mad-hatter)",
        "(mad-hatter)"
    ]));

    // Test #120: let-syntax scope
    println!("Test #120: {}", eval_expr(
        "(let () (define x 1) (let-syntax () (define x 2) #f) x)"
    ));

    // Test #121: foo bar y
    println!("Test #121: {}", eval_multi(&[
        "(define-syntax foo (syntax-rules () ((foo bar y) (define-syntax bar (syntax-rules () ((bar x) 'y))))))",
        "(foo bar x)",
        "(bar 1)"
    ]));

    // Test #125: bound-identifier=?
    println!("Test #125: {}", eval_expr(
        "(let-syntax ((m (syntax-rules () ((m x) (let-syntax ((n (syntax-rules (k) ((n x) 'bound-identifier=?) ((n y) 'free-identifier=?)))) (n z)))))) (m k))"
    ));

    // Test #126: ellipsis as literal
    println!("Test #126: {}", eval_multi(&[
        "(define-syntax elli-lit-1 (syntax-rules ... (...) ((_ x) '(x ...))))",
        "(elli-lit-1 100)"
    ]));

    // Test #896: read-bytevector
    println!("Test #896: {}", eval_expr(
        "(equal? #u8(1 2 3) (read-bytevector 3 (open-input-bytevector #u8(1 2 3))))"
    ));

    // Test #899
    println!("Test #899: {}", eval_expr(
        "(let ((bv (read-bytevector 10 (open-input-bytevector #u8(1 2 3 4 5 6 7 8 9 10))))) (equal? #u8(6 7 8 9 10) (bytevector-copy bv 5)))"
    ));
    
    // Simpler test for #899
    println!("Test #899 simple: {}", eval_expr(
        "(read-bytevector 10 (open-input-bytevector #u8(1 2 3 4 5 6 7 8 9 10)))"
    ));

    // Test #913: datum labels
    println!("Test #913: {}", eval_expr(
        "(read (open-input-string \"#0=(1 . #0#)\"))"
    ));

    // Test #914
    println!("Test #914: {}", eval_expr(
        "(read (open-input-string \"(#0=(1 2 3) #0#)\"))"
    ));
}
