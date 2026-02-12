;;; FIBFP -- Computes fib(n) using floating point.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-fibfp")

(define (fibfp n)
  (if (< n 2.0)
      n
      (+ (fibfp (- n 1.0))
         (fibfp (- n 2.0)))))

(test-equal "fibfp-0" 0.0 (fibfp 0.0))
(test-equal "fibfp-1" 1.0 (fibfp 1.0))
(test-equal "fibfp-10" 55.0 (fibfp 10.0))
(test-equal "fibfp-15" 610.0 (fibfp 15.0))
(test-equal "fibfp-20" 6765.0 (fibfp 20.0))

(test-end)
