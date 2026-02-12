;;; SUMFP -- Compute sum of integers from 0 to n using floating point.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-sumfp")

(define (sumfp n)
  (let loop ((i n) (sum 0.0))
    (if (< i 0.0)
        sum
        (loop (- i 1.0) (+ i sum)))))

(test-equal "sumfp-0" 0.0 (sumfp 0.0))
(test-equal "sumfp-10" 55.0 (sumfp 10.0))
(test-equal "sumfp-100" 5050.0 (sumfp 100.0))
(test-equal "sumfp-1000" 500500.0 (sumfp 1000.0))
(test-equal "sumfp-10000" 50005000.0 (sumfp 10000.0))

(test-end)
