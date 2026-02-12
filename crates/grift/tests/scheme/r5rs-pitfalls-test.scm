;; R5RS Pitfalls Tests adapted from chicken-scheme
;; (https://github.com/alaricsp/chicken-scheme/blob/master/tests/r5rs_pitfalls.scm)
;;
;; These tests check for edge cases and pitfalls in R5RS Scheme implementations.
;; Original code collected from public forums, adapted for Grift.

(test-begin "r5rs-pitfalls")

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 4: No identifiers are reserved
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "lambda-as-parameter"
  (let ((result ((lambda lambda lambda) 'x)))
    (and (pair? result) (eq? (car result) 'x))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 5: #f/() distinctness
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "false-nil-distinct-eq"
  (not (eq? #f '())))

(test-assert "false-nil-distinct-eqv"
  (not (eqv? #f '())))

(test-assert "false-nil-distinct-equal"
  (not (equal? #f '())))

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 8: Miscellaneous
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "petrofsky-let" -1
  (let - ((n (- 1))) n))

(test-equal "append-shares-structure" 9
  (length (let ((ls (list 1 2 3 4)))
            (append (append ls ls) '(5)))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Additional tests for Scheme conformance
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "empty-list-is-truthy" 1
  (if '() 1 2))

(test-equal "zero-is-truthy" 1
  (if 0 1 2))

(test-equal "empty-string-is-truthy" 1
  (if "" 1 2))

(test-equal "nested-defines" 30
  (let ()
    (define x 10)
    (define y 20)
    (+ x y)))

(test-assert "letrec-mutual-recursion"
  (letrec ((even? (lambda (n)
                    (if (= n 0) #t (odd? (- n 1)))))
           (odd? (lambda (n)
                   (if (= n 0) #f (even? (- n 1))))))
    (even? 10)))

(test-equal "set-car" 10
  (let ((p (cons 1 2)))
    (set-car! p 10)
    (car p)))

(test-equal "set-cdr" 20
  (let ((p (cons 1 2)))
    (set-cdr! p 20)
    (cdr p)))

(test-equal "begin-returns-last" 5
  (begin 1 2 3 4 5))

(test-equal "begin-with-side-effects" 3
  (let ((x 0))
    (begin (set! x 1) (set! x 2) (set! x 3) x)))

(test-equal "lambda-body-defines-counter-1" 1
  (let ()
    (define (make-counter)
      (define count 0)
      (lambda ()
        (set! count (+ count 1))
        count))
    (define c (make-counter))
    (c)))

(test-equal "lambda-body-defines-counter-2" 2
  (let ()
    (define (make-counter)
      (define count 0)
      (lambda ()
        (set! count (+ count 1))
        count))
    (define c (make-counter))
    (c)
    (c)))

(test-equal "lambda-body-defines-counter-3" 3
  (let ()
    (define (make-counter)
      (define count 0)
      (lambda ()
        (set! count (+ count 1))
        count))
    (define c (make-counter))
    (c)
    (c)
    (c)))

(test-equal "call-with-values-basic" 6
  (call-with-values (lambda () (values 1 2 3)) (lambda (a b c) (+ a b c))))

(test-equal "variadic-lambda-sum" 15
  (let ()
    (define (sum-all . nums) (apply + nums))
    (sum-all 1 2 3 4 5)))

(test-equal "variadic-lambda-empty" 0
  (let ()
    (define (sum-all . nums) (apply + nums))
    (sum-all)))

(test-equal "list-tail-car" 3
  (car (list-tail '(1 2 3 4 5) 2)))

(test-equal "list-tail-length" 3
  (length (list-tail '(1 2 3 4 5) 2)))

(test-assert "filter-returns-pair"
  (pair? (filter (lambda (x) (> x 2)) '(1 2 3 4 5))))

(test-equal "filter-length" 3
  (length (filter (lambda (x) (> x 2)) '(1 2 3 4 5))))

(test-equal "reduce-sum" 15
  (reduce + 0 '(1 2 3 4 5)))

(test-equal "reduce-product" 120
  (reduce * 1 '(1 2 3 4 5)))

(test-assert "caar-test"
  (eq? (caar '((a b) (c d) (e f))) 'a))

(test-assert "cadr-is-pair"
  (pair? (cadr '((a b) (c d) (e f)))))

(test-assert "cddr-is-pair"
  (pair? (cddr '((a b) (c d) (e f)))))

(test-equal "expt-zero-power" 1
  (expt 2 0))

(test-equal "expt-2-to-10" 1024
  (expt 2 10))

(test-equal "expt-3-to-4" 81
  (expt 3 4))

(test-equal "min-of-list" 1
  (min 3 1 4 1 5 9))

(test-equal "max-of-list" 9
  (max 3 1 4 1 5 9))

(test-assert "even-zero" (even? 0))
(test-assert "even-four" (even? 4))
(test-assert "not-even-three" (not (even? 3)))

(test-assert "not-odd-zero" (not (odd? 0)))
(test-assert "not-odd-four" (not (odd? 4)))
(test-assert "odd-three" (odd? 3))

(test-assert "zero-predicate-true" (zero? 0))
(test-assert "zero-predicate-false" (not (zero? 1)))

(test-assert "positive-one" (positive? 1))
(test-assert "not-positive-zero" (not (positive? 0)))
(test-assert "not-positive-neg" (not (positive? -1)))

(test-assert "negative-neg-one" (negative? -1))
(test-assert "not-negative-zero" (not (negative? 0)))
(test-assert "not-negative-one" (not (negative? 1)))

(test-equal "list-copy-car" 1
  (let ()
    (define original '(1 2 3))
    (define copy (list-copy original))
    (car copy)))

(test-equal "list-copy-length" 3
  (let ()
    (define original '(1 2 3))
    (define copy (list-copy original))
    (length copy)))

(test-assert "list-copy-equal"
  (let ()
    (define original '(1 2 3))
    (define copy (list-copy original))
    (equal? original copy)))

(test-assert "list-copy-not-eq"
  (let ()
    (define original '(1 2 3))
    (define copy (list-copy original))
    (not (eq? original copy))))

(test-end)
