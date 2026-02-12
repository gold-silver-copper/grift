(test-begin "continuation-delay")

;; ============================================================
;; Continuations
;; ============================================================

;; member using call/cc for nonlocal exit from a do loop

(define member-cc
  (lambda (x ls)
    (call/cc
      (lambda (break)
        (do ((ls ls (cdr ls)))
            ((null? ls) #f)
            (if (equal? x (car ls))
                (break ls)))))))

(test-assert "member-callcc-not-found" (not (member-cc 'd '(a b c))))

(test-equal "member-callcc-found" '(b c) (member-cc 'b '(a b c)))

;; dynamic-wind basic: in/body/out thunks called in order

(define log '())

(test-equal "dynamic-wind-basic-returns-42" 42
  (dynamic-wind
    (lambda () (set! log (cons 'in log)))
    (lambda () (set! log (cons 'body log)) 42)
    (lambda () (set! log (cons 'out log)))))

(test-equal "dynamic-wind-basic-log-order" '(out body in) log)

;; unwind-protect macro: cleanup runs even on continuation escape

(define-syntax unwind-protect
  (syntax-rules ()
    ((_ body cleanup ...)
     (dynamic-wind
       (lambda () #f)
       (lambda () body)
       (lambda () cleanup ...)))))

(test-equal "unwind-protect-cleanup-on-escape" 'b
  ((call/cc
     (let ((x 'a))
       (lambda (k)
         (unwind-protect
           (k (lambda () x))
           (set! x 'b)))))))

;; fluid-let basic: temporary binding restored after exit

(define-syntax fluid-let
  (syntax-rules ()
    ((_ ((x v)) e1 e2 ...)
     (let ((y v))
       (let ((swap (lambda ()
                     (let ((t x))
                       (set! x y)
                       (set! y t)))))
         (dynamic-wind
           swap
           (lambda () e1 e2 ...)
           swap))))))

(test-equal "fluid-let-basic" 8
  (let ((x 3))
    (+ (fluid-let ((x 5))
         x)
       x)))

;; fluid-let with call/cc: variable reverts on continuation escape

(test-equal "fluid-let-callcc-revert" '(b . a)
  (let ((x 'a))
    (let ((f (lambda () x)))
      (cons (call/cc
              (lambda (k)
                (fluid-let ((x 'b))
                  (f))))
            (f)))))

;; fluid-let with reenter: continuation re-invocation reinstates temporary value

(define reenter #f)
(define x 0)

(test-equal "fluid-let-reenter-first" 2
  (fluid-let ((x 1))
    (call/cc (lambda (k) (set! reenter k)))
    (set! x (+ x 1))
    x))

(test-equal "fluid-let-reenter-x-after-exit" 0 x)

(test-equal "fluid-let-reenter-second" 3 (reenter '*))

(test-equal "fluid-let-reenter-third" 4 (reenter '*))

(test-equal "fluid-let-reenter-x-still-zero" 0 x)

;; ============================================================
;; Delayed Evaluation
;; ============================================================

;; Basic delay and force

(test-equal "delay-force-basic" 3 (force (delay (+ 1 2))))

;; delay memoizes: expression evaluated only once

(define count 0)
(define p (delay (begin (set! count (+ count 1)) count)))

(test-equal "delay-memoization-first-force" 1 (force p))
(test-equal "delay-memoization-second-force" 1 (force p))
(test-equal "delay-memoization-count" 1 count)

;; Stream abstraction: stream-car, stream-cdr, counters

(define stream-car
  (lambda (s)
    (car (force s))))

(define stream-cdr
  (lambda (s)
    (cdr (force s))))

(define counters
  (let next ((n 1))
    (delay (cons n (next (+ n 1))))))

(test-equal "stream-car-counters" 1 (stream-car counters))

(test-equal "stream-car-cdr-counters" 2 (stream-car (stream-cdr counters)))

;; stream-add and even-counters

(define stream-add
  (lambda (s1 s2)
    (delay (cons
             (+ (stream-car s1) (stream-car s2))
             (stream-add (stream-cdr s1) (stream-cdr s2))))))

(define even-counters (stream-add counters counters))

(test-equal "stream-car-even-counters" 2 (stream-car even-counters))

(test-equal "stream-car-cdr-even-counters" 4
  (stream-car (stream-cdr even-counters)))

;; make-promise and force definitions from the spec

(define make-promise
  (lambda (p)
    (let ((val #f) (set? #f))
      (lambda ()
        (if (not set?)
            (let ((x (p)))
              (if (not set?)
                  (begin (set! val x)
                         (set! set? #t)))))
        val))))

(define my-force
  (lambda (promise)
    (promise)))

(define my-delay-val (make-promise (lambda () (+ 10 20))))

(test-equal "make-promise-force-first" 30 (my-force my-delay-val))
(test-equal "make-promise-force-memoized" 30 (my-force my-delay-val))

(test-end)
