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

