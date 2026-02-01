;;; Standard Scheme Macros for Grift
;;;
;;; These macros are loaded at startup and provide standard R7RS-compatible
;;; macro-based implementations of common forms.

;; ============================================================
;; Binding Forms (R7RS Section 4.2.2)
;; ============================================================

;; let - basic binding form
;; Regular: (let ((var val) ...) body ...)
;; Named: (let name ((var val) ...) body ...) for iteration
;; Note: Regular let must come first because named let's 'name' would
;; match any expression including the binding list
(define-syntax let
  (syntax-rules ()
    ;; Regular let: (let ((var val) ...) body ...)
    ((let ((var val) ...) body1 body2 ...)
     ((lambda (var ...) body1 body2 ...) val ...))
    ;; Named let: (let name ((var val) ...) body ...)
    ((let name ((var val) ...) body1 body2 ...)
     (letrec ((name (lambda (var ...) body1 body2 ...)))
       (name val ...)))))

;; let* - sequential binding form
(define-syntax let*
  (syntax-rules ()
    ((let* () body1 body2 ...)
     (let () body1 body2 ...))
    ((let* ((name1 val1) (name2 val2) ...)
       body1 body2 ...)
     (let ((name1 val1))
       (let* ((name2 val2) ...)
         body1 body2 ...)))))

;; letrec - recursive binding form
;; We use begin to sequence the set! calls and body
(define-syntax letrec
  (syntax-rules ()
    ((letrec () body1 body2 ...)
     (let () body1 body2 ...))
    ((letrec ((var1 init1)) body1 body2 ...)
     (let ((var1 #f))
       (set! var1 init1)
       (let () body1 body2 ...)))
    ((letrec ((var1 init1) (var2 init2) ...) body1 body2 ...)
     (let ((var1 #f))
       (set! var1 init1)
       (letrec ((var2 init2) ...) body1 body2 ...)))))

;; letrec* - sequential recursive binding form
(define-syntax letrec*
  (syntax-rules ()
    ((letrec* () body1 body2 ...)
     (let () body1 body2 ...))
    ((letrec* ((var1 init1) (var2 init2) ...)
       body1 body2 ...)
     (let ((var1 #f))
       (set! var1 init1)
       (letrec* ((var2 init2) ...)
         body1 body2 ...)))))

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
;; Pattern Matching (case)
;; ============================================================

;; case - Pattern matching on datum values
(define-syntax case
  (syntax-rules (else)
    ((case key (else result1 result2 ...))
     (begin result1 result2 ...))
    ((case key ((atoms ...) result1 result2 ...))
     (if (memv key '(atoms ...))
         (begin result1 result2 ...)))
    ((case key ((atoms ...) result1 result2 ...) clause ...)
     (if (memv key '(atoms ...))
         (begin result1 result2 ...)
         (case key clause ...)))))

;; ============================================================
;; Iteration (do)
;; ============================================================

;; do - General iteration construct
;; Helper macro for extracting step expression
(define-syntax do-step
  (syntax-rules ()
    ((do-step var) var)
    ((do-step var step) step)))

;; do - (do ((var init step) ...) (test result ...) body ...)
(define-syntax do
  (syntax-rules ()
    ((do ((var init step ...) ...)
         (test result ...)
         body ...)
     (let loop ((var init) ...)
       (if test
           (begin (if #f #f) result ...)
           (begin
             body ...
             (loop (do-step var step ...) ...)))))))

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
