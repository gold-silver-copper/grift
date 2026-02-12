;;; R5RS Tests adapted from chibi-scheme
;;; (https://github.com/ashinn/chibi-scheme/blob/master/tests/r5rs-tests.scm)
;;;
;;; These tests verify core R5RS Scheme compliance.
;;; Original tests by Alex Shinn, adapted for Grift.

(test-begin "r5rs-chibi")

;; ═══════════════════════════════════════════════════════════════════════════
;; LAMBDA TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "lambda-basic" 8 ((lambda (x) (+ x x)) 4))

(test-equal "lambda-rest-args-car" 3 (car ((lambda x x) 3 4 5 6)))
(test-equal "lambda-rest-args-length" 4 (length ((lambda x x) 3 4 5 6)))

(test-equal "lambda-rest-with-required-car" 5
  (car ((lambda (x y . z) z) 3 4 5 6)))
(test-equal "lambda-rest-with-required-length" 2
  (length ((lambda (x y . z) z) 3 4 5 6)))

;; ═══════════════════════════════════════════════════════════════════════════
;; IF TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "if-basic-yes" 'yes (if (> 3 2) 'yes 'no))
(test-equal "if-basic-no" 'no (if (> 2 3) 'yes 'no))
(test-equal "if-basic-arithmetic" 1 (if (> 3 2) (- 3 2) (+ 3 2)))

;; ═══════════════════════════════════════════════════════════════════════════
;; COND TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "cond-basic-greater" 'greater
  (cond ((> 3 2) 'greater) ((< 3 2) 'less)))

(test-equal "cond-basic-equal" 'equal
  (cond ((> 3 3) 'greater) ((< 3 3) 'less) (else 'equal)))

;; ═══════════════════════════════════════════════════════════════════════════
;; CASE TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "case-basic" 'composite
  (case (* 2 3)
    ((2 3 5 7) 'prime)
    ((1 4 6 8 9) 'composite)))

(test-equal "case-else" 'consonant
  (case (car '(c d))
    ((a e i o u) 'vowel)
    ((w y) 'semivowel)
    (else 'consonant)))

;; ═══════════════════════════════════════════════════════════════════════════
;; AND/OR TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "and-basic-true" (and (= 2 2) (> 2 1)))
(test-assert "and-basic-false" (not (and (= 2 2) (< 2 1))))
(test-assert "and-empty" (and))

(test-equal "and-returns-last-value" '(f g) (and 1 2 'c '(f g)))

(test-assert "or-basic-both-true" (or (= 2 2) (> 2 1)))
(test-assert "or-basic-first-true" (or (= 2 2) (< 2 1)))

(test-assert "or-short-circuit"
  (pair? (or (memq 'b '(a b c)) 'never-reached)))

;; ═══════════════════════════════════════════════════════════════════════════
;; LET TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "let-basic" 6 (let ((x 2) (y 3)) (* x y)))

(test-equal "let-nested-parallel-binding" 35
  (let ((x 2) (y 3))
    (let ((x 7) (z (+ x y)))
      (* z x))))

(test-equal "let-star" 70
  (let ((x 2) (y 3))
    (let* ((x 7) (z (+ x y)))
      (* z x))))

(test-equal "let-define-in-body" -2
  (let ()
    (define x 2)
    (define f (lambda () (- x)))
    (f)))

;; ═══════════════════════════════════════════════════════════════════════════
;; DO LOOP TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "do-vector" '#(0 1 2 3 4)
  (do ((vec (make-vector 5))
       (i 0 (+ i 1)))
      ((= i 5) vec)
    (vector-set! vec i i)))

(test-equal "do-sum" 25
  (let ((x '(1 3 5 7 9)))
    (do ((x x (cdr x))
         (sum 0 (+ sum (car x))))
        ((null? x) sum))))

(test-equal "named-let-loop" '((6 1 3) (-5 -2))
  (let loop ((numbers '(3 -2 1 6 -5)) (nonneg '()) (neg '()))
    (cond
      ((null? numbers) (list nonneg neg))
      ((>= (car numbers) 0)
       (loop (cdr numbers) (cons (car numbers) nonneg) neg))
      ((< (car numbers) 0)
       (loop (cdr numbers) nonneg (cons (car numbers) neg))))))

;; ═══════════════════════════════════════════════════════════════════════════
;; QUASIQUOTE TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "quasiquote-basic" '(list 3 4) `(list ,(+ 1 2) 4))

(test-equal "quasiquote-with-name" '(list a (quote a))
  (let ((name 'a)) `(list ,name ',name)))

(test-equal "quasiquote-unquote-splicing" '(a 3 4 5 6 b)
  `(a ,(+ 1 2) ,@(map abs '(4 -5 6)) b))

;; ═══════════════════════════════════════════════════════════════════════════
;; EQUIVALENCE PREDICATES
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "eqv-symbol-same" (eqv? 'a 'a))
(test-assert "eqv-symbol-different" (not (eqv? 'a 'b)))
(test-assert "eqv-empty-list" (eqv? '() '()))
(test-assert "eqv-cons-different" (not (eqv? (cons 1 2) (cons 1 2))))
(test-assert "eqv-lambda-same" (let ((p (lambda (x) x))) (eqv? p p)))

(test-assert "eq-symbol-same" (eq? 'a 'a))
(test-assert "eq-list-different" (not (eq? (list 'a) (list 'a))))
(test-assert "eq-empty-list" (eq? '() '()))
(test-assert "eq-car-same" (eq? car car))
(test-assert "eq-let-same" (let ((x '(a))) (eq? x x)))
(test-assert "eq-lambda-same" (let ((p (lambda (x) x))) (eq? p p)))

(test-assert "equal-symbol" (equal? 'a 'a))
(test-assert "equal-list-simple" (equal? '(a) '(a)))
(test-assert "equal-list-nested" (equal? '(a (b) c) '(a (b) c)))
(test-assert "string-equal-same" (string=? "abc" "abc"))
(test-assert "string-equal-different-length" (not (string=? "abc" "abcd")))
(test-assert "string-equal-different-char" (not (string=? "a" "b")))
(test-assert "equal-number" (equal? 2 2))

;; ═══════════════════════════════════════════════════════════════════════════
;; ARITHMETIC TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "max" 4 (max 3 4))

(test-equal "plus-two" 7 (+ 3 4))
(test-equal "plus-one" 3 (+ 3))
(test-equal "plus-zero" 0 (+))

(test-equal "multiply-one" 4 (* 4))
(test-equal "multiply-zero" 1 (*))

(test-equal "minus-two" -1 (- 3 4))
(test-equal "minus-three" -6 (- 3 4 5))
(test-equal "minus-negate" -3 (- 3))

(test-equal "abs" 7 (abs -7))

(test-equal "modulo-positive" 1 (modulo 13 4))
(test-equal "remainder-positive" 1 (remainder 13 4))
(test-equal "modulo-neg-dividend" 3 (modulo -13 4))
(test-equal "remainder-neg-dividend" -1 (remainder -13 4))
(test-equal "modulo-neg-divisor" -3 (modulo 13 -4))
(test-equal "remainder-neg-divisor" 1 (remainder 13 -4))
(test-equal "modulo-both-neg" -1 (modulo -13 -4))
(test-equal "remainder-both-neg" -1 (remainder -13 -4))

(test-equal "gcd" 4 (gcd 32 -36))
(test-equal "lcm" 288 (lcm 32 -36))

;; ═══════════════════════════════════════════════════════════════════════════
;; NOT AND BOOLEAN TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "not-number" (not (not 3)))
(test-assert "not-list" (not (not (list 3))))
(test-assert "not-empty-list" (not (not '())))
(test-assert "not-list-empty" (not (not (list))))

(test-assert "boolean-zero" (not (boolean? 0)))
(test-assert "boolean-empty-list" (not (boolean? '())))
(test-assert "boolean-true" (boolean? #t))
(test-assert "boolean-false" (boolean? #f))

;; ═══════════════════════════════════════════════════════════════════════════
;; PAIR AND LIST TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "pair-dotted" (pair? '(a . b)))
(test-assert "pair-list" (pair? '(a b c)))

(test-equal "cons-empty" '(a) (cons 'a '()))
(test-equal "cons-list" '((a) b c d) (cons '(a) '(b c d)))
(test-equal "cons-string" '("a" b c) (cons "a" '(b c)))

(test-equal "car" 'a (car '(a b c)))
(test-equal "cdr" '(b c d) (cdr '((a) b c d)))
(test-equal "car-dotted" 1 (car '(1 . 2)))
(test-equal "cdr-dotted" 2 (cdr '(1 . 2)))

(test-assert "list-predicate-true" (list? '(a b c)))
(test-assert "list-predicate-empty" (list? '()))
(test-assert "list-predicate-dotted" (not (list? '(a . b))))

(test-equal "list-elements" '(a 7 c) (list 'a (+ 3 4) 'c))
(test-equal "list-empty" '() (list))

(test-equal "length-three" 3 (length '(a b c)))
(test-equal "length-nested" 3 (length '(a (b) (c d e))))
(test-equal "length-empty" 0 (length '()))

(test-equal "append-two" '(x y) (append '(x) '(y)))
(test-equal "append-unequal" '(a b c d) (append '(a) '(b c d)))
(test-equal "append-nested" '(a (b) (c)) (append '(a (b)) '((c))))

(test-equal "reverse" '(c b a) (reverse '(a b c)))
(test-equal "reverse-nested" '((e (f)) d (b c) a) (reverse '(a (b c) d (e (f)))))

(test-equal "list-ref" 'c (list-ref '(a b c d) 2))

(test-equal "memq-first" '(a b c) (memq 'a '(a b c)))
(test-equal "memq-middle" '(b c) (memq 'b '(a b c)))
(test-assert "memq-missing" (not (memq 'a '(b c d))))
(test-assert "memq-list-not-found" (not (memq (list 'a) '(b (a) c))))
(test-equal "member-list" '((a) c) (member (list 'a) '(b (a) c)))
(test-equal "memv" '(101 102) (memv 101 '(100 101 102)))

(test-assert "assq-list-not-found" (not (assq (list 'a) '(((a)) ((b)) ((c))))))
(test-equal "assoc-list" '((a)) (assoc (list 'a) '(((a)) ((b)) ((c)))))
(test-equal "assv" '(5 7) (assv 5 '((2 3) (5 7) (11 13))))

;; ═══════════════════════════════════════════════════════════════════════════
;; SYMBOL TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "symbol-foo" (symbol? 'foo))
(test-assert "symbol-car-list" (symbol? (car '(a b))))
(test-assert "symbol-string" (not (symbol? "bar")))
(test-assert "symbol-nil" (symbol? 'nil))
(test-assert "symbol-empty-list" (not (symbol? '())))

;; Note: symbol->string is not implemented in grift.
;; symbol-string-conversion test skipped.

;; ═══════════════════════════════════════════════════════════════════════════
;; STRING TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "string-predicate-true" (string? "a"))
(test-assert "string-predicate-false" (not (string? 'a)))

(test-equal "string-length-empty" 0 (string-length ""))
(test-equal "string-length-abc" 3 (string-length "abc"))

(test-equal "string-ref-first" #\a (string-ref "abc" 0))
(test-equal "string-ref-last" #\c (string-ref "abc" 2))

(test-assert "string-less-than" (string<? "a" "aa"))
(test-assert "string-not-less-than" (not (string<? "aa" "a")))
(test-assert "string-not-less-than-equal" (not (string<? "a" "a")))
(test-assert "string-less-equal-less" (string<=? "a" "aa"))
(test-assert "string-less-equal-equal" (string<=? "a" "a"))

(test-equal "substring-empty" "" (substring "abc" 0 0))
(test-equal "substring-one" "a" (substring "abc" 0 1))
(test-equal "substring-end" "bc" (substring "abc" 1 3))

(test-equal "string-append-empty-right" "abc" (string-append "abc" ""))
(test-equal "string-append-empty-left" "abc" (string-append "" "abc"))
(test-equal "string-append-two" "abc" (string-append "a" "bc"))

;; ═══════════════════════════════════════════════════════════════════════════
;; VECTOR TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "vector-set" '#(0 ("Sue" "Sue") "Anna")
  (let ((vec (vector 0 '(2 2 2 2) "Anna")))
    (vector-set! vec 1 '("Sue" "Sue"))
    vec))

(test-equal "vector-to-list" '(dah dah didah) (vector->list '#(dah dah didah)))
(test-equal "list-to-vector" '#(dididit dah) (list->vector '(dididit dah)))

;; ═══════════════════════════════════════════════════════════════════════════
;; PROCEDURE TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-assert "procedure-car" (procedure? car))
(test-assert "procedure-symbol" (not (procedure? 'car)))
(test-assert "procedure-lambda" (procedure? (lambda (x) (* x x))))
(test-assert "procedure-quoted-lambda" (not (procedure? '(lambda (x) (* x x)))))

;; ═══════════════════════════════════════════════════════════════════════════
;; APPLY AND MAP TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "apply" 7 (apply + (list 3 4)))

(test-equal "map-cadr" '(b e h) (map cadr '((a b) (d e) (g h))))
(test-equal "map-expt" '(1 4 27 256 3125)
  (map (lambda (n) (expt n n)) '(1 2 3 4 5)))

(test-equal "for-each" '#(0 1 4 9 16)
  (let ((v (make-vector 5)))
    (for-each
      (lambda (i) (vector-set! v i (* i i)))
      '(0 1 2 3 4))
    v))

;; ═══════════════════════════════════════════════════════════════════════════
;; DELAY AND FORCE TESTS
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "delay-force" 3 (force (delay (+ 1 2))))
(test-equal "delay-force-multiple" '(3 3)
  (let ((p (delay (+ 1 2)))) (list (force p) (force p))))

;; ═══════════════════════════════════════════════════════════════════════════
;; EDGE CASES FOR KEYWORDS AS VARIABLES
;; ═══════════════════════════════════════════════════════════════════════════

(test-equal "else-as-variable" 'ok
  (let ((else 1)) (cond (else 'ok) (#t 'bad))))

(test-end)
