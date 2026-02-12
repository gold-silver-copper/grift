;;; SUM -- Compute sum of integers from 0 to n.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-sum")

(define (sum-iter n)
  (let loop ((i n) (sum 0))
    (if (< i 0)
        sum
        (loop (- i 1) (+ i sum)))))

(test-equal "sum-0" 0 (sum-iter 0))
(test-equal "sum-10" 55 (sum-iter 10))
(test-equal "sum-100" 5050 (sum-iter 100))
(test-equal "sum-1000" 500500 (sum-iter 1000))
(test-equal "sum-10000" 50005000 (sum-iter 10000))

(test-end)
