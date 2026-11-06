"""Where the design gives way. Prints what happened; nothing here passes or fails.
usage: limits.py BINARY WORKDIR [TEST...]"""
import os, sys, time
sys.path.insert(0, os.path.dirname(__file__))
from ed import Editor, alive, file_pid, locks, print_json

SHELL = "!"


def start(binary, work):
    ed = Editor(binary, work)
    ed.run_for(0.2)
    return ed


def define_waiting_cleanup(ed, work, name):
    """(waiter): waits for a long job, its cleanup waits for a short one, catching what that says,
    then writes a file. Returns the two files it writes."""
    said, after = os.path.join(work, f"{name}-said"), os.path.join(work, f"{name}-after")
    for path in (said, after):
        if os.path.exists(path):
            os.remove(path)
    ed.evaluate(" ".join(f"""(define! waiter (lambda! (seconds)
      (define! long (make-job (output-buffer "long")))
      (define! short (make-job (output-buffer "short")))
      (finally!
        (sequence! (add-job long (job-start seconds)) (wait-for long))
        (cancel-job long)
        (add-job short (job-start "sleep 0.3"))
        (catch! (wait-for short) e (write-file "{said}" (value-to-string (evaluate-in! kind e))))
        (write-file "{after}" "after"))))""".split()))
    return said, after


def waits_in_cleanup(binary, work):
    """A cleanup that waits: on a normal end it waits; when abandoned that call fails."""
    out = {}
    for how in ("normal_end", "c_g", "c_x_c_c"):
        ed = start(binary, work)
        said, after = define_waiting_cleanup(ed, work, how)
        ed.evaluate(f'(spawn (lambda! () (waiter "{"sleep 0.2" if how == "normal_end" else "sleep 30"}")))')
        ed.run_for(0.1)
        if how == "c_g":
            ed.send("\x07")
        if how == "c_x_c_c":
            ed.send("\x18\x03")
            ed.wait_exit()
        ed.run_for(1.0)
        out[how] = {"wait_said": open(said).read() if os.path.exists(said) else None,
                    "forms_after_ran": os.path.exists(after)}
        ed.close()
    return out


def uncaught_wait_in_cleanup(binary, work):
    """The same with nothing catching: which forms of the finally! still run when C-g abandons it."""
    ed = start(binary, work)
    before, after = os.path.join(work, "form-before"), os.path.join(work, "form-after")
    outer = os.path.join(work, "form-outer")
    for path in (before, after, outer):
        if os.path.exists(path):
            os.remove(path)
    ed.evaluate(" ".join(f"""(spawn (lambda! ()
      (define! long (make-job (output-buffer "long")))
      (define! short (make-job (output-buffer "short")))
      (finally!
        (finally!
          (sequence! (add-job long (job-start "sleep 30")) (wait-for long))
          (cancel-job long)
          (write-file "{before}" "ran")
          (add-job short (job-start "sleep 0.3"))
          (wait-for short)
          (write-file "{after}" "ran"))
        (write-file "{outer}" "ran"))))""".split()))
    ed.run_for(0.1)
    ed.send("\x07")
    ed.run_for(0.5)
    got = {"form_before_the_wait": os.path.exists(before), "form_after_the_wait": os.path.exists(after),
           "outer_finally": os.path.exists(outer), "echo": ed.echo()[:40]}
    ed.close()
    return got


def stuck_cleanup(binary, work):
    """A cleanup that never ends, when the editor is leaving: how each way out gets past it."""
    out = {}
    for how in ("closed_terminal", "c_x_c_c_then_c_g"):
        ed = start(binary, work)
        ed.evaluate('(define! stuck (lambda! () (define! job (make-job (output-buffer "stuck"))) '
                    '(finally! (sequence! (add-job job (job-start "sleep 30")) (wait-for job)) '
                    '(define! spin (lambda! () (spin))) (spin))))')
        ed.evaluate("(spawn stuck)")
        ed.run_for(0.1)
        start_at = time.perf_counter()
        if how == "closed_terminal":
            ed.close()
            out[how] = {"exited": ed.exited, "seconds": round(time.perf_counter() - start_at, 2)}
        else:
            ed.send("\x18\x03")
            ed.run_for(0.3)
            ed.send("\x07")
            ed.send("\x07")
            out[how] = {"exited": ed.wait_exit(), "seconds": round(time.perf_counter() - start_at, 2)}
            ed.close()
    return out


def cleanup_error(binary, work):
    """A cleanup form that fails: the forms after it in that finally! don't run, as in any grift
    finally!."""
    ed = start(binary, work)
    ed.evaluate('(define! delete-file (lambda! (path) (raise (error (quote! disk-gone) path))))')
    ed.run_for(0.2)
    ed.command(SHELL, "sleep 0.3")
    ed.send("\x18b")
    ed.run_for(1.0)
    mode = ed.mode_line()
    echo = ed.echo()
    ed.keys("b")
    ed.run_for(0.2)
    got = {"mode_line_after": mode[:36], "echo": echo[:50],
           "typing_after": any(r.endswith("b") for r in ed.rows[:22])}
    for path in locks(ed):
        os.remove(path)
    ed.close()
    return got


def quiet_runaway(binary, work):
    """A run that became a runaway after its wait: is it one C-g or two?"""
    ed = start(binary, work)
    pidfile = os.path.join(work, "pid-other")
    if os.path.exists(pidfile):
        os.remove(pidfile)
    ed.command(SHELL, f"sleep 300 & echo $! > {pidfile}; wait")
    other = file_pid(pidfile, ed)
    ed.evaluate('(sequence! (define! spin (lambda! () (spin))) '
                '(spawn (lambda! () (run-shell "sleep 0.2") (spin))))')
    ed.run_for(0.6)
    stuck = not any(r.startswith("a") for r in ed.rows[:22])
    ed.send("\x07")
    ed.run_for(0.3)
    first = "interrupted" in ed.echo()
    ed.send("\x07")
    ed.run_for(0.3)
    second = "interrupted" in ed.echo()
    ed.keys("a")
    ed.run_for(0.2)
    got = {"stopped_by_first_c_g": first, "stopped_by_second_c_g": second, "other_alive": alive(other),
           "typing_after": any(r.endswith("a") for r in ed.rows[:22]), "was_stuck": stuck}
    ed.close()
    return got


def idle_quit(binary, work):
    """What C-g shows with nothing to cancel, and in a prefix."""
    ed = start(binary, work)
    ed.send("\x07")
    ed.run_for(0.2)
    idle = ed.echo()[:70]
    ed.send("\x18")
    ed.run_for(0.1)
    ed.send("\x07")
    ed.run_for(0.2)
    prefix = ed.echo()[:70]
    ed.close()
    return {"idle_echo": idle, "after_prefix_echo": prefix}


TESTS = [waits_in_cleanup, uncaught_wait_in_cleanup, stuck_cleanup, cleanup_error, quiet_runaway, idle_quit]
os.makedirs(sys.argv[2], exist_ok=True)
for test in TESTS:
    if len(sys.argv) < 4 or test.__name__ in sys.argv[3:]:
        print_json({"test": test.__name__, **test(sys.argv[1], sys.argv[2])})
