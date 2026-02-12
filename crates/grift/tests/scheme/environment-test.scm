(test-begin "environment")

;; ============================================================================
;; interaction-environment (R7RS §6.12)
;; ============================================================================

;; test_interaction_environment_returns_environment
(test-assert "interaction-environment returns a mutable environment"
  (environment? (interaction-environment)))

;; test_eval_with_interaction_environment
(test-equal "eval with interaction-environment"
  3
  (eval '(+ 1 2) (interaction-environment)))

;; test_eval_with_interaction_environment_sees_definitions
(define x 42)
(test-equal "eval with interaction-environment sees definitions"
  42
  (eval 'x (interaction-environment)))

;; test_eval_1_arg_still_works
(test-equal "eval 1-arg still works"
  30
  (eval '(+ 10 20)))

;; test_eval_2_arg_with_expression
(test-equal "eval 2-arg with expression"
  21
  (eval '(* 3 (+ 2 5)) (interaction-environment)))

;; ============================================================================
;; environment (R7RS §6.12)
;; ============================================================================

;; test_environment_with_library
(define-library (test math)
  (export add1)
  (begin (define (add1 x) (+ x 1))))

(test-assert "environment with library returns an environment"
  (environment? (environment '(test math))))

;; test_eval_in_library_environment
(define math-env (environment '(test math)))
(test-equal "eval in library environment"
  11
  (eval '(add1 10) math-env))

;; test_environment_with_quoted_spec
(define-library (test greet)
  (export hi)
  (begin (define hi 99)))

(define greet-env (environment '(test greet)))
(test-equal "environment with quoted spec"
  99
  (eval 'hi greet-env))

;; test_environment_multiple_libraries
(define-library (lib a)
  (export x)
  (begin (define x 10)))

(define-library (lib b)
  (export y)
  (begin (define y 20)))

(define combined-env (environment '(lib a) '(lib b)))
(test-equal "environment multiple libraries - x"
  10
  (eval 'x combined-env))
(test-equal "environment multiple libraries - y"
  20
  (eval 'y combined-env))

;; test_environment_display
(test-equal "environment displays as #<environment>"
  "#<environment>"
  (let ((port (open-output-string)))
    (display (interaction-environment) port)
    (get-output-string port)))

;; test_eval_non_environment_error
(test-error "eval with non-environment second arg should error"
  (lambda () (eval '(+ 1 2) 42)))

;; ============================================================================
;; Large environment tests (validates merge_environments fix)
;; ============================================================================

;; test_let_with_many_bindings
;; A let with 100 bindings (v0=0, v1=1, ..., v99=99), summing them all.
;; Sum of 0..99 = 4950
(test-equal "let with 100 bindings"
  4950
  (eval
    (list 'let
      (let loop ((i 0) (bindings '()))
        (if (= i 100)
          (reverse bindings)
          (loop (+ i 1)
                (cons (list (string->symbol
                              (string-append "v" (number->string i)))
                            i)
                      bindings))))
      (let loop ((i 0) (vars '()))
        (if (= i 100)
          (cons '+ (reverse vars))
          (loop (+ i 1)
                (cons (string->symbol
                        (string-append "v" (number->string i)))
                      vars)))))
    (interaction-environment)))

(test-end)
