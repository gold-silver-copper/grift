;;; Syntax Edge Case Tests for Grift
;;; Migrated from syntax_edge_case_tests.rs
;;;
;;; Test categories:
;;; A: First-class syntax object manipulation
;;; B: Runtime syntax creation
;;; C: Hygiene edge cases
;;; D: datum->syntax and syntax->datum
;;; E: with-syntax edge cases
;;; F: Ellipsis edge cases
;;; G: Identifier comparison edge cases
;;; H: Macro-defining macros
;;; I: Stress tests
;;; J: Macro limitation tests (features that actually work)
;;; K: datum->syntax-object and syntax-object->datum (R6RS aliases)

(test-begin "syntax-edge-case")

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite A: First-Class Syntax Object Manipulation
;; ═══════════════════════════════════════════════════════════════════════════

;; A.1: Syntax objects can be stored in variables
(define a1-stx
  (let ((x 42))
    (syntax x)))
(define-syntax a1-use
  (lambda (_) a1-stx))
(test-equal "a1-syntax-in-variable" 42 (a1-use))

;; A.2: Syntax objects in pairs
(define a2-stx-pair
  (let ((a 10) (b 20))
    (cons (syntax a) (syntax b))))
(define-syntax a2-use-car
  (lambda (_) (car a2-stx-pair)))
(define-syntax a2-use-cdr
  (lambda (_) (cdr a2-stx-pair)))
(test-equal "a2-syntax-in-pair-car" 10 (a2-use-car))
(test-equal "a2-syntax-in-pair-cdr" 20 (a2-use-cdr))

;; A.3: Syntax objects in vectors
(define a3-stx-vec
  (let ((x 100))
    (vector (syntax x) (syntax x))))
(define-syntax a3-use-first
  (lambda (_) (vector-ref a3-stx-vec 0)))
(test-equal "a3-syntax-in-vector" 100 (a3-use-first))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite B: Runtime Syntax Creation
;; ═══════════════════════════════════════════════════════════════════════════

;; B.1: Function that returns syntax
(define (b1-make-const-syntax n)
  (let ((val n))
    (syntax val)))
(define b1-stx-5 (b1-make-const-syntax 5))
(define b1-stx-10 (b1-make-const-syntax 10))
(define-syntax b1-get-5
  (lambda (_) b1-stx-5))
(define-syntax b1-get-10
  (lambda (_) b1-stx-10))
(test-equal "b1-function-returning-syntax-5" 5 (b1-get-5))
(test-equal "b1-function-returning-syntax-10" 10 (b1-get-10))

;; B.2: Conditional syntax creation
(define (b2-make-syntax-or-literal use-syntax)
  (let ((x 42))
    (if use-syntax
        (syntax x)
        x)))
(define b2-my-stx (b2-make-syntax-or-literal #t))
(define-syntax b2-use-stx
  (lambda (_) b2-my-stx))
(test-equal "b2-conditional-syntax-creation" 42 (b2-use-stx))

;; B.3: Recursive syntax creation
(define (b3-nested-syntax depth)
  (let ((x depth))
    (if (= depth 0)
        (syntax x)
        (b3-nested-syntax (- depth 1)))))
(define b3-deep-stx (b3-nested-syntax 5))
(define-syntax b3-use-deep
  (lambda (_) b3-deep-stx))
(test-equal "b3-recursive-syntax-creation" 0 (b3-use-deep))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite C: Hygiene Edge Cases
;; ═══════════════════════════════════════════════════════════════════════════

;; C.1: Multiple macro invocations with same introduced names
(define-syntax c1-with-temp
  (lambda (stx)
    (syntax-case stx ()
      ((_ val body)
       (syntax (let ((temp val))
                 body))))))
(test-equal "c1-multiple-expansions-hygiene" 40
  (c1-with-temp 10
    (c1-with-temp 20
      (+ temp temp))))

;; C.2: Pattern variable shadowing local binding
(define-syntax c2-shadow-test
  (lambda (stx)
    (syntax-case stx ()
      ((kw x)
       (let ((outer-x (syntax x)))
         (let ((x 999))
           (let ((inner-x (syntax x)))
             (if (free-identifier=? outer-x inner-x)
                 (syntax 'same)
                 (syntax 'different)))))))))
(test-equal "c2-pattern-shadowing" 'different (c2-shadow-test 123))

;; C.3: Hygiene with recursive macros
(define-syntax c3-sum-to
  (lambda (stx)
    (syntax-case stx ()
      ((kw n)
       (let ((num (syntax->datum (syntax n))))
         (if (= num 0)
             (syntax 0)
             (with-syntax ((m (- num 1)))
               (syntax (+ n (c3-sum-to m))))))))))
(test-equal "c3-recursive-macro-hygiene" 6 (c3-sum-to 3))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite D: datum->syntax and syntax->datum
;; ═══════════════════════════════════════════════════════════════════════════

;; D.1: datum->syntax with template context
(define-syntax d1-make-ref
  (lambda (stx)
    (syntax-case stx ()
      ((kw name)
       (datum->syntax (syntax kw) (syntax->datum (syntax name)))))))
(define d1-foo 42)
(test-equal "d1-datum-to-syntax-context" 42 (d1-make-ref d1-foo))

;; D.2: syntax->datum round-trip
(define-syntax d2-structure-test
  (lambda (stx)
    (syntax-case stx ()
      ((kw (a b c))
       (let ((datum (syntax->datum (syntax (a b c)))))
         (if (and (pair? datum)
                  (eq? (car datum) 'a)
                  (eq? (cadr datum) 'b)
                  (eq? (caddr datum) 'c))
             (syntax 'correct)
             (syntax 'wrong)))))))
(test-equal "d2-syntax-datum-roundtrip" 'correct (d2-structure-test (a b c)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite E: with-syntax Edge Cases
;; ═══════════════════════════════════════════════════════════════════════════

;; E.1: with-syntax with computed values
(define-syntax e1-double-literal
  (lambda (stx)
    (syntax-case stx ()
      ((kw n)
       (let ((doubled (* 2 (syntax->datum (syntax n)))))
         (with-syntax ((result doubled))
           (syntax result)))))))
(test-equal "e1-with-syntax-computed" 42 (e1-double-literal 21))

;; E.2: with-syntax with multiple bindings
(define-syntax e2-swap-and-add
  (lambda (stx)
    (syntax-case stx ()
      ((kw a b)
       (with-syntax ((x (syntax b))
                     (y (syntax a)))
         (syntax (+ x y)))))))
(test-equal "e2-with-syntax-multiple" 30 (e2-swap-and-add 10 20))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite F: Ellipsis Edge Cases
;; ═══════════════════════════════════════════════════════════════════════════

;; F.1: Empty ellipsis match
(define-syntax f1-list-or-zero
  (lambda (stx)
    (syntax-case stx ()
      ((kw x ...)
       (syntax (list x ...))))))
(test-assert "f1-empty-ellipsis" (null? (f1-list-or-zero)))
(test-equal "f1-non-empty-ellipsis" 3 (length (f1-list-or-zero 1 2 3)))

;; F.2: Ellipsis with multiple pattern variables (parallel)
(define-syntax f2-zip-add
  (lambda (stx)
    (syntax-case stx ()
      ((kw (a b) ...)
       (syntax (list (+ a b) ...))))))
(test-equal "f2-parallel-ellipsis-first" 11 (car (f2-zip-add (1 10) (2 20) (3 30))))

;; F.3: Ellipsis after fixed elements
(define-syntax f3-first-then-rest
  (lambda (stx)
    (syntax-case stx ()
      ((kw first rest ...)
       (syntax (cons first (list rest ...)))))))
(test-equal "f3-fixed-then-ellipsis-car" 1 (car (f3-first-then-rest 1 2 3 4)))
(test-equal "f3-fixed-then-ellipsis-cdr-len" 3 (length (cdr (f3-first-then-rest 1 2 3 4))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite G: Identifier Comparison Edge Cases
;; ═══════════════════════════════════════════════════════════════════════════

;; G.1: bound-identifier=? with identifiers from same context
(define-syntax g1-check-different-contexts
  (lambda (stx)
    (syntax-case stx ()
      ((kw)
       (let ((id1 (datum->syntax (syntax kw) 'x))
             (id2 (datum->syntax (syntax kw) 'x)))
         (if (bound-identifier=? id1 id2)
             (syntax 'same)
             (syntax 'different)))))))
(test-equal "g1-same-context-bound-eq" 'same (g1-check-different-contexts))

;; G.2: free-identifier=? across environments
(define g2-global-x 100)
(define-syntax g2-check-free-eq
  (lambda (stx)
    (syntax-case stx ()
      ((kw a b)
       (if (free-identifier=? (syntax a) (syntax b))
           (syntax 'same)
           (syntax 'different))))))
(test-equal "g2-free-identifier-same-binding" 'same (g2-check-free-eq g2-global-x g2-global-x))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite H: Macro-Defining Macros
;; ═══════════════════════════════════════════════════════════════════════════

;; H.1: Simple macro-defining macro
(define-syntax h1-define-constant-macro
  (lambda (stx)
    (syntax-case stx ()
      ((kw name val)
       (syntax (define-syntax name
                 (lambda (_) (syntax val))))))))
(h1-define-constant-macro h1-forty-two 42)
(test-equal "h1-macro-defining-macro" 42 (h1-forty-two))

;; H.2: Parameterized macro generator
(define-syntax h2-define-adder-macro
  (lambda (stx)
    (syntax-case stx ()
      ((kw name amount)
       (syntax (define-syntax name
                 (lambda (inner-stx)
                   (syntax-case inner-stx ()
                     ((_ x) (syntax (+ x amount)))))))))))
(h2-define-adder-macro h2-add-5 5)
(h2-define-adder-macro h2-add-10 10)
(test-equal "h2-parameterized-macro-add5" 105 (h2-add-5 100))
(test-equal "h2-parameterized-macro-add10" 110 (h2-add-10 100))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite I: Stress Tests
;; ═══════════════════════════════════════════════════════════════════════════

;; I.1: Deeply nested let-syntax
(test-equal "i1-deeply-nested-let-syntax" 6
  (let-syntax ((a (lambda (stx) (syntax 1))))
    (let-syntax ((b (lambda (stx) (syntax (+ (a) 2)))))
      (let-syntax ((c (lambda (stx) (syntax (+ (b) 3)))))
        (c)))))

;; I.2: Large ellipsis expansion
(define-syntax i2-sum-all
  (lambda (stx)
    (syntax-case stx ()
      ((kw x ...)
       (syntax (+ x ...))))))
(test-equal "i2-large-ellipsis" 55 (i2-sum-all 1 2 3 4 5 6 7 8 9 10))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite J: Macro Limitation Tests (features that actually work)
;; ═══════════════════════════════════════════════════════════════════════════

;; J.1: Compile-time arithmetic without runtime values
(define-syntax j1-static-compute
  (lambda (stx)
    (syntax-case stx ()
      ((_ n m)
       (datum->syntax stx
         (+ (syntax->datum (syntax n))
            (syntax->datum (syntax m))))))))
(test-equal "j1-compile-time-arithmetic" 30 (j1-static-compute 10 20))

;; J.2: Match multiple patterns with different arities
(define-syntax j2-flexible-macro
  (lambda (stx)
    (syntax-case stx ()
      ((_ a)
       (syntax (list a)))
      ((_ a b)
       (syntax (list a b)))
      ((_ a b c)
       (syntax (list a b c))))))
(test-equal "j2-flexible-arity-1" 1 (length (j2-flexible-macro 1)))
(test-equal "j2-flexible-arity-2" 2 (length (j2-flexible-macro 1 2)))
(test-equal "j2-flexible-arity-3" 3 (length (j2-flexible-macro 1 2 3)))

;; J.3: Nested ellipsis patterns
(define-syntax j3-matrix-transpose
  (lambda (stx)
    (syntax-case stx ()
      ((_ ((a ...) ...))
       (syntax (quote ((a ...) ...)))))))
(test-equal "j3-nested-ellipsis-len" 2
  (length (j3-matrix-transpose ((1 2 3) (4 5 6)))))
(test-equal "j3-nested-ellipsis-first-len" 3
  (length (car (j3-matrix-transpose ((1 2 3) (4 5 6))))))
(test-equal "j3-nested-ellipsis-first-car" 1
  (car (car (j3-matrix-transpose ((1 2 3) (4 5 6))))))

;; J.4: Fenders (guards) in patterns
(define-syntax j4-only-positive
  (lambda (stx)
    (syntax-case stx ()
      ((_ n)
       (> (syntax->datum (syntax n)) 0)
       (syntax (quote positive)))
      ((_ n)
       (syntax (quote non-positive))))))
(test-equal "j4-fender-positive" 'positive (j4-only-positive 5))
(test-equal "j4-fender-negative" 'non-positive (j4-only-positive -3))
(test-equal "j4-fender-zero" 'non-positive (j4-only-positive 0))

;; J.5: Complex literal matching
(define-syntax j5-match-literals
  (lambda (stx)
    (syntax-case stx (foo bar baz)
      ((_ foo x) (syntax (quote (matched-foo x))))
      ((_ bar x) (syntax (quote (matched-bar x))))
      ((_ baz x) (syntax (quote (matched-baz x))))
      ((_ other x) (syntax (quote (matched-other other x)))))))
(test-equal "j5-literal-foo" 'matched-foo (car (j5-match-literals foo 1)))
(test-equal "j5-literal-bar" 'matched-bar (car (j5-match-literals bar 2)))
(test-equal "j5-literal-other" 'matched-other (car (j5-match-literals qux 3)))

;; J.6: Identifier comparison in templates
(define-syntax j6-test-hygiene
  (lambda (stx)
    (syntax-case stx ()
      ((_ x)
       (let ((id1 (datum->syntax (syntax x) (quote temp)))
             (id2 (datum->syntax (syntax x) (quote temp))))
         (if (bound-identifier=? id1 id2)
             (syntax (lambda (temp) temp))
             (syntax (lambda (y) y))))))))
(define j6-f (j6-test-hygiene dummy))
(test-equal "j6-identifier-comparison" 42 (j6-f 42))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite K: datum->syntax-object and syntax-object->datum (R6RS aliases)
;; ═══════════════════════════════════════════════════════════════════════════

;; K.1: datum->syntax-object basic usage
(define-syntax k1-make-ref
  (lambda (stx)
    (syntax-case stx ()
      ((kw name)
       (datum->syntax-object (syntax kw) (syntax-object->datum (syntax name)))))))
(define k1-foo 42)
(test-equal "k1-datum-to-syntax-object-basic" 42 (k1-make-ref k1-foo))

;; K.2: define-structure macro using datum->syntax-object
(define-syntax k2-define-structure
  (lambda (x)
    (define gen-id
      (lambda (template-id . args)
        (datum->syntax-object template-id
          (string->symbol
            (apply string-append
                   (map (lambda (x)
                          (if (string? x)
                              x
                              (symbol->string
                                (syntax-object->datum x))))
                        args))))))
    (syntax-case x ()
      ((_ name field ...)
       (with-syntax
         ((constructor (gen-id (syntax name) "make-" (syntax name)))
          (predicate (gen-id (syntax name) (syntax name) "?"))
          ((access ...)
           (map (lambda (x) (gen-id x (syntax name) "-" x))
                (syntax (field ...))))
          ((assign ...)
           (map (lambda (x) (gen-id x "set-" (syntax name) "-" x "!"))
                (syntax (field ...))))
          (structure-length (+ (length (syntax (field ...))) 1))
          ((index ...) (let f ((i 1) (ids (syntax (field ...))))
                         (if (null? ids)
                             '()
                             (cons i (f (+ i 1) (cdr ids)))))))
         (syntax (begin
                   (define constructor
                     (lambda (field ...)
                       (vector 'name field ...)))
                   (define predicate
                     (lambda (x)
                       (and (vector? x)
                            (= (vector-length x) structure-length)
                            (eq? (vector-ref x 0) 'name))))
                   (define access
                     (lambda (x)
                       (vector-ref x index)))
                   ...
                   (define assign
                     (lambda (x update)
                       (vector-set! x index update)))
                   ...)))))))

(k2-define-structure k2-tree left right)

(define k2-t
  (make-k2-tree
    (make-k2-tree 0 1)
    (make-k2-tree 2 3)))

(test-assert "k2-tree-predicate" (k2-tree? k2-t))
(test-assert "k2-tree-left-is-tree" (k2-tree? (k2-tree-left k2-t)))
(test-equal "k2-tree-left-left" 0 (k2-tree-left (k2-tree-left k2-t)))
(test-equal "k2-tree-left-right" 1 (k2-tree-right (k2-tree-left k2-t)))
(test-equal "k2-tree-right-left" 2 (k2-tree-left (k2-tree-right k2-t)))
(test-equal "k2-tree-right-right" 3 (k2-tree-right (k2-tree-right k2-t)))

;; Test set-tree-left! mutation
(set-k2-tree-left! k2-t 0)
(test-equal "k2-tree-mutated-left" 0 (k2-tree-left k2-t))
(test-equal "k2-tree-right-still-intact" 2 (k2-tree-left (k2-tree-right k2-t)))

(test-end "syntax-edge-case")
