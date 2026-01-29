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
