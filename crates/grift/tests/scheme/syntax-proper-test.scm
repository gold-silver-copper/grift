;;; Syntax Proper Tests for Grift
;;; Migrated from syntax_proper_tests.rs
;;;
;;; Tests lexically-scoped syntax objects, identifier comparison,
;;; scope preservation, and basic hygiene.

(test-begin "syntax-proper")

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

;; 2.1: Helper function returns syntax object
(define (sp-2-1-helper)
  (let ((secret 42))
    (define (inner) (syntax secret))
    (inner)))
(define sp-2-1-borrowed-stx (sp-2-1-helper))
(define-syntax sp-2-1-use-borrowed
  (lambda (_) sp-2-1-borrowed-stx))
(test-equal "2.1-helper-function-returns-syntax" 42 (sp-2-1-use-borrowed))

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

;; 6.1: bound-identifier=? with same context
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

;; Pattern substitution
(define-syntax sp-ps-my-let
  (lambda (stx)
    (syntax-case stx ()
      ((sp-ps-my-let ((name val) ...) body ...)
       (syntax ((lambda (name ...) body ...) val ...))))))
(test-equal "pattern-substitution" 15
  (sp-ps-my-let ((x 5) (y 10)) (+ x y)))

(test-end)
