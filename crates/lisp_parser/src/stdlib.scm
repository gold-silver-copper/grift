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
