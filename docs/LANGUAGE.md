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
for bit. Every not-a-number prints as `#not-a-number`, which reads back as Rust's `f64::NAN`: a
not-a-number's sign and payload don't survive printing.
The escapes mean what Rust's do: `\\ \" \n \r \t` are Rust's characters, and a string prints with
them. Rust's `\0`, `\'` and backslash-newline aren't grift's (`\x0;` says `\0`, and a `'` needs no
escape), and grift's `\xHEX;` takes any scalar value in place of Rust's `\xNN` and `\u{...}`.
A bytes literal is `#bytes(`, the bytes as integers 0 to 255 written as the reader reads integers,
with space and comments between them, and `)`. A bytes value prints so, every byte as the integer
`bytes-byte` gives for it, and reads back as the same bytes; there is no text and no hex in it, so
bytes never look like a string. Anything else inside is a `parse-error` (`bad-byte`) at it, a `.`
is `misplaced-dot`, and a missing `)` is `unclosed-list` at the `(`; `#bytes` with no `(` after it
is `unknown-constant`. `display` prints bytes as their literal: they leave as they are only through
`write-bytes`.
`;` starts a comment to the end of the line, and so does `#!` wherever a token can start.
For Rust hosts, `grift::write_string(text, out)` writes one quoted string literal with these same
escapes, into a `std::fmt::Write`, without an interpreter or allocation. It returns `std::fmt::Result`;
`Grift::write` uses it for strings too.

## Primitives (Rust)

All are applicatives except `vau!`. Most are **core**: what no grift definition could do the same
(the machine, its frames, bindings, pairs, identity, the number substrate, a string's bytes, the
reader and the printer). The rest are **native**: `first`, `rest`, `select`, `lookup`, the bitwise
operations, `integer-to-string`, `string-to-integer`, `integer-to-float`, `float-to-integer`,
`string-byte-length`, `byte-substring`, `code-point-to-string` and the bytes operations
`bytes-length`, `bytes-byte`, `bytes-slice`, `bytes-append`, `string-to-bytes`, `bytes-to-string`,
`list-to-bytes` and `bytes-to-list` exist only for speed (a bytes value's contents are what the
printer writes and the reader reads back), and each
one's meaning is its grift definition in [`natives.grift`](../crates/grift/src/natives.grift),
which a test shows no program can tell from the Rust.

| Combiner | Meaning |
|---|---|
| `(vau! parameters context-parameter body)` | An operative closing over the current context (4.10.3). `parameters` is a tree of symbols, `#ignore`, `()` and pairs, with no symbol twice; `context-parameter` is a symbol not in it, or `#ignore`. Checked when the operative is made (`parameter-error`). A call binds them in a new child of that context and evaluates `body` as a tail call. |
| `(wrap c)`, `(unwrap a)` | The applicative around combiner `c`; the combiner inside applicative `a` (4.10.4–5). Wrapping an applicative wraps it again. |
| `(evaluate x c)` | Evaluates `x` in context `c`, as a tail call (4.8.3). |
| `(context parent)` | A new empty context, a child of `parent`, or with no parent if that is `()` (4.8.4). |
| `(bind c parameters value)` | Matches the tree `parameters` to `value` and binds its symbols in context `c` itself, replacing bindings there. Returns `#inert`. The matching of `define!` (4.9.1) as an applicative. |
| `(unbind c symbol)` | Removes the binding of `symbol` from context `c` itself, never a parent's: `#true` if there was one, `#false` if not. A lookup in `c` then finds a parent's binding of the name, if there is one, and the value, if nothing else holds it, is collected. The symbol itself stays: symbols are never freed, so a table keyed by computed symbols (`string-to-symbol`) grows the symbol table by each new name for the life of the interpreter. The name is stored twice, once by id and once in the hash index. |
| `(lookup c symbol otherwise)` | The value `symbol` has in context `c` or the nearest parent binding it, as evaluating it there gives; `otherwise` if none does, where evaluating would be `unbound-symbol`. Both are evaluated, so `(lookup counts word 0)` reads a table with a default. |
| `(names c)` | The symbols bound in context `c` itself (not its parents), sorted by name. |
| `(pair a b)`, `(first p)`, `(rest p)` | A new pair; its two halves (4.6.3, 5.4.1). |
| `(same? a b)` | Identity for pairs, contexts and combiners; equality of value for integers, symbols, strings and bytes; the same stored bits for floats. Values of different types are never the same (4.2.1, `eq?`). |
| `(select b x y)` | `x` if `b` is `#true`, `y` if `#false`; a `type-error` otherwise. Both are evaluated; `if!` uses it to pick an expression. |
| `(type-of v)` | The symbol naming `v`'s type, as in the table above. |
| `(add a b)`, `(subtract a b)`, `(multiply a b)`, `(divide a b)`, `(remainder a b)` | Of two integers or two floats; mixing them is a `type-error`. On integers, Rust's checked arithmetic: `divide` and `remainder` are Rust's `/` and `%`, truncating toward zero, so `(remainder -7 2)` is `-1`; dividing by 0 is `division-by-zero`, and a result beyond 64 bits, `(divide -9223372036854775808 -1)` included, is `overflow`. On floats, IEEE: `(divide 1.0 0.0)` is `#infinity`. |
| `(less-than? a b)` | Whether `a < b`, of two integers or two floats; with `#not-a-number`, `#false`. |
| `(bitwise-and a b)` `(bitwise-or a b)` `(bitwise-exclusive-or a b)` `(shift-left a n)` `(shift-right a n)` | Rust's `&` `\|` `^` `<<` `>>` on two integers: the bits shifted out are lost, as in Rust, and `shift-right` keeps the sign (`(shift-right -8 1)` is `-4`). A shift by an amount outside 0 to 63, where Rust's checked shift gives none, is `overflow`. |
| `(square-root x)` `(sine x)` `(cosine x)` `(tangent x)` `(exponential x)` `(natural-logarithm x)` `(floor x)` `(ceiling x)` `(round x)` `(truncate x)` | Rust `std`'s `sqrt` `sin` `cos` `tan` `exp` `ln` `floor` `ceil` `round` (half away from zero) and `trunc`, of floats only. Results may differ slightly between platforms, as in Rust. `absolute-value` and `power` are the library's, below. |
| `(integer-to-float n)`, `(float-to-integer x)` | Rust's `as`: the nearest float; the integer part, saturating at the ends of the 64-bit range, and 0 for `#not-a-number`. |
| `(evaluate-with-cleanup x c cleanup)` | Evaluates `x` in `c`; when that ends, with a value or an error, calls the combiner `cleanup` with no arguments, then returns the value or lets the error go on. An error in `cleanup` replaces the one leaving, unless that one is `interrupted`, `abandoned` or `out-of-memory` and the new one is none of them. A stop from the host can cut `cleanup`, even before it does anything. Starting `cleanup` after an error can itself need memory, while the machine's stacks are still growing: if that allocation fails, `cleanup` is passed over and `out-of-memory` goes on in that error's place, even one a `catch!` would have caught; the cleanups around it are still tried. Not a tail call. |
| `(call-with-handler thunk handler)` | Calls `thunk` with no arguments and returns its value. If an error leaves it, the cleanups inside run first, then `handler` is called with the error as a new context with no parent, binding `kind` (a symbol), `values` (a list) and `trace` (a list, see Errors); its value is `call-with-handler`'s, as a tail call. `interrupted`, `abandoned` and `out-of-memory` are never caught. |
| `(raise e)` | Raises the error `e`, a context binding `kind`, `values` and `trace` as `error` makes and a handler gets. A new error (an empty trace) happens at the `raise`; a caught one keeps its trace, with nothing added for the `raise` itself. Like any error, it then gains the combinations it passes on its way out. Anything else is a `type-error`. |
| `(string-byte-length s)`, `(string-byte s i)` | The length of string `s` in bytes of UTF-8; the byte at offset `i`, an integer 0–255. Both O(1). |
| `(byte-substring s start end)` | The string of the bytes of `s` from offset `start` to `end`; a `range-error` unless both lie on character boundaries, in order. |
| `(string-append s ...)` | The strings joined. |
| `(code-point-to-string n)` | The string of the one character whose Unicode code point is `n`; a `range-error` for a surrogate or past `#x10FFFF`. |
| `(bytes-length b)`, `(bytes-byte b i)` | The number of bytes in bytes value `b`; the byte at offset `i`, an integer 0–255, a `range-error` unless `0 <= i <` the length. Both O(1). |
| `(bytes-slice b start end)` | A copy of the bytes of `b` from offset `start` to `end`; a `range-error` unless `0 <= start <= end <=` the length. |
| `(bytes-append b ...)` | The bytes values joined; `#bytes()` for none. |
| `(string-to-bytes s)`, `(bytes-to-string b)` | A string's UTF-8 bytes; and back, by Rust's `str::from_utf8`, keeping NUL and a byte-order mark. Bytes that aren't UTF-8 are an `encoding-error` with the primitive, `b` and the offset of the first sequence that isn't (Rust's `valid_up_to`): `(bytes-to-string #bytes(97 255))` is `encoding-error #[primitive bytes-to-string] #bytes(97 255) 1`. |
| `(list-to-bytes list)`, `(bytes-to-list b)` | The bytes of a list of integers 0–255, and back. An element that isn't an integer, or a tail that isn't `()`, is a `type-error`, and an integer outside 0–255 a `range-error`, each with the primitive, the value and its position: `(list-to-bytes (list 1 256))` is `range-error #[primitive list-to-bytes] 256 1`. |
| `(string-to-symbol s)` | The symbol named `s`, interned. |
| `(integer-to-string n)`, `(string-to-integer s)` | Rust's `to_string` and `parse::<i64>`: `"-42"`, and back. A string that isn't an integer in range is a `parse-error`. |
| `(string-to-float s)` | Rust's `parse::<f64>` and the three `#` names, so it reads what `value-to-string` writes for a float: `(string-to-float "1")` is `1.0`, `(string-to-float "inf")` is `#infinity`. `value-to-string` writes a not-a-number with no sign or payload, so those don't come back. A string that reads as neither is a `parse-error`. |
| `(read-form text offset)` | The program reader, on one form of string `text`: the first form after byte `offset`, as the list `(form start end)`, where `byte-substring` from `start` to `end` is the form's own text, without the space and comments before it; `()` if only those are left, so a form that is `()` gives `(() start end)`. It evaluates nothing. Text that doesn't read is a `parse-error` with why and where, in the whole text, as for a program (see Errors); an offset out of range or off a character boundary, a `range-error`. Reading a long text form by form costs nothing per call for its length. |
| `(value-to-string v)` | The text the printer writes for `v`, combiners and contexts as they print (see Values): `(value-to-string (list 1.0 "a"))` is `"(1.0 \"a\")"`, and a float's text reads back as the same float. |

## The library (grift, [`library.grift`](../crates/grift/src/library.grift))

| Combiner | Meaning |
|---|---|
| `(define! parameters expression)` | Evaluates `expression` and matches `parameters` to it in the current context (4.9.1), Kernel's `$define!` and Scheme's internal `define`. Returns `#inert`. Inside a body that context is the call's own, so `define!` there makes a local and never changes an outer binding; see below the table. |
| `(quote! x)` | `x`, unevaluated. |
| `(list x ...)` | The list of the arguments (5.2.1). |
| `(if! test consequent alternative)` | Evaluates `test`, which must give a boolean, then `consequent` or `alternative` as a tail call (4.5.2). Any other number of operands is an `arity-error`. A native primitive. |
| `(sequence! x ...)` | Each in turn; the last one's value, as a tail call; `#inert` for none (5.1.1). A native primitive. |
| `(lambda! parameters body ...)` | `(wrap (vau! parameters #ignore body))`; several body forms become `(sequence! form ...)`, with the symbol, so the combiner prints its body as written (5.3.2). |
| `(current-context)` | The context it is called in (6.7.2). |
| `(evaluate-in! o c)` | Evaluates `o`, as a tail call, in the context `c` evaluates to (6.7.9); `(evaluate-in! kind e)` reads a field of a record such as an error, `((evaluate-in! write f) line)` calls an operation a context holds, with `line` evaluated where you are. |
| `(not? b)`, `(and!? x ...)`, `(or!? x ...)` | Booleans (6.1). `and!?` and `or!?` evaluate left to right and stop early; every operand they evaluate must give a boolean, the last one included, so it isn't a tail call. |
| `(empty-list? x)` `(boolean? x)` `(integer? x)` `(float? x)` `(symbol? x)` `(pair? x)` `(string? x)` `(bytes? x)` `(context? x)` `(operative? x)` `(applicative? x)` | A predicate for each type `type-of` gives (4.4.1, 4.6.1, 4.6.2, 4.8.1, 12.5.1), but `#inert` and `#ignore`, each the only value of its type: `(same? x #inert)`. |
| `(apply a arguments)` | Calls applicative `a`'s combiner on the list `arguments`, in a new empty context (5.5.1). |
| `(map a list ...)`, `(for-each a list)` | The list of `a` applied to each element, or with several lists to the elements at each place, the lists being of one length (a `type-error` otherwise); `a` on each element in turn, giving `#inert` (5.9.1, 6.9.1). |
| `(any? a list)`, `(all? a list)` | Whether `a` gives `#true` for some element, for every element, from the left, stopping at the answer. `a` must give booleans. |
| `(filter a list)` | The elements for which `a` gives `#true`, in order; `a` must give booleans (6.3.5). |
| `(fold a value list)` | `a` on the value so far and each element in turn, from the left, starting from `value`: `(fold add 0 list)` is a sum. Rust's `Iterator::fold`. |
| `(range start end)` | The list of the integers from `start` up to, not including, `end`, as Rust's `start..end`; `()` if `start` isn't below `end`. |
| `(sort list before?)` | The elements in the order `before?` gives, an applicative of two giving a boolean: stable, so equal elements keep their order, and `n log n` calls of `before?` (a merge sort). `(sort words string-less-than?)`. 10,000 integers sort in about 0.3 s, 10,000 short strings by `string-less-than?` in about 0.9 s. |
| `(element-at list index)`, `(second list)`, `(third list)`, `(last list)` | The element at `index`, counting from 0 as Rust does; the second, the third, the last. |
| `(take list n)`, `(drop list n)` | The first `n` elements; the list after them, shared. An index or count out of range in any of these is a `range-error` carrying the name, the index and the list's length: `(element-at (list 1) 3)` gives `range-error element-at 3 1`; `(last ())`, with no index, gives `range-error last ()`. |
| `(when! test body ...)` | Evaluates `test`, which must give a boolean; if it is `#true`, the body forms in turn, the last as a tail call, and its value; if `#false`, `#inert`. |
| `(conditional! (test body ...) ...)` | The body of the first clause whose test is `#true`; `#inert` if none is, or if its body is empty (5.6.1). A clause that isn't a pair `(test . body)` is an `arity-error`; a body is checked only when it runs. |
| `(let! ((parameters expression) ...) body ...)` | Makes one new child of the caller's context; each binding in order evaluates its expression there and binds its parameters there, so it sees the bindings before it, and a combiner it binds can call itself and the ones after it; then the body (R-1RK's `$letrec*`, 6.7.6). A binding that isn't `(parameters expression)` is an `arity-error`. |
| `(finally! body cleanup ...)` | The body's value, after the cleanup forms ran; they also run when an error leaves the body, and the error then goes on, to a `catch!` or the host. An error in a cleanup form ends the cleanup and goes on in its place, as in `evaluate-with-cleanup`. A stop from the host can cut a cleanup that has begun, even before its first form: the body's value or error is lost, and `interrupted` goes on. After `out-of-memory` a cleanup stops at its first allocation that fails, so it may do nothing. While an error unwinds, a machine-stack allocation that fails as a cleanup starts passes that cleanup over, as in `evaluate-with-cleanup`; out of memory collects first, so the cleanups have what is free. |
| `(catch! body e handler ...)` | The body's value. If an error leaves it, the cleanups inside run first, then the handler forms, with the parameter `e` bound to the error in a new child of the caller's context; the last one's value is `catch!`'s. `interrupted`, `abandoned` and `out-of-memory` are never caught. |
| `(catch-kind! body kind e handler ...)` | `catch!` for the errors of one kind, the symbol `kind`, unevaluated: `(catch-kind! (string-to-integer s) parse-error e 0)`. An error of any other kind goes on unchanged, with the trace it had, as `(raise e)` in a `catch!` passes it on. A `kind` that isn't a symbol is a `type-error`. |
| `(bindings c)` | Context `c`'s own bindings as data: a list of `(name value)`, sorted by name as `names` gives them, so a record's fields are a list: `(bindings (error (quote! e) 1))` is `((kind e) (trace ()) (values (1)))`. |
| `(error kind value ...)` | A new error: a context with no parent binding `kind`, `values` (the list of the values) and an empty `trace`. It raises nothing; `raise` does. |
| `(length list)`, `(append list ...)`, `(reverse list)` | List length; concatenation, sharing the last list; the elements in the opposite order (6.3.1, 6.3.3). |
| `(equal? a b)` | Pairs by structure, floats as IEEE does (`(equal? 0.0 -0.0)` is `#true`, `#not-a-number` is equal to nothing), everything else as `same?`. Values of different types are never equal: `(equal? 1 1.0)` is `#false` (6.6.1). |
| `(greater-than? a b)`, `(less-than-or-equal? a b)`, `(greater-than-or-equal? a b)` | The rest of the comparisons, as Rust's `>`, `<=`, `>=` (12.5.3). |
| `(minimum a b)`, `(maximum a b)` | The smaller, the larger, of two integers or two floats (12.5.13). |
| `(absolute-value x)`, `(power x y)` | Of an integer or a float, as `add` is: two integers give an integer (Rust's checked `abs` and `pow`), two floats a float (`abs` and `powf`), and mixing them is a `type-error`. `(absolute-value -9223372036854775808)` is `overflow`, and so is an integer power beyond 64 bits; an integer power with an exponent below 0, which isn't an integer, is a `range-error` (12.5.12, 12.9.6). |
| `(find-association x pairs)`, `(member? x list)` | The first pair whose `first` is `equal?` to `x`, or `()`; whether `x` is `equal?` to an element (6.3.6, 6.3.7). |
| `(binds? c symbol)` | Whether the symbol is bound in context `c` or one of its parents (6.7.1), with both evaluated, so the symbol can be computed: `(binds? table (string-to-symbol key))`. It is `lookup` with a value no program holds as `otherwise`, so a `type-error` names `lookup`. |
| `(symbol-to-string s)` | A symbol's name. |
| `(float-to-string f)` | A float's printed text, which reads back as the same float: `(float-to-string 1.0)` is `"1.0"`. Anything else is a `type-error`. |
| `(string-search text pattern start)` | The byte offset of the first place at or after `start` where `pattern` is in `text`, or `()`; an empty pattern is at `start`. A `start` out of range or off a character boundary is a `range-error`. |
| `(bytes-search b pattern start)` | `string-search` over bytes: the offset of the first place at or after `start` where `pattern`'s bytes are in `b`, or `()`; an empty pattern is at `start`. A `start` out of range is `bytes-slice`'s `range-error`. It isn't native, as no program searches bytes yet: 9 KB takes about 12 ms. |
| `(string-starts-with? s prefix)`, `(string-ends-with? s suffix)` | Whether `s` starts or ends with those bytes. |
| `(string-split text separator)`, `(string-join strings separator)` | The parts of `text` between the separators, empty ones too, so `(string-split "a,,b" ",")` is `("a" "" "b")` and `(string-split "" ",")` is `("")`; an empty separator is a `range-error`. The strings with the separator between them. `string-join` undoes `string-split`. |
| `(string-trim s)` | `s` without the ASCII whitespace at either end (space, tab, newline, form feed, return), as Rust's `trim_ascii`. |
| `(string-less-than? a b)` | Whether `a` comes before `b` in byte order, as Rust's `<` on `str`: at the first byte where they differ, or the shorter first when one starts the other. |
| `(string-repeat s count)` | `s` `count` times, as Rust's `str::repeat`; a count below 0 is a `range-error`. |
| `(string-pad-left s width fill)`, `(string-pad-right s width fill)` | `s` with copies of `fill`, a string of one byte, before it, after it, up to `width` bytes; `s` itself if it is that long already: `(string-pad-left "7" 2 "0")` is `"07"`. A `fill` of any other length is a `range-error`. |
| `(string-to-lowercase s)` | `s` with `A` to `Z` made `a` to `z` and every other byte as it is, as Rust's `to_ascii_lowercase`. |
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

**What the library forms cost.** `if!` and `sequence!` are native primitives, operatives whose
meaning is their definition in `natives.grift`. The other forms with a body walk
their operands in grift, so in a loop run per byte or per node each costs a few calls. Measured with
the runner's `elapsed-nanoseconds` (release, one run of 200,000 turns, at this version), on top of a
loop of one `if!` and a call, 0.32 µs a turn: another `if!` 0.05 µs, `sequence!` of two forms
0.07 µs, `let!` of one binding 1.95 µs, `and!?` of two 1.19 µs, making a `lambda!` 1.0 µs, and
`conditional!` of three clauses, the last one true, 2.1 µs more than the same choice in nested
`if!`s. A library combiner such as `second` costs a call, about 0.9 µs, where `(first (rest x))`
costs 0.1 µs. Nested `if!`s are the cheapest branch; the other forms read better where speed
doesn't matter.

## Errors

Every error stops evaluation and goes to the innermost `catch!` around it, or to the host, as a
kind, the values involved, and a trace: the combinations of the program being evaluated,
innermost first, each once, out to the top-level form (the library's own code and code built
around a combiner value are skipped). A combination in tail position has finished when the next
one starts, so it isn't listed. Cleanups (`finally!`) run on the way out. A combination read from
a named text, such as a file the runner runs or loads, keeps its source and line beside it, not in
it, so the host reports `in (first x) at lib/table.grift:12`.
Rust hosts can read such forms without evaluating them with `Grift::read_in(source, text)`, which
returns the list of all forms, as `Grift::read(text)` does. `Grift::position` gives each combination's
source and opening line, also after a loader evaluates it later. If `read_in` or `evaluate_in`
cannot read the text, its `parse-error` keeps `(reason line column)` in the values and the supplied
source name as a symbol innermost in the trace. `read` and `evaluate`, which take no source name,
keep an empty trace for unreadable text. A loader can propagate the named error unchanged; the
runner's loaded-file error starts with that path, followed by the outer load's combination.
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
(catch-kind! (first 1) type-error e 0)                                  ; => 0
```

**What can be caught.** Errors about the program can: the primitives', the library's, a host
function's, and the program's own. Limits set by the host or the machine can't: `interrupted`
and `abandoned` (the host's) and `out-of-memory` (the allocator's) go on through every `catch!` to
the host, so a `catch!` in a loop can't swallow C-g. They run cleanups on the way, as any error
does, though a stop can cut one and a failed allocation can pass one over (`evaluate-with-cleanup`).
An ordinary error in one of those cleanups doesn't replace them, but another limit does. Whether an error
is a limit goes by its kind: a program that raises `interrupted` itself, or a host function that
reports `out-of-memory`, isn't caught either.

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
| `type-error` | a primitive got the wrong kind of value, integers and floats mixed, a test that isn't a boolean; an improper operand list given a primitive or an applicative; an element or a tail given `list-to-bytes` that isn't an integer or `()` | the combiner and the value; for `list-to-bytes`, the primitive, the value and its position in the list |
| `arity-error` | a primitive got the wrong number of operands; a parameter tree didn't match, as when a library form gets operands that aren't its shape | the primitive and the operands; or the parameters and the value |
| `not-a-combiner` | a combination's head isn't a combiner | the value |
| `parameter-error` | a malformed parameter tree, a symbol twice, or a bad context parameter | the tree or the context parameter |
| `overflow` | integer arithmetic beyond 64 bits, `absolute-value` and `power` included; a shift by an amount outside 0 to 63 | the primitive or library combiner's name, and the operands |
| `division-by-zero` | integer `divide` or `remainder` by 0 | the primitive and the operands |
| `range-error` | a string offset off a character boundary or out of range; a bytes offset out of range; an integer outside 0–255 given `list-to-bytes`; a code point that isn't one; an empty separator given `string-split`; a list index out of range; a negative count or exponent; a fill that isn't one byte | the primitive and the operands; or the library combiner's name and the value; for a list index, the name, the index and the length; for `list-to-bytes`, the primitive, the integer and its position |
| `encoding-error` | bytes given `bytes-to-string` that aren't UTF-8 | the primitive, the bytes, and the offset where the first sequence that isn't UTF-8 starts (Rust's `valid_up_to`) |
| `parse-error` | text that doesn't read: a program or `read-form`'s text, or a string given `string-to-integer` or `string-to-float` | why, then the line and the column, which counts characters from 1; or the primitive and the string |
| `out-of-memory` | the allocator refused | none |
| `stale-value` | the host used a value the collector reclaimed | none |
| `foreign-value` | the host gave one interpreter a value, a symbol or a host function of another | none |
| `interrupted` | the host stopped the run in progress; stops waiting together count as one, and a later stop stops the cleanups too, even one that would have ended | none |
| `abandoned` | the host ended a run that was waiting, at the call it was waiting in | none |
| `cannot-park` | a call that would wait came where nothing can be set aside: see Calls that wait | the host function |
| any symbol | a program raised an error it made | the values given |

A text that doesn't read says why, as a symbol, and where: `unclosed-list` and `unclosed-string`
at the `(` or `"` left open (the innermost, for lists), not at the end of the text;
`unmatched-parenthesis` at a `)` with no `(`; `misplaced-dot` at a `.` or what follows it outside
`(a . b)`; `bad-escape` at a `\` that isn't one of the escapes; `bad-number` at a token that starts
as a number and isn't one; `unknown-constant` at a `#` name that isn't one; `bad-byte` at what
isn't an integer 0 to 255 inside `#bytes(`. `(read-form "(a\n (b" 0)`
is `parse-error unclosed-list 2 2`.
A host function that replaces its interpreter ends the run with `foreign-value` too.

The runner adds its own operations and its own kinds; [the README](../README.md#the-runner)
describes each. Those added with the evidence of wave 4:

| Operation | Meaning |
|---|---|
| `(load path)` | The file's forms, all read first, evaluated in turn in the caller's context, as Kernel's `load`; a relative path is relative to the file being loaded when `load` is called (at first the script). Its combinations keep their lines, so the runner reports an error `at path:line`. |
| `script-path` | The script's path as given; unbound for a program on stdin. |
| `(read-file path)` | The file's whole text. |
| `(display-error v ...)` | `display`, to stderr. |
| `(read-bytes count)`, `(write-bytes bytes)` | On reader and writer contexts (`open-file`'s, `create-file`'s, `standard-input`, `standard-output`, `standard-error`): a bytes value of `count` bytes as soon as they have come, fewer at the end of input, `#bytes()` at the end; a bytes value's bytes, as they are (anything else is a `type-error`). |
| `(wall-clock-nanoseconds)`, `(elapsed-nanoseconds)` | The system's time since 1970 UTC, which can be set back; the time since the run started, which never goes back. |

## Calls that wait

A host can register a function that answers later. To a program it is a call like any other: there
is no new form or name, and nothing marks it.

```lisp
(finally!
  (sequence!
    (send pane "make\n")
    (wait-for pane "done")   ; the host sets this run aside; its loop and other runs go on
    (send pane "exit\n"))
  (close-log log))
```

The program reads top to bottom. Every `catch!` and `finally!` around the call is still there when it
returns, the trace of a later error shows the whole path, and the call can be inside `map`,
`for-each`, `let!`, `load`, or a cleanup. It returns a value, or raises an error, once.

What a program may see change across such a call: **other runs went on meanwhile.** They share the
bindings, so `(bind here (quote! n) ...)` by another run is seen after the wait, and a value read
before the wait may be out of date after it. Nothing else changes: what the run holds itself, its
locals and arguments, stays as it was, however long it waits. Runs take turns, one at a time, and
only at these calls: between two waiting calls no other run can come in.

The host may end the waiting run instead. The call then raises `abandoned`, which no `catch!` catches,
and the cleanups run on the way out. In such a cleanup a call that would wait fails with `cannot-park`:
nothing could take the run up again. A cleanup can catch that and go on.

A call that would wait inside a run that can't be set aside, because it runs under `Grift::evaluate`
or `Grift::call` or inside a host function that called back into grift, is the error `cannot-park`,
with the function in its values. It can be caught like any other error.

A host that drops its interpreter with runs waiting runs none of their cleanups, as if its process
had died. The README's embedding section gives the host's side.

## Where grift differs from R-1RK, and why

[VISION "Kernel"](VISION.md#kernel) gives the larger differences. Beyond those:

- **Three operative primitives.** `if!`, `define!` and `sequence!` are primitive in Kernel; here `define!`
  is library code over `vau!`, `bind` and `evaluate`, and `if!` and `sequence!` are native, each
  defined in grift over those and `select`.
- **`finally!`** is on `evaluate-with-cleanup` (not in Kernel), the exit half of Kernel's guards;
  `binds?` is derived from `lookup`, a primitive Kernel doesn't have, where R-1RK derives `$binds?`
  from its error continuation: a miss raised and caught an error, 5-14 µs a probe.
- **Binary arithmetic and comparison.** `add`, `multiply`, `less-than?`, `minimum` and the rest
  take two operands; Kernel's take any number. The type predicates take one value.
- **Bytes beside strings.** Bytes are a value of their own, never a string, with their own
  literal; `same?` compares them by content, as strings.
- **Strings are immutable,** and `same?` compares them by content, like integers. R-1RK has no
  strings; grift's offsets count bytes of UTF-8, so `string-byte` is O(1), and there is no
  character type: a code point is an integer.
- **`apply` has one form,** `(apply a arguments)`, with a new empty context; `for-each` takes one
  list, and `map` lists of one length, as Kernel's does. `context` takes at most one parent.
- **New, not in Kernel:** `bind`, `unbind`, `lookup`, `names` and `built-in-names`, `select`,
  `type-of`, `when!`, `error`, `raise`, `catch-kind!`, `reverse`, `read-form`, `value-to-string`, bytes,
  `any?`, `all?`, `fold`, `range`, `sort`, the list access by index, the bitwise
  operations, and the string and float primitives and helpers.
- **`lambda!` keeps the symbol `sequence!`** in a body of several forms, so the body prints as
  written. It is looked up where the body runs, like any other name in it.
