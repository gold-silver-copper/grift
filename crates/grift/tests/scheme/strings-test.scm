;;; String operation tests
;;; Migrated from lib_tests.rs

(test-begin "strings")

;; String predicates
(test-assert "string-pred" (string? "hello"))
(test-assert "not-string-num" (not (string? 42)))

;; String length
(test-equal "string-length" 5 (string-length "hello"))
(test-equal "string-length-empty" 0 (string-length ""))

;; String ref
(test-equal "string-ref-0" #\h (string-ref "hello" 0))
(test-equal "string-ref-4" #\o (string-ref "hello" 4))

;; String append
(test-equal "string-append" "hello world" (string-append "hello" " " "world"))
(test-equal "string-append-empty" "hello" (string-append "hello" ""))

;; Substring
(test-equal "substring" "ell" (substring "hello" 1 4))
(test-equal "substring-start" "hel" (substring "hello" 0 3))

;; String contains characters
(test-assert "char-pred" (char? #\a))
(test-equal "char-alpha" #t (char-alphabetic? #\a))
(test-equal "char-numeric" #t (char-numeric? #\5))

;; String comparison
(test-assert "string-equal" (string=? "hello" "hello"))
(test-assert "string-not-equal" (not (string=? "hello" "world")))
(test-assert "string-less" (string<? "abc" "abd"))

;; String conversion
(test-equal "number-to-string" "42" (number->string 42))
(test-equal "string-to-number" 42 (string->number "42"))

;; String list conversions
(test-equal "string-to-list" '(#\h #\i) (string->list "hi"))
(test-equal "list-to-string" "hi" (list->string '(#\h #\i)))

;; String copy
(test-equal "string-copy" "hello" (string-copy "hello"))

;; Make-string
(test-equal "make-string" "aaa" (make-string 3 #\a))

;; String upcase/downcase
(test-equal "string-upcase" "HELLO" (string-upcase "hello"))
(test-equal "string-downcase" "hello" (string-downcase "HELLO"))

;; Symbol/string conversions
(test-equal "symbol-to-string" "hello" (symbol->string 'hello))
(test-assert "string-to-symbol" (symbol? (string->symbol "hello")))

(test-end)
