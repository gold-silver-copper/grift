;;; Grift Scheme Prelude
;;;
;;; This file contains all standard macro definitions and library functions.
;;; It is the single source for both compile-time StdLib enum generation
;;; (via include_stdlib!) and runtime macro loading.
;;;
;;; Macro definitions (define-syntax) are evaluated at runtime.
;;; Function definitions (define) are extracted at compile time for StdLib.

;;; ============================================================
;;; Macro Definitions
;;; ============================================================

;;; Standard Scheme Macros for Grift
;;;
;;; These macros are loaded at startup and provide standard R7RS-compatible
;;; macro-based implementations of common forms.
;;;
;;; All macros use syntax-case for pattern matching and template expansion.

;; ============================================================
;; syntax-rules - Declarative Macro Definition (R7RS)
;; ============================================================

;; syntax-rules - Create pattern-based macro transformers
;; 
;; (syntax-rules (literals ...) clause ...)
;; where each clause is ((keyword . pattern) template)
;;
;; This expands to a lambda that uses syntax-case internally.
;; The implementation uses nested ellipsis patterns to handle any number of clauses.
(define-syntax syntax-rules
  (lambda (form)
    (syntax-case form ()
      ((syntax-rules (lit ...) ((keyword . pattern) template) ...)
       (syntax 
         (lambda (x)
           (syntax-case x (lit ...)
             ((dummy . pattern) (syntax template)) ...)))))))

;; define-syntax-rule - Convenient single-clause macro definition
;;
;; (define-syntax-rule (name . pattern) template)
;; =>
;; (define-syntax name
;;   (syntax-rules ()
;;     ((name . pattern) template)))
(define-syntax define-syntax-rule
  (lambda (form)
    (syntax-case form ()
      ((define-syntax-rule (name . pattern) template)
       (syntax (define-syntax name
                 (syntax-rules ()
                   ((name . pattern) template))))))))

;; ============================================================
;; Internal Helpers
;; ============================================================

;; Helper macro for processing a single binding (used by let*)
;; Transforms ((name val) body...) into ((lambda (name) body...) val)
(define-syntax %let-binding
  (lambda (x)
    (syntax-case x ()
      ((%let-binding (name val) body ...)  ;; Match a single binding
       (syntax ((lambda (name) body ...) val))))))  ;; Expand to lambda application

;; Helper for parallel let bindings (used by regular let)
;; Collects all variables and values, then creates a single lambda application
(define-syntax %let-parallel-helper
  (lambda (x)
    (syntax-case x ()
      ;; Base case: all bindings processed, create lambda application
      ((%let-parallel-helper () (vars ...) (vals ...) (body ...))
       (syntax ((lambda (vars ...) body ...) vals ...)))
      ;; Recursive case: extract one var/val pair at a time
      ((%let-parallel-helper ((var val) . rest) (vars ...) (vals ...) (body ...))
       (syntax (%let-parallel-helper rest (vars ... var) (vals ... val) (body ...)))))))

;; ============================================================
;; Binding Forms (let, let*)
;; ============================================================

;; Helper for named let - single helper that builds the complete expansion
;; Replaces the old 3-helper chain (%named-let-build -> %named-let-expand -> %named-let-extract-and-call)
(define-syntax %named-let-helper
  (lambda (x)
    (syntax-case x ()
      ;; Base case: all bindings processed
      ((%named-let-helper loop () (vars ...) (vals ...) (body ...))
       (syntax ((lambda (vars ...)
                  (letrec ((loop (lambda (vars ...) . body)))
                    (loop vars ...)))
                vals ...)))
      ;; Recursive case: extract one var/val pair at a time
      ((%named-let-helper loop ((var val) . rest) (vars ...) (vals ...) (body ...))
       (syntax (%named-let-helper loop rest (vars ... var) (vals ... val) (body ...)))))))

;; let - R5RS parallel binding semantics
;; All values are evaluated first, then all bindings happen simultaneously.
;; Supports both regular let and named let forms.
;; Pattern order matters: more specific patterns first.
(define-syntax let
  (lambda (x)
    (syntax-case x ()
      ;; Empty bindings - just evaluate body
      ((let () body ...)
       (syntax (begin body ...)))
      ;; Regular let with bindings - use parallel binding helper
      ((let ((var val) . rest) body ...)
       (syntax (%let-parallel-helper ((var val) . rest) () () (body ...))))
      ;; Named let: (let name bindings body ...)
      ;; name must be a symbol (not a list), followed by bindings
      ((let loop bindings body ...)
       (syntax (%named-let-helper loop bindings () () (body ...)))))))

;; let* - sequential binding (each binding can refer to previous ones)
;; Uses recursive self-reference (let* calls let*) to ensure each binding
;; is in scope for subsequent bindings. This matches R7RS semantics.
(define-syntax let*
  (lambda (x)
    (syntax-case x ()
      ((let* () body ...)
       (syntax (begin body ...)))
      ((let* (first-binding . rest-bindings) body ...)
       (syntax (%let-binding first-binding
                 (let* rest-bindings body ...)))))))

;; ============================================================
;; Recursive Binding Forms (letrec, letrec*)
;; ============================================================

;; Two-phase helper for letrec:
;; Phase 1: Create all bindings with undefined values
;; Phase 2: Set all bindings to their init values

;; Helper to create all undefined bindings first
(define-syntax %letrec-names
  (lambda (x)
    (syntax-case x ()
      ;; No more bindings - now do the assignments
      ((%letrec-names () bindings body)
       (syntax (%letrec-inits bindings body)))
      ;; Create binding for first name, recurse for rest
      ((%letrec-names ((name init) . rest) bindings body)
       (syntax (let ((name #f))
                 (%letrec-names rest bindings body)))))))

;; Helper to assign all values after all names are bound
(define-syntax %letrec-inits
  (lambda (x)
    (syntax-case x ()
      ;; No more bindings - evaluate body
      ((%letrec-inits () body)
       (syntax body))
      ;; Assign first binding, recurse for rest
      ((%letrec-inits ((name init) . rest) body)
       (syntax (begin
                 (set! name init)
                 (%letrec-inits rest body)))))))

;; letrec - mutually recursive local bindings
;; All variables are visible to all init expressions.
(define-syntax letrec
  (lambda (x)
    (syntax-case x ()
      ((letrec () body ...)
       (syntax (begin body ...)))
      ((letrec bindings body ...)
       (syntax (%letrec-names bindings bindings (begin body ...)))))))

;; letrec* - sequential recursive local bindings  
;; Like letrec, but evaluates init expressions left-to-right.
;; In our implementation, this is the same as letrec.
(define-syntax letrec*
  (lambda (x)
    (syntax-case x ()
      ((letrec* () body ...)
       (syntax (begin body ...)))
      ((letrec* bindings body ...)
       (syntax (%letrec-names bindings bindings (begin body ...)))))))

;; ============================================================
;; Conditionals
;; ============================================================

;; and - logical AND, short-circuits on first #f
(define-syntax and
  (syntax-rules ()
    ((and) #t)
    ((and test) test)
    ((and test rest ...)
     (if test (and rest ...) #f))))

;; or - logical OR, short-circuits on first truthy value
(define-syntax or
  (syntax-rules ()
    ((or) #f)
    ((or test) test)
    ((or test rest ...)
     (let ((temp test))
       (if temp temp (or rest ...))))))

;; when - conditional execution when test is true
(define-syntax when
  (syntax-rules ()
    ((when test body ...)
     (if test (begin body ...)))))

;; unless - conditional execution when test is false
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
    ((case key)
     (if #f #f))
    ((case key (else result ...))
     (begin result ...))
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
  (lambda (x)
    (syntax-case x ()
      ;; Base case - no more bindings
      ((%do-vars () (pairs ...) (steps ...) test result body ...)
       (syntax (%do-run (pairs ...) (steps ...) test result body ...)))
      ;; Binding with step
      ((%do-vars ((var init step) . rest) (pairs ...) (steps ...) test result body ...)
       (syntax (%do-vars rest (pairs ... (var init)) (steps ... step) test result body ...)))
      ;; Binding without step (step = var)
      ((%do-vars ((var init) . rest) (pairs ...) (steps ...) test result body ...)
       (syntax (%do-vars rest (pairs ... (var init)) (steps ... var) test result body ...))))))

;; Helper to run the do loop using named let
(define-syntax %do-run
  (lambda (x)
    (syntax-case x ()
      ((%do-run (bindings ...) (steps ...) test (result ...) body ...)
       (syntax (let %do-loop (bindings ...)
                 (if test
                     (begin (if #f #f) result ...)
                     (begin
                       body ...
                       (%do-loop steps ...)))))))))

;; do - iteration with variable bindings
;; Pattern: (do ((var init step) ...) (test result ...) body ...)
(define-syntax do
  (lambda (x)
    (syntax-case x ()
      ((do bindings (test result ...) body ...)
       (syntax (%do-vars bindings () () test (result ...) body ...))))))

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
    ((append) '())
    ((append a) a)
    ((append a b) (append-two a b))
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
    ((let-values () body ...)
     (begin body ...))
    ((let-values ((formals init)) body ...)
     (call-with-values
       (lambda () init)
       (lambda formals body ...)))
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
    ((let*-values () body ...)
     (begin body ...))
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
  (lambda (x)
    (syntax-case x ()
      ;; Empty formals - just evaluate for side effects
      ((define-values () expr)
       (syntax (define %define-values-dummy
                 (call-with-values (lambda () expr) (lambda args #f)))))
      ;; Single variable - extract using call-with-values
      ((define-values (var) expr)
       (syntax (define var (call-with-values (lambda () expr) (lambda (val) val)))))
      ;; Multiple variables (2 or more) - use ellipsis pattern for arbitrary arity
      ;; var0 holds the list initially, then each var1... extracts and mutates,
      ;; finally varn extracts the last value and sets var0 to its first element
      ((define-values (var0 var1 ... varn) expr)
       (syntax (begin
                 (define var0
                   (call-with-values (lambda () expr) list))
                 (define var1
                   (let ((v (cadr var0)))
                     (set-cdr! var0 (cddr var0))
                     v)) ...
                 (define varn
                   (let ((v (cadr var0)))
                     (set! var0 (car var0))
                     v)))))
      ;; Single identifier (not in a list) - capture all values as a list
      ((define-values var expr)
       (syntax (define var
                 (call-with-values (lambda () expr) list)))))))

;; force - force evaluation of a delayed expression
(define-syntax force
  (syntax-rules ()
    ((force promise)
     (promise))))

;; identifier-syntax - create macros that expand in identifier position (R6RS)
;;
;; (identifier-syntax e) creates a transformer that:
;; - When referenced as a bare identifier, expands to e
;; - When used in application position (id args ...), expands to (e args ...)
;;
;; Example:
;;   (let ((x 0))
;;     (define-syntax x++
;;       (identifier-syntax
;;         (let ((t x)) (set! x (+ t 1)) t)))
;;     (let ((a x++))
;;       (list a x)))  => (0 1)
(define-syntax identifier-syntax
  (lambda (x)
    (syntax-case x ()
      ((_ e)
       (syntax
         (lambda (x)
           (syntax-case x ()
             (id (identifier? (syntax id)) (syntax e))
             ((id rest (... ...)) (identifier? (syntax id)) (syntax (e rest (... ...)))))))))))

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
  (lambda (x)
    (syntax-case x ()
      ;; Exact arity matches for proper lists
      ((%cl-arity-check n ()) (syntax (= n 0)))
      ((%cl-arity-check n (a)) (syntax (= n 1)))
      ((%cl-arity-check n (a b)) (syntax (= n 2)))
      ((%cl-arity-check n (a b c)) (syntax (= n 3)))
      ((%cl-arity-check n (a b c d)) (syntax (= n 4)))
      ((%cl-arity-check n (a b c d e)) (syntax (= n 5)))
      ((%cl-arity-check n (a b c d e f)) (syntax (= n 6)))
      ((%cl-arity-check n (a b c d e f g)) (syntax (= n 7)))
      ((%cl-arity-check n (a b c d e f g h)) (syntax (= n 8)))
      ;; Catch-all: plain symbol (variadic) - matches any arity
      ;; This matches formals like `args` in `(lambda args ...)`
      ((%cl-arity-check n variadic) (syntax #t)))))

;; Helper: Recursively build clause dispatch
;; Tries each clause in order until one matches
(define-syntax %cl-build
  (lambda (x)
    (syntax-case x ()
      ;; No more clauses - error
      ((%cl-build n args ())
       (syntax (error "case-lambda: no matching clause for argument count")))
      ;; Try first clause; if arity matches, apply it; otherwise try rest
      ((%cl-build n args ((formals body ...) . rest))
       (syntax (if (%cl-arity-check n formals)
                   (apply (lambda formals body ...) args)
                   (%cl-build n args rest)))))))

;; Main case-lambda macro
(define-syntax case-lambda
  (lambda (x)
    (syntax-case x ()
      ;; No clauses - error on any call
      ((case-lambda)
       (syntax (lambda args (error "case-lambda: no clauses provided"))))
      ;; Single clause - optimize to regular lambda
      ((case-lambda (formals body ...))
       (syntax (lambda formals body ...)))
      ;; Multiple clauses - dispatch based on argument count
      ((case-lambda clause ...)
       (syntax (lambda %args
                 (let ((%n (length %args)))
                   (%cl-build %n %args (clause ...)))))))))

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
;; Note: Uses syntax-case directly because it has many clauses (more than syntax-rules supports)
(define-syntax %feature-check
  (lambda (x)
    (syntax-case x (and or not library r7rs grift exact-closed exact-complex ratios ieee-float)
      ;; Core features we support
      ((%feature-check r7rs) (syntax #t))
      ((%feature-check grift) (syntax #t))
      ((%feature-check exact-closed) (syntax #t))
      ;; Features we don't support
      ((%feature-check exact-complex) (syntax #f))
      ((%feature-check ratios) (syntax #f))
      ((%feature-check ieee-float) (syntax #f))
      ;; Compound requirements
      ((%feature-check (and)) (syntax #t))
      ((%feature-check (and req)) (syntax (%feature-check req)))
      ((%feature-check (and req1 req2 ...))
       (syntax (if (%feature-check req1)
                   (%feature-check (and req2 ...))
                   #f)))
      ((%feature-check (or)) (syntax #f))
      ((%feature-check (or req)) (syntax (%feature-check req)))
      ((%feature-check (or req1 req2 ...))
       (syntax (if (%feature-check req1)
                   #t
                   (%feature-check (or req2 ...)))))
      ((%feature-check (not req))
       (syntax (if (%feature-check req) #f #t)))
      ;; Library checks - we don't support any libraries yet
      ((%feature-check (library name)) (syntax #f))
      ;; Unknown feature
      ((%feature-check other) (syntax #f)))))

;; Main cond-expand macro
;; Note: Uses syntax-case to properly expand %feature-check at macro-expansion time
(define-syntax cond-expand
  (lambda (x)
    (syntax-case x (else)
      ((cond-expand)
       (syntax (if #f #f)))
      ((cond-expand (else body ...))
       (syntax (begin body ...)))
      ((cond-expand (req body ...))
       (syntax (if (%feature-check req)
                   (begin body ...)
                   (if #f #f))))
      ((cond-expand (req body ...) rest ...)
       (syntax (if (%feature-check req)
                   (begin body ...)
                   (cond-expand rest ...)))))))

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
;; (with-syntax ((pattern expr) ...) body ...)
;;
;; Evaluates each expr and binds the result to the corresponding pattern.
;; The bindings are available in the body expressions.
;; This is implemented as a macro that uses syntax-case internally.
;; NOTE: We use (begin ...) instead of (let () ...) to preserve
;; pattern bindings in the current environment scope. Using let
;; would create a new lambda whose environment doesn't include
;; the #:pattern-bindings from the syntax-case context.
(define-syntax with-syntax
  (lambda (x)
    (syntax-case x ()
      ;; No bindings: just evaluate the body
      ((_ () e1 e2 ...)
       (syntax (begin e1 e2 ...)))
      ;; Single binding: use syntax-case directly
      ((_ ((out in)) e1 e2 ...)
       (syntax (syntax-case in ()
                 (out (begin e1 e2 ...)))))
      ;; Multiple bindings: use syntax-case with a list
      ((_ ((out in) ...) e1 e2 ...)
       (syntax (syntax-case (list in ...) ()
                 ((out ...) (begin e1 e2 ...))))))))

;; ============================================================
;; Exception Handling (R7RS Section 4.2.7)
;; ============================================================

;; guard - Exception handling syntax (R7RS §4.2.7)
;;
;; (guard (var cond-clause ...) body ...)
;;
;; Evaluates body with an exception handler. If an exception is raised,
;; the exception is bound to var and the cond-clauses are evaluated.
;; If no clause matches and there's no else clause, the exception is re-raised.
;;
;; Example:
;;   (guard (exn
;;            ((string? exn) exn)
;;            (else "unknown error"))
;;     (raise "test error"))

;; Helper: Evaluate guard cond clauses (used when exception is caught)
(define-syntax %guard-cond
  (syntax-rules (else)
    ((%guard-cond var (else result ...))
     (begin result ...))
    ((%guard-cond var (test result ...))
     (if test (begin result ...) (raise-continuable var)))
    ((%guard-cond var (test result ...) rest ...)
     (if test (begin result ...) (%guard-cond var rest ...)))))

;; guard - full implementation using with-exception-handler
(define-syntax guard
  (lambda (x)
    (syntax-case x ()
      ((guard (var clause ...) body ...)
       (syntax
         (with-exception-handler
           (lambda (var) (%guard-cond var clause ...))
           (lambda () body ...)))))))

;; ============================================================
;; Dynamic Parameters (R7RS Section 4.2.6)
;; ============================================================

;; parameterize - temporarily bind parameter values using dynamic-wind
;;
;; (parameterize ((param value) ...) body ...)
;;
;; Each param must be a parameter object created by make-parameter.
;; The parameter is set to value for the dynamic extent of body,
;; and restored afterwards (even if body raises an exception or
;; invokes a continuation).
(define-syntax parameterize
  (lambda (x)
    (syntax-case x ()
      ((parameterize () body ...)
       (syntax (begin body ...)))
      ((parameterize ((param value)) body ...)
       (syntax
         (let ((saved (param)))
           (dynamic-wind
             (lambda () (param value))
             (lambda () body ...)
             (lambda () (param saved))))))
      ((parameterize ((p1 v1) rest ...) body ...)
       (syntax
         (parameterize ((p1 v1))
           (parameterize (rest ...) body ...)))))))



;;; ============================================================
;;; Standard Library Function Definitions
;;; ============================================================

;;; Scheme Standard Library
;;; 
;;; This file contains standard library function definitions.
;;; It is processed by the include_stdlib! macro to generate the StdLib enum.
;;;
;;; Format:
;;;   ;;; Documentation comment
;;;   (define (function-name param1 param2 ...) body)

;;; (map f lst) - Apply f to each element of lst (tail-recursive)
(define (map f lst)
  (define (map-iter lst acc)  ;; Helper function for tail-recursive iteration
    (if (null? lst)
        (reverse acc)  ;; Base case: reverse accumulated list
        (map-iter (cdr lst) (cons (f (car lst)) acc))))  ;; Recursive case: apply f and continue
  (map-iter lst '()))  ;; Start with empty accumulator

;;; (filter pred lst) - Return elements where pred is true (tail-recursive)
(define (filter pred lst)
  (define (filter-iter lst acc)  ;; Tail-recursive helper
    (if (null? lst)
        (reverse acc)  ;; Base case
        (if (pred (car lst))  ;; Test if element matches predicate
            (filter-iter (cdr lst) (cons (car lst) acc))  ;; Include element
            (filter-iter (cdr lst) acc))))  ;; Skip element
  (filter-iter lst '()))  ;; Start with empty accumulator

;;; (fold f acc lst) - Left fold over lst
(define (fold f acc lst)  ;; Left-associative fold
  (if (null? lst)
      acc  ;; Base case: return accumulator
      (fold f (f acc (car lst)) (cdr lst))))  ;; Apply f to acc and car, recurse

;;; (fold-left f acc lst) - Left fold over lst (R7RS name, same as fold)
;;; f takes (accumulator, element) and returns new accumulator
(define (fold-left f acc lst)
  (if (null? lst)
      acc
      (fold-left f (f acc (car lst)) (cdr lst))))

;;; (length lst) - Return length of lst (tail-recursive)
(define (length lst)
  (define (length-iter lst acc)
    (if (null? lst) acc (length-iter (cdr lst) (+ acc 1))))
  (length-iter lst 0))

;;; (append-two a b) - Internal: Concatenate exactly two lists (tail-recursive)
;;; This is the workhorse for the variadic append macro.
(define (append-two a b)
  (define (rev-helper lst acc)
    (if (null? lst) acc (rev-helper (cdr lst) (cons (car lst) acc))))
  (define (append-iter lst acc)
    (if (null? lst) acc (append-iter (cdr lst) (cons (car lst) acc))))
  (append-iter (rev-helper a '()) b))

;;; (reverse lst) - Reverse a list
(define (reverse lst) (fold (lambda (acc x) (cons x acc)) '() lst))

;;; (nth n lst) - Get nth element (0-indexed)
;;; Validates that n is a non-negative integer.
(define (nth n lst)
  (if (not (and (integer? n) (exact? n) (>= n 0)))
      (error "nth: invalid index" n)
      (nth-iter n lst n)))
(define (nth-iter n lst original-n)
  (if (null? lst)
      (error "nth: index out of range" original-n)
      (if (= n 0) (car lst)
          (nth-iter (- n 1) (cdr lst) original-n))))

;;; (take n lst) - Take first n elements
;;; Validates that n is a non-negative integer.
(define (take n lst)
  (if (not (and (integer? n) (exact? n) (>= n 0)))
      (error "take: expected non-negative integer" n)
      (take-iter n lst '())))
(define (take-iter n lst acc)
  (if (= n 0) (reverse acc)
      (if (null? lst) (reverse acc)
          (take-iter (- n 1) (cdr lst) (cons (car lst) acc)))))

;;; (drop n lst) - Drop first n elements
;;; Validates that n is a non-negative integer.
(define (drop n lst)
  (if (not (and (integer? n) (exact? n) (>= n 0)))
      (error "drop: expected non-negative integer" n)
      (drop-iter n lst)))
(define (drop-iter n lst)
  (if (= n 0) lst
      (if (null? lst) '()
          (drop-iter (- n 1) (cdr lst)))))

;;; (zip a b) - Zip two lists into list of pairs
(define (zip a b) (if (null? a) '() (if (null? b) '() (cons (cons (car a) (car b)) (zip (cdr a) (cdr b))))))

;;; ============================================================
;;; Internal Helper Functions
;;; ============================================================

;;; (mem-helper pred obj lst) - Generic member helper using predicate
(define (mem-helper pred obj lst) (if (null? lst) #f (if (pred obj (car lst)) lst (mem-helper pred obj (cdr lst)))))

;;; (assoc-helper pred key alist) - Generic assoc helper using predicate
(define (assoc-helper pred key alist) (if (null? alist) #f (if (pred key (car (car alist))) (car alist) (assoc-helper pred key (cdr alist)))))

;;; ============================================================
;;; Member and Assoc Functions (Using Helpers)
;;; ============================================================

;;; (member x lst) - Find x in lst using equal?, return sublist or #f
(define (member x lst) (mem-helper equal? x lst))

;;; (assoc key alist) - Look up key in association list using equal?
(define (assoc key alist) (assoc-helper equal? key alist))

;;; (range start end) - Generate list of integers [start, end) (tail-recursive)
(define (range start end)
  (define (range-iter n acc)
    (if (< n start)
        acc
        (range-iter (- n 1) (cons n acc))))
  (range-iter (- end 1) '()))

;;; (compose f g) - Return function that applies g then f
(define (compose f g) (lambda (x) (f (g x))))

;;; (identity x) - Return x unchanged
(define (identity x) x)

;;; (constantly x) - Return function that always returns x
(define (constantly x) (lambda (y) x))

;;; (flip f) - Flip argument order of binary function
(define (flip f) (lambda (a b) (f b a)))

;;; (curry f x) - Partial application
(define (curry f x) (lambda (y) (f x y)))

;;; (cadr lst) - (car (cdr lst))
(define (cadr lst) (car (cdr lst)))

;;; (caddr lst) - (car (cdr (cdr lst)))
(define (caddr lst) (car (cdr (cdr lst))))

;;; (cddr lst) - (cdr (cdr lst))
(define (cddr lst) (cdr (cdr lst)))

;;; ============================================================
;;; Phase 1: Core R7RS Procedures (Section 6.3-6.4)
;;; ============================================================

;;; (for-each f lst) - Apply f to each element for side effects
;;; R7RS: The value returned is unspecified
;;; We use (if #f #f) to produce an unspecified value (standard Scheme idiom)
(define (for-each f lst) (if (null? lst) (if #f #f) (begin (f (car lst)) (for-each f (cdr lst)))))

;;; (list-tail lst k) - Return sublist starting at k-th element
;;; Validates that k is a valid non-negative index.
(define (list-tail lst k)
  (if (not (and (integer? k) (exact? k) (>= k 0)))
      (error "list-tail: invalid index" k)
      (list-tail-iter lst k)))
(define (list-tail-iter lst k)
  (if (= k 0) lst
      (if (null? lst)
          (error "list-tail: index out of range" k)
          (list-tail-iter (cdr lst) (- k 1)))))

;;; (list-ref lst k) - Return k-th element of lst (0-indexed)
;;; Validates that lst is a proper list and k is a non-negative integer.
(define (list-ref lst k)
  (if (not (list? lst))
      (error "list-ref: not a list" lst)
      (if (not (and (integer? k) (exact? k) (>= k 0)))
          (error "list-ref: invalid index" k)
          (list-ref-iter lst k k))))
(define (list-ref-iter lst k original-k)
  (if (null? lst)
      (error "list-ref: index out of range" original-k)
      (if (= k 0) (car lst)
          (list-ref-iter (cdr lst) (- k 1) original-k))))

;;; (list? obj) - Check if obj is a proper list
(define (list? obj) (if (null? obj) #t (if (pair? obj) (list? (cdr obj)) #f)))

;;; (list-copy lst) - Create a shallow copy of a list
(define (list-copy lst) (if (null? lst) '() (cons (car lst) (list-copy (cdr lst)))))

;;; (memq obj lst) - Find obj in lst using eq?, return sublist or #f
(define (memq obj lst) (mem-helper eq? obj lst))

;;; (memv obj lst) - Find obj in lst using eqv?, return sublist or #f
(define (memv obj lst) (mem-helper eqv? obj lst))

;;; (assq key alist) - Look up key in alist using eq?
(define (assq key alist) (assoc-helper eq? key alist))

;;; (assv key alist) - Look up key in alist using eqv?
(define (assv key alist) (assoc-helper eqv? key alist))

;;; ============================================================
;;; Additional c...r accessors (R7RS Section 6.4)
;;; ============================================================

;;; (caar lst) - (car (car lst))
(define (caar lst) (car (car lst)))

;;; (cdar lst) - (cdr (car lst))
(define (cdar lst) (cdr (car lst)))

;;; (caaar lst) - (car (car (car lst)))
(define (caaar lst) (car (car (car lst))))

;;; (caadr lst) - (car (car (cdr lst)))
(define (caadr lst) (car (car (cdr lst))))

;;; (cadar lst) - (car (cdr (car lst)))
(define (cadar lst) (car (cdr (car lst))))

;;; (cdaar lst) - (cdr (car (car lst)))
(define (cdaar lst) (cdr (car (car lst))))

;;; (cdadr lst) - (cdr (car (cdr lst)))
(define (cdadr lst) (cdr (car (cdr lst))))

;;; (cddar lst) - (cdr (cdr (car lst)))
(define (cddar lst) (cdr (cdr (car lst))))

;;; (cdddr lst) - (cdr (cdr (cdr lst)))
(define (cdddr lst) (cdr (cdr (cdr lst))))

;;; (cadddr lst) - (car (cdr (cdr (cdr lst))))
(define (cadddr lst) (car (cdr (cdr (cdr lst)))))

;;; (cddddr lst) - (cdr (cdr (cdr (cdr lst))))
(define (cddddr lst) (cdr (cdr (cdr (cdr lst)))))

;;; ============================================================
;;; Number utilities (R7RS Section 6.2.6)
;;; ============================================================

;;; (modulo a b) already builtin - use remainder-based modulo for stdlib
;;; (sign n) - Return -1, 0, or 1 based on sign of n
(define (sign n) (if (positive? n) 1 (if (negative? n) -1 0)))

;;; (sqrt x) - Integer square root using Newton's method
;;; Returns the largest integer whose square is <= x.
;;; Raises an error if x is negative.
(define (sqrt x)
  (if (not (and (integer? x) (exact? x)))
      (error "sqrt: expected exact integer" x)
      (if (negative? x)
          (error "sqrt: negative argument" x)
          (if (= x 0)
              0
              (sqrt-iter x x)))))
(define (sqrt-iter x guess)
  (let ((next (/ (+ guess (/ x guess)) 2)))
    (if (>= next guess)
        guess
        (sqrt-iter x next))))

;;; (square x) - Return x squared
(define (square x) (* x x))

;;; (cube x) - Return x cubed
(define (cube x) (* x x x))

;;; (sum lst) - Sum all elements in a list
(define (sum lst) (fold + 0 lst))

;;; (product lst) - Product of all elements in a list
(define (product lst) (fold * 1 lst))

;;; (average lst) - Average of all elements in a list
;;; Raises an error if the list is empty (division by zero).
(define (average lst)
  (if (null? lst)
      (error "average: empty list")
      (/ (sum lst) (length lst))))

;;; ============================================================
;;; Additional R7RS List Functions (Section 6.4)
;;; ============================================================

;;; (make-list k fill) - Create a list of k elements, each initialized to fill.
;;; Validates that k is a non-negative integer.
(define (make-list k fill)
  (if (not (and (integer? k) (exact? k)))
      (error "make-list: expected exact integer" k)
      (if (< k 0)
          (error "make-list: expected non-negative integer" k)
          (make-list-iter k fill '()))))
(define (make-list-iter k fill acc)
  (if (<= k 0)
      acc
      (make-list-iter (- k 1) fill (cons fill acc))))

;;; (list-set! lst k obj) - Store obj in element k of lst
;;; Validates that k is a valid non-negative index.
(define (list-set! lst k obj)
  (if (not (and (integer? k) (exact? k) (>= k 0)))
      (error "list-set!: invalid index" k)
      (set-car! (list-tail lst k) obj)))

;;; (last-pair lst) - Return the last pair in a non-empty list
;;; Raises an error if called on an empty list.
(define (last-pair lst)
  (if (null? lst)
      (error "last-pair: empty list")
      (if (null? (cdr lst)) lst (last-pair (cdr lst)))))

;;; (last lst) - Return the last element of a non-empty list
;;; Raises an error if called on an empty list.
(define (last lst)
  (if (null? lst)
      (error "last: empty list")
      (car (last-pair lst))))

;;; ============================================================
;;; R7RS member/assoc with equal? (Section 6.4)
;;; ============================================================

;;; (member-equal obj lst) - Find obj in lst using equal?, return sublist or #f
(define (member-equal obj lst) (mem-helper equal? obj lst))

;;; (assoc-equal key alist) - Look up key in alist using equal?
(define (assoc-equal key alist) (assoc-helper equal? key alist))

;;; ============================================================
;;; Higher-order list functions (R7RS Section 6.10)
;;; ============================================================

;;; (reduce f init lst) - Right fold (foldr)
(define (reduce f init lst) (if (null? lst) init (f (car lst) (reduce f init (cdr lst)))))

;;; (fold-right f init lst) - Right fold, R7RS name
(define (fold-right f init lst) (reduce f init lst))

;;; (any pred lst) - Return #t if pred is true for any element
(define (any pred lst) (if (null? lst) #f (if (pred (car lst)) #t (any pred (cdr lst)))))

;;; (every pred lst) - Return #t if pred is true for all elements
(define (every pred lst) (if (null? lst) #t (if (pred (car lst)) (every pred (cdr lst)) #f)))

;;; (find pred lst) - Return first element where pred is true, or #f
(define (find pred lst) (if (null? lst) #f (if (pred (car lst)) (car lst) (find pred (cdr lst)))))

;;; (filter-map f lst) - Map f over lst, keeping only non-#f results (tail-recursive, no double calls)
(define (filter-map f lst)
  (define (filter-map-iter lst acc)
    (if (null? lst)
        (reverse acc)
        (let ((result (f (car lst))))
          (if result
              (filter-map-iter (cdr lst) (cons result acc))
              (filter-map-iter (cdr lst) acc)))))
  (filter-map-iter lst '()))

;;; (partition pred lst) - Split lst into pair of two lists: (matching . non-matching)
;;; Returns (cons matches non-matches) where matches contains elements satisfying pred.
;;; Note: Uses tail-recursive helper to avoid let-binding issue in recursion.
(define (partition pred lst) (partition-helper pred lst '() '()))
(define (partition-helper pred lst matches non-matches) (if (null? lst) (cons (reverse matches) (reverse non-matches)) (if (pred (car lst)) (partition-helper pred (cdr lst) (cons (car lst) matches) non-matches) (partition-helper pred (cdr lst) matches (cons (car lst) non-matches)))))

;;; (remove pred lst) - Return lst with elements where pred is true removed
(define (remove pred lst) (filter (lambda (x) (not (pred x))) lst))

;;; (delete x lst) - Remove all occurrences of x from lst using equal?
(define (delete x lst) (filter (lambda (y) (not (equal? x y))) lst))

;;; ============================================================
;;; Boolean operations (R7RS Section 6.3)
;;; ============================================================

;;; (boolean-eq b1 b2) - Return #t if both arguments are #t or both are #f
(define (boolean-eq b1 b2) (or (and b1 b2) (and (not b1) (not b2))))


;;; ============================================================
;;; Character Predicates (R7RS Section 6.6)
;;; ============================================================

;;; (char-alphabetic? char) - Check if char is alphabetic (a-z, A-Z)
(define (char-alphabetic? c)
  (let ((n (char->integer c)))
    (or (and (>= n 65) (<= n 90))
        (and (>= n 97) (<= n 122)))))

;;; (char-numeric? char) - Check if char is a decimal digit (0-9)
(define (char-numeric? c)
  (let ((n (char->integer c)))
    (and (>= n 48) (<= n 57))))

;;; (char-whitespace? char) - Check if char is whitespace
(define (char-whitespace? c)
  (let ((n (char->integer c)))
    (or (= n 32) (= n 9) (= n 10) (= n 13) (= n 12))))

;;; (char-upper-case? char) - Check if char is uppercase (A-Z)
(define (char-upper-case? c)
  (let ((n (char->integer c)))
    (and (>= n 65) (<= n 90))))

;;; (char-lower-case? char) - Check if char is lowercase (a-z)
(define (char-lower-case? c)
  (let ((n (char->integer c)))
    (and (>= n 97) (<= n 122))))

;;; (digit-value char) - Return numeric value (0-9) of a digit character, or #f
(define (digit-value c)
  (let ((n (char->integer c)))
    (if (and (>= n 48) (<= n 57))
        (- n 48)
        #f)))

;;; (char-foldcase char) - Unicode simple case-folding (lowercase for ASCII)
(define (char-foldcase c)
  (char-downcase c))

;;; Case-insensitive character comparisons

;;; (char-ci=? char1 char2 ...) - Case-insensitive char=?
(define (char-ci=? c1 c2)
  (char=? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci<? char1 char2) - Case-insensitive char<?
(define (char-ci<? c1 c2)
  (char<? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci>? char1 char2) - Case-insensitive char>?
(define (char-ci>? c1 c2)
  (char>? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci<=? char1 char2) - Case-insensitive char<=?
(define (char-ci<=? c1 c2)
  (char<=? (char-foldcase c1) (char-foldcase c2)))

;;; (char-ci>=? char1 char2) - Case-insensitive char>=?
(define (char-ci>=? c1 c2)
  (char>=? (char-foldcase c1) (char-foldcase c2)))

;;; ============================================================
;;; String Case-Insensitive Comparisons (R7RS Section 6.7)
;;; ============================================================

;;; (string-ci=? s1 s2) - Case-insensitive string=?
;;; Note: For full implementation, would need to fold case of entire strings
(define (string-ci=? s1 s2)
  (string-ci-compare-helper s1 s2 0 (string-length s1) (string-length s2)))

(define (string-ci-compare-helper s1 s2 i len1 len2)
  (cond
   ((and (= i len1) (= i len2)) #t)
   ((= i len1) #f)
   ((= i len2) #f)
   ((char-ci=? (string-ref s1 i) (string-ref s2 i))
    (string-ci-compare-helper s1 s2 (+ i 1) len1 len2))
   (else #f)))

;;; (string-upcase s) - Convert string to uppercase
(define (string-upcase s)
  (list->string (map char-upcase (string->list s))))

;;; (string-downcase s) - Convert string to lowercase
(define (string-downcase s)
  (list->string (map char-downcase (string->list s))))

;;; (string-foldcase s) - Convert string using case folding
(define (string-foldcase s)
  (list->string (map char-foldcase (string->list s))))

;;; ============================================================
;;; Additional R7RS List Functions (SRFI-1 compatible)
;;; ============================================================

;;; (iota1 count) - Generate list of integers [0, count)
(define (iota1 count)
  (iota-helper count 0 1 '()))

;;; (iota2 count start) - Generate list of integers [start, start+count)
(define (iota2 count start)
  (iota-helper count start 1 '()))

;;; (iota3 count start step) - Generate arithmetic sequence
(define (iota3 count start step)
  (iota-helper count start step '()))

(define (iota-helper count start step acc)
  (if (<= count 0)
      (reverse acc)
      (iota-helper (- count 1) (+ start step) step (cons start acc))))

;;; (list-tabulate n proc) - Create list by applying proc to 0..n-1
(define (list-tabulate n proc)
  (list-tabulate-helper n 0 proc '()))

(define (list-tabulate-helper n i proc acc)
  (if (>= i n)
      (reverse acc)
      (list-tabulate-helper n (+ i 1) proc (cons (proc i) acc))))

;;; (circular-list x ...) - Create a circular list (infinite)
;;; Note: This is dangerous - use carefully or not at all in finite memory

;;; (first lst) - Return first element (alias for car)
(define (first lst) (car lst))

;;; (second lst) - Return second element
(define (second lst) (cadr lst))

;;; (third lst) - Return third element
(define (third lst) (caddr lst))

;;; (fourth lst) - Return fourth element
(define (fourth lst) (cadddr lst))

;;; (fifth lst) - Return fifth element
(define (fifth lst) (car (cddddr lst)))

;;; (sixth lst) - Return sixth element
(define (sixth lst) (cadr (cddddr lst)))

;;; (seventh lst) - Return seventh element
(define (seventh lst) (caddr (cddddr lst)))

;;; (eighth lst) - Return eighth element
(define (eighth lst) (cadddr (cddddr lst)))

;;; (ninth lst) - Return ninth element
(define (ninth lst) (car (cddddr (cddddr lst))))

;;; (tenth lst) - Return tenth element
(define (tenth lst) (cadr (cddddr (cddddr lst))))

;;; (take-right lst k) - Return the last k elements of lst
;;; Uses lag-pointer technique: O(n) single traversal instead of O(n) for length + O(n) for drop
(define (take-right lst k)
  (define (advance p count)
    (if (= count 0)
        p
        (if (null? p)
            '()
            (advance (cdr p) (- count 1)))))
  (define (walk lead lag)
    (if (null? lead)
        lag
        (walk (cdr lead) (cdr lag))))
  (let ((lead (advance lst k)))
    (if (null? lead)
        lst
        (walk lead lst))))

;;; (drop-right lst k) - Return all but the last k elements
;;; Uses lag-pointer technique: O(n) single traversal, tail-recursive with accumulator
(define (drop-right lst k)
  (define (advance p count)
    (if (= count 0)
        p
        (if (null? p)
            '()
            (advance (cdr p) (- count 1)))))
  (define (walk lead lag acc)
    (if (null? lead)
        (reverse acc)
        (walk (cdr lead) (cdr lag) (cons (car lag) acc))))
  (let ((lead (advance lst k)))
    (if (null? lead)
        '()
        (walk lead lst '()))))

;;; (split-at lst k) - Split list at position k, returns (take . drop)
(define (split-at lst k)
  (cons (take k lst) (drop k lst)))

;;; (concatenate lsts) - Append all lists in lsts
(define (concatenate lsts)
  (fold-right append-two '() lsts))

;;; (flatten lst) - Flatten a nested list structure (O(n) tail-recursive)
(define (flatten lst)
  (define (flatten-iter lst acc)
    (cond
      ((null? lst) acc)
      ((not (pair? lst)) (cons lst acc))
      (else (flatten-iter (car lst) (flatten-iter (cdr lst) acc)))))
  (flatten-iter lst '()))

;;; (count pred lst) - Count elements satisfying predicate
(define (count pred lst)
  (fold (lambda (acc x) (if (pred x) (+ acc 1) acc)) 0 lst))

;;; ============================================================
;;; Additional String Functions
;;; ============================================================

;;; (string-for-each proc s) - Apply proc to each character for side effects
(define (string-for-each proc s)
  (for-each proc (string->list s)))

;;; (string-map proc s) - Map proc over characters, return new string
(define (string-map proc s)
  (list->string (map proc (string->list s))))

;;; (string-null? s) - Check if string is empty
(define (string-null? s)
  (= (string-length s) 0))

;;; (string-reverse s) - Reverse a string
(define (string-reverse s)
  (list->string (reverse (string->list s))))

;;; (string-contains s1 s2) - Check if s2 is a substring of s1
;;; Returns index of first occurrence or #f
(define (string-contains s1 s2)
  (let ((len1 (string-length s1))
        (len2 (string-length s2)))
    (if (> len2 len1)
        #f
        (string-contains-helper s1 s2 0 len1 len2))))

(define (string-contains-helper s1 s2 i len1 len2)
  (if (> (+ i len2) len1)
      #f
      (if (string-prefix? s1 s2 i)
          i
          (string-contains-helper s1 s2 (+ i 1) len1 len2))))

(define (string-prefix? s1 s2 start)
  (string-prefix-helper s1 s2 start 0 (string-length s2)))

(define (string-prefix-helper s1 s2 i j len2)
  (if (>= j len2)
      #t
      (if (char=? (string-ref s1 i) (string-ref s2 j))
          (string-prefix-helper s1 s2 (+ i 1) (+ j 1) len2)
          #f)))

;;; (string-join lst sep) - Join list of strings with separator
(define (string-join lst sep)
  (if (null? lst)
      ""
      (fold (lambda (acc s) (string-append acc sep s))
            (car lst)
            (cdr lst))))

;;; (string-split s sep) - Split string by separator character
;;; Returns list of strings
(define (string-split s sep)
  (string-split-helper (string->list s) sep '() '()))

(define (string-split-helper chars sep current result)
  (cond
    ((null? chars)
     (reverse (cons (list->string (reverse current)) result)))
    ((char=? (car chars) sep)
     (string-split-helper (cdr chars) sep '() 
                          (cons (list->string (reverse current)) result)))
    (else
     (string-split-helper (cdr chars) sep (cons (car chars) current) result))))

;;; (string-trim s) - Remove leading and trailing whitespace
(define (string-trim s)
  (list->string (reverse (drop-while-ws (reverse (drop-while-ws (string->list s)))))))

(define (drop-while-ws lst)
  (cond
    ((null? lst) '())
    ((char-whitespace? (car lst)) (drop-while-ws (cdr lst)))
    (else lst)))

;;; ============================================================
;;; Lazy Evaluation Functions (R7RS Section 4.2.5)
;;; ============================================================

;;; (promise? obj) - Check if obj is a promise
;;; Note: In this implementation, promises are procedures (thunks).
;;; This is consistent with R7RS which says "promises are not necessarily
;;; disjoint from other Scheme types such as procedures."
(define (promise? obj)
  (procedure? obj))

;;; (make-promise obj) - Create a promise that returns obj when forced
;;; If obj is already a promise, it is returned unchanged.
;;; This is a procedure, not syntax - it does not delay evaluation.
(define (make-promise obj)
  (if (promise? obj)
      obj
      (lambda () obj)))

;;; ============================================================
;;; Dynamic Parameters (R7RS Section 4.2.6)
;;; ============================================================

;;; (make-parameter init) - Create a parameter object (R7RS §4.2.6)
;;; A parameter object is a procedure that:
;;;   - With no arguments, returns the current value
;;;   - With one argument, sets the value (via parameterize / internal use)
(define (make-parameter init)
  (let ((value init))
    (lambda args
      (if (null? args)
          value
          (set! value (car args))))))

