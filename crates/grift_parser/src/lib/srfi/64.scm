;;; (srfi 64) — A Scheme API for test suites
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
(define-library (srfi 64)
  (export test-begin test-end test-group
          test-equal test-eqv test-assert test-error
          test-approximate)
  (begin
    ;; Internal state
    (define %srfi64-suite-name "")
    (define %srfi64-group-name "")
    (define %srfi64-pass-count 0)
    (define %srfi64-fail-count 0)
    (define %srfi64-error-count 0)
    (define %srfi64-results '())

    (define (test-begin suite-name)
      (set! %srfi64-suite-name suite-name)
      (set! %srfi64-pass-count 0)
      (set! %srfi64-fail-count 0)
      (set! %srfi64-error-count 0)
      (set! %srfi64-results '())
      (display "SRFI64:BEGIN ")
      (display suite-name)
      (newline))

    (define (test-end . args)
      (display "SRFI64:SUMMARY passed:")
      (display %srfi64-pass-count)
      (display " failed:")
      (display %srfi64-fail-count)
      (display " errors:")
      (display %srfi64-error-count)
      (newline)
      (display "SRFI64:END ")
      (display %srfi64-suite-name)
      (newline))

    (define (test-group name thunk)
      (display "SRFI64:GROUP:BEGIN ")
      (display name)
      (newline)
      (let ((saved-group %srfi64-group-name))
        (set! %srfi64-group-name name)
        (thunk)
        (set! %srfi64-group-name saved-group))
      (display "SRFI64:GROUP:END ")
      (display name)
      (newline))

    (define (%srfi64-record-pass name)
      (set! %srfi64-pass-count (+ %srfi64-pass-count 1))
      (display "SRFI64:PASS ")
      (display name)
      (newline))

    (define (%srfi64-record-fail name expected actual)
      (set! %srfi64-fail-count (+ %srfi64-fail-count 1))
      (display "SRFI64:FAIL ")
      (display name)
      (display " expected:")
      (write expected)
      (display " actual:")
      (write actual)
      (newline))

    (define (%srfi64-record-error name message)
      (set! %srfi64-error-count (+ %srfi64-error-count 1))
      (display "SRFI64:ERROR ")
      (display name)
      (display " message:")
      (display message)
      (newline))

    (define (test-equal name expected expr)
      (guard (exn
              (#t (%srfi64-record-error
                   name
                   (if (error-object? exn)
                       (error-object-message exn)
                       "unknown error"))))
        (if (equal? expected expr)
            (%srfi64-record-pass name)
            (%srfi64-record-fail name expected expr))))

    (define (test-eqv name expected expr)
      (guard (exn
              (#t (%srfi64-record-error
                   name
                   (if (error-object? exn)
                       (error-object-message exn)
                       "unknown error"))))
        (if (eqv? expected expr)
            (%srfi64-record-pass name)
            (%srfi64-record-fail name expected expr))))

    (define (test-assert name expr)
      (guard (exn
              (#t (%srfi64-record-error
                   name
                   (if (error-object? exn)
                       (error-object-message exn)
                       "unknown error"))))
        (if expr
            (%srfi64-record-pass name)
            (%srfi64-record-fail name #t expr))))

    (define (test-error name thunk)
      (let ((got-error #f))
        (guard (exn
                (#t (set! got-error #t)))
          (thunk))
        (if got-error
            (%srfi64-record-pass name)
            (%srfi64-record-fail name "error" "no error raised"))))

    (define (test-approximate name expected expr tolerance)
      (guard (exn
              (#t (%srfi64-record-error
                   name
                   (if (error-object? exn)
                       (error-object-message exn)
                       "unknown error"))))
        (if (<= (abs (- expected expr)) tolerance)
            (%srfi64-record-pass name)
            (%srfi64-record-fail name expected expr))))))
