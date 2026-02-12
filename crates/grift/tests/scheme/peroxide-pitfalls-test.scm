;;; Peroxide R5RS Pitfalls Tests
;;; Migrated from peroxide_pitfalls_tests.rs
;;;
;;; These tests check for edge cases and pitfalls in R5RS Scheme implementations.
;;; Source: https://github.com/MattX/peroxide/tree/master/tests/scheme

(test-begin "peroxide-pitfalls")

;; Section 4: No identifiers are reserved

;; 4.1: lambda can be used as a parameter name
;; ((lambda lambda lambda) 'x) => (x)
(test-assert "no-reserved-identifiers-lambda-as-param"
  (pair? ((lambda lambda lambda) 'x)))
(test-assert "no-reserved-identifiers-lambda-as-param-value"
  (eq? (car ((lambda lambda lambda) 'x)) 'x))

;; 4.2: begin can be used as a parameter
;; When begin is a parameter, (begin 1 2 3) calls the function stored in begin
(test-assert "no-reserved-identifiers-begin-as-param-is-pair"
  (pair? ((lambda (begin) (begin 1 2 3)) (lambda lambda lambda))))
(test-equal "no-reserved-identifiers-begin-as-param-first-element" 1
  (car ((lambda (begin) (begin 1 2 3)) (lambda lambda lambda))))
(test-equal "no-reserved-identifiers-begin-as-param-length" 3
  (length ((lambda (begin) (begin 1 2 3)) (lambda lambda lambda))))

;; 4.3: quote can be shadowed
(test-assert "no-reserved-identifiers-quote-shadowed"
  (not (let ((quote -)) (eqv? '1 1))))

;; Section 5: #f/() distinctness

;; 5.1: #f and () must be distinct (eq?)
(test-assert "false-nil-distinct-eq"
  (not (eq? #f '())))

;; 5.2: #f and () must be distinct (eqv?)
(test-assert "false-nil-distinct-eqv"
  (not (eqv? #f '())))

;; 5.3: #f and () must be distinct (equal?)
(test-assert "false-nil-distinct-equal"
  (not (equal? #f '())))

;; Section 8: Miscellaneous

;; 8.1: The Petrofsky let test - named-let with - as loop name
(test-equal "petrofsky-let-test" -1
  (let - ((n (- 1))) n))

(test-end)
