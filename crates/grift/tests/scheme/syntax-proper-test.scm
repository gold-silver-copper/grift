;;; Syntax Proper Tests for Grift
;;; Migrated from syntax_proper_tests.rs
;;;
;;; Test categories:
;;; 1: Basic Lexical Scope Preservation
;;; 2: Cross-Context Identifier Resolution
;;; 5: Scope Preservation Through Transformation
;;; 6: Identifier Comparison
;;; 7: Edge Cases and Stress Tests
;;; H: Basic Hygiene Tests

(test-begin "syntax-proper")

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite 1: Basic Lexical Scope Preservation
;; ═══════════════════════════════════════════════════════════════════════════

;; 1.1: Syntax object preserves creation context
(test-equal "1.1-syntax-preserves-creation-context"
  10
  (let ((x 10))
    (let ((stx (syntax x)))
      (let ((x 20))
        (define-syntax sp-1-1-use-stx
          (lambda (_) stx))
        (sp-1-1-use-stx)))))

;; 1.2: Syntax objects through procedures
(define (sp-1-2-make-syntax-getter val)
  (let ((x val))
    (syntax x)))
(define sp-1-2-stx1 (sp-1-2-make-syntax-getter 100))
(define sp-1-2-stx2 (sp-1-2-make-syntax-getter 200))
(define-syntax sp-1-2-test1
  (lambda (_) sp-1-2-stx1))
(define-syntax sp-1-2-test2
  (lambda (_) sp-1-2-stx2))
(test-equal "1.2-syntax-through-procedures-stx1" 100 (sp-1-2-test1))
(test-equal "1.2-syntax-through-procedures-stx2" 200 (sp-1-2-test2))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite 2: Cross-Context Identifier Resolution
;; ═══════════════════════════════════════════════════════════════════════════

;; 2.1: Helper function returns syntax object
(define (sp-2-1-helper)
  (let ((secret 42))
    (define (inner) (syntax secret))
    (inner)))
(define sp-2-1-borrowed-stx (sp-2-1-helper))
(define-syntax sp-2-1-use-borrowed
  (lambda (_) sp-2-1-borrowed-stx))
(test-equal "2.1-helper-function-returns-syntax" 42 (sp-2-1-use-borrowed))

;; 2.2: Syntax objects in data structures
(define (sp-2-2-make-stx-list)
  (let ((a 1) (b 2) (c 3))
    (list (syntax a) (syntax b) (syntax c))))
(define sp-2-2-stx-list (sp-2-2-make-stx-list))
(define-syntax sp-2-2-sum-stx-list
  (lambda (_)
    (syntax-case sp-2-2-stx-list ()
      ((x y z)
       (syntax (+ x y z))))))
(test-equal "2.2-syntax-in-data-structures" 6 (sp-2-2-sum-stx-list))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite 5: Scope Preservation Through Transformation
;; ═══════════════════════════════════════════════════════════════════════════

;; 5.1: Nested macro expansion
(define-syntax sp-5-1-inner-macro
  (lambda (stx)
    (syntax-case stx ()
      ((kw expr)
       (syntax expr)))))
(define-syntax sp-5-1-outer-macro
  (lambda (stx)
    (syntax-case stx ()
      ((kw val)
       (let ((captured-val (syntax val)))
         (with-syntax ((v captured-val))
           (syntax (sp-5-1-inner-macro v))))))))
(test-equal "5.1-nested-macro-expansion" 555
  (let ((x 555)) (sp-5-1-outer-macro x)))

;; 5.2: Template reconstruction
(define-syntax sp-5-2-reconstruct
  (lambda (stx)
    (syntax-case stx ()
      ((kw (a b c))
       (with-syntax ((new-a (syntax a))
                     (new-b (syntax b))
                     (new-c (syntax c)))
         (syntax (+ new-a new-b new-c)))))))
(test-equal "5.2-template-reconstruction" 6
  (let ((a 1) (b 2) (c 3)) (sp-5-2-reconstruct (a b c))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite 6: Identifier Comparison
;; ═══════════════════════════════════════════════════════════════════════════

;; 6.1: bound-identifier=? with different scopes
(define-syntax sp-6-1-test-bound-id-eq
  (lambda (stx)
    (syntax-case stx ()
      ((kw)
       (let ((id1 (datum->syntax (syntax kw) 'foo))
             (id2 (datum->syntax (syntax kw) 'foo)))
         (if (bound-identifier=? id1 id2)
             (syntax #t)
             (syntax #f)))))))
(test-assert "6.1-bound-identifier-eq" (sp-6-1-test-bound-id-eq))

;; 6.2: free-identifier=? with different lexical contexts
(define-syntax sp-6-2-test-free-id-eq
  (lambda (stx)
    (syntax-case stx ()
      ((kw x)
       (let ((user-x (syntax x)))
         (let ((x 999))
           (let ((macro-x (syntax x)))
             (if (free-identifier=? user-x macro-x)
                 (syntax #t)
                 (syntax #f)))))))))
(test-assert "6.2-free-identifier-eq"
  (not (let ((x 111)) (sp-6-2-test-free-id-eq x))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Suite 7: Edge Cases and Stress Tests
;; (test 7.1 skipped: requires call-site environment propagation)
;; ═══════════════════════════════════════════════════════════════════════════

;; 7.2: Deep nesting and scope chains
(test-equal "7.2-deep-nesting" 6
  (let ((level1 1))
    (let ((level2 2))
      (let ((level3 3))
        (let ((stx (syntax (+ level1 level2 level3))))
          (define-syntax sp-7-2-eval-stx
            (lambda (_) stx))
          (let ((level1 100) (level2 200) (level3 300))
            (sp-7-2-eval-stx)))))))

;; 7.3: Recursive macro with captured syntax
(define-syntax sp-7-3-count-down
  (lambda (stx)
    (syntax-case stx ()
      ((kw n)
       (let ((num (syntax->datum (syntax n))))
         (if (= num 0)
             (syntax 'done)
             (with-syntax ((m-1 (- num 1)))
               (syntax (cons n (sp-7-3-count-down m-1))))))))))
(test-equal "7.3-recursive-macro-car" 3 (car (sp-7-3-count-down 3)))
(test-equal "7.3-recursive-macro-cadr" 2 (cadr (sp-7-3-count-down 3)))
(test-equal "7.3-recursive-macro-caddr" 1 (caddr (sp-7-3-count-down 3)))
(test-equal "7.3-recursive-macro-cdddr" 'done (cdddr (sp-7-3-count-down 3)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Basic Hygiene Tests
;; ═══════════════════════════════════════════════════════════════════════════

;; Basic hygiene: macro-introduced bindings shouldn't capture user variables
(define-syntax sp-h-swap
  (lambda (stx)
    (syntax-case stx ()
      ((sp-h-swap a b)
       (syntax (let ((temp a))
                 (set! a b)
                 (set! b temp)))))))
(define sp-h-x 1)
(define sp-h-y 2)
(define sp-h-temp 999)
(sp-h-swap sp-h-x sp-h-y)
(test-equal "basic-hygiene-x" 2 sp-h-x)
(test-equal "basic-hygiene-y" 1 sp-h-y)
(test-equal "basic-hygiene-temp" 999 sp-h-temp)

;; Pattern substitution: pattern variables correctly substituted
(define-syntax sp-ps-my-let
  (lambda (stx)
    (syntax-case stx ()
      ((sp-ps-my-let ((name val) ...) body ...)
       (syntax ((lambda (name ...) body ...) val ...))))))
(test-equal "pattern-substitution" 15
  (sp-ps-my-let ((x 5) (y 10)) (+ x y)))

(test-end "syntax-proper")
