(test-begin "issues")

;; Issue 1: or hygiene - rebound `if` should not affect or's template
(test-equal "or-hygiene-rebound-if"
  'okay
  (let ((if #f))
    (let ((t 'okay))
      (or if t))))

;; Issue 2: dolet hygiene - template variable `a` distinct from user `a`
(test-equal "dolet-hygiene-returns-7"
  7
  (let-syntax ((dolet (lambda (x)
                        (syntax-case x ()
                          ((_ b)
                           (syntax (let ((a 3) (b 4))
                                     (+ a b))))))))
    (dolet a)))

;; Issue 3: identifier-syntax basic
(define-syntax my-val
  (identifier-syntax 42))

(test-equal "identifier-syntax-basic"
  42
  my-val)

;; Issue 4: named let via syntax-rules
(define-syntax rec
  (syntax-rules ()
    ((_ x e) (letrec ((x e)) x))))

(define-syntax my-let2
  (syntax-rules ()
    ((_ ((x v) ...) e1 e2 ...)
     ((lambda (x ...) e1 e2 ...) v ...))
    ((_ f ((x v) ...) e1 e2 ...)
     ((rec f (lambda (x ...) e1 e2 ...)) v ...))))

(test-equal "named-let-regular-let"
  3
  (my-let2 ((a 1) (b 2)) (+ a b)))

(test-equal "named-let-loop"
  10
  (my-let2 loop ((i 0) (sum 0))
    (if (= i 5) sum (loop (+ i 1) (+ sum i)))))

;; Issue 5: cond with lexically bound else should not match
(test-equal "cond-bound-else-not-matched"
  #f
  (let ((else #f))
    (cond (else 42))))

;; Issue 6: identifier-syntax with set! and variable mutation
;; Known limitation: set! does not work with syntax objects produced by
;; identifier-syntax expansion. In conforming R7RS implementations
;; (e.g. Chez, Racket) this returns '(0 1).
(test-error "identifier-syntax-with-set"
  (lambda ()
    (let ((x 0))
      (define-syntax x++
        (identifier-syntax
          (let ((t x)) (set! x (+ t 1)) t)))
      (let ((a x++))
        (list a x)))))

;; Issue 7: redefined cond with free-identifier=?
(define-syntax my-cond
  (lambda (x)
    (syntax-case x ()
      ((_ (e0 e1 e2 ...))
       (and (identifier? (syntax e0))
            (free-identifier=? (syntax e0) (syntax else)))
       (syntax (begin e1 e2 ...)))
      ((_ (e0 e1 e2 ...)) (syntax (if e0 (begin e1 e2 ...))))
      ((_ (e0 e1 e2 ...) c1 c2 ...)
       (syntax (if e0 (begin e1 e2 ...) (my-cond c1 c2 ...)))))))

(test-equal "redefined-cond-else-keyword"
  42
  (my-cond (else 42)))

(test-equal "redefined-cond-bound-else-not-matched"
  #f
  (let ((else #f))
    (my-cond (else 42))))

(test-end)
