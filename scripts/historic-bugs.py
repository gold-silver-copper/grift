#!/usr/bin/env python3
"""Every historic bug the tests caught stays caught.

tests/historic/ holds bugs from grift's history as patches that put them back, and INDEX.tsv names,
for each, the tests that fail with it in. For each gated patch this script copies the tree to a
scratch directory (the tree you run it in is never changed), applies the patch, builds the test
targets its tests live in and runs only those tests. The patch is caught when at least one of them is
reported failed; each named test that passed is listed, so a catch that thins out shows.

A test binary that dies before it reports (an abort, a stack overflow) is a failed test, so a catch.
Not a catch: a patch that doesn't apply or doesn't build (the code it touches changed: re-port it),
a test that runs past the time limit, a named test that no longer exists (the index is stale). A patch
that changes grift source (`*.grift`, read only when a test runs) must leave it readable: the patched
`grift` runner must start (it reads the library and its own runner.grift) and read every patched
`.grift` file, or a typo would count as a catch; if it can't, the patch is broken too. Before any
patch, the named tests run on the unpatched copy and must pass, or their failing proves nothing.
A doctest is built as it runs, so a patch that breaks only a named doctest's compiling counts as a
catch, not as broken. Its library is not: the build step builds what the doctest run builds first
(the library without cfg(test)), and a test run that says a crate could not compile is broken too.

INDEX.tsv columns: name, gated (yes/no), tests, description. A test is `package/target::name`:
target `lib` (unit tests), `doc` (doctests: the start of the doctest's name, its file from the
root and the item, as in `crates/grift/src/lib.rs - HostFunction`; the full name carries a line
number) or an integration test's name. Several are separated by `; `. A doctest's file must be in the
workflow's pull_request paths, so a PR that changes it runs the gate; the script refuses otherwise.

Linux only, like its workflow: some catching tests are Linux-only and a hang is ended by killing its
process group. It refuses on any other system.

usage: scripts/historic-bugs.py [--only NAME]... [--shard I/N] [--timeout SECS]   (from the root)
The build goes to $CARGO_TARGET_DIR, or target/historic. Exits non-zero if a gated patch is not
caught, is broken, or a named test fails without any patch.
"""
import fnmatch, os, re, shutil, signal, subprocess, sys, tempfile, time

HISTORIC = "tests/historic"
WORKFLOW = ".github/workflows/historic-bugs.yml"
BUILD_LIMIT = 600  # seconds for one build: a cold one on a small runner takes minutes
RESULT = re.compile(r"^test (.+?)(?: - should panic)? \.\.\. (ok|FAILED|ignored)", re.M)


def run(args, cwd, limit, env=None):
    """(exit status or None on timeout, output). The whole process group goes on a timeout."""
    p = subprocess.Popen(args, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT,
                         text=True, errors="replace", start_new_session=True)
    try:
        out, _ = p.communicate(timeout=limit)
        return p.returncode, out
    except subprocess.TimeoutExpired:
        kill(p)
        out, _ = p.communicate()
        return None, out
    except BaseException:  # Ctrl-C or SIGTERM: the group is in a session of its own, so end it here
        kill(p)
        raise


def kill(p):
    try:
        os.killpg(p.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def read_index():
    rows = []
    for n, line in enumerate(open(os.path.join(HISTORIC, "INDEX.tsv"), encoding="utf-8")):
        cols = line.rstrip("\n").split("\t")
        if n == 0 or not line.strip():
            continue
        if len(cols) != 4 or cols[1] not in ("yes", "no"):
            sys.exit(f"{HISTORIC}/INDEX.tsv line {n + 1}: want name, gated (yes/no), tests, description")
        name, gated, tests, description = cols
        specs = []
        for t in filter(None, (t.strip() for t in tests.split(";"))):
            m = re.fullmatch(r"([\w-]+)/([\w-]+)::(.+)", t)
            if not m:
                sys.exit(f"{HISTORIC}/INDEX.tsv: {name}: a test is package/target::name, not {t!r}")
            specs.append(m.groups())
        if gated == "yes" and not specs:
            sys.exit(f"{HISTORIC}/INDEX.tsv: {name} is gated and names no test")
        rows.append((name, gated == "yes", specs, description))
    gated = {r[0] for r in rows if r[1]}
    files = {f[:-len(".patch")] for f in os.listdir(HISTORIC) if f.endswith(".patch")}
    if gated != files:
        sys.exit(f"{HISTORIC}: gated rows without a patch: {' '.join(sorted(gated - files)) or 'none'}; "
                 f"patches without a gated row: {' '.join(sorted(files - gated)) or 'none'}")
    paths = open(WORKFLOW, encoding="utf-8").read().split("paths:", 1)[-1]
    watched = re.findall(r'^\s*- "([^"]+)"', re.match(r'(?:\s*- "[^\n]*\n)*', paths).group(), re.M)
    for name, _, specs, _ in rows:
        for _, target, test in specs:
            path = os.path.normpath(test.split(" - ")[0])
            if target == "doc" and not any(fnmatch.fnmatch(path, w) for w in watched):
                sys.exit(f"{name}: the doctest's file {path} is not in {WORKFLOW}'s paths, "
                         "so a PR changing it wouldn't run the gate; add it there")
    return rows


def cargo(package, target, *rest):
    select = {"lib": ["--lib"], "doc": ["--doc"]}.get(target, ["--test", target])
    return ["cargo", "test", "--locked", "-p", package, *select, *rest]


def build(tree, specs, env):
    """None, or why the test targets of `specs` don't build."""
    # `--no-run` doesn't take doctests, and `--doc` builds the library without cfg(test): listing the
    # doctests builds exactly that and compiles none of them (they build as they run).
    for package, target in sorted({(p, t) for p, t, _ in specs}):
        rest = ["--", "--list"] if target == "doc" else ["--no-run"]
        status, out = run(cargo(package, target, *rest), tree, BUILD_LIMIT, env)
        if status != 0:
            tail = "\n".join(out.strip().split("\n")[-15:])
            return f"{package}/{target} does not build" + ("" if status is not None else " (timed out)") + f":\n{tail}"
    return None


def run_tests(tree, specs, deadline, env):
    """{spec: "failed" | "passed" | "ignored" | "missing" | "timeout" | why it didn't build}."""
    found = {}
    for spec in specs:
        package, target, name = spec
        left = deadline - time.monotonic()
        if left <= 0:
            found[spec] = "timeout"
            continue
        # A doctest's name carries its line number, so it's matched as a part; the rest exactly.
        args = [] if target == "doc" else ["--exact"]
        status, out = run(cargo(package, target, "--", name, *args), tree, left, env)
        results = [(t, r) for t, r in RESULT.findall(out) if (name in t if target == "doc" else t == name)]
        if status is None:
            found[spec] = "timeout"
        elif "could not compile" in out or not results and "error[E" in out:
            # The build step should have built it all; if a crate still didn't compile, it's broken.
            tail = "\n".join(out.strip().split("\n")[-15:])
            found[spec] = f"does not build:\n{tail}"
        elif any(r == "FAILED" for _, r in results):
            found[spec] = "failed"
        elif any(r == "ok" for _, r in results):
            found[spec] = "passed"
        elif results:
            found[spec] = "ignored"
        else:
            # The build succeeded, so a filter that matched nothing exits 0; a binary that died
            # before reporting (abort, stack overflow, killed for memory) doesn't.
            found[spec] = "missing" if status == 0 else "failed"
    return found


def still_reads(tree, patch, env):
    """None, or why the patched grift can't read the grift source the patch changes."""
    changed = re.findall(r"^\+\+\+ b/(.+\.grift)$", open(patch, encoding="utf-8").read(), re.M)
    if not changed:
        return None
    status, out = run(["cargo", "build", "--locked", "-p", "grift-cli", "--bin", "grift"], tree, BUILD_LIMIT, env)
    if status != 0:
        return "the grift runner does not build:\n" + "\n".join(out.strip().split("\n")[-15:])
    control = os.path.join(env["TMPDIR"], "still-reads.grift")
    with open(control, "w", encoding="utf-8") as f:
        f.writelines(f'(read-forms "{path}" (read-file "{os.path.join(tree, path)}"))\n' for path in changed)
    status, out = run([os.path.join(env["CARGO_TARGET_DIR"], "debug", "grift"), control], tree, 60, env)
    if status != 0:
        return f"the patched grift can't read {' '.join(changed)}:\n{out.strip()}"
    return None


def show(spec):
    package, target, name = spec
    return f"{package}/{target}::{name}"


def main():
    if not sys.platform.startswith("linux"):
        sys.exit(f"{sys.argv[0]} runs on Linux only (some catching tests are Linux-only), not {sys.platform}")
    sys.stdout.reconfigure(line_buffering=True)  # one line a patch, as it ends, in a CI log too
    args, only, shard, limit = sys.argv[1:], [], (1, 1), 300.0
    while args:
        flag = args.pop(0)
        if flag in ("--only", "--shard", "--timeout") and not args:
            sys.exit(f"{flag} needs a value")
        if flag == "--only":
            only.append(args.pop(0))
        elif flag == "--shard":
            m = re.fullmatch(r"(\d+)/(\d+)", args.pop(0))
            if not m or not 1 <= int(m[1]) <= int(m[2]):
                sys.exit("--shard is I/N with 1 <= I <= N")
            shard = int(m[1]), int(m[2])
        elif flag == "--timeout":
            limit = float(args.pop(0))
        else:
            sys.exit(__doc__)

    rows = read_index()
    unknown = set(only) - {r[0] for r in rows}
    if unknown:
        sys.exit(f"not in {HISTORIC}/INDEX.tsv: {', '.join(sorted(unknown))}")
    gated = [r for r in rows if r[1] and (not only or r[0] in only)]
    gated = [r for i, r in enumerate(gated) if i % shard[1] == shard[0] - 1]
    gaps = [r[0] for r in rows if not r[1]]

    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = os.path.abspath(env.get("CARGO_TARGET_DIR") or "target/historic")
    # The scratch copy has no .git; without a ceiling, git apply would find a repository above it
    # (the temp dir can sit inside one) and apply relative to that repository's root.
    env["GIT_CEILING_DIRECTORIES"] = tempfile.gettempdir()
    # SIGTERM (a cancelled CI job) unwinds like Ctrl-C, so the scratch copy is removed either way.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit("stopped"))
    scratch = tempfile.mkdtemp(prefix="grift-historic-")
    env["TMPDIR"] = scratch  # what a killed test or rustdoc leaves goes with the scratch copy
    bad = []
    try:
        tree = os.path.join(scratch, "tree")
        # Not .git, target or the scratch copy itself (TMPDIR can be inside the tree).
        skip = lambda d, names: [n for n in names if n in (".git", "target")
                                 or os.path.realpath(os.path.join(d, n)) == os.path.realpath(scratch)]
        shutil.copytree(".", tree, ignore=skip, symlinks=True)
        specs = sorted({s for r in gated for s in r[2]})

        why = build(tree, specs, env)
        if why:
            sys.exit(f"the unpatched tree doesn't build its tests: {why}")
        start = time.monotonic()
        found = run_tests(tree, specs, start + limit * max(1, len(gated)), env)
        wrong = [f"{show(s)} {'does not exist (the index is stale)' if r == 'missing' else r}"
                 for s, r in found.items() if r != "passed"]
        print(f"unpatched: {len(specs)} tests in {time.monotonic() - start:.0f} s"
              + (f"; not passing: {'; '.join(wrong)}" if wrong else ", all pass"))
        if wrong:
            sys.exit("a named test doesn't pass without any patch, so its failing proves nothing")

        for name, _, specs, _ in gated:
            patch = os.path.abspath(os.path.join(HISTORIC, name + ".patch"))
            start = time.monotonic()
            status, out = run(["git", "apply", patch], tree, 60, env)
            if status != 0:
                print(f"BROKEN      {name}: does not apply, re-port this patch: {name}\n{out.strip()}")
                bad.append(name)
                continue
            try:
                why = build(tree, specs, env) or still_reads(tree, patch, env)
                if why:
                    print(f"BROKEN      {name}: {why}\nre-port this patch: {name}")
                    bad.append(name)
                    continue
                found = run_tests(tree, specs, time.monotonic() + limit, env)
            finally:
                status, out = run(["git", "apply", "-R", patch], tree, 60, env)
                if status != 0:
                    sys.exit(f"could not take {name} back out of the scratch copy:\n{out}")
            took = f"{time.monotonic() - start:.0f} s"
            broken = [f"{show(s)} {r}" for s, r in found.items() if r.startswith("does not build")]
            if broken:
                print(f"BROKEN      {name}: {'; '.join(broken)}\nre-port this patch: {name}")
                bad.append(name)
                continue
            caught = [show(s) for s, r in found.items() if r == "failed"]
            rest = [f"{show(s)} {r}" for s, r in found.items() if r != "failed"]
            if any(r in ("missing", "ignored") for r in found.values()):
                print(f"STALE INDEX {name}: a named test doesn't exist: {'; '.join(rest)}")
                bad.append(name)
            elif caught:
                print(f"caught      {name} ({took}) by {'; '.join(caught)}"
                      + (f"; not by {'; '.join(rest)}" if rest else ""))
            else:
                print(f"NOT CAUGHT  {name} ({took}): {'; '.join(rest)}")
                bad.append(name)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)

    print(f"{len(gated) - len(bad)}/{len(gated)} gated patches caught"
          + (f"; failing: {' '.join(bad)}" if bad else "")
          + (f". Known gaps, not gated: {' '.join(gaps)}" if gaps and not only else ""))
    if bad:
        sys.exit(1)


if __name__ == "__main__":
    main()
