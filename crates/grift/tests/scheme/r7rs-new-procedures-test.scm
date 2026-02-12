;;; Tests for newly implemented R7RS procedures
;;; Migrated from r7rs_new_procedures_tests.rs

(test-begin "r7rs-new-procedures")

;; ============================================================================
;; Time Procedures (R7RS §6.13.3)
;; ============================================================================

;; current-second
(test-assert "current-second-is-inexact" (inexact? (current-second)))
(test-assert "current-second-is-positive" (> (current-second) 0))
(test-assert "current-second-is-number" (number? (current-second)))
(test-assert "current-second-after-2020" (> (current-second) 1577836800))

;; current-jiffy
(test-assert "current-jiffy-is-exact" (exact? (current-jiffy)))
(test-assert "current-jiffy-is-positive" (> (current-jiffy) 0))
(test-assert "current-jiffy-monotonic"
  (let ((j1 (current-jiffy))) (<= j1 (current-jiffy))))

;; jiffies-per-second
(test-assert "jiffies-per-second-is-exact" (exact? (jiffies-per-second)))
(test-assert "jiffies-per-second-is-positive" (> (jiffies-per-second) 0))
(test-equal "jiffies-per-second-value" 1000000000 (jiffies-per-second))

;; ============================================================================
;; Error Predicates (R7RS §6.11)
;; ============================================================================

;; read-error? returns false for non-error values
(test-assert "read-error?-number" (not (read-error? 42)))
(test-assert "read-error?-string" (not (read-error? "hello")))
(test-assert "read-error?-boolean" (not (read-error? #t)))
(test-assert "read-error?-empty-list" (not (read-error? '())))

;; file-error? returns false for non-error values
(test-assert "file-error?-number" (not (file-error? 42)))
(test-assert "file-error?-string" (not (file-error? "hello")))
(test-assert "file-error?-boolean" (not (file-error? #t)))
(test-assert "file-error?-empty-list" (not (file-error? '())))

;; read-error? returns false for regular error
(test-assert "read-error?-false-for-regular-error"
  (not (guard (e (#t (read-error? e))) (error "test" 1))))

;; file-error? recognizes file errors
(test-assert "file-error?-recognizes-file-error"
  (guard (e (#t (file-error? e)))
    (open-input-file "/tmp/grift_nonexistent_12345.txt")))

;; read-error? recognizes parse errors
(test-assert "read-error?-recognizes-parse-error"
  (guard (e (#t (read-error? e)))
    (read (open-input-string ")"))))

;; file error is not a read error
(test-assert "file-error-not-read-error"
  (not (guard (e (#t (read-error? e)))
    (open-input-file "/tmp/grift_nonexistent_12345.txt"))))

;; read error is not a file error
(test-assert "read-error-not-file-error"
  (not (guard (e (#t (file-error? e)))
    (read (open-input-string ")")))))

;; delete-file on nonexistent is file-error
(test-assert "delete-file-nonexistent-is-file-error"
  (guard (e (#t (file-error? e)))
    (delete-file "/tmp/grift_nonexistent_12345.txt")))

;; error predicates return false for each other
(test-assert "error-predicates-mutually-exclusive"
  (guard (e (#t (and (error-object? e)
                     (not (read-error? e))
                     (not (file-error? e)))))
    (error "test")))

;; ============================================================================
;; Vector-String Conversion (R7RS §6.8)
;; ============================================================================

(test-equal "vector->string-basic" "hello"
  (vector->string #(#\h #\e #\l #\l #\o)))

(test-equal "vector->string-empty" ""
  (vector->string #()))

(test-equal "vector->string-with-start" "bcd"
  (vector->string #(#\a #\b #\c #\d) 1))

(test-equal "vector->string-with-start-end" "bc"
  (vector->string #(#\a #\b #\c #\d) 1 3))

(test-equal "string->vector-basic" #(#\h #\e #\l #\l #\o)
  (string->vector "hello"))

(test-equal "string->vector-empty" #()
  (string->vector ""))

(test-equal "string->vector-with-start-end" #(#\b #\c)
  (string->vector "abcd" 1 3))

(test-equal "vector->string-roundtrip" "hello"
  (vector->string (string->vector "hello")))

(test-equal "string->vector-roundtrip" #(#\a #\b #\c)
  (string->vector (vector->string #(#\a #\b #\c))))

;; vector->string type errors
(test-error "vector->string-non-char-elements" (lambda () (vector->string #(1 2 3))))
(test-error "vector->string-not-a-vector" (lambda () (vector->string "hello")))
(test-error "string->vector-not-a-string" (lambda () (string->vector 42)))

;; ============================================================================
;; Include Forms (R7RS §4.1.7)
;; ============================================================================

;; Note: include tests require file I/O and are environment-dependent.
;; The basic behavior is tested here with a missing file error.
(test-error "include-missing-file"
  (lambda () (include "/tmp/grift_nonexistent_file_12345.scm")))

;; ============================================================================
;; Features Procedure (R7RS §6.13.3)
;; ============================================================================

(test-assert "features-is-list" (list? (features)))
(test-assert "features-contains-r7rs" (pair? (memq 'r7rs (features))))
(test-assert "features-contains-grift" (pair? (memq 'grift (features))))

(test-end)
