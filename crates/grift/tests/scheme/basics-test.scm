;;; Basic evaluation tests
;;; Migrated from lib_tests.rs

(test-begin "basics")

;; Numbers
(test-equal "eval-number" 42 42)
(test-equal "eval-negative" -10 -10)

;; Booleans
(test-assert "true-is-true" #t)
(test-assert "false-is-false" (not #f))

;; Empty list is truthy (only #f is false in Scheme)
(test-equal "empty-list-truthy" 1 (if '() 1 2))
(test-equal "zero-is-truthy" 1 (if 0 1 2))
(test-equal "false-is-falsy" 2 (if #f 1 2))

;; Quote
(test-assert "quote-symbol" (symbol? 'hello))
(test-equal "quote-list" '(1 2 3) (quote (1 2 3)))

;; If
(test-equal "if-true" 1 (if #t 1 2))
(test-equal "if-false" 2 (if #f 1 2))
(test-equal "if-less-than" 10 (if (< 1 2) 10 20))

;; Define
(define x 42)
(test-equal "define-variable" 42 x)

;; Lambda
(test-equal "lambda-call" 6 ((lambda (x) (+ x 1)) 5))

;; Define function
(define (square x) (* x x))
(test-equal "define-function" 25 (square 5))

;; Let
(test-equal "let-binding" 30 (let ((x 10) (y 20)) (+ x y)))

;; Let*
(test-equal "let-star" 110 (let* ((x 10) (y (* x 11))) y))

;; Letrec
(test-equal "letrec-even-odd" #t
  (letrec ((even? (lambda (n) (if (= n 0) #t (odd? (- n 1)))))
           (odd? (lambda (n) (if (= n 0) #f (even? (- n 1))))))
    (even? 10)))

;; Begin
(test-equal "begin-returns-last" 3 (begin 1 2 3))

;; Nested define
(test-equal "nested-define" 30
  (let ()
    (define a 10)
    (define b 20)
    (+ a b)))

;; Boolean predicates
(test-assert "boolean-true" (boolean? #t))
(test-assert "boolean-false" (boolean? #f))
(test-assert "not-boolean-number" (not (boolean? 42)))

;; Equality
(test-assert "eq-same-symbol" (eq? 'a 'a))
(test-assert "eqv-numbers" (eqv? 42 42))
(test-equal "equal-lists" #t (equal? '(1 2 3) '(1 2 3)))

(test-end)
