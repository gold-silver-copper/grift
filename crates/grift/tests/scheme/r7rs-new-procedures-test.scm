(test-begin "r7rs-debug")

(test-equal "environment displays as #<environment>"
  "#<environment>"
  (let ((port (open-output-string)))
    (display (interaction-environment) port)
    (get-output-string port)))

(test-end)
