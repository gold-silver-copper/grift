;;; NQUEENS -- Compute number of solutions to the n-queens problem.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-nqueens")

(define (nqueens n)

  (define (iota1 n)
    (let loop ((i n) (l '()))
      (if (= i 0) l (loop (- i 1) (cons i l)))))

  (define (my-try x y z)
    (if (null? x)
        (if (null? y)
            1
            0)
        (+ (if (ok? (car x) 1 z)
               (my-try (append (cdr x) y) '() (cons (car x) z))
               0)
           (my-try (cdr x) (cons (car x) y) z))))

  (define (ok? row dist placed)
    (if (null? placed)
        #t
        (and (not (= (car placed) (+ row dist)))
             (not (= (car placed) (- row dist)))
             (ok? row (+ dist 1) (cdr placed)))))

  (my-try (iota1 n) '() '()))

(test-equal "nqueens-1" 1 (nqueens 1))
(test-equal "nqueens-4" 2 (nqueens 4))
(test-equal "nqueens-5" 10 (nqueens 5))
(test-equal "nqueens-6" 4 (nqueens 6))
(test-equal "nqueens-7" 40 (nqueens 7))
(test-equal "nqueens-8" 92 (nqueens 8))

(test-end)
