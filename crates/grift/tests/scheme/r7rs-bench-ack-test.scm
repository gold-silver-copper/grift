;;; ACK -- One of the Kernighan and Van Wyk benchmarks.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-ack")

(define (ack m n)
  (cond ((= m 0) (+ n 1))
        ((= n 0) (ack (- m 1) 1))
        (else (ack (- m 1) (ack m (- n 1))))))

(test-equal "ack-0-0" 1 (ack 0 0))
(test-equal "ack-1-1" 3 (ack 1 1))
(test-equal "ack-2-2" 7 (ack 2 2))
(test-equal "ack-2-4" 11 (ack 2 4))
(test-equal "ack-3-3" 61 (ack 3 3))
(test-equal "ack-3-4" 125 (ack 3 4))

(test-end)
