; Test basic syntax-case functionality

; Test 1: Simple syntax-case pattern matching
(define-syntax simple-mac
  (lambda (x)
    (syntax-case x ()
      ((simple-mac a b)
       (syntax (list a b))))))

(simple-mac 1 2)
; Expected: (list 1 2) => (1 2)

; Test 2: Syntax object creation
(syntax (foo bar baz))
; Expected: syntax object wrapping (foo bar baz)

; Test 3: Multiple clauses
(define-syntax multi-mac
  (lambda (x)
    (syntax-case x ()
      ((multi-mac)
       (syntax 'empty))
      ((multi-mac a)
       (syntax (list 'one a)))
      ((multi-mac a b)
       (syntax (list 'two a b))))))

(multi-mac)
; Expected: 'empty

(multi-mac 42)
; Expected: (list 'one 42) => (one 42)

(multi-mac 1 2)
; Expected: (list 'two 1 2) => (two 1 2)
