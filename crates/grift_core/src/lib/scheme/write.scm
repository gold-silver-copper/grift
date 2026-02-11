;;; (scheme write) — R7RS §6.13.3
;;;
;;; All exports are native builtins implemented in Rust.
(define-library (scheme write)
  (export write display
          write-shared write-simple
          write-char write-string newline))
