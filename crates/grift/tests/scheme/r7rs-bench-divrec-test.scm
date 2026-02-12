;;; DIVREC -- Benchmark which divides by 2 using lists of n ()'s.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-divrec")

(define (create-n n)
  (do ((n n (- n 1))
       (a '() (cons '() a)))
      ((= n 0) a)))

(define (recursive-div2 l)
  (cond ((null? l) '())
        (else (cons (car l) (recursive-div2 (cddr l))))))

(test-equal "divrec-0" 0 (length (recursive-div2 (create-n 0))))
(test-equal "divrec-10" 5 (length (recursive-div2 (create-n 10))))
(test-equal "divrec-100" 50 (length (recursive-div2 (create-n 100))))
(test-equal "divrec-1000" 500 (length (recursive-div2 (create-n 1000))))

(test-end)
