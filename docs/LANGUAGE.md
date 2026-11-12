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
| `$if`, `$cond` | `if!` (both branches), `conditional!`; `when!` (one branch, a body) is new |
| `$let`, `$let*`, `$letrec*` | `let!` (binds in order in one new context) |
| `$and?` `$or?` | `and!?` `or!?` |
| `$set!` | `bind`, with the parameters quoted |
| `$remote-eval` | `evaluate-in!` |
| `$binds?` | `binds?`, for one symbol, with both operands evaluated; `lookup` |
| `guard-dynamic-extent` | `finally!`, `catch!` |
| `eval` `make-environment` `get-current-environment` | `evaluate` `context` `current-context` |
| `cons` `car` `cdr` `null?` | `pair` `first` `rest` `empty-list?` |
| `eq?` `equal?` `=?` | `same?` `equal?` |
| `+` `-` `*` `div` `mod` `abs` `expt` | `add` `subtract` `multiply` `divide` `remainder` `absolute-value` `power` |
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
| strings, immutable, addressed by byte; escapes `\\` `\"` `\n` `\r` `\t` and `\xHEX;` (a Unicode scalar value) | `"hi\x1b;"` | `string` |
| bytes, immutable, any bytes, addressed by offset; a byte is an integer 0–255 | `#bytes(104 105 255)` `#bytes()` | `bytes` |
| contexts | `#[context a b]` (its own names, sorted) | `context` |
| operatives | `#[operative (x) caller body]`, `#[primitive vau!]`, `#[host name]` | `operative` |
| applicatives | `#[applicative #[operative (x) #ignore (multiply x x)]]` | `applicative` |

A number starts with a digit, after an optional sign: it is an integer if it is one, else a float
if it has a `.` or an exponent, as Rust reads them (`1.`, `2e5`); anything else that starts so,
such as `99999999999999999999` or `1.2.3`, is a `parse-error` (`bad-number`). A float prints in Rust's shortest
form that reads back (`{:?}`: `1.0`, `0.1`, `1e300`), so it reads back as the same float, bit
for bit. Every not-a-number is one value: it prints as `#not-a-number`, which reads back as Rust's
`f64::NAN`, and `same?` takes any two as the same. The sign and payload bits a not-a-number has
are the machine's (x86_64's `(divide 0.0 0.0)` has its sign set, aarch64's hasn't), so no grift
operation reads them; a host that takes one out gets whatever Rust's arithmetic left there.
The escapes mean what Rust's do: `\\ \" \n \r \t` are Rust's characters, and a string prints with
them. Rust's `\0`, `\'` and backslash-newline aren't grift's (`\x0;` says `\0`, and a `'` needs no
escape), and grift's `\xHEX;` takes any scalar value in place of Rust's `\xNN` and `\u{...}`.
A bytes literal is `#bytes(`, the bytes as integers 0 to 255 written as the reader reads integers,
with space and comments between them, and `)`. A bytes value prints so, every byte as the integer
`byte-at` gives for it, and reads back as the same bytes; there is no text and no hex in it, so
bytes never look like a string. Inside, a token is read as anywhere: one that doesn't read is its
own `parse-error` (`bad-number`, `unknown-constant`, `unclosed-string`) and one that reads as anything
but an integer 0 to 255 is `bad-byte`, at it, and a list or a nested `#bytes(` at its `)`. A `.` is `misplaced-dot`, and a missing `)` is
`unclosed-list` at the `(`; `#bytes` with no `(` after it is `unknown-constant`. `display` prints bytes as their literal: they leave as they are only through
a file's `write`.
`;` starts a comment to the end of the line, and so does `#!` wherever a token can start.

## Primitives (Rust)

All are applicatives except `vau!`. Most are **core**: what no grift definition could do the same
(the machine, its frames, bindings, pairs, identity, the number substrate, a string's bytes, the
reader and the printer). The rest are **native**: `first`, `rest`, `select`, `lookup`, the bitwise
operations, `integer-to-string`, `string-to-integer`, `integer-to-float`, `float-to-integer`,
`remainder`, `less-than?`, `absolute-value`, `power`, `square-root`, `floor`, `ceiling`, `round`,
`truncate`, `byte-length`, `slice`, `search`, `code-point-to-string` and the bytes operations
`string-to-bytes`, `bytes-to-string`, `list-to-bytes` and `bytes-to-list` exist only for speed (a bytes value's contents are what the
printer writes and the reader reads back), and each
one's meaning is its grift definition in [`natives.grift`](../crates/grift/src/natives.grift),
which a test shows no program can tell from the Rust, but for how the combiner itself prints.

| Combiner | Meaning |
|---|---|
| `(vau! parameters context-parameter body ...)` | An operative closing over the current context (4.10.3). `parameters` is a tree of symbols, `#ignore`, `()` and pairs, with no symbol twice; `context-parameter` is a symbol not in it, or `#ignore`. Checked when the operative is made (`parameter-error`). A call binds them in a new child of that context and evaluates the body's forms in turn, the last as a tail call; `#inert` for none. |
| `(wrap c)`, `(unwrap a)` | The applicative around combiner `c`; the combiner inside applicative `a` (4.10.4–5). Wrapping an applicative wraps it again. |
| `(evaluate x c)` | Evaluates `x` in context `c`, as a tail call (4.8.3). |
| `(context parent)` | A new empty context, a child of `parent`, or with no parent if that is `()` (4.8.4). |
| `(bind c parameters value)` | Matches the tree `parameters` to `value` and binds its symbols in context `c` itself, replacing bindings there. Returns `#inert`. The matching of `define!` (4.9.1) as an applicative. |
| `(unbind c symbol)` | Removes the binding of `symbol` from context `c` itself, never a parent's: `#true` if there was one, `#false` if not. A lookup in `c` then finds a parent's binding of the name, if there is one, and the value, if nothing else holds it, is collected. A symbol is collected like any value once nothing refers to it, so a table keyed by computed symbols (`string-to-symbol`) holds only the names it still has. A live name is stored twice, once in its heap slot and once in the index that finds it by spelling. |
| `(lookup c symbol otherwise)` | The value `symbol` has in context `c` or the nearest parent binding it, as evaluating it there gives; `otherwise` if none does, where evaluating would be `unbound-symbol`. Both are evaluated, so `(lookup counts word 0)` reads a table with a default. |
| `(names c)` | The symbols bound in context `c` itself (not its parents), sorted by name. |
| `(pair a b)`, `(first p)`, `(rest p)` | A new pair; its two halves (4.6.3, 5.4.1). |
| `(same? a b)` | Identity for pairs, contexts and combiners; equality of value for integers, symbols, strings and bytes; the same stored bits for floats, and any two not-a-numbers, which are one value. Values of different types are never the same (4.2.1, `eq?`). |
| `(select b x y)` | `x` if `b` is `#true`, `y` if `#false`; a `type-error` otherwise. Both are evaluated; `if!` uses it to pick an expression. |
| `(type-of v)` | The symbol naming `v`'s type, as in the table above. |
| `(add a b)`, `(subtract a b)`, `(multiply a b)`, `(divide a b)`, `(remainder a b)` | Of two integers or two floats; mixing them is a `type-error`. On integers, Rust's checked arithmetic: `divide` and `remainder` are Rust's `/` and `%`, truncating toward zero, so `(remainder -7 2)` is `-1`; dividing by 0 is `division-by-zero`, and a result beyond 64 bits, `(divide -9223372036854775808 -1)` included, is `overflow`. On floats, IEEE: `(divide 1.0 0.0)` is `#infinity`. |
| `(less-than? a b)` | Whether `a < b`, of two integers, two floats, two strings or two bytes values; mixing kinds is a `type-error`. With `#not-a-number`, `#false`. Strings and bytes compare in byte order, as Rust's `<` on `str`: at the first byte where they differ, or the shorter first when one starts the other. |
| `(bitwise-and a b)` `(bitwise-or a b)` `(bitwise-exclusive-or a b)` `(shift-left a n)` `(shift-right a n)` | Rust's `&` `\|` `^` `<<` `>>` on two integers: the bits shifted out are lost, as in Rust, and `shift-right` keeps the sign (`(shift-right -8 1)` is `-4`). A shift by an amount outside 0 to 63, where Rust's checked shift gives none, is `overflow`. |
| `(square-root x)` `(sine x)` `(cosine x)` `(tangent x)` `(exponential x)` `(natural-logarithm x)` `(floor x)` `(ceiling x)` `(round x)` `(truncate x)` | Rust `std`'s `sqrt` `sin` `cos` `tan` `exp` `ln` `floor` `ceil` `round` (half away from zero) and `trunc`, of floats only. `sqrt` and the four roundings are exact, so the same everywhere; the others may differ slightly between platforms, as in Rust. |
| `(absolute-value x)`, `(power x y)` | Of an integer or a float, as `add` is: two integers give an integer (Rust's checked `abs` and `pow`), two floats a float (`abs` and `powf`), and mixing them is a `type-error`. `(absolute-value -9223372036854775808)` is `overflow`, and so is an integer power beyond 64 bits; an integer power with an exponent below 0, which isn't an integer, is a `range-error` (12.5.12, 12.9.6). |
| `(integer-to-float n)`, `(float-to-integer x)` | Rust's `as`: the nearest float; the integer part, saturating at the ends of the 64-bit range, and 0 for `#not-a-number`. |
| `(evaluate-with-cleanup x c cleanup)` | Evaluates `x` in `c`; when that ends, with a value or an error, calls the combiner `cleanup` with no arguments, then returns the value or lets the error go on. An error in `cleanup` meets the error leaving by the one rule (see Errors). A stop from the host can cut `cleanup`, even before it does anything. If memory is refused at its first step, `cleanup` has done nothing and `out-of-memory` goes on, by the same rule; the cleanups around it are still tried. Not a tail call. |
| `(evaluate-with-handler x c handler)` | Evaluates `x` in `c` and returns its value. If an error leaves it, the cleanups inside run first, then the combiner `handler` is called in `c` with the error as a new context with no parent, binding `kind` (a symbol), `values` (a list) and `trace` (a list, see Errors); its value is `evaluate-with-handler`'s, as a tail call. A limit (see Errors) is never caught. |
| `(raise e)` | Raises the error `e`, a context binding `kind`, `values` and `trace` as `error` makes and a handler gets. It keeps the trace it has, which then gains the `raise` and the combinations it passes on its way out on its front, each once. The fields are looked up as any record's, so they may be inherited; others aren't carried. Anything else is a `type-error`. |
| `(byte-length s)`, `(byte-at s i)` | The length of `s`, a string or a bytes value, in bytes (a string's of UTF-8); the byte at offset `i`, an integer 0–255, a `range-error` unless `0 <= i <` the length. Both O(1). |
| `(slice s start end)` | The bytes of `s` from offset `start` to `end`, of `s`'s kind: a string or a bytes value. A `range-error` unless `0 <= start <= end <=` the length and, for a string, both lie on character boundaries. |
| `(search text pattern start)` | The byte offset of the first place at or after `start` where `pattern` is in `text`, or `()`; an empty pattern is at `start`. Both are strings or both bytes; a pattern of the other kind is a `type-error`. A `start` out of range, or off a character boundary of a string, is a `range-error`. |
| `(join s ...)` | The strings joined, or the bytes values joined: the first decides, and one of the other kind is a `type-error`. `""` for none. |
| `(code-point-to-string n)` | The string of the one character whose Unicode code point is `n`; a `range-error` for a surrogate or past `#x10FFFF`. |
| `(string-to-bytes s)`, `(bytes-to-string b)` | A string's UTF-8 bytes; and back, by Rust's `str::from_utf8`, keeping NUL and a byte-order mark. Bytes that aren't UTF-8 are an `encoding-error` with `b` and the offset of the first sequence that isn't (Rust's `valid_up_to`): `(bytes-to-string #bytes(97 255))` is `encoding-error #bytes(97 255) 1`. |
| `(list-to-bytes list)`, `(bytes-to-list b)` | The bytes of a list of integers 0–255, and back. An element that isn't an integer, or a tail that isn't `()`, is a `type-error`, and an integer outside 0–255 a `range-error`, each with the value and its position: `(list-to-bytes (list 1 256))` is `range-error 256 1`. |
| `(string-to-symbol s)` | The symbol named `s`, interned. |
| `(integer-to-string n)`, `(string-to-integer s)` | Rust's `to_string` and `parse::<i64>`: `"-42"`, and back. A string that isn't an integer in range is a `parse-error`. |
| `(string-to-float s)` | Rust's `parse::<f64>` and the three `#` names, so it reads what `value-to-string` writes for a float: `(string-to-float "1")` is `1.0`, `(string-to-float "inf")` is `#infinity`. `value-to-string` writes a not-a-number with no sign or payload, so those don't come back. A string that reads as neither is a `parse-error`. |
| `(read-form text offset)` | The program reader, on one form of string `text`: the first form after byte `offset`, as the list `(form start end)`, where `slice` from `start` to `end` is the form's own text, without the space and comments before it; `()` if only those are left, so a form that is `()` gives `(() start end)`. It evaluates nothing. Text that doesn't read is a `parse-error` with why and where, in the whole text, as for a program (see Errors); an offset out of range or off a character boundary, a `range-error`. Reading a long text form by form costs nothing per call for its length. |
| `(value-to-string v)` | The text the printer writes for `v`, combiners and contexts as they print (see Values): `(value-to-string (list 1.0 "a"))` is `"(1.0 \"a\")"`, and a float's text reads back as the same float. |

## The library (grift, [`library.grift`](../crates/grift/src/library.grift))

| Combiner | Meaning |
|---|---|
| `(define! parameters expression)` | Evaluates `expression` and matches `parameters` to it in the current context (4.9.1), Kernel's `$define!` and Scheme's internal `define`. Returns `#inert`. Inside a body that context is the call's own, so `define!` there makes a local and never changes an outer binding; see below the table. |
| `(quote! x)` | `x`, unevaluated. |
| `(list x ...)` | The list of the arguments (5.2.1). |
| `(if! test consequent alternative)` | Evaluates `test`, which must give a boolean, then `consequent` or `alternative` as a tail call (4.5.2). Any other number of operands is an `arity-error`. A native twin runs it (see "What the library forms cost"). |
| `(sequence! x ...)` | Each in turn; the last one's value, as a tail call; `#inert` for none (5.1.1). A native twin runs it. |
| `(lambda! parameters body ...)` | `(wrap (vau! parameters #ignore body ...))` (5.3.2). |
| `(current-context)` | The context it is called in (6.7.2). |
| `(evaluate-in! o c)` | Evaluates `o`, as a tail call, in the context `c` evaluates to (6.7.9); `(evaluate-in! kind e)` reads a field of a record such as an error, `((evaluate-in! write f) line)` calls an operation a context holds, with `line` evaluated where you are. |
| `(not? b)`, `(and!? x ...)`, `(or!? x ...)` | Booleans (6.1). `and!?` and `or!?` evaluate left to right and stop early; every operand they evaluate must give a boolean, the last one included, so it isn't a tail call. |
| `(empty-list? x)` `(inert? x)` `(ignore? x)` `(boolean? x)` `(integer? x)` `(float? x)` `(symbol? x)` `(pair? x)` `(string? x)` `(bytes? x)` `(context? x)` `(operative? x)` `(applicative? x)` | A predicate for each type `type-of` gives (4.4.1, 4.6.1, 4.6.2, 4.8.1, 12.5.1). |
| `(apply a arguments)` | Calls applicative `a`'s combiner on the list `arguments`, in a new empty context (5.5.1). |
| `(map a list ...)`, `(for-each a list)` | The list of `a` applied to each element, or with several lists to the elements at each place, the lists being of one length (a `type-error` otherwise); `a` on each element in turn, giving `#inert` (5.9.1, 6.9.1). |
| `(any? a list)`, `(all? a list)` | Whether `a` gives `#true` for some element, for every element, from the left, stopping at the answer. `a` must give booleans. |
| `(filter a list)` | The elements for which `a` gives `#true`, in order; `a` must give booleans (6.3.5). |
| `(fold a value list)` | `a` on the value so far and each element in turn, from the left, starting from `value`: `(fold add 0 list)` is a sum. Rust's `Iterator::fold`. |
| `(range start end)` | The list of the integers from `start` up to, not including, `end`, as Rust's `start..end`; `()` if `start` isn't below `end`. |
| `(sort list before?)` | The elements in the order `before?` gives, an applicative of two giving a boolean: stable, so equal elements keep their order, and `n log n` calls of `before?` (a merge sort). `(sort words less-than?)`. |
| `(element-at list index)`, `(second list)`, `(third list)`, `(last list)` | The element at `index`, counting from 0 as Rust does; the second, the third, the last. |
| `(take list n)`, `(drop list n)` | The first `n` elements; the list after them, shared. An index or count out of range in any of these is a `range-error` carrying the operands, the list and the index or count: `(element-at (list 1) 3)` gives `range-error (1) 3`, `(second (list 1))` `range-error (1) 1`, and `(last ())` `range-error ()`. An index that isn't an integer is a `type-error`. |
| `(when! test body ...)` | Evaluates `test`, which must give a boolean; if it is `#true`, the body forms in turn, the last as a tail call, and its value; if `#false`, `#inert`. |
| `(conditional! (test body ...) ...)` | The body of the first clause whose test is `#true`; `#inert` if none is, or if there are no clauses, or if its body is empty (5.6.1), as `when!` and an empty body give `#inert`: `conditional!` may choose nothing, where `if!` must have both branches. A clause that isn't a pair `(test . body)` is an `arity-error` carrying, as for any parameter tree that doesn't fit, the whole of what was matched: here the list of the clauses and the caller's context, which `write` prints with all its names and a report as their count, so `(conditional! 1)` reports `arity-error ((1) #[context N names])`. A body is checked only when it runs. |
| `(let! ((parameters expression) ...) body ...)` | Makes one new child of the caller's context; each binding in order evaluates its expression there and binds its parameters there, so it sees the bindings before it, and a combiner it binds can call itself and the ones after it; then the body (R-1RK's `$letrec*`, 6.7.6). A binding that isn't `(parameters expression)` is an `arity-error`. |
| `(finally! body cleanup ...)` | The body's value, after the cleanup forms ran; they also run when an error leaves the body, and the error then goes on, to a `catch!` or the host. An error in a cleanup form ends the cleanup and goes on by the one rule (see Errors). A stop from the host can cut a cleanup that has begun, even before its first form: the body's value or error is lost, and `interrupted` goes on. A force cuts every cleanup. Memory refused collects once, before the nearest cleanup's first step; a cleanup stops at its first allocation that still fails, so it may do nothing, and that refusal collects again for the next. |
| `(catch! body e handler ...)` | The body's value. If an error leaves it, the cleanups inside run first, then the handler forms, with the parameter `e` bound to the error in a new child of the caller's context; the last one's value is `catch!`'s. A limit (see Errors) is never caught. |
| `(catch-kind! body kind e handler ...)` | `catch!` for the errors of one kind, the symbol `kind`, unevaluated: `(catch-kind! (string-to-integer s) parse-error e 0)`. An error of any other kind goes on unchanged, with the trace it had. A `kind` that isn't a symbol is a `type-error`. |
| `(bindings c)` | Context `c`'s own bindings as data: a list of `(name value)`, sorted by name as `names` gives them, so a record's fields are a list: `(bindings (error (quote! e) 1))` is `((kind e) (trace ()) (values (1)))`. |
| `(record names values)` | A new context with no parent, `values` bound to `names` as `bind` matches them: `(record (quote! (x y)) (list 1 2))`. |
| `(error kind value ...)` | A new error: a context with no parent binding `kind`, `values` (the list of the values) and an empty `trace`. It raises nothing; `raise` does. |
| `(length list)`, `(append list ...)`, `(reverse list)` | List length; concatenation, sharing the last list; the elements in the opposite order (6.3.1, 6.3.3). |
| `(equal? a b)` | Pairs by structure, floats as IEEE does (`(equal? 0.0 -0.0)` is `#true`, `#not-a-number` is equal to nothing), everything else as `same?`. Values of different types are never equal: `(equal? 1 1.0)` is `#false` (6.6.1). |
| `(greater-than? a b)`, `(less-than-or-equal? a b)`, `(greater-than-or-equal? a b)` | The rest of the comparisons, as Rust's `>`, `<=`, `>=` (12.5.3). |
| `(minimum a b)`, `(maximum a b)` | The smaller, the larger, of two values of one kind `less-than?` orders (12.5.13). |
| `(find-association x pairs)`, `(member? x list)` | The first pair whose `first` is `equal?` to `x`, or `()`; whether `x` is `equal?` to an element (6.3.6, 6.3.7). |
| `(binds? c symbol)` | Whether the symbol is bound in context `c` or one of its parents (6.7.1), with both evaluated, so the symbol can be computed: `(binds? table (string-to-symbol key))`. It is `lookup` with a value no program holds as `otherwise`, so its `type-error` is `lookup`'s. |
| `(symbol-to-string s)` | A symbol's name. |
| `(float-to-string f)` | A float's printed text, which reads back as the same float: `(float-to-string 1.0)` is `"1.0"`. Anything else is a `type-error`. |
| `(string-starts-with? s prefix)`, `(string-ends-with? s suffix)` | Whether `s` starts or ends with those bytes. |
| `(string-split text separator)`, `(string-join strings separator)` | The parts of `text` between the separators, empty ones too, so `(string-split "a,,b" ",")` is `("a" "" "b")` and `(string-split "" ",")` is `("")`; an empty separator is a `range-error`. The strings with the separator between them. `string-join` undoes `string-split`. |
| `(string-trim s)` | `s` without the ASCII whitespace at either end (space, tab, newline, form feed, return), as Rust's `trim_ascii`. |
| `(string-repeat s count)` | `s` `count` times; a count below 1 is no copies, `""`. |
| `(string-pad-left s width fill)`, `(string-pad-right s width fill)` | `s` with copies of `fill`, a string of one byte, before it, after it, up to `width` bytes; `s` itself if it is that long already: `(string-pad-left "7" 2 "0")` is `"07"`. A `fill` of any other length is a `range-error`. |
| `(string-to-lowercase s)` | `s` with `A` to `Z` made `a` to `z` and every other byte as it is, as Rust's `to_ascii_lowercase`. |
| `(standard-context)` | A new child of the built-in context: it sees the language and nothing of any program, and what is bound in it stays there (6.7.7). The built-in context itself stays out of reach. |
| `built-in-names` | The list of every name above and every primitive's, bound when the library is loaded. |

Programs run in the **top-level context**, a child of the **built-in context**, which is out of
reach. Defining a name at top level shadows the library's binding without changing what the library
itself uses.

**`define!` in a body makes a local.** A call evaluates its body in a new child of the context the
combiner was made in, and `define!` binds in the context it is evaluated in, so inside a body it
binds in the call's context, never in an outer one, and says nothing when the name is bound
outside too. There is no `set!`: changing an outer binding means holding its context and binding
there, as VISION "State" has it.

```lisp
(define! counter 0)
(define! bump-wrong! (lambda! () (define! counter (add counter 1))))   ; a local, gone after the call
(bump-wrong!)
counter                                                               ; => 0
(define! here (current-context))
(define! bump! (lambda! () (bind here (quote! counter) (add counter 1))))
(bump!)
counter                                                               ; => 1
```

**Dispatch.** A context of combiners, looked up by a symbol, is the fast dispatch:
`((evaluate (quote! add) operations) x)` or `((lookup operations name default) x)` costs one
lookup whatever the number of cases, where `conditional!` tests its clauses in order.

**What the library forms cost.** `if!` and `sequence!` are library operatives with a native twin:
the same operative, run in Rust when its operands are the shape it handles (three for `if!`, a
proper list for `sequence!`), and by its body otherwise, so what a program sees, its errors and
traces included, is the library's. The other forms with a body walk
their operands in grift, so in a loop run per byte or per node each costs a few calls. From cheapest
to dearest: `if!`, `sequence!`, making a `lambda!`, `and!?`, `let!`, and `conditional!`, which costs
more than the same choice in nested `if!`s. A library combiner such as `second` costs a few calls,
as it checks the list's length to say what is wrong, where `(first (rest x))` costs far less. Nested
`if!`s are the cheapest branch; the other forms read better where speed doesn't matter.

## Errors

Every error stops evaluation and goes to the innermost `catch!` around it, or to the host, as a
kind, the values involved, and a trace: the combinations of the program being evaluated, each
once, from the innermost out to the top-level form. A program reads the trace newest first, so the
place the error began is its `last`; the host's `Error::trace` and the runner's report list it
innermost first. A handler's trace shares the list it was raised with: a catch and raise adds only
the places it passed to the front, and raising the trace a handler just got costs nothing per
place, while raising any other trace walks it once. Who made a combination decides: the
library's own code, and the code it builds as it runs, are skipped; code a program reads or builds
is listed, whatever its head is. Listed are the combination evaluated when the error came, and
each one with work left to do around it: an operand still to evaluate, or its value awaited. A
combination in tail position has finished when the next one starts, so it isn't listed, and nor is
a call once its combiner's body is running, for the body takes the call's place: below, `(check x)`
is gone once `check`'s body runs, and the trace has the body's `raise` and the `(add 1 (check x))`
that awaits it. An error outside every combination, such as a bare symbol at top level (`nope`
alone on a line), has no combination to list, so its trace is empty and it has no line. A place is
added only if there is memory to spare for it, so an `out-of-memory` error's trace can miss some.
Cleanups (`finally!`) run on the way out. A combination read from
a named text, such as a file the runner runs or loads, keeps its source and line beside it, not in
it, so the host can report each combination of a trace with its file and line.
Rust hosts can read such forms without evaluating them with `Grift::read_in(source, text)`, which
returns the list of all forms, as `Grift::read(text)` does. `Grift::position` gives each combination's
source and opening line, also after a loader evaluates it later. If `read_in` or `evaluate_in`
cannot read the text, the values of its `parse-error` are `(reason line column source)`, with the
source name a symbol. For `read` and `evaluate`, which take no source name, they are
`(reason line column)`. A loader can propagate the named error unchanged; the runner's loaded-file
error names that path in its values, and its trace has the outer load's combination.
When evaluation starts one of those named outer forms, including through a native branch or
sequence, its trace retains a boundary for the form. A tail evaluation of another named outer
form updates that boundary to the new form, so the finished one disappears and space stays
constant. The host's own entry boundary stays; nested evaluations with work left around them
retain their own boundaries too.

```lisp
(define! check (lambda! (x) (if! (integer? x) x (raise (error (quote! not-integer) x)))))
(define! run (lambda! (x) (add 1 (check x))))
(run (quote! a))   ; not-integer a in (raise (error (quote! not-integer) x))
                   ;   from (add 1 (check x))
                   ;   from (run (quote! a))
```

```lisp
(catch! (run (quote! a)) e (list (evaluate-in! kind e) (evaluate-in! values e)))   ; => (not-integer (a))
(catch! (run (quote! a)) e (last (evaluate-in! trace e)))   ; => (raise (error (quote! not-integer) x))
(catch-kind! (first 1) type-error e 0)                                  ; => 0
```

**What can be caught.** What a program raises always can: the primitives' errors, the library's, a
host function's, and the program's own, whatever kind they are called. A **limit** can't: an error
the host or the machine began, a real stop (`interrupted`), the allocator refusing (`out-of-memory`),
or an error a host function or a host's answer made a limit. It goes on through every `catch!` to the
host, so a `catch!` in a loop can't swallow C-g, and it stays a limit through a host function. Who began
an unwinding decides this, and no name does: a program that raises its own `interrupted` raises an
ordinary error, `catch!` catches it, and rebinding a kind's name changes nothing.

**The one rule, for a reason arriving during a cleanup:** the newest reason wins, unless a program's
would replace a limit. And the host can always end a run: a force ends it with `interrupted`, whatever
its cleanups do.

**Early exit** is `catch!` and a kind of error of the program's own; there is no other escape:

```lisp
(define! first-even
  (lambda! (xs)
    (catch-kind! (sequence! (for-each (lambda! (x) (when! (same? (remainder x 2) 0) (raise (error (quote! found) x)))) xs)
                            #false)
                 found
                 e
                 (first (evaluate-in! values e)))))
(first-even (list 1 3 4 5))   ; => 4
```

| Kind | When | Values |
|---|---|---|
| `unbound-symbol` | a symbol with no binding | the symbol |
| `type-error` | a primitive got the wrong kind of value, integers and floats mixed, a test that isn't a boolean; an improper operand list given a primitive or an applicative; an element or a tail given `list-to-bytes` that isn't an integer or `()` | the value; for `list-to-bytes`, the value and its position in the list |
| `arity-error` | a primitive or a host function got the wrong number of operands; a parameter tree didn't match, as when a library form gets operands that aren't its shape | the operands, as one list: `(add 1)` gives `arity-error (1)`, `(if! #true 1)` `arity-error (#true 1)`; for a parameter tree, the whole of what it was matched against, which for a library form's helper can hold the caller's context |
| `not-a-combiner` | a combination's head isn't a combiner | the value |
| `parameter-error` | a malformed parameter tree, a symbol twice, or a bad context parameter | the tree or the context parameter |
| `overflow` | integer arithmetic beyond 64 bits, `absolute-value` and `power` included; a shift by an amount outside 0 to 63 | the operands of the arithmetic that overflowed |
| `division-by-zero` | integer `divide` or `remainder` by 0 | the operands |
| `range-error` | a string offset off a character boundary or out of range; a bytes offset out of range; an integer outside 0–255 given `list-to-bytes`; a code point that isn't one; an empty separator given `string-split`; a list index out of range; a negative exponent; a fill that isn't one byte | the operands; for `list-to-bytes`, the integer and its position |
| `encoding-error` | bytes given `bytes-to-string` that aren't UTF-8 | the bytes, and the offset where the first sequence that isn't UTF-8 starts (Rust's `valid_up_to`) |
| `parse-error` | text that doesn't read: a program or `read-form`'s text, or a string given `string-to-integer` or `string-to-float` | why, then the line and the column, which counts characters from 1, then the source's name if the text was read under one; or the string |
| `out-of-memory` | the allocator refused | none |
| `stale-value` | the host used a value the collector reclaimed | none |
| `foreign-value` | the host gave one interpreter a value, a symbol or a host function of another | none |
| `interrupted` | the host stopped the run; stops waiting together count as one, and a later stop stops the cleanups too, even one that would have ended | none |
| `cannot-park` | a call that would wait came where nothing can be set aside: see Calls that wait | none |
| `output-error` | a host's output refused what `write` or `describe` wrote: a host may pass it on | none |
| any symbol | a program raised an error it made | the values given |

A text that doesn't read says why, as a symbol, and where: `unclosed-list` and `unclosed-string`
at the `(` or `"` left open (the innermost, for lists), not at the end of the text;
`unmatched-parenthesis` at a `)` with no `(`; `misplaced-dot` at a `.` or what follows it outside
`(a . b)`; `bad-escape` at a `\` that isn't one of the escapes; `bad-number` at a token that starts
as a number and isn't one; `unknown-constant` at a `#` name that isn't one; `bad-byte` at a value
inside `#bytes(` that isn't an integer 0 to 255. `(read-form "(a\n (b" 0)`
is `parse-error unclosed-list 2 2`.

The runner adds its own operations and its own kinds; [the README](../README.md#the-runner)
describes each.

## Calls that wait

A host can register a function that answers later. To a program it is a call like any other: there
is no new form or name, and nothing marks it.

```lisp
(finally!
  (sequence!
    (say "started")
    (sleep 100)   ; the host sets this run aside; its loop and other runs go on
    (say "done"))
  (say "closed"))
```

The program reads top to bottom. Every `catch!` and `finally!` around the call is still there when it
returns, the trace of a later error shows the whole path, and the call can be inside `map`,
`for-each`, `let!`, a `load` written in grift, or a cleanup. It returns a value, or raises an error, once.

What a program may see change across such a call: **other runs went on meanwhile.** They share the
bindings, so `(bind here (quote! n) ...)` by another run is seen after the wait, and a value read
before the wait may be out of date after it. Nothing else changes: what the run holds itself, its
locals and arguments, stays as it was, however long it waits. Runs take turns, one at a time, and
only at these calls: between two waiting calls no other run can come in.

The host may answer with a limit instead, to end the run. The call raises it, no `catch!` catches it,
and the cleanups run on the way out. A cleanup that waits there is a run that waits like any other.

A call that would wait inside a run that can't be set aside, because it runs under `Grift::evaluate`
or `Grift::call`, is the error `cannot-park`; the trace says which call. It can be caught like any
other error. A call a host function hands grift to make is part of the run, and waits as it does.

A host that drops its interpreter with runs waiting runs none of their cleanups, as if its process
had died. The README's embedding section gives the host's side.

## Where grift differs from R-1RK, and why

[VISION "Kernel"](VISION.md#kernel) gives the larger differences. Beyond those:

- **One special primitive.** `if!`, `define!` and `sequence!` are primitive in Kernel; here they
  are library code over `vau!`, `select`, `bind` and `evaluate`, `if!` and `sequence!` with a
  native twin.
- **`finally!`** is on `evaluate-with-cleanup` (not in Kernel), the exit half of Kernel's guards;
  `binds?` is derived from `lookup`, a primitive Kernel doesn't have, where R-1RK derives `$binds?`
  from its error continuation: a miss raised and caught an error, 5-14 µs a probe.
- **Binary arithmetic and comparison.** `add`, `multiply`, `less-than?`, `minimum` and the rest
  take two operands; Kernel's take any number. The type predicates take one value.
- **Bytes beside strings.** Bytes are a value of their own, never a string, with their own
  literal; `same?` compares them by content and `less-than?` orders them by byte, as strings. Both
  are sequences of bytes, so one vocabulary serves both, as `add` serves integers and floats:
  `byte-length`, `byte-at`, `slice`, `join` and `search` take either and answer the kind they were
  given.
- **Strings are immutable,** and `same?` compares them by content, like integers. R-1RK has no
  strings; grift's offsets count bytes of UTF-8, so `byte-at` is O(1), and there is no
  character type: a code point is an integer.
- **`apply` has one form,** `(apply a arguments)`, with a new empty context; `for-each` takes one
  list, and `map` lists of one length, as Kernel's does. `context` takes one operand, its parent or `()`.
- **New, not in Kernel:** `bind`, `unbind`, `lookup`, `names` and `built-in-names`, `select`,
  `type-of`, `when!`, `record`, `error`, `raise`, `catch-kind!`, `reverse`, `read-form`, `value-to-string`, bytes,
  `any?`, `all?`, `fold`, `range`, `sort`, the list access by index, the bitwise
  operations, and the string and float primitives and helpers.
