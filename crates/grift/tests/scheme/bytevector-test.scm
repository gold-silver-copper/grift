;;; Bytevector operation tests
;;; Migrated from bytevector_tests.rs

(test-begin "bytevector")

;; bytevector? predicate
(test-assert "bytevector-pred-literal" (bytevector? #u8(1 2 3)))
(test-assert "bytevector-pred-make-empty" (bytevector? (make-bytevector 0)))
(test-assert "bytevector-pred-make" (bytevector? (make-bytevector 5)))
(test-assert "bytevector-pred-not-number" (not (bytevector? 42)))
(test-assert "bytevector-pred-not-string" (not (bytevector? "hello")))
(test-assert "bytevector-pred-not-list" (not (bytevector? '(1 2 3))))
(test-assert "bytevector-pred-not-bool" (not (bytevector? #t)))

;; make-bytevector with default fill
(test-equal "make-bytevector-default-fill-0" 0
  (bytevector-u8-ref (make-bytevector 3) 0))
(test-equal "make-bytevector-default-fill-2" 0
  (bytevector-u8-ref (make-bytevector 3) 2))

;; make-bytevector with custom fill
(test-equal "make-bytevector-fill-42-ref-0" 42
  (bytevector-u8-ref (make-bytevector 3 42) 0))
(test-equal "make-bytevector-fill-42-ref-1" 42
  (bytevector-u8-ref (make-bytevector 3 42) 1))
(test-equal "make-bytevector-fill-42-ref-2" 42
  (bytevector-u8-ref (make-bytevector 3 42) 2))

;; make-bytevector fill 255
(test-equal "make-bytevector-fill-255" 255
  (bytevector-u8-ref (make-bytevector 1 255) 0))

;; make-bytevector empty
(test-equal "make-bytevector-empty-length" 0
  (bytevector-length (make-bytevector 0)))

;; bytevector-length
(test-equal "bytevector-length-empty" 0 (bytevector-length #u8()))
(test-equal "bytevector-length-one" 1 (bytevector-length #u8(1)))
(test-equal "bytevector-length-three" 3 (bytevector-length #u8(1 2 3)))
(test-equal "bytevector-length-make" 5 (bytevector-length (make-bytevector 5)))

;; bytevector-u8-ref
(test-equal "bytevector-u8-ref-0" 10 (bytevector-u8-ref #u8(10 20 30) 0))
(test-equal "bytevector-u8-ref-1" 20 (bytevector-u8-ref #u8(10 20 30) 1))
(test-equal "bytevector-u8-ref-2" 30 (bytevector-u8-ref #u8(10 20 30) 2))

;; bytevector-u8-set!
(test-equal "bytevector-u8-set-value" 99
  (let ((bv (make-bytevector 3 0)))
    (bytevector-u8-set! bv 1 99)
    (bytevector-u8-ref bv 1)))
(test-equal "bytevector-u8-set-unchanged-0" 0
  (let ((bv (make-bytevector 3 0)))
    (bytevector-u8-set! bv 1 99)
    (bytevector-u8-ref bv 0)))
(test-equal "bytevector-u8-set-unchanged-2" 0
  (let ((bv (make-bytevector 3 0)))
    (bytevector-u8-set! bv 1 99)
    (bytevector-u8-ref bv 2)))

;; bytevector-copy
(test-equal "bytevector-copy-full" #u8(1 2 3)
  (bytevector-copy #u8(1 2 3)))
(test-equal "bytevector-copy-start" #u8(3 4 5)
  (bytevector-copy #u8(1 2 3 4 5) 2))
(test-equal "bytevector-copy-start-end" #u8(2 3)
  (bytevector-copy #u8(1 2 3 4 5) 1 3))
(test-equal "bytevector-copy-empty-range" #u8()
  (bytevector-copy #u8(1 2 3) 2 2))

;; bytevector-copy independence
(test-equal "bytevector-copy-independence" 1
  (let ((bv #u8(1 2 3))
        (bv2 (bytevector-copy #u8(1 2 3))))
    (bytevector-u8-set! bv2 0 99)
    (bytevector-u8-ref bv 0)))

;; bytevector-append
(test-equal "bytevector-append-two" #u8(1 2 3 4)
  (bytevector-append #u8(1 2) #u8(3 4)))
(test-equal "bytevector-append-three" #u8(1 2 3)
  (bytevector-append #u8(1) #u8(2) #u8(3)))
(test-equal "bytevector-append-empty-first" #u8(1 2)
  (bytevector-append #u8() #u8(1 2)))
(test-equal "bytevector-append-empty-second" #u8(1 2)
  (bytevector-append #u8(1 2) #u8()))
(test-equal "bytevector-append-no-args" #u8()
  (bytevector-append))

;; utf8->string
(test-equal "utf8-to-string-ascii" "hello"
  (utf8->string #u8(104 101 108 108 111)))
(test-equal "utf8-to-string-empty" ""
  (utf8->string #u8()))
(test-equal "utf8-to-string-range" "ell"
  (utf8->string #u8(104 101 108 108 111) 1 4))

;; utf8->string multibyte
(test-equal "utf8-to-string-lambda" "λ"
  (utf8->string #u8(206 187)))

;; string->utf8
(test-equal "string-to-utf8-ascii" #u8(104 101 108 108 111)
  (string->utf8 "hello"))
(test-equal "string-to-utf8-empty" #u8()
  (string->utf8 ""))
(test-equal "string-to-utf8-range" #u8(101 108 108)
  (string->utf8 "hello" 1 4))

;; string->utf8 multibyte round-trip
(test-equal "string-to-utf8-multibyte-roundtrip" #u8(206 187)
  (string->utf8 (utf8->string #u8(206 187))))

;; utf8 round-trip
(test-equal "utf8-roundtrip" "hello"
  (utf8->string (string->utf8 "hello")))

;; bytevector-append no args length
(test-equal "bytevector-append-no-args-length" 0
  (bytevector-length (bytevector-append)))

;; bytevector literal display
(test-equal "bytevector-literal" #u8(0 1 2) #u8(0 1 2))
(test-equal "bytevector-literal-empty" #u8() #u8())

(test-end)
