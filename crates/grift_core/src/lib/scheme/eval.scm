;;; (scheme eval) — R7RS §6.12
;;;
;;; All exports are native builtins implemented in Rust.
(define-library (scheme eval)
  (export eval environment))
