;;; I/O operation tests
;;; Migrated from io_tests.rs

(test-begin "io")

;; ============================================================================
;; String port operations (R7RS §6.13.2)
;; ============================================================================

(test-equal "write-string-to-string-port" "hello"
  (let ((p (open-output-string)))
    (write-string "hello" p)
    (get-output-string p)))

(test-equal "write-string-with-start-end" "ell"
  (let ((p (open-output-string)))
    (write-string "hello" p 1 4)
    (get-output-string p)))

;; ============================================================================
;; Bytevector port operations (R7RS §6.13.2)
;; ============================================================================

(test-equal "open-input-bytevector" 30
  (let ((p (open-input-bytevector #u8(10 20 30))))
    (let ((a (read-u8 p))
          (b (read-u8 p)))
      (+ a b))))

(test-equal "open-output-bytevector-length" 3
  (let ((p (open-output-bytevector)))
    (write-u8 1 p)
    (write-u8 2 p)
    (write-u8 3 p)
    (bytevector-length (get-output-bytevector p))))

(test-equal "get-output-bytevector-contents" 65
  (let ((p (open-output-bytevector)))
    (write-u8 65 p)
    (write-u8 66 p)
    (bytevector-u8-ref (get-output-bytevector p) 0)))

;; ============================================================================
;; Port predicates (R7RS §6.13.2)
;; Test multiple predicates per port to minimize port allocations
;; ============================================================================

(test-equal "bytevector-input-port-predicates" (list #t #t #f #f)
  (let ((p (open-input-bytevector #u8(1 2 3))))
    (list (binary-port? p) (input-port? p)
          (output-port? p) (textual-port? p))))

(test-equal "bytevector-output-port-predicates" (list #t #f #t)
  (let ((p (open-output-bytevector)))
    (list (binary-port? p) (input-port? p) (output-port? p))))

;; ============================================================================
;; u8-ready? (R7RS §6.13.2)
;; ============================================================================

(test-assert "u8-ready?-bytevector-port"
  (u8-ready? (open-input-bytevector #u8(1 2 3))))

;; ============================================================================
;; File port operations (R7RS §6.13.2)
;; Each file open consumes 2 port slots (string port + file port)
;; ============================================================================

;; Write then read back text file
(test-equal "open-output-file-and-read-back" #\A
  (let ((path "/tmp/grift-io-test-output.txt"))
    (let ((p (open-output-file path)))
      (write-char #\A p)
      (write-char #\B p)
      (close-port p))
    (let ((p (open-input-file path)))
      (let ((c (read-char p)))
        (close-port p)
        c))))

;; Nonexistent file raises error
(test-error "open-input-file-nonexistent"
  (lambda () (open-input-file "/tmp/grift-io-test-nonexistent_file_xyz_999.txt")))

;; Write then read back binary file
(test-equal "open-binary-output-and-read-back" 65
  (let ((path "/tmp/grift-io-test-binary.bin"))
    (let ((p (open-binary-output-file path)))
      (write-u8 65 p)
      (write-u8 66 p)
      (close-port p))
    (let ((p (open-binary-input-file path)))
      (let ((b (read-u8 p)))
        (close-port p)
        b))))

;; call-with-output-file / call-with-input-file
(test-equal "call-with-output-file-and-read-back" "hello"
  (let ((path "/tmp/grift-io-test-call-output.txt"))
    (call-with-output-file path (lambda (p) (write-string "hello" p)))
    (call-with-input-file path (lambda (p) (read-line p)))))

;; with-output-to-file / with-input-from-file
(test-equal "with-output-to-file-and-read-back" "world"
  (let ((path "/tmp/grift-io-test-with-output.txt"))
    (with-output-to-file path (lambda () (write-string "world")))
    (with-input-from-file path (lambda () (read-line)))))

;; ============================================================================
;; Binary I/O: read-u8, peek-u8, eof, write-u8 (R7RS §6.13.2)
;; ============================================================================

;; peek-u8 and read-u8
(test-equal "read-u8-and-peek-u8" (list 10 10 20)
  (let ((path "/tmp/grift-io-test-peek-u8.bin"))
    (let ((p (open-binary-output-file path)))
      (write-u8 10 p)
      (write-u8 20 p)
      (write-u8 30 p)
      (close-port p))
    (let ((p (open-binary-input-file path)))
      (let ((peeked (peek-u8 p)))
        (let ((first (read-u8 p)))
          (let ((second (read-u8 p)))
            (close-port p)
            (list peeked first second)))))))

;; eof detection
(test-assert "read-u8-eof"
  (let ((path "/tmp/grift-io-test-u8-eof.bin"))
    (let ((p (open-binary-output-file path)))
      (write-u8 42 p)
      (close-port p))
    (let ((p (open-binary-input-file path)))
      (read-u8 p)
      (let ((result (eof-object? (read-u8 p))))
        (close-port p)
        result))))

;; write-u8 round-trip
(test-equal "write-u8-read-back" (list 255 0)
  (let ((path "/tmp/grift-io-test-write-u8.bin"))
    (let ((p (open-binary-output-file path)))
      (write-u8 255 p)
      (write-u8 0 p)
      (close-port p))
    (let ((p (open-binary-input-file path)))
      (let ((a (read-u8 p))
            (b (read-u8 p)))
        (close-port p)
        (list a b)))))

;; read-bytevector and write-bytevector
(test-equal "write-and-read-bytevector" (list 3 10 20 30)
  (let ((path "/tmp/grift-io-test-bv.bin"))
    (let ((p (open-binary-output-file path)))
      (write-bytevector #u8(10 20 30) p)
      (close-port p))
    (let ((p (open-binary-input-file path)))
      (let ((bv (read-bytevector 3 p)))
        (close-port p)
        (list (bytevector-length bv)
              (bytevector-u8-ref bv 0)
              (bytevector-u8-ref bv 1)
              (bytevector-u8-ref bv 2))))))

;; ============================================================================
;; flush-output-port, file port predicates, open/close (R7RS §6.13.2)
;; Combined to minimize port allocations
;; ============================================================================

(test-equal "flush-output-port" "flushed"
  (let ((path "/tmp/grift-io-test-flush.txt"))
    (let ((p (open-output-file path)))
      (write-string "flushed" p)
      (flush-output-port p)
      (close-port p))
    (let ((p (open-input-file path)))
      (let ((s (read-line p)))
        (close-port p)
        s))))

;; File port predicates: textual input
(test-equal "file-input-port-predicates" (list #t #t #f #t #f)
  (let ((p (open-input-file "/tmp/grift-io-test-flush.txt")))
    (let ((r (list (port? p) (input-port? p) (output-port? p)
                   (textual-port? p) (binary-port? p))))
      (close-port p)
      r)))

;; File port predicates: binary input
(test-equal "file-binary-input-port-predicates" (list #t #t #f #f #t)
  (let ((p (open-binary-input-file "/tmp/grift-io-test-flush.txt")))
    (let ((r (list (port? p) (input-port? p) (output-port? p)
                   (textual-port? p) (binary-port? p))))
      (close-port p)
      r)))

;; File port open/close state
(test-equal "file-port-open-close" (list #t #f)
  (let ((p (open-input-file "/tmp/grift-io-test-flush.txt")))
    (let ((before (input-port-open? p)))
      (close-port p)
      (let ((after (input-port-open? p)))
        (list before after)))))

;; ============================================================================
;; read-bytevector! (R7RS §6.13.2)
;; ============================================================================

(test-equal "read-bytevector!" (list 5 10 20)
  (let ((path "/tmp/grift-io-test-bv-bang.bin"))
    (let ((p (open-binary-output-file path)))
      (write-bytevector #u8(10 20 30 40 50) p)
      (close-port p))
    (let ((bv (make-bytevector 5 0))
          (p (open-binary-input-file path)))
      (let ((n (read-bytevector! bv p)))
        (close-port p)
        (list n (bytevector-u8-ref bv 0) (bytevector-u8-ref bv 1))))))

;; ============================================================================
;; Large data tests (validates removal of fixed-size stack buffer limits)
;; ============================================================================

(test-equal "open-input-string-large" 2000
  (let* ((s (make-string 2000 #\x))
         (p (open-input-string s))
         (line (read-line p)))
    (close-port p)
    (string-length line)))

(test-equal "read-line-large" 3000
  (let* ((s (string-append (make-string 3000 #\a) "\n" "extra"))
         (p (open-input-string s))
         (line (read-line p)))
    (close-port p)
    (string-length line)))

(test-equal "read-string-large" 3000
  (let* ((s (make-string 3000 #\b))
         (p (open-input-string s))
         (result (read-string 3000 p)))
    (close-port p)
    (string-length result)))

(test-end)
