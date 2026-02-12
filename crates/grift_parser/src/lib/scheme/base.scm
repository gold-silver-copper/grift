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
  (export
    ;; Prelude-only extensions
    nth take drop zip range
    compose identity constantly flip curry
    cube sum product average
    last last-pair
    reduce any every find filter-map
    partition remove delete
    boolean-eq member-equal assoc-equal
    iota1 iota2 iota3
    list-tabulate
    first second third fourth fifth
    sixth seventh eighth ninth tenth
    take-right drop-right split-at
    concatenate flatten count
    string-null? string-reverse string-contains
    string-join string-split string-trim
    caar cadr caddr cddr cdddr cadddr cddddr
    cdaddr cddaar cddadr cdddar
    append-two)
  (begin

    ;;; ========================================================
    ;;; FUNCTION DEFINITIONS
    ;;; ========================================================
    ;;; Note: Macro definitions (syntax-rules, let, let*, letrec,
    ;;; letrec*, and, or, when, unless, cond, case, do, append,
    ;;; %qq-expand, delay, let-values, let*-values, define-values,
    ;;; force, identifier-syntax, case-lambda, cond-expand,
    ;;; delay-force, with-syntax, guard, parameterize, and all
    ;;; internal % helpers) are provided by the prelude and
    ;;; re-exported by this library.
    ;;; ========================================================

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
      (fold f acc lst))

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
      (if (= k 0) lst (list-tail (cdr lst) (- k 1))))

    (define (list-ref lst k)
      (if (= k 0) (car lst) (list-ref (cdr lst) (- k 1))))

    (define (make-list k fill)
      (define (make-list-loop k acc)
        (if (<= k 0) acc (make-list-loop (- k 1) (cons fill acc))))
      (make-list-loop k '()))

    (define (list-set! lst k obj)
      (set-car! (list-tail lst k) obj))

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
      '(r7rs grift exact-closed))

    ;;; --------------------------------------------------------
    ;;; Prelude extensions: c..r accessors
    ;;; --------------------------------------------------------

    (define (caar lst) (car (car lst)))
    (define (cadr lst) (car (cdr lst)))
    (define (caddr lst) (car (cdr (cdr lst))))
    (define (cddr lst) (cdr (cdr lst)))
    (define (cdddr lst) (cdr (cdr (cdr lst))))
    (define (cadddr lst) (car (cdr (cdr (cdr lst)))))
    (define (cddddr lst) (cdr (cdr (cdr (cdr lst)))))
    (define (cdaddr lst) (cdr (car (cdr (cdr lst)))))
    (define (cddaar lst) (cdr (cdr (car (car lst)))))
    (define (cddadr lst) (cdr (cdr (car (cdr lst)))))
    (define (cdddar lst) (cdr (cdr (cdr (car lst)))))

    ;;; --------------------------------------------------------
    ;;; Prelude extensions: list functions
    ;;; --------------------------------------------------------

    (define (nth n lst)
      (if (= n 0) (car lst) (nth (- n 1) (cdr lst))))

    (define (take n lst)
      (define (take-iter n lst acc)
        (if (= n 0) (reverse acc)
            (if (null? lst) (reverse acc)
                (take-iter (- n 1) (cdr lst) (cons (car lst) acc)))))
      (take-iter n lst '()))

    (define (drop n lst)
      (if (= n 0) lst
          (if (null? lst) '()
              (drop (- n 1) (cdr lst)))))

    (define (zip a b)
      (if (null? a) '()
          (if (null? b) '()
              (cons (cons (car a) (car b))
                    (zip (cdr a) (cdr b))))))

    (define (range start end)
      (define (range-iter n acc)
        (if (< n start)
            acc
            (range-iter (- n 1) (cons n acc))))
      (range-iter (- end 1) '()))

    (define (compose f g) (lambda (x) (f (g x))))
    (define (identity x) x)
    (define (constantly x) (lambda (y) x))
    (define (flip f) (lambda (a b) (f b a)))
    (define (curry f x) (lambda (y) (f x y)))

    (define (cube x) (* x x x))
    (define (sum lst) (fold + 0 lst))
    (define (product lst) (fold * 1 lst))
    (define (average lst) (/ (sum lst) (length lst)))

    (define (last-pair lst)
      (if (null? (cdr lst)) lst (last-pair (cdr lst))))

    (define (last lst)
      (car (last-pair lst)))

    (define (reduce f init lst)
      (if (null? lst) init
          (f (car lst) (reduce f init (cdr lst)))))

    (define (any pred lst)
      (if (null? lst) #f
          (if (pred (car lst)) #t
              (any pred (cdr lst)))))
    (define (every pred lst)
      (if (null? lst) #t
          (if (pred (car lst))
              (every pred (cdr lst))
              #f)))
    (define (find pred lst)
      (if (null? lst) #f
          (if (pred (car lst))
              (car lst)
              (find pred (cdr lst)))))

    (define (filter-map f lst)
      (define (filter-map-iter lst acc)
        (if (null? lst)
            (reverse acc)
            (let ((result (f (car lst))))
              (if result
                  (filter-map-iter (cdr lst) (cons result acc))
                  (filter-map-iter (cdr lst) acc)))))
      (filter-map-iter lst '()))

    (define (partition pred lst)
      (define (partition-helper pred lst matches non-matches)
        (if (null? lst)
            (cons (reverse matches) (reverse non-matches))
            (if (pred (car lst))
                (partition-helper pred (cdr lst) (cons (car lst) matches) non-matches)
                (partition-helper pred (cdr lst) matches (cons (car lst) non-matches)))))
      (partition-helper pred lst '() '()))

    (define (remove pred lst) (filter (lambda (x) (not (pred x))) lst))
    (define (delete x lst) (filter (lambda (y) (not (equal? x y))) lst))

    (define (boolean-eq b1 b2) (or (and b1 b2) (and (not b1) (not b2))))
    (define (member-equal obj lst) (mem-helper equal? obj lst))
    (define (assoc-equal key alist) (assoc-helper equal? key alist))

    (define (iota-helper count start step acc)
      (if (<= count 0)
          (reverse acc)
          (iota-helper (- count 1) (+ start step) step (cons start acc))))

    (define (iota1 count) (iota-helper count 0 1 '()))
    (define (iota2 count start) (iota-helper count start 1 '()))
    (define (iota3 count start step) (iota-helper count start step '()))

    (define (list-tabulate n proc)
      (define (list-tabulate-helper n i proc acc)
        (if (>= i n)
            (reverse acc)
            (list-tabulate-helper n (+ i 1) proc (cons (proc i) acc))))
      (list-tabulate-helper n 0 proc '()))

    (define (first lst) (car lst))
    (define (second lst) (cadr lst))
    (define (third lst) (caddr lst))
    (define (fourth lst) (cadddr lst))
    (define (fifth lst) (car (cddddr lst)))
    (define (sixth lst) (cadr (cddddr lst)))
    (define (seventh lst) (caddr (cddddr lst)))
    (define (eighth lst) (cadddr (cddddr lst)))
    (define (ninth lst) (car (cddddr (cddddr lst))))
    (define (tenth lst) (cadr (cddddr (cddddr lst))))

    (define (take-right lst k)
      (define (advance p count)
        (if (= count 0)
            p
            (if (null? p)
                '()
                (advance (cdr p) (- count 1)))))
      (define (walk lead lag)
        (if (null? lead)
            lag
            (walk (cdr lead) (cdr lag))))
      (let ((lead (advance lst k)))
        (if (null? lead)
            lst
            (walk lead lst))))

    (define (drop-right lst k)
      (define (advance p count)
        (if (= count 0)
            p
            (if (null? p)
                '()
                (advance (cdr p) (- count 1)))))
      (define (walk lead lag acc)
        (if (null? lead)
            (reverse acc)
            (walk (cdr lead) (cdr lag) (cons (car lag) acc))))
      (let ((lead (advance lst k)))
        (if (null? lead)
            '()
            (walk lead lst '()))))

    (define (split-at lst k)
      (cons (take k lst) (drop k lst)))

    (define (concatenate lsts)
      (fold-right append-two '() lsts))

    (define (flatten lst)
      (define (flatten-iter lst acc)
        (cond
          ((null? lst) acc)
          ((not (pair? lst)) (cons lst acc))
          (else (flatten-iter (car lst) (flatten-iter (cdr lst) acc)))))
      (flatten-iter lst '()))

    (define (count pred lst)
      (fold (lambda (acc x) (if (pred x) (+ acc 1) acc)) 0 lst))

    ;;; --------------------------------------------------------
    ;;; Prelude extensions: string functions
    ;;; --------------------------------------------------------

    (define (string-null? s)
      (= (string-length s) 0))

    (define (string-reverse s)
      (list->string (reverse (string->list s))))

    (define (string-prefix? s1 s2 start)
      (define (string-prefix-helper s1 s2 i j len2)
        (if (>= j len2)
            #t
            (if (char=? (string-ref s1 i) (string-ref s2 j))
                (string-prefix-helper s1 s2 (+ i 1) (+ j 1) len2)
                #f)))
      (string-prefix-helper s1 s2 start 0 (string-length s2)))

    (define (string-contains s1 s2)
      (let ((len1 (string-length s1))
            (len2 (string-length s2)))
        (if (> len2 len1)
            #f
            (let loop ((i 0))
              (if (> (+ i len2) len1)
                  #f
                  (if (string-prefix? s1 s2 i)
                      i
                      (loop (+ i 1))))))))

    (define (string-join lst sep)
      (if (null? lst)
          ""
          (fold (lambda (acc s) (string-append acc sep s))
                (car lst)
                (cdr lst))))

    (define (string-split s sep)
      (define (string-split-helper chars sep current result)
        (cond
          ((null? chars)
           (reverse (cons (list->string (reverse current)) result)))
          ((char=? (car chars) sep)
           (string-split-helper (cdr chars) sep '()
                                (cons (list->string (reverse current)) result)))
          (else
           (string-split-helper (cdr chars) sep (cons (car chars) current) result))))
      (string-split-helper (string->list s) sep '() '()))

    (define (string-trim s)
      (define (drop-while-ws lst)
        (cond
          ((null? lst) '())
          ((char-whitespace? (car lst)) (drop-while-ws (cdr lst)))
          (else lst)))
      (list->string (reverse (drop-while-ws (reverse (drop-while-ws (string->list s)))))))))
