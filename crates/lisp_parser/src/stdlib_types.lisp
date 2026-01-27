;;; Lisp Standard Library Type Declarations
;;;
;;; This file contains type declarations for the standard library and builtins.
;;; These declarations are used by the bidirectional type checker.
;;;
;;; Type Syntax:
;;;   isize                      - Integer type
;;;   bool                       - Boolean type
;;;   nil                        - Nil/unit type
;;;   char                       - Character type
;;;   (fn param result)          - Single-param function
;;;   (fn (t1 t2 ...) result)    - Multi-param function (curried)
;;;   (list t)                   - Homogeneous list
;;;   (pair t1 t2)               - Pair/cons cell
;;;   (forall (a b ...) type)    - Polymorphic type

;;; ==========================================================================
;;; Arithmetic Operations
;;; ==========================================================================

(declare + (fn (isize isize) isize))
(declare - (fn (isize isize) isize))
(declare * (fn (isize isize) isize))
(declare / (fn (isize isize) isize))
(declare mod (fn (isize isize) isize))

;;; ==========================================================================
;;; Comparison Operations
;;; ==========================================================================

(declare < (fn (isize isize) bool))
(declare > (fn (isize isize) bool))
(declare <= (fn (isize isize) bool))
(declare >= (fn (isize isize) bool))
(declare = (fn (isize isize) bool))

;;; ==========================================================================
;;; Boolean Operations
;;; ==========================================================================

(declare not (fn bool bool))

;;; ==========================================================================
;;; List Operations (Polymorphic)
;;; ==========================================================================

(declare car (forall (a) (fn (list a) a)))
(declare cdr (forall (a) (fn (list a) (list a))))
(declare cons (forall (a) (fn (a (list a)) (list a))))
(declare list (forall (a) (fn a (list a))))
(declare null? (forall (a) (fn (list a) bool)))
(declare pair? (forall (a) (fn a bool)))

;;; ==========================================================================
;;; Type Predicates
;;; ==========================================================================

(declare atom (forall (a) (fn a bool)))
(declare number? (forall (a) (fn a bool)))
(declare boolean? (forall (a) (fn a bool)))
(declare symbol? (forall (a) (fn a bool)))
(declare procedure? (forall (a) (fn a bool)))

;;; ==========================================================================
;;; Equality (Polymorphic)
;;; ==========================================================================

(declare eq (forall (a) (fn (a a) bool)))

;;; ==========================================================================
;;; Standard Library Functions
;;; ==========================================================================

;;; List Processing
(declare map (forall (a b) (fn ((fn a b) (list a)) (list b))))
(declare filter (forall (a) (fn ((fn a bool) (list a)) (list a))))
(declare fold (forall (a b) (fn ((fn (b a) b) b (list a)) b)))
(declare length (forall (a) (fn (list a) isize)))
(declare append (forall (a) (fn ((list a) (list a)) (list a))))
(declare reverse (forall (a) (fn (list a) (list a))))
(declare nth (forall (a) (fn (isize (list a)) a)))
(declare take (forall (a) (fn (isize (list a)) (list a))))
(declare drop (forall (a) (fn (isize (list a)) (list a))))
(declare zip (forall (a b) (fn ((list a) (list b)) (list (pair a b)))))
(declare member (forall (a) (fn (a (list a)) bool)))
(declare assoc (forall (a b) (fn (a (list (pair a b))) (pair a b))))

;;; Numeric
(declare range (fn (isize isize) (list isize)))

;;; Higher-Order Functions
(declare compose (forall (a b c) (fn ((fn b c) (fn a b)) (fn a c))))
(declare identity (forall (a) (fn a a)))
(declare constantly (forall (a b) (fn a (fn b a))))
(declare flip (forall (a b c) (fn ((fn (a b) c)) (fn (b a) c))))
(declare curry (forall (a b c) (fn ((fn (a b) c) a) (fn b c))))

;;; List Accessors
(declare cadr (forall (a) (fn (list a) a)))
(declare caddr (forall (a) (fn (list a) a)))
(declare cddr (forall (a) (fn (list a) (list a))))
