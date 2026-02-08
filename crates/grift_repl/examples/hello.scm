;;; hello.scm - Example Scheme file for loading into the Grift REPL
;;;
;;; Load this file in the REPL with:
;;;   :load examples/hello.scm

;; Define a greeting function
(define (greet name)
  (display "Hello, ")
  (display name)
  (display "!")
  (newline))

;; Define a factorial function
(define (fact n)
  (if (= n 0) 1 (* n (fact (- n 1)))))

;; Run some examples
(greet "World")
(display "5! = ")
(display (fact 5))
(newline)
