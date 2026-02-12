;;; (scheme process-context) — R7RS §6.14
;;;
;;; All exports are native builtins implemented in Rust.
(define-library (scheme process-context)
  (export
    command-line exit emergency-exit
    get-environment-variable
    get-environment-variables))
