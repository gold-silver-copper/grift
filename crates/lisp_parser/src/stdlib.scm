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
   ((> x 0) (atan1 (/ y x)))
   ((and (< x 0) (>= y 0)) (+ (atan1 (/ y x)) 3.141592653589793))
   ((and (< x 0) (< y 0)) (- (atan1 (/ y x)) 3.141592653589793))
   ((and (= x 0) (> y 0)) (/ 3.141592653589793 2))
   ((and (= x 0) (< y 0)) (/ 3.141592653589793 -2))
   (else 0.0)))

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
  (let ((lx (log x))
        (lb (log base)))
    (/ lx lb)))

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
