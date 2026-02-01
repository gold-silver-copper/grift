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

;; let - using recursive approach to avoid nested ellipsis bug
;; Note: Named let is not supported by this macro; use the special form.
(define-syntax let
  (syntax-rules ()
    ;; Empty bindings - just evaluate body
    ((let () body ...)
     (begin body ...))
    ;; One or more bindings - use dotted pair matching to process one at a time
    ((let (first-binding . rest-bindings) body ...)
     (%let-binding first-binding 
       (let rest-bindings body ...)))))

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
