;;; PRIMES -- Compute primes less than n, written by Eric Mohr.
;;; Ported from https://github.com/skyfskyf/r7rs-benchmarks
;;; Values reduced for grift performance.

(test-begin "r7rs-bench-primes")

(define (interval-list m n)
  (if (> m n)
      '()
      (cons m (interval-list (+ 1 m) n))))

(define (sieve l)
  (letrec ((remove-multiples
            (lambda (n l)
              (if (null? l)
                  '()
                  (if (= (remainder (car l) n) 0)
                      (remove-multiples n (cdr l))
                      (cons (car l)
                            (remove-multiples n (cdr l))))))))
    (if (null? l)
        '()
        (cons (car l)
              (sieve (remove-multiples (car l) (cdr l)))))))

(define (primes<= n)
  (sieve (interval-list 2 n)))

(test-equal "primes-10" '(2 3 5 7) (primes<= 10))
(test-equal "primes-20" '(2 3 5 7 11 13 17 19) (primes<= 20))
(test-equal "primes-50" '(2 3 5 7 11 13 17 19 23 29 31 37 41 43 47) (primes<= 50))
(test-equal "primes-100-count" 25 (length (primes<= 100)))

(test-end)
