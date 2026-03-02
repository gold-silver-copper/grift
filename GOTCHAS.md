# Gotchas

Known sharp edges and surprising interactions in Grift.

## 1. `define!` Rejects Redefinition

`define!` now signals an error if the symbol already exists in the current
frame. Use `set!` to update existing bindings:

```lisp
(define! x 1)
(define! x 2)                     ;; ERROR: x already defined
(define! e (current-environment))
(set! e x 2)                      ;; OK — use set! to update
```

Mutation via `set!` affects all closures that closed over that frame.
This is intentional — the frame is a mutable shared reference:

```lisp
(define! x 1)
(fn! get-x () x)
(get-x)                       ;; => 1
(define! e (current-environment))
(set! e x 2)
(get-x)                       ;; => 2
```

Similarly, redefining a function via `set!` changes the behavior of
everything that calls it:

```lisp
(fn! helper (x) (+ x 1))
(fn! main (x) (helper x))
(main 5)                       ;; => 6
(define! e (current-environment))
(set! e helper (lambda (x) (* x 100)))
(main 5)                       ;; => 500
```

## 2. `set!` Does Not Walk the Parent Chain

A binding visible through the parent chain cannot be modified via `set!`
with the current frame:

```lisp
(define! x 1)
(let ()
  x                                    ;; => 1 (visible via parent)
  (set! (current-environment) x 2))    ;; ERROR: x not in this frame
```

You must capture the frame that contains the binding:

```lisp
(define! x 1)
(define! top (current-environment))
(let ()
  (set! top x 2))                     ;; OK
x                                      ;; => 2
```

## 3. `set!` Without Environment Gives Cryptic Error

Scheme users may write:

```lisp
(set! x 2)    ;; evaluates x (gets 1), checks if 1 is an environment,
              ;; gets TypeError
```

The correct form requires an explicit environment argument:

```lisp
(define! x 1)
(define! e (current-environment))
(set! e x 2)
x                                      ;; => 2
```

## 4. `fn!` Is a Separate Form

`fn!` is a standalone operative for defining named functions. It is not
part of `define!`. A pair in `define!`'s definiend is always ptree
destructuring:

```lisp
;; define! with pair: destructuring
(define! (x y) (list 10 20))
x                                     ;; => 10
y                                     ;; => 20

;; fn! for function definition
(fn! x (y) (list 10 20))
(x 99)                                ;; => (10 20)
```

`fn` can still be used as a regular variable name:

```lisp
(define! fn 42)                        ;; binds the symbol fn to 42
fn                                     ;; => 42
(fn! fn (x) (+ x 1))                  ;; defines a function NAMED fn
(fn 5)                                 ;; => 6 — fn is now a function
```

This is legal but confusing.

## 5. Named `let` Creates a Local Scope

The function defined by named `let` is only visible inside the `let` body.
It cannot be called from outside:

```lisp
(let loop ((i 0))
  (if (< i 5) (loop (+ i 1)) i))      ;; => 5
loop                                    ;; ERROR: UnboundVariable
```

## 6. `current-environment` Returns a Mutable Reference

`current-environment` returns the actual live environment, not a snapshot.
Bindings created after the call are visible through the captured reference:

```lisp
(define! e (current-environment))
(define! x 42)
(eval (quote x) e)                     ;; => 42
```
