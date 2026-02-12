(test-begin "system")

;; file-exists?
(test-assert "file-exists-true" (file-exists? "Cargo.toml"))
(test-assert "file-exists-false" (not (file-exists? "nonexistent_file_12345.txt")))

;; delete-file
(test-assert "delete-file"
  (let ((path "/tmp/grift-system-test-delete.txt"))
    (call-with-output-file path
      (lambda (port) (display "temp" port)))
    (delete-file path)
    (not (file-exists? path))))

(test-error "delete-file-nonexistent"
  (lambda () (delete-file "nonexistent_file_xyz_12345.txt")))

;; load
(test-equal "load-file" 42
  (let ((path "/tmp/grift-system-test-load.scm"))
    (call-with-output-file path
      (lambda (port) (display "(define grift-load-test-var 42)" port)))
    (load path)
    grift-load-test-var))

(test-equal "load-file-multiple-expressions" 30
  (let ((path "/tmp/grift-system-test-load-multi.scm"))
    (call-with-output-file path
      (lambda (port)
        (display "(define x-load-test 10)" port)
        (newline port)
        (display "(define y-load-test 20)" port)
        (newline port)
        (display "(define z-load-test (+ x-load-test y-load-test))" port)))
    (load path)
    z-load-test))

(test-error "load-nonexistent-file"
  (lambda () (load "nonexistent_file_xyz_12345.scm")))

;; command-line
(test-assert "command-line-returns-list" (pair? (command-line)))
(test-assert "command-line-car-is-string" (string? (car (command-line))))

;; get-environment-variable
(test-assert "get-environment-variable-exists"
  (string? (get-environment-variable "PATH")))
(test-assert "get-environment-variable-not-exists"
  (not (get-environment-variable "GRIFT_NONEXISTENT_VAR_XYZ_12345")))

;; get-environment-variables
(test-assert "get-environment-variables-returns-list"
  (pair? (get-environment-variables)))
(test-assert "get-environment-variables-car-is-pair"
  (pair? (car (get-environment-variables))))
(test-assert "get-environment-variables-car-car-is-string"
  (string? (car (car (get-environment-variables)))))
(test-assert "get-environment-variables-car-cdr-is-string"
  (string? (cdr (car (get-environment-variables)))))

(test-end)
