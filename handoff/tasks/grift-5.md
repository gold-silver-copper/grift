---
id: 01a1a55c-4831-7fc1-8678-cfb400841c54
slug: tasks/grift-5
title: "The fux PR: grift replaces fuxth"
type: task
status: draft
priority: high
tags: [fux]
in:
  tasks/grift-1: "a0VVV"
repo: gold-silver-copper/grift
---

<!-- Projected from the GitKB knowledge base at ~/code/grift-kb (the source); refresh with: cd ~/code/grift-kb && git-kb project tasks/ --to proj/ --mode copy, likewise decisions/ -->

# The fux PR: grift replaces fuxth

**Links:** Blocked by [tasks/grift-4](grift-4.md); Implements [decisions/grift-in-fux](../decisions/grift-in-fux.md).

Part of [tasks/grift-1](grift-1.md). Owner decisions: [decisions/grift-in-fux](../decisions/grift-in-fux.md) items 1, 2, 6, 7, 8, 9. Needs [tasks/grift-4](grift-4.md).

The fux PR that replaces fuxth with grift in fux's own architecture: Rust keeps the event loop, panes, layout and drawing; grift becomes the config, key bindings, command prompt, saved sessions and every `fux` command line. fuxth, fuxth-run and fuxed move to `fux/archived/`.

This document's body becomes the brief: [tasks/grift-4](grift-4.md)'s Stage 6 writes it here, targeting the earned grift PR's API by name, starting from the port its panel picks.