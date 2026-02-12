;;; Control flow tests
;;; Migrated from lib_tests.rs and syntax_proper_tests.rs

(test-begin "control")

;; Cond
(test-equal "cond-first" 1 (cond (#t 1) (#f 2)))
(test-equal "cond-second" 2 (cond (#f 1) (#t 2)))
(test-equal "cond-else" 3 (cond (#f 1) (#f 2) (else 3)))

;; Case
(test-equal "case-match" 1
  (case (+ 1 1)
    ((1) 0)
    ((2) 1)
    ((3) 2)))

(test-equal "case-else" 99
  (case 42
    ((1) 0)
    ((2) 1)
    (else 99)))

;; And / Or
(test-equal "and-true" 3 (and 1 2 3))
(test-assert "and-false" (not (and 1 #f 3)))
(test-equal "and-empty" #t (and))

(test-equal "or-first-true" 1 (or 1 2 3))
(test-equal "or-false-then-true" 2 (or #f 2 3))
(test-assert "or-all-false" (not (or #f #f #f)))

;; When / Unless
(test-equal "when-true" 42
  (let ((x 0))
    (when #t (set! x 42))
    x))

(test-equal "when-false" 0
  (let ((x 0))
    (when #f (set! x 42))
    x))

(test-equal "unless-true" 0
  (let ((x 0))
    (unless #t (set! x 42))
    x))

(test-equal "unless-false" 42
  (let ((x 0))
    (unless #f (set! x 42))
    x))

;; Do loop
(test-equal "do-loop-sum" 10
  (do ((i 0 (+ i 1))
       (sum 0 (+ sum i)))
      ((= i 5) sum)))

;; Recursion
(define (factorial n)
  (if (= n 0) 1 (* n (factorial (- n 1)))))
(test-equal "factorial-5" 120 (factorial 5))
(test-equal "factorial-0" 1 (factorial 0))

;; Tail recursion
(define (loop-sum n acc)
  (if (= n 0) acc (loop-sum (- n 1) (+ acc n))))
(test-equal "tail-call-sum" 5050 (loop-sum 100 0))

;; Named let
(test-equal "named-let-sum" 10
  (let loop ((i 0) (sum 0))
    (if (= i 5)
        sum
        (loop (+ i 1) (+ sum i)))))

;; Multiple values
(test-equal "values-receive" 3
  (call-with-values
    (lambda () (values 1 2))
    +))

;; Guard (exception handling)
(test-equal "guard-catch" 42
  (guard (exn (#t 42))
    (error "test error")))

;; Dynamic-wind
(test-equal "dynamic-wind" '(in body out)
  (let ((result '()))
    (dynamic-wind
      (lambda () (set! result (cons 'in result)))
      (lambda () (set! result (cons 'body result)))
      (lambda () (set! result (cons 'out result))))
    (reverse result)))

(test-end)
