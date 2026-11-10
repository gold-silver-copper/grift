#!/usr/bin/env python3
"""Every historic bug the tests caught stays caught.

tests/historic/ holds bugs from grift's history as patches that put them back, and INDEX.tsv names,
for each, the tests that fail with it in. For each gated patch this script copies the tree to a
scratch directory (the tree you run it in is never changed), applies the patch, builds the test
targets its tests live in and runs only those tests. The patch is caught when at least one of them is
reported failed; each named test that passed is listed, so a catch that thins out shows.

Not a catch: a patch that doesn't apply or doesn't build (the code it touches changed: re-port it),
a test that runs past the time limit, a named test that no longer exists (the index is stale). Before
any patch, the named tests run on the unpatched copy and must pass, or their failing proves nothing.

INDEX.tsv columns: name, gated (yes/no), tests, description. A test is `package/target::name`:
target `lib` (unit tests), `doc` (doctests; the name is matched as part of the doctest's name, which
carries a line number) or an integration test's name. Several are separated by `; `.

usage: scripts/historic-bugs.py [--only NAME]... [--shard I/N] [--timeout SECS]   (from the root)
The build goes to $CARGO_TARGET_DIR, or target/historic. Exits non-zero if a gated patch is not
caught, is broken, or a named test fails without any patch.
"""
import os, re, shutil, signal, subprocess, sys, tempfile, time

HISTORIC = "tests/historic"
BUILD_LIMIT = 1200  # seconds for one build: a cold one on a small runner takes minutes
RESULT = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored)", re.M)


def run(args, cwd, limit, env=None):
    """(exit status or None on timeout, output). The whole process group goes on a timeout."""
    p = subprocess.Popen(args, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
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
    return rows


def cargo(package, target, *rest):
    select = {"lib": ["--lib"], "doc": ["--doc"]}.get(target, ["--test", target])
    return ["cargo", "test", "--locked", "-p", package, *select, *rest]


def build(tree, specs, env):
    """None, or why the test targets of `specs` don't build."""
    # Doctests build as they run; their library is what builds first.
    for package, target in sorted({(p, "lib" if t == "doc" else t) for p, t, _ in specs}):
        status, out = run(cargo(package, target, "--no-run"), tree, BUILD_LIMIT, env)
        if status != 0:
            tail = "\n".join(out.strip().split("\n")[-15:])
            return f"{package}/{target} does not build" + ("" if status is not None else " (timed out)") + f":\n{tail}"
    return None


def run_tests(tree, specs, deadline, env):
    """{spec: "failed" | "passed" | "missing" | "timeout"} for each spec."""
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
        elif any(r == "FAILED" for _, r in results):
            found[spec] = "failed"
        elif any(r == "ok" for _, r in results):
            found[spec] = "passed"
        else:
            found[spec] = "missing"
    return found


def show(spec):
    package, target, name = spec
    return f"{package}/{target}::{name}"


def main():
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
    env["GIT_CEILING_DIRECTORIES"] = tempfile.gettempdir()
    # SIGTERM (a cancelled CI job) unwinds like Ctrl-C, so the scratch copy is removed either way.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit("stopped"))
    scratch = tempfile.mkdtemp(prefix="grift-historic-")
    bad = []
    try:
        tree = os.path.join(scratch, "tree")
        shutil.copytree(".", tree, ignore=shutil.ignore_patterns(".git", "target"), symlinks=True)
        specs = sorted({s for r in gated for s in r[2]})

        why = build(tree, specs, env)
        if why:
            sys.exit(f"the unpatched tree doesn't build its tests: {why}")
        start = time.monotonic()
        found = run_tests(tree, specs, start + limit * max(1, len(gated)), env)
        wrong = [f"{show(s)} {r}" for s, r in found.items() if r != "passed"]
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
                why = build(tree, specs, env)
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
            caught = [show(s) for s, r in found.items() if r == "failed"]
            rest = [f"{show(s)} {r}" for s, r in found.items() if r != "failed"]
            if any(r == "missing" for r in found.values()):
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
