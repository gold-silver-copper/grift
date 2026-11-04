---
id: 01a1a55c-47b3-7572-acf5-bcc77e72312c
slug: decisions/grift-in-fux
title: "Owner decisions, 2026-11-02: grift in fux, built like the config spike"
type: adr
status: active
priority: high
tags: [owner, fux]
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# Owner decisions, 2026-11-02: grift in fux, built like the config spike

Copied word for word from box2 `~/code/notes/projects/grift/mux-2026-11-02/owner-decisions.md`. Every grift and fux task cites this as the owner's yes. Items 3, 4, 5, 11 and 12 are built (grift #198, [tasks/grift-2](../tasks/grift-2.md)); 10 is [tasks/grift-3](../tasks/grift-3.md); 1, 2, 6, 7, 8 and 9 are for the fux replacement ([tasks/grift-4](../tasks/grift-4.md), [tasks/grift-5](../tasks/grift-5.md)).


From `synthesis/report.md` section 5 and the conversation that followed. The owner wants something
like the **config** spike: fux's architecture, grift where fuxth is.

## For fux (the replacement PR, after the grift PRs)
1. **1a.** A fux command line is one quoted grift program, shell arguments after it as strings:
   `fux '(send-keys (pane 3) (text "ls -l") (key "Enter"))'`. Replaces fux D-88 for fux.
2. **2a.** Later: after the grift embedding PR (`~/code/grift/prompts/embed.md`) lands.
6. **No fuxthisms.** fux's words, error kinds and records follow grift's conventions (`no-pane`, not
   `no_pane`; VISION "Names").
7. **grift data, not JSON.** Records print as grift data that `read-form` reads back (e.g.
   `((id 3) (columns 80) (title "sh"))`); a script asks for the field it wants (`fux '(columns (pane 3))'`).
   No JSON output; a library `to-json` only if a real program asks (growth rule).
8. **8a.** No converter for sessions saved by today's fux; no backwards compatibility.
9. **`fux/archived/`** holds fuxth, fuxth-run and fuxed, out of the workspace (not built, not tested).

## For grift
3. **keep/kept/release**: a kept value is a binding in a context the interpreter owns; `release`
   unbinds it; `Kept` can't be copied. Amends G-21's wording ("no handles"), not its rule (a value
   lives while a binding holds it).
4. **Fix the interrupt, no clock**: G-5 stands word for word. The interpreter owns its flag and hands
   out a sendable `Interrupter` that stops only the run in progress when it is called, never a later
   one. G-17's `&'static` goes (its chip target went with G-25). Limits stay host policy.
5. **`string-search` native** (under 12).
10. **Bytes: design an immutable bytes value** beside UTF-8 strings, replacing G-100's lists of
    integers, in its own PR (`~/code/grift/prompts/bytes.md`). **No pausing evaluations** (G-53 stands)
    until a second program needs it.
11. **No call or context representation changes** now (G-33, G-34 stand).
12. **Core and native primitives, separated; native twins allowed.** `core` holds what grift can't
    express; `native` holds what exists only for speed, each with its grift definition kept as the
    meaning and a test proving the two indistinguishable (results, errors, traces, tail calls,
    `type-of`, `operative?`, printing). `if!` and `sequence!` get native twins: reverses G-56 with this
    rule; VISION "Implementation" changes from "no fast paths" to "a library form may have a native
    twin, proven equal and justified by a measurement on `programs/`".
- Also: the two holes are bugs (a reclaimed value read as "not a pair"; a value or symbol from
  another interpreter not detected). The small conversions, `\r` and reading a record's fields are
  wanted ("grift should be a proper language").