;; R7RS test suite ported to SRFI-64 format
;; Originally from the Chibi Scheme R7RS test suite

;; Helper for test-values (converts multi-value returns to lists)
(define-syntax test-values
  (syntax-rules ()
    ((_ expected-expr actual-expr)
     (test-equal "test-values"
       (call-with-values (lambda () expected-expr) list)
       (call-with-values (lambda () actual-expr) list)))))

;; Compatibility wrapper for old Chibi-style (test expected expr) calls
(define-syntax test
  (syntax-rules ()
    ((_ expected expr)
     (test-equal "test" expected expr))
    ((_ name expected expr)
     (test-equal name expected expr))))



(test-begin "R7RS")

(test-begin "4.1 Primitive expression types")

(let ()
  (define x 28)
  (test-equal "r7rs-1" 28 x))

(test-equal "r7rs-2" 'a (quote a))
(test-equal "r7rs-3" #(a b c) (quote #(a b c)))
(test-equal "r7rs-4" '(+ 1 2) (quote (+ 1 2)))

(test-equal "r7rs-5" 'a 'a)
(test-equal "r7rs-6" #(a b c) '#(a b c))
(test-equal "r7rs-7" '() '())
(test-equal "r7rs-8" '(+ 1 2) '(+ 1 2))
(test-equal "r7rs-9" '(quote a) '(quote a))
(test-equal "r7rs-10" '(quote a) ''a)

(test-equal "r7rs-11" "abc" '"abc")
(test-equal "r7rs-12" "abc" "abc")
(test-equal "r7rs-13" 145932 '145932)
(test-equal "r7rs-14" 145932 145932)
(test-equal "r7rs-15" #t '#t)
(test-equal "r7rs-16" #t #t)

(test-equal "r7rs-17" 7 (+ 3 4))
(test-equal "r7rs-18" 12 ((if #f + *) 3 4))

(test-equal "r7rs-19" 8 ((lambda (x) (+ x x)) 4))
(define reverse-subtract
  (lambda (x y) (- y x)))
(test-equal "r7rs-20" 3 (reverse-subtract 7 10))
(define add4
  (let ((x 4))
    (lambda (y) (+ x y))))
(test-equal "r7rs-21" 10 (add4 6))

(test-equal "r7rs-22" '(3 4 5 6) ((lambda x x) 3 4 5 6))
(test-equal "r7rs-23" '(5 6) ((lambda (x y . z) z)
 3 4 5 6))

(test-equal "r7rs-24" 'yes (if (> 3 2) 'yes 'no))
(test-equal "r7rs-25" 'no (if (> 2 3) 'yes 'no))
(test-equal "r7rs-26" 1 (if (> 3 2)
    (- 3 2)
    (+ 3 2)))
(let ()
  (define x 2)
  (test-equal "r7rs-27" 3 (+ x 1)))

(test-end)

(test-begin "4.2 Derived expression types")

(test-equal "r7rs-28" 'greater (cond ((> 3 2) 'greater)
          ((< 3 2) 'less)))

(test-equal "r7rs-29" 'equal (cond ((> 3 3) 'greater)
          ((< 3 3) 'less)
          (else 'equal)))

(test-equal "r7rs-30" 2 (cond ((assv 'b '((a 1) (b 2))) => cadr)
          (else #f)))

(test-equal "r7rs-31" 'composite (case (* 2 3)
      ((2 3 5 7) 'prime)
      ((1 4 6 8 9) 'composite)))

(test-equal "r7rs-32" 'c (case (car '(c d))
      ((a e i o u) 'vowel)
      ((w y) 'semivowel)
      (else => (lambda (x) x))))

(test-equal "r7rs-33" '((other . z) (semivowel . y) (other . x)
        (semivowel . w) (vowel . u)) (map (lambda (x)
           (case x
             ((a e i o u) => (lambda (w) (cons 'vowel w)))
             ((w y) (cons 'semivowel x))
             (else => (lambda (w) (cons 'other w)))))
         '(z y x w u)))

(test-equal "r7rs-34" #t (and (= 2 2) (> 2 1)))
(test-equal "r7rs-35" #f (and (= 2 2) (< 2 1)))
(test-equal "r7rs-36" '(f g) (and 1 2 'c '(f g)))
(test-equal "r7rs-37" #t (and))

(test-equal "r7rs-38" #t (or (= 2 2) (> 2 1)))
(test-equal "r7rs-39" #t (or (= 2 2) (< 2 1)))
(test-equal "r7rs-40" #f (or #f #f #f))
(test-equal "r7rs-41" '(b c) (or (memq 'b '(a b c))
    (/ 3 0)))

(test-equal "r7rs-42" 6 (let ((x 2) (y 3))
  (* x y)))

(test-equal "r7rs-43" 35 (let ((x 2) (y 3))
  (let ((x 7)
        (z (+ x y)))
    (* z x))))

(test-equal "r7rs-44" 70 (let ((x 2) (y 3))
  (let* ((x 7)
         (z (+ x y)))
    (* z x))))

(test-equal "r7rs-45" #t (letrec ((even?
              (lambda (n)
                (if (zero? n)
                    #t
                    (odd? (- n 1)))))
             (odd?
              (lambda (n)
                (if (zero? n)
                    #f
                    (even? (- n 1))))))
      (even? 88)))

(test-equal "r7rs-46" 5 (letrec* ((p
               (lambda (x)
                 (+ 1 (q (- x 1)))))
              (q
               (lambda (y)
                 (if (zero? y)
                     0
                     (+ 1 (p (- y 1))))))
              (x (p 5))
              (y x))
             y))

;; By Jussi Piitulainen <jpiitula@ling.helsinki.fi>
;; and John Cowan <cowan@mercury.ccil.org>:
;; http://lists.scheme-reports.org/pipermail/scheme-reports/2013-December/003876.html
(define (means ton)
  (letrec*
     ((mean
        (lambda (f g)
          (f (/ (sum g ton) n))))
      (sum
        (lambda (g ton)
          (if (null? ton)
            (+)
            (if (number? ton)
                (g ton)
                (+ (sum g (car ton))
                   (sum g (cdr ton)))))))
      (n (sum (lambda (x) 1) ton)))
    (values (mean values values)
            (mean exp log)
            (mean / /))))
(let*-values (((a b c) (means '(8 5 99 1 22))))
  (test-equal "r7rs-47" 27 a)
  (test-equal "r7rs-48" 9.728 b)
  (test-equal "r7rs-49" 1800/497 c))

(let*-values (((root rem) (exact-integer-sqrt 32)))
  (test-equal "r7rs-50" 35 (* root rem)))

(test-equal "r7rs-51" '(1073741824 0) (let*-values (((root rem) (exact-integer-sqrt (expt 2 60))))
      (list root rem)))

(test-equal "r7rs-52" '(1518500249 3000631951) (let*-values (((root rem) (exact-integer-sqrt (expt 2 61))))
      (list root rem)))

(test-equal "r7rs-53" '(815238614083298888 443242361398135744) (let*-values (((root rem) (exact-integer-sqrt (expt 2 119))))
      (list root rem)))

(test-equal "r7rs-54" '(1152921504606846976 0) (let*-values (((root rem) (exact-integer-sqrt (expt 2 120))))
      (list root rem)))

(test-equal "r7rs-55" '(1630477228166597776 1772969445592542976) (let*-values (((root rem) (exact-integer-sqrt (expt 2 121))))
      (list root rem)))

;; Bignum tests skipped: grift uses isize, no bignum support
;; (test-equal "r7rs-56" '(31622776601683793319 62545769258890964239) (let*-values (((root rem) (exact-integer-sqrt (expt 10 39))))
;;       (list root rem)))
;;
;; (let*-values (((root rem) (exact-integer-sqrt (expt 2 140))))
;;   (test-equal "r7rs-57" 0 rem)
;;   (test-equal "r7rs-58" (expt 2 140) (square root)))

(test-equal "r7rs-59" '(x y x y) (let ((a 'a) (b 'b) (x 'x) (y 'y))
  (let*-values (((a b) (values x y))
                ((x y) (values a b)))
    (list a b x y))))

(test-equal "r7rs-60" 'ok (let-values () 'ok))

(test-equal "r7rs-61" 1 (let ((x 1))
	  (let*-values ()
	    (define x 2)
	    #f)
	  x))

(let ()
  (define x 0)
  (set! x 5)
  (test-equal "r7rs-62" 6 (+ x 1)))

(test-equal "r7rs-63" #(0 1 2 3 4) (do ((vec (make-vector 5))
     (i 0 (+ i 1)))
    ((= i 5) vec)
  (vector-set! vec i i)))

(test-equal "r7rs-64" 25 (let ((x '(1 3 5 7 9)))
  (do ((x x (cdr x))
       (sum 0 (+ sum (car x))))
      ((null? x) sum))))

(test-equal "r7rs-65" '((6 1 3) (-5 -2)) (let loop ((numbers '(3 -2 1 6 -5))
               (nonneg '())
               (neg '()))
      (cond ((null? numbers) (list nonneg neg))
            ((>= (car numbers) 0)
             (loop (cdr numbers)
                   (cons (car numbers) nonneg)
                   neg))
            ((< (car numbers) 0)
             (loop (cdr numbers)
                   nonneg
                   (cons (car numbers) neg))))))

(test-equal "r7rs-66" 3 (force (delay (+ 1 2))))

(test-equal "r7rs-67" '(3 3) (let ((p (delay (+ 1 2))))
      (list (force p) (force p))))

(define integers
  (letrec ((next
            (lambda (n)
              (delay (cons n (next (+ n 1)))))))
    (next 0)))
(define head
  (lambda (stream) (car (force stream))))
(define tail
  (lambda (stream) (cdr (force stream))))

(test-equal "r7rs-68" 2 (head (tail (tail integers))))

(define (stream-filter p? s)
  (delay-force
   (if (null? (force s)) 
       (delay '())
       (let ((h (car (force s)))
             (t (cdr (force s))))
         (if (p? h)
             (delay (cons h (stream-filter p? t)))
             (stream-filter p? t))))))

(test-equal "r7rs-69" 5 (head (tail (tail (stream-filter odd? integers)))))

(let ()
  (define x 5)
  (define count 0)
  (define p
    (delay (begin (set! count (+ count 1))
                  (if (> count x)
                      count
                      (force p)))))
  (test-equal "r7rs-70" 6 (force p))
  (test-equal "r7rs-71" 6 (begin (set! x 10) (force p))))

(test-equal "r7rs-72" #t (promise? (delay (+ 2 2))))
(test-equal "r7rs-73" #t (promise? (make-promise (+ 2 2))))
(test-equal "r7rs-74" #t (let ((x (delay (+ 2 2))))
      (force x)
      (promise? x)))
(test-equal "r7rs-75" #t (let ((x (make-promise (+ 2 2))))
      (force x)
      (promise? x)))
(test-equal "r7rs-76" 4 (force (make-promise (+ 2 2))))
(test-equal "r7rs-77" 4 (force (make-promise (make-promise (+ 2 2)))))

(define radix
  (make-parameter
   10
   (lambda (x)
     (if (and (integer? x) (<= 2 x 16))
         x
         (error "invalid radix")))))
(define (f n) (number->string n (radix)))
(test-equal "r7rs-78" "12" (f 12))
(test-equal "r7rs-79" "1100" (parameterize ((radix 2))
  (f 12)))
(test-equal "r7rs-80" "12" (f 12))

(test-equal "r7rs-81" '(list 3 4) `(list ,(+ 1 2) 4))
(let ((name 'a)) (test-equal "r7rs-82" '(list a (quote a)) `(list ,name ',name)))
(test-equal "r7rs-83" '(a 3 4 5 6 b) `(a ,(+ 1 2) ,@(map abs '(4 -5 6)) b))
(test-equal "r7rs-84" #(10 5 4 16 9 8) `#(10 5 ,(square 2) ,@(map square '(4 3)) 8))
(test-equal "r7rs-85" '(a `(b ,(+ 1 2) ,(foo 4 d) e) f) `(a `(b ,(+ 1 2) ,(foo ,(+ 1 3) d) e) f))
(let ((name1 'x)
      (name2 'y))
   (test-equal "r7rs-86" '(a `(b ,x ,'y d) e) `(a `(b ,,name1 ,',name2 d) e)))
(test-equal "r7rs-87" '(list 3 4) (quasiquote (list (unquote (+ 1 2)) 4)))
(test-equal "r7rs-88" `(list ,(+ 1 2) 4) (quasiquote (list (unquote (+ 1 2)) 4)))

(define any-arity
  (case-lambda 
    (() 'zero)
    ((x) x)
    ((x y) (cons x y))
    ((x y z) (list x y z))
    (args (cons 'many args))))

(test-equal "r7rs-89" 'zero (any-arity))
(test-equal "r7rs-90" 1 (any-arity 1))
(test-equal "r7rs-91" '(1 . 2) (any-arity 1 2))
(test-equal "r7rs-92" '(1 2 3) (any-arity 1 2 3))
(test-equal "r7rs-93" '(many 1 2 3 4) (any-arity 1 2 3 4))

(define rest-arity
  (case-lambda 
    (() '(zero))
    ((x) (list 'one x))
    ((x y) (list 'two x y))
    ((x y . z) (list 'more x y z))))

(test-equal "r7rs-94" '(zero) (rest-arity))
(test-equal "r7rs-95" '(one 1) (rest-arity 1))
(test-equal "r7rs-96" '(two 1 2) (rest-arity 1 2))
(test-equal "r7rs-97" '(more 1 2 (3)) (rest-arity 1 2 3))

(define dead-clause
  (case-lambda
    ((x . y) 'many)
    (() 'none)
    (foo 'unreachable)))

(test-equal "r7rs-98" 'none (dead-clause))
(test-equal "r7rs-99" 'many (dead-clause 1))
(test-equal "r7rs-100" 'many (dead-clause 1 2))
(test-equal "r7rs-101" 'many (dead-clause 1 2 3))

(test-end)

(test-begin "4.3 Macros")

(test-equal "r7rs-102" 'now (let-syntax
               ((when (syntax-rules ()
                        ((when test stmt1 stmt2 ...)
                         (if test
                             (begin stmt1
                                    stmt2 ...))))))
             (let ((if #t))
               (when if (set! if 'now))
               if)))

(test-equal "r7rs-103" 'outer (let ((x 'outer))
  (let-syntax ((m (syntax-rules () ((m) x))))
    (let ((x 'inner))
      (m)))))

(test-equal "r7rs-104" 7 (letrec-syntax
  ((my-or (syntax-rules ()
            ((my-or) #f)
            ((my-or e) e)
            ((my-or e1 e2 ...)
             (let ((temp e1))
               (if temp
                   temp
                   (my-or e2 ...)))))))
  (let ((x #f)
        (y 7)
        (temp 8)
        (let odd?)
        (if even?))
    (my-or x
           (let temp)
           (if y)
           y))))

(define-syntax be-like-begin1
  (syntax-rules ()
    ((be-like-begin1 name)
     (define-syntax name
       (syntax-rules ()
         ((name expr (... ...))
          (begin expr (... ...))))))))
(be-like-begin1 sequence1)
(test-equal "r7rs-105" 3 (sequence1 0 1 2 3))

(define-syntax be-like-begin2
  (syntax-rules ()
    ((be-like-begin2 name)
     (define-syntax name
       (... (syntax-rules ()
              ((name expr ...)
               (begin expr ...))))))))
(be-like-begin2 sequence2)
(test-equal "r7rs-106" 4 (sequence2 1 2 3 4))

;; be-like-begin3 uses custom ellipsis identifier (syntax-rules dots ())
;; which is not yet supported by grift's macro system
;; (define-syntax be-like-begin3
;;   (syntax-rules ()
;;     ((be-like-begin3 name)
;;      (define-syntax name
;;        (syntax-rules dots ()
;;          ((name expr dots)
;;           (begin expr dots)))))))
;; (be-like-begin3 sequence3)
;; (test-equal "r7rs-107" 5 (sequence3 2 3 4 5))

;; ellipsis escape
(define-syntax elli-esc-1
  (syntax-rules ()
    ((_)
     '(... ...))
    ((_ x)
     '(... (x ...)))
    ((_ x y)
     '(... (... x y)))))

(test-equal "r7rs-108" '... (elli-esc-1))
(test-equal "r7rs-109" '(100 ...) (elli-esc-1 100))
(test-equal "r7rs-110" '(... 100 200) (elli-esc-1 100 200))

;; Syntax pattern with ellipsis in middle of proper list.
(define-syntax part-2
  (syntax-rules ()
    ((_ a b (m n) ... x y)
     (vector (list a b) (list m ...) (list n ...) (list x y)))
    ((_ . rest) 'error)))
(test-equal "r7rs-111" '#((10 43) (31 41 51) (32 42 52) (63 77)) (part-2 10 (+ 21 22) (31 32) (41 42) (51 52) (+ 61 2) 77))
;; Syntax pattern with ellipsis in middle of improper list.
(define-syntax part-2x
  (syntax-rules ()
    ((_ (a b (m n) ... x y . rest))
     (vector (list a b) (list m ...) (list n ...) (list x y)
             (cons "rest:" 'rest)))
    ((_ . rest) 'error)))
(test-equal "r7rs-112" '#((10 43) (31 41 51) (32 42 52) (63 77) ("rest:")) (part-2x (10 (+ 21 22) (31 32) (41 42) (51 52) (+ 61 2) 77)))
(test-equal "r7rs-113" '#((10 43) (31 41 51) (32 42 52) (63 77) ("rest:" . "tail")) (part-2x (10 (+ 21 22) (31 32) (41 42) (51 52) (+ 61 2) 77 . "tail")))

;; underscore
(define-syntax underscore
  (syntax-rules ()
    ((foo _) '_)))
(test-equal "r7rs-114" '_ (underscore foo))

(let ()
  (define-syntax underscore2
    (syntax-rules ()
      ((underscore2 (a _) ...) 42)))
  (test-equal "r7rs-115" 42 (underscore2 (1 2))))

(define-syntax count-to-2
  (syntax-rules ()
    ((_) 0)
    ((_ _) 1)
    ((_ _ _) 2)
    ((_ . _) 'many)))
(test-equal "r7rs-116" '(2 0 many) (list (count-to-2 a b) (count-to-2) (count-to-2 a b c d)))

(define-syntax count-to-2_
  (syntax-rules (_)
    ((_) 0)
    ((_ _) 1)
    ((_ _ _) 2)
    ((x . y) 'fail)))
(test-equal "r7rs-117" '(2 0 fail fail) (list (count-to-2_ _ _) (count-to-2_)
          (count-to-2_ a b) (count-to-2_ a b c d)))

(define-syntax jabberwocky
  (syntax-rules ()
    ((_ hatter)
     (begin
       (define march-hare 42)
       (define-syntax hatter
         (syntax-rules ()
           ((_) march-hare)))))))
(jabberwocky mad-hatter)
(test-equal "r7rs-118" 42 (mad-hatter))

(test-equal "r7rs-119" 'ok (let ((=> #f)) (cond (#t => 'ok))))

(let ()
  (define x 1)
  (let-syntax ()
    (define x 2)
    #f)
  (test-equal "r7rs-120" 1 x))

(let ()
 (define-syntax foo
   (syntax-rules ()
     ((foo bar y)
      (define-syntax bar
        (syntax-rules ()
          ((bar x) 'y))))))
 (foo bar x)
 (test-equal "r7rs-121" 'x (bar 1)))

(begin
  (define-syntax ffoo
    (syntax-rules ()
      ((ffoo ff)
       (begin
         (define (ff x)
           (gg x))
         (define (gg x)
           (* x x))))))
  (ffoo ff)
  (test-equal "r7rs-122" 100 (ff 10)))

(let-syntax ((vector-lit
               (syntax-rules ()
                 ((vector-lit)
                  '#(b)))))
  (test-equal "r7rs-123" '#(b) (vector-lit)))

(let ()
  ;; forward hygienic refs
  (define-syntax foo399
    (syntax-rules () ((foo399) (bar399))))
  (define (quux399)
    (foo399))
  (define (bar399)
    42)
  (test-equal "r7rs-124" 42 (quux399)))

;; Hygienic identifier comparison test
;; Grift's syntax-rules does not correctly handle nested let-syntax
;; with literals matching outer pattern variables
;; (let-syntax
;;     ((m (syntax-rules ()
;;           ((m x) (let-syntax
;;                      ((n (syntax-rules (k)
;;                            ((n x) 'bound-identifier=?)
;;                            ((n y) 'free-identifier=?))))
;;                    (n z))))))
;;   (test-equal "r7rs-125" 'bound-identifier=? (m k)))

;; literal has priority to ellipsis (R7RS 4.3.2)
;; Uses custom ellipsis identifier which grift does not yet support
;; (let ()
;;   (define-syntax elli-lit-1
;;     (syntax-rules ... (...)
;;       ((_ x)
;;        '(x ...))))
;;   (test-equal "r7rs-126" '(100 ...) (elli-lit-1 100)))

;; bad ellipsis
#|
(test-equal "r7rs-127" 'error (guard (exn (else 'error))
        (eval
         '(define-syntax bad-elli-1
            (syntax-rules ()
              ((_ ... x)
               '(... x))))
         (interaction-environment))))

(test-equal "r7rs-128" 'error (guard (exn (else 'error))
        (eval
         '(define-syntax bad-elli-2
            (syntax-rules ()
              ((_ (... x))
               '(... x))))
         (interaction-environment))))
|#

(test-end)

(test-begin "5 Program structure")

(define add3
  (lambda (x) (+ x 3)))
(test-equal "r7rs-129" 6 (add3 3))
(define first car)
(test-equal "r7rs-130" 1 (first '(1 2)))

(test-equal "r7rs-131" 45 (let ((x 5))
  (define foo (lambda (y) (bar x y)))
  (define bar (lambda (a b) (+ (* a b) a)))
  (foo (+ x 3))))

(test-equal "r7rs-132" 'ok (let ()
      (define-values () (values))
      'ok))
(test-equal "r7rs-133" 1 (let ()
      (define-values (x) (values 1))
      x))
(test-equal "r7rs-134" 3 (let ()
      (define-values x (values 1 2))
      (apply + x)))
(test-equal "r7rs-135" 3 (let ()
      (define-values (x y) (values 1 2))
      (+ x y)))
(test-equal "r7rs-136" 6 (let ()
      (define-values (x y z) (values 1 2 3))
      (+ x y z)))
(test-equal "r7rs-137" 10 (let ()
      (define-values (x y . z) (values 1 2 3 4))
      (+ x y (car z) (cadr z))))

(test-equal "r7rs-138" '(2 1) (let ((x 1) (y 2))
  (define-syntax swap!
    (syntax-rules ()
      ((swap! a b)
       (let ((tmp a))
         (set! a b)
         (set! b tmp)))))
  (swap! x y)
  (list x y)))

;; Records

(define-record-type <pare>
  (kons x y)
  pare?
  (x kar set-kar!)
  (y kdr))

(test-equal "r7rs-139" #t (pare? (kons 1 2)))
(test-equal "r7rs-140" #f (pare? (cons 1 2)))
(test-equal "r7rs-141" 1 (kar (kons 1 2)))
(test-equal "r7rs-142" 2 (kdr (kons 1 2)))
(test-equal "r7rs-143" 3 (let ((k (kons 1 2)))
          (set-kar! k 3)
          (kar k)))

(test-end)

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 6 Standard Procedures

(test-begin "6.1 Equivalence Predicates")

(test-equal "r7rs-144" #t (eqv? 'a 'a))
(test-equal "r7rs-145" #f (eqv? 'a 'b))
(test-equal "r7rs-146" #t (eqv? 2 2))
(test-equal "r7rs-147" #t (eqv? '() '()))
(test-equal "r7rs-148" #t (eqv? 100000000 100000000))
(test-equal "r7rs-149" #f (eqv? (cons 1 2) (cons 1 2)))
(test-equal "r7rs-150" #f (eqv? (lambda () 1)
               (lambda () 2)))
(test-equal "r7rs-151" #f (eqv? #f 'nil))

(define gen-counter
  (lambda ()
    (let ((n 0))
      (lambda () (set! n (+ n 1)) n))))
(test-equal "r7rs-152" #t (let ((g (gen-counter)))
      (eqv? g g)))
(test-equal "r7rs-153" #f (eqv? (gen-counter) (gen-counter)))
(define gen-loser
  (lambda ()
    (let ((n 0))
      (lambda () (set! n (+ n 1)) 27))))
(test-equal "r7rs-154" #t (let ((g (gen-loser)))
  (eqv? g g)))

(test-equal "r7rs-155" #f (letrec ((f (lambda () (if (eqv? f g) 'f 'both)))
         (g (lambda () (if (eqv? f g) 'g 'both))))
   (eqv? f g)))

(test-equal "r7rs-156" #t (let ((x '(a)))
      (eqv? x x)))

(test-equal "r7rs-157" #t (eq? 'a 'a))
(test-equal "r7rs-158" #f (eq? (list 'a) (list 'a)))
(test-equal "r7rs-159" #t (eq? '() '()))
(test-equal "r7rs-160" #t (let ((x '(a)))
      (eq? x x)))
(test-equal "r7rs-161" #t (let ((x '#()))
      (eq? x x)))
(test-equal "r7rs-162" #t (let ((p (lambda (x) x)))
      (eq? p p)))

(test-equal "r7rs-163" #t (equal? 'a 'a))
(test-equal "r7rs-164" #t (equal? '(a) '(a)))
(test-equal "r7rs-165" #t (equal? '(a (b) c)
                 '(a (b) c)))
(test-equal "r7rs-166" #t (equal? "abc" "abc"))
(test-equal "r7rs-167" #t (equal? 2 2))
(test-equal "r7rs-168" #t (equal? (make-vector 5 'a)
                 (make-vector 5 'a)))

(test-end)

(test-begin "6.2 Numbers")

(test-equal "r7rs-169" #t (complex? 3+4i))
(test-equal "r7rs-170" #t (complex? 3))
(test-equal "r7rs-171" #t (real? 3))
(test-equal "r7rs-172" #t (real? -2.5+0i))
(test-equal "r7rs-173" #f (real? -2.5+0.0i))
(test-equal "r7rs-174" #t (real? #e1e10))
(test-equal "r7rs-175" #t (real? +inf.0))
(test-equal "r7rs-176" #f (rational? -inf.0))
(test-equal "r7rs-177" #f (rational? +nan.0))
(test-equal "r7rs-178" #t (rational? 9007199254740991.0))
(test-equal "r7rs-179" #t (rational? 9007199254740992.0))
(test-equal "r7rs-180" #t (rational? 1.7976931348623157e308))
(test-equal "r7rs-181" #t (rational? 6/10))
(test-equal "r7rs-182" #t (rational? 6/3))
(test-equal "r7rs-183" #t (integer? 3+0i))
(test-equal "r7rs-184" #t (integer? 3.0))
(test-equal "r7rs-185" #t (integer? 8/4))

(test-equal "r7rs-186" #f (exact? 3.0))
(test-equal "r7rs-187" #t (exact? #e3.0))
(test-equal "r7rs-188" #t (inexact? 3.))

(test-equal "r7rs-189" #t (exact-integer? 32))
(test-equal "r7rs-190" #f (exact-integer? 32.0))
(test-equal "r7rs-191" #f (exact-integer? 32/5))

(test-equal "r7rs-192" #t (finite? 3))
(test-equal "r7rs-193" #f (finite? +inf.0))
(test-equal "r7rs-194" #f (finite? 3.0+inf.0i))

(test-equal "r7rs-195" #f (infinite? 3))
(test-equal "r7rs-196" #t (infinite? +inf.0))
(test-equal "r7rs-197" #f (infinite? +nan.0))
(test-equal "r7rs-198" #t (infinite? 3.0+inf.0i))

(test-equal "r7rs-199" #t (nan? +nan.0))
(test-equal "r7rs-200" #f (nan? 32))
;; (test-equal "r7rs-201" #t (nan? +nan.0+5.0i))
(test-equal "r7rs-202" #f (nan? 1+2i))

(test-equal "r7rs-203" #t (= 1 1.0 1.0+0.0i))
(test-equal "r7rs-204" #f (= 1.0 1.0+1.0i))
(test-equal "r7rs-205" #t (< 1 2 3))
(test-equal "r7rs-206" #f (< 1 1 2))
(test-equal "r7rs-207" #t (> 3.0 2.0 1.0))
(test-equal "r7rs-208" #f (> -3.0 2.0 1.0))
(test-equal "r7rs-209" #t (<= 1 1 2))
(test-equal "r7rs-210" #f (<= 1 2 1))
(test-equal "r7rs-211" #t (>= 2 1 1))
(test-equal "r7rs-212" #f (>= 1 2 1))
(test-equal "r7rs-213" #f (< +nan.0 0))
(test-equal "r7rs-214" #f (> +nan.0 0))
(test-equal "r7rs-215" #f (< +nan.0 0.0))
(test-equal "r7rs-216" #f (> +nan.0 0.0))
(test-equal "r7rs-217" '(#t #f) (list (<= 1 1 2) (<= 2 1 3)))
(test-equal "r7rs-218" #f (= 9007199254740992.0 9007199254740993))

;; From R7RS 6.2.6 Numerical operations:
;;
;; These predicates are required to be transitive.
;;
;; _Note:_ The traditional implementations of these predicates in
;; Lisp-like languages, which involve converting all arguments to inexact
;; numbers if any argument is inexact, are not transitive.

;; Example from Alan Bawden
(let ((a (- (expt 2 1000) 1))
      (b (inexact (expt 2 1000))) ; assuming > single-float-epsilon
      (c (+ (expt 2 1000) 1)))
  (test-equal "r7rs-219" #t (if (and (= a b) (= b c))
               (= a c)
               #t)))

;; From CLtL 12.3. Comparisons on Numbers:
;;
;;  Let _a_ be the result of (/ 10.0 single-float-epsilon), and let
;;  _j_ be the result of (floor a). ..., all of (<= a j), (< j (+ j
;;  1)), and (<= (+ j 1) a) would be true; transitivity would then
;;  imply that (< a a) ought to be true ...

;; Transliteration from Jussi Piitulainen
(define single-float-epsilon
  (do ((eps 1.0 (* eps 2.0)))
      ((= eps (+ eps 1.0)) eps)))

(let* ((a (/ 10.0 single-float-epsilon))
       (j (exact a)))
  (test-equal "r7rs-220" #t (if (and (<= a j) (< j (+ j 1)))
               (not (<= (+ j 1) a))
               #t)))

(test-equal "r7rs-221" #t (zero? 0))
(test-equal "r7rs-222" #t (zero? 0.0))
(test-equal "r7rs-223" #t (zero? 0.0+0.0i))
(test-equal "r7rs-224" #f (zero? 1))
(test-equal "r7rs-225" #f (zero? -1))

(test-equal "r7rs-226" #f (positive? 0))
(test-equal "r7rs-227" #f (positive? 0.0))
(test-equal "r7rs-228" #t (positive? 1))
(test-equal "r7rs-229" #t (positive? 1.0))
(test-equal "r7rs-230" #f (positive? -1))
(test-equal "r7rs-231" #f (positive? -1.0))
(test-equal "r7rs-232" #t (positive? +inf.0))
(test-equal "r7rs-233" #f (positive? -inf.0))
(test-equal "r7rs-234" #f (positive? +nan.0))

(test-equal "r7rs-235" #f (negative? 0))
(test-equal "r7rs-236" #f (negative? 0.0))
(test-equal "r7rs-237" #f (negative? 1))
(test-equal "r7rs-238" #f (negative? 1.0))
(test-equal "r7rs-239" #t (negative? -1))
(test-equal "r7rs-240" #t (negative? -1.0))
(test-equal "r7rs-241" #f (negative? +inf.0))
(test-equal "r7rs-242" #t (negative? -inf.0))
(test-equal "r7rs-243" #f (negative? +nan.0))

(test-equal "r7rs-244" #f (odd? 0))
(test-equal "r7rs-245" #t (odd? 1))
(test-equal "r7rs-246" #t (odd? -1))
(test-equal "r7rs-247" #f (odd? 102))

(test-equal "r7rs-248" #t (even? 0))
(test-equal "r7rs-249" #f (even? 1))
(test-equal "r7rs-250" #t (even? -2))
(test-equal "r7rs-251" #t (even? 102))

(test-equal "r7rs-252" 3 (max 3))
(test-equal "r7rs-253" 4 (max 3 4))
(test-equal "r7rs-254" 4.0 (max 3.9 4))
(test-equal "r7rs-255" 5.0 (max 5 3.9 4))
(test-equal "r7rs-256" +inf.0 (max 100 +inf.0))
(test-equal "r7rs-257" 3 (min 3))
(test-equal "r7rs-258" 3 (min 3 4))
(test-equal "r7rs-259" 3.0 (min 3 3.1))
(test-equal "r7rs-260" -inf.0 (min -inf.0 -100))

(test-equal "r7rs-261" 7 (+ 3 4))
(test-equal "r7rs-262" 3 (+ 3))
(test-equal "r7rs-263" 0 (+))
(test-equal "r7rs-264" 4 (* 4))
(test-equal "r7rs-265" 1 (*))

(test-equal "r7rs-266" -1 (- 3 4))
(test-equal "r7rs-267" -6 (- 3 4 5))
(test-equal "r7rs-268" -3 (- 3))
(test-equal "r7rs-269" -3/2 (- 3/2))
;; r7rs-270 uses complex rational literal -3/2-i which grift parses as two tokens
;; (test-equal "r7rs-270" -3/2-i (- 3/2+i))
(test-equal "r7rs-271" 3/20 (/ 3 4 5))
(test-equal "r7rs-272" 1/3 (/ 3))

(test-equal "r7rs-273" 1073741824 (/ -1073741824 -1))
(test-equal "r7rs-274" 1073741824 (quotient -1073741824 -1))
(test-equal "r7rs-275" 0 (remainder -1073741824 -1))
(test-equal "r7rs-276" 4611686018427387904 (/ -4611686018427387904 -1))
(test-equal "r7rs-277" 4611686018427387904 (quotient -4611686018427387904 -1))
(test-equal "r7rs-278" 0 (remainder -4611686018427387904 -1))

(test-equal "r7rs-279" 7 (abs -7))
(test-equal "r7rs-280" 7 (abs 7))

(test-values (values 2 1) (floor/ 5 2))
(test-values (values -3 1) (floor/ -5 2))
(test-values (values -3 -1) (floor/ 5 -2))
(test-values (values 2 -1) (floor/ -5 -2))
(test-values (values 2 1) (truncate/ 5 2))
(test-values (values -2 -1) (truncate/ -5 2))
(test-values (values -2 1) (truncate/ 5 -2))
(test-values (values 2 -1) (truncate/ -5 -2))
(test-values (values 2.0 -1.0) (truncate/ -5.0 -2))

(test-equal "r7rs-281" 1 (modulo 13 4))
(test-equal "r7rs-282" 1 (remainder 13 4))

(test-equal "r7rs-283" 3 (modulo -13 4))
(test-equal "r7rs-284" -1 (remainder -13 4))

(test-equal "r7rs-285" -3 (modulo 13 -4))
(test-equal "r7rs-286" 1 (remainder 13 -4))

(test-equal "r7rs-287" -1 (modulo -13 -4))
(test-equal "r7rs-288" -1 (remainder -13 -4))

(test-equal "r7rs-289" -1.0 (remainder -13 -4.0))

(test-equal "r7rs-290" 4 (gcd 32 -36))
(test-equal "r7rs-291" 0 (gcd))
(test-equal "r7rs-292" 288 (lcm 32 -36))
(test-equal "r7rs-293" 288.0 (lcm 32.0 -36))
(test-equal "r7rs-294" 1 (lcm))

(test-equal "r7rs-295" 3 (numerator (/ 6 4)))
(test-equal "r7rs-296" 2 (denominator (/ 6 4)))
(test-equal "r7rs-297" 2.0 (denominator (inexact (/ 6 4))))
(test-equal "r7rs-298" 11.0 (numerator 5.5))
(test-equal "r7rs-299" 2.0 (denominator 5.5))
(test-equal "r7rs-300" 5.0 (numerator 5.0))
(test-equal "r7rs-301" 1.0 (denominator 5.0))

(test-equal "r7rs-302" -5.0 (floor -4.3))
(test-equal "r7rs-303" -4.0 (ceiling -4.3))
(test-equal "r7rs-304" -4.0 (truncate -4.3))
(test-equal "r7rs-305" -4.0 (round -4.3))

(test-equal "r7rs-306" 3.0 (floor 3.5))
(test-equal "r7rs-307" 4.0 (ceiling 3.5))
(test-equal "r7rs-308" 3.0 (truncate 3.5))
(test-equal "r7rs-309" 4.0 (round 3.5))

(test-equal "r7rs-310" 4 (round 7/2))
(test-equal "r7rs-311" 7 (round 7))
(test-equal "r7rs-312" 1 (round 7/10))
(test-equal "r7rs-313" -4 (round -7/2))
(test-equal "r7rs-314" -7 (round -7))
(test-equal "r7rs-315" -1 (round -7/10))

(test-equal "r7rs-316" 1/3 (rationalize (exact .3) 1/10))
;; r7rs-317 uses #i1/3 literal which grift parses as two tokens (1.0 /3)
;; (test-equal "r7rs-317" #i1/3 (rationalize .3 1/10))

(test-equal "r7rs-318" 1.0 (inexact (exp 0))) ;; may return exact number
(test-equal "r7rs-319" 20.0855369231877 (exp 3))

(test-equal "r7rs-320" 0.0 (inexact (log 1))) ;; may return exact number
(test-equal "r7rs-321" 1.0 (log (exp 1)))
(test-equal "r7rs-322" 42.0 (log (exp 42)))
(test-equal "r7rs-323" 2.0 (log 100 10))
(test-equal "r7rs-324" 12.0 (log 4096 2))

(test-equal "r7rs-325" 0.0 (inexact (sin 0))) ;; may return exact number
(test-equal "r7rs-326" 1.0 (sin 1.5707963267949))
(test-equal "r7rs-327" 1.0 (inexact (cos 0))) ;; may return exact number
(test-equal "r7rs-328" -1.0 (cos 3.14159265358979))
(test-equal "r7rs-329" 0.0 (inexact (tan 0))) ;; may return exact number
(test-equal "r7rs-330" 1.5574077246549 (tan 1))

(test-equal "r7rs-331" 0.0 (inexact (asin 0))) ;; may return exact number
(test-equal "r7rs-332" 1.5707963267949 (asin 1))
(test-equal "r7rs-333" 0.0 (inexact (acos 1))) ;; may return exact number
(test-equal "r7rs-334" 3.14159265358979 (acos -1))

;; (test-equal "r7rs-335" 0.0-0.0i (asin 0+0.0i))
;; (test-equal "r7rs-336" 1.5707963267948966+0.0i (acos 0+0.0i))

(test-equal "r7rs-337" 0.0 (atan 0.0 1.0))
(test-equal "r7rs-338" -0.0 (atan -0.0 1.0))
(test-equal "r7rs-339" 0.785398163397448 (atan 1.0 1.0))
(test-equal "r7rs-340" 1.5707963267949 (atan 1.0 0.0))
(test-equal "r7rs-341" 2.35619449019234 (atan 1.0 -1.0))
(test-equal "r7rs-342" 3.14159265358979 (atan 0.0 -1.0))
(test-equal "r7rs-343" -3.14159265358979 (atan -0.0 -1.0)) ;
(test-equal "r7rs-344" -2.35619449019234 (atan -1.0 -1.0))
(test-equal "r7rs-345" -1.5707963267949 (atan -1.0 0.0))
(test-equal "r7rs-346" -0.785398163397448 (atan -1.0 1.0))
;; (test-equal "r7rs-347" undefined (atan 0.0 0.0))

(test-equal "r7rs-348" 1764 (square 42))
(test-equal "r7rs-349" 4 (square 2))

(test-equal "r7rs-350" 3.0 (inexact (sqrt 9)))
(test-equal "r7rs-351" 1.4142135623731 (sqrt 2))
(test-equal "r7rs-352" 0.0+1.0i (inexact (sqrt -1)))
(test-equal "r7rs-353" 0.0+1.0i (sqrt -1.0-0.0i))

(test-equal "r7rs-354" '(2 0) (call-with-values (lambda () (exact-integer-sqrt 4)) list))
(test-equal "r7rs-355" '(2 1) (call-with-values (lambda () (exact-integer-sqrt 5)) list))

(test-equal "r7rs-356" 27 (expt 3 3))
(test-equal "r7rs-357" 1 (expt 0 0))
(test-equal "r7rs-358" 0 (expt 0 1))
(test-equal "r7rs-359" 1.0 (expt 0.0 0))
(test-equal "r7rs-360" 0.0 (expt 0 1.0))

(test-equal "r7rs-361" 1+2i (make-rectangular 1 2))

(test-equal "r7rs-362" 0.54030230586814+0.841470984807897i (make-polar 1 1))

(test-equal "r7rs-363" 1 (real-part 1+2i))

(test-equal "r7rs-364" 2 (imag-part 1+2i))

(test-equal "r7rs-365" 2.23606797749979 (magnitude 1+2i))

(test-equal "r7rs-366" 1.10714871779409 (angle 1+2i))

(test-equal "r7rs-367" 1.0 (inexact 1))
(test-equal "r7rs-368" #t (inexact? (inexact 1)))
(test-equal "r7rs-369" 1 (exact 1.0))
(test-equal "r7rs-370" #t (exact? (exact 1.0)))

(test-equal "r7rs-371" 100 (string->number "100"))
(test-equal "r7rs-372" 256 (string->number "100" 16))
(test-equal "r7rs-373" 100.0 (string->number "1e2"))
(test-equal "r7rs-374" #f (string->number "1 2"))

(test-end)

(test-begin "6.3 Booleans")

(test-equal "r7rs-375" #t #t)
(test-equal "r7rs-376" #f #f)
(test-equal "r7rs-377" #f '#f)

(test-equal "r7rs-378" #f (not #t))
(test-equal "r7rs-379" #f (not 3))
(test-equal "r7rs-380" #f (not (list 3)))
(test-equal "r7rs-381" #t (not #f))
(test-equal "r7rs-382" #f (not '()))
(test-equal "r7rs-383" #f (not (list)))
(test-equal "r7rs-384" #f (not 'nil))

(test-equal "r7rs-385" #t (boolean? #f))
(test-equal "r7rs-386" #f (boolean? 0))
(test-equal "r7rs-387" #f (boolean? '()))

(test-equal "r7rs-388" #t (boolean=? #t #t))
(test-equal "r7rs-389" #t (boolean=? #f #f))
(test-equal "r7rs-390" #f (boolean=? #t #f))
(test-equal "r7rs-391" #t (boolean=? #f #f #f))
(test-equal "r7rs-392" #f (boolean=? #t #t #f))

(test-end)

(test-begin "6.4 Lists")

(let* ((x (list 'a 'b 'c))
       (y x))
  (test-equal "r7rs-393" '(a b c) (values y))
  (test-equal "r7rs-394" #t (list? y))
  (set-cdr! x 4)
  (test-equal "r7rs-395" '(a . 4) (values x))
  (test-equal "r7rs-396" #t (eqv? x y))
  (test-equal "r7rs-397" #f (list? y))
  (set-cdr! x x)
  (test-equal "r7rs-398" #f (list? x)))

(test-equal "r7rs-399" #t (pair? '(a . b)))
(test-equal "r7rs-400" #t (pair? '(a b c)))
(test-equal "r7rs-401" #f (pair? '()))
(test-equal "r7rs-402" #f (pair? '#(a b)))

(test-equal "r7rs-403" '(a) (cons 'a '()))
(test-equal "r7rs-404" '((a) b c d) (cons '(a) '(b c d)))
(test-equal "r7rs-405" '("a" b c) (cons "a" '(b c)))
(test-equal "r7rs-406" '(a . 3) (cons 'a 3))
(test-equal "r7rs-407" '((a b) . c) (cons '(a b) 'c))

(test-equal "r7rs-408" 'a (car '(a b c)))
(test-equal "r7rs-409" '(a) (car '((a) b c d)))
(test-equal "r7rs-410" 1 (car '(1 . 2)))

(test-equal "r7rs-411" '(b c d) (cdr '((a) b c d)))
(test-equal "r7rs-412" 2 (cdr '(1 . 2)))
(define (g) '(constant-list))

(test-equal "r7rs-413" #t (list? '(a b c)))
(test-equal "r7rs-414" #t (list? '()))
(test-equal "r7rs-415" #f (list? '(a . b)))
(test-equal "r7rs-416" #f (let ((x (list 'a))) (set-cdr! x x) (list? x)))

(test-equal "r7rs-417" '(3 3) (make-list 2 3))

(test-equal "r7rs-418" '(a 7 c) (list 'a (+ 3 4) 'c))
(test-equal "r7rs-419" '() (list))

(test-equal "r7rs-420" 3 (length '(a b c)))
(test-equal "r7rs-421" 3 (length '(a (b) (c d e))))
(test-equal "r7rs-422" 0 (length '()))

(test-equal "r7rs-423" '(x y) (append '(x) '(y)))
(test-equal "r7rs-424" '(a b c d) (append '(a) '(b c d)))
(test-equal "r7rs-425" '(a (b) (c)) (append '(a (b)) '((c))))

(test-equal "r7rs-426" '(a b c . d) (append '(a b) '(c . d)))
(test-equal "r7rs-427" 'a (append '() 'a))

(test-equal "r7rs-428" '(c b a) (reverse '(a b c)))
(test-equal "r7rs-429" '((e (f)) d (b c) a) (reverse '(a (b c) d (e (f)))))

(test-equal "r7rs-430" '(d e) (list-tail '(a b c d e) 3))

(test-equal "r7rs-431" 'c (list-ref '(a b c d) 2))
(test-equal "r7rs-432" 'c (list-ref '(a b c d)
          (exact (round 1.8))))

(test-equal "r7rs-433" '(0 ("Sue" "Sue") "Anna") (let ((lst (list 0 '(2 2 2 2) "Anna")))
      (list-set! lst 1 '("Sue" "Sue"))
      lst))

(test-equal "r7rs-434" '(a b c) (memq 'a '(a b c)))
(test-equal "r7rs-435" '(b c) (memq 'b '(a b c)))
(test-equal "r7rs-436" #f (memq 'a '(b c d)))
(test-equal "r7rs-437" #f (memq (list 'a) '(b (a) c)))
(test-equal "r7rs-438" '((a) c) (member (list 'a) '(b (a) c)))
(test-equal "r7rs-439" '("b" "c") (member "B" '("a" "b" "c") string-ci=?))
(test-equal "r7rs-440" '(101 102) (memv 101 '(100 101 102)))

(let ()
  (define e '((a 1) (b 2) (c 3)))
  (test-equal "r7rs-441" '(a 1) (assq 'a e))
  (test-equal "r7rs-442" '(b 2) (assq 'b e))
  (test-equal "r7rs-443" #f (assq 'd e)))

(test-equal "r7rs-444" #f (assq (list 'a) '(((a)) ((b)) ((c)))))
(test-equal "r7rs-445" '((a)) (assoc (list 'a) '(((a)) ((b)) ((c)))))
(test-equal "r7rs-446" '(2 4) (assoc 2.0 '((1 1) (2 4) (3 9)) =))
(test-equal "r7rs-447" '(5 7) (assv 5 '((2 3) (5 7) (11 13))))

(test-equal "r7rs-448" '(1 2 3) (list-copy '(1 2 3)))
(test-equal "r7rs-449" "foo" (list-copy "foo"))
(test-equal "r7rs-450" '() (list-copy '()))
(test-equal "r7rs-451" '(3 . 4) (list-copy '(3 . 4)))
(test-end)
(test-end)