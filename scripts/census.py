#!/usr/bin/env python3
"""Counts every built-in, library and runner name's uses in programs/, the editor, the library and
the runner. A symbol counts where it is evaluated and not shadowed by the file's own binding
(define!, lambda!/vau! parameters, let!, catch!, catch-kind!); a symbol in (quote! ...) and catch-kind!'s
kind are data.

usage: scripts/census.py RUNNER  (from the repository root; prints a Markdown table)
"""
import glob, re, subprocess, sys

FIELDS = "read-line read-all read-bytes write close path status output \
kind values trace".split()
TOKEN = re.compile(r'\s+|(?:;|#!)[^\n]*|"(?:\\.|[^"\\])*"?|[()]|[^\s();"]+', re.S)


def parse(text):
    stack = [[]]
    for t in TOKEN.findall(text):
        if t == "(":
            stack.append([])
        elif t == ")":
            if len(stack) > 1:
                stack[-2].append(stack.pop())
        elif t[0] == '"':
            stack[-1].append(None)
        elif not t.isspace() and not t.startswith((";", "#!")):
            stack[-1].append(t)
    while len(stack) > 1:
        stack[-2].append(stack.pop())
    return stack[0]


def symbols(tree):
    if isinstance(tree, list):
        for x in tree:
            yield from symbols(x)
    elif tree and tree != "." and tree[0] != "#":
        yield tree


def defines(forms):
    return {s for f in forms if isinstance(f, list) and f[:1] == ["define!"] for s in symbols(f[1:2])}


def walk(x, scope, uses):
    if not isinstance(x, list):
        if x and x not in scope:
            uses[x] = uses.get(x, 0) + 1
    elif x and isinstance(x[0], str) and x[0] not in scope and x[0] in SPECIAL:
        uses[x[0]] = uses.get(x[0], 0) + 1
        SPECIAL[x[0]](x[1:], scope, uses)
    else:
        for e in x:
            walk(e, scope, uses)


def each(forms, scope, uses):
    for e in forms:
        walk(e, scope, uses)


def function(params, x, scope, uses):
    each(x, scope | set(symbols(params)) | defines(x), uses)


def let(x, scope, uses):
    inner = scope | set(symbols([b[:1] for b in x[0] if isinstance(b, list)])) | defines(x[1:])
    each([e for b in x[0] if isinstance(b, list) for e in b[1:]] + x[1:], inner, uses)


def field(x, scope, uses):  # the first operand is evaluated in another context
    walk(x[0], set(), uses)
    each(x[1:], scope, uses)


SPECIAL = {
    "quote!": lambda x, scope, uses: None,
    "define!": lambda x, scope, uses: each(x[1:], scope, uses),
    "lambda!": lambda x, scope, uses: function(x[:1], x[1:], scope, uses),
    "vau!": lambda x, scope, uses: function(x[:2], x[2:], scope, uses),
    "let!": let,
    "catch!": lambda x, scope, uses: (walk(x[0], scope, uses), function(x[1:2], x[2:], scope, uses)),
    "catch-kind!": lambda x, scope, uses: (walk(x[0], scope, uses), function(x[2:3], x[3:], scope, uses)),
    "evaluate-in!": field,
}


def count(paths):
    uses = {}
    for path in paths:
        forms = parse(open(path, encoding="utf-8", errors="replace").read())
        each(forms, defines(forms) if "library" not in path else set(), uses)
    return uses


def names(of):
    """The names in the list `of` evaluates to in the runner: a run that fails is an error, never no names."""
    listing = f"(for-each (lambda! (n) (display n \"\\n\")) {of})"
    return subprocess.run(sys.argv[1], input=listing, capture_output=True, text=True, check=True).stdout.split()


def main():
    built_in, runner = names("built-in-names"), names("(names (current-context))")
    programs = {p.split("/")[1]: count(glob.glob(p + "/**/*.grift", recursive=True))
                for p in sorted(glob.glob("programs/*"))}
    others = {"editor": count(["examples/emacs/src/editor.grift"]),
              "library": count(["crates/grift/src/library.grift"]),
              "runner": count(["crates/grift-cli/src/runner.grift"])}
    print("| name | programs using it | uses in programs | editor | library | runner |")
    print("|---|---|---|---|---|---|")
    for name in built_in + runner + FIELDS:
        using = [p for p in programs if programs[p].get(name)]
        total = sum(programs[p].get(name, 0) for p in programs)
        row = [str(others[o].get(name, 0)) for o in others]
        print(f"| `{name}` | {len(using)} | {total} | " + " | ".join(row) + " |")


main()
