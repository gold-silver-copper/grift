;;; (scheme case-lambda) — R7RS §4.2.9
;;; Re-exports case-lambda from (scheme base) to avoid macro redefinition conflicts.
(define-library (scheme case-lambda)
  (import (only (scheme base) case-lambda))
  (export case-lambda))
