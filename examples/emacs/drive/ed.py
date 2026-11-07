"""Drives the real grift-emacs binary in a pty of its own, and keeps a small model of its screen."""
import fcntl, glob, json, os, pty, re, select, signal, struct, termios, time

ROWS, COLS = 24, 80
CSI = re.compile(rb"\x1b\[([?0-9;]*)([A-Za-z])")
INIT = """
(define! dump
  (lambda! (b path)
    (define! saved current-buffer)
    (bind editor-context (quote! current-buffer) b)
    (write-file path (buffer-text))
    (bind editor-context (quote! current-buffer) saved)))
"""


class Editor:
    def __init__(self, binary, work, init=True):
        self.work = work
        args = [binary]
        if init:
            path = os.path.join(work, "init.grift")
            with open(path, "w") as f:
                f.write(INIT)
            args += ["--init", path]
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execv(binary, args)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        self.rows = [""] * ROWS
        self.row = 0
        self.fresh = False
        self.pending = b""
        self.frames = 0
        self.alive = True
        self.status = None
        self.exited = False

    def send(self, data):
        os.write(self.fd, data.encode() if isinstance(data, str) else data)

    def feed(self, data):
        """Applies terminal output to the model; True if a frame ended in it."""
        data = self.pending + data
        at, framed = 0, False
        while at < len(data):
            esc = data.find(b"\x1b", at)
            text = data[at:] if esc < 0 else data[at:esc]
            if text:
                # Text after a move to column 1 replaces the row, as the mode line relies on.
                old = "" if self.fresh else self.rows[self.row]
                self.rows[self.row] = old + text.decode("utf-8", "replace")
                self.fresh = False
            if esc < 0:
                at = len(data)
                break
            m = CSI.match(data, esc)
            if not m:
                if len(data) - esc < 16:
                    break
                at = esc + 1
                continue
            at = m.end()
            arg, final = m.group(1).decode(), m.group(2).decode()
            if final == "H":
                self.row = min(ROWS, max(1, int(arg.split(";")[0] or 1))) - 1
                self.fresh = arg.split(";")[-1] == "1"
            elif final == "K":
                self.rows[self.row] = ""
            elif final == "h" and arg == "?25":
                self.frames += 1
                framed = True
        self.pending = data[at:]
        return framed

    def pump(self, timeout):
        """Reads what the editor wrote within `timeout` seconds; True if a frame ended."""
        if not self.alive:
            return False
        ready, _, _ = select.select([self.fd], [], [], timeout)
        if not ready:
            return False
        try:
            data = os.read(self.fd, 1 << 16)
        except OSError:
            data = b""
        if not data:
            self.alive = False
            return False
        return self.feed(data)

    def run_for(self, seconds):
        end = time.perf_counter() + seconds
        while (left := end - time.perf_counter()) > 0:
            self.pump(left)

    def wait_until(self, test, timeout=10.0):
        """Pumps until test() holds; the seconds it took, or None."""
        start = time.perf_counter()
        while time.perf_counter() - start < timeout:
            if test():
                return time.perf_counter() - start
            self.pump(0.02)
        return None

    def screen(self):
        return "\n".join(self.rows)

    def mode_line(self):
        return self.rows[ROWS - 2]

    def echo(self):
        return self.rows[ROWS - 1]

    def keys(self, text, gap=0.02):
        """Types text a key at a time."""
        for c in text:
            self.send(c)
            self.run_for(gap)

    def command(self, key, text):
        """Runs a command that reads a line, as M-! does: ESC, its key, the line, RET."""
        self.send("\x1b" + key)
        self.run_for(0.05)
        self.send(text)
        self.run_for(0.05)
        self.send("\r")
        self.run_for(0.05)

    def evaluate(self, form):
        """Runs a form with M-:, waiting until its prompt has closed: typing a long one takes a while."""
        self.command(":", form)
        self.run_for(0.02)
        self.wait_until(lambda: not self.echo().startswith("Eval:"), 15)

    def wait_exit(self, timeout=5.0):
        end = time.perf_counter() + timeout
        while time.perf_counter() < end:
            self.pump(0.02)
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.status = status
                self.alive = False
                return True
        return False

    def close(self):
        """Hangs up, as closing a terminal does, so the editor's cleanups run; kills it if it stays."""
        os.close(self.fd)
        if self.status is None:
            end = time.perf_counter() + 5
            while time.perf_counter() < end:
                pid, status = os.waitpid(self.pid, os.WNOHANG)
                if pid:
                    self.status = status
                    self.exited = True
                    return
                time.sleep(0.01)
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)


def latency(ed, count=200, line=40, gap=0.02):
    """Key to screen, in ms: each key typed alone, timed until a whole frame shows the current line
    with it. `line` keys make a line, then RET, which isn't timed. Returns the samples; a key that
    doesn't show in 10 s is a miss, reported as None."""
    letters = "abcdefghijklmnopqrstuvwxyz"
    samples, text, n = [], "", 0
    while len(samples) < count:
        if len(text) == line:
            ed.send("\r")
            text = ""
            ed.run_for(0.05)
        c = letters[(n * 7 + n // 26) % 26]
        n += 1
        text += c
        start = time.perf_counter()
        ed.send(c)
        shown = None
        while time.perf_counter() - start < 10:
            framed = ed.pump(0.5)
            if framed and any(r == text for r in ed.rows[: ROWS - 2]):
                shown = (time.perf_counter() - start) * 1000
                break
        samples.append(shown)
        ed.run_for(gap)
    return samples


def summary(samples):
    good = sorted(s for s in samples if s is not None)
    if not good:
        return {"n": 0, "missed": len(samples)}
    at = lambda q: round(good[min(len(good) - 1, int(q * len(good)))], 2)
    return {"n": len(good), "missed": len(samples) - len(good), "p50": at(0.5), "p95": at(0.95),
            "p99": at(0.99), "max": round(good[-1], 2)}


def alive(pid):
    """Whether the process exists and isn't a zombie."""
    try:
        with open(f"/proc/{pid}/stat") as f:
            return f.read().rsplit(")", 1)[1].split()[0] != "Z"
    except OSError:
        return False


def locks(ed):
    """The temporary files the editor's commands hold."""
    return sorted(glob.glob(os.path.join(os.environ["TMPDIR"], f"grift-emacs-{ed.pid}-*")))


def file_pid(path, ed):
    """The pid a command wrote to a file."""
    ed.wait_until(lambda: os.path.exists(path) and open(path).read().strip(), 5)
    return int(open(path).read())


def print_json(value):
    print(json.dumps(value), flush=True)
