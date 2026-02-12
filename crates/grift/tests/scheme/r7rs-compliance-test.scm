;;; R7RS compliance tests
;;; Migrated from r7rs_compliance_tests.rs

(test-begin "r7rs-compliance")

;; ============================================================================
;; map with multiple lists (R7RS §6.4)
;; ============================================================================

;; map single list
(test-equal "map-single-list-plus" '(1 2 3) (map + '(1 2 3)))
(test-equal "map-single-list-car" '(1 3) (map car '((1 2) (3 4))))

;; map two lists
(test-equal "map-two-lists-plus" '(11 22 33) (map + '(1 2 3) '(10 20 30)))
(test-equal "map-two-lists-multiply" '(8 15) (map * '(2 3) '(4 5)))

;; map three lists
(test-equal "map-three-lists-plus" '(111 222 333) (map + '(1 2 3) '(10 20 30) '(100 200 300)))
(test-equal "map-three-lists-list" '((1 3 5) (2 4 6)) (map list '(1 2) '(3 4) '(5 6)))

;; map four lists
(test-equal "map-four-lists-plus" '(10) (map + '(1) '(2) '(3) '(4)))

;; map unequal length uses shortest
(test-equal "map-unequal-length-long-short" '(11 22) (map + '(1 2 3) '(10 20)))
(test-equal "map-unequal-length-short-long" '(11) (map + '(1) '(10 20 30)))

;; ============================================================================
;; for-each with multiple lists (R7RS §6.4)
;; ============================================================================

;; for-each single list
(test-equal "for-each-single-list" 6
  (let ((sum 0))
    (for-each (lambda (x) (set! sum (+ sum x))) '(1 2 3))
    sum))

;; for-each two lists
(test-equal "for-each-two-lists" 66
  (let ((sum 0))
    (for-each (lambda (x y) (set! sum (+ sum x y))) '(1 2 3) '(10 20 30))
    sum))

;; for-each three lists
(test-equal "for-each-three-lists" 333
  (let ((sum 0))
    (for-each (lambda (x y z) (set! sum (+ sum x y z))) '(1 2) '(10 20) '(100 200))
    sum))

;; ============================================================================
;; bytevector constructor (R7RS §6.9)
;; ============================================================================

(test-assert "bytevector-constructor-is-bytevector" (bytevector? (bytevector)))
(test-equal "bytevector-constructor-empty-length" 0 (bytevector-length (bytevector)))
(test-equal "bytevector-constructor-three-length" 3 (bytevector-length (bytevector 1 2 3)))
(test-equal "bytevector-constructor-ref-0" 10 (bytevector-u8-ref (bytevector 10 20 30) 0))
(test-equal "bytevector-constructor-ref-1" 20 (bytevector-u8-ref (bytevector 10 20 30) 1))
(test-equal "bytevector-constructor-ref-2" 30 (bytevector-u8-ref (bytevector 10 20 30) 2))

;; bytevector constructor edge cases
(test-equal "bytevector-constructor-max-byte" 255 (bytevector-u8-ref (bytevector 255) 0))
(test-equal "bytevector-constructor-zero-byte" 0 (bytevector-u8-ref (bytevector 0) 0))

;; ============================================================================
;; bytevector-copy! (R7RS §6.9)
;; ============================================================================

;; bytevector-copy! basic
(test-equal "bytevector-copy!-basic" #u8(1 2 3 0 0)
  (let ((to (make-bytevector 5 0))
        (from (bytevector 1 2 3)))
    (bytevector-copy! to 0 from)
    to))

;; bytevector-copy! with offset
(test-equal "bytevector-copy!-with-offset" #u8(0 1 2 3 0)
  (let ((to (make-bytevector 5 0))
        (from (bytevector 1 2 3)))
    (bytevector-copy! to 1 from)
    to))

;; bytevector-copy! with range
(test-equal "bytevector-copy!-with-range" #u8(0 2 3 0 0)
  (let ((to (make-bytevector 5 0))
        (from (bytevector 1 2 3 4 5)))
    (bytevector-copy! to 1 from 1 3)
    to))

;; ============================================================================
;; scheme-report-environment and null-environment (R7RS §6.12)
;; ============================================================================

(test-assert "scheme-report-environment" (scheme-report-environment 5))
(test-assert "null-environment" (null-environment 5))

;; ============================================================================
;; rational number literals (R7RS §7.1.1)
;; ============================================================================

(test-equal "rational-literal-3/4" 3/4 3/4)
(test-equal "rational-literal-neg-1/2" -1/2 -1/2)
(test-equal "rational-literal-pos-5/3" 5/3 +5/3)

;; rational normalization
(test-equal "rational-normalize-2/4" 1/2 2/4)
(test-equal "rational-normalize-6/3" 2 6/3)
(test-equal "rational-normalize-10/5" 2 10/5)

;; rational arithmetic
(test-assert "rational-add-lower-bound" (> (+ 1/2 1/3) 0.83))
(test-assert "rational-add-upper-bound" (< (+ 1/2 1/3) 0.84))
(test-assert "rational-multiply" (= (* 1/2 2) 1.0))

;; rational exact/inexact
(test-assert "rational-is-exact" (exact? 3/4))
(test-assert "rational-inexact-conversion" (inexact? (exact->inexact 3/4)))
(test-assert "rational-is-number" (number? 3/4))

;; rational numerator/denominator
(test-equal "rational-numerator-3/4" 3 (numerator 3/4))
(test-equal "rational-denominator-3/4" 4 (denominator 3/4))
(test-equal "rational-numerator-neg-1/2" -1 (numerator -1/2))
(test-equal "rational-denominator-neg-1/2" 2 (denominator -1/2))

;; rational comparison
(test-assert "rational-less-than" (< 1/4 1/2))
(test-assert "rational-greater-than" (> 3/4 1/2))
(test-assert "rational-close-to-float" (< (- 1/2 0.5) 0.001))

;; ============================================================================
;; complex number literals (R7RS §7.1.1)
;; ============================================================================

(test-assert "complex-rectangular-1+2i" (number? 1+2i))
(test-assert "complex-rectangular-3-4i" (number? 3-4i))

;; complex real/imag parts
(test-assert "complex-real-part-1+2i" (= (real-part 1+2i) 1.0))
(test-assert "complex-imag-part-1+2i" (= (imag-part 1+2i) 2.0))
(test-assert "complex-real-part-3-4i" (= (real-part 3-4i) 3.0))
(test-assert "complex-imag-part-3-4i" (= (imag-part 3-4i) -4.0))

;; complex magnitude
(test-assert "complex-magnitude-3+4i" (= (magnitude 3+4i) 5.0))
(test-assert "complex-magnitude-0+1i" (= (magnitude 0+1i) 1.0))

;; complex make-rectangular
(test-assert "make-rectangular-real-part" (= (real-part (make-rectangular 3 4)) 3.0))
(test-assert "make-rectangular-imag-part" (= (imag-part (make-rectangular 3 4)) 4.0))

;; complex make-polar
(test-assert "make-polar-magnitude"
  (< (- (magnitude (make-polar 5 0.9272952180016122)) 5.0) 0.001))

;; ============================================================================
;; member/assoc with optional comparison procedure (R7RS §6.4)
;; ============================================================================

;; member with custom comparator
(test-equal "member-default" '(2 3) (member 2 '(1 2 3)))
(test-equal "member-custom-equal" '(2 3) (member 2.0 '(1 2 3) =))
(test-assert "member-custom-never-matches" (not (member 2 '(1 2 3) (lambda (a b) #f))))

;; assoc with custom comparator
(test-equal "assoc-default" '(b 2) (assoc 'b '((a 1) (b 2) (c 3))))
(test-equal "assoc-custom-equal" '(2 b) (assoc 2.0 '((1 a) (2 b) (3 c)) =))
(test-assert "assoc-custom-never-matches" (not (assoc 2 '((1 a) (2 b)) (lambda (a b) #f))))

(test-end)
