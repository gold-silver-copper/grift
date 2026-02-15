(define-library (chibi loop)
  (export loop in-string in-string-reverse)
  (import (scheme base))
  (begin
    (define (in-string s) (string->list s))
    (define (in-string-reverse s) (reverse (string->list s)))

    (define-syntax loop
      (syntax-rules (for listing =>)
        ((loop ((for var1 (proc1 arg1)) (for var2 (listing var1))) => var2)
         (let loop-iter ((items (proc1 arg1)) (var2 '()))
           (if (null? items)
               (reverse var2)
               (let ((var1 (car items)))
                 (loop-iter (cdr items) (cons var1 var2))))))))))
