;;; CPSTAK -- A continuation-passing version of the TAK benchmark.
;;; A good test of first class procedures and tail recursion.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-cpstak")

(define (cpstak x y z)

  (define (tak x y z k)
    (if (not (< y x))
        (k z)
        (tak (- x 1)
             y
             z
             (lambda (v1)
               (tak (- y 1)
                    z
                    x
                    (lambda (v2)
                      (tak (- z 1)
                           x
                           y
                           (lambda (v3)
                             (tak v1 v2 v3 k)))))))))

  (tak x y z (lambda (a) a)))

(test-equal "cpstak-8-4-0" 1 (cpstak 8 4 0))
(test-equal "cpstak-10-5-0" 5 (cpstak 10 5 0))

(test-end)
