# grift: an immutable bytes value

You own one PR in gold-silver-copper/grift (default branch `pure`) end to end. Run this **after** the
embedding PR (`prompts/embed.md`) has merged; base on `origin/pure` at the head in your launch message.
The owner decided on 2026-11-02 (`~/code/notes/projects/grift/mux-2026-11-02/owner-decisions.md`,
decision 10) that grift gets **an immutable bytes value beside UTF-8 strings**, replacing G-100's lists
of integers 0-255. **Do not reopen that.** The design inside it is yours: decide, write down why
(ledger and PR), continue. Never ask the user anything. No prototypes or spikes.

Use the agentic coding skill (`~/.claude/skills/agentic-coding/SKILL.md`; `S=~/.claude/skills/agentic-coding/scripts`).

## Evidence
- K4 stopped parts of 6 programs over three waves (`~/code/notes/projects/grift/apps4-2026-11-02/tally.md`
  section 2; wave 2 build-h F5; wave 3 machine and diff F9). PR #195 (G-100) made bytes lists of integers;
  its "choices left to the owner" measured the cost: `tail -c 100` of 589 KB 2 ms -> 1.8 s, `length`,
  `drop`, `append` 0.35-1.5 µs an element.
- The logic spike: 107 µs converting a 9 KB chunk, as much as grift's own work on it
  (`~/code/notes/projects/grift/mux-2026-11-02/logic/friction.md` F2).
- Read `docs/VISION.md` ("Values", "Close to Rust", "Names", "How grift grows"), `docs/LANGUAGE.md`,
  `DECISIONS.md` (G-100, and G-104 if it names bytes), `crates/grift/src/`, `crates/grift-cli/`.

## Done means all of these hold
1. **A bytes value**: immutable, any bytes, its own type (`type-of`, a predicate, a printed form that
   `read-form` reads back as the same bytes: choose the literal syntax by the naming and reader rules,
   say why). Strings stay UTF-8 text and are never bytes; converting bytes to a string refuses anything
   that isn't UTF-8, with the place (Rust's `str::from_utf8`).
2. **Operations** by the evidence, named by the rules, mirroring the string ones where they exist:
   length, the byte at an index, a slice, append, search (a native twin under the core/native rule if
   the embedding PR's measurement bar says so), string to bytes and back, a list of integers to bytes and
   back. Nothing a program in the evidence didn't need.
3. **The runner**: `read-bytes` and `write-bytes` (G-100) take and give bytes values; `read-file`'s bytes
   counterpart if one is needed. Every program in `programs/` and the editor pass; the ones using
   G-100's lists move to the new value.
4. **The embedding API**: make bytes from a `&[u8]`, read a bytes value as `&[u8]`, under the same
   stale-value and other-interpreter checks as strings.
5. **Evidence**: in scratch worktrees (not in the PR), port wave 4's unix `head`/`tail` (branch
   `app4-unix`), text's base64 (`app4-text`) and interpreters' Brainfuck (`app4-interpreters`), and report
   lines and timings before/after (one run each, both built here), each test.sh passing.
6. **Docs**: VISION "Values" lists the new kind; LANGUAGE.md every name; DECISIONS.md an entry replacing
   G-100 with the design and its reasons.

## Acceptance bands (set before the work; accept without asking)
- `tail -c 100` of a 589 KB file under 20 ms; the base64 decode of 565 KB at least 5x faster than with
  lists. `fib 25` and every `programs/` test.sh within 5% of the base.
- `crates/grift` non-test Rust grows by at most 300 lines; the library by at most 60.
- A band that can't be met without hurting correctness is revised as CLAUDE.md says.

## Rules, gates, review, ledger, end
As in `prompts/embed.md` (the same sections apply word for word), with these names: worktree
`~/.cache/claude-tmp/grift-bytes/wt`, branch `bytes`, target and tmp under `~/.cache/claude-tmp/grift-bytes/`,
ledger and status in `~/code/notes/projects/grift/bytes-2026-11-02/`. Reviewers: Sol and Opus, never
Astra or Fable. No Co-Authored-By or any AI attribution. Don't merge. When the PR is open, the gate
passes at its head and the review is done, write `DONE` beside the status with the PR URL, head SHA and
the gate's result.
