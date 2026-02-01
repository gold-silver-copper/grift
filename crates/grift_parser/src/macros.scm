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

;; Helper for named let - extract values from bindings and build the call
(define-syntax %named-let-values
  (syntax-rules ()
    ((%named-let-values loop () (vals ...))
     (loop vals ...))
    ((%named-let-values loop ((var val) . rest) (vals ...))
     (%named-let-values loop rest (vals ... val)))))

;; Helper for named let - extract variable names from bindings and build lambda
(define-syntax %named-let-build
  (syntax-rules ()
    ((%named-let-build loop () (vars ...) bindings body ...)
     (letrec ((loop (lambda (vars ...) body ...)))
       (%named-let-values loop bindings ())))
    ((%named-let-build loop ((var val) . rest) (vars ...) bindings body ...)
     (%named-let-build loop rest (vars ... var) bindings body ...))))

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
