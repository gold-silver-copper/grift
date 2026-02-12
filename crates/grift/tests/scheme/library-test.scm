;;; Library system tests
;;; Migrated from library_tests.rs

(test-begin "library")

;; ============================================================================
;; define-library & import basics
;; ============================================================================

(define-library (test lib)
  (export greet)
  (begin
    (define (greet) 42)))

(import (test lib))
(test-equal "define-library-and-import" 42 (greet))

(define-library (test isolation)
  (export public-fn)
  (begin
    (define (helper x) (* x 2))
    (define (public-fn x) (helper x))))

(import (test isolation))
(test-equal "library-isolation" 10 (public-fn 5))

(define-library (test all)
  (begin
    (define (foo) 1)
    (define (bar) 2)))

(import (test all))
(test-equal "no-exports-exports-all-foo" 1 (foo))
(test-equal "no-exports-exports-all-bar" 2 (bar))

;; ============================================================================
;; Auto-loading from embedded library sources
;; ============================================================================

(import (scheme base))
(test-equal "import-scheme-base" 3 (+ 1 2))

(import (scheme cxr))
(test-equal "import-scheme-cxr-cadr" 2 (cadr '(1 2 3)))
(test-equal "import-scheme-cxr-caddr" 3 (caddr '(1 2 3)))

(import (scheme char))
(test-assert "import-scheme-char-alphabetic" (char-alphabetic? #\a))
(test-assert "import-scheme-char-not-alphabetic" (not (char-alphabetic? #\1)))

(import (scheme write))

(import (scheme lazy))
(test-equal "import-scheme-lazy-force-delay" 42 (force (delay 42)))

(import (scheme inexact))
(test-assert "import-scheme-inexact-finite" (finite? 1))

(import (scheme file))

(import (scheme eval))
(test-equal "import-scheme-eval" 3 (eval '(+ 1 2)))

(import (scheme process-context))

(import (scheme read))

(import (scheme repl))

(import (scheme time))

(import (scheme case-lambda))
(test-equal "import-scheme-case-lambda" 7
  (let ((f (case-lambda
             (() 0)
             ((x) x)
             ((x y) (+ x y)))))
    (f 3 4)))

;; ============================================================================
;; Import modifiers
;; ============================================================================

(define-library (test mod)
  (export a b c)
  (begin
    (define a 1)
    (define b 2)
    (define c 3)))

(import (only (test mod) a c))
(test-equal "import-only-a" 1 a)
(test-equal "import-only-c" 3 c)

(define-library (test except-mod)
  (export aa bb cc)
  (begin
    (define aa 10)
    (define bb 20)
    (define cc 30)))

(import (except (test except-mod) bb))
(test-equal "import-except-aa" 10 aa)
(test-equal "import-except-cc" 30 cc)

(define-library (test prefix-mod)
  (export val)
  (begin
    (define val 99)))

(import (prefix (test prefix-mod) my-))
(test-equal "import-prefix" 99 my-val)

(define-library (test rename-mod)
  (export original)
  (begin
    (define original 77)))

(import (rename (test rename-mod) (original renamed)))
(test-equal "import-rename" 77 renamed)

;; ============================================================================
;; Macro export/import through the library system
;; ============================================================================

(define-library (test macros)
  (export my-if)
  (begin
    (define-syntax my-if
      (syntax-rules ()
        ((my-if test then else)
         (cond (test then) (#t else)))))))

(import (test macros))
(test-equal "library-exports-macros-true" 1 (my-if #t 1 2))
(test-equal "library-exports-macros-false" 2 (my-if #f 1 2))

(define-library (test mixed)
  (export my-when double)
  (begin
    (define (double x) (* x 2))
    (define-syntax my-when
      (syntax-rules ()
        ((my-when test body ...)
         (if test (begin body ...) (if #f #f)))))))

(import (test mixed))
(test-equal "library-exports-both-double" 10 (double 5))
(test-equal "library-exports-both-my-when" 42 (my-when #t 42))

;; ============================================================================
;; Lazy loading / multiple imports
;; ============================================================================

(import (scheme cxr) (scheme char))
(test-equal "multiple-imports-cadr" 2 (cadr '(1 2 3)))
(test-assert "multiple-imports-char-alphabetic" (char-alphabetic? #\z))

(define-library (test dep-base)
  (export base-val)
  (begin (define base-val 100)))

(define-library (test dep-user)
  (export derived-val)
  (import (test dep-base))
  (begin (define derived-val (+ base-val 1))))

(import (test dep-user))
(test-equal "library-with-dependency" 101 derived-val)

;; ============================================================================
;; environment form with libraries
;; ============================================================================

(import (scheme base))
(define base-env (environment '(scheme base)))
(test-equal "eval-in-library-environment" 5 (eval '(+ 2 3) base-env))

;; ============================================================================
;; (scheme r5rs) library
;; ============================================================================

(import (scheme r5rs))

(test-equal "r5rs-addition" 6 (+ 1 2 3))
(test-equal "r5rs-multiplication" 20 (* 4 5))
(test-equal "r5rs-abs" 7 (abs -7))
(test-equal "r5rs-max" 5 (max 3 5 1))
(test-equal "r5rs-min" 1 (min 3 5 1))
(test-equal "r5rs-gcd" 4 (gcd 12 8))
(test-equal "r5rs-quotient" 3 (quotient 10 3))
(test-equal "r5rs-modulo" 1 (modulo 10 3))

(test-equal "r5rs-car" 1 (car '(1 2 3)))
(test-equal "r5rs-cadr" 2 (cadr '(1 2 3)))
(test-equal "r5rs-length" 3 (length '(1 2 3)))
(test-assert "r5rs-list?" (list? '(1 2)))
(test-equal "r5rs-list-ref" 20 (list-ref '(10 20 30) 1))

(test-equal "r5rs-let" 3 (let ((x 3)) x))
(test-equal "r5rs-cond" 2 (cond (#f 1) (#t 2) (#t 3)))
(test-assert "r5rs-and-true" (and #t #t))
(test-assert "r5rs-or-true" (or #f #t))
(test-assert "r5rs-and-false" (not (and #t #f)))

(test-assert "r5rs-exact->inexact" (inexact? (exact->inexact 5)))
(test-assert "r5rs-inexact->exact" (exact? (inexact->exact 5.0)))

(test-assert "r5rs-string=?-equal" (string=? "hello" "hello"))
(test-assert "r5rs-string=?-not-equal" (not (string=? "hello" "world")))
(test-equal "r5rs-string-length" 5 (string-length "hello"))

(test-assert "r5rs-char-alphabetic" (char-alphabetic? #\a))
(test-assert "r5rs-char-not-alphabetic" (not (char-alphabetic? #\1)))
(test-assert "r5rs-char-ci=?" (char-ci=? #\A #\a))

(test-equal "r5rs-vector-ref" 20 (vector-ref (vector 10 20 30) 1))
(test-equal "r5rs-vector-length" 3 (vector-length (vector 1 2 3)))
(test-assert "r5rs-vector?" (vector? (vector 1)))

(test-error "r5rs-no-transcript-on" (lambda () (transcript-on "log.txt")))
(test-error "r5rs-no-transcript-off" (lambda () (transcript-off)))

;; ============================================================================
;; (scheme complex) library
;; ============================================================================

(import (scheme complex))
(test-equal "complex-real-part" 5 (real-part 5))
(test-equal "complex-imag-part" 0 (imag-part 5))

;; ============================================================================
;; (scheme base) new exports
;; ============================================================================

(import (scheme base))
(test-assert "base-bytevector?" (bytevector? (make-bytevector 3)))
(test-equal "base-bytevector-length" 5 (bytevector-length (make-bytevector 5)))
(test-equal "base-bytevector-u8-ref" 20 (bytevector-u8-ref (bytevector 10 20 30) 1))

(test-assert "base-write-string-procedure" (procedure? write-string))
(test-assert "base-flush-output-port-procedure" (procedure? flush-output-port))
(test-assert "base-call-with-port-procedure" (procedure? call-with-port))

(test-assert "base-features-list" (list? (features)))

(test-equal "base-apply" 6 (apply + '(1 2 3)))
(test-equal "base-call-with-values" 3
  (call-with-values (lambda () (values 1 2)) +))

(test-assert "base-read-error?-procedure" (procedure? read-error?))
(test-assert "base-file-error?-procedure" (procedure? file-error?))

(test-assert "base-utf8->string-procedure" (procedure? utf8->string))
(test-assert "base-string->utf8-procedure" (procedure? string->utf8))

;; ============================================================================
;; (scheme cxr) new 4-level exports
;; ============================================================================

(import (scheme cxr))
(test-equal "cxr-caaaar" 1 (caaaar '((((1 2) 3) 4) 5)))
(test-equal "cxr-caddar" 3 (caddar '((a b 3) d)))

;; ============================================================================
;; Test new cxr functions directly (no import needed)
;; ============================================================================

(test-equal "cxr-cdaddr" 3 (car (cdaddr '(a b (c 3) d))))
(test-equal "cxr-cddaar" 3 (car (cddaar '(((a b 3) c) d))))
(test-equal "cxr-cddadr" 3 (car (cddadr '(a (b c 3) d))))
(test-equal "cxr-cdddar" 3 (car (cdddar '((a b c 3) d))))

;; ============================================================================
;; (scheme file) new exports
;; ============================================================================

(import (scheme file))
(test-assert "file-open-input-file-procedure" (procedure? open-input-file))
(test-assert "file-open-output-file-procedure" (procedure? open-output-file))
(test-assert "file-call-with-input-file-procedure" (procedure? call-with-input-file))
(test-assert "file-call-with-output-file-procedure" (procedure? call-with-output-file))

;; ============================================================================
;; (scheme time) new exports
;; ============================================================================

(import (scheme time))
(test-assert "time-current-second-procedure" (procedure? current-second))
(test-assert "time-current-jiffy-procedure" (procedure? current-jiffy))
(test-assert "time-jiffies-per-second-procedure" (procedure? jiffies-per-second))

;; ============================================================================
;; Error cases
;; ============================================================================

(test-error "import-unknown-library-fails" (lambda () (import (nonexistent lib))))

;; ============================================================================
;; (scheme inexact) R7RS names: exact, inexact
;; ============================================================================

(test-assert "inexact-r7rs-exact-procedure" (procedure? exact))
(test-assert "inexact-r7rs-inexact-procedure" (procedure? inexact))
(test-equal "inexact-r7rs-exact" 3 (exact 3.0))

;; ============================================================================
;; (scheme base) char comparison exports
;; ============================================================================

(test-assert "base-char=?" (char=? #\a #\a))
(test-assert "base-char<?" (char<? #\a #\b))
(test-assert "base-char>?" (char>? #\b #\a))
(test-assert "base-char<=?" (char<=? #\a #\b))
(test-assert "base-char>=?" (char>=? #\b #\a))

;; ============================================================================
;; (scheme base) read-bytevector exports
;; ============================================================================

(test-assert "base-read-bytevector-procedure" (procedure? read-bytevector))
(test-assert "base-read-bytevector!-procedure" (procedure? read-bytevector!))

;; ============================================================================
;; import inside begin block must not halt execution
;; ============================================================================

(test-equal "import-does-not-halt-begin-block" 42
  (begin (import (scheme base)) 42))

(test-equal "import-then-define-in-begin" 99
  (begin (import (scheme write)) (define x2 99) x2))

(define-library (test begin-lib)
  (export my-val)
  (begin (define my-val 7)))

(test-equal "import-in-begin-with-user-library" 10
  (begin (import (test begin-lib)) (+ my-val 3)))

(test-end)
