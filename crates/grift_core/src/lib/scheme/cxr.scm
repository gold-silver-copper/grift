;;; (scheme cxr) — R7RS §6.4 caar ... cddddr
(define-library (scheme cxr)
  (export
    caar cadr cdar cddr
    caaar caadr cadar caddr
    cdaar cdadr cddar cdddr
    caaaar caaadr caadar caaddr
    cadaar cadadr caddar cadddr
    cdaaar cdaadr cdadar cdaddr
    cddaar cddadr cdddar cddddr)
  (begin
    ;;; 2-level compositions
    (define (caar lst) (car (car lst)))
    (define (cadr lst) (car (cdr lst)))
    (define (cdar lst) (cdr (car lst)))
    (define (cddr lst) (cdr (cdr lst)))
    ;;; 3-level compositions
    (define (caaar lst) (car (car (car lst))))
    (define (caadr lst) (car (car (cdr lst))))
    (define (cadar lst) (car (cdr (car lst))))
    (define (caddr lst) (car (cdr (cdr lst))))
    (define (cdaar lst) (cdr (car (car lst))))
    (define (cdadr lst) (cdr (car (cdr lst))))
    (define (cddar lst) (cdr (cdr (car lst))))
    (define (cdddr lst) (cdr (cdr (cdr lst))))
    ;;; 4-level compositions
    (define (caaaar lst) (car (car (car (car lst)))))
    (define (caaadr lst) (car (car (car (cdr lst)))))
    (define (caadar lst) (car (car (cdr (car lst)))))
    (define (caaddr lst) (car (car (cdr (cdr lst)))))
    (define (cadaar lst) (car (cdr (car (car lst)))))
    (define (cadadr lst) (car (cdr (car (cdr lst)))))
    (define (caddar lst) (car (cdr (cdr (car lst)))))
    (define (cadddr lst) (car (cdr (cdr (cdr lst)))))
    (define (cdaaar lst) (cdr (car (car (car lst)))))
    (define (cdaadr lst) (cdr (car (car (cdr lst)))))
    (define (cdadar lst) (cdr (car (cdr (car lst)))))
    (define (cdaddr lst) (cdr (car (cdr (cdr lst)))))
    (define (cddaar lst) (cdr (cdr (car (car lst)))))
    (define (cddadr lst) (cdr (cdr (car (cdr lst)))))
    (define (cdddar lst) (cdr (cdr (cdr (car lst)))))
    (define (cddddr lst) (cdr (cdr (cdr (cdr lst)))))))
