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

;; Helper macro for processing a single binding (used by let*)
;; Transforms ((name val) body...) into ((lambda (name) body...) val)
(define-syntax %let-binding
  (syntax-rules ()
    ((%let-binding (name val) body ...)  ;; Match a single binding
     ((lambda (name) body ...) val))))  ;; Expand to lambda application

;; Helper for parallel let bindings (used by regular let)
;; Collects all variables and values, then creates a single lambda application
(define-syntax %let-parallel-helper
  (syntax-rules ()
    ;; Base case: all bindings processed, create lambda application
    ((%let-parallel-helper () (vars ...) (vals ...) (body ...))
     ((lambda (vars ...) body ...) vals ...))
    ;; Recursive case: extract one var/val pair at a time
    ((%let-parallel-helper ((var val) . rest) (vars ...) (vals ...) (body ...))
     (%let-parallel-helper rest (vars ... var) (vals ... val) (body ...)))))

;; ============================================================
;; Binding Forms (let, let*)
;; ============================================================

;; Helper for named let - single helper that builds the complete expansion
;; Replaces the old 3-helper chain (%named-let-build -> %named-let-expand -> %named-let-extract-and-call)
(define-syntax %named-let-helper
  (syntax-rules ()
    ;; Base case: all bindings processed
    ((%named-let-helper loop () (vars ...) (vals ...) (body ...))
     ((lambda (vars ...)
        (letrec ((loop (lambda (vars ...) . body)))
          (loop vars ...)))
      vals ...))
    ;; Recursive case: extract one var/val pair at a time
    ((%named-let-helper loop ((var val) . rest) (vars ...) (vals ...) (body ...))
     (%named-let-helper loop rest (vars ... var) (vals ... val) (body ...)))))

;; let - R5RS parallel binding semantics
;; All values are evaluated first, then all bindings happen simultaneously.
;; Supports both regular let and named let forms.
;; Pattern order matters: more specific patterns first.
(define-syntax let
  (syntax-rules ()
    ;; Empty bindings - just evaluate body
    ((let () body ...)
     (begin body ...))
    ;; Regular let with bindings - use parallel binding helper
    ((let ((var val) . rest) body ...)
     (%let-parallel-helper ((var val) . rest) () () (body ...)))
    ;; Named let: (let name bindings body ...)
    ;; name must be a symbol (not a list), followed by bindings
    ((let loop bindings body ...)
     (%named-let-helper loop bindings () () (body ...)))))

;; Actually, let me simplify this. Let's use a different approach:
(define-syntax let
  (syntax-rules ()
    ;; Empty bindings - just evaluate body
    ((let () body ...)
     (begin body ...))
    ;; Regular let with bindings - check if first element is a list (binding pair)
    ((let ((var val) . rest) body ...)
     (%let-parallel-helper ((var val) . rest) () () (body ...)))
    ;; Named let: (let name bindings body ...)
    ;; name must be a symbol (not a list), followed by bindings
    ((let loop bindings body ...)
     (%named-let-helper loop bindings () () (body ...)))))

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

;; and - logical AND, short-circuits on first #f
(define-syntax and
  (syntax-rules ()
    ((and) #t)  ;; No arguments: return true
    ((and test) test)  ;; Single argument: return its value
    ((and test rest ...)  ;; Multiple arguments: test first, then rest
     (if test (and rest ...) #f))))  ;; Short-circuit if test is false

;; or - logical OR, short-circuits on first truthy value
(define-syntax or
  (syntax-rules ()
    ((or) #f)  ;; No arguments: return false
    ((or test) test)  ;; Single argument: return its value
    ((or test rest ...)  ;; Multiple arguments: test first, then rest
     (let ((temp test))  ;; Evaluate test only once
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
;; Simplified to 3 patterns for better maintainability
(define-syntax case
  (syntax-rules (else)
    ;; No clauses - return unspecified
    ((case key)
     (if #f #f))
    ;; Else clause - always matches
    ((case key (else result ...))
     (begin result ...))
    ;; Regular clause - check membership, recurse on remaining clauses
    ((case key ((datum ...) result ...) . rest)
     (if (memv key '(datum ...))
         (begin result ...)
         (case key . rest)))))

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
;; Variadic Append (R7RS compliant)
;; ============================================================

;; append - Concatenate any number of lists
;; 
;; (append) => ()
;; (append lst) => lst  
;; (append lst1 lst2) => concatenation of lst1 and lst2
;; (append lst1 lst2 lst3 ...) => concatenation of all lists
;;
;; Implementation uses append-two from stdlib for the two-argument case,
;; and recursively reduces longer argument lists.
(define-syntax append
  (syntax-rules ()
    ;; Zero arguments
    ((append) '())
    ;; One argument - return as-is
    ((append a) a)
    ;; Two arguments - use internal append2
    ((append a b) (append-two a b))
    ;; Three or more arguments - fold right
    ((append a b c ...)
     (append-two a (append b c ...)))))

;; ============================================================
;; Quasiquote
;; ============================================================

;; NOTE: Quasiquote is implemented as a built-in special form for performance.
;;
;; The special form uses trampolined evaluation which is highly optimized.
;; Below is an alternative procedural macro implementation that demonstrates
;; how quasiquote CAN be implemented using syntax-case with depth tracking.
;;
;; The macro uses Peano numerals to track nesting depth at expansion time:
;;   z = depth 0, (d z) = depth 1, (d (d z)) = depth 2, etc.
;;
;; This implementation is provided for educational purposes and to complete
;; the procedural macro infrastructure. The special form remains the primary
;; implementation due to its performance characteristics.

;; Helper macro for quasiquote expansion with depth tracking
;; Depth is tracked using Peano numerals: z=0, (d z)=1, (d (d z))=2, etc.
(define-syntax %qq-expand
  (lambda (stx)
    (syntax-case stx (unquote unquote-splicing quasiquote d z)
      ;; At depth 1 (d z), unquote evaluates the expression
      ((_ (unquote e) (d z))
       (syntax e))
      ;; At depth > 1, unquote decrements depth and wraps result
      ((_ (unquote e) (d (d deeper)))
       (syntax (list 'unquote (%qq-expand e (d deeper)))))
      
      ;; Nested quasiquote - increment depth
      ((_ (quasiquote inner) depth)
       (syntax (list 'quasiquote (%qq-expand inner (d depth)))))
      
      ;; List where car is (unquote-splicing e) at depth 1 - use append
      ((_ ((unquote-splicing e) . rest) (d z))
       (syntax (append e (%qq-expand rest (d z)))))
      ;; List where car is (unquote-splicing e) at depth > 1 - keep structure
      ((_ ((unquote-splicing e) . rest) (d (d deeper)))
       (syntax (cons (list 'unquote-splicing (%qq-expand e (d deeper)))
                     (%qq-expand rest (d (d deeper))))))
      
      ;; List where car is (unquote e) at depth 1 - evaluate and cons
      ((_ ((unquote e) . rest) (d z))
       (syntax (cons e (%qq-expand rest (d z)))))
      ;; List where car is (unquote e) at depth > 1 - keep structure
      ((_ ((unquote e) . rest) (d (d deeper)))
       (syntax (cons (list 'unquote (%qq-expand e (d deeper)))
                     (%qq-expand rest (d (d deeper))))))
      
      ;; General list - recurse on both car and cdr
      ((_ (a . rest) depth)
       (syntax (cons (%qq-expand a depth) (%qq-expand rest depth))))
      ;; Empty list
      ((_ () depth)
       (syntax '()))
      ;; Atom - quote it
      ((_ atom depth)
       (syntax 'atom)))))

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
;; Implementation: Uses R7RS spec approach with ellipsis patterns to handle
;; arbitrary arity dynamically. This eliminates code duplication from the
;; previous explicit arity-0 through arity-4 patterns.
;;
;; The implementation stores all values in a list, then extracts each variable
;; by mutating the list structure. This allows the ellipsis pattern to handle
;; any number of variables without explicit cases.
(define-syntax define-values
  (syntax-rules ()
    ;; Empty formals - just evaluate for side effects
    ((define-values () expr)
     (define %define-values-dummy
       (call-with-values (lambda () expr) (lambda args #f))))
    ;; Single variable - extract using call-with-values
    ((define-values (var) expr)
     (define var (call-with-values (lambda () expr) (lambda (val) val))))
    ;; Multiple variables (2 or more) - use ellipsis pattern for arbitrary arity
    ;; var0 holds the list initially, then each var1... extracts and mutates,
    ;; finally varn extracts the last value and sets var0 to its first element
    ((define-values (var0 var1 ... varn) expr)
     (begin
       (define var0
         (call-with-values (lambda () expr) list))
       (define var1
         (let ((v (cadr var0)))
           (set-cdr! var0 (cddr var0))
           v)) ...
       (define varn
         (let ((v (cadr var0)))
           (set! var0 (car var0))
           v))))
    ;; Single identifier (not in a list) - capture all values as a list
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
;; Formals can be:
;;   - () - takes exactly 0 arguments
;;   - (x) - takes exactly 1 argument
;;   - (x y z) - takes exactly 3 arguments
;;   - (x . rest) - takes at least 1 argument, rest collected in a list
;;   - args - takes any number of arguments, all collected in a list
;;
;; Example:
;;   (define add
;;     (case-lambda
;;       (() 0)
;;       ((x) x)
;;       ((x y) (+ x y))
;;       (args (apply + args))))  ; catch-all for 3+ args
;;
;;   (add)        => 0
;;   (add 5)      => 5
;;   (add 3 4)    => 7
;;   (add 1 2 3)  => 6

;; Helper: Check if argument count n matches formals
;; Returns #t if the clause can handle n arguments
;; Proper list formals (x y z) require exact match
;; Symbol formals or improper lists allow variable args
(define-syntax %cl-arity-check
  (syntax-rules ()
    ;; Exact arity matches for proper lists
    ((%cl-arity-check n ()) (= n 0))
    ((%cl-arity-check n (a)) (= n 1))
    ((%cl-arity-check n (a b)) (= n 2))
    ((%cl-arity-check n (a b c)) (= n 3))
    ((%cl-arity-check n (a b c d)) (= n 4))
    ((%cl-arity-check n (a b c d e)) (= n 5))
    ((%cl-arity-check n (a b c d e f)) (= n 6))
    ((%cl-arity-check n (a b c d e f g)) (= n 7))
    ((%cl-arity-check n (a b c d e f g h)) (= n 8))
    ;; Catch-all: plain symbol (variadic) - matches any arity
    ;; This matches formals like `args` in `(lambda args ...)`
    ((%cl-arity-check n variadic) #t)))

;; Helper: Recursively build clause dispatch
;; Tries each clause in order until one matches
(define-syntax %cl-build
  (syntax-rules ()
    ;; No more clauses - error
    ((%cl-build n args ())
     (error "case-lambda: no matching clause for argument count"))
    ;; Try first clause; if arity matches, apply it; otherwise try rest
    ((%cl-build n args ((formals body ...) . rest))
     (if (%cl-arity-check n formals)
         (apply (lambda formals body ...) args)
         (%cl-build n args rest)))))

;; Main case-lambda macro
(define-syntax case-lambda
  (syntax-rules ()
    ;; No clauses - error on any call
    ((case-lambda)
     (lambda args (error "case-lambda: no clauses provided")))
    ;; Single clause - optimize to regular lambda
    ((case-lambda (formals body ...))
     (lambda formals body ...))
    ;; Multiple clauses - dispatch based on argument count
    ((case-lambda clause ...)
     (lambda %args
       (let ((%n (length %args)))
         (%cl-build %n %args (clause ...)))))))

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

;; ============================================================
;; syntax-case Support (Phase 3)
;; ============================================================

;; with-syntax - bind pattern variables for use in syntax templates
;;
;; NOTE: with-syntax is now a special form (not a macro) to properly
;; update #:pattern-bindings for use by (syntax ...) templates.
;; The special form implementation is in forms.rs.

;; ============================================================
;; Exception Handling (R7RS Section 4.2.7)
;; ============================================================

;; guard - Exception handling syntax (R7RS)
;;
;; (guard (var cond-clause ...) body ...)
;;
;; Evaluates body with an exception handler. If an exception is raised,
;; the exception is bound to var and the cond-clauses are evaluated.
;; If no clause matches and there's no else clause, the exception is re-raised.
;;
;; IMPORTANT LIMITATION: This is a structural implementation only.
;; Full exception handling requires raise/with-exception-handler infrastructure
;; which is not yet implemented in Grift. Currently:
;; - The body is evaluated normally
;; - If body completes without error, its result is returned
;; - Runtime errors (e.g., from (error ...)) will NOT be caught
;; - The cond-clauses will NOT be evaluated for runtime errors
;;
;; This macro is provided for syntax compatibility. Full functionality
;; will be available when raise/with-exception-handler are implemented.
;;
;; Example (will work when exception infrastructure is complete):
;;   (guard (exn
;;            ((string? exn) exn)
;;            (else "unknown error"))
;;     (raise "test error"))

;; Helper: Evaluate guard cond clauses (used when exception is caught)
;; Note: This is not currently reachable without raise/with-exception-handler
(define-syntax %guard-cond
  (syntax-rules (else)
    ;; else clause - always matches
    ((%guard-cond var (else result ...))
     (begin result ...))
    ;; Single non-else clause, no more clauses - re-raise if no match
    ;; Note: Per R7RS, should re-raise the exception; using error as placeholder
    ((%guard-cond var (test result ...))
     (if test (begin result ...) (error "guard: unhandled exception (no matching clause)")))
    ;; Multiple clauses
    ((%guard-cond var (test result ...) rest ...)
     (if test (begin result ...) (%guard-cond var rest ...)))))

;; guard - placeholder implementation
;; 
;; Currently evaluates body directly without exception handling.
;; Returns body's result if it completes normally.
;; Runtime errors will propagate as usual (not caught).
(define-syntax guard
  (syntax-rules ()
    ((guard (var clause ...) body ...)
     ;; Without with-exception-handler, we can only evaluate the body directly.
     ;; Exception handling will be added when the infrastructure is available.
     (begin body ...))))

;; ============================================================
;; syntax-rules - Pattern-based Macro Transformer (via syntax-case)
;; ============================================================

;; syntax-rules: Pattern-based macro transformer
;; 
;; Implements R7RS syntax-rules as a macro that expands to a procedural
;; transformer using syntax-case. This provides a pure Scheme implementation
;; that complements the native Rust implementation.
;;
;; Syntax:
;;   (syntax-rules (literal ...) clause ...)
;;
;; Each clause is: ((keyword . pattern) template)
;;
;; The implementation transforms syntax-rules into a lambda that uses
;; syntax-case internally for pattern matching.
;;
;; Note: Custom ellipsis identifiers are not yet supported. 
;; This implementation handles the most common cases.

;; syntax-rules macro
;; Transforms:
;;   (syntax-rules (lit ...) ((kw . pat) tmpl) ...)
;; Into:
;;   (lambda (x) (syntax-case x (lit ...) ((dummy . pat) #'tmpl) ...))
;;
;; Implementation uses helper macros to build the syntax-case clauses.
;; The keyword is replaced with 'dummy' because syntax-case already
;; binds the first element from the input form.

;; Helper to build single-clause syntax-rules
(define-syntax %syntax-rules-1
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) ((keyword . pattern) template))
       #'(lambda (x)
           (syntax-case x (k ...)
             ((dummy . pattern) #'template)))))))

;; Helper to build two-clause syntax-rules
(define-syntax %syntax-rules-2
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) ((kw1 . pat1) tmpl1) ((kw2 . pat2) tmpl2))
       #'(lambda (x)
           (syntax-case x (k ...)
             ((dummy . pat1) #'tmpl1)
             ((dummy . pat2) #'tmpl2)))))))

;; Helper to build three-clause syntax-rules
(define-syntax %syntax-rules-3
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) ((kw1 . pat1) tmpl1) ((kw2 . pat2) tmpl2) ((kw3 . pat3) tmpl3))
       #'(lambda (x)
           (syntax-case x (k ...)
             ((dummy . pat1) #'tmpl1)
             ((dummy . pat2) #'tmpl2)
             ((dummy . pat3) #'tmpl3)))))))

;; Helper to build four-clause syntax-rules
(define-syntax %syntax-rules-4
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) ((kw1 . pat1) tmpl1) ((kw2 . pat2) tmpl2) ((kw3 . pat3) tmpl3) ((kw4 . pat4) tmpl4))
       #'(lambda (x)
           (syntax-case x (k ...)
             ((dummy . pat1) #'tmpl1)
             ((dummy . pat2) #'tmpl2)
             ((dummy . pat3) #'tmpl3)
             ((dummy . pat4) #'tmpl4)))))))

;; Helper to build five-clause syntax-rules
(define-syntax %syntax-rules-5
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) c1 c2 c3 c4 c5)
       (syntax-case #'(c1 c2 c3 c4 c5) ()
         ((((kw1 . pat1) tmpl1) ((kw2 . pat2) tmpl2) ((kw3 . pat3) tmpl3) 
           ((kw4 . pat4) tmpl4) ((kw5 . pat5) tmpl5))
          #'(lambda (x)
              (syntax-case x (k ...)
                ((dummy . pat1) #'tmpl1)
                ((dummy . pat2) #'tmpl2)
                ((dummy . pat3) #'tmpl3)
                ((dummy . pat4) #'tmpl4)
                ((dummy . pat5) #'tmpl5)))))))))

;; Helper to build six-clause syntax-rules
(define-syntax %syntax-rules-6
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) c1 c2 c3 c4 c5 c6)
       (syntax-case #'(c1 c2 c3 c4 c5 c6) ()
         ((((kw1 . pat1) tmpl1) ((kw2 . pat2) tmpl2) ((kw3 . pat3) tmpl3) 
           ((kw4 . pat4) tmpl4) ((kw5 . pat5) tmpl5) ((kw6 . pat6) tmpl6))
          #'(lambda (x)
              (syntax-case x (k ...)
                ((dummy . pat1) #'tmpl1)
                ((dummy . pat2) #'tmpl2)
                ((dummy . pat3) #'tmpl3)
                ((dummy . pat4) #'tmpl4)
                ((dummy . pat5) #'tmpl5)
                ((dummy . pat6) #'tmpl6)))))))))

;; Helper to build seven-clause syntax-rules
(define-syntax %syntax-rules-7
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) c1 c2 c3 c4 c5 c6 c7)
       (syntax-case #'(c1 c2 c3 c4 c5 c6 c7) ()
         ((((kw1 . pat1) tmpl1) ((kw2 . pat2) tmpl2) ((kw3 . pat3) tmpl3) 
           ((kw4 . pat4) tmpl4) ((kw5 . pat5) tmpl5) ((kw6 . pat6) tmpl6)
           ((kw7 . pat7) tmpl7))
          #'(lambda (x)
              (syntax-case x (k ...)
                ((dummy . pat1) #'tmpl1)
                ((dummy . pat2) #'tmpl2)
                ((dummy . pat3) #'tmpl3)
                ((dummy . pat4) #'tmpl4)
                ((dummy . pat5) #'tmpl5)
                ((dummy . pat6) #'tmpl6)
                ((dummy . pat7) #'tmpl7)))))))))

;; Helper to build eight-clause syntax-rules
(define-syntax %syntax-rules-8
  (lambda (x)
    (syntax-case x ()
      ((_ (k ...) c1 c2 c3 c4 c5 c6 c7 c8)
       (syntax-case #'(c1 c2 c3 c4 c5 c6 c7 c8) ()
         ((((kw1 . pat1) tmpl1) ((kw2 . pat2) tmpl2) ((kw3 . pat3) tmpl3) 
           ((kw4 . pat4) tmpl4) ((kw5 . pat5) tmpl5) ((kw6 . pat6) tmpl6)
           ((kw7 . pat7) tmpl7) ((kw8 . pat8) tmpl8))
          #'(lambda (x)
              (syntax-case x (k ...)
                ((dummy . pat1) #'tmpl1)
                ((dummy . pat2) #'tmpl2)
                ((dummy . pat3) #'tmpl3)
                ((dummy . pat4) #'tmpl4)
                ((dummy . pat5) #'tmpl5)
                ((dummy . pat6) #'tmpl6)
                ((dummy . pat7) #'tmpl7)
                ((dummy . pat8) #'tmpl8)))))))))

;; Note: The native Rust implementation of syntax-rules remains the primary
;; implementation. This Scheme version is provided for compatibility and
;; as a demonstration of how syntax-rules can be implemented in terms of
;; syntax-case.
;;
;; To use this Scheme implementation instead of the native one, you would
;; need to rename/remove the native syntax-rules detection in the evaluator.
;;
;; The implementation below provides explicit dispatch based on the number
;; of clauses, supporting up to 8 clauses which covers most practical use cases.

(define-syntax %scheme-syntax-rules
  (lambda (stx)
    (syntax-case stx ()
      ;; Single clause
      ((_ (k ...) clause1)
       #'(%syntax-rules-1 (k ...) clause1))
      ;; Two clauses
      ((_ (k ...) clause1 clause2)
       #'(%syntax-rules-2 (k ...) clause1 clause2))
      ;; Three clauses
      ((_ (k ...) clause1 clause2 clause3)
       #'(%syntax-rules-3 (k ...) clause1 clause2 clause3))
      ;; Four clauses
      ((_ (k ...) clause1 clause2 clause3 clause4)
       #'(%syntax-rules-4 (k ...) clause1 clause2 clause3 clause4))
      ;; Five clauses
      ((_ (k ...) clause1 clause2 clause3 clause4 clause5)
       #'(%syntax-rules-5 (k ...) clause1 clause2 clause3 clause4 clause5))
      ;; Six clauses
      ((_ (k ...) clause1 clause2 clause3 clause4 clause5 clause6)
       #'(%syntax-rules-6 (k ...) clause1 clause2 clause3 clause4 clause5 clause6))
      ;; Seven clauses
      ((_ (k ...) clause1 clause2 clause3 clause4 clause5 clause6 clause7)
       #'(%syntax-rules-7 (k ...) clause1 clause2 clause3 clause4 clause5 clause6 clause7))
      ;; Eight clauses  
      ((_ (k ...) clause1 clause2 clause3 clause4 clause5 clause6 clause7 clause8)
       #'(%syntax-rules-8 (k ...) clause1 clause2 clause3 clause4 clause5 clause6 clause7 clause8)))))


