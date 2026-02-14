;;; (scheme r5rs) — R7RS Appendix A: R5RS compatibility library
;;;
;;; Exports all R5RS standard bindings except transcript-on and
;;; transcript-off.  Uses the R5RS names exact->inexact and
;;; inexact->exact (not the R7RS names inexact and exact).
;;;
;;; Most definitions are imported from (scheme base) to avoid duplication.
;;; Builtins are inherited from the global environment.
;;;
;;; Note: exports are split across multiple (export ...) declarations
;;; to stay within the parser's per-list element limit.
(define-library (scheme r5rs)
  (import (scheme base))
  (export
    ;; Syntax / macros
    syntax-rules
    let let* letrec
    and or cond case do
    delay force
    ;; Numeric helpers
    not zero? positive? negative? even? odd?
    abs gcd lcm max min
    complex? real? rational?
    ;; List helpers
    append map for-each
    length reverse
    list-tail list-ref list?
    memq memv member
    assq assv assoc)
  (export
    ;; Core pair / list builtins
    car cdr cons list pair? null?
    ;; Type predicates
    number? boolean? procedure? symbol? integer?
    exact? inexact?
    ;; Equivalence
    eq? eqv? equal?
    ;; Arithmetic
    + - * / modulo remainder quotient expt
    ;; R5RS conversion names
    exact->inexact inexact->exact
    ;; Rounding
    floor ceiling truncate round
    ;; Comparison
    < > <= >= =
    ;; Rational
    numerator denominator rationalize
    ;; Transcendental / inexact
    exp log sin cos tan asin acos atan sqrt
    ;; Complex
    make-rectangular make-polar
    real-part imag-part magnitude angle)
  (export
    ;; Mutation
    set-car! set-cdr!
    ;; Vectors
    vector? make-vector vector vector-length
    vector-ref vector-set! vector->list list->vector
    vector-fill!
    ;; Characters (builtins)
    char? char=? char<? char>? char<=? char>=?
    char->integer integer->char
    char-upcase char-downcase
    ;; Characters (builtins via icu4x)
    char-alphabetic? char-numeric? char-whitespace?
    char-upper-case? char-lower-case?
    char-ci=? char-ci<? char-ci>? char-ci<=? char-ci>=?)
  (export
    ;; Strings
    string? make-string string string-length
    string-ref string-set!
    string=? string<? string>? string<=? string>=?
    string-ci=? string-ci<? string-ci>? string-ci<=? string-ci>=?
    string-append string->list list->string
    substring string-copy string-fill!
    ;; Symbol / number conversion
    symbol->string string->symbol
    number->string string->number
    ;; I/O
    input-port? output-port?
    current-input-port current-output-port
    open-input-file open-output-file
    close-input-port close-output-port
    call-with-input-file call-with-output-file
    with-input-from-file with-output-to-file
    read read-char peek-char char-ready?
    write write-char
    eof-object?
    display newline
    ;; cxr compositions (available subset)
    caar cadr cdar cddr
    ;; Miscellaneous
    load interaction-environment))
