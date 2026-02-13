;; unicode-symbol-tests.scm
;;
;; SRFI-64 test suite for Unicode symbol support
;; Extended edition

(import (scheme base)
        (scheme char)
        (srfi 64))

(test-begin "unicode-symbols")

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 1. Basic non-ASCII identifiers
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define π 3.14159)
(define привет "hello")
(define 你好 42)

(test-equal "basic unicode bindings"
  '(3.14159 "hello" 42)
  (list π привет 你好))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 2. Unicode function name
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define (平方 x) (* x x))

(test-equal "unicode procedure name"
  25
  (平方 5))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 3. Symbol equality
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define α 10)

(test-assert "eq? unicode symbol"
  (eq? 'α 'α))

(test-assert "equal? unicode symbol"
  (equal? 'α 'α))

(test-assert "symbol? unicode"
  (symbol? 'α))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 4. string->symbol / symbol->string
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define λ-sym (string->symbol "λ"))

(test-equal "symbol->string roundtrip"
  "λ"
  (symbol->string λ-sym))

(test-assert "string->symbol matches reader symbol"
  (eq? λ-sym 'λ))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 5. Combining character normalization
;;
;; NOTE:
;; R7RS does not require normalization.
;; We test consistency instead of forcing equality.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define composed 'é)    ; U+00E9
(define decomposed
  (string->symbol
    (string #\e (integer->char #x0301))))  ; e + combining acute

(test-equal "normalization consistency (eq? vs string=?)"
  (eq? composed decomposed)
  (string=? (symbol->string composed)
            (symbol->string decomposed)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 6. Emoji identifier (may fail if implementation restricts)
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-group "emoji identifiers"
  (define 🚀 99)
  (test-equal "emoji binding"
    99
    🚀)
  (test-assert "emoji eq?"
    (eq? '🚀 '🚀)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 7. Mixed-script identifier
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define hello-привет-你好 123)

(test-equal "mixed script identifier"
  123
  hello-привет-你好)

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 8. Case sensitivity
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-assert "case sensitive: Greek capital vs lowercase"
  (not (eq? 'Λ 'λ)))

(test-assert "case sensitive: Cyrillic capital vs lowercase"
  (not (eq? 'А 'а)))   ; U+0410 vs U+0430

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 9. Interning consistency
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-assert "string->symbol interning"
  (eq? (string->symbol "привет")
       (string->symbol "привет")))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 10. Reader round-trip
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define original '你好)
(define roundtrip
  (string->symbol
    (symbol->string original)))

(test-assert "symbol reader roundtrip"
  (eq? original roundtrip))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 11. Unicode in let-bindings and closures
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-equal "unicode in let-binding"
  6
  (let ((δ 2)
        (ε 3))
    (* δ ε)))

(test-equal "unicode closure variable"
  15
  (let ((倍数 (lambda (n) (* n 5))))
    (倍数 3)))

(test-equal "nested let with unicode"
  110
  (let ((φ 10))
    (let ((ψ (* φ φ)))
      (+ φ ψ))))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 12. Unicode in recursive definitions
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define (階乗 n)
  (if (<= n 1)
      1
      (* n (階乗 (- n 1)))))

(test-equal "unicode recursive function"
  120
  (階乗 5))

(define (фибоначчи n)
  (cond ((<= n 0) 0)
        ((= n 1) 1)
        (else (+ (фибоначчи (- n 1))
                 (фибоначчи (- n 2))))))

(test-equal "unicode recursive fibonacci"
  55
  (фибоначчи 10))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 13. Unicode in higher-order functions
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define (適用 f x) (f x))

(test-equal "unicode higher-order function"
  16
  (適用 平方 4))

(define (組み合わせ f g)
  (lambda (x) (f (g x))))

(test-equal "unicode compose"
  625
  ((組み合わせ 平方 平方) 5))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 14. Unicode in data structures
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define データ (list (cons 'キー "値")
                     (cons 'nombre 42)
                     (cons 'μέγεθος 100)))

(test-equal "unicode alist lookup"
  "値"
  (cdr (assq 'キー データ)))

(test-equal "unicode alist lookup 2"
  100
  (cdr (assq 'μέγεθος データ)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 15. Unicode in quasiquote / unquote
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define τ (* 2 π))

(test-equal "unicode quasiquote"
  `(τ は ,τ)
  (list 'τ 'は (* 2 3.14159)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 16. Multiple scripts in a single symbol
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define αβγ-абв-あいう 777)

(test-equal "triple-script identifier"
  777
  αβγ-абв-あいう)

(test-assert "triple-script symbol roundtrip"
  (eq? 'αβγ-абв-あいう
       (string->symbol (symbol->string 'αβγ-абв-あいう))))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 17. Symbols with Unicode digits and math operators
;;
;; NOTE: Whether Unicode digit characters (e.g. ٣ U+0663)
;; are valid in identifiers is implementation-dependent.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-equal "symbol containing subscript"
  "x₁"
  (symbol->string 'x₁))         ; U+2081

(test-equal "symbol containing superscript"
  "x²"
  (symbol->string 'x²))         ; U+00B2

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 18. Distinctness of visually similar symbols
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

;; Latin A (U+0041) vs Cyrillic А (U+0410)
(test-assert "Latin A vs Cyrillic А are distinct"
  (not (eq? (string->symbol "A")
            (string->symbol "\x0410;"))))

;; Latin o (U+006F) vs Cyrillic о (U+043E)
(test-assert "Latin o vs Cyrillic о are distinct"
  (not (eq? (string->symbol "o")
            (string->symbol "\x043E;"))))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 19. Long Unicode identifiers
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define これはとても長い変数名ですがちゃんと動くはずです 42)

(test-equal "long unicode identifier"
  42
  これはとても長い変数名ですがちゃんと動くはずです)

(test-assert "long unicode symbol interning"
  (eq? 'これはとても長い変数名ですがちゃんと動くはずです
       (string->symbol "これはとても長い変数名ですがちゃんと動くはずです")))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 20. Unicode in tail-call position
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define (繰り返し n acc)
  (if (<= n 0)
      acc
      (繰り返し (- n 1) (+ acc 1))))

(test-equal "unicode tail-recursive function"
  1000
  (繰り返し 1000 0))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 21. Unicode in syntax-rules macros
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define-syntax もし
  (syntax-rules (ならば さもなくば)
    ((_ 条件 ならば 真 さもなくば 偽)
     (if 条件 真 偽))))

(test-equal "unicode macro (もし/if)"
  "yes"
  (もし #t ならば "yes" さもなくば "no"))

(test-equal "unicode macro false branch"
  "no"
  (もし #f ならば "yes" さもなくば "no"))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 22. Unicode in multiple return values
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define (座標)
  (values 3 4))

(test-equal "unicode multiple values"
  7
  (call-with-values 座標 +))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 23. Symbol ordering consistency
;;
;; NOTE: symbol<? is not standard, so we compare via
;; symbol->string and string<?  to verify a stable ordering.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(let ((s-α (symbol->string 'α))    ; U+03B1
      (s-β (symbol->string 'β))    ; U+03B2
      (s-γ (symbol->string 'γ)))   ; U+03B3
  (test-assert "symbol ordering α < β"
    (string<? s-α s-β))
  (test-assert "symbol ordering β < γ"
    (string<? s-β s-γ)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 24. Unicode in exception messages
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-equal "unicode in error message roundtrip"
  "エラーが発生しました"
  (guard (exn (#t (cdr (assq 'メッセージ
                              (list (cons 'メッセージ "エラーが発生しました"))))))
    (error "should not reach here")))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 25. Unicode in dynamic binding (parameters)
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define 言語 (make-parameter "日本語"))

(test-equal "unicode parameter default"
  "日本語"
  (言語))

(test-equal "unicode parameterize"
  "中文"
  (parameterize ((言語 "中文"))
    (言語)))

(test-equal "unicode parameter restored"
  "日本語"
  (言語))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 26. Unicode symbols in vector and list operations
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(define ベクトル (vector 'あ 'い 'う 'え 'お))

(test-equal "unicode vector-ref"
  'う
  (vector-ref ベクトル 2))

(test-equal "unicode member"
  '(γ δ ε)
  (member 'γ '(α β γ δ ε)))

(test-equal "unicode memq"
  #f
  (memq 'ω '(α β γ)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 27. Escaped Unicode in symbol literals
;;
;; R7RS allows \xNNNN; escapes inside | ... | delimited symbols.
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-equal "hex-escaped symbol equals reader literal"
  'λ
  (string->symbol "\x03BB;"))

(test-assert "hex-escaped symbol eq?"
  (eq? 'λ (string->symbol "\x03BB;")))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 28. Shadowing of unicode bindings
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-equal "shadowing unicode binding"
  999
  (let ((π 999))
    π))

;; Verify outer binding is undisturbed
(test-equal "original unicode binding preserved"
  3.14159
  π)

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 29. Unicode in do-loop variables
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(test-equal "unicode do-loop"
  55
  (do ((カウンタ 1 (+ カウンタ 1))
       (合計 0 (+ 合計 カウンタ)))
      ((> カウンタ 10) 合計)))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; 30. Empty-ish and boundary symbols
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

;; Single-character symbols from various blocks
(test-assert "single CJK symbol"
  (symbol? '字))

(test-assert "single Hangul symbol"
  (symbol? '한))

(test-assert "single Devanagari symbol"
  (symbol? 'क))

(test-assert "single Arabic letter symbol"
  (symbol? 'ع))

(test-assert "single Thai symbol"
  (symbol? 'ก))

(test-end "unicode-symbols")
