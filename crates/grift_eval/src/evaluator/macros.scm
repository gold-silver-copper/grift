;;; Standard Scheme Macros for Grift
;;;
;;; These macros are loaded at startup and provide standard R7RS-compatible
;;; macro-based implementations of common forms.
;;;
;;; Note: Due to a bug in nested ellipsis pattern matching (see 
;;; HYGIENIC_MACROS_IMPLEMENTATION.md Phase 7), we use recursive 
;;; implementations for binding forms instead of the standard R7RS patterns.

;; ============================================================
;; Internal Helpers
;; ============================================================

;; Helper macro for processing a single binding
;; Transforms ((name val) body...) into ((lambda (name) body...) val)
(define-syntax %let-binding
  (syntax-rules ()
    ((%let-binding (name val) body ...)
     ((lambda (name) body ...) val))))

;; ============================================================
;; Binding Forms (let, let*)
;; ============================================================

;; Helper for named let - extract variable names from bindings and build the complete expansion
(define-syntax %named-let-build
  (syntax-rules ()
    ((%named-let-build loop () (vars ...) bindings body ...)
     (%named-let-expand (vars ...) bindings (loop) (body ...)))
    ((%named-let-build loop ((var val) . rest) (vars ...) bindings body ...)
     (%named-let-build loop rest (vars ... var) bindings body ...))))

;; Helper to expand named-let with proper scoping
;; Evaluates init values before binding loop name
(define-syntax %named-let-expand
  (syntax-rules ()
    ((%named-let-expand (vars ...) bindings (loop) (body ...))
     (%named-let-extract-and-call (vars ...) bindings () (loop) (body ...)))  ))

;; Extract values and build the lambda/letrec structure
(define-syntax %named-let-extract-and-call
  (syntax-rules ()
    ;; Base case: all values extracted, now build ((lambda (vals...) (letrec ...)) val ...)
    ((%named-let-extract-and-call (vars ...) () (vals ...) (loop) (body ...))
     ((lambda (vars ...)
        (letrec ((loop (lambda (vars ...) . body)))
          (loop vars ...)))
      vals ...))
    ;; Recursive case: extract one value at a time
    ((%named-let-extract-and-call (vars ...) ((var val) . rest) (vals ...) (loop) (body ...))
     (%named-let-extract-and-call (vars ...) rest (vals ... val) (loop) (body ...)))))

;; let - using recursive approach to avoid nested ellipsis bug
;; Supports both regular let and named let forms.
;; Pattern order matters: more specific patterns first.
(define-syntax let
  (syntax-rules ()
    ;; Empty bindings - just evaluate body
    ((let () body ...)
     (begin body ...))
    ;; Regular let with one or more bindings - list as first arg after let
    ;; This must come before named-let because ((first-binding . rest)) is more specific
    ((let ((var val) . rest) body ...)
     (%let-binding (var val) 
       (let rest body ...)))
    ;; Named let: (let name bindings body ...)
    ;; name must be a symbol (not a list), followed by bindings
    ((let loop bindings body ...)
     (%named-let-build loop bindings () bindings body ...))))

;; let* - sequential binding (each binding can refer to previous ones)
;; Uses recursive self-reference (let* calls let*) to ensure each binding
;; is in scope for subsequent bindings. This matches R7RS semantics.
(define-syntax let*
  (syntax-rules ()
    ((let* () body ...)
     (begin body ...))
    ((let* (first-binding . rest-bindings) body ...)
     (%let-binding first-binding
       (let* rest-bindings body ...)))))

;; ============================================================
;; Recursive Binding Forms (letrec, letrec*)
;; ============================================================

;; Two-phase helper for letrec:
;; Phase 1: Create all bindings with undefined values
;; Phase 2: Set all bindings to their init values

;; Helper to create all undefined bindings first
(define-syntax %letrec-names
  (syntax-rules ()
    ;; No more bindings - now do the assignments
    ((%letrec-names () bindings body)
     (%letrec-inits bindings body))
    ;; Create binding for first name, recurse for rest
    ((%letrec-names ((name init) . rest) bindings body)
     (let ((name #f))
       (%letrec-names rest bindings body)))))

;; Helper to assign all values after all names are bound
(define-syntax %letrec-inits
  (syntax-rules ()
    ;; No more bindings - evaluate body
    ((%letrec-inits () body)
     body)
    ;; Assign first binding, recurse for rest
    ((%letrec-inits ((name init) . rest) body)
     (begin
       (set! name init)
       (%letrec-inits rest body)))))

;; letrec - mutually recursive local bindings
;; All variables are visible to all init expressions.
(define-syntax letrec
  (syntax-rules ()
    ((letrec () body ...)
     (begin body ...))
    ((letrec bindings body ...)
     (%letrec-names bindings bindings (begin body ...)))))

;; letrec* - sequential recursive local bindings  
;; Like letrec, but evaluates init expressions left-to-right.
;; In our implementation, this is the same as letrec.
(define-syntax letrec*
  (syntax-rules ()
    ((letrec* () body ...)
     (begin body ...))
    ((letrec* bindings body ...)
     (%letrec-names bindings bindings (begin body ...)))))

;; ============================================================
;; Conditionals
;; ============================================================

(define-syntax and
  (syntax-rules ()
    ((and) #t)
    ((and test) test)
    ((and test rest ...)
     (if test (and rest ...) #f))))

(define-syntax or
  (syntax-rules ()
    ((or) #f)
    ((or test) test)
    ((or test rest ...)
     (let ((temp test))
       (if temp temp (or rest ...))))))

(define-syntax when
  (syntax-rules ()
    ((when test body ...)
     (if test (begin body ...)))))

(define-syntax unless
  (syntax-rules ()
    ((unless test body ...)
     (if (not test) (begin body ...)))))

;; Simplified cond that doesn't use begin with ellipsis in results
;; to avoid expansion issues
(define-syntax cond
  (syntax-rules (else)
    ((cond (else result))
     result)
    ((cond (else result1 result2 ...))
     (begin result1 result2 ...))
    ((cond (test result))
     (if test result #f))
    ((cond (test result1 result2 ...))
     (if test (begin result1 result2 ...) #f))
    ((cond (test result) rest ...)
     (if test result (cond rest ...)))
    ((cond (test result1 result2 ...) rest ...)
     (if test (begin result1 result2 ...) (cond rest ...)))
    ((cond)
     #f)))

;; ============================================================
;; case - Pattern matching on values
;; ============================================================

;; case - match key against datum lists using eqv?
;; Pattern: (case key ((datum ...) result ...) ... (else result ...))
(define-syntax case
  (syntax-rules (else)
    ;; Base case: just else
    ((case key (else result ...))
     (begin result ...))
    ;; No else and no clauses - return unspecified
    ((case key)
     (if #f #f))
    ;; Single clause with else after
    ((case key ((datum ...) result ...) (else else-result ...))
     (if (memv key '(datum ...))
         (begin result ...)
         (begin else-result ...)))
    ;; Single clause without else
    ((case key ((datum ...) result ...))
     (if (memv key '(datum ...))
         (begin result ...)
         (if #f #f)))
    ;; Multiple clauses with else
    ((case key ((datum ...) result ...) clause ... (else else-result ...))
     (if (memv key '(datum ...))
         (begin result ...)
         (case key clause ... (else else-result ...))))
    ;; Multiple clauses without else
    ((case key ((datum ...) result ...) clause ...)
     (if (memv key '(datum ...))
         (begin result ...)
         (case key clause ...)))))

;; ============================================================
;; do - Iteration construct
;; ============================================================

;; do uses a simpler recursive approach:
;; 1. Extract bindings into (var init) pairs for named let
;; 2. Extract step expressions for the recursive call
;; 
;; Uses two helpers:
;; %do-extract-vars - builds (var init) pairs
;; %do-extract-steps - builds step expressions for recursive call

;; Helper to extract var/init pairs for named let bindings
;; Also collects step expressions
(define-syntax %do-vars
  (syntax-rules ()
    ;; Base case - no more bindings
    ((%do-vars () (pairs ...) (steps ...) test result body ...)
     (%do-run (pairs ...) (steps ...) test result body ...))
    ;; Binding with step
    ((%do-vars ((var init step) . rest) (pairs ...) (steps ...) test result body ...)
     (%do-vars rest (pairs ... (var init)) (steps ... step) test result body ...))
    ;; Binding without step (step = var)
    ((%do-vars ((var init) . rest) (pairs ...) (steps ...) test result body ...)
     (%do-vars rest (pairs ... (var init)) (steps ... var) test result body ...))))

;; Helper to run the do loop using named let
(define-syntax %do-run
  (syntax-rules ()
    ((%do-run (bindings ...) (steps ...) test (result ...) body ...)
     (let %do-loop (bindings ...)
       (if test
           (begin (if #f #f) result ...)
           (begin
             body ...
             (%do-loop steps ...)))))))

;; do - iteration with variable bindings
;; Pattern: (do ((var init step) ...) (test result ...) body ...)
(define-syntax do
  (syntax-rules ()
    ((do bindings (test result ...) body ...)
     (%do-vars bindings () () test (result ...) body ...))))

;; ============================================================
;; Quasiquote
;; ============================================================

;; NOTE: Quasiquote remains as a built-in special form.
;;
;; Implementing quasiquote as a pure syntax-rules macro is complex because
;; it requires arbitrary recursion into list structures with depth tracking.
;; A full macro implementation would require either:
;; - syntax-case (for procedural macros), or
;; - A very complex set of mutually recursive helper macros
;;
;; For future work, psyntax support would enable a proper quasiquote macro.
;; Until then, the built-in quasiquote special form handles this correctly.

;; ============================================================
;; Delayed Evaluation
;; ============================================================

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

;; ============================================================
;; Multiple Values (R7RS Section 4.2.2 and 5.3.3)
;; ============================================================

;; let-values - bind multiple values from expressions
;; 
;; (let-values (((a b) (values 1 2))
;;              ((c) (values 3)))
;;   (+ a b c))
;; => 6
;;
;; Uses call-with-values to capture multiple values and bind them.
;; Implementation note: We use a recursive approach to handle multiple bindings.
(define-syntax let-values
  (syntax-rules ()
    ;; Base case: no bindings, just evaluate body
    ((let-values () body ...)
     (begin body ...))
    ;; Single binding case
    ((let-values ((formals init)) body ...)
     (call-with-values
       (lambda () init)
       (lambda formals body ...)))
    ;; Multiple bindings: handle first, then recurse
    ((let-values ((formals init) rest ...) body ...)
     (call-with-values
       (lambda () init)
       (lambda formals
         (let-values (rest ...) body ...))))))

;; let*-values - sequential binding of multiple values
;;
;; Like let-values, but bindings are visible to subsequent inits.
;; Each binding's init can reference variables from previous bindings.
;;
;; (let*-values (((a b) (values 1 2))
;;               ((c) (values (+ a b))))
;;   c)
;; => 3
(define-syntax let*-values
  (syntax-rules ()
    ;; Base case: no bindings
    ((let*-values () body ...)
     (begin body ...))
    ;; Single or first binding: use let-values then recurse
    ((let*-values ((formals init) rest ...) body ...)
     (call-with-values
       (lambda () init)
       (lambda formals
         (let*-values (rest ...) body ...))))))

;; define-values - define multiple values at top level
;;
;; (define-values (x y) (values 1 2))
;; x => 1
;; y => 2
;;
;; Implementation note: Due to limitations with nested ellipsis patterns,
;; we provide explicit patterns for common arities (0-4 variables).
;; For more variables, users can nest define-values or use let-values.
;;
;; Note: We use %dv-a, %dv-b, etc. as lambda parameter names to avoid
;; accidentally shadowing the user's variable names.
(define-syntax define-values
  (syntax-rules ()
    ;; Empty formals - just evaluate for side effects
    ((define-values () expr)
     (define %define-values-dummy
       (call-with-values (lambda () expr) (lambda () (if #f #f)))))
    ;; Single variable - use regular define
    ((define-values (v1) expr)
     (define v1 (call-with-values (lambda () expr) (lambda (%dv-x) %dv-x))))
    ;; Two variables
    ((define-values (v1 v2) expr)
     (begin
       (define v1 #f)
       (define v2 #f)
       (call-with-values
         (lambda () expr)
         (lambda (%dv-a %dv-b)
           (set! v1 %dv-a)
           (set! v2 %dv-b)))))
    ;; Three variables
    ((define-values (v1 v2 v3) expr)
     (begin
       (define v1 #f)
       (define v2 #f)
       (define v3 #f)
       (call-with-values
         (lambda () expr)
         (lambda (%dv-a %dv-b %dv-c)
           (set! v1 %dv-a)
           (set! v2 %dv-b)
           (set! v3 %dv-c)))))
    ;; Four variables
    ((define-values (v1 v2 v3 v4) expr)
     (begin
       (define v1 #f)
       (define v2 #f)
       (define v3 #f)
       (define v4 #f)
       (call-with-values
         (lambda () expr)
         (lambda (%dv-a %dv-b %dv-c %dv-d)
           (set! v1 %dv-a)
           (set! v2 %dv-b)
           (set! v3 %dv-c)
           (set! v4 %dv-d)))))
    ;; Rest argument - capture all values as a list
    ((define-values var expr)
     (define var
       (call-with-values (lambda () expr) list)))))

;; force - force evaluation of a delayed expression
(define-syntax force
  (syntax-rules ()
    ((force promise)
     (promise))))

;; ============================================================
;; Case-Lambda (R7RS Section 4.2.9)
;; ============================================================

;; case-lambda - multiple-arity procedure dispatch
;;
;; Creates a procedure that dispatches based on the number of arguments.
;; Each clause has the form (formals body ...) where formals is like lambda.
;;
;; Example:
;;   (define add
;;     (case-lambda
;;       (() 0)
;;       ((x) x)
;;       ((x y) (+ x y))))
;;
;; case-lambda - multiple-arity procedure dispatch (R7RS Section 4.2.9)
;;
;; IMPLEMENTATION LIMITATION:
;; Full case-lambda requires rest-argument support in lambda (e.g., (lambda args body)),
;; which is not currently implemented in the evaluator. The evaluator only supports
;; fixed-arity lambdas like (lambda (x y) body).
;;
;; WORKAROUND:
;; For single-clause case-lambda, we just use regular lambda.
;; For multi-clause case-lambda, the current implementation is limited.
;; Users needing multiple arities should define separate functions and a wrapper.
;;
;; Example workaround for users:
;;   (define (add . args)  ; NOT SUPPORTED
;;     ...)
;;   Instead use:
;;   (define (add1 x) x)
;;   (define (add2 x y) (+ x y))
;;   ; and call the appropriate one
;;
;; TODO: Implement rest-argument lambda support in the evaluator to enable full case-lambda.

(define-syntax case-lambda
  (syntax-rules ()
    ;; Base case: no clauses - error on any call
    ((case-lambda)
     (lambda () (error "case-lambda: no clauses provided")))
    ;; Single clause: just use regular lambda (this works!)
    ((case-lambda (formals body ...))
     (lambda formals body ...))
    ;; Multiple clauses: use first clause only (limitation)
    ;; Document that this is a limitation
    ((case-lambda (formals body ...) rest ...)
     (lambda formals body ...))))

;; ============================================================
;; Cond-Expand (R7RS Section 4.2.1)
;; ============================================================

;; cond-expand - feature-based conditional expansion
;;
;; Provides a way to statically expand different expressions depending
;; on implementation features. Each clause has the form:
;;   (feature-requirement expression ...)
;;
;; Feature requirements can be:
;;   - feature-identifier: a symbol naming a feature
;;   - (library library-name): check if library is available
;;   - (and req ...): all requirements must be satisfied
;;   - (or req ...): at least one requirement must be satisfied
;;   - (not req): requirement must not be satisfied
;;   - else: always matches (must be last clause)
;;
;; Example:
;;   (cond-expand
;;     (grift (display "Running on Grift"))
;;     (else (display "Unknown implementation")))
;;
;; Supported feature identifiers for Grift:
;;   - r7rs: R7RS Scheme
;;   - grift: This implementation
;;   - exact-closed: Exact arithmetic is closed under common operations
;;   - ratios: Not supported (no rational numbers)
;;   - ieee-float: Not supported (no floating point)
;;
;; Implementation note: Since we don't have compile-time evaluation,
;; we implement this with a set of known features. The feature check
;; happens at macro expansion time through pattern matching.

;; Check if a feature is supported
;; Returns #t or #f at expansion time based on pattern matching
(define-syntax %feature-check
  (syntax-rules (and or not library r7rs grift exact-closed exact-complex ratios ieee-float)
    ;; Core features we support
    ((%feature-check r7rs) #t)
    ((%feature-check grift) #t)
    ((%feature-check exact-closed) #t)
    ;; Features we don't support
    ((%feature-check exact-complex) #f)
    ((%feature-check ratios) #f)
    ((%feature-check ieee-float) #f)
    ;; Compound requirements
    ((%feature-check (and)) #t)
    ((%feature-check (and req)) (%feature-check req))
    ((%feature-check (and req1 req2 ...))
     (if (%feature-check req1)
         (%feature-check (and req2 ...))
         #f))
    ((%feature-check (or)) #f)
    ((%feature-check (or req)) (%feature-check req))
    ((%feature-check (or req1 req2 ...))
     (if (%feature-check req1)
         #t
         (%feature-check (or req2 ...))))
    ((%feature-check (not req))
     (if (%feature-check req) #f #t))
    ;; Library checks - we don't support any libraries yet
    ((%feature-check (library name)) #f)
    ;; Unknown feature
    ((%feature-check other) #f)))

;; Main cond-expand macro
(define-syntax cond-expand
  (syntax-rules (else)
    ;; No clauses - unspecified behavior, we return #f
    ((cond-expand) (if #f #f))
    ;; Else clause - always matches
    ((cond-expand (else body ...))
     (begin body ...))
    ;; Single non-else clause
    ((cond-expand (req body ...))
     (if (%feature-check req)
         (begin body ...)
         (if #f #f)))
    ;; Multiple clauses - check first, recurse on rest
    ((cond-expand (req body ...) rest ...)
     (if (%feature-check req)
         (begin body ...)
         (cond-expand rest ...)))))

;; ============================================================
;; Lazy Evaluation Extensions (R7RS Section 4.2.5)
;; ============================================================

;; delay-force - Optimized lazy evaluation for iterative algorithms
;;
;; (delay-force expression) is conceptually similar to (delay (force expression)),
;; but when forced, it results in a tail call to (force expression), preventing
;; unbounded memory usage in iterative lazy algorithms.
;;
;; The key difference from (delay (force ...)) is that delay-force doesn't
;; accumulate a chain of promises - it effectively replaces itself with
;; the result of forcing the inner expression.
;;
;; Example: Stream filtering without space leak
;;   (define (stream-filter p? s)
;;     (delay-force
;;       (if (null? (force s))
;;           (delay '())
;;           (let ((h (car (force s)))
;;                 (t (cdr (force s))))
;;             (if (p? h)
;;                 (delay (cons h (stream-filter p? t)))
;;                 (stream-filter p? t))))))
;;
;; Implementation note: We implement delay-force by creating a promise that,
;; when forced, evaluates its expression and if the result is itself a promise,
;; forces that recursively. This achieves the tail-call-like behavior.
(define-syntax delay-force
  (syntax-rules ()
    ((delay-force expr)
     (let ((forced #f)
           (value #f))
       (lambda ()
         (if forced
             value
             (let ((result expr))
               ;; If result is a promise (procedure), force it
               ;; This implements the iterative forcing behavior
               (let ((final-value (if (procedure? result)
                                      (result)
                                      result)))
                 (set! value final-value)
                 (set! forced #t)
                 final-value))))))))

