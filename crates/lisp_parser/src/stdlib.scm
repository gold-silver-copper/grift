;;; Scheme Standard Library
;;; 
;;; This file contains standard library function definitions.
;;; It is processed by the include_stdlib! macro to generate the StdLib enum.
;;;
;;; Format:
;;;   ;;; Documentation comment
;;;   (define (function-name param1 param2 ...) body)

;;; (map f lst) - Apply f to each element of lst
(define (map f lst) (if (null? lst) '() (cons (f (car lst)) (map f (cdr lst)))))

;;; (filter pred lst) - Return elements where pred is true
(define (filter pred lst) (if (null? lst) '() (if (pred (car lst)) (cons (car lst) (filter pred (cdr lst))) (filter pred (cdr lst)))))

;;; (fold f acc lst) - Left fold over lst
(define (fold f acc lst) (if (null? lst) acc (fold f (f acc (car lst)) (cdr lst))))

;;; (length lst) - Return length of lst
(define (length lst) (if (null? lst) 0 (+ 1 (length (cdr lst)))))

;;; (append a b) - Concatenate two lists
(define (append a b) (if (null? a) b (cons (car a) (append (cdr a) b))))

;;; (reverse lst) - Reverse a list
(define (reverse lst) (fold (lambda (acc x) (cons x acc)) '() lst))

;;; (nth n lst) - Get nth element (0-indexed)
(define (nth n lst) (if (= n 0) (car lst) (nth (- n 1) (cdr lst))))

;;; (take n lst) - Take first n elements
(define (take n lst) (if (= n 0) '() (if (null? lst) '() (cons (car lst) (take (- n 1) (cdr lst))))))

;;; (drop n lst) - Drop first n elements
(define (drop n lst) (if (= n 0) lst (if (null? lst) '() (drop (- n 1) (cdr lst)))))

;;; (zip a b) - Zip two lists into list of pairs
(define (zip a b) (if (null? a) '() (if (null? b) '() (cons (cons (car a) (car b)) (zip (cdr a) (cdr b))))))

;;; (member x lst) - Check if x is in lst using eq?
(define (member x lst) (if (null? lst) #f (if (eq? (car lst) x) #t (member x (cdr lst)))))

;;; (assoc key alist) - Look up key in association list using eq?
(define (assoc key alist) (if (null? alist) #f (if (eq? (car (car alist)) key) (car alist) (assoc key (cdr alist)))))

;;; (range start end) - Generate list of integers [start, end)
(define (range start end) (if (>= start end) '() (cons start (range (+ start 1) end))))

;;; (compose f g) - Return function that applies g then f
(define (compose f g) (lambda (x) (f (g x))))

;;; (identity x) - Return x unchanged
(define (identity x) x)

;;; (constantly x) - Return function that always returns x
(define (constantly x) (lambda (y) x))

;;; (flip f) - Flip argument order of binary function
(define (flip f) (lambda (a b) (f b a)))

;;; (curry f x) - Partial application
(define (curry f x) (lambda (y) (f x y)))

;;; (cadr lst) - (car (cdr lst))
(define (cadr lst) (car (cdr lst)))

;;; (caddr lst) - (car (cdr (cdr lst)))
(define (caddr lst) (car (cdr (cdr lst))))

;;; (cddr lst) - (cdr (cdr lst))
(define (cddr lst) (cdr (cdr lst)))

;;; ============================================================
;;; Phase 1: Core R7RS Procedures (Section 6.3-6.4)
;;; ============================================================

;;; (for-each f lst) - Apply f to each element for side effects
(define (for-each f lst) (if (null? lst) '() (begin (f (car lst)) (for-each f (cdr lst)))))

;;; (list-tail lst k) - Return sublist starting at k-th element
(define (list-tail lst k) (if (= k 0) lst (list-tail (cdr lst) (- k 1))))

;;; (list-ref lst k) - Return k-th element of lst (0-indexed)
(define (list-ref lst k) (if (= k 0) (car lst) (list-ref (cdr lst) (- k 1))))

;;; (list? obj) - Check if obj is a proper list
(define (list? obj) (if (null? obj) #t (if (pair? obj) (list? (cdr obj)) #f)))

;;; (list-copy lst) - Create a shallow copy of a list
(define (list-copy lst) (if (null? lst) '() (cons (car lst) (list-copy (cdr lst)))))

;;; (memq obj lst) - Find obj in lst using eq?, return sublist or #f
(define (memq obj lst) (if (null? lst) #f (if (eq? obj (car lst)) lst (memq obj (cdr lst)))))

;;; (memv obj lst) - Find obj in lst using eqv?, return sublist or #f
(define (memv obj lst) (if (null? lst) #f (if (eqv? obj (car lst)) lst (memv obj (cdr lst)))))

;;; (assq key alist) - Look up key in alist using eq?
(define (assq key alist) (if (null? alist) #f (if (eq? key (car (car alist))) (car alist) (assq key (cdr alist)))))

;;; (assv key alist) - Look up key in alist using eqv?
(define (assv key alist) (if (null? alist) #f (if (eqv? key (car (car alist))) (car alist) (assv key (cdr alist)))))

;;; ============================================================
;;; Additional c...r accessors (R7RS Section 6.4)
;;; ============================================================

;;; (caar lst) - (car (car lst))
(define (caar lst) (car (car lst)))

;;; (cdar lst) - (cdr (car lst))
(define (cdar lst) (cdr (car lst)))

;;; (caaar lst) - (car (car (car lst)))
(define (caaar lst) (car (car (car lst))))

;;; (caadr lst) - (car (car (cdr lst)))
(define (caadr lst) (car (car (cdr lst))))

;;; (cadar lst) - (car (cdr (car lst)))
(define (cadar lst) (car (cdr (car lst))))

;;; (cdaar lst) - (cdr (car (car lst)))
(define (cdaar lst) (cdr (car (car lst))))

;;; (cdadr lst) - (cdr (car (cdr lst)))
(define (cdadr lst) (cdr (car (cdr lst))))

;;; (cddar lst) - (cdr (cdr (car lst)))
(define (cddar lst) (cdr (cdr (car lst))))

;;; (cdddr lst) - (cdr (cdr (cdr lst)))
(define (cdddr lst) (cdr (cdr (cdr lst))))

;;; (cadddr lst) - (car (cdr (cdr (cdr lst))))
(define (cadddr lst) (car (cdr (cdr (cdr lst)))))

;;; (cddddr lst) - (cdr (cdr (cdr (cdr lst))))
(define (cddddr lst) (cdr (cdr (cdr (cdr lst)))))

;;; ============================================================
;;; Number utilities (R7RS Section 6.2.6)
;;; ============================================================

;;; (modulo a b) already builtin - use remainder-based modulo for stdlib
;;; (sign n) - Return -1, 0, or 1 based on sign of n
(define (sign n) (if (positive? n) 1 (if (negative? n) -1 0)))

;;; ============================================================
;;; Additional R7RS List Functions (Section 6.4)
;;; ============================================================

;;; (make-list k fill) - Create a list of k elements, each initialized to fill. 
;;; Note: k must be non-negative, negative values cause infinite recursion.
(define (make-list k fill) (if (<= k 0) '() (cons fill (make-list (- k 1) fill))))

;;; (list-set! lst k obj) - Store obj in element k of lst
(define (list-set! lst k obj) (set-car! (list-tail lst k) obj))

;;; (last-pair lst) - Return the last pair in a non-empty list
;;; Note: Error if called on empty list.
(define (last-pair lst) (if (null? (cdr lst)) lst (last-pair (cdr lst))))

;;; (last lst) - Return the last element of a non-empty list
;;; Note: Error if called on empty list.
(define (last lst) (car (last-pair lst)))

;;; ============================================================
;;; R7RS member/assoc with equal? (Section 6.4)
;;; ============================================================

;;; (member-equal obj lst) - Find obj in lst using equal?, return sublist or #f
(define (member-equal obj lst) (if (null? lst) #f (if (equal? obj (car lst)) lst (member-equal obj (cdr lst)))))

;;; (assoc-equal key alist) - Look up key in alist using equal?
(define (assoc-equal key alist) (if (null? alist) #f (if (equal? key (car (car alist))) (car alist) (assoc-equal key (cdr alist)))))

;;; ============================================================
;;; Higher-order list functions (R7RS Section 6.10)
;;; ============================================================

;;; (reduce f init lst) - Right fold (foldr)
(define (reduce f init lst) (if (null? lst) init (f (car lst) (reduce f init (cdr lst)))))

;;; (fold-right f init lst) - Right fold, R7RS name
(define (fold-right f init lst) (reduce f init lst))

;;; (any pred lst) - Return #t if pred is true for any element
(define (any pred lst) (if (null? lst) #f (if (pred (car lst)) #t (any pred (cdr lst)))))

;;; (every pred lst) - Return #t if pred is true for all elements
(define (every pred lst) (if (null? lst) #t (if (pred (car lst)) (every pred (cdr lst)) #f)))

;;; (find pred lst) - Return first element where pred is true, or #f
(define (find pred lst) (if (null? lst) #f (if (pred (car lst)) (car lst) (find pred (cdr lst)))))

;;; (filter-map f lst) - Map f over lst, keeping only non-#f results
;;; Note: This version avoids let binding due to recursion issue
(define (filter-map f lst) (if (null? lst) '() (if (f (car lst)) (cons (f (car lst)) (filter-map f (cdr lst))) (filter-map f (cdr lst)))))

;;; (partition pred lst) - Split lst into pair of two lists: (matching . non-matching)
;;; Returns (cons matches non-matches) where matches contains elements satisfying pred.
;;; Note: Uses tail-recursive helper to avoid let-binding issue in recursion.
(define (partition pred lst) (partition-helper pred lst '() '()))
(define (partition-helper pred lst matches non-matches) (if (null? lst) (cons (reverse matches) (reverse non-matches)) (if (pred (car lst)) (partition-helper pred (cdr lst) (cons (car lst) matches) non-matches) (partition-helper pred (cdr lst) matches (cons (car lst) non-matches)))))

;;; (remove pred lst) - Return lst with elements where pred is true removed
(define (remove pred lst) (filter (lambda (x) (not (pred x))) lst))

;;; (delete x lst) - Remove all occurrences of x from lst using equal?
(define (delete x lst) (filter (lambda (y) (not (equal? x y))) lst))

;;; ============================================================
;;; Boolean operations (R7RS Section 6.3)
;;; ============================================================

;;; (boolean-eq b1 b2) - Return #t if both arguments are #t or both are #f
(define (boolean-eq b1 b2) (or (and b1 b2) (and (not b1) (not b2))))

;;; ============================================================
;;; Mathematical Constants and Functions (R7RS Section 6.2.6)
;;; ============================================================

;;; (get-pi) - Returns pi, the ratio of a circle's circumference to its diameter
(define (get-pi) 3.141592653589793)

;;; (get-e) - Returns Euler's number e, the base of natural logarithms
(define (get-e) 2.718281828459045)

;;; (get-epsilon) - Returns accuracy threshold for iterative algorithms
(define (get-epsilon) 1e-15)

;;; ============================================================
;;; Square Root - Newton-Raphson method
;;; ============================================================

;;; (sqrt-iter x guess) - Helper for sqrt iteration
(define (sqrt-iter x guess)
  (let ((next (/ (+ guess (/ x guess)) 2.0)))
    (if (< (abs (- next guess)) (* 1e-15 (abs guess)))
        next
        (sqrt-iter x next))))

;;; (sqrt x) - Square root using Newton-Raphson iteration
(define (sqrt x)
  (if (< x 0)
      +nan.0
      (if (= x 0)
          0.0
          (sqrt-iter x 1.0))))

;;; ============================================================
;;; Exponential and Logarithm Functions
;;; ============================================================

;;; (exp-iter x term sum n) - Helper for exp Taylor series
(define (exp-iter x term sum n)
  (let ((new-term (* term (/ x (* 1.0 n)))))
    (if (< (abs new-term) 1e-15)
        sum
        (exp-iter x new-term (+ sum new-term) (+ n 1)))))

;;; (exp x) - Exponential function e^x using Taylor series
(define (exp x)
  (cond
   ((= x 0) 1.0)
   ((< x 0)
    (let ((pos-exp (exp (- x))))
      (/ 1.0 pos-exp)))
   ((> x 1)
    (let ((half (exp (/ x 2.0))))
      (* half half)))
   (else (exp-iter x 1.0 1.0 1))))

;;; (log-series-iter z term sum n sign) - Helper for log series
(define (log-series-iter z term sum n sign)
  (let ((new-term (* term z)))
    (if (< (abs new-term) 1e-15)
        sum
        (log-series-iter z new-term (+ sum (* sign (/ new-term (* 1.0 n)))) (+ n 1) (- sign)))))

;;; (log-newton-iter x guess) - Helper for log Newton iteration
(define (log-newton-iter x guess)
  (let* ((exp-guess (exp guess))
         (next (+ guess (/ (- x exp-guess) exp-guess))))
    (if (< (abs (- next guess)) 1e-15)
        next
        (log-newton-iter x next))))

;;; (log x) - Natural logarithm using Newton's method and series
(define (log x)
  (cond
   ((<= x 0) +nan.0)
   ((= x 1) 0.0)
   ((and (> x 0.5) (< x 2.0))
    (let ((z (- x 1.0)))
      (log-series-iter z z z 2 -1)))
   (else
    (log-newton-iter x (if (> x 1) 1.0 -1.0)))))

;;; ============================================================
;;; Trigonometric Functions
;;; ============================================================

;;; (sin-iter x term sum n) - Helper for sin Taylor series
(define (sin-iter x term sum n)
  (let ((new-term (* term (/ (- (square x)) (* (* 1.0 (+ n 1)) (+ n 2))))))
    (if (< (abs new-term) 1e-15)
        sum
        (sin-iter x new-term (+ sum new-term) (+ n 2)))))

;;; (sin-reduce x) - Reduce angle to [-pi, pi]
(define (sin-reduce x)
  (- x (* 2.0 3.141592653589793 (round (/ x (* 2.0 3.141592653589793))))))

;;; (sin x) - Sine using Taylor series
(define (sin x)
  (let ((reduced (sin-reduce x)))
    (sin-iter reduced reduced reduced 1)))

;;; (cos-iter x term sum n) - Helper for cos Taylor series
(define (cos-iter x term sum n)
  (let ((new-term (* term (/ (- (square x)) (* (* 1.0 (+ n 1)) (+ n 2))))))
    (if (< (abs new-term) 1e-15)
        sum
        (cos-iter x new-term (+ sum new-term) (+ n 2)))))

;;; (cos x) - Cosine using Taylor series
(define (cos x)
  (let ((reduced (sin-reduce x)))
    (cos-iter reduced 1.0 1.0 0)))

;;; (tan x) - Tangent as sin/cos
(define (tan x)
  (let ((s (sin x))
        (c (cos x)))
    (/ s c)))

;;; ============================================================
;;; Inverse Trigonometric Functions
;;; ============================================================

;;; (asin-iter guess x) - Helper for asin Newton iteration
(define (asin-iter guess x)
  (let* ((sin-guess (sin guess))
         (cos-guess (cos guess))
         (next (- guess (/ (- sin-guess x) cos-guess))))
    (if (< (abs (- next guess)) 1e-15)
        next
        (asin-iter next x))))

;;; (asin x) - Inverse sine using Newton's method
(define (asin x)
  (cond
   ((< (abs x) 1e-15) 0.0)
   ((> (abs x) 1) +nan.0)
   ((= x 1) (/ 3.141592653589793 2))
   ((= x -1) (/ 3.141592653589793 -2))
   (else (asin-iter x x))))

;;; (acos x) - Inverse cosine
(define (acos x)
  (- (/ 3.141592653589793 2) (asin x)))

;;; (atan-series-iter y y2 term sum n) - Helper for atan series
(define (atan-series-iter y y2 term sum n)
  (let ((new-term (* term (- y2))))
    (let ((denom (+ (* 2.0 n) 1.0)))
      (if (< (abs (/ new-term denom)) 1e-15)
          sum
          (atan-series-iter y y2 new-term (+ sum (/ new-term denom)) (+ n 1))))))

;;; (atan1 y) - Arctangent of single argument
(define (atan1 y)
  (cond
   ((= y 0) 0.0)
   ((= y 1) 0.7853981633974483)
   ((= y -1) -0.7853981633974483)
   ((> (abs y) 1)
    (let ((result (- (/ 3.141592653589793 2.0) (atan1 (/ 1.0 y)))))
      (if (< y 0) (- result) result)))
   (else
    (let ((y2 (square y)))
      (atan-series-iter y y2 y y 1)))))

;;; (atan2 y x) - Arctangent of two arguments
(define (atan2 y x)
  (cond
   ((and (= x 0) (= y 0)) +nan.0)
   ((> x 0) (atan1 (/ y x)))
   ((and (< x 0) (>= y 0)) (+ (atan1 (/ y x)) 3.141592653589793))
   ((and (< x 0) (< y 0)) (- (atan1 (/ y x)) 3.141592653589793))
   ((and (= x 0) (> y 0)) (/ 3.141592653589793 2))
   ((and (= x 0) (< y 0)) (/ 3.141592653589793 -2))
   (else +nan.0)))

;;; ============================================================
;;; Hyperbolic Functions
;;; ============================================================

;;; (sinh x) - Hyperbolic sine
(define (sinh x)
  (let ((ex (exp x))
        (emx (exp (- x))))
    (/ (- ex emx) 2.0)))

;;; (cosh x) - Hyperbolic cosine
(define (cosh x)
  (let ((ex (exp x))
        (emx (exp (- x))))
    (/ (+ ex emx) 2.0)))

;;; (tanh x) - Hyperbolic tangent
(define (tanh x)
  (let ((sh (sinh x))
        (ch (cosh x)))
    (/ sh ch)))

;;; ============================================================
;;; Logarithm Variants
;;; ============================================================

;;; (log-base base x) - Logarithm with arbitrary base
(define (log-base base x)
  (cond
   ((<= base 0) +nan.0)
   ((= base 1) +nan.0)
   (else
    (let ((lx (log x))
          (lb (log base)))
      (/ lx lb)))))

;;; (log10 x) - Base-10 logarithm
(define (log10 x)
  (log-base 10 x))

;;; (log2 x) - Base-2 logarithm
(define (log2 x)
  (log-base 2 x))

;;; ============================================================
;;; Type Predicates for Numbers
;;; ============================================================

;;; (nan? x) - Check if x is NaN
(define (nan? x)
  (and (number? x) (not (= x x))))

;;; (infinite? x) - Check if x is infinite
(define (infinite? x)
  (and (number? x) (or (= x +inf.0) (= x -inf.0))))

;;; (finite? x) - Check if x is a finite number
(define (finite? x)
  (and (number? x) (not (nan? x)) (not (infinite? x))))

;;; (real? x) - Check if x is a real number (same as number? in our implementation)
(define (real? x) (number? x))

;;; (rational? x) - Check if x is a rational number (floats are approximate rationals)
(define (rational? x) (and (number? x) (finite? x)))

;;; (complex? x) - Check if x is a complex number (same as number? - no complex support)
(define (complex? x) (number? x))

;;; ============================================================
;;; Type Conversion Functions
;;; ============================================================

;;; (exact->inexact x) - Convert exact to inexact (int to float)
(define (exact->inexact x)
  (if (exact? x)
      (+ x 0.0)
      x))

;;; (inexact->exact x) - Convert inexact to exact (float to int, truncates)
(define (inexact->exact x)
  (if (inexact? x)
      (truncate x)
      x))

;;; ============================================================
;;; Character Predicates (R7RS Section 6.6)
;;; ============================================================

;;; (char-alphabetic? char) - Check if char is alphabetic (a-z, A-Z)
(define (char-alphabetic? c)
  (let ((n (char->integer c)))
    (or (and (>= n 65) (<= n 90))
        (and (>= n 97) (<= n 122)))))

;;; (char-numeric? char) - Check if char is a decimal digit (0-9)
(define (char-numeric? c)
  (let ((n (char->integer c)))
    (and (>= n 48) (<= n 57))))

;;; (char-whitespace? char) - Check if char is whitespace
(define (char-whitespace? c)
  (let ((n (char->integer c)))
    (or (= n 32) (= n 9) (= n 10) (= n 13) (= n 12))))

;;; (char-upper-case? char) - Check if char is uppercase (A-Z)
(define (char-upper-case? c)
  (let ((n (char->integer c)))
    (and (>= n 65) (<= n 90))))

;;; (char-lower-case? char) - Check if char is lowercase (a-z)
(define (char-lower-case? c)
  (let ((n (char->integer c)))
    (and (>= n 97) (<= n 122))))

;;; (digit-value char) - Return numeric value (0-9) of a digit character, or #f
(define (digit-value c)
  (let ((n (char->integer c)))
    (if (and (>= n 48) (<= n 57))
        (- n 48)
        #f)))

;;; (char-foldcase char) - Unicode simple case-folding (lowercase for ASCII)
(define (char-foldcase c)
  (char-downcase c))

;;; Case-insensitive character comparisons

;;; (char-ci=? char1 char2 ...) - Case-insensitive char=?
(define (char-ci=? c1 c2)
  (char=? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci<? char1 char2) - Case-insensitive char<?
(define (char-ci<? c1 c2)
  (char<? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci>? char1 char2) - Case-insensitive char>?
(define (char-ci>? c1 c2)
  (char>? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci<=? char1 char2) - Case-insensitive char<=?
(define (char-ci<=? c1 c2)
  (char<=? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci>=? char1 char2) - Case-insensitive char>=?
(define (char-ci>=? c1 c2)
  (char>=? (char-foldcase c1) (char-foldcase c2)))

;;; ============================================================
;;; String Case-Insensitive Comparisons (R7RS Section 6.7)
;;; ============================================================

;;; (string-ci=? s1 s2) - Case-insensitive string=?
;;; Note: For full implementation, would need to fold case of entire strings
(define (string-ci=? s1 s2)
  (string-ci-compare-helper s1 s2 0 (string-length s1) (string-length s2)))

(define (string-ci-compare-helper s1 s2 i len1 len2)
  (cond
   ((and (= i len1) (= i len2)) #t)
   ((= i len1) #f)
   ((= i len2) #f)
   ((char-ci=? (string-ref s1 i) (string-ref s2 i))
    (string-ci-compare-helper s1 s2 (+ i 1) len1 len2))
   (else #f)))

;;; (string-upcase s) - Convert string to uppercase
(define (string-upcase s)
  (list->string (map char-upcase (string->list s))))

;;; (string-downcase s) - Convert string to lowercase
(define (string-downcase s)
  (list->string (map char-downcase (string->list s))))

;;; (string-foldcase s) - Convert string using case folding
(define (string-foldcase s)
  (list->string (map char-foldcase (string->list s))))

;;; ============================================================================
;;; Numerical Tower Implementation (R7RS Section 6.2)
;;; ============================================================================
;;;
;;; Type hierarchy (predicates follow Scheme subset relationships):
;;;
;;; number?      - ALL numbers (every complex is a number)
;;;   └─ complex?   - Complex numbers AND all reals (every real is complex)
;;;        └─ real?      - Real numbers: floats, rationals, and integers
;;;             └─ rational?  - Rationals AND integers (every int is rational)
;;;                  └─ integer?   - Only integers (isize)
;;;
;;; Internal representations:
;;; - Integer: primitive isize
;;; - Rational: (rational . (numerator . denominator))
;;; - Real/Float: primitive f64
;;; - Complex: (complex . (real-part . imag-part))
;;; ============================================================================

;;; ============================================================================
;;; Rational Number Type Tags and Constructors
;;; ============================================================================

;;; (rational-representation? x) - Check if x is tagged as a rational pair
(define (rational-representation? x)
  (and (pair? x) (eq? (car x) 'rational)))

;;; (make-rational-raw n d) - Create a rational pair without normalization
(define (make-rational-raw n d)
  (cons 'rational (cons n d)))

;;; (rational-gcd a b) - GCD for rational normalization
(define (rational-gcd a b)
  (if (= b 0)
      (abs a)
      (rational-gcd b (modulo a b))))

;;; (normalize-rational n d) - Create a normalized rational number
;;; Reduces to lowest terms and ensures positive denominator
;;; If d=1, returns the integer n instead of a rational
;;; Helper to build normalized rational from computed gcd
(define (normalize-rational-helper n d g)
  (normalize-rational-final (quotient n g) (quotient d g)))

(define (normalize-rational-final n1 d1)
  (if (< d1 0)
      (if (= (- d1) 1)
          (- n1)
          (make-rational-raw (- n1) (- d1)))
      (if (= d1 1)
          n1
          (make-rational-raw n1 d1))))

(define (normalize-rational n d)
  (if (= d 0)
      (error "Division by zero in rational")
      (normalize-rational-helper n d (rational-gcd (abs n) (abs d)))))

;;; (make-rational n d) - Create a normalized rational number
(define (make-rational n d)
  (normalize-rational n d))

;;; ============================================================================
;;; Complex Number Type Tags and Constructors
;;; ============================================================================

;;; (complex-representation? x) - Check if x is tagged as a complex pair
(define (complex-representation? x)
  (and (pair? x) (eq? (car x) 'complex)))

;;; (make-complex-raw real imag) - Create a complex pair without normalization
(define (make-complex-raw real imag)
  (cons 'complex (cons real imag)))

;;; (make-complex real imag) - Create a complex number, normalizing to real if imag=0
(define (make-complex real imag)
  (if (and (number? imag) (= imag 0))
      real
      (make-complex-raw real imag)))

;;; (make-rectangular x1 x2) - R7RS: Create complex from real and imaginary parts
(define (make-rectangular x1 x2)
  (make-complex x1 x2))

;;; (make-polar r theta) - R7RS: Create complex from magnitude and angle
;;; Uses helper to avoid nested stdlib call issue
(define (make-polar-helper real-part imag-part)
  (make-complex real-part imag-part))

(define (make-polar r theta)
  (define real-val (* r (cos theta)))
  (define imag-val (* r (sin theta)))
  (make-polar-helper real-val imag-val))

;;; ============================================================================
;;; Rational Number Accessors
;;; ============================================================================

;;; (numerator-of q) - Get numerator of a rational or integer
(define (numerator-of q)
  (cond
    ((rational-representation? q) (car (cdr q)))
    ((integer? q) q)
    (else (error "numerator requires rational or integer"))))

;;; (denominator-of q) - Get denominator of a rational or integer
(define (denominator-of q)
  (cond
    ((rational-representation? q) (cdr (cdr q)))
    ((integer? q) 1)
    (else (error "denominator requires rational or integer"))))

;;; ============================================================================
;;; Complex Number Accessors
;;; ============================================================================

;;; (real-part z) - R7RS: Get real part of a complex number
(define (real-part z)
  (cond
    ((complex-representation? z) (car (cdr z)))
    ((number? z) z)
    (else (error "real-part requires a number"))))

;;; (imag-part z) - R7RS: Get imaginary part of a complex number
(define (imag-part z)
  (cond
    ((complex-representation? z) (cdr (cdr z)))
    ((number? z) 0)
    (else (error "imag-part requires a number"))))

;;; (magnitude z) - R7RS: Get magnitude of a complex number
(define (magnitude z)
  (cond
    ((complex-representation? z)
     (let ((r (real-part z))
           (i (imag-part z)))
       (sqrt (+ (* r r) (* i i)))))
    ((number? z) (abs z))
    (else (error "magnitude requires a number"))))

;;; (angle z) - R7RS: Get angle (argument) of a complex number
(define (angle z)
  (cond
    ((complex-representation? z)
     (atan2 (imag-part z) (real-part z)))
    ((number? z)
     (if (< z 0) (get-pi) 0))
    (else (error "angle requires a number"))))

;;; ============================================================================
;;; Rational Arithmetic
;;; ============================================================================

;;; (add-rational a b) - Add two rationals
(define (add-rational-helper an ad bn bd)
  (normalize-rational (+ (* an bd) (* bn ad))
                      (* ad bd)))

(define (add-rational a b)
  (add-rational-helper (numerator-of a) (denominator-of a)
                       (numerator-of b) (denominator-of b)))

;;; (sub-rational a b) - Subtract two rationals
(define (sub-rational-helper an ad bn bd)
  (normalize-rational (- (* an bd) (* bn ad))
                      (* ad bd)))

(define (sub-rational a b)
  (sub-rational-helper (numerator-of a) (denominator-of a)
                       (numerator-of b) (denominator-of b)))

;;; (mul-rational a b) - Multiply two rationals
(define (mul-rational-helper an ad bn bd)
  (normalize-rational (* an bn) (* ad bd)))

(define (mul-rational a b)
  (mul-rational-helper (numerator-of a) (denominator-of a)
                       (numerator-of b) (denominator-of b)))

;;; (div-rational a b) - Divide two rationals
(define (div-rational-helper an ad bn bd)
  (normalize-rational (* an bd) (* ad bn)))

(define (div-rational a b)
  (div-rational-helper (numerator-of a) (denominator-of a)
                       (numerator-of b) (denominator-of b)))

;;; (rational->float r) - Convert rational to float
(define (rational->float r)
  (/ (exact->inexact (numerator-of r))
     (exact->inexact (denominator-of r))))

;;; ============================================================================
;;; Complex Arithmetic
;;; ============================================================================

;;; (add-complex a b) - Add two complex numbers
(define (add-complex a b)
  (make-complex (+ (real-part a) (real-part b))
                (+ (imag-part a) (imag-part b))))

;;; (sub-complex a b) - Subtract two complex numbers
(define (sub-complex a b)
  (make-complex (- (real-part a) (real-part b))
                (- (imag-part a) (imag-part b))))

;;; (mul-complex a b) - Multiply two complex numbers
(define (mul-complex-helper ar ai br bi)
  (make-complex (- (* ar br) (* ai bi))
                (+ (* ar bi) (* ai br))))

(define (mul-complex a b)
  (mul-complex-helper (real-part a) (imag-part a)
                      (real-part b) (imag-part b)))

;;; (div-complex a b) - Divide two complex numbers
(define (div-complex-denom ar ai br bi denom)
  (make-complex (/ (+ (* ar br) (* ai bi)) denom)
                (/ (- (* ai br) (* ar bi)) denom)))

(define (div-complex-helper ar ai br bi)
  (define denom (+ (* br br) (* bi bi)))
  (if (= denom 0)
      (error "Division by zero in div-complex")
      (div-complex-denom ar ai br bi denom)))

(define (div-complex a b)
  (div-complex-helper (real-part a) (imag-part a)
                      (real-part b) (imag-part b)))

;;; (negate-complex z) - Negate a complex number
(define (negate-complex z)
  (make-complex (- (real-part z)) (- (imag-part z))))

;;; (conjugate z) - Complex conjugate
(define (conjugate z)
  (make-complex (real-part z) (- (imag-part z))))

;;; ============================================================================
;;; Rationalize (R7RS Section 6.2.6)
;;; ============================================================================

;;; (rationalize x y) - Return simplest rational within y of x
;;; Uses Stern-Brocot tree / continued fraction approach
(define (rationalize x y)
  (rationalize-helper (- x y) (+ x y)))

(define (rationalize-helper lo hi)
  (define lo-floor (floor lo))
  (define hi-floor (floor hi))
  (cond
    ;; If lo-floor > hi-floor, lo-floor is in the range
    ((> lo-floor hi-floor) lo-floor)
    ;; If lo-floor = hi-floor, that integer might be in range
    ((= lo-floor hi-floor)
     (cond
       ;; If lo equals lo-floor exactly, that's the answer
       ((= lo-floor lo) lo-floor)
       ;; Otherwise recurse with reciprocals to find a fraction
       (else
        (+ lo-floor (/ 1 (rationalize-helper (/ 1 (- hi lo-floor))
                                             (/ 1 (- lo lo-floor))))))))
    ;; Otherwise, ceiling of lo is an integer in the range
    (else (+ lo-floor 1))))

;;; ============================================================================
;;; Extended Type Predicates for Numerical Tower
;;; ============================================================================
;;; Note: We redefine these to include the tower types

;;; (tower-integer? x) - Check if x is an integer in the tower
(define (tower-integer? x)
  (or (integer? x)
      (and (rational-representation? x) (= (denominator-of x) 1))))

;;; (tower-rational? x) - Check if x is rational (integer or rational representation)
(define (tower-rational? x)
  (or (integer? x)
      (rational-representation? x)))

;;; (tower-real? x) - Check if x is real (integer, rational, or float)
(define (tower-real? x)
  (or (integer? x)
      (rational-representation? x)
      (and (number? x) (not (complex-representation? x)))))

;;; (tower-complex? x) - Check if x is complex (any number including complex)
(define (tower-complex? x)
  (or (integer? x)
      (rational-representation? x)
      (number? x)
      (complex-representation? x)))

;;; (tower-number? x) - Check if x is any kind of number
(define (tower-number? x)
  (tower-complex? x))

;;; ============================================================================
;;; Complex Comparisons
;;; ============================================================================

;;; (complex=? a b) - Check if two complex numbers are equal
(define (complex=? a b)
  (and (= (real-part a) (real-part b))
       (= (imag-part a) (imag-part b))))

;;; ============================================================================
;;; Complex Transcendental Functions
;;; ============================================================================

;;; (complex-sqrt z) - Square root of a complex number
;;; For complex input: uses polar form
;;; For negative real: returns pure imaginary
;;; For positive real: returns sqrt
(define (complex-sqrt-from-polar r theta)
  (make-polar (sqrt r) (/ theta 2)))

;;; Helper that creates 0+yi given y (avoids nested call issue)
(define (make-pure-imaginary y)
  (make-complex-raw 0 y))

;;; Helper for negative real sqrt (uses define to avoid nested call bug)
(define (complex-sqrt-neg z)
  (define neg-val (- z))
  (define sqrt-val (sqrt neg-val))
  (make-pure-imaginary sqrt-val))

(define (complex-sqrt z)
  (if (complex-representation? z)
      (complex-sqrt-from-polar (magnitude z) (angle z))
      (if (< z 0)
          (complex-sqrt-neg z)
          (sqrt z))))

;;; (complex-exp z) - e^z for complex z
(define (complex-exp-parts r i er)
  (make-complex (* er (cos i))
                (* er (sin i))))

(define (complex-exp-helper r i)
  (complex-exp-parts r i (exp r)))

(define (complex-exp z)
  (if (complex-representation? z)
      (complex-exp-helper (real-part z) (imag-part z))
      (exp z)))

;;; (complex-log z) - Natural log of complex z
(define (complex-log z)
  (if (complex-representation? z)
      (make-complex (log (magnitude z)) (angle z))
      (if (< z 0)
          (make-complex (log (abs z)) (get-pi))
          (log z))))

;;; (complex-sin z) - Sine of complex z
(define (complex-sin-parts r i)
  (make-complex (* (sin r) (cosh i))
                (* (cos r) (sinh i))))

(define (complex-sin z)
  (if (complex-representation? z)
      (complex-sin-parts (real-part z) (imag-part z))
      (sin z)))

;;; (complex-cos z) - Cosine of complex z
(define (complex-cos-parts r i)
  (make-complex (* (cos r) (cosh i))
                (- (* (sin r) (sinh i)))))

(define (complex-cos z)
  (if (complex-representation? z)
      (complex-cos-parts (real-part z) (imag-part z))
      (cos z)))

;;; (complex-tan z) - Tangent of complex z
(define (complex-tan z)
  (div-complex (complex-sin z) (complex-cos z)))

;;; (complex-expt base power) - Exponentiation with complex support
(define (complex-expt base power)
  (cond
    ((= power 0) 1)
    ((and (integer? power) (> power 0))
     (complex-expt-pos base power))
    ((and (integer? power) (< power 0))
     (complex-expt-neg base power))
    (else
     (complex-exp (mul-complex power (complex-log base))))))

(define (complex-expt-pos base power)
  (if (= power 1)
      base
      (if (complex-representation? base)
          (mul-complex base (complex-expt-pos base (- power 1)))
          (* base (complex-expt-pos base (- power 1))))))

(define (complex-expt-neg base power)
  (if (complex-representation? base)
      (div-complex 1 (complex-expt base (- power)))
      (/ 1 (complex-expt base (- power)))))

;;; ============================================================================
;;; Display Functions for Numerical Tower
;;; ============================================================================

;;; (display-rational r) - Display a rational number
(define (display-rational r)
  (display (numerator-of r))
  (display "/")
  (display (denominator-of r)))

;;; (display-complex z) - Display a complex number  
(define (display-complex z)
  (display (real-part z))
  (if (>= (imag-part z) 0)
      (display "+")
      (display ""))
  (display (imag-part z))
  (display "i"))

;;; (display-tower-number x) - Display any tower number appropriately
(define (display-tower-number x)
  (cond
    ((complex-representation? x) (display-complex x))
    ((rational-representation? x) (display-rational x))
    (else (display x))))
