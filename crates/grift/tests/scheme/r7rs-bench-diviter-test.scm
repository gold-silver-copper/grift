;;; DIVITER -- Benchmark which divides by 2 using lists of n ()'s.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-diviter")

(define (create-n n)
  (do ((n n (- n 1))
       (a '() (cons '() a)))
      ((= n 0) a)))

(define (iterative-div2 l)
  (do ((l l (cddr l))
       (a '() (cons (car l) a)))
      ((null? l) a)))

(test-equal "diviter-0" 0 (length (iterative-div2 (create-n 0))))
(test-equal "diviter-10" 5 (length (iterative-div2 (create-n 10))))
(test-equal "diviter-100" 50 (length (iterative-div2 (create-n 100))))
(test-equal "diviter-1000" 500 (length (iterative-div2 (create-n 1000))))

(test-end)
