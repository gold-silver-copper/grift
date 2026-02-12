;;; List operation tests
;;; Migrated from lib_tests.rs

(test-begin "lists")

;; Cons
(test-equal "car-cons" 1 (car (cons 1 2)))
(test-equal "cdr-cons" 2 (cdr (cons 1 2)))

;; List construction
(test-equal "list-length" 5 (length '(a b c d e)))
(test-equal "list-create" '(1 2 3) (list 1 2 3))
(test-assert "null-empty" (null? '()))
(test-assert "not-null-list" (not (null? '(1))))
(test-assert "pair-pred" (pair? '(1 2)))
(test-assert "list-pred" (list? '(1 2 3)))

;; Append
(test-equal "append-two" '(1 2 3 4) (append '(1 2) '(3 4)))
(test-equal "append-empty" '(1 2) (append '() '(1 2)))
(test-equal "append-three" '(1 2 3 4 5 6) (append '(1 2) '(3 4) '(5 6)))

;; Reverse
(test-equal "reverse" '(3 2 1) (reverse '(1 2 3)))
(test-equal "reverse-empty" '() (reverse '()))

;; Map
(test-equal "map-square" '(1 4 9) (map (lambda (x) (* x x)) '(1 2 3)))
(test-equal "map-add" '(11 12 13) (map (lambda (x) (+ x 10)) '(1 2 3)))

;; For-each
(test-equal "for-each-side-effect" 6
  (let ((sum 0))
    (for-each (lambda (x) (set! sum (+ sum x))) '(1 2 3))
    sum))

;; Filter (using standard library)
(test-equal "filter-even" '(2 4 6) (filter even? '(1 2 3 4 5 6)))

;; Assoc
(test-equal "assoc-found" '(b 2) (assoc 'b '((a 1) (b 2) (c 3))))
(test-assert "assoc-not-found" (not (assoc 'd '((a 1) (b 2) (c 3)))))

;; Member
(test-equal "member-found" '(3 4 5) (member 3 '(1 2 3 4 5)))
(test-assert "member-not-found" (not (member 6 '(1 2 3 4 5))))

;; Caar, cadr, etc.
(test-equal "cadr" 2 (cadr '(1 2 3)))
(test-equal "caddr" 3 (caddr '(1 2 3)))
(test-equal "caar" 1 (caar '((1 2) 3)))

;; Apply
(test-equal "apply-add" 10 (apply + '(1 2 3 4)))
(test-equal "apply-list" '(1 2 3) (apply list '(1 2 3)))

;; Folding
(test-equal "fold-right" '(1 2 3) (fold-right cons '() '(1 2 3)))

(test-end)
