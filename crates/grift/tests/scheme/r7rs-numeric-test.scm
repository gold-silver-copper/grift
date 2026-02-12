;;; Tests for R7RS §6.2.6 numeric procedures
;;; Migrated from r7rs_numeric_tests.rs

(test-begin "r7rs-numeric")

;; ============================================================================
;; Type Predicates (R7RS §6.2.6)
;; ============================================================================

;; complex?
(test-assert "complex?-integer" (complex? 3))
(test-assert "complex?-float" (complex? 3.5))
(test-assert "complex?-zero" (complex? 0))
(test-assert "complex?-string-false" (not (complex? "hello")))
(test-assert "complex?-boolean-false" (not (complex? #t)))

;; real?
(test-assert "real?-integer" (real? 3))
(test-assert "real?-float" (real? 3.5))
(test-assert "real?-negative" (real? -1))
(test-assert "real?-string-false" (not (real? "hello")))

;; rational?
(test-assert "rational?-integer" (rational? 3))
(test-assert "rational?-float" (rational? 3.5))
(test-assert "rational?-zero" (rational? 0))
(test-assert "rational?-string-false" (not (rational? "hello")))

;; integer?
(test-assert "integer?-integer" (integer? 3))
(test-assert "integer?-float-whole" (integer? 3.0))
(test-assert "integer?-float-frac-false" (not (integer? 3.5)))
(test-assert "integer?-string-false" (not (integer? "hello")))

;; ============================================================================
;; Division Procedures (R7RS §6.2.6)
;; ============================================================================

;; floor-quotient
(test-equal "floor-quotient-5-2" 2 (floor-quotient 5 2))
(test-equal "floor-quotient-neg5-2" -3 (floor-quotient -5 2))
(test-equal "floor-quotient-5-neg2" -3 (floor-quotient 5 -2))
(test-equal "floor-quotient-neg5-neg2" 2 (floor-quotient -5 -2))
(test-equal "floor-quotient-10-5" 2 (floor-quotient 10 5))

;; floor-remainder
(test-equal "floor-remainder-5-2" 1 (floor-remainder 5 2))
(test-equal "floor-remainder-neg5-2" 1 (floor-remainder -5 2))
(test-equal "floor-remainder-5-neg2" -1 (floor-remainder 5 -2))
(test-equal "floor-remainder-neg5-neg2" -1 (floor-remainder -5 -2))

;; floor/
(test-equal "floor/-5-2" '(2 1) (floor/ 5 2))
(test-equal "floor/-neg5-2" '(-3 1) (floor/ -5 2))

;; truncate-quotient
(test-equal "truncate-quotient-5-2" 2 (truncate-quotient 5 2))
(test-equal "truncate-quotient-neg5-2" -2 (truncate-quotient -5 2))
(test-equal "truncate-quotient-5-neg2" -2 (truncate-quotient 5 -2))
(test-equal "truncate-quotient-neg5-neg2" 2 (truncate-quotient -5 -2))

;; truncate-remainder
(test-equal "truncate-remainder-5-2" 1 (truncate-remainder 5 2))
(test-equal "truncate-remainder-neg5-2" -1 (truncate-remainder -5 2))
(test-equal "truncate-remainder-5-neg2" 1 (truncate-remainder 5 -2))
(test-equal "truncate-remainder-neg5-neg2" -1 (truncate-remainder -5 -2))

;; truncate/
(test-equal "truncate/-5-2" '(2 1) (truncate/ 5 2))
(test-equal "truncate/-neg5-2" '(-2 -1) (truncate/ -5 2))

;; division invariant: n = d*q + r
(test-assert "floor-division-invariant-positive"
  (let ((q (floor-quotient 7 3)) (r (floor-remainder 7 3)))
    (= 7 (+ (* 3 q) r))))
(test-assert "floor-division-invariant-negative"
  (let ((q (floor-quotient -7 3)) (r (floor-remainder -7 3)))
    (= -7 (+ (* 3 q) r))))
(test-assert "truncate-division-invariant-positive"
  (let ((q (truncate-quotient 7 3)) (r (truncate-remainder 7 3)))
    (= 7 (+ (* 3 q) r))))
(test-assert "truncate-division-invariant-negative"
  (let ((q (truncate-quotient -7 3)) (r (truncate-remainder -7 3)))
    (= -7 (+ (* 3 q) r))))

;; ============================================================================
;; Rational Number Operations (R7RS §6.2.6)
;; ============================================================================

;; numerator exact
(test-equal "numerator-6" 6 (numerator 6))
(test-equal "numerator-0" 0 (numerator 0))
(test-equal "numerator-neg3" -3 (numerator -3))

;; denominator exact
(test-equal "denominator-6" 1 (denominator 6))
(test-equal "denominator-0" 1 (denominator 0))

;; numerator inexact
(test-assert "numerator-0.5" (= (numerator 0.5) 1.0))
(test-assert "numerator-0.5-inexact" (inexact? (numerator 0.5)))

;; denominator inexact
(test-assert "denominator-0.5" (= (denominator 0.5) 2.0))
(test-assert "denominator-0.5-inexact" (inexact? (denominator 0.5)))

;; rationalize
(test-equal "rationalize-0" 0 (rationalize 0 1))

;; ============================================================================
;; Exact Integer Square Root (R7RS §6.2.6)
;; ============================================================================

(test-equal "exact-integer-sqrt-4" '(2 0) (exact-integer-sqrt 4))
(test-equal "exact-integer-sqrt-5" '(2 1) (exact-integer-sqrt 5))
(test-equal "exact-integer-sqrt-0" '(0 0) (exact-integer-sqrt 0))
(test-equal "exact-integer-sqrt-1" '(1 0) (exact-integer-sqrt 1))
(test-equal "exact-integer-sqrt-15" '(3 6) (exact-integer-sqrt 15))
(test-equal "exact-integer-sqrt-16" '(4 0) (exact-integer-sqrt 16))

;; ============================================================================
;; Transcendental Functions (R7RS §6.2.6)
;; ============================================================================

;; exp
(test-assert "exp-0" (= (exp 0) 1.0))
(test-assert "exp-1-lower" (> (exp 1) 2.718))
(test-assert "exp-1-upper" (< (exp 1) 2.719))

;; log
(test-assert "log-1" (= (log 1) 0.0))
(test-approximate "log-e" 1.0 (log (exp 1)) 0.0001)

;; log with base
(test-approximate "log-100-base-10" 2.0 (log 100 10) 0.0001)
(test-approximate "log-8-base-2" 3.0 (log 8 2) 0.0001)

;; sin
(test-assert "sin-0" (= (sin 0) 0.0))
(test-assert "sin-1-lower" (> (sin 1) 0.84))
(test-assert "sin-1-upper" (< (sin 1) 0.85))

;; cos
(test-assert "cos-0" (= (cos 0) 1.0))

;; tan
(test-assert "tan-0" (= (tan 0) 0.0))

;; asin
(test-assert "asin-0" (= (asin 0) 0.0))
(test-assert "asin-0.5-lower" (> (asin 0.5) 0.523))
(test-assert "asin-0.5-upper" (< (asin 0.5) 0.524))

;; acos
(test-assert "acos-1" (= (acos 1) 0.0))

;; atan one arg
(test-assert "atan-0" (= (atan 0) 0.0))
(test-assert "atan-1-lower" (> (atan 1) 0.785))
(test-assert "atan-1-upper" (< (atan 1) 0.786))

;; atan two args
(test-assert "atan-1-1-lower" (> (atan 1 1) 0.785))
(test-assert "atan-1-1-upper" (< (atan 1 1) 0.786))

;; ============================================================================
;; Complex Number Operations (R7RS §6.2.6)
;; ============================================================================

;; make-rectangular
(test-assert "make-rectangular-pure-real" (= (make-rectangular 3 0) 3.0))
(test-assert "make-rectangular-is-number" (number? (make-rectangular 3 4)))

;; real-part
(test-equal "real-part-integer" 5 (real-part 5))
(test-assert "real-part-complex" (= (real-part (make-rectangular 3 4)) 3.0))

;; imag-part
(test-equal "imag-part-integer" 0 (imag-part 5))
(test-assert "imag-part-complex" (= (imag-part (make-rectangular 3 4)) 4.0))

;; magnitude
(test-equal "magnitude-positive" 5 (magnitude 5))
(test-equal "magnitude-negative" 5 (magnitude -5))
(test-assert "magnitude-complex" (= (magnitude (make-rectangular 3 4)) 5.0))

;; angle
(test-assert "angle-positive-real" (= (angle 5) 0.0))
(test-assert "angle-1+1i-lower" (> (angle (make-rectangular 1 1)) 0.785))
(test-assert "angle-1+1i-upper" (< (angle (make-rectangular 1 1)) 0.786))

;; make-polar
(test-assert "make-polar-angle-0" (= (make-polar 5 0) 5.0))
(test-assert "make-polar-real-part"
  (< (abs (- (real-part (make-polar 5 0.9272952180016122)) 3.0)) 0.001))

;; ============================================================================
;; Number-String Conversion with Radix (R7RS §6.2.6)
;; ============================================================================

;; number->string decimal
(test-equal "number->string-255" "255" (number->string 255))
(test-equal "number->string-0" "0" (number->string 0))
(test-equal "number->string-neg42" "-42" (number->string -42))

;; number->string hex
(test-equal "number->string-255-hex" "ff" (number->string 255 16))
(test-equal "number->string-16-hex" "10" (number->string 16 16))

;; number->string binary
(test-equal "number->string-255-binary" "11111111" (number->string 255 2))
(test-equal "number->string-10-binary" "1010" (number->string 10 2))

;; number->string octal
(test-equal "number->string-255-octal" "377" (number->string 255 8))

;; string->number decimal
(test-equal "string->number-255" 255 (string->number "255"))
(test-equal "string->number-neg42" -42 (string->number "-42"))

;; string->number hex
(test-equal "string->number-ff-hex" 255 (string->number "ff" 16))
(test-equal "string->number-FF-hex" 255 (string->number "FF" 16))

;; string->number with prefix
(test-equal "string->number-hex-prefix" 255 (string->number "#xff"))
(test-equal "string->number-binary-prefix" 15 (string->number "#b1111"))
(test-equal "string->number-octal-prefix" 255 (string->number "#o377"))

;; string->number invalid
(test-assert "string->number-invalid-word" (not (string->number "hello")))
(test-assert "string->number-invalid-empty" (not (string->number "")))

;; string/number roundtrip
(test-assert "string-number-roundtrip-decimal"
  (= (string->number (number->string 42)) 42))
(test-assert "string-number-roundtrip-hex"
  (= (string->number (number->string 255 16) 16) 255))

;; ============================================================================
;; Complex predicates with tagged complex values
;; ============================================================================

(test-assert "complex?-make-rectangular" (complex? (make-rectangular 3 4)))
(test-assert "complex?-integer-also" (complex? 3))

(test-end)
