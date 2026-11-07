"""What must hold, run for real in a pty: a command ends, is cancelled, fails, outlives the editor;
typing and output go on meanwhile; commands end independently of each other and of prompts.
usage: check.py BINARY WORKDIR [TEST...]"""
import os, sys, time
sys.path.insert(0, os.path.dirname(__file__))
from ed import Editor, alive, file_pid, locks

SHELL = "!"
results = []


def report(name, ok, detail=""):
    results.append(ok)
    print(f"{'ok  ' if ok else 'FAIL'} {name} {detail}", flush=True)


def fresh(binary, work):
    ed = Editor(binary, work)
    ed.run_for(0.2)
    return ed


def background(ed, work, name, seconds=300):
    """Starts a command whose child's pid goes in a file; the child's pid and the temp files."""
    pidfile = os.path.join(work, name)
    if os.path.exists(pidfile):
        os.remove(pidfile)
    before = set(locks(ed))
    ed.command(SHELL, f"sleep {seconds} & echo $! > {pidfile}; wait")
    child = file_pid(pidfile, ed)
    return child, set(locks(ed)) - before


def ends(binary, work):
    ed = fresh(binary, work)
    ed.command(SHELL, "seq 1 3000; sleep 1; seq 3001 6000")
    ed.wait_until(lambda: locks(ed), 3)
    running = "[1 running]" in ed.mode_line() and bool(locks(ed))
    ed.keys("typed while it runs")
    shown = ed.wait_until(lambda: "typed while it runs" in ed.screen(), 3) is not None
    ed.send("\x18b")  # C-x b: to the output
    streaming = ed.wait_until(lambda: "3000" in ed.rows[:22], 3) is not None
    mode = "[running]" in ed.mode_line()
    ended = ed.wait_until(lambda: not locks(ed) and "running]" not in ed.mode_line(), 8) is not None
    out, typed = os.path.join(work, "out.txt"), os.path.join(work, "typed.txt")
    ed.send("\x18b")
    ed.run_for(0.1)
    ed.evaluate(f'(dump (first output-buffers) "{out}")')
    ed.evaluate(f'(dump window-buffer "{typed}")')
    ed.run_for(0.3)
    got = open(out).read().split()
    report("ends: running shown, typing lands, output streams, cleanup ran",
           running and shown and streaming and mode and ended,
           f"running={running} shown={shown} streaming={streaming} mode={mode} ended={ended}")
    report("ends: all 6000 lines arrive in order", got == [str(i) for i in range(1, 6001)], f"lines={len(got)}")
    report("ends: the typed text is what was typed", open(typed).read() == "typed while it runs")
    ed.close()


def cancelled(binary, work):
    ed = fresh(binary, work)
    child, _ = background(ed, work, "pid-cancel")
    before = alive(child) and bool(locks(ed))
    ed.keys("x")
    ed.send("\x07")
    gone = ed.wait_until(lambda: not alive(child) and not locks(ed), 3)
    ed.run_for(0.2)
    ed.keys("y")
    ed.run_for(0.2)
    mode_clear = "running" not in ed.mode_line()
    typed = any(r.startswith("xy") for r in ed.rows)
    report("C-g: child killed, temp file removed, mode restored, editing goes on",
           before and gone is not None and mode_clear and typed,
           f"before={before} cleanup_ms={None if gone is None else round(gone * 1000, 1)} "
           f"mode_clear={mode_clear} typed={typed}")
    ed.close()


def fails(binary, work):
    ed = fresh(binary, work)
    ed.command(SHELL, "echo oops; sleep 0.3; exit 3")
    seen = bool(locks(ed))
    ended = ed.wait_until(lambda: not locks(ed), 3) is not None
    said = ed.wait_until(lambda: "shell-failed" in ed.echo(), 2) is not None
    ed.keys("z")
    ed.run_for(0.2)
    report("fails: cleanup ran and the failure is an error shown",
           seen and ended and said and any(r.startswith("z") for r in ed.rows),
           f"seen={seen} ended={ended} said={said}")
    ed.close()


def quits(binary, work):
    ed = fresh(binary, work)
    child, paths = background(ed, work, "pid-quit")
    ed.send("\x18\x03")
    exited = ed.wait_exit()
    temp_left = any(map(os.path.exists, paths))  # the cleanup ran before the exit, or never will
    time.sleep(0.2)  # SIGKILL was sent; a process takes a moment to go, under load
    report("C-x C-c: child killed and temp file removed before exit",
           exited and not alive(child) and paths and not temp_left,
           f"exited={exited} status={ed.status} child_alive={alive(child)} temp_left={temp_left}")
    ed.close()


def out_of_memory_shown(binary, work):
    """Memory really runs out, with no cleanup on the way: the collection that asks for is still due
    when the host reports, and the report must survive it."""
    ed = Editor(binary, work, memory=300 << 20)
    ed.run_for(0.2)
    ed.command(":", "(sequence! (define! deeper (lambda! (n) (add 1 (deeper n)))) (deeper 0))")
    said = ed.wait_until(lambda: "out-of-memory" in ed.echo(), 30) is not None
    report("out of memory: the error reaches the echo area", said, f"echo={ed.echo()!r}")
    ed.close()


def hangs_up(binary, work):
    ed = fresh(binary, work)
    child, paths = background(ed, work, "pid-hup", 20)
    ed.close()
    time.sleep(0.2)
    report("closed terminal: child killed, temp file removed, editor gone",
           not alive(child) and not any(map(os.path.exists, paths)) and ed.exited,
           f"child_alive={alive(child)} exited={ed.exited} status={ed.status}")


def hangs_up_on_a_runaway(binary, work):
    """The terminal closes while a command never returns: no C-g can come, and the editor still ends."""
    ed = fresh(binary, work)
    ed.send("\x1b:")
    ed.run_for(0.2)
    ed.keys("(sequence! (define! spin (lambda! () (spin))) (spin))\r")
    ed.run_for(0.5)
    ed.close()
    report("closed terminal during a runaway: editor gone", ed.exited,
           f"exited={ed.exited} status={ed.status}")


def scrolled_back(binary, work):
    """Typing in the output buffer itself, point away from its end, while output arrives."""
    ed = fresh(binary, work)
    ed.command(SHELL, "seq 1 200; sleep 1; seq 201 400")
    ed.send("\x18b")
    ed.wait_until(lambda: "200" in ed.rows[:22], 3)  # a whole row: the mode line names the command
    ed.keys("\x10\x10\x10Q")  # C-p three times, then a character
    ed.wait_until(lambda: not locks(ed), 4)
    ed.run_for(0.2)
    out = os.path.join(work, "scrolled.txt")
    ed.evaluate(f'(dump (first output-buffers) "{out}")')
    ed.run_for(0.3)
    got = open(out).read().split()
    want = [str(i) for i in range(1, 401)]
    want[197] = "Q198"
    report("typing in the output buffer away from its end keeps every line", got == want, f"lines={len(got)}")
    ed.close()


def two_independent(binary, work):
    """Loss 1: the first command's cleanup must run when it ends, not when the second does."""
    first_for, second_for = 1.0, 4.0
    ed = fresh(binary, work)
    start = time.perf_counter()
    ed.command(SHELL, f"seq 1 100; sleep {first_for}; seq 101 200")
    ed.wait_until(lambda: len(locks(ed)) == 1, 3)
    first = locks(ed)[0]
    ed.command(SHELL, f"seq 1001 1100; sleep {second_for}; seq 1101 1200")
    ed.wait_until(lambda: len(locks(ed)) == 2, 3)
    ed.wait_until(lambda: not os.path.exists(first), second_for + 3)
    first_clean = time.perf_counter() - start
    ed.wait_until(lambda: not locks(ed), second_for + 3)
    all_clean = time.perf_counter() - start
    paths = [os.path.join(work, f"two-{n}.txt") for n in (0, 1)]
    for n, path in enumerate(paths):
        ed.evaluate(f'(dump (first (drop output-buffers {n})) "{path}")')
    ed.run_for(0.3)
    counts = [len(open(p).read().split()) for p in paths]
    report("two commands: each cleaned up when it ends, all output in",
           first_clean < first_for + 0.5 and all_clean < second_for + 1 and counts == [200, 200],
           f"first_cleanup_at={first_clean:.2f}s all_clean_at={all_clean:.2f}s lines={counts}")
    ed.close()


def prompt_holds_nothing(binary, work):
    """Loss 2: a prompt opened after a command started must not hold its cleanup."""
    ed = fresh(binary, work)
    ed.command(SHELL, "sleep 1")
    ed.wait_until(lambda: locks(ed), 3)
    start = time.perf_counter()
    ed.send("\x1bx")  # M-x: a prompt, left open
    ed.run_for(0.1)
    opened = "M-x" in ed.echo()
    gone = ed.wait_until(lambda: not locks(ed), 3)
    still = "M-x" in ed.echo()
    ed.send("abc")
    typed = ed.wait_until(lambda: "M-x abc" in ed.echo(), 2) is not None
    ed.send("\x07")
    ed.run_for(0.2)
    report("a prompt left open doesn't hold a command's cleanup, and stays open",
           opened and gone is not None and still and typed,
           f"gone_after={gone} opened={opened} still={still} typed={typed}")
    ed.close()


def chosen(binary, work):
    """Loss 3: C-g reaches the command whose output is in the window, not only the newest."""
    ed = fresh(binary, work)
    older, older_files = background(ed, work, "pid-older")
    newer, newer_files = background(ed, work, "pid-newer")
    middle, _ = background(ed, work, "pid-middle")
    ed.send("\x18j")  # C-x j: the newest's output
    ed.run_for(0.1)
    ed.send("\x18j")  # the one before: "newer"
    ed.run_for(0.1)
    ed.send("\x07")
    ed.wait_until(lambda: not alive(newer), 3)
    ed.run_for(0.1)
    first = alive(older) and not alive(newer) and alive(middle) and not any(map(os.path.exists, newer_files))
    ed.send("\x18b")  # back to the text: C-g there takes the newest
    ed.run_for(0.1)
    ed.send("\x07")
    ed.wait_until(lambda: not alive(middle), 3)
    second = alive(older) and not alive(middle)
    ed.send("\x07")
    ed.wait_until(lambda: not alive(older), 3)
    report("C-g in a command's output cancels that one; elsewhere the newest, one at a time",
           first and second and not alive(older) and not any(map(os.path.exists, older_files)),
           f"first={first} second={second} older_alive={alive(older)}")
    ed.close()


def runaway(binary, work):
    """Loss 4: stopping a runaway must end only the runaway: as the current command and as a run
    of its own."""
    ed = fresh(binary, work)
    child, _ = background(ed, work, "pid-runaway")
    ed.evaluate("(sequence! (define! spin (lambda! () (spin))) (spin))")
    ed.run_for(0.3)
    ed.send("\x07")
    ed.run_for(0.3)
    command = alive(child) and "interrupted" in ed.echo()
    ed.evaluate("(spawn (lambda! () (spin)))")
    ed.run_for(0.3)
    ed.send("\x07")
    ed.run_for(0.3)
    spawned = alive(child) and "interrupted" in ed.echo()
    ed.keys("a")
    ed.run_for(0.1)
    typed = any(r.endswith("a") for r in ed.rows[:22])
    report("stopping a runaway leaves the other commands running, and the editor typing",
           command and spawned and typed, f"command={command} spawned={spawned} typed={typed}")
    ed.close()


def waiting_command(binary, work):
    """A command that waits by itself, not through M-!: it reads top to bottom and holds nothing."""
    ed = fresh(binary, work)
    after = os.path.join(work, "after-wait")
    if os.path.exists(after):
        os.remove(after)
    ed.evaluate(f'(sequence! (run-shell "sleep 0.6") (write-file "{after}" "here"))')
    ed.keys("typed")
    typed = any(r.startswith("typed") for r in ed.rows)
    early = os.path.exists(after)
    went_on = ed.wait_until(lambda: os.path.exists(after), 3) is not None
    ed.keys("!")
    ed.run_for(0.1)
    report("a command that waits by itself holds up nothing, and goes on after",
           typed and not early and went_on and any(r.startswith("typed!") for r in ed.rows),
           f"typed={typed} early={early} went_on={went_on}")
    ed.close()


def spawned_chain(binary, work):
    """Work queued by spawned work starts with no key pressed, and a program that keeps spawning
    leaves typing alone."""
    ed = fresh(binary, work)
    done = os.path.join(work, "spawned-chain")
    if os.path.exists(done):
        os.remove(done)
    ed.evaluate(f'(spawn (lambda! () (spawn (lambda! () (spawn (lambda! () (write-file "{done}" "here")))))))')
    started = ed.wait_until(lambda: os.path.exists(done), 3)
    beat = os.path.join(work, "spawned-beat")
    ed.evaluate(f'(define! again (lambda! () (write-file "{beat}" "here") (spawn again)))')
    ed.evaluate("(again)")
    ed.keys("typed")
    typed = ed.wait_until(lambda: any(r.startswith("typed") for r in ed.rows), 3)
    os.remove(beat)
    spinning = ed.wait_until(lambda: os.path.exists(beat), 3)
    report("work queued by spawned work starts by itself, and typing beats a program that keeps spawning",
           started is not None and typed is not None and spinning is not None,
           f"started={started} typed={typed} spinning={spinning}")
    ed.close()


def under_a_prompt(binary, work):
    """M-! typed inside a prompt: the prompt stays usable, and the command runs beside it."""
    ed = fresh(binary, work)
    ed.send("\x1bx")
    ed.run_for(0.1)
    ed.command(SHELL, "sleep 0.5")
    ed.run_for(0.1)
    ed.send("abc")
    prompt = ed.wait_until(lambda: "M-x abc" in ed.echo(), 2) is not None
    running = bool(locks(ed))
    ended = ed.wait_until(lambda: not locks(ed), 3) is not None
    still = "M-x abc" in ed.echo()
    ed.send("\x07")
    ed.run_for(0.2)
    report("M-! inside a prompt: the prompt stays and the command runs beside it",
           prompt and running and ended and still, f"prompt={prompt} running={running} ended={ended} still={still}")
    ed.close()


def run_asks(binary, work):
    """A run that opens a prompt while another waits for a key gets the keys until it is done."""
    ed = fresh(binary, work)
    answer = os.path.join(work, "answer.txt")
    if os.path.exists(answer):
        os.remove(answer)
    ed.evaluate(f'(spawn (lambda! () (run-shell "sleep 0.3") '
                f'(write-file "{answer}" (read-from-minibuffer "Well? "))))')
    asked = ed.wait_until(lambda: ed.echo().startswith("Well?"), 3) is not None
    ed.keys("yes\r")
    ed.run_for(0.1)
    ed.keys("q")
    ed.run_for(0.1)
    wrote = os.path.exists(answer) and open(answer).read() == "yes"
    report("a command that asks after its wait gets the answer, then typing goes back",
           asked and wrote and any(r.startswith("q") for r in ed.rows), f"asked={asked} wrote={wrote}")
    ed.close()


def editor_broken(binary, work):
    """The editor's own drawing fails: the host says so, and q quits, cleaning up."""
    ed = fresh(binary, work)
    child, paths = background(ed, work, "pid-broken")
    ed.evaluate("(define! redisplay (lambda! () (raise (error (quote! broken)))))")
    asked = ed.wait_until(lambda: "the editor itself fails" in ed.screen(), 3) is not None
    ed.send("q")
    exited = ed.wait_exit()
    time.sleep(0.2)
    report("a broken redisplay is reported; q quits and the cleanups run",
           asked and exited and not alive(child) and not any(map(os.path.exists, paths)),
           f"asked={asked} exited={exited} child_alive={alive(child)}")
    ed.close()


def main():
    binary, work = sys.argv[1], sys.argv[2]
    os.makedirs(work, exist_ok=True)
    tests = (ends, cancelled, fails, out_of_memory_shown, quits, hangs_up, hangs_up_on_a_runaway, scrolled_back, two_independent,
             prompt_holds_nothing, chosen, runaway, waiting_command, under_a_prompt, run_asks,
             editor_broken, spawned_chain)
    for test in tests:
        if len(sys.argv) < 4 or test.__name__ in sys.argv[3:]:
            test(binary, work)
    sys.exit(0 if all(results) else 1)


main()
