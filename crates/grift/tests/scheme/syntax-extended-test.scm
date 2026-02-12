;;; Extended Syntax Tests for Grift
;;; Migrated from syntax_extended_tests.rs

(test-begin "syntax-extended")

;; ═══════════════════════════════════════════════════════════════════════════
;; SYNTAX-CASE EXTENDED TESTS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_syntax_case_empty_pattern
(define-syntax always-42
  (lambda (x)
    (syntax-case x ()
      ((_) (syntax 42)))))

(test-equal "syntax-case-empty-pattern" 42 (always-42))

;; test_syntax_case_single_pattern
(define-syntax double
  (lambda (stx)
    (syntax-case stx ()
      ((_ x) (syntax (* x 2))))))

(test-equal "syntax-case-single-pattern" 10 (double 5))
(test-equal "syntax-case-single-pattern-expr" 6 (double (+ 1 2)))

;; test_syntax_case_multiple_patterns
(define-syntax my-add
  (lambda (stx)
    (syntax-case stx ()
      ((_ a b) (syntax (+ a b))))))

(test-equal "syntax-case-multiple-patterns" 7 (my-add 3 4))

;; test_syntax_case_multiple_clauses
(define-syntax my-add2
  (lambda (stx)
    (syntax-case stx ()
      ((_ a) (syntax a))
      ((_ a b) (syntax (+ a b)))
      ((_ a b c) (syntax (+ a b c))))))

(test-equal "syntax-case-multiple-clauses-1" 5 (my-add2 5))
(test-equal "syntax-case-multiple-clauses-2" 15 (my-add2 5 10))
(test-equal "syntax-case-multiple-clauses-3" 30 (my-add2 5 10 15))

;; test_syntax_case_ellipsis
(define-syntax my-list
  (lambda (stx)
    (syntax-case stx ()
      ((_ x ...) (syntax (list x ...))))))

(test-equal "syntax-case-ellipsis-length" 5 (length (my-list 1 2 3 4 5)))
(test-equal "syntax-case-ellipsis-car" 10 (car (my-list 10 20 30)))

;; test_syntax_case_nested_ellipsis — skipped (empty in Rust)

;; test_syntax_case_with_literals
(define-syntax with-value
  (lambda (stx)
    (syntax-case stx (is)
      ((_ name is value) (syntax (let ((name value)) name))))))

(test-equal "syntax-case-with-literals" 42 (with-value x is 42))

;; ═══════════════════════════════════════════════════════════════════════════
;; LET-SYNTAX AND LETREC-SYNTAX TESTS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_let_syntax_basic
(test-equal "let-syntax-basic" 11
  (let-syntax ((add1 (lambda (stx)
                       (syntax-case stx ()
                         ((_ x) (syntax (+ x 1)))))))
    (add1 10)))

;; test_let_syntax_scoping
(define outer-value 100)

(test-equal "let-syntax-scoping" 100
  (let-syntax ((get-outer (lambda (stx)
                            (syntax-case stx ()
                              ((_) (syntax outer-value))))))
    (get-outer)))

;; test_let_syntax_nested
(test-equal "let-syntax-nested" 30
  (let-syntax ((outer (lambda (stx)
                        (syntax-case stx ()
                          ((_ x) (syntax (* x 2)))))))
    (let-syntax ((inner (lambda (stx)
                          (syntax-case stx ()
                            ((_ x) (syntax (+ x 10)))))))
      (outer (inner 5)))))

;; test_letrec_syntax_basic — skipped (placeholder in Rust)
;; test_identifier_check — skipped (empty in Rust)

;; ═══════════════════════════════════════════════════════════════════════════
;; DEFINE-SYNTAX AT TOP LEVEL
;; ═══════════════════════════════════════════════════════════════════════════

;; test_define_syntax_top_level
(define-syntax square
  (lambda (stx)
    (syntax-case stx ()
      ((_ x) (syntax (* x x))))))

(test-equal "define-syntax-top-level" 25 (square 5))
(test-equal "define-syntax-top-level-expr" 9 (square (+ 1 2)))

;; test_define_syntax_shadowing
(define inc (lambda (x) (+ x 1)))
(test-equal "define-syntax-shadowing-fn" 11 (inc 10))

(define-syntax inc-macro
  (lambda (stx)
    (syntax-case stx ()
      ((_ x) (syntax (+ x 100))))))

(test-equal "define-syntax-shadowing-macro" 110 (inc-macro 10))
(test-equal "define-syntax-shadowing-fn-still-works" 11 (inc 10))

;; ═══════════════════════════════════════════════════════════════════════════
;; COMPLEX MACRO PATTERNS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_macro_with_begin
(define-syntax do-both
  (lambda (stx)
    (syntax-case stx ()
      ((_ a b) (syntax (begin a b))))))

(define do-both-x 0)
(do-both (set! do-both-x 10) (set! do-both-x (+ do-both-x 5)))
(test-equal "macro-with-begin" 15 do-both-x)

;; test_macro_with_lambda
(define-syntax make-adder
  (lambda (stx)
    (syntax-case stx ()
      ((_ n) (syntax (lambda (x) (+ x n)))))))

(define add5 (make-adder 5))
(test-equal "macro-with-lambda" 15 (add5 10))

;; test_macro_recursive_expansion
(test-assert "macro-recursive-and-true" (and #t #t #t))
(test-assert "macro-recursive-and-false" (not (and #t #f #t)))
(test-assert "macro-recursive-and-empty" (and))

;; ═══════════════════════════════════════════════════════════════════════════
;; HYGIENE TESTS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_hygiene_basic
(define-syntax swap!
  (lambda (stx)
    (syntax-case stx ()
      ((_ a b)
       (syntax (let ((temp a))
                 (set! a b)
                 (set! b temp)))))))

(define temp 999)
(define swap-x 1)
(define swap-y 2)
(swap! swap-x swap-y)

(test-equal "hygiene-swap-x" 2 swap-x)
(test-equal "hygiene-swap-y" 1 swap-y)
(test-equal "hygiene-temp-preserved" 999 temp)

;; ═══════════════════════════════════════════════════════════════════════════
;; WHEN/UNLESS MACRO TESTS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_when_true
(define when-true-x 0)
(when #t (set! when-true-x 42))
(test-equal "when-true" 42 when-true-x)

;; test_when_false
(define when-false-x 0)
(when #f (set! when-false-x 42))
(test-equal "when-false" 0 when-false-x)

;; test_unless_true
(define unless-true-x 0)
(unless #t (set! unless-true-x 42))
(test-equal "unless-true" 0 unless-true-x)

;; test_unless_false
(define unless-false-x 0)
(unless #f (set! unless-false-x 42))
(test-equal "unless-false" 42 unless-false-x)

;; ═══════════════════════════════════════════════════════════════════════════
;; COND MACRO TESTS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_cond_first_true
(test-equal "cond-first-true" 1 (cond (#t 1) (#f 2)))

;; test_cond_second_true
(test-equal "cond-second-true" 2 (cond (#f 1) (#t 2)))

;; test_cond_else
(test-equal "cond-else" 3 (cond (#f 1) (#f 2) (else 3)))

;; test_cond_multiple_expressions
(define cond-y 0)
(cond (#t (set! cond-y 10) (set! cond-y (+ cond-y 5))))
(test-equal "cond-multiple-expressions" 15 cond-y)

;; ═══════════════════════════════════════════════════════════════════════════
;; CASE MACRO TESTS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_case_single_datum
(test-equal "case-single-datum-a" 1 (case 'a ((a) 1) ((b) 2)))
(test-equal "case-single-datum-b" 2 (case 'b ((a) 1) ((b) 2)))

;; test_case_multiple_data
(test-equal "case-multiple-data-b" 1 (case 'b ((a b c) 1) ((d e) 2)))
(test-equal "case-multiple-data-e" 2 (case 'e ((a b c) 1) ((d e) 2)))

;; test_case_else
(test-equal "case-else" 99 (case 'z ((a) 1) ((b) 2) (else 99)))

;; ═══════════════════════════════════════════════════════════════════════════
;; DO LOOP TESTS
;; ═══════════════════════════════════════════════════════════════════════════

;; test_do_basic — sum 1 to 5
(test-equal "do-basic" 15
  (do ((i 1 (+ i 1))
       (sum 0 (+ sum i)))
      ((> i 5) sum)))

;; test_do_with_body
(define do-counter 0)
(do ((i 0 (+ i 1)))
    ((>= i 3) do-counter)
  (set! do-counter (+ do-counter 1)))
(test-equal "do-with-body" 3 do-counter)

;; ═══════════════════════════════════════════════════════════════════════════
;; DEFINE-SYNTAX SHORTHAND FORM
;; ═══════════════════════════════════════════════════════════════════════════

;; test_define_syntax_shorthand_form
(define-syntax (my-ten stx)
  (syntax-case stx ()
    ((_) #'10)))

(test-equal "define-syntax-shorthand-form" 10 (my-ten))

;; test_define_syntax_shorthand_multiple_clauses
(define-syntax (my-val stx)
  (syntax-case stx (foo)
    ((_) #'10)
    ((_ (foo)) #'67)))

(test-equal "define-syntax-shorthand-multiple-clauses-1" 10 (my-val))
(test-equal "define-syntax-shorthand-multiple-clauses-2" 67 (my-val (foo)))

;; test_nested_define_syntax_via_syntax_rules
(define-syntax make-macro3
  (syntax-rules (foo bar baz)
    ((_ name)
     (define-syntax name
       (syntax-rules (foo bar baz)
         ((_) 100))))))

(make-macro3 my-hundred)
(test-equal "nested-define-syntax-via-syntax-rules" 100 (my-hundred))

;; test_nested_define_syntax_shorthand_via_syntax_rules
(define-syntax make-macro4
  (syntax-rules ()
    ((_ name)
     (define-syntax (name stx)
       (syntax-case stx (foo bar baz)
         ((_) #'10)
         ((_ (foo)) #'67))))))

(make-macro4 my-bar)
(test-equal "nested-define-syntax-shorthand-via-syntax-rules-1" 10 (my-bar))
(test-equal "nested-define-syntax-shorthand-via-syntax-rules-2" 67 (my-bar (foo)))

(test-end)
