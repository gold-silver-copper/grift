# Contributing

## Testing

- **A finding becomes a rule first.** When a review or a bug report finds something wrong, add it
  to `crates/grift-rules` before writing a scenario test: a case in an operation family of the
  rules harness (`rules.rs`), a law (`laws.rs`), or a shape the generator writes (`generate.rs`).
  A rule covers every way in and every program the generator writes; a scenario test covers one.
  Write a scenario test only when none of them can state the property, and say why in the PR.
- **`RULES.md` lists the rules.** A new obligation or law gets its line there, with its test.
- **Tests are deleted only with evidence.** The held-back bug set (historic bugs and mutants kept
  away from whoever writes the tests) is the measure: a test goes only when the suite without it
  still catches everything the set caught.
- **Properties, not snapshots.** No snapshot, golden-output or parity tests: they freeze today's
  behaviour, broken parts included. Compare outcomes by kind (a value, an error's kind, a stop, out
  of memory), never by a report's text.
- **Fixed seeds, small counts.** Generated tests use a fixed seed and a case count CI can afford;
  a failure prints its seed and the smallest program that still fails.
