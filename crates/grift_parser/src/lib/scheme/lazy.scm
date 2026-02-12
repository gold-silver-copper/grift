;;; (scheme lazy) — R7RS §4.2.5 Delayed evaluation
(define-library (scheme lazy)
  (export delay force delay-force
          make-promise promise?)
  (begin
    ;; delay - create a promise (memoizing thunk)
    (define-syntax delay
      (syntax-rules ()
        ((delay expr)
         (let ((forced #f)
               (value #f))
           (lambda ()
             (if forced
                 value
                 (begin
                   (set! value expr)
                   (set! forced #t)
                   value)))))))

    ;; force - force evaluation of a delayed expression
    (define-syntax force
      (syntax-rules ()
        ((force promise)
         (promise))))

    ;; delay-force - Optimized lazy evaluation for iterative algorithms
    (define-syntax delay-force
      (syntax-rules ()
        ((delay-force expr)
         (let ((forced #f)
               (value #f))
           (lambda ()
             (if forced
                 value
                 (let ((result expr))
                   (let ((final-value (if (procedure? result)
                                          (result)
                                          result)))
                     (set! value final-value)
                     (set! forced #t)
                     final-value))))))))

    ;; promise? - Check if obj is a promise
    (define (promise? obj)
      (procedure? obj))

    ;; make-promise - Create a promise that returns obj when forced
    (define (make-promise obj)
      (if (promise? obj)
          obj
          (lambda () obj)))))
