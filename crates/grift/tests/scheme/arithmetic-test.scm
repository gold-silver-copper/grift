;;; Arithmetic tests
;;; Migrated from lib_tests.rs

(test-begin "arithmetic")

;; Basic operations
(test-equal "addition" 3 (+ 1 2))
(test-equal "subtraction" 7 (- 10 3))
(test-equal "multiplication" 20 (* 4 5))
(test-equal "division" 5 (/ 20 4))
(test-equal "multi-add" 10 (+ 1 2 3 4))

;; Nested arithmetic
(test-equal "nested-add-mul" 14 (+ (* 2 3) (* 2 4)))
(test-equal "nested-complex" 25 (+ (* 3 5) (- 20 10)))

;; Comparison
(test-assert "less-than" (< 1 2))
(test-assert "greater-than" (> 3 2))
(test-assert "less-equal" (<= 2 2))
(test-assert "greater-equal" (>= 3 3))
(test-equal "equal-nums" #t (= 5 5))
(test-equal "not-equal" #f (= 5 6))

;; Numeric predicates
(test-assert "zero-pred" (zero? 0))
(test-assert "positive-pred" (positive? 5))
(test-assert "negative-pred" (negative? -3))
(test-assert "number-pred" (number? 42))
(test-assert "integer-pred" (integer? 42))

;; Min/Max
(test-equal "min-two" 1 (min 1 2))
(test-equal "max-two" 2 (max 1 2))
(test-equal "min-multi" 1 (min 3 1 4 1 5))
(test-equal "max-multi" 5 (max 3 1 4 1 5))

;; Abs
(test-equal "abs-positive" 5 (abs 5))
(test-equal "abs-negative" 5 (abs -5))
(test-equal "abs-zero" 0 (abs 0))

;; Modulo / Remainder
(test-equal "modulo" 1 (modulo 10 3))
(test-equal "remainder" 1 (remainder 10 3))

;; Exact/inexact
(test-assert "exact-integer" (exact? 42))

;; Even/odd
(test-assert "even-4" (even? 4))
(test-assert "odd-3" (odd? 3))
(test-assert "not-even-3" (not (even? 3)))
(test-assert "not-odd-4" (not (odd? 4)))

;; GCD / LCM
(test-equal "gcd-basic" 4 (gcd 8 12))
(test-equal "lcm-basic" 12 (lcm 4 6))

(test-end)
