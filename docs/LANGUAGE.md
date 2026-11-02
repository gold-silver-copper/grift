# The grift language

The reference: every value, primitive, library form and error kind. [`VISION.md`](VISION.md) says
what grift is and why; [`DECISIONS.md`](../DECISIONS.md) records each decision. grift follows the
Revised⁻¹ Report on the Kernel Programming Language (R-1RK, [05-07.pdf](../05-07.pdf)) in its
evaluation model, with grift's own names and a smaller set of combiners; section numbers below
are R-1RK's.

## Names

[VISION "Names"](VISION.md#names) gives the rules. Kernel's names against grift's:

| Kernel | grift |
|---|---|
| `$vau` `$lambda` `$define!` `$quote` `$sequence` | `vau!` `lambda!` `define!` `quote!` `sequence!` |
| `$if`, `$when`, `$cond` | `if!` (both branches), `when!` (one branch, a body), `conditional!` |
| `$let`, `$let*`, `$letrec*` | `let!` (binds in order in one new context) |
| `$and?` `$or?` | `and!?` `or!?` |
| `$set!` | `bind`, with the parameters quoted |
| `$remote-eval` | `evaluate-in!` |
| `$binds?` | `binds?`, for one symbol, with both operands evaluated |
| `$unwind-protect`, `guard` | `finally!`, `catch!` |
| `eval` `make-environment` `get-current-environment` | `evaluate` `context` `current-context` |
| `cons` `car` `cdr` `null?` | `pair` `first` `rest` `empty-list?` |
| `eq?` `equal?` `=?` | `same?` `equal?` |
| `+` `-` `*` `div` `mod` | `add` `subtract` `multiply` `divide` `remainder` |
| `<?` and the rest | `less-than?` `greater-than?` `less-than-or-equal?` `greater-than-or-equal?` |
| `min` `max` `assoc` | `minimum` `maximum` `find-association` |
| `#t` `#f` | `#true` `#false` |
| environment, ground environment, formals | context, built-in context, parameters |

## Evaluation

- A symbol evaluates to its binding, looked up through the context and its parents; an unbound
  symbol is the error `unbound-symbol` carrying the symbol.
- A pair `(c . operands)` is a combination. `c` is evaluated to a combiner. An **operative**
  receives the operands unevaluated, with the caller's context. An **applicative** evaluates the
  operands left to right and passes the list of results to the combiner it wraps.
- Everything else evaluates to itself, `()` included.
- A compound operative's body, `evaluate`, and a `catch!`'s handler are evaluated as tail calls:
  a loop through `if!`, `when!`, `sequence!`, `conditional!`, `let!` or `lambda!` bodies runs in
  constant space.
- There is no truthiness: the arguments of `not?` and `select`, like every test, must be booleans.
- There are no limits on time, depth or memory but the allocator's, the OS's and the host's: it can
  interrupt an evaluation (`interrupted`).

## Values

| Value | Written | `type-of` |
|---|---|---|
| the empty list | `()` | `empty-list` |
| booleans | `#true` `#false` | `boolean` |
| inert, the value of a form with nothing useful to return | `#inert` | `inert` |
| ignore, the parameter that binds nothing | `#ignore` | `ignore` |
| integers, 64-bit; overflow is an error | `42` `-7` | `integer` |
| floats, 64-bit IEEE | `1.5` `-0.0` `1e300` `#infinity` `#-infinity` `#not-a-number` | `float` |
| symbols, interned, case-sensitive | `foo` `if!` `less-than?` | `symbol` |
| pairs, immutable | `(1 . 2)` `(1 2 3)` | `pair` |
| strings, immutable, addressed by byte; escapes `\\` `\"` `\n` `\t` and `\xHEX;` (a Unicode scalar value) | `"hi\x1b;"` | `string` |
| contexts | `#[context a b]` (its own names, sorted) | `context` |
| operatives | `#[operative (x) caller body]`, `#[primitive vau!]`, `#[host name]` | `operative` |
| applicatives | `#[applicative #[operative (x) #ignore (multiply x x)]]` | `applicative` |

A number starts with a digit, after an optional sign: it is an integer if it is one, else a float
if it has a `.` or an exponent, as Rust reads them (`1.`, `2e5`); anything else that starts so,
such as `99999999999999999999` or `1.2.3`, is a `parse-error`. A float prints in Rust's shortest
form that reads back (`{:?}`: `1.0`, `0.1`, `1e300`), so it reads back as the same float, bit
for bit. Every not-a-number prints as `#not-a-number`, which reads back as Rust's `f64::NAN`: a
not-a-number's sign and payload don't survive printing.
`;` starts a comment to the end of the line, and so does `#!` at the very start of a text.

## Primitives (Rust)

All are applicatives except `vau!`.

| Combiner | Meaning |
|---|---|
| `(vau! parameters context-parameter body)` | An operative closing over the current context (4.10.3). `parameters` is a tree of symbols, `#ignore`, `()` and pairs, with no symbol twice; `context-parameter` is a symbol not in it, or `#ignore`. Checked when the operative is made (`parameter-error`). A call binds them in a new child of that context and evaluates `body` as a tail call. |
| `(wrap c)`, `(unwrap a)` | The applicative around combiner `c`; the combiner inside applicative `a` (4.10.4–5). Wrapping an applicative wraps it again. |
| `(evaluate x c)` | Evaluates `x` in context `c`, as a tail call (4.8.3). |
| `(context)`, `(context parent)` | A new empty context, with no parent or one (4.8.4). |
| `(bind c parameters value)` | Matches the tree `parameters` to `value` and binds its symbols in context `c` itself, replacing bindings there. Returns `#inert`. The matching of `define!` (4.9.1) as an applicative. |
| `(names c)` | The symbols bound in context `c` itself (not its parents), sorted by name. |
| `(pair a b)`, `(first p)`, `(rest p)` | A new pair; its two halves (4.6.3, 5.4.1). |
| `(same? a b)` | Identity for pairs, contexts and combiners; equality of value for integers, symbols and strings; the same stored bits for floats. Values of different types are never the same (4.2.1, `eq?`). |
| `(select b x y)` | `x` if `b` is `#true`, `y` if `#false`; a `type-error` otherwise. Both are evaluated; `if!` uses it to pick an expression. |
| `(type-of v)` | The symbol naming `v`'s type, as in the table above. |
| `(add a b)`, `(subtract a b)`, `(multiply a b)`, `(divide a b)`, `(remainder a b)` | Of two integers or two floats; mixing them is a `type-error`. On integers, Rust's checked arithmetic: `divide` and `remainder` are Rust's `/` and `%`, truncating toward zero, so `(remainder -7 2)` is `-1`; dividing by 0 is `division-by-zero`, and a result beyond 64 bits, `(divide -9223372036854775808 -1)` included, is `overflow`. On floats, IEEE: `(divide 1.0 0.0)` is `#infinity`. |
| `(less-than? a b)` | Whether `a < b`, of two integers or two floats; with `#not-a-number`, `#false`. |
| `(square-root x)` `(sine x)` `(cosine x)` `(tangent x)` `(exponential x)` `(natural-logarithm x)` `(floor x)` `(ceiling x)` `(round x)` `(truncate x)` `(absolute-value x)`, `(power x y)` | Rust `std`'s `sqrt` `sin` `cos` `tan` `exp` `ln` `floor` `ceil` `round` (half away from zero) `trunc` `abs` and `powf`, of floats only. Results may differ slightly between platforms, as in Rust. |
| `(integer-to-float n)`, `(float-to-integer x)` | Rust's `as`: the nearest float; the integer part, saturating at the ends of the 64-bit range, and 0 for `#not-a-number`. |
| `(evaluate-with-cleanup x c cleanup)` | Evaluates `x` in `c`; when that ends, with a value or an error, calls the combiner `cleanup` with no arguments, then returns the value or lets the error go on. An error in `cleanup` replaces the one leaving, unless that one is `interrupted` or `out-of-memory`, which go on. Not a tail call. |
| `(call-with-handler thunk handler)` | Calls `thunk` with no arguments and returns its value. If an error leaves it, the cleanups inside run first, then `handler` is called with the error as a new context with no parent, binding `kind` (a symbol), `values` (a list) and `trace` (a list, see Errors); its value is `call-with-handler`'s, as a tail call. `interrupted` and `out-of-memory` are never caught. |
| `(raise e)` | Raises the error `e`, a context binding `kind`, `values` and `trace` as `error` makes and a handler gets. A new error (an empty trace) happens at the `raise`; a caught one keeps its trace, with nothing added for the `raise` itself. Like any error, it then gains the combinations it passes on its way out. Anything else is a `type-error`. |
| `(string-byte-length s)`, `(string-byte s i)` | The length of string `s` in bytes of UTF-8; the byte at offset `i`, an integer 0–255. Both O(1). |
| `(byte-substring s start end)` | The string of the bytes of `s` from offset `start` to `end`; a `range-error` unless both lie on character boundaries, in order. |
| `(string-append s ...)` | The strings joined. |
| `(code-point-to-string n)` | The string of the one character whose Unicode code point is `n`; a `range-error` for a surrogate or past `#x10FFFF`. |
| `(string-to-symbol s)` | The symbol named `s`, interned. |
| `(integer-to-string n)`, `(string-to-integer s)` | Rust's `to_string` and `parse::<i64>`: `"-42"`, and back. A string that isn't an integer in range is a `parse-error`. |
| `(string-to-float s)` | Rust's `parse::<f64>` and the three `#` names, so it reads what `value-to-string` writes for a float: `(string-to-float "1")` is `1.0`, `(string-to-float "inf")` is `#infinity`. A not-a-number's sign and payload are lost. A string that reads as neither is a `parse-error`. |
| `(read-form text offset)` | The program reader, on one form of string `text`: the first form after byte `offset`, as the list `(form start end)`, where `byte-substring` from `start` to `end` is the form's own text, without the space and comments before it; `()` if only those are left, so a form that is `()` gives `(() start end)`. It evaluates nothing. Text that doesn't read is a `parse-error` with the line and column in the whole text; an offset out of range or off a character boundary, a `range-error`. Reading a long text form by form costs nothing per call for its length. |
| `(value-to-string v)` | The text the printer writes for `v`, combiners and contexts as they print (see Values): `(value-to-string (list 1.0 "a"))` is `"(1.0 \"a\")"`, and a float's text reads back as the same float. |

## The library (grift, [`library.grift`](../crates/grift/src/library.grift))

| Combiner | Meaning |
|---|---|
| `(define! parameters expression)` | Evaluates `expression` and matches `parameters` to it in the current context (4.9.1). Returns `#inert`. |
| `(quote! x)` | `x`, unevaluated. |
| `(list x ...)` | The list of the arguments (5.2.1). |
| `(if! test consequent alternative)` | Evaluates `test`, which must give a boolean, then `consequent` or `alternative` as a tail call (4.5.2). Any other number of operands is an `arity-error`. |
| `(sequence! x ...)` | Each in turn; the last one's value, as a tail call; `#inert` for none (5.1.1). |
| `(lambda! parameters body ...)` | `(wrap (vau! parameters #ignore body))`; several body forms become `(sequence! form ...)`, with the symbol, so the combiner prints its body as written (5.3.2). |
| `(current-context)` | The context it is called in (6.7.2). |
| `(evaluate-in! o c)` | Evaluates `o`, as a tail call, in the context `c` evaluates to (6.7.9); `(evaluate-in! kind e)` reads a field of a record such as an error, `((evaluate-in! write f) line)` calls an operation a context holds, with `line` evaluated where you are. |
| `(not? b)`, `(and!? x ...)`, `(or!? x ...)` | Booleans (6.1). `and!?` and `or!?` evaluate left to right and stop early; every operand they evaluate must give a boolean, the last one included, so it isn't a tail call. |
| `(empty-list? x)` `(boolean? x)` `(integer? x)` `(float? x)` `(symbol? x)` `(pair? x)` `(string? x)` `(context? x)` `(operative? x)` `(applicative? x)` | A predicate for each type `type-of` gives (4.4.1, 4.6.1, 4.6.2, 4.8.1, 12.5.1), but `#inert` and `#ignore`, each the only value of its type: `(same? x #inert)`. |
| `(apply a arguments)` | Calls applicative `a`'s combiner on the list `arguments`, in a new empty context (5.5.1). |
| `(map a list ...)`, `(for-each a list)` | The list of `a` applied to each element, or with several lists to the elements at each place, the lists being of one length (a `type-error` otherwise); `a` on each element in turn, giving `#inert` (5.9.1, 6.9.1). |
| `(any? a list)`, `(all? a list)` | Whether `a` gives `#true` for some element, for every element, from the left, stopping at the answer. `a` must give booleans. |
| `(filter a list)` | The elements for which `a` gives `#true`, in order; `a` must give booleans. |
| `(fold a value list)` | `a` on the value so far and each element in turn, from the left, starting from `value`: `(fold add 0 list)` is a sum. Rust's `Iterator::fold`. |
| `(range start end)` | The list of the integers from `start` up to, not including, `end`, as Rust's `start..end`; `()` if `start` isn't below `end`. |
| `(when! test body ...)` | Evaluates `test`, which must give a boolean; if it is `#true`, the body forms in turn, the last as a tail call, and its value; if `#false`, `#inert`. |
| `(conditional! (test body ...) ...)` | The body of the first clause whose test is `#true`; `#inert` if none is, or if its body is empty (5.6.1). A clause that isn't a pair `(test . body)` is an `arity-error`; a body is checked only when it runs. |
| `(let! ((parameters expression) ...) body ...)` | Makes one new child of the caller's context; each binding in order evaluates its expression there and binds its parameters there, so it sees the bindings before it, and a combiner it binds can call itself and the ones after it; then the body (R-1RK's `$letrec*`, 6.7.5). A binding that isn't `(parameters expression)` is an `arity-error`. |
| `(finally! body cleanup ...)` | The body's value, after the cleanup forms ran; they also run when an error leaves the body, and the error then goes on unchanged, to a `catch!` or the host. After `out-of-memory` a cleanup stops at its first allocation that fails, so it may do nothing. |
| `(catch! body e handler ...)` | The body's value. If an error leaves it, the cleanups inside run first, then the handler forms, with the parameter `e` bound to the error in a new child of the caller's context; the last one's value is `catch!`'s. `interrupted` and `out-of-memory` are never caught. |
| `(error kind value ...)` | A new error: a context with no parent binding `kind`, `values` (the list of the values) and an empty `trace`. It raises nothing; `raise` does. |
| `(length list)`, `(append list ...)`, `(reverse list)` | List length; concatenation, sharing the last list; the elements in the opposite order (6.3.1, 6.3.3). |
| `(equal? a b)` | Pairs by structure, floats as IEEE does (`(equal? 0.0 -0.0)` is `#true`, `#not-a-number` is equal to nothing), everything else as `same?`. Values of different types are never equal: `(equal? 1 1.0)` is `#false` (6.6.1). |
| `(greater-than? a b)`, `(less-than-or-equal? a b)`, `(greater-than-or-equal? a b)` | The rest of the comparisons, as Rust's `>`, `<=`, `>=` (12.5.1). |
| `(minimum a b)`, `(maximum a b)` | The smaller, the larger, of two integers or two floats (12.5.13). |
| `(find-association x pairs)`, `(member? x list)` | The first pair whose `first` is `equal?` to `x`, or `()`; whether `x` is `equal?` to an element (6.3.6, 6.3.7). |
| `(binds? c symbol)` | Whether the symbol is bound in context `c` or one of its parents (6.7.1), with both evaluated, so the symbol can be computed: `(binds? table (string-to-symbol key))`. |
| `(symbol-to-string s)` | A symbol's name. |
| `(string-search text pattern start)` | The byte offset of the first place at or after `start` where `pattern` is in `text`, or `()`; an empty pattern is at `start`. A `start` out of range or off a character boundary is a `range-error`. |
| `(string-starts-with? s prefix)`, `(string-ends-with? s suffix)` | Whether `s` starts or ends with those bytes. |
| `(string-split text separator)`, `(string-join strings separator)` | The parts of `text` between the separators, empty ones too, so `(string-split "a,,b" ",")` is `("a" "" "b")` and `(string-split "" ",")` is `("")`; an empty separator is a `range-error`. The strings with the separator between them. `string-join` undoes `string-split`. |
| `(string-trim s)` | `s` without the ASCII whitespace at either end (space, tab, newline, form feed, return), as Rust's `trim_ascii`. |
| `(string-pad-left s width)`, `(string-pad-right s width)` | `s` with spaces before it, after it, up to `width` bytes; `s` itself if it is that long already. |
| `(string-to-lowercase s)` | `s` with `A` to `Z` made `a` to `z` and every other byte as it is, as Rust's `to_ascii_lowercase`. |
| `built-in-names` | The list of every name above and every primitive's, bound when the library is loaded. |

Programs run in the **top-level context**, a child of the **built-in context**, which is out of
reach. Defining a name at top level shadows the library's binding without changing what the library
itself uses.

## Errors

Every error stops evaluation and goes to the innermost `catch!` around it, or to the host, as a
kind, the values involved, and a trace: the combinations of the program being evaluated,
innermost first, each once, out to the top-level form (the library's own code and code built
around a combiner value are skipped). A combination in tail position has finished when the next
one starts, so it isn't listed. Cleanups (`finally!`) run on the way out.

```lisp
(define! check (lambda! (x) (if! (integer? x) x (raise (error (quote! not-integer) x)))))
(define! run (lambda! (x) (add 1 (check x))))
(run (quote! a))   ; not-integer a in (raise (error (quote! not-integer) x))
                   ;   from (add 1 (check x))
                   ;   from (run (quote! a))
```

```lisp
(catch! (run (quote! a)) e (list (evaluate-in! kind e) (evaluate-in! values e)))   ; => (not-integer (a))
(catch! (first 1) e (if! (same? (evaluate-in! kind e) (quote! type-error)) 0 (raise e)))   ; => 0
```

**What can be caught.** Errors about the program can: the primitives', the library's, a host
function's, and the program's own. Limits set by the host or the machine can't: `interrupted`
(the host's) and `out-of-memory` (the allocator's) go on through every `catch!` to the host,
running cleanups on the way (with no memory left, one may stop at once), so a `catch!` in a loop
can't swallow C-g; an error in one of those cleanups doesn't replace them. Whether an error is a limit goes by its kind: a program that raises
`interrupted` itself, or a host function that reports `out-of-memory`, isn't caught either.

**Early exit** is `catch!` and a kind of error of the program's own; there is no other escape:

```lisp
(define! first-even
  (lambda! (xs)
    (catch! (sequence! (for-each (lambda! (x) (when! (same? (remainder x 2) 0) (raise (error (quote! found) x)))) xs)
                       #false)
            e
            (if! (same? (evaluate-in! kind e) (quote! found)) (first (evaluate-in! values e)) (raise e)))))
(first-even (list 1 3 4 5))   ; => 4
```

| Kind | When | Values |
|---|---|---|
| `unbound-symbol` | a symbol with no binding | the symbol |
| `type-error` | a primitive got the wrong kind of value, integers and floats mixed, a test that isn't a boolean; an improper operand list given a primitive or an applicative | the primitive and the value |
| `arity-error` | a primitive got the wrong number of operands; a parameter tree didn't match, as when a library form gets operands that aren't its shape | the primitive and the operands; or the parameters and the value |
| `not-a-combiner` | a combination's head isn't a combiner | the value |
| `parameter-error` | a malformed parameter tree, a symbol twice, or a bad context parameter | the tree or the context parameter |
| `overflow` | integer arithmetic beyond 64 bits | the primitive and the operands |
| `division-by-zero` | integer `divide` or `remainder` by 0 | the primitive and the operands |
| `range-error` | a string offset off a character boundary or out of range; a code point that isn't one; an empty separator given `string-split` | the primitive and the operands; or `string-split` and the separator |
| `parse-error` | text that doesn't read: a program or `read-form`'s text (unfinished text fails at its end), or a string given `string-to-integer` or `string-to-float` | the line and the column, which counts characters from 1; or the primitive and the string |
| `out-of-memory` | the allocator refused | none |
| `stale-value` | the host used a value the collector reclaimed | none |
| `interrupted` | the host set its interrupt flag | none |
| any symbol | a program raised an error it made | the values given |

The runner adds its own operations, `standard-error` among them, and its own kinds;
[the README](../README.md#the-runner) lists them.

## Where grift differs from R-1RK, and why

[VISION "Kernel"](VISION.md#kernel) gives the larger differences. Beyond those:

- **One special primitive.** `if!`, `define!` and `sequence!` are primitive in Kernel; here they
  are library code over `vau!`, `select`, `bind` and `evaluate`.
- **`finally!`** is on `evaluate-with-cleanup` (not in Kernel), the exit half of Kernel's guards;
  `binds?` is derived with `catch!` as R-1RK derives `$binds?` from its error continuation.
- **Binary arithmetic and comparison.** `add`, `multiply`, `less-than?`, `minimum` and the rest
  take two operands; Kernel's take any number. The type predicates take one value.
- **Strings are immutable,** and `same?` compares them by content, like integers. R-1RK has no
  strings; grift's offsets count bytes of UTF-8, so `string-byte` is O(1), and there is no
  character type: a code point is an integer.
- **`apply` has one form,** `(apply a arguments)`, with a new empty context; `for-each` takes one
  list, and `map` lists of one length, as Kernel's does. `context` takes at most one parent.
- **New, not in Kernel:** `bind`, `names` and `built-in-names`, `select`, `type-of`, `when!`, `error`,
  `raise`, `reverse`, `read-form`, `value-to-string`, `any?`, `all?`, `filter`, `fold`, `range`, and
  the string and float primitives and helpers.
- **`lambda!` keeps the symbol `sequence!`** in a body of several forms, so the body prints as
  written. It is looked up where the body runs, like any other name in it.
