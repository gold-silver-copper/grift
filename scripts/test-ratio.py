#!/usr/bin/env python3
"""Test size per production size in crates/, held to a ratchet.

What counts as test: every file under crates/*/tests/ (fixtures and .grift programs included),
crates/*/src/*tests.rs, and any item in crates/*/src/ under an attribute that names `test` without
`not(test)`: #[cfg(test)], #[cfg(any(test, feature = ...))], #[cfg_attr(test, ...)] counts only its
own line. Production: the rest of crates/*/src/ (.rs and .grift). programs/ and examples/ are
neither: they are programs the corpus test runs, not the interpreter.

What is counted: non-blank characters outside comments, so packing or spreading code changes nothing.
A `;` (grift) or `//` (Rust) comment is left out; doc comments (`///`, `//!`) too. Strings count.

usage: scripts/test-ratio.py [--lines]   (from the repository root)
Fails if the ratio is above the number in scripts/test-ratio.baseline, or more than 0.01 under it
(then it says the number to write there: the ratchet only turns down).
"""
import glob, math, os, re, sys

TEST_ATTR = re.compile(r"^\s*#\[cfg(_attr)?\((any\()?[^]]*\btest\b")
COMMENT = {".rs": re.compile(r'\br(#*)"[\s\S]*?"\1|\'(?:\\.|[^\'\\])\'|"(?:\\.|[^"\\])*"|//[^\n]*'), ".grift": re.compile(r'"(?:\\.|[^"\\])*"|;[^\n]*')}


def size(lines, ext, by_lines):
    text = "\n".join(lines)
    text = COMMENT[ext].sub(lambda m: "" if m.group(0)[0] in "/;" else m.group(0), text)
    if by_lines:
        return sum(1 for l in text.split("\n") if l.strip())
    return sum(1 for c in text if not c.isspace())


def split(path):
    """(production lines, test lines) of one source file in src/."""
    lines = open(path, encoding="utf-8").read().split("\n")
    prod, test, i = [], [], 0
    while i < len(lines):
        line = lines[i]
        is_test = TEST_ATTR.match(line) and "not(test)" not in line
        if not is_test or "cfg_attr" in line:
            (test if is_test else prod).append(line)
            i += 1
            continue
        # The item after the attribute: up to the end of its braces, or its `;`/`,`.
        depth, j = 0, i + 1
        while j < len(lines):
            depth += lines[j].count("{") - lines[j].count("}")
            if depth <= 0 and re.search(r"[;,}]\s*$", lines[j]):
                break
            j += 1
        test.extend(lines[i : j + 1])
        i = j + 1
    return prod, test


def measure(by_lines=False):
    prod = test = 0
    for path in sorted(glob.glob("crates/*/**/*", recursive=True)):
        ext = os.path.splitext(path)[1]
        if not os.path.isfile(path) or ext not in COMMENT:
            continue
        lines = open(path, encoding="utf-8").read().split("\n")
        if "/tests/" in path or path.endswith("tests.rs"):
            test += size(lines, ext, by_lines)
        elif "/src/" in path:
            p, t = split(path) if ext == ".rs" else (lines, [])
            prod += size(p, ext, by_lines)
            test += size(t, ext, by_lines)
    return test, prod


def main():
    test, prod = measure("--lines" in sys.argv)
    ratio = test / prod
    baseline = float(open("scripts/test-ratio.baseline").read().split()[0])
    print(f"test {test}, production {prod}: {ratio:.3f} test per production (baseline {baseline:.2f})")
    if ratio > baseline:
        sys.exit(f"tests grew faster than production. A review finding goes into the generator, a native's "
                 f"twin or a guard (CONTRIBUTING.md); a new scenario test pays for itself by removing another.")
    if ratio < baseline - 0.01:
        sys.exit(f"the ratio fell: write {math.ceil(ratio * 100) / 100:.2f} in scripts/test-ratio.baseline")


if __name__ == "__main__":
    main()
