;;; (scheme complex) — R7RS §6.2.6 Complex number operations
(define-library (scheme complex)
  (export
    make-rectangular make-polar
    real-part imag-part
    magnitude angle))
