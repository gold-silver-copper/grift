;;; SRFI-64 - A Scheme API for test suites
;;; Minimal implementation for Grift Scheme
;;;
;;; Outputs parseable test results in the format:
;;;   SRFI64:BEGIN <suite-name>
;;;   SRFI64:PASS <test-name>
;;;   SRFI64:FAIL <test-name> expected:<expected> actual:<actual>
;;;   SRFI64:ERROR <test-name> message:<message>
;;;   SRFI64:GROUP:BEGIN <group-name>
;;;   SRFI64:GROUP:END <group-name>
;;;   SRFI64:SUMMARY passed:<n> failed:<n> errors:<n>
;;;   SRFI64:END <suite-name>

;; Internal state
(define %srfi64-suite-name "")
(define %srfi64-group-name "")
(define %srfi64-pass-count 0)
(define %srfi64-fail-count 0)
(define %srfi64-error-count 0)
(define %srfi64-results '())
(define %srfi64-depth 0)
(define %srfi64-test-number 0)
(define %srfi64-section-stack '())
(define %srfi64-group-stack '())

(define (%srfi64-stack->string stack)
  (let loop ((xs (reverse stack)) (acc ""))
    (if (null? xs)
        acc
        (let ((head (car xs)))
          (loop (cdr xs)
                (if (string=? acc "")
                    head
                    (string-append acc " > " head)))))))

;; Build a qualified test name: "#N [section] name"
(define (%srfi64-qualified-name name)
  (set! %srfi64-test-number (+ %srfi64-test-number 1))
  (let* ((section (%srfi64-stack->string %srfi64-section-stack))
         (group (%srfi64-stack->string %srfi64-group-stack))
         (context (if (string=? group "")
                      section
                      (if (string=? section "")
                          group
                          (string-append section " > " group)))))
    (string-append "#"
                   (number->string %srfi64-test-number)
                   " [" context "] "
                   name)))

(define (test-begin suite-name)
  (if (= %srfi64-depth 0)
      (begin
        (set! %srfi64-suite-name suite-name)
        (set! %srfi64-pass-count 0)
        (set! %srfi64-fail-count 0)
        (set! %srfi64-error-count 0)
        (set! %srfi64-results '())
        (set! %srfi64-test-number 0)
        (set! %srfi64-section-stack '())
        (set! %srfi64-group-stack '())))
  (set! %srfi64-section-stack (cons suite-name %srfi64-section-stack))
  (set! %srfi64-depth (+ %srfi64-depth 1))
  (display "SRFI64:BEGIN ")
  (display suite-name)
  (newline))

(define (test-end . args)
  (set! %srfi64-depth (- %srfi64-depth 1))
  (if (not (null? %srfi64-section-stack))
      (set! %srfi64-section-stack (cdr %srfi64-section-stack)))
  (if (= %srfi64-depth 0)
      (begin
        (display "SRFI64:SUMMARY passed:")
        (display %srfi64-pass-count)
        (display " failed:")
        (display %srfi64-fail-count)
        (display " errors:")
        (display %srfi64-error-count)
        (newline)))
  (display "SRFI64:END ")
  (display (if (null? args) %srfi64-suite-name (car args)))
  (newline))

(define (%srfi64-run-group name thunk)
  (display "SRFI64:GROUP:BEGIN ")
  (display name)
  (newline)
  (let ((saved-group %srfi64-group-name))
    (set! %srfi64-group-name name)
    (set! %srfi64-group-stack (cons name %srfi64-group-stack))
    (thunk)
    (set! %srfi64-group-stack (cdr %srfi64-group-stack))
    (set! %srfi64-group-name saved-group))
  (display "SRFI64:GROUP:END ")
  (display name)
  (newline))

(define-syntax test-group
  (syntax-rules ()
    ((_ name body ...)
     (%srfi64-run-group name (lambda () body ...)))))

(define (%srfi64-record-pass name)
  (let ((qname (%srfi64-qualified-name name)))
    (set! %srfi64-pass-count (+ %srfi64-pass-count 1))
    (display "SRFI64:PASS ")
    (display qname)
    (newline)))

(define (%srfi64-record-fail name expected actual)
  (let ((qname (%srfi64-qualified-name name)))
    (set! %srfi64-fail-count (+ %srfi64-fail-count 1))
    (display "SRFI64:FAIL ")
    (display qname)
    (display " expected:")
    (write expected)
    (display " actual:")
    (write actual)
    (newline)))

(define (%srfi64-record-error name message)
  (let ((qname (%srfi64-qualified-name name)))
    (set! %srfi64-error-count (+ %srfi64-error-count 1))
    (display "SRFI64:ERROR ")
    (display qname)
    (display " message:")
    (display message)
    (newline)))

(define (%approx-equal? a b)
  ;; Approximate comparison for inexact numbers (like Chibi's test library)
  (cond
    ((and (number? a) (number? b) (or (inexact? a) (inexact? b))
          (real? a) (real? b))
     (cond
       ((and (infinite? a) (infinite? b)) (= a b))
       ((and (nan? a) (nan? b)) #t)
       ((or (nan? a) (nan? b)) #f)
       ((or (infinite? a) (infinite? b)) #f)
       (else (< (abs (- a b)) (max 1e-6 (* (abs a) 1e-6))))))
    ((and (number? a) (number? b) (or (inexact? a) (inexact? b)))
     ;; Complex number comparison: compare real and imag parts
     (and (%approx-equal? (real-part a) (real-part b))
          (%approx-equal? (imag-part a) (imag-part b))))
    ((and (pair? a) (pair? b))
     (and (%approx-equal? (car a) (car b))
          (%approx-equal? (cdr a) (cdr b))))
    ((and (vector? a) (vector? b) (= (vector-length a) (vector-length b)))
     (let loop ((i 0))
       (or (= i (vector-length a))
           (and (%approx-equal? (vector-ref a i) (vector-ref b i))
                (loop (+ i 1))))))
    (else (equal? a b))))

(define-syntax test-equal
  (syntax-rules ()
    ((_ name expected expr)
     (guard (exn
             (#t (%srfi64-record-error
                  name
                  (if (error-object? exn)
                      (string-append
                       (error-object-message exn)
                       (if (read-error? exn) " [read-error]" "")
                       (if (null? (error-object-irritants exn))
                           ""
                           (string-append " irritants:" (let ((out (open-output-string)))
                                                           (write (error-object-irritants exn) out)
                                                           (get-output-string out)))))
                      "unknown error"))))
       (let ((e expected) (a expr))
         (if (%approx-equal? e a)
             (%srfi64-record-pass name)
             (%srfi64-record-fail name e a)))))))

(define-syntax test-eqv
  (syntax-rules ()
    ((_ name expected expr)
     (guard (exn
             (#t (%srfi64-record-error
                  name
                  (if (error-object? exn)
                      (error-object-message exn)
                      "unknown error"))))
       (let ((e expected) (a expr))
         (if (eqv? e a)
             (%srfi64-record-pass name)
             (%srfi64-record-fail name e a)))))))

(define-syntax test-assert
  (syntax-rules ()
    ((_ name expr)
     (guard (exn
             (#t (%srfi64-record-error
                  name
                  (if (error-object? exn)
                      (error-object-message exn)
                      "unknown error"))))
       (let ((a expr))
         (if a
             (%srfi64-record-pass name)
             (%srfi64-record-fail name #t a)))))))

(define (%test-error-impl name thunk)
  (let ((got-error #f))
    (guard (exn
            (#t (set! got-error #t)))
      (thunk))
    (if got-error
        (%srfi64-record-pass name)
        (%srfi64-record-fail name "error" "no error raised"))))

(define-syntax test-error
  (syntax-rules ()
    ((_ name thunk)
     (%test-error-impl name thunk))
    ((_ expr)
     (%test-error-impl "test-error" (lambda () expr)))))

(define-syntax test-approximate
  (syntax-rules ()
    ((_ name expected expr tolerance)
     (guard (exn
             (#t (%srfi64-record-error
                  name
                  (if (error-object? exn)
                      (error-object-message exn)
                      "unknown error"))))
       (let ((e expected) (a expr) (t tolerance))
         (if (<= (abs (- e a)) t)
             (%srfi64-record-pass name)
             (%srfi64-record-fail name e a)))))))
