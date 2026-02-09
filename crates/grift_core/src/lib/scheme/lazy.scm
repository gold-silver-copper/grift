;;; (scheme lazy) — R7RS §4.2.5 Delayed evaluation
(define-library (scheme lazy)
  (export delay force delay-force
          make-promise promise?))
