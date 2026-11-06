# The grift guide

What grift is, and the decisions behind it. [`LANGUAGE.md`](LANGUAGE.md) is the reference;
[`DECISIONS.md`](../DECISIONS.md) records each decision and why.

## Purpose
grift is a Lisp built entirely on fexprs: `vau!` is its one primitive operative, and there are no
macros and no other special forms. Everything else, `if!` included, is a grift definition you can
read and redefine. It is both an extension language for Rust programs, reshaped by their users
while they run, and a general scripting language with a Unix runner.

## Nothing guessed
grift is meant to be worked with without reading its source, by people and by agents alike: every
definition can be listed and printed, nothing is a made-up default, and every error says what went
wrong, with the values involved and where. That is ordinary good language design, not a separate
mode; grift adds nothing special for agents.

## Kernel
grift is heavily inspired by Kernel and uses R-1RK's derivations where they fit, but it makes its
own decisions where they make grift simpler or more honest, each recorded in DECISIONS.md: only
bindings change; errors are caught with `catch!`; there are no continuations; names follow
grift's own rules (Kernel's *environment* is grift's *context*); `let!` binds in order in one new
context, so a combiner it binds can call itself and the bindings after it (other Lisps call this
`letrec*`).

## Fexprs
An operative receives its operands unevaluated together with the caller's context, and evaluates
what it chooses, where it chooses, explicitly. Nothing is captured by accident, so fexprs are
hygienic by construction; that is why grift has no macros. The known cost (Mitchell Wand, 1998) is
that code can't be analysed or optimised before it runs, because the meaning of any form can
change. grift accepts this on purpose: it is interpreted, and a redefinition always takes effect.
By convention an operative's context parameter is called `caller`.

## Values
grift has few kinds of value, and each earns its place:
- integers (64-bit; overflow is an error, never a silent rollover) and floats (64-bit IEEE); inside
  grift they are called integer and float, never `i64` and `f64`;
- the booleans `#true` and `#false`;
- symbols;
- pairs, and the empty list `()`; a list is a chain of pairs ending in `()`, and `first` and `rest`
  take a pair apart, named for their use on lists;
- strings (immutable UTF-8 text, addressed by byte);
- bytes (immutable, any bytes: what a file, a pipe or a host gives that isn't text);
- contexts;
- combiners (operatives, applicatives, primitives and host functions);
- `#inert` (the value of a form with nothing useful to return) and `#ignore` (a parameter that
  binds nothing).

There is no truthiness: the tests of `if!`, `when!`, `conditional!`, `and!?` and `or!?` must be
booleans, and anything else is a type error. Nothing is guessed for a missing branch either: `if!`
needs both, and `when!`, which has one and a body, gives `#inert` when its test is false. Integers
and floats never mix implicitly: `(add 1 1.5)` is a type error, and conversions are explicit. There is one integer type (no `usize` or other widths)
and no vector type. Strings stay UTF-8 text and bytes are never a string: converting bytes to a
string refuses any that aren't UTF-8, and says where. Host types are not grift values: the host converts at the boundary (checked;
`range-error` when a number doesn't fit) or gives grift a context of operations.

## Numbers
Rationals, big integers and any richer tower are grift library code, used by evaluating in a
context whose arithmetic is theirs; nothing pays for a tower it doesn't use. Floats are hardware
floats with Rust `std`'s operations and functions (`square-root`, `sine`, `exponential`, ...);
results may differ slightly between platforms, as they do in Rust.

## Close to Rust
grift is a fairly low-level scripting language and mirrors Rust where the meaning is the same:
Rust's numeric semantics (integer `divide` and `remainder` are Rust's `/` and `%`;
`float-to-integer` is exactly Rust's `as`, saturating), `std` underneath, nothing hidden between a
grift operation and the Rust it runs. The shipped version of anything is the most minimal, most
Rust-like one; a program that wants stricter behaviour redefines it.

## Names
A name says what the thing does in grift; Rust decides nothing by itself. Words are joined with
dashes, as in Lisp, and every word is spelled out, with no abbreviations and no exceptions (`vau!`
follows `lambda!`: both name the abstraction they make).
- A combiner that gets its operands unevaluated ends in `!` (`lambda!`, `if!`, `define!`), as a
  Rust macro does. `!` never means "changes something", as it does in Scheme.
- A combiner that returns a boolean ends in `?` (`empty-list?`, `not?`); an operative that does,
  in `!?` (`and!?`).
- A constructor is named after what it makes (`pair`, `list`, `context`, `error`).
- A conversion spells out `to` (`symbol-to-string`, `string-to-integer`).
- Arithmetic and comparison are words, never operator symbols (`add`, `divide`, `less-than?`), as
  in fuxth.
- Only bindings change, so the combiners that change anything are a short, complete list of verbs
  about bindings: `define!`, `bind` and `unbind`.
Kernel's markings are respelled, not lost: `$` became the `!` suffix, and Kernel's mutation `!`
became the verb itself. Equality is `same?` and `equal?`: `same?` is identity for pairs, contexts
and combiners, equality of value for integers, symbols, strings and bytes, and equality of the
stored bits for floats; `equal?` compares pairs by structure, floats as IEEE does (NaN is never equal), and
everything else as `same?` does. Values of different types are never equal (`(equal? 1 1.0)` is
`#false`).

## Embedding
grift is a Rust library with zero dependencies, using `std`. It does no I/O, starts no threads and
keeps no global state: the host owns those, lends its state per call, and sets every limit, a time
limit being a thread that stops the run. Every value the host holds says which interpreter it
belongs to, and every reference which object it was, so a value from another interpreter or one
the collector reclaimed is an error, never read as another. A value the host keeps across runs is
a binding the interpreter owns, released by the host.
An evaluation or call can collect before it needs its first heap slot, with its checked inputs
kept live. After out-of-memory, the host can start one to let discarded allocations be reclaimed;
allocation itself never collects.
`read_in` reads a named text without evaluating it, keeping each combination's source and line;
the host or a loader written in grift chooses when and where to evaluate those forms. Parse errors
keep their reason, line and column; named reading and evaluation retain the supplied source in
the trace too.
Bare-metal targets are not a goal; WebAssembly works wherever `std` does. grift does not isolate
code from the host or from other code in the same interpreter; an embedder running untrusted code
isolates it itself, for example in its own process.

## State
Only bindings change: a context gains, replaces or loses a binding, and no other value ever
changes. You can change only a context you hold; a child never gives access to change its parent
(there is no parent accessor). That is why there is no `set!`: `define!` binds in the context it
is evaluated in, so in a body it makes a local, and changing an outer binding means holding that
context and binding there, `(bind here (quote! counter) (add counter 1))`. Records, cells and symbol-keyed tables are contexts. There is no
object system: no classes, prototypes, `send` or method syntax, only contexts. To call an operation
a context holds, look it up there and call it where you are: `((evaluate-in! write f) line)`
evaluates `line` in your context, not in `f`.

## Contexts
A context is a hash map from symbols to values, of any size, with at most one parent it reads
through. `names` lists a context's own names (not its parent's), sorted. A child sees its parent's
bindings but can change only its own. Programs run in a child of the built-in context, which holds
the primitives and the library and which no program can reach or change; `built-in-names` lists
its names.

## The host boundary
A host resource (a buffer, a file, a socket) appears in grift as a context of operations on it.
Its operations call host functions bound only in a private context, so a program can't forge one
from an id. Closing is explicit (no finalizers), and using it after that is the error `closed`.

## The runner
The `grift` runner is a Unix tool: it runs a script or stdin, prints data to stdout and diagnostics
to stderr, and exits 0, 1 on an error, 2 on a usage error. It gives programs a small, fixed set of
host operations: reading stdin as data, writing to stderr, files (as contexts of operations, as
text or as bytes), loading another file's forms, the command-line arguments and the script's
path, environment variables, two clocks, the exit status, and running processes.

## Implementation
The implementation stays small and uniform: one evaluation step for every kind of form, one
representation for each kind of value. Speed comes from simpler library code and better
representations. When simple and fast disagree, simple wins and the cost is written down. Library
forms are `vau!` operatives that walk their operands and call `evaluate`; no expansion caching, no
compile step. A library form may have a native twin, proven equal and justified by a measurement on
`programs/`: the library's own operative, run in Rust when its operands are the shape the twin
handles and by its body otherwise, so a program can't tell them apart (`if!`, `sequence!` and
`string-search`, owner, 2026-11-02). Everything else keeps one evaluation path per kind of form.
The primitives are core, what grift can't express, or native, there for speed, each with its grift
definition as its meaning and a test that no program can tell the two apart.

## Errors
An error has a kind (a symbol such as `type-error`), the values involved, and a trace: the program
combinations it happened in, innermost first, each with its source and line when it was read from a
named text, so the host says `at config.grift:12`. `(error kind value ...)` makes an error and `raise`
raises it: `(raise (error (quote! type-error) x))`. `finally!` runs cleanup as an error passes and
lets it go on, unless the cleanup fails or a limit cuts it (`LANGUAGE.md` has each case).
`catch!` catches: cleanups run first, then the handler receives the error as a context with no parent, binding `kind`, `values` and `trace`; `(raise e)` passes a caught error on,
keeping its trace. Errors about the program can be caught; limits set by the host or the machine,
`interrupted` and `out-of-memory`, never can, and always reach the host. Early exit is `catch!` with
an error kind of your own; there is no other escape. (Unlike Common Lisp's `catch`, which is a
non-error escape, grift's `catch!` catches errors.) Under them are two primitives,
`call-with-handler` and `evaluate-with-cleanup`.

## No continuations
Generators and backtracking are closures or lazy streams; a language built on grift that wants
`call/cc` writes its evaluator in continuation-passing style, which proper tail calls make cheap.
Continuations would cost the meaning of cleanup and of interruption, and the simplicity of the
machine.

## Printing
Every value prints as text that reads back where it can. A combiner prints its source: its
parameters, its context parameter and its body (not the context it was made in, so two combiners
can print alike and differ). A context prints as its own names, sorted
(`#[context close path read-all read-line]`), never its values, which may reach the context
again. An error, a context, prints its names; the host reports its kind, its values and its trace.
`write_string` writes the same string literal spelling for host text, without an interpreter or
allocation. It and the value printer share the quoting and escapes the reader understands.

## Terms
- **Symbol:** a value that is a name; a symbol names a value in a context.
- **Binding:** one name with its value in a context.
- **Context** (Kernel: *environment*): a map from symbols to values, with at most one parent it
  reads through; it fixes what each name refers to.
- **Built-in context:** the context holding the primitives and the library (Kernel: the *ground
  environment*).
- **Combiner:** anything that can be called: an operative or an applicative.
- **Operative:** a combiner that receives its operands unevaluated, with the caller's context
  (made by `vau!`; names end in `!`).
- **Applicative:** a combiner that evaluates its operands first and passes the values on (made by
  `wrap`, or `lambda!`).
- **Primitive:** a combiner written in Rust as part of grift. **Host function:** an applicative the
  host registers.
- **Combination:** a pair being evaluated: its first element is evaluated to a combiner, the rest
  are its operands.
- **Operand / argument:** what a combination is given / the value of an operand.
- **Parameters:** the tree of symbols an operative or applicative binds its operands or arguments
  to; a `vau!` also has a context parameter.
- **Trace:** the combinations an error happened in, innermost first.

## The ontology
1. **What exists.** The values listed under Values; errors (a kind, values, a trace); the machine
   that evaluates, the heap that holds values, and the host that embeds grift.
2. **One source of truth for each fact.** A binding lives only in its context. A combiner's
   meaning is its parameters, the context it was made in and its body; it prints as its source. A
   library form's meaning is its grift definition, and a native primitive's too: a native twin or
   primitive is proven equal to it. Where a combination was read is the reader's, kept beside the
   code, not in it.
3. **What is derived.** Every library form, from a few dozen primitives. `names`,
   `built-in-names` and printed source, from the definitions. An error's trace, from the
   combinations being evaluated when it happened.
4. **What changes, and who may change it.** Only bindings change: programs through `define!` and
   `bind`, on contexts they hold; the host through `define` and its own state.
   Errors unwind the machine; `finally!` and `catch!` meet them on the way. Collection happens only
   at safe points; a stop the host asks for ends the run in progress there, and never a later one.
5. **What makes a change to grift correct.** No Rust recursion that grows with a program's nesting;
   tail calls in constant space; no limits of grift's own; nothing guessed; every error carries its
   values and its trace; redefinition always takes effect; one evaluation path for each kind of
   form but a native twin, proven equal; a new primitive only when the library version can't exist
   or can't be used.

## What complete means
grift is complete when a closed list of programs runs well on it, each with no blocker in its
friction log: the Emacs-style editor in `examples/emacs` and the programs in `programs/`, which the
gate runs. "Well" means the program's own test passes and its hot path is acceptable for what it
does. Adding a program to the list is a decision, recorded like any other.

## How grift grows
A feature enters only with evidence from real programs (a friction entry with a reproduction: one
blocker, or the same cost in more than one program) and a recorded decision. Library grift comes
before a primitive; a primitive only when the library version can't exist or is measured to be
unusable. Decisions are append-only and reopened only with new evidence from use. Removing things
is welcome: grift is pre-release, so old code is deleted, never deprecated.
