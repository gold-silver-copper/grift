(test-begin "r7rs-new-procedures")

;; Time Procedures
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

(test-end)
