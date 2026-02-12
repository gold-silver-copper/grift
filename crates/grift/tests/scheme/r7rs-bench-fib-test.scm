;;; FIB -- A classic benchmark, computes fib(n) inefficiently.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-fib")

(define (fib n)
  (if (< n 2)
      n
      (+ (fib (- n 1))
         (fib (- n 2)))))

(test-equal "fib-0" 0 (fib 0))
(test-equal "fib-1" 1 (fib 1))
(test-equal "fib-5" 5 (fib 5))
(test-equal "fib-10" 55 (fib 10))
(test-equal "fib-15" 610 (fib 15))
(test-equal "fib-20" 6765 (fib 20))

(test-end)
