;;; (scheme file) — R7RS §6.13 File I/O
;;;
;;; All exports are native builtins implemented in Rust.
(define-library (scheme file)
  (export
    file-exists? delete-file
    call-with-input-file call-with-output-file
    with-input-from-file with-output-to-file
    open-input-file open-output-file
    open-binary-input-file open-binary-output-file))
