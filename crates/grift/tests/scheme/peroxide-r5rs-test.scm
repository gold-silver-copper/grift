;;; Peroxide R5RS Tests
;;; Migrated from peroxide_r5rs_tests.rs
;;;
;;; Based on chibi-scheme's R5RS test suite.
;;; Source: https://github.com/MattX/peroxide/tree/master/tests/scheme

(test-begin "peroxide-r5rs")

;; Basic lambda

(test-equal "lambda-double" 8
  ((lambda (x) (+ x x)) 4))

(test-equal "lambda-variadic-length" 4
  (length ((lambda x x) 3 4 5 6)))

(test-equal "lambda-rest-args-length" 2
  (length ((lambda (x y . z) z) 3 4 5 6)))

;; If and cond

(test-assert "if-greater-yes"
  (eq? (if (> 3 2) 'yes 'no) 'yes))

(test-assert "if-greater-no"
  (eq? (if (> 2 3) 'yes 'no) 'no))

(test-equal "if-arithmetic" 1
  (if (> 3 2) (- 3 2) (+ 3 2)))

(test-assert "cond-greater"
  (eq? (cond ((> 3 2) 'greater) ((< 3 2) 'less)) 'greater))

;; And / Or

(test-assert "and-true"
  (and (= 2 2) (> 2 1)))

(test-assert "and-false"
  (not (and (= 2 2) (< 2 1))))

(test-assert "or-both-true"
  (or (= 2 2) (> 2 1)))

(test-assert "or-first-true"
  (or (= 2 2) (< 2 1)))

;; Let and let*

(test-equal "let-basic" 6
  (let ((x 2) (y 3)) (* x y)))

;; In standard R5RS, regular let evaluates all bindings in the outer scope
;; So z = (+ 2 3) = 5, then x=7, result = (* 5 7) = 35
(test-equal "let-parallel-binding" 35
  (let ((x 2) (y 3))
    (let ((x 7) (z (+ x y)))
      (* z x))))

;; In let*, bindings are sequential
;; So x=7 first, then z = (+ 7 3) = 10, result = (* 10 7) = 70
(test-equal "let-star-sequential-binding" 70
  (let ((x 2) (y 3))
    (let* ((x 7) (z (+ x y)))
      (* z x))))

;; List operations

(test-equal "length-simple" 3
  (length '(a b c)))

(test-equal "length-nested" 3
  (length '(a (b) (c d e))))

(test-equal "length-empty" 0
  (length '()))

(test-end)
