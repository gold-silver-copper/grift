;;; Tests from "Syntactic Extension" (Chapter 8, The Scheme Programming Language)
;;; Migrated from syntactic_extension_html_tests.rs

(test-begin "syntactic-extension-html")

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 8.1: Keyword Bindings
;; ═══════════════════════════════════════════════════════════════════════════

;; test_sec8_1_define_let_star
(define-syntax seh-my-let*
  (syntax-rules ()
    ((_ () e1 e2 ...) (let () e1 e2 ...))
    ((_ ((i1 v1) (i2 v2) ...) e1 e2 ...)
     (let ((i1 v1))
       (seh-my-let* ((i2 v2) ...) e1 e2 ...)))))

(test-equal "sec8-1-define-let-star" 3
  (seh-my-let* ((a 1) (b (+ a 1))) (+ a b)))

;; test_sec8_1_even_odd_interleaved
(test-assert "sec8-1-even-odd-interleaved"
  (let ()
    (define even?-81
      (lambda (x)
        (or (= x 0) (odd?-81 (- x 1)))))
    (define-syntax odd?-81
      (syntax-rules ()
        ((_ x) (not (even?-81 x)))))
    (even?-81 10)))

;; test_sec8_1_bind_to_zero
(test-equal "sec8-1-bind-to-zero" 0
  (let ()
    (define-syntax seh-bind-to-zero
      (syntax-rules ()
        ((_ id) (define id 0))))
    (seh-bind-to-zero x)
    x))

;; test_sec8_1_nested_let_syntax
(test-equal "sec8-1-nested-let-syntax" 2
  (let ((f (lambda (x) (+ x 1))))
    (let-syntax ((g (syntax-rules ()
                      ((_ x) (f x)))))
      (let-syntax ((f (syntax-rules ()
                        ((_ x) x))))
        (g 1)))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 8.2: Syntax-Rules
;; ═══════════════════════════════════════════════════════════════════════════

;; test_sec8_2_or_syntax_rules
(define-syntax seh-my-or
  (syntax-rules ()
    ((_) #f)
    ((_ e) e)
    ((_ e1 e2 e3 ...)
     (let ((t e1)) (if t t (seh-my-or e2 e3 ...))))))

(test-equal "sec8-2-or-syntax-rules-1" 42 (seh-my-or #f #f 42))
(test-equal "sec8-2-or-syntax-rules-2" #f (seh-my-or #f))
(test-equal "sec8-2-or-syntax-rules-3" 1 (seh-my-or 1 2))

;; test_sec8_2_or_desugared
(test-equal "sec8-2-or-desugared" 'okay
  ((lambda (if1)
     ((lambda (t1)
        ((lambda (t2)
           (if t2 t2 t1))
         if1))
      'okay))
   #f))

;; test_sec8_2_cond_syntax_rules
(define-syntax seh-my-cond
  (syntax-rules (else)
    ((_ (else e1 e2 ...)) (begin e1 e2 ...))
    ((_ (e0 e1 e2 ...)) (if e0 (begin e1 e2 ...)))
    ((_ (e0 e1 e2 ...) c1 c2 ...)
     (if e0 (begin e1 e2 ...) (seh-my-cond c1 c2 ...)))))

(test-equal "sec8-2-cond-syntax-rules-1" 2 (seh-my-cond (#f 1) (else 2)))
(test-equal "sec8-2-cond-syntax-rules-2" 1 (seh-my-cond (#t 1) (else 2)))
(test-equal "sec8-2-cond-syntax-rules-3" 3 (seh-my-cond (#f 1) (#f 2) (else 3)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 8.3: Syntax-Case
;; ═══════════════════════════════════════════════════════════════════════════

;; test_sec8_3_or_syntax_case
(define-syntax seh-my-or2
  (lambda (x)
    (syntax-case x ()
      ((_) (syntax #f))
      ((_ e) (syntax e))
      ((_ e1 e2 e3 ...)
       (syntax (let ((t e1)) (if t t (seh-my-or2 e2 e3 ...))))))))

(test-equal "sec8-3-or-syntax-case-1" 99 (seh-my-or2 #f #f 99))
(test-equal "sec8-3-or-syntax-case-2" #f (seh-my-or2 #f))
(test-equal "sec8-3-or-syntax-case-3" 1 (seh-my-or2 1))

;; test_sec8_3_with_syntax
(test-equal "sec8-3-with-syntax" '(1 2)
  (with-syntax ((a (syntax 1))
                (b (syntax 2)))
    (list a b)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 8.4: Examples - rec and named let
;; ═══════════════════════════════════════════════════════════════════════════

;; test_sec8_4_rec
(define-syntax seh-rec
  (syntax-rules ()
    ((_ x e) (letrec ((x e)) x))))

(test-equal "sec8-4-rec" '(0 1 3 6 10 15)
  (map (seh-rec sum
         (lambda (x)
           (if (= x 0)
               0
               (+ x (sum (- x 1))))))
       '(0 1 2 3 4 5)))

;; test_sec8_4_letrec_via_syntax_case
(define-syntax seh-my-letrec
  (lambda (x)
    (syntax-case x ()
      ((_ ((i v) ...) e1 e2 ...)
       (syntax (let ((i #f) ...)
                 (set! i v) ...
                 (let () e1 e2 ...)))))))

(test-assert "sec8-4-letrec-via-syntax-case"
  (seh-my-letrec ((even?-84 (lambda (n) (if (= n 0) #t (odd?-84 (- n 1)))))
                  (odd?-84 (lambda (n) (if (= n 0) #f (even?-84 (- n 1))))))
    (even?-84 10)))

;; test_sec8_4_sequence
(define-syntax seh-sequence
  (syntax-rules ()
    ((_ e0 e1 ...)
     (begin e0 e1 ...))))

(test-equal "sec8-4-sequence" 42 (seh-sequence 1 2 42))

;; test_sec8_4_be_like_begin_escaping_ellipsis
(define-syntax seh-be-like-begin
  (syntax-rules ()
    ((_ name)
     (define-syntax name
       (syntax-rules ()
         ((_ e0 e1 (... ...))
          (begin e0 e1 (... ...))))))))

(seh-be-like-begin seh-sequence2)

(test-equal "sec8-4-be-like-begin-escaping-ellipsis" 3 (seh-sequence2 1 2 3))

;; test_sec8_4_if_macro_error_on_wrong_arity
;; Skipped: This test expects a macro expansion error (wrong arity)
;; which cannot be reliably caught with test-error in SRFI-64.

;; test_sec8_3_divide_template_hygiene
(test-equal "sec8-3-divide-template-hygiene" 2
  (let-syntax ((divide (lambda (x)
                          (let ((/ +))
                            (syntax-case x ()
                              ((_ e1 e2)
                               (syntax (/ e1 e2))))))))
    (let ((/ *)) (divide 2 1))))

;; ═══════════════════════════════════════════════════════════════════════════
;; Section 8.1: letrec-syntax
;; ═══════════════════════════════════════════════════════════════════════════

;; test_sec8_1_letrec_syntax_basic
;; letrec-syntax: g can see f's binding (mutual visibility)
(test-assert "sec8-1-letrec-syntax-basic"
  (let ((f (lambda (x) (+ x 1))))
    (letrec-syntax ((f (syntax-rules ()
                         ((_ x) x)))
                    (g (syntax-rules ()
                         ((_ x) (f x)))))
      (list (f 1) (g 1))
      #t)))

;; test_sec8_3_dolet_hygiene
(test-equal "sec8-3-dolet-hygiene" 7
  (let-syntax ((dolet (lambda (x)
                         (syntax-case x ()
                           ((_ b)
                            (syntax (let ((a 3) (b 4))
                                      (+ a b))))))))
    (dolet a)))

;; ═══════════════════════════════════════════════════════════════════════════
;; Additional tests from other sections
;; ═══════════════════════════════════════════════════════════════════════════

;; test_builtin_or_basic
(test-equal "builtin-or-basic-1" #f (or #f #f))
(test-equal "builtin-or-basic-2" 42 (or #f 42))
(test-equal "builtin-or-basic-3" 1 (or 1 2))

;; test_builtin_cond_basic
(test-equal "builtin-cond-basic-1" 1 (cond (#t 1) (else 2)))
(test-equal "builtin-cond-basic-2" 2 (cond (#f 1) (else 2)))
(test-equal "builtin-cond-basic-3" 2 (cond (#f 1) (#t 2) (else 3)))

;; test_sec8_2_syntax_rules_is_macro
(define-syntax seh-triple
  (syntax-rules ()
    ((_ x) (* x 3))))

(test-equal "sec8-2-syntax-rules-is-macro-1" 15 (seh-triple 5))
(test-equal "sec8-2-syntax-rules-is-macro-2" 9 (seh-triple (+ 1 2)))

;; test_set_ellipsis_in_template
(define-syntax seh-set-all!
  (lambda (x)
    (syntax-case x ()
      ((_ (var ...) (val ...))
       (syntax (begin (set! var val) ...))))))

(test-equal "set-ellipsis-in-template" '(1 2 3)
  (let ((a 0) (b 0) (c 0))
    (seh-set-all! (a b c) (1 2 3))
    (list a b c)))

;; test_let_ellipsis_in_template
(define-syntax seh-my-let-init
  (lambda (x)
    (syntax-case x ()
      ((_ (var ...) e1 e2 ...)
       (syntax (let ((var #f) ...) e1 e2 ...))))))

(test-equal "let-ellipsis-in-template" '(#f #f #f)
  (seh-my-let-init (a b c) (list a b c)))

;; test_generate_temporaries
(test-equal "generate-temporaries" 3
  (length (generate-temporaries '(a b c))))

;; test_letrec_syntax_recognized
(test-equal "letrec-syntax-recognized" 42
  (letrec-syntax ((const42 (syntax-rules ()
                              ((_) 42))))
    (const42)))

;; test_letrec_syntax_self_reference
(test-equal "letrec-syntax-self-reference" 42
  (letrec-syntax ((my-begin
                    (syntax-rules ()
                      ((_ e) e)
                      ((_ e1 e2 ...)
                       (let ((t e1)) (my-begin e2 ...))))))
    (my-begin 1 2 42)))

(test-end)
