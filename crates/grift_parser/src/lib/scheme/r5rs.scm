;;; (scheme r5rs) — R7RS Appendix A: R5RS compatibility library
;;;
;;; Exports all R5RS standard bindings except transcript-on and
;;; transcript-off.  Uses the R5RS names exact->inexact and
;;; inexact->exact (not the R7RS names inexact and exact).
;;;
;;; Identifiers whose underlying features are not provided by this
;;; implementation (e.g. null-environment, scheme-report-environment)
;;; are omitted.
;;;
;;; Note: exports are split across multiple (export ...) declarations
;;; to stay within the parser's per-list element limit.
(define-library (scheme r5rs)
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
    load interaction-environment)
  (begin
    ;;; Scheme-defined R5RS procedures.
    ;;; Builtins and macros are inherited from the global environment.

    ;; Numeric predicates
    (define (not x) (if x #f #t))
    (define (zero? x) (= x 0))
    (define (positive? x) (> x 0))
    (define (negative? x) (< x 0))
    (define (even? x) (= (remainder x 2) 0))
    (define (odd? x) (not (= (remainder x 2) 0)))
    (define (abs x) (if (negative? x) (- x) x))
    (define (complex? x) (number? x))
    (define (real? x) (number? x))
    (define (rational? x)
      (and (number? x)
           (if (inexact? x) (finite? x) #t)))

    ;; List operations
    (define (list? obj) (if (null? obj) #t (if (pair? obj) (list? (cdr obj)) #f)))

    (define (length lst)
      (define (length-iter lst acc)
        (if (null? lst) acc (length-iter (cdr lst) (+ acc 1))))
      (length-iter lst 0))

    (define (fold f acc lst)
      (if (null? lst)
          acc
          (fold f (f acc (car lst)) (cdr lst))))

    (define (reverse lst) (fold (lambda (acc x) (cons x acc)) '() lst))

    (define (append-two a b)
      (define (rev-helper lst acc)
        (if (null? lst) acc (rev-helper (cdr lst) (cons (car lst) acc))))
      (define (append-iter lst acc)
        (if (null? lst) acc (append-iter (cdr lst) (cons (car lst) acc))))
      (append-iter (rev-helper a '()) b))

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

    ;; Member/assoc helpers and functions
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

    ;; Numeric operations
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

    ;; CXR compositions
    (define (caar lst) (car (car lst)))
    (define (cadr lst) (car (cdr lst)))
    (define (cdar lst) (cdr (car lst)))
    (define (cddr lst) (cdr (cdr lst)))))
