;;; TAK -- A vanilla version of the TAKeuchi function.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-tak")

(define (tak x y z)
  (if (not (< y x))
      z
      (tak (tak (- x 1) y z)
           (tak (- y 1) z x)
           (tak (- z 1) x y))))

(test-equal "tak-8-4-0" 1 (tak 8 4 0))
(test-equal "tak-10-5-0" 5 (tak 10 5 0))
(test-equal "tak-12-6-0" 1 (tak 12 6 0))

(test-end)
