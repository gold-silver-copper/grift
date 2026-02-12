;;; Closure and higher-order function tests
;;; Migrated from lib_tests.rs and first_class_procedures_tests.rs

(test-begin "closures")

;; Basic closure
(define (make-adder n)
  (lambda (x) (+ x n)))
(define add5 (make-adder 5))
(test-equal "basic-closure" 15 (add5 10))

;; Closure captures mutable state
(define (make-counter)
  (let ((count 0))
    (lambda ()
      (set! count (+ count 1))
      count)))
(define counter (make-counter))
(test-equal "counter-1" 1 (counter))
(test-equal "counter-2" 2 (counter))
(test-equal "counter-3" 3 (counter))

;; Higher-order functions
(test-equal "map-lambda" '(2 4 6)
  (map (lambda (x) (* x 2)) '(1 2 3)))

(test-equal "filter-lambda" '(2 4)
  (filter (lambda (x) (even? x)) '(1 2 3 4 5)))

;; Compose-style
(define (compose f g)
  (lambda (x) (f (g x))))
(define inc-then-double (compose (lambda (x) (* x 2)) (lambda (x) (+ x 1))))
(test-equal "compose" 12 (inc-then-double 5))

;; Functions as arguments
(define (apply-twice f x)
  (f (f x)))
(test-equal "apply-twice" 19 (apply-twice (lambda (x) (+ x 3)) 13))

;; Mutual recursion via letrec
(test-equal "mutual-recursion" #t
  (letrec ((even? (lambda (n) (if (= n 0) #t (odd? (- n 1)))))
           (odd? (lambda (n) (if (= n 0) #f (even? (- n 1))))))
    (even? 100)))

;; Currying
(define (curry f)
  (lambda (a)
    (lambda (b) (f a b))))
(test-equal "curry" 7 (((curry +) 3) 4))

;; Accumulator
(define (make-accumulator init)
  (let ((total init))
    (lambda (amount)
      (set! total (+ total amount))
      total)))
(define acc (make-accumulator 100))
(test-equal "accumulator-1" 125 (acc 25))
(test-equal "accumulator-2" 175 (acc 50))

;; Case-lambda
(define f
  (case-lambda
    (() 0)
    ((x) x)
    ((x y) (+ x y))
    ((x y z) (+ x y z))))
(test-equal "case-lambda-0" 0 (f))
(test-equal "case-lambda-1" 5 (f 5))
(test-equal "case-lambda-2" 7 (f 3 4))
(test-equal "case-lambda-3" 12 (f 3 4 5))

(test-end)
