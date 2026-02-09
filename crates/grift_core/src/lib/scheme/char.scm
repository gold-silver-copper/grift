;;; (scheme char) — R7RS §6.6 Characters (full Unicode)
(define-library (scheme char)
  (export
    char-alphabetic? char-numeric? char-whitespace?
    char-upper-case? char-lower-case?
    char-ci=? char-ci<? char-ci>? char-ci<=? char-ci>=?
    char-upcase char-downcase char-foldcase
    digit-value
    string-ci=?
    string-upcase string-downcase string-foldcase))
