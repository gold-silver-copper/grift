;;; Scheme Standard Library
;;; 
;;; This file contains standard library function definitions.
;;; It is processed by the include_stdlib! macro to generate the StdLib enum.
;;;
;;; Format:
;;;   ;;; Documentation comment
;;;   (define (function-name param1 param2 ...) body)

;;; (map f lst) - Apply f to each element of lst (tail-recursive)
(define (map f lst)
  (define (map-iter lst acc)  ;; Helper function for tail-recursive iteration
    (if (null? lst)
        (reverse acc)  ;; Base case: reverse accumulated list
        (map-iter (cdr lst) (cons (f (car lst)) acc))))  ;; Recursive case: apply f and continue
  (map-iter lst '()))  ;; Start with empty accumulator

;;; (filter pred lst) - Return elements where pred is true (tail-recursive)
(define (filter pred lst)
  (define (filter-iter lst acc)  ;; Tail-recursive helper
    (if (null? lst)
        (reverse acc)  ;; Base case
        (if (pred (car lst))  ;; Test if element matches predicate
            (filter-iter (cdr lst) (cons (car lst) acc))  ;; Include element
            (filter-iter (cdr lst) acc))))  ;; Skip element
  (filter-iter lst '()))  ;; Start with empty accumulator

;;; (fold f acc lst) - Left fold over lst
(define (fold f acc lst)  ;; Left-associative fold
  (if (null? lst)
      acc  ;; Base case: return accumulator
      (fold f (f acc (car lst)) (cdr lst))))  ;; Apply f to acc and car, recurse

;;; (fold-left f acc lst) - Left fold over lst (R7RS name, same as fold)
;;; f takes (accumulator, element) and returns new accumulator
(define (fold-left f acc lst)
  (if (null? lst)
      acc
      (fold-left f (f acc (car lst)) (cdr lst))))

;;; (length lst) - Return length of lst (tail-recursive)
(define (length lst)
  (define (length-iter lst acc)
    (if (null? lst) acc (length-iter (cdr lst) (+ acc 1))))
  (length-iter lst 0))

;;; (append-two a b) - Internal: Concatenate exactly two lists (tail-recursive)
;;; This is the workhorse for the variadic append macro.
(define (append-two a b)
  (define (rev-helper lst acc)
    (if (null? lst) acc (rev-helper (cdr lst) (cons (car lst) acc))))
  (define (append-iter lst acc)
    (if (null? lst) acc (append-iter (cdr lst) (cons (car lst) acc))))
  (append-iter (rev-helper a '()) b))

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

;;; ============================================================
;;; Internal Helper Functions
;;; ============================================================

;;; (mem-helper pred obj lst) - Generic member helper using predicate
(define (mem-helper pred obj lst) (if (null? lst) #f (if (pred obj (car lst)) lst (mem-helper pred obj (cdr lst)))))

;;; (assoc-helper pred key alist) - Generic assoc helper using predicate
(define (assoc-helper pred key alist) (if (null? alist) #f (if (pred key (car (car alist))) (car alist) (assoc-helper pred key (cdr alist)))))

;;; ============================================================
;;; Member and Assoc Functions (Using Helpers)
;;; ============================================================

;;; (member x lst) - Find x in lst using equal?, return sublist or #f
(define (member x lst) (mem-helper equal? x lst))

;;; (assoc key alist) - Look up key in association list using equal?
(define (assoc key alist) (assoc-helper equal? key alist))

;;; (range start end) - Generate list of integers [start, end) (tail-recursive)
(define (range start end)
  (define (range-iter n acc)
    (if (< n start)
        acc
        (range-iter (- n 1) (cons n acc))))
  (range-iter (- end 1) '()))

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
;;; R7RS: The value returned is unspecified
;;; We use (if #f #f) to produce an unspecified value (standard Scheme idiom)
(define (for-each f lst) (if (null? lst) (if #f #f) (begin (f (car lst)) (for-each f (cdr lst)))))

;;; (list-tail lst k) - Return sublist starting at k-th element
(define (list-tail lst k) (if (= k 0) lst (list-tail (cdr lst) (- k 1))))

;;; (list-ref lst k) - Return k-th element of lst (0-indexed)
(define (list-ref lst k) (if (= k 0) (car lst) (list-ref (cdr lst) (- k 1))))

;;; (list? obj) - Check if obj is a proper list
(define (list? obj) (if (null? obj) #t (if (pair? obj) (list? (cdr obj)) #f)))

;;; (list-copy lst) - Create a shallow copy of a list
(define (list-copy lst) (if (null? lst) '() (cons (car lst) (list-copy (cdr lst)))))

;;; (memq obj lst) - Find obj in lst using eq?, return sublist or #f
(define (memq obj lst) (mem-helper eq? obj lst))

;;; (memv obj lst) - Find obj in lst using eqv?, return sublist or #f
(define (memv obj lst) (mem-helper eqv? obj lst))

;;; (assq key alist) - Look up key in alist using eq?
(define (assq key alist) (assoc-helper eq? key alist))

;;; (assv key alist) - Look up key in alist using eqv?
(define (assv key alist) (assoc-helper eqv? key alist))

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

;;; (sqrt x) - Integer square root using Newton's method
;;; Returns the largest integer whose square is <= x
(define (sqrt x)
  (if (<= x 0)
      0
      (letrec ((iter (lambda (guess)
                       (let ((next (/ (+ guess (/ x guess)) 2)))
                         (if (>= next guess)
                             guess
                             (iter next))))))
        (iter x))))

;;; (square x) - Return x squared
(define (square x) (* x x))

;;; (cube x) - Return x cubed
(define (cube x) (* x x x))

;;; (sum lst) - Sum all elements in a list
(define (sum lst) (fold + 0 lst))

;;; (product lst) - Product of all elements in a list
(define (product lst) (fold * 1 lst))

;;; (average lst) - Average of all elements in a list
(define (average lst) (/ (sum lst) (length lst)))

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
(define (member-equal obj lst) (mem-helper equal? obj lst))

;;; (assoc-equal key alist) - Look up key in alist using equal?
(define (assoc-equal key alist) (assoc-helper equal? key alist))

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

;;; (filter-map f lst) - Map f over lst, keeping only non-#f results (tail-recursive, no double calls)
(define (filter-map f lst)
  (define (filter-map-iter lst acc)
    (if (null? lst)
        (reverse acc)
        (let ((result (f (car lst))))
          (if result
              (filter-map-iter (cdr lst) (cons result acc))
              (filter-map-iter (cdr lst) acc)))))
  (filter-map-iter lst '()))

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

;;; ============================================================
;;; Additional R7RS List Functions (SRFI-1 compatible)
;;; ============================================================

;;; (iota1 count) - Generate list of integers [0, count)
(define (iota1 count)
  (iota-helper count 0 1 '()))

;;; (iota2 count start) - Generate list of integers [start, start+count)
(define (iota2 count start)
  (iota-helper count start 1 '()))

;;; (iota3 count start step) - Generate arithmetic sequence
(define (iota3 count start step)
  (iota-helper count start step '()))

(define (iota-helper count start step acc)
  (if (<= count 0)
      (reverse acc)
      (iota-helper (- count 1) (+ start step) step (cons start acc))))

;;; (list-tabulate n proc) - Create list by applying proc to 0..n-1
(define (list-tabulate n proc)
  (list-tabulate-helper n 0 proc '()))

(define (list-tabulate-helper n i proc acc)
  (if (>= i n)
      (reverse acc)
      (list-tabulate-helper n (+ i 1) proc (cons (proc i) acc))))

;;; (circular-list x ...) - Create a circular list (infinite)
;;; Note: This is dangerous - use carefully or not at all in finite memory

;;; (first lst) - Return first element (alias for car)
(define (first lst) (car lst))

;;; (second lst) - Return second element
(define (second lst) (cadr lst))

;;; (third lst) - Return third element
(define (third lst) (caddr lst))

;;; (fourth lst) - Return fourth element
(define (fourth lst) (cadddr lst))

;;; (fifth lst) - Return fifth element
(define (fifth lst) (car (cddddr lst)))

;;; (sixth lst) - Return sixth element
(define (sixth lst) (cadr (cddddr lst)))

;;; (seventh lst) - Return seventh element
(define (seventh lst) (caddr (cddddr lst)))

;;; (eighth lst) - Return eighth element
(define (eighth lst) (cadddr (cddddr lst)))

;;; (ninth lst) - Return ninth element
(define (ninth lst) (car (cddddr (cddddr lst))))

;;; (tenth lst) - Return tenth element
(define (tenth lst) (cadr (cddddr (cddddr lst))))

;;; (take-right lst k) - Return the last k elements of lst
;;; Uses lag-pointer technique: O(n) single traversal instead of O(n) for length + O(n) for drop
(define (take-right lst k)
  (define (advance p count)
    (if (= count 0)
        p
        (if (null? p)
            '()
            (advance (cdr p) (- count 1)))))
  (define (walk lead lag)
    (if (null? lead)
        lag
        (walk (cdr lead) (cdr lag))))
  (let ((lead (advance lst k)))
    (if (null? lead)
        lst
        (walk lead lst))))

;;; (drop-right lst k) - Return all but the last k elements
;;; Uses lag-pointer technique: O(n) single traversal, tail-recursive with accumulator
(define (drop-right lst k)
  (define (advance p count)
    (if (= count 0)
        p
        (if (null? p)
            '()
            (advance (cdr p) (- count 1)))))
  (define (walk lead lag acc)
    (if (null? lead)
        (reverse acc)
        (walk (cdr lead) (cdr lag) (cons (car lag) acc))))
  (let ((lead (advance lst k)))
    (if (null? lead)
        '()
        (walk lead lst '()))))

;;; (split-at lst k) - Split list at position k, returns (take . drop)
(define (split-at lst k)
  (cons (take k lst) (drop k lst)))

;;; (concatenate lsts) - Append all lists in lsts
(define (concatenate lsts)
  (fold-right append-two '() lsts))

;;; (flatten lst) - Flatten a nested list structure (O(n) tail-recursive)
(define (flatten lst)
  (define (flatten-iter lst acc)
    (cond
      ((null? lst) acc)
      ((not (pair? lst)) (cons lst acc))
      (else (flatten-iter (car lst) (flatten-iter (cdr lst) acc)))))
  (flatten-iter lst '()))

;;; (count pred lst) - Count elements satisfying predicate
(define (count pred lst)
  (fold (lambda (acc x) (if (pred x) (+ acc 1) acc)) 0 lst))

;;; ============================================================
;;; Additional String Functions
;;; ============================================================

;;; (string-for-each proc s) - Apply proc to each character for side effects
(define (string-for-each proc s)
  (for-each proc (string->list s)))

;;; (string-map proc s) - Map proc over characters, return new string
(define (string-map proc s)
  (list->string (map proc (string->list s))))

;;; (string-null? s) - Check if string is empty
(define (string-null? s)
  (= (string-length s) 0))

;;; (string-reverse s) - Reverse a string
(define (string-reverse s)
  (list->string (reverse (string->list s))))

;;; (string-contains s1 s2) - Check if s2 is a substring of s1
;;; Returns index of first occurrence or #f
(define (string-contains s1 s2)
  (let ((len1 (string-length s1))
        (len2 (string-length s2)))
    (if (> len2 len1)
        #f
        (string-contains-helper s1 s2 0 len1 len2))))

(define (string-contains-helper s1 s2 i len1 len2)
  (if (> (+ i len2) len1)
      #f
      (if (string-prefix? s1 s2 i)
          i
          (string-contains-helper s1 s2 (+ i 1) len1 len2))))

(define (string-prefix? s1 s2 start)
  (string-prefix-helper s1 s2 start 0 (string-length s2)))

(define (string-prefix-helper s1 s2 i j len2)
  (if (>= j len2)
      #t
      (if (char=? (string-ref s1 i) (string-ref s2 j))
          (string-prefix-helper s1 s2 (+ i 1) (+ j 1) len2)
          #f)))

;;; (string-join lst sep) - Join list of strings with separator
(define (string-join lst sep)
  (if (null? lst)
      ""
      (fold (lambda (acc s) (string-append acc sep s))
            (car lst)
            (cdr lst))))

;;; (string-split s sep) - Split string by separator character
;;; Returns list of strings
(define (string-split s sep)
  (string-split-helper (string->list s) sep '() '()))

(define (string-split-helper chars sep current result)
  (cond
    ((null? chars)
     (reverse (cons (list->string (reverse current)) result)))
    ((char=? (car chars) sep)
     (string-split-helper (cdr chars) sep '() 
                          (cons (list->string (reverse current)) result)))
    (else
     (string-split-helper (cdr chars) sep (cons (car chars) current) result))))

;;; (string-trim s) - Remove leading and trailing whitespace
(define (string-trim s)
  (list->string (reverse (drop-while-ws (reverse (drop-while-ws (string->list s)))))))

(define (drop-while-ws lst)
  (cond
    ((null? lst) '())
    ((char-whitespace? (car lst)) (drop-while-ws (cdr lst)))
    (else lst)))

;;; ============================================================
;;; Lazy Evaluation Functions (R7RS Section 4.2.5)
;;; ============================================================

;;; (promise? obj) - Check if obj is a promise
;;; Note: In this implementation, promises are procedures (thunks).
;;; This is consistent with R7RS which says "promises are not necessarily
;;; disjoint from other Scheme types such as procedures."
(define (promise? obj)
  (procedure? obj))

;;; (make-promise obj) - Create a promise that returns obj when forced
;;; If obj is already a promise, it is returned unchanged.
;;; This is a procedure, not syntax - it does not delay evaluation.
(define (make-promise obj)
  (if (promise? obj)
      obj
      (lambda () obj)))


;;; ============================================================
;;; Effect System Support (Pure Functional Grift)
;;; ============================================================

;;; (run-io effect) - Interpret and execute an IO effect
;;; This is the standard IO effect handler that actually performs IO.
;;; It's the boundary between pure code (effect descriptions) and the outside world.
;;;
;;; Supported effects:
;;; - io/pure: Return the wrapped value
;;; - io/bind: Sequence effects
;;; - io/print: Print to output
;;; - io/read-line: Read a line (currently returns empty string as placeholder)
;;;
;;; Example:
;;;   (run-io (io/print "Hello, World!"))  ; Actually prints
;;;   (run-io (io/pure 42))                ; => 42
(define (run-io effect)
  (if (effect? effect)
      (let ((tag (effect-tag effect))
            (data (effect-data effect)))
        (cond
          ;; io/pure - just return the value
          ((eq? tag 'io/pure)
           data)
          
          ;; io/bind - execute first effect, pass result to continuation
          ((eq? tag 'io/bind)
           (let ((first-effect (car data))
                 (continuation (cdr data)))
             (let ((result (run-io first-effect)))
               (run-io (continuation result)))))
          
          ;; io/print - actually print the value
          ((eq? tag 'io/print)
           (display data)
           (newline)
           #t)  ; Return #t to indicate success
          
          ;; io/read-line - placeholder that returns empty string
          ;; NOTE: Actual stdin reading requires I/O primitives not available in stdlib.
          ;; For real input, use the REPL's input mechanism or implement a custom
          ;; handler that calls native read-line functionality.
          ((eq? tag 'io/read-line)
           "")
          
          ;; Unknown effect - error
          (else
           (error "run-io: unknown effect type" tag))))
      ;; Not an effect - return as-is
      effect))

;;; (effect-sequence effects) - Sequence a list of effects, return last result
;;; Example: (effect-sequence (list (io/print "a") (io/print "b") (io/pure 42)))
(define (effect-sequence effects)
  (if (null? effects)
      (io/pure #f)
      (if (null? (cdr effects))
          (car effects)
          (io/bind (car effects)
                   (lambda (_) (effect-sequence (cdr effects)))))))

;;; (effect-for-each f effects) - Apply f to each effect's result for side effects
;;; Returns unit (void-like) effect
(define (effect-for-each f effects)
  (effect-sequence (map (lambda (e) (io/bind e (lambda (x) (io/pure (f x))))) effects)))

;;; ============================================================
;;; State Effect Handler
;;; ============================================================

;;; (run-state initial-state effect) - Run a stateful computation
;;; Returns (cons final-value final-state)
;;;
;;; State effects:
;;;   (state/get)       - Get current state value
;;;   (state/put v)     - Set state to v, returns old state
;;;   (state/modify f)  - Apply f to state, returns old state
;;;
;;; Example:
;;;   (run-state 0
;;;     (io/bind (state/get)
;;;       (lambda (s)
;;;         (io/bind (state/put (+ s 1))
;;;           (lambda (_)
;;;             (io/pure s))))))
;;;   ; => (0 . 1) - returned 0 (initial state), final state is 1
(define (run-state initial-state effect)
  (letrec ((loop
            (lambda (state current-effect)
              (if (effect? current-effect)
                  (let ((tag (effect-tag current-effect))
                        (data (effect-data current-effect)))
                    (cond
                      ;; io/pure - return value with current state
                      ((eq? tag 'io/pure)
                       (cons data state))
                      
                      ;; io/bind - run first effect, pass result to continuation
                      ((eq? tag 'io/bind)
                       (let* ((first-effect (car data))
                              (continuation (cdr data))
                              (result-pair (loop state first-effect))
                              (result (car result-pair))
                              (new-state (cdr result-pair)))
                         (loop new-state (continuation result))))
                      
                      ;; state/get - return current state
                      ((eq? tag 'state/get)
                       (cons state state))
                      
                      ;; state/put - update state, return void
                      ((eq? tag 'state/put)
                       (cons #f data))  ; Return void, new state is data
                      
                      ;; state/modify - apply function to state
                      ((eq? tag 'state/modify)
                       (cons state (data state)))  ; Return old state, new state is (data state)
                      
                      ;; Unknown effect - pass through as pure value
                      (else
                       (cons current-effect state))))
                  ;; Not an effect - return as-is with state
                  (cons current-effect state)))))
    (loop initial-state effect)))

;;; Convenience function: run-state returning just the value
(define (eval-state initial-state effect)
  (car (run-state initial-state effect)))

;;; Convenience function: run-state returning just the final state  
(define (exec-state initial-state effect)
  (cdr (run-state initial-state effect)))

;;; ============================================================
;;; Error Effect Handler
;;; ============================================================

;;; (run-error effect) - Run a computation that may raise errors
;;; Returns (list 'ok value) on success, (list 'error err) on error
;;;
;;; Error effects:
;;;   (error/raise e) - Raise an error with value e
;;;
;;; Example:
;;;   (run-error (io/pure 42))                ; => (ok 42)
;;;   (run-error (error/raise 'not-found))    ; => (error not-found)
(define (run-error effect)
  (letrec ((loop
            (lambda (current-effect)
              (if (effect? current-effect)
                  (let ((tag (effect-tag current-effect))
                        (data (effect-data current-effect)))
                    (cond
                      ;; io/pure - return success with value
                      ((eq? tag 'io/pure)
                       (list 'ok data))
                      
                      ;; io/bind - run first effect, pass result to continuation if successful
                      ((eq? tag 'io/bind)
                       (let* ((first-effect (car data))
                              (continuation (cdr data))
                              (result (loop first-effect)))
                         (if (eq? (car result) 'ok)
                             (loop (continuation (car (cdr result))))
                             result)))  ; Propagate error
                      
                      ;; error/raise - return error
                      ((eq? tag 'error/raise)
                       (list 'error data))
                      
                      ;; Unknown effect - treat as pure value
                      (else
                       (list 'ok current-effect))))
                  ;; Not an effect - return as success
                  (list 'ok current-effect)))))
    (loop effect)))

;;; (try-error effect handler) - Try to run effect, call handler on error
;;; handler is a function that takes the error value and returns an effect
;;;
;;; Example:
;;;   (try-error 
;;;     (io/bind (io/pure 10)
;;;       (lambda (x) (if (> x 5) (error/raise 'too-big) (io/pure x))))
;;;     (lambda (err) (io/pure 0)))  ; Return 0 on any error
(define (try-error effect handler)
  (let ((result (run-error effect)))
    (if (eq? (car result) 'ok)
        (io/pure (car (cdr result)))
        (handler (car (cdr result))))))

;; ============================================================================
;; Effect Type System
;; ============================================================================
;;
;; This section implements a foundation for effect types. Since Grift is
;; dynamically typed, effect types are represented as data values that can
;; be used for:
;; 1. Documentation (annotating expected effects)
;; 2. Runtime checking (validating effect handlers cover all effects)
;; 3. Future static analysis
;;
;; Effect types are represented as tagged lists:
;;   (effect-type io)                - IO effect
;;   (effect-type state <state-type>) - Stateful computation
;;   (effect-type error <error-type>) - May raise errors
;;   (effect-type pure)              - No effects (pure computation)
;;   (effect-type union <type1> <type2> ...) - Multiple effects

;;; (eff-type/io) - Create an IO effect type
(define (eff-type/io)
  '(effect-type io))

;;; (eff-type/state state-type) - Create a State effect type
;;; state-type describes the type of state (e.g., 'int, 'string, etc.)
(define (eff-type/state state-type)
  (list 'effect-type 'state state-type))

;;; (eff-type/error error-type) - Create an Error effect type  
;;; error-type describes what errors may be raised
(define (eff-type/error error-type)
  (list 'effect-type 'error error-type))

;;; (eff-type/pure) - Create a Pure (no effects) type
(define (eff-type/pure)
  '(effect-type pure))

;;; (eff-type/union2 type1 type2) - Combine two effect types into a union
;;; For combining more than 2 types, nest calls: (eff-type/union2 a (eff-type/union2 b c))
(define (eff-type/union2 type1 type2)
  (list 'effect-type 'union type1 type2))

;;; (eff-type? x) - Check if x is an effect type
(define (eff-type? x)
  (and (pair? x) (eq? (car x) 'effect-type)))

;;; (eff-type-kind type) - Get the kind of effect type (io, state, error, pure, union)
(define (eff-type-kind type)
  (if (eff-type? type)
      (car (cdr type))
      #f))

;;; (eff-type-param type) - Get the parameter of an effect type (e.g., state-type)
(define (eff-type-param type)
  (if (and (eff-type? type) (pair? (cdr (cdr type))))
      (car (cdr (cdr type)))
      #f))

;;; (eff-type-union-members type) - Get the member types of a union
(define (eff-type-union-members type)
  (if (and (eff-type? type) (eq? (eff-type-kind type) 'union))
      (cdr (cdr type))
      '()))

;;; (effect-type effect) - Infer the effect type from an effect value
;;; Returns the appropriate effect type for the given effect
(define (effect-type effect)
  (if (effect? effect)
      (let ((tag (effect-tag effect)))
        (cond
          ;; IO effects
          ((eq? tag 'io/pure) (eff-type/pure))
          ((eq? tag 'io/print) (eff-type/io))
          ((eq? tag 'io/read-line) (eff-type/io))
          ((eq? tag 'io/bind) 
           ;; For bind, the type is the union of inner effects
           ;; This is a simplification - full inference would require monadic composition
           (eff-type/io))
          
          ;; State effects
          ((eq? tag 'state/get) (eff-type/state 'any))
          ((eq? tag 'state/put) (eff-type/state 'any))
          ((eq? tag 'state/modify) (eff-type/state 'any))
          
          ;; Error effects
          ((eq? tag 'error/raise) (eff-type/error 'any))
          
          ;; Unknown effect - return generic type
          (else (list 'effect-type 'unknown tag))))
      ;; Not an effect - pure
      (eff-type/pure)))

;;; (eff-type-covers? handler-type effect-type) - Check if handler covers effect
;;; Returns #t if the handler type covers all effects in effect-type
(define (eff-type-covers? handler-type effect-type)
  (let ((h-kind (eff-type-kind handler-type))
        (e-kind (eff-type-kind effect-type)))
    (cond
      ;; Pure effects need no handler
      ((eq? e-kind 'pure) #t)
      
      ;; Union effect requires all members to be covered
      ((eq? e-kind 'union)
       (eff-type-covers-all? handler-type (eff-type-union-members effect-type)))
      
      ;; Union handler covers anything in its members
      ((eq? h-kind 'union)
       (eff-type-covers-any? (eff-type-union-members handler-type) effect-type))
      
      ;; Direct match
      (else (eq? h-kind e-kind)))))

;;; (eff-type-covers-all? handler members) - Check if handler covers all member types
(define (eff-type-covers-all? handler members)
  (if (null? members)
      #t
      (and (eff-type-covers? handler (car members))
           (eff-type-covers-all? handler (cdr members)))))

;;; (eff-type-covers-any? members effect) - Check if any member covers the effect
(define (eff-type-covers-any? members effect)
  (if (null? members)
      #f
      (or (eff-type-covers? (car members) effect)
          (eff-type-covers-any? (cdr members) effect))))

;;; (eff-type->string type) - Convert effect type to string for display
(define (eff-type->string type)
  (if (eff-type? type)
      (let ((kind (eff-type-kind type)))
        (cond
          ((eq? kind 'pure) "Pure")
          ((eq? kind 'io) "IO")
          ((eq? kind 'state) 
           (string-append "State<" (symbol->string (or (eff-type-param type) 'any)) ">"))
          ((eq? kind 'error)
           (string-append "Error<" (symbol->string (or (eff-type-param type) 'any)) ">"))
          ((eq? kind 'union)
           (let ((members (eff-type-union-members type)))
             (define (join-with sep lst)
               (if (null? lst)
                   ""
                   (if (null? (cdr lst))
                       (eff-type->string (car lst))
                       (string-append (eff-type->string (car lst)) sep (join-with sep (cdr lst))))))
             (string-append "(" (join-with " + " members) ")")))
          (else (string-append "Unknown<" (symbol->string (or kind 'any)) ">"))))
      "NotAnEffectType"))

;;; (describe-effect effect) - Get a description of an effect including its type
(define (describe-effect effect)
  (if (effect? effect)
      (list 'effect
            (list 'tag (effect-tag effect))
            (list 'type (eff-type->string (effect-type effect)))
            (list 'data (effect-data effect)))
      (list 'not-an-effect effect)))
