;;; (scheme inexact) — R7RS §6.2.6 Inexact arithmetic
(define-library (scheme inexact)
  (export
    finite? infinite? nan?
    sqrt exp log sin cos tan asin acos atan
    exact inexact))
