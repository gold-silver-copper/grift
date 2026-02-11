;;; (scheme complex) — R7RS §6.2.6 Complex number operations
;;;
;;; All exports are native builtins implemented in Rust.
(define-library (scheme complex)
  (export
    make-rectangular make-polar
    real-part imag-part
    magnitude angle))
