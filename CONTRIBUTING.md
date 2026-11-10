# Contributing

## Tests grow no faster than the code

CI runs `scripts/test-ratio.py`: test characters per production character in `crates/` may not rise
above the number in `scripts/test-ratio.baseline`. When a change lowers it, CI asks you to write
the new, lower number there, so the bound only tightens. The script's header says what counts as a
test.

A review finding or a bug report goes, in this order of preference, into:

1. **the generator**: a shape the program generator writes, so every generated program checks it
   (`parked_tests.rs`, `mod generated`);
2. **a native's twin**: the grift definition in `natives.grift` that the tests run in place of the
   Rust, so no program can tell the two apart;
3. **a guard**: a type, a lint or an assertion in the code that makes the mistake impossible or
   fails at the spot.

A new scenario test needs a sentence in the PR on why none of those can say it. Good reasons:
the runner's input or output, the reader's exact spelling of a value, an invalid value from a
host. "It is quicker to write" is not one.

The ratio can be gamed, and review is what stops it: don't pack tests onto fewer lines, move test
helpers into the library, or delete assertions to make room. A test is deleted only when the
held-back bug set shows the suite still catches everything it caught.

## Caught bugs stay caught

`tests/historic/` holds bugs from grift's history as patches that put them back, and `INDEX.tsv`
names the tests that fail with each. `scripts/historic-bugs.py` applies each one to a scratch copy
and checks its tests still fail; CI runs it nightly and on a PR that changes tests. A PR that changes
code a patch touches re-ports the patch: run `scripts/historic-bugs.py --only <name>`; a patch that
no longer applies or builds says "re-port this patch", and you write the same bug into the new code
and save it with `git diff > tests/historic/<name>.patch`. A PR that changes the tests that catch a
bug updates its row; a doctest that catches one has its file in the workflow's `paths`. The script
runs on Linux only, like its workflow. The bugs no test catches are in the index as not gated: catching one is a good
first test to write.
