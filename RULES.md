# The rules grift keeps

Each line is a promise and the test that checks it, in `crates/grift-rules` (`witnesses::*` and
`laws::*`). Every test states a property; none compares output with a saved copy. Run them with
`cargo build -p grift-cli && cargo test -p grift-rules`. Memory limits and refused allocations
come from grift's `hooks` feature, which only these tests turn on.

## The language
- **L1** The reader and printer handle every datum; a reader error is an ordinary error. `l1`
- **L2** Operatives, `wrap`, `unwrap`; parameter trees are checked at call time. `l2`
- **L3** Contexts are values, and no name is special: every library word can be shadowed. `l3`
- **L4** Tail calls are proper in operatives, applicatives, `if!`, `sequence!`, `evaluate` and a
  host function's `Answer::Call`: a million of each runs in 8 MB. `l4`
- **L5** An error is a value with a kind, values and a trace of places; handlers and cleanups
  see it. `l5`
- **L6** The natives and library run fib and life. `l6`

## Runs and the host
- **H1** Interpreters share nothing, and each keeps its own memory limit. `h1`
- **H2** A host evaluates text and calls a value with arguments. `h2`
- **H3** A host function answers a value, an error or a call back, recursion through it costs no
  Rust stack, and it can't start a run of its own. `h3`
- **H4** A value from another interpreter, or one kept past a collection without `keep`, is
  refused, never read as another value. `h4`
- **H5** A kept value lives across runs; a released one is collected. `h5`
- **H6** A stop names one run, and a force ends a run whose cleanup never ends, within 50 ms. `h6`
- **H7** A parked run resumes with a value or an error, survives other runs, can be abandoned
  with its cleanups run. `h7`
- **H8** A report keeps its byte budget and says exactly when it was cut. `h8`

## Resources (each checked in every operation family: machine steps, read, print, search,
parameter matching, reports, collection, host calls, park and resume)
- **R1** A stop asked before or during the work ends it within 50 ms. `r1`
- **R2** Nothing recurses on the Rust stack: deep data and deep recursion run on a 256 KiB
  thread. `r2`
- **R3** Memory refused at the Nth allocation, for every N in turn, ends the work with its own
  result or `out-of-memory`, and the interpreter works after. `r3`
- **R4** Printing and reports keep their budget, even for a context that holds itself. `r4`
- **F1** Collection takes time in proportion to what it walks, and memory is given back. `f1`
- **F2** Catching and raising again at every level is linear in depth. `f2`
- **F3** Symbol names are reclaimed: new symbols forever fit in a fixed memory limit. `f3`
- **F4** The runner refuses, with an exit status, a file larger than its memory. `f4`

## The laws (on generated programs, with a fixed seed; `GRIFT_LAW_SEED` and `GRIFT_LAW_CASES`
choose others)
1. Two fresh interpreters agree on every program. `handler`
2. A handler that raises the error again changes nothing, and keeps the trace's places. `handler`
3. A handler catches every error a program raises, whatever its kind. `catch`
4. A cleanup runs once, after everything else, and changes nothing else. `cleanup`
5. `if!`, `sequence!`, `let!` and `lambda!` shadowed by plain operatives change nothing. `shadowed`
6. Collecting at every safe point, parked at each wait, changes nothing. `stress`
7. Memory refused at any allocation ends a run as `out-of-memory` or as it would have ended. `memory`
8. Printed data reads back as itself. `read_back`
9. Runs parked together mean what each means alone, up to a stop, a force or refused memory
   inside one. `together`

After anything goes wrong, the interpreter means what a fresh one means: `memory` and
`together` check it after every case.
