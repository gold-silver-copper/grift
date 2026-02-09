;;; (scheme base) — R7RS §6.1–6.10 core library
;;;
;;; Note: exports are split across two (export ...) declarations
;;; to stay within the parser's per-list element limit.
(define-library (scheme base)
  (export
    ;; Syntax (macros)
    syntax-rules define-syntax-rule
    let let* letrec letrec*
    and or when unless
    cond case do
    append
    let-values let*-values define-values
    case-lambda
    guard parameterize
    delay force delay-force
    cond-expand
    ;; Numeric predicates and operations
    not zero? positive? negative? even? odd?
    abs square sign
    gcd lcm max min
    exact-integer? real? rational? complex?
    boolean=? symbol=?
    ;; List operations
    map filter fold fold-left fold-right
    for-each length reverse
    list-tail list-ref list? list-copy
    memq memv member
    assq assv assoc
    make-list list-set!
    ;; String operations
    string-map string-for-each
    ;; Promise
    make-promise promise? make-parameter
    ;; Builtins (part 1)
    car cdr cons list pair? null?
    number? boolean? procedure? symbol?
    eq? eqv? equal?
    + - * / modulo remainder quotient expt
    integer? exact? inexact?
    exact inexact exact->inexact inexact->exact
    floor ceiling truncate round
    < > <= >= =
    display newline)
  (export
    ;; Builtins (part 2)
    error error-object? error-object-message
    error-object-irritants error-object-type
    set-car! set-cdr!
    vector? make-vector vector vector-length
    vector-ref vector-set! vector->list list->vector
    vector-fill! vector-copy vector-copy! vector-append
    vector-map vector-for-each
    char? char->integer integer->char
    string? make-string string string-length
    string-ref string-set!
    string=? string<? string>? string<=? string>=?
    string-append string->list list->string
    substring string-copy string-copy! string-fill!
    symbol->string string->symbol
    number->string string->number
    port? input-port? output-port?
    current-input-port current-output-port current-error-port
    close-port close-input-port close-output-port
    read-char write-char peek-char char-ready?
    open-input-string open-output-string get-output-string
    read-line read-string
    eof-object eof-object?
    textual-port? binary-port?
    input-port-open? output-port-open?))

