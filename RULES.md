# Rules

What holds on master today, by topic. A link such as [G-170] opens the section of
[`DECISIONS.md`](DECISIONS.md), the history, that holds the entry: find its id in bold there. Only
the latest word on each question is here. Where this file and the code disagree, this file is wrong.

## What grift is

- A small Lisp built on fexprs, as Kernel is: everything that can be grift code is grift code
  ([G-1]). The forms with a body (`sequence!`, `when!`, `conditional!`, `let!`, `catch!`,
  `catch-kind!`, `finally!`) evaluate only the operands they were given and build no code; `lambda!`
  builds the one `vau!` combination it evaluates, as Kernel derives it ([G-31]).
- A feature enters only with evidence from real programs and a recorded decision; library grift
  comes before a primitive, and removing things is welcome ([G-57]). grift is complete when
  `programs/` and the editor in `examples/emacs` run well on it unchanged ([G-84]).
- No guessing: no made-up defaults and no optional operand with a default; the only variable counts
  are rest lists, such as `join`'s operands and `error`'s values ([G-8], [G-14], [G-103]).
- Names are spelled out and joined with dashes: an operative ends in `!`, a predicate in `?`, an
  operative predicate in `!?`, and a conversion says `to` ([G-38]).
- One path per kind of form: speed comes from simpler code and better representations, never a fast
  path beside the real one, except native primitives and twins ([G-33], [G-110]).
- grift sets no limit of its own (no step, depth, time or memory limit) and doesn't sandbox what it
  runs: limits and isolation are the host's ([G-5], [G-74]). What it does is bounded by the work
  asked of it, and what it can't do it refuses with an error, with one exception under Memory
  ([G-178]).
- The core is a Rust library on `std` with no dependencies, I/O, threads or global state beyond the
  counter that numbers interpreters ([G-36], [G-181]). `unsafe` is forbidden there, and the
  workspace denies `unwrap`, `expect`, `panic!`, indexing, `unreachable!`, `todo!` and
  `unimplemented!` (`Cargo.toml`). CI also builds the core for `wasm32-unknown-unknown`.

## Values

- The kinds: the empty list, booleans, `#inert`, `#ignore`, 64-bit integers, 64-bit floats, symbols,
  pairs, strings, bytes, contexts, operatives and applicatives; no vectors, characters or numeric
  tower ([G-9], [G-42], [G-43], [G-44], [G-142]).
- Integer overflow is `overflow`, and integers and floats never mix: `(add 1 1.5)` is a `type-error`
  ([G-42], [G-43]). Integer `divide` and `remainder` truncate as Rust's do; division by zero is
  `division-by-zero` and `MIN / -1` is `overflow`. Float arithmetic is IEEE, so `(divide 1.0 0.0)`
  is `#infinity`, and `float-to-integer` saturates as Rust's `as` does ([G-45]).
- `absolute-value` takes an integer or a float, and `power` two integers or two floats, a negative
  integer exponent being `range-error`; both are Rust primitives ([G-181]). The bitwise operations
  are Rust's on `i64`; a shift outside 0 to 63 is `overflow` ([G-105]).
- No truthiness: the tests of `if!`, `when!`, `conditional!`, `and!?` and `or!?` must be booleans;
  `and!?` and `or!?` check every operand they evaluate, the last too ([G-52], [G-65]).
- `same?` is identity for pairs, contexts and combiners, equality of value for integers, symbols,
  strings and bytes, and of the stored bits for floats; `equal?` compares pairs by structure and
  floats as IEEE does. Values of different kinds are never equal ([G-51], [G-145]).
- Strings are immutable UTF-8 with byte offsets, and a code point is an integer ([G-27]); bytes are
  a kind of their own, never equal to a string ([G-142]). Both share `byte-length`, `byte-at`,
  `slice`, `join` and `search`, the first argument's kind being the answer's ([G-173]).
- An offset out of range is `range-error`, and so is a `slice` or `search` offset off a string's
  character boundary ([G-27]). `less-than?` orders two integers, floats, strings or bytes values
  ([G-177]). Bytes that aren't UTF-8 are `encoding-error` from `bytes-to-string` ([G-144]).

## Reading and printing

- Floats, strings and bytes print as text that reads back to the same value, but a not-a-number
  ([G-61], [G-78], [G-142]). A symbol prints its bare name, which may not read back ([G-173]), and
  combiners and contexts print as `#[...]`.
- The string escapes are `\\ \" \n \r \t` and `\xHEX;` ([G-122]). In `#bytes(104 105 255)` an
  element is read like any datum and must then be an integer 0 to 255, else `parse-error bad-byte`
  ([G-142], [G-177]). `#!` starts a comment wherever a token can ([G-172]).
- `(read-form text offset)` gives `(form start end)`, or `()` at the end, through the host's reader
  ([G-77]). A `parse-error`'s values are `(reason line column)`, an unclosed list or string placed
  at its opening character; under a source name, the source is the last value ([G-97], [G-177]).
- A context prints as its own names, sorted, never its values ([G-54]); in an error report it shows
  only how many names it has ([G-178]).

## Contexts and forms

- A context is a hash map from symbols to values with at most one parent it reads through; a child
  changes only its own bindings, and there is no parent accessor ([G-34], [G-39]). Records, cells
  and tables are contexts; there is no object system ([G-40]).
- Programs run in a child of the built-in context, so a top-level definition shadows a library name
  without changing what the library uses ([G-10]). The built-in context is out of reach;
  `(standard-context)` makes a child of it, `built-in-names` lists it, and `(context parent)` is
  `context`'s one form ([G-20], [G-172]).
- Only bindings change. `define!` binds in the current context, so in a body it makes a local; there
  is no `set!`: change an outer binding through its context with `bind` ([G-38], [G-95]). `unbind`
  removes a binding from that context only ([G-96]).
- `(lookup context symbol otherwise)` is the primitive and `binds?` is library over it ([G-104]);
  `(bindings c)` lists a context's own `(name value)` pairs by name ([G-121]). Every kind has a
  predicate, `inert?` and `ignore?` included ([G-172]).
- `if!` needs both branches; `when!` has one, and its body may be empty ([G-71], [G-72]).
  `conditional!` with no true clause gives `#inert` ([G-19], [G-173]). `let!` binds in order in one
  new context, so its combiners can call themselves ([G-47]). `vau!` takes any number of body forms
  ([G-172]).
- The list helpers call through `apply`, Kernel's two-operand form; `map` takes lists of one length
  ([G-14], [G-81]). `sort` is stable ([G-101]). An index or count out of range is `range-error`,
  never a shorter list ([G-102], [G-173]).
- No continuations and no escape primitive: early exit is catching an error of the program's own
  ([G-35], [G-53], [G-171]).

## Primitives, natives and twins

- A primitive is core when no grift definition could behave the same, native when it exists only for
  speed. Each native keeps its grift definition in `natives.grift`, and the test
  `natives_are_their_definitions` runs them in place of the Rust to show no program can tell the two
  apart, except by printing the native itself ([G-110], [G-115], [G-172]). `absolute-value`,
  `power`, `remainder` and `less-than?` are defined in grift for part of their inputs only and call
  the Rust primitive for the rest (`natives.grift`).
- `if!` and `sequence!` stay grift in `library.grift`, with native twins for the shapes they handle
  ([G-117], [G-173]). `error`, `equal?`, `catch!`, `finally!`, `catch-kind!`, `float-to-string` and
  `symbol-to-string` are library grift ([G-48], [G-64], [G-66], [G-86], [G-93], [G-99]).

## Errors and traces

- An error is a kind, a list of values and a trace; a caught one is a context with no parent binding
  `kind`, `values` and `trace` ([G-50]). Its values are the values involved, never the name of the
  combiner that failed ([G-173]).
- `error` makes an error and raises nothing; `raise` raises one, new or caught ([G-49]). `catch!`,
  `catch-kind!` and `finally!` are library operatives on `evaluate-with-handler` and
  `evaluate-with-cleanup` ([G-48], [G-99], [G-170]).
- A trace lists the program's combinations around the error, each once, from the innermost out to
  the top-level form; a finished tail call isn't listed ([G-29]). Library code is skipped, by a mark
  set when a pair is made ([G-173], [G-180]). Every `raise` adds its own combination, a re-raise
  included ([G-170]).
- The host's `Error::trace` is innermost first. A program reads it the other way round, places added
  last first, and a catch and raise adds only the places since ([G-179]).
- Where a combination was read lives in its pair; `Grift::position` gives its source and line
  ([G-177]). A named outer form keeps its place through its tail calls, until a tail call evaluates
  another named outer form, which takes its place ([G-154], [G-157]).

## How a run ends

- A run ends one way: it unwinds with a reason. An error carries one bit, `limit`, which only the
  host (a stop, a force, a host function's error with `limit` set) and the machine (memory refused)
  can set; a handler runs only if it is clear, and no kind's name decides anything ([G-170]).
- So a program can't forge a limit: its error of any kind, `interrupted` included, is caught. Of
  reasons arriving during a cleanup the newest wins, unless a program's would replace a limit
  ([G-170]).
- The host can always end a run, and that beats "cleanups always finish": a stop can cut a cleanup
  that has begun ([G-168], [G-170]). A cleanup's install makes the room its start needs ([G-170]).
- `Grift::interrupter` gives a handle for the run in progress, or else the next to begin; its `stop`
  and `force` reach only that run, and a clone is for the same run ([G-175]). A stop ends the run
  with `interrupted`; a force also cuts every cleanup it meets; `true` from either says the run was
  in progress, not that it acted ([G-168], [G-170]).
- A stop asked before its run starts does nothing ([G-179]). The host's `read`, `read_in`, `write`,
  `describe` and `bind` are runs a stop reaches, or join the run in progress inside a host function
  ([G-179], [G-181]). Inside a step, list walks, `search` and the printer look for stops as they go
  ([G-178], [G-179]).
- A host function that panics ends its run: the panic goes on to the host, no cleanup runs, and the
  next evaluation starts afresh ([G-168]).

## Parked runs

- A host function answers `Answer::Later(request)` to set its run aside, as an entry of the kept
  table, leaving the interpreter free ([G-169], [G-177]). `Parked` is neither `Copy` nor `Clone`,
  and `resume` takes it by value, so a run goes on at most once ([G-169]). There is no `abandon`: a
  host gives a run up by answering its wait with a limit ([G-170]).
- A parked run lives until resumed or the interpreter is dropped, which drops it with no cleanups;
  `parked` counts every parked run, and `forgotten` gives a new handle for one whose handle was
  dropped ([G-169]).
- Each `resume` is a new run, so a host takes an interrupter just before it; no stop reaches a
  parked run ([G-169], [G-175]). Where a run can't park, `Later` fails with `cannot-park`;
  `can_park` is `false` with no run in progress ([G-169], [G-180]).

## Host API

- `Grift<T>` lends the host's state, `&mut T`, to each run and on to host functions ([G-23]). A host
  function gets `&mut HostCall<T>`, which makes, binds and keeps values but can't begin a run: that
  doesn't compile ([G-181]). To have grift call something it answers `Answer::Call`, run on the
  machine's own frames, so recursion through a host costs heap, not Rust stack ([G-178]).
- Every host function is registered with `host_function` and returns `Result<Answer, Error>`
  ([G-178]); it is a value bound nowhere, which the host binds or hands to grift code that keeps it
  private ([G-60]). A host resource is a context of operations on it, closed explicitly, and use
  after closing is `closed` ([G-41]).
- `context`, `private_context`, `top_level`, `bind`, `lookup` and `names` follow the primitives'
  rules; `evaluate_in` reads a named text whole before running it, `read_in` reads one without
  running it, and `evaluate_library_in` marks what it runs as library code ([G-118], [G-151],
  [G-173]). `list`, `list_items`, `as_str`, `bytes` and `as_bytes` convert ([G-114], [G-147],
  [G-174]).
- `Value`, `Symbol`, `Reference` and `Operative` are `Copy`. One that belongs to an interpreter
  (anything but immediate data and primitives) carries its number, and another interpreter's is
  `foreign-value` ([G-181]). Every value handed in is checked for its interpreter, generation and
  kind, so nothing stale or foreign is stored ([G-114], [G-132]).
- `Error` has neither `Display` nor `Clone`. `describe(&error, &mut out, bytes)` writes it within a
  byte budget, kind and innermost place first, and says `Complete` or `Truncated`; printing fails
  with an ordinary `Error`, and `TextBuffer` refuses rather than aborts ([G-32], [G-177], [G-178]).

## Memory

- The heap is a growable vector with mark-and-sweep collection and no limit of its own ([G-2]). In
  the core, collections grow fallibly and memory refused is `out-of-memory`, not an abort ([G-32]);
  clippy's `disallowed_methods` refuses aborting allocations ([G-179]). The exception is
  `Grift::new`'s shared control block, an `Arc::new` ([G-178]).
- Collection happens only at safe points: the top of the machine loop, and a host's entry before its
  run needs a slot. Allocation never collects, and nothing roots by hand ([G-12], [G-155]).
- A reference carries its slot's generation: a collected value is `stale-value`, never another
  object, and a slot is retired at its last generation ([G-11], [G-133]). Names are collected like
  any value, so a host's `Symbol` can go stale too ([G-179]).
- A value returned to the host lives until the next run; to keep one, bind it or `keep` it. A `Kept`
  is neither `Copy` nor `Clone` ([G-21], [G-108], [G-172]).
- Only memory really refused asks for a collection ([G-170]). A run refused memory gives up a spare
  64 KiB so its report can say where ([G-176]). A collection needs no memory of its own: each slot
  reserves its mark-stack entry when made ([G-177]).
- A collection waits for as much allocation as the largest part of what it last walked (what
  survived, the roots, half the free slots; at least 4,096 slots), so collecting costs a constant
  factor of what a program allocates; free slots at the top of the heap are given back ([G-180]).
  Strings and bytes count their storage ([G-143], [G-172]).

## The runner

- `grift SCRIPT ARGUMENT...` runs a script; with none it reads stdin, and at a terminal
  `runner.grift` runs a session ([G-69], [G-172]). Line editing is `rlwrap`'s job ([G-6]).
- The first argument is always the script. One that can't be read is `file-error`, or
  `encoding-error` if its text isn't UTF-8, status 1, and one that isn't there also prints the usage
  ([G-172], [G-173]). A path or argument that isn't UTF-8 is `encoding-error` before anything runs
  ([G-173]).
- `exit` is a limit of kind `exit` carrying the status: nothing catches it and cleanups run
  ([G-170]). A write to a pipe no one reads is a `broken-pipe` limit, ending quietly with 141
  ([G-170], [G-173]). An uncaught error ends a script with status 1, its description cut at 4,096
  bytes and written at once; at a terminal the session reports it and goes on ([G-175], [G-178]).
- Programs get `display`, `display-error`, `read-file`, `read-forms`, `position`, `standard-input`,
  `standard-output`, `standard-error`, `open-file`, `create-file`, `run-process`, `load`,
  `command-line-arguments`, `script-path`, `environment-variable`, `wall-clock-nanoseconds`,
  `elapsed-nanoseconds` and `exit` ([G-55], [G-106], [G-172]).
- Standard streams are files like any other, with `path` and `close`; `write` takes a string or
  bytes ([G-172]). Memory refused for a process's output is the `out-of-memory` limit ([G-175]).
- `(load path)` evaluates a file in the caller's context, a relative path resolved against the
  working directory ([G-98], [G-173]). `script-path` is always bound, `""` on stdin ([G-172]).
- `runner.grift` runs in a private context as library code, so programs can't intercept it and
  traces list only theirs ([G-60], [G-124], [G-173]). A session waits for more input exactly when
  the reason is `unclosed-list` or `unclosed-string` ([G-97], [G-175]).

## Testing

- A review finding or bug report goes into the program generator, a native's twin or a guard, in
  that order; a new scenario test says in its PR why none of those can ([G-179],
  [CONTRIBUTING.md](CONTRIBUTING.md)).
- `scripts/test-ratio.py` fails CI when test characters per production character rise above
  `scripts/test-ratio.baseline`, and also when the ratio falls more than 0.01 below it, naming the
  new baseline to write ([G-179]).
- A test is deleted only when the held-back bug set shows the suite still catches all it caught
  ([G-179]). `scripts/historic-bugs.py` re-applies the gated bugs in `tests/historic/` and checks
  their tests still fail, nightly and on PRs that change tests ([CONTRIBUTING.md](CONTRIBUTING.md)).
- The workspace tests run every program's own `test.sh` (`every_program_passes_its_own_test`)
  ([G-84], [G-166]).
- Every primitive is classed in one test-side `match` with no catch-all arm, which puts it in the
  deep-data and memory-refusal sweeps, and in the stop sweep if it walks data ([G-179]).
- The machine loop's data shape is asserted at compile time (a frame 56 bytes, a value 16); that
  can't catch code layout, so speed is measured on desk, the quiet machine ([G-175], [G-177]).

[G-1]: DECISIONS.md#owner-decisions-2026-10-31
[G-2]: DECISIONS.md#owner-decisions-2026-10-31
[G-5]: DECISIONS.md#owner-decisions-2026-10-31
[G-6]: DECISIONS.md#owner-decisions-2026-10-31
[G-8]: DECISIONS.md#owner-decisions-2026-10-31
[G-9]: DECISIONS.md#owner-decisions-2026-10-31
[G-10]: DECISIONS.md#made-during-the-rewrite-2026-10-31-the-owner-may-overrule-any
[G-11]: DECISIONS.md#made-during-the-rewrite-2026-10-31-the-owner-may-overrule-any
[G-12]: DECISIONS.md#made-during-the-rewrite-2026-10-31-the-owner-may-overrule-any
[G-14]: DECISIONS.md#made-during-the-rewrite-2026-10-31-the-owner-may-overrule-any
[G-19]: DECISIONS.md#made-after-three-editors-were-built-on-grift-2026-11-01-the-owner-may-overrule-any
[G-20]: DECISIONS.md#made-after-three-editors-were-built-on-grift-2026-11-01-the-owner-may-overrule-any
[G-21]: DECISIONS.md#made-after-three-editors-were-built-on-grift-2026-11-01-the-owner-may-overrule-any
[G-23]: DECISIONS.md#made-after-three-editors-were-built-on-grift-2026-11-01-the-owner-may-overrule-any
[G-27]: DECISIONS.md#owner-decisions-2026-11-01-after-twelve-programs-were-written-on-grift
[G-29]: DECISIONS.md#made-with-g-26-to-g-28-2026-11-01-the-owner-may-overrule-any
[G-31]: DECISIONS.md#made-with-g-26-to-g-28-2026-11-01-the-owner-may-overrule-any
[G-32]: DECISIONS.md#owner-decisions-2026-11-01-during-the-work-on-g-26-to-g-31
[G-33]: DECISIONS.md#owner-decisions-2026-11-01-during-the-work-on-g-26-to-g-31
[G-34]: DECISIONS.md#owner-decisions-2026-11-01-during-the-work-on-g-26-to-g-31
[G-35]: DECISIONS.md#owner-decisions-2026-11-01-during-the-work-on-g-26-to-g-31
[G-36]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-38]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-39]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-40]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-41]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-42]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-43]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-44]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-45]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-47]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-48]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-49]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-50]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-51]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-52]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-53]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-54]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-55]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-57]: DECISIONS.md#owner-decisions-2026-11-01-the-guide-and-the-names
[G-60]: DECISIONS.md#made-with-g-36-to-g-58-2026-11-01-the-owner-may-overrule-any
[G-61]: DECISIONS.md#made-with-g-36-to-g-58-2026-11-01-the-owner-may-overrule-any
[G-64]: DECISIONS.md#made-with-g-36-to-g-58-2026-11-01-the-owner-may-overrule-any
[G-65]: DECISIONS.md#made-with-g-36-to-g-58-2026-11-01-the-owner-may-overrule-any
[G-66]: DECISIONS.md#made-with-g-36-to-g-58-2026-11-01-the-owner-may-overrule-any
[G-69]: DECISIONS.md#made-with-g-36-to-g-58-2026-11-01-the-owner-may-overrule-any
[G-71]: DECISIONS.md#owner-decision-2026-11-01-if-requires-its-else-again
[G-72]: DECISIONS.md#made-with-g-71-2026-11-01-the-owner-may-overrule-any
[G-74]: DECISIONS.md#owner-decisions-2026-11-01-after-wave-2
[G-77]: DECISIONS.md#made-with-g-74-and-g-75-2026-11-01-the-owner-may-overrule-any
[G-78]: DECISIONS.md#made-with-g-74-and-g-75-2026-11-01-the-owner-may-overrule-any
[G-81]: DECISIONS.md#made-with-g-74-and-g-75-2026-11-01-the-owner-may-overrule-any
[G-84]: DECISIONS.md#owner-request-2026-11-02-a-corpus-then-less-surface
[G-86]: DECISIONS.md#made-with-g-84-2026-11-02-the-owner-may-overrule-any
[G-93]: DECISIONS.md#the-owner-on-g-85-2026-11-02
[G-95]: DECISIONS.md#owner-decisions-2026-11-02-after-wave-4
[G-96]: DECISIONS.md#owner-decisions-2026-11-02-after-wave-4
[G-97]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-98]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-99]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-101]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-102]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-103]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-104]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-105]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-106]: DECISIONS.md#made-with-g-94-to-g-96-2026-11-02-the-owner-may-overrule-any
[G-108]: DECISIONS.md#owner-decisions-2026-11-02-for-embedding-grift-in-fux
[G-110]: DECISIONS.md#owner-decisions-2026-11-02-for-embedding-grift-in-fux
[G-114]: DECISIONS.md#settled-by-the-design-ballot-2026-11-03-four-models-the-owner-may-overrule-any
[G-115]: DECISIONS.md#settled-by-the-design-ballot-2026-11-03-four-models-the-owner-may-overrule-any
[G-117]: DECISIONS.md#settled-by-the-design-ballot-2026-11-03-four-models-the-owner-may-overrule-any
[G-118]: DECISIONS.md#settled-by-the-design-ballot-2026-11-03-four-models-the-owner-may-overrule-any
[G-121]: DECISIONS.md#settled-by-the-design-ballot-2026-11-03-four-models-the-owner-may-overrule-any
[G-122]: DECISIONS.md#settled-by-the-design-ballot-2026-11-03-four-models-the-owner-may-overrule-any
[G-124]: DECISIONS.md#made-while-building-g-108-to-g-123-2026-11-03-the-owner-may-overrule-any
[G-132]: DECISIONS.md#made-after-the-red-team-and-the-picking-panel-2026-11-03-the-owner-may-overrule-any
[G-133]: DECISIONS.md#made-after-the-red-team-and-the-picking-panel-2026-11-03-the-owner-may-overrule-any
[G-142]: DECISIONS.md#settled-by-the-bytes-design-ballot-2026-11-04-four-models-the-owner-may-overrule-any
[G-143]: DECISIONS.md#settled-by-the-bytes-design-ballot-2026-11-04-four-models-the-owner-may-overrule-any
[G-144]: DECISIONS.md#settled-by-the-bytes-design-ballot-2026-11-04-four-models-the-owner-may-overrule-any
[G-145]: DECISIONS.md#settled-by-the-bytes-design-ballot-2026-11-04-four-models-the-owner-may-overrule-any
[G-147]: DECISIONS.md#settled-by-the-bytes-design-ballot-2026-11-04-four-models-the-owner-may-overrule-any
[G-151]: DECISIONS.md#earned-by-the-port-reading-2026-11-05
[G-154]: DECISIONS.md#conformance-fixes-2026-11-05
[G-155]: DECISIONS.md#conformance-fixes-2026-11-05
[G-157]: DECISIONS.md#picking-panel-follow-through-2026-11-05
[G-166]: DECISIONS.md#round-2b-2026-11-05
[G-168]: DECISIONS.md#round-2b-2026-11-05
[G-169]: DECISIONS.md#round-2b-2026-11-05
[G-170]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-171]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-172]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-173]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-174]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-175]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-176]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-177]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-178]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-179]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-180]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
[G-181]: DECISIONS.md#one-model-of-how-a-run-ends-2026-11-06
