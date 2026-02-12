;;; (scheme time) — R7RS §6.14
;;;
;;; All exports are native builtins implemented in Rust.
(define-library (scheme time)
  (export
    current-second
    current-jiffy
    jiffies-per-second))
