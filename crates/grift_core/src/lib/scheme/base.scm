;;; (scheme base) — R7RS §6.1–6.10 core library
;;;
;;; Note: exports are split across multiple (export ...) declarations
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
    char=? char<? char>? char<=? char>=?
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
    input-port-open? output-port-open?
    ;; R7RS §6.2.6 numeric operations
    floor-quotient floor-remainder floor/
    truncate-quotient truncate-remainder truncate/
    numerator denominator rationalize
    exact-integer-sqrt
    exp log sin cos tan asin acos atan
    make-rectangular make-polar real-part imag-part magnitude angle)
  (export
    ;; Continuation/control flow (special forms)
    apply call-with-current-continuation call/cc
    values call-with-values dynamic-wind
    ;; Exception handling (special forms)
    with-exception-handler raise raise-continuable
    ;; Syntax (special forms)
    define-record-type include include-ci
    ;; I/O - text
    write-string flush-output-port
    open-input-file open-output-file
    read write write-shared write-simple
    ;; I/O - binary
    read-u8 peek-u8 u8-ready? write-u8 write-bytevector
    read-bytevector read-bytevector!
    ;; Bytevector operations
    bytevector? make-bytevector bytevector-length
    bytevector-u8-ref bytevector-u8-set!
    bytevector-copy bytevector-copy! bytevector-append
    bytevector
    ;; Bytevector I/O
    open-input-bytevector open-output-bytevector
    get-output-bytevector
    ;; Port management
    call-with-port
    ;; Encoding conversion
    utf8->string string->utf8
    ;; Error predicates
    read-error? file-error?
    ;; Vector-String conversion
    vector->string string->vector
    ;; Introspection
    features)
  (begin
    ;;; --------------------------------------------------------
    ;;; Numeric predicates and operations (R7RS §6.2–6.3)
    ;;; --------------------------------------------------------

    (define (not x) (if x #f #t))
    (define (square x) (* x x))
    (define (zero? x) (= x 0))
    (define (positive? x) (> x 0))
    (define (negative? x) (< x 0))
    (define (even? x) (= (remainder x 2) 0))
    (define (odd? x) (not (= (remainder x 2) 0)))
    (define (abs x) (if (negative? x) (- x) x))
    (define (sign n) (if (positive? n) 1 (if (negative? n) -1 0)))

    (define (boolean=? b1 b2 . rest)
      (define (check val remaining)
        (if (null? remaining) #t
            (if (eq? val (car remaining))
                (check val (cdr remaining))
                #f)))
      (if (eq? b1 b2) (check b1 rest) #f))

    (define (symbol=? s1 s2 . rest)
      (define (check val remaining)
        (if (null? remaining) #t
            (if (eq? val (car remaining))
                (check val (cdr remaining))
                #f)))
      (if (eq? s1 s2) (check s1 rest) #f))

    (define (exact-integer? x) (and (integer? x) (exact? x)))
    (define (real? x) (number? x))
    (define (rational? x)
      (and (number? x)
           (if (inexact? x) (finite? x) #t)))
    (define (complex? x) (number? x))

    ;;; --------------------------------------------------------
    ;;; List fundamentals (R7RS §6.4)
    ;;; --------------------------------------------------------

    (define (list? obj) (if (null? obj) #t (if (pair? obj) (list? (cdr obj)) #f)))

    (define (length lst)
      (define (length-iter lst acc)
        (if (null? lst) acc (length-iter (cdr lst) (+ acc 1))))
      (length-iter lst 0))

    (define (reverse lst) (fold (lambda (acc x) (cons x acc)) '() lst))

    (define (append-two a b)
      (define (rev-helper lst acc)
        (if (null? lst) acc (rev-helper (cdr lst) (cons (car lst) acc))))
      (define (append-iter lst acc)
        (if (null? lst) acc (append-iter (cdr lst) (cons (car lst) acc))))
      (append-iter (rev-helper a '()) b))

    (define (list-copy lst) (if (null? lst) '() (cons (car lst) (list-copy (cdr lst)))))

    ;;; --------------------------------------------------------
    ;;; Higher-order functions (R7RS §6.4, §6.10)
    ;;; --------------------------------------------------------

    (define (fold f acc lst)
      (if (null? lst)
          acc
          (fold f (f acc (car lst)) (cdr lst))))

    (define (fold-left f acc lst)
      (if (null? lst)
          acc
          (fold-left f (f acc (car lst)) (cdr lst))))

    (define (fold-right f init lst)
      (if (null? lst) init (f (car lst) (fold-right f init (cdr lst)))))

    (define (any-null? lists)
      (if (null? lists) #f
          (if (null? (car lists)) #t
              (any-null? (cdr lists)))))
    (define (map-car lists)
      (if (null? lists) '()
          (cons (car (car lists)) (map-car (cdr lists)))))
    (define (map-cdr lists)
      (if (null? lists) '()
          (cons (cdr (car lists)) (map-cdr (cdr lists)))))

    (define (map f lst . rest)
      (if (null? rest)
          (let map-one ((lst lst) (acc '()))
            (if (null? lst)
                (reverse acc)
                (map-one (cdr lst) (cons (f (car lst)) acc))))
          (let map-multi ((lists (cons lst rest)) (acc '()))
            (if (any-null? lists)
                (reverse acc)
                (map-multi (map-cdr lists)
                           (cons (apply f (map-car lists)) acc))))))

    (define (for-each f lst . rest)
      (if (null? rest)
          (if (null? lst) (if #f #f) (begin (f (car lst)) (for-each f (cdr lst))))
          (let for-each-multi ((lists (cons lst rest)))
            (if (any-null? lists)
                (if #f #f)
                (begin
                  (apply f (map-car lists))
                  (for-each-multi (map-cdr lists)))))))

    (define (filter pred lst)
      (define (filter-iter lst acc)
        (if (null? lst)
            (reverse acc)
            (if (pred (car lst))
                (filter-iter (cdr lst) (cons (car lst) acc))
                (filter-iter (cdr lst) acc))))
      (filter-iter lst '()))

    ;;; --------------------------------------------------------
    ;;; Numeric operations (R7RS §6.2.6)
    ;;; --------------------------------------------------------

    (define (gcd . args)
      (define (gcd2 a b)
        (if (= b 0) a (gcd2 b (remainder a b))))
      (if (null? args) 0
          (fold (lambda (acc x) (gcd2 (abs acc) (abs x)))
                (car args)
                (cdr args))))

    (define (lcm . args)
      (define (lcm2 a b)
        (if (or (= a 0) (= b 0)) 0
            (abs (/ (* a b) (gcd a b)))))
      (if (null? args) 1
          (fold lcm2 (car args) (cdr args))))

    (define (max x . rest)
      (define (max-iter best has-inexact remaining)
        (if (null? remaining)
            (if has-inexact (inexact best) best)
            (let ((y (car remaining)))
              (max-iter (if (> y best) y best)
                        (or has-inexact (inexact? y))
                        (cdr remaining)))))
      (max-iter x (inexact? x) rest))

    (define (min x . rest)
      (define (min-iter best has-inexact remaining)
        (if (null? remaining)
            (if has-inexact (inexact best) best)
            (let ((y (car remaining)))
              (min-iter (if (< y best) y best)
                        (or has-inexact (inexact? y))
                        (cdr remaining)))))
      (min-iter x (inexact? x) rest))

    ;;; --------------------------------------------------------
    ;;; Member and association list functions (R7RS §6.4)
    ;;; --------------------------------------------------------

    (define (mem-helper pred obj lst)
      (if (null? lst) #f (if (pred obj (car lst)) lst (mem-helper pred obj (cdr lst)))))

    (define (assoc-helper pred key alist)
      (if (null? alist) #f (if (pred key (car (car alist))) (car alist) (assoc-helper pred key (cdr alist)))))

    (define (memq obj lst) (mem-helper eq? obj lst))
    (define (memv obj lst) (mem-helper eqv? obj lst))
    (define (member x lst . rest)
      (mem-helper (if (null? rest) equal? (car rest)) x lst))

    (define (assq key alist) (assoc-helper eq? key alist))
    (define (assv key alist) (assoc-helper eqv? key alist))
    (define (assoc key alist . rest)
      (assoc-helper (if (null? rest) equal? (car rest)) key alist))

    ;;; --------------------------------------------------------
    ;;; List accessors and mutators (R7RS §6.4)
    ;;; --------------------------------------------------------

    (define (list-tail lst k)
      (if (not (and (integer? k) (exact? k) (>= k 0)))
          (error "list-tail: invalid index" k)
          (list-tail-iter lst k)))
    (define (list-tail-iter lst k)
      (if (= k 0) lst
          (if (null? lst)
              (error "list-tail: index out of range" k)
              (list-tail-iter (cdr lst) (- k 1)))))

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

    (define (list-set! lst k obj)
      (if (not (and (integer? k) (exact? k) (>= k 0)))
          (error "list-set!: invalid index" k)
          (set-car! (list-tail lst k) obj)))

    ;;; --------------------------------------------------------
    ;;; String operations (R7RS §6.7)
    ;;; --------------------------------------------------------

    (define (string-for-each proc s)
      (for-each proc (string->list s)))

    (define (string-map proc s)
      (list->string (map proc (string->list s))))

    ;;; --------------------------------------------------------
    ;;; Promise and parameter functions (R7RS §4.2.5–4.2.6)
    ;;; --------------------------------------------------------

    (define (promise? obj)
      (procedure? obj))

    (define (make-promise obj)
      (if (promise? obj)
          obj
          (lambda () obj)))

    (define (make-parameter init)
      (let ((value init))
        (lambda args
          (if (null? args)
              value
              (set! value (car args))))))

    ;;; --------------------------------------------------------
    ;;; Introspection (R7RS §6.14)
    ;;; --------------------------------------------------------

    (define (features)
      '(r7rs grift exact-closed))))
