;;; (scheme case-lambda) — R7RS §4.2.9
(define-library (scheme case-lambda)
  (export case-lambda)
  (begin
    ;; Helper: Check if argument count n matches formals
    (define-syntax %cl-arity-check
      (lambda (x)
        (syntax-case x ()
          ((%cl-arity-check n ()) (syntax (= n 0)))
          ((%cl-arity-check n (a)) (syntax (= n 1)))
          ((%cl-arity-check n (a b)) (syntax (= n 2)))
          ((%cl-arity-check n (a b c)) (syntax (= n 3)))
          ((%cl-arity-check n (a b c d)) (syntax (= n 4)))
          ((%cl-arity-check n (a b c d e)) (syntax (= n 5)))
          ((%cl-arity-check n (a b c d e f)) (syntax (= n 6)))
          ((%cl-arity-check n (a b c d e f g)) (syntax (= n 7)))
          ((%cl-arity-check n (a b c d e f g h)) (syntax (= n 8)))
          ((%cl-arity-check n variadic) (syntax #t)))))

    ;; Helper: Recursively build clause dispatch
    (define-syntax %cl-build
      (lambda (x)
        (syntax-case x ()
          ((%cl-build n args ())
           (syntax (error "case-lambda: no matching clause for argument count")))
          ((%cl-build n args ((formals body ...) . rest))
           (syntax (if (%cl-arity-check n formals)
                       (apply (lambda formals body ...) args)
                       (%cl-build n args rest)))))))

    ;; Main case-lambda macro
    (define-syntax case-lambda
      (lambda (x)
        (syntax-case x ()
          ((case-lambda)
           (syntax (lambda args (error "case-lambda: no clauses provided"))))
          ((case-lambda (formals body ...))
           (syntax (lambda formals body ...)))
          ((case-lambda clause ...)
           (syntax (lambda %args
                     (let ((%n (length %args)))
                       (%cl-build %n %args (clause ...)))))))))))
