(test-begin "first-class-procedures")

;; ============================================================================
;; values — must be first-class (R7RS §6.10)
;; ============================================================================

(define my-values values)
(test-equal "values-bind-to-variable"
  3
  (call-with-values (lambda () (my-values 1 2)) +))

(define v-values (make-vector 1))
(vector-set! v-values 0 values)
(test-equal "values-store-in-vector"
  30
  (call-with-values (lambda () ((vector-ref v-values 0) 10 20)) +))

(test-equal "values-pass-to-higher-order"
  6
  (call-with-values (lambda () (apply values '(1 2 3))) +))

(test-assert "values-eq-identity" (eq? values values))
(define v1 values)
(define v2 values)
(test-assert "values-bound-eq-identity" (eq? v1 v2))

(test-assert "values-is-procedure" (procedure? values))

;; ============================================================================
;; call-with-values — must be first-class (R7RS §6.10)
;; ============================================================================

(define my-cwv call-with-values)
(test-equal "call-with-values-bind-to-variable"
  12
  (my-cwv (lambda () (values 3 4)) *))

(test-assert "call-with-values-eq-identity" (eq? call-with-values call-with-values))
(test-assert "call-with-values-is-procedure" (procedure? call-with-values))

;; ============================================================================
;; apply — must be first-class (R7RS §6.4)
;; ============================================================================

(define my-apply apply)
(test-equal "apply-bind-to-variable"
  6
  (my-apply + '(1 2 3)))

(define v-apply (make-vector 1))
(vector-set! v-apply 0 apply)
(test-equal "apply-store-in-vector"
  30
  ((vector-ref v-apply 0) + '(10 20)))

(test-assert "apply-eq-identity" (eq? apply apply))
(test-assert "apply-is-procedure" (procedure? apply))

(test-equal "apply-with-fixed-args"
  6
  (apply + 1 2 '(3)))

;; ============================================================================
;; call/cc — must be first-class (R7RS §6.10)
;; ============================================================================

(define my-cc call/cc)
(test-equal "call/cc-bind-to-variable"
  42
  (my-cc (lambda (k) (k 42))))

(define my-cc2 call-with-current-continuation)
(test-equal "call-with-current-continuation-bind-to-variable"
  99
  (my-cc2 (lambda (k) (k 99))))

(define v-cc (make-vector 1))
(vector-set! v-cc 0 call/cc)
(test-equal "call/cc-store-in-vector"
  77
  ((vector-ref v-cc 0) (lambda (k) (k 77))))

(test-assert "call/cc-eq-identity" (eq? call/cc call/cc))
(test-assert "call/cc-is-procedure" (procedure? call/cc))
(test-assert "call-with-current-continuation-is-procedure" (procedure? call-with-current-continuation))

;; ============================================================================
;; dynamic-wind — must be first-class (R7RS §6.10)
;; ============================================================================

(define my-dw dynamic-wind)
(test-equal "dynamic-wind-bind-to-variable"
  42
  (my-dw (lambda () #f) (lambda () 42) (lambda () #f)))

(test-assert "dynamic-wind-eq-identity" (eq? dynamic-wind dynamic-wind))
(test-assert "dynamic-wind-is-procedure" (procedure? dynamic-wind))

;; ============================================================================
;; with-exception-handler — must be first-class (R7RS §6.11)
;; ============================================================================

(define my-weh with-exception-handler)
(test-equal "with-exception-handler-bind-to-variable"
  42
  (my-weh (lambda (e) 0) (lambda () 42)))

(test-assert "with-exception-handler-eq-identity" (eq? with-exception-handler with-exception-handler))
(test-assert "with-exception-handler-is-procedure" (procedure? with-exception-handler))

;; ============================================================================
;; raise — must be first-class (R7RS §6.11)
;; ============================================================================

(define my-raise raise)
(test-equal "raise-bind-to-variable"
  42
  (with-exception-handler (lambda (e) e) (lambda () (my-raise 42))))

(test-assert "raise-eq-identity" (eq? raise raise))
(test-assert "raise-is-procedure" (procedure? raise))
(test-assert "raise-continuable-is-procedure" (procedure? raise-continuable))

;; ============================================================================
;; eval — must be first-class (R7RS §6.12)
;; ============================================================================

(define my-eval eval)
(test-equal "eval-bind-to-variable"
  3
  (my-eval '(+ 1 2)))

(define v-eval (make-vector 1))
(vector-set! v-eval 0 eval)
(test-equal "eval-store-in-vector"
  30
  ((vector-ref v-eval 0) '(+ 10 20)))

(test-assert "eval-eq-identity" (eq? eval eval))
(test-assert "eval-is-procedure" (procedure? eval))

;; ============================================================================
;; environment — must be first-class (R7RS §6.12)
;; ============================================================================

(define my-env environment)
(test-assert "environment-bind-to-variable" (procedure? my-env))

(test-assert "environment-eq-identity" (eq? environment environment))
(test-assert "environment-is-procedure" (procedure? environment))

;; ============================================================================
;; Cross-procedure first-class tests
;; ============================================================================

(define (hide r x)
  (call-with-values
   (lambda ()
     (values (vector values (lambda (x) x))
             (if (< r 100) 0 1)))
   (lambda (v i)
     ((vector-ref v i) x))))
(test-equal "values-proc-in-hide-pattern"
  42
  (hide 0 42))

(define procs (list values call-with-values apply
                    call/cc call-with-current-continuation
                    dynamic-wind with-exception-handler
                    raise raise-continuable eval environment))
(test-equal "all-procedures-in-list"
  11
  (length (filter procedure? procs)))

(define (get-apply) apply)
(test-equal "return-from-function"
  6
  ((get-apply) + '(1 2 3)))

(define (use-it f) (f + '(1 2 3)))
(test-equal "higher-order-with-apply"
  6
  (use-it apply))

(test-end)
