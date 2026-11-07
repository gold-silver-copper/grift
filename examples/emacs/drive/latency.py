"""Key-to-screen latency, in ms, 200 keys each, while a real command runs, against idle.
usage: latency.py BINARY WORKDIR [SCENARIO...]
A scenario is idle, or a load: quiet, stream, flood."""
import os, sys, time
sys.path.insert(0, os.path.dirname(__file__))
from ed import Editor, latency, print_json, summary

LOADS = {
    "quiet": "sleep 600",
    # About 100 lines a second, the rate of a compiler or a test run.
    "stream": "i=0; while :; do i=$((i+1)); echo line $i; sleep 0.01; done",
    # As fast as the editor will take it, for the whole measurement.
    "flood": "yes 'the same line of output, over and over'",
}


def lines(ed, work, name):
    """How many lines of output had reached the buffer."""
    path = os.path.join(work, "lines-" + name)
    if os.path.exists(path):
        os.remove(path)
    ed.evaluate(f'(write-file "{path}" (integer-to-string (add (length (evaluate-in! above (first output-buffers))) 0)))')
    ed.wait_until(lambda: os.path.exists(path), 20)
    ed.run_for(0.3)
    return int(open(path).read() or 0)


def rss(pid):
    with open(f"/proc/{pid}/status") as f:
        return round(int(next(l for l in f if l.startswith("VmRSS")).split()[1]) / 1024, 1)


def gone(ed):
    """Milliseconds from now until the mode line says nothing is running."""
    took = ed.wait_until(lambda: "running" not in ed.mode_line(), 10)
    return None if took is None else round(took * 1000, 1)


def scenario(name, binary, work):
    ed = Editor(binary, work)
    ed.run_for(0.3)
    if name != "idle":
        ed.command("!", LOADS[name])
        ed.run_for(float(os.environ.get("WARM", 0.5)))  # WARM=60: a long flood fills the buffer first
    start = time.perf_counter()
    samples = latency(ed)
    took = time.perf_counter() - start
    row = {"scenario": name, "seconds": round(took, 1), **summary(samples)}
    if name != "idle":
        row["lines_during"] = lines(ed, work, name) if name == "flood" else None
        row["rss_mb"] = rss(ed.pid)
        ed.send("\x07")
        row["cg_to_clear_ms"] = gone(ed)
    print_json(row)
    ed.close()


os.makedirs(sys.argv[2], exist_ok=True)
for name in sys.argv[3:] or ["idle", *LOADS]:
    scenario(name, sys.argv[1], sys.argv[2])
