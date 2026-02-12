;;; DERIV -- Symbolic derivation.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-deriv")

(define (deriv a)
  (cond ((not (pair? a))
         (if (eq? a 'x) 1 0))
        ((eq? (car a) '+)
         (cons '+
               (map deriv (cdr a))))
        ((eq? (car a) '-)
         (cons '-
               (map deriv (cdr a))))
        ((eq? (car a) '*)
         (list '*
               a
               (cons '+
                     (map (lambda (a) (list '/ (deriv a) a)) (cdr a)))))
        ((eq? (car a) '/)
         (list '-
               (list '/
                     (deriv (cadr a))
                     (caddr a))
               (list '/
                     (cadr a)
                     (list '*
                           (caddr a)
                           (caddr a)
                           (deriv (caddr a))))))
        (else
         (error "No derivation method available"))))

(test-equal "deriv-x" 1 (deriv 'x))
(test-equal "deriv-y" 0 (deriv 'y))
(test-equal "deriv-const" 0 (deriv 42))

(test-equal "deriv-sum"
  '(+ 1 0)
  (deriv '(+ x y)))

(test-equal "deriv-product"
  '(* (* x y) (+ (/ 1 x) (/ 0 y)))
  (deriv '(* x y)))

(test-equal "deriv-nested"
  '(+ 1 1)
  (deriv '(+ x x)))

(test-end)
