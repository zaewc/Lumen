"""Parent-death guardian. POSIX process groups, no model cooperation required."""

import json
from datetime import datetime
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time

from state import atomic, read


def private_output(path):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.fchmod(descriptor, 0o600)
    return os.fdopen(descriptor, "wb")


def process_table():
    output = subprocess.check_output(["ps", "-axo", "pid=,ppid=,pgid=,stat=,lstart="]).decode()
    return {int(row[0]): {"parent": int(row[1]), "group": int(row[2]),
                          "state": row[3], "birth": " ".join(row[4:])}
            for line in output.splitlines() if len(row := line.split()) >= 9}


def family(table, pgid, known):
    # Retain observed descendants even after reparenting or setsid(). Birth time
    # avoids sending a signal to an unrelated process that reused a recorded PID.
    changed = True
    while changed:
        changed = False
        for pid, info in table.items():
            parent = info["parent"]
            parent_known = parent in known and parent in table and known[parent] == table[parent]["birth"]
            if (info["group"] == pgid or parent_known) and pid not in known:
                known[pid] = info["birth"]
                changed = True
    return {pid: info for pid, info in table.items()
            if not info["state"].startswith("Z") and
            (info["group"] == pgid or known.get(pid) == info["birth"])}


def stop_group(process, known):
    for sig in (signal.SIGTERM, signal.SIGKILL):
        live = family(process_table(), process.pid, known)
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        for pid, info in live.items():
            if info["group"] != process.pid:
                fresh = process_table().get(pid)
                if fresh and fresh["birth"] == info["birth"]:
                    try:
                        os.kill(pid, sig)
                    except ProcessLookupError:
                        pass
        until = time.monotonic() + 2
        while time.monotonic() < until:
            process.poll()
            if not family(process_table(), process.pid, known):
                process.wait()
                return True
            time.sleep(0.05)
    return False


def guard(spec_path, control):
    spec = read(spec_path)
    started = time.time()
    clock_start = time.monotonic()
    process = None
    known = {}
    result = {"started_at": started, "guardian_pid": os.getpid()}
    try:
        with private_output(spec["stdout"]) as output, private_output(spec["stderr"]) as errors:
            env = dict(os.environ, CARGO_TARGET_DIR=spec["target"])
            process = subprocess.Popen(spec["argv"], cwd=spec["cwd"],
                                       stdin=subprocess.DEVNULL, stdout=output,
                                       stderr=errors, env=env, start_new_session=True)
            result["pgid"] = process.pid
            atomic(Path(spec_path).with_suffix(".pid.json"), result)
            reason = "exit"
            while process.poll() is None:
                family(process_table(), process.pid, known)
                if time.monotonic() - clock_start >= spec["timeout"]:
                    reason = "timeout"
                    break
                if select.select([control], [], [], 0.05)[0] and not os.read(control, 1):
                    reason = "supervisor_lost"
                    break
            stopped = stop_group(process, known)
            result.update(code=process.poll(), reason=reason, stopped=stopped)
            output.flush()
            errors.flush()
            os.fsync(output.fileno())
            os.fsync(errors.fileno())
    except Exception as error:
        try:
            stopped = stop_group(process, known) if process else True
        except Exception:
            stopped = False  # Cannot inspect processes: fail closed on resume.
        result.update(code=127, reason="guardian_error", error=str(error), stopped=stopped)
    result["elapsed"] = time.monotonic() - clock_start
    result["observed_processes"] = known
    result["signal"] = -result["code"] if result.get("code", 0) is not None and result["code"] < 0 else None
    atomic(Path(spec_path).with_suffix(".result.json"), result)


def launch(directory, name, argv, worktree, target, timeout, lock_fd):
    """The caller checkpoints this spec path BEFORE launch (also for checks)."""
    directory = Path(directory)
    path = directory / (name + ".json")
    atomic(path, {"argv": argv, "cwd": str(worktree), "target": str(target),
                  "timeout": timeout, "stdout": str(directory / (name + ".jsonl")),
                  "stderr": str(directory / (name + ".stderr"))})
    control, writer = os.pipe()
    try:
        with private_output(directory / (name + ".guardian.stderr")) as diagnostic:
            process = subprocess.Popen([sys.executable, __file__, str(path), str(control)],
                                       pass_fds=(control, lock_fd), start_new_session=True,
                                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                       stderr=diagnostic)
    except BaseException:
        os.close(writer)
        raise
    finally:
        os.close(control)
    try:
        process.wait()
    finally:
        os.close(writer)
        process.wait()
    return read(path.with_suffix(".result.json"))


def events(path, stderr=None):
    result = {"session_id": None, "usage": [], "errors": [], "handoff": "", "completed": False}
    with Path(path).open(errors="replace") as stream:
        for line in stream:
            try:
                event = json.loads(line)
            except ValueError:
                continue  # Keep malformed/truncated bytes in the original log.
            if not isinstance(event, dict):
                continue
            kind = event.get("type")
            if kind == "thread.started":
                result["session_id"] = event.get("thread_id")
            elif kind == "turn.completed":
                result["usage"].append(event.get("usage"))
                result["completed"] = True
            elif kind in ("error", "turn.failed"):
                result["errors"].append(event)
            elif kind == "item.completed" and event.get("item", {}).get("type") == "agent_message":
                result["handoff"] = event["item"].get("text", "")
    if stderr and not result["errors"]:
        # Some CLI startup failures precede the JSON stream. Only explicit fatal
        # limit diagnostics from a failed process are eligible for this fallback.
        for line in Path(stderr).read_text(errors="replace").splitlines()[-40:]:
            lower = line.lower()
            if any(marker in lower for marker in ("usage limit", "usage_limit", "insufficient_quota",
                                                   "context_length_exceeded", "context window exceeded")):
                result["errors"].append({"type": "stderr_error", "message": line})
    return result


def policy(run, event, fingerprint, previous, attempt, maximum=4):
    """Only error envelopes drive retry classification, never agent prose."""
    text = json.dumps(event["errors"]).lower()
    reason = run["reason"]
    if "usage_limit" in text or "usage limit" in text or "quota" in text:
        reason = "usage_limit"
    elif "context_length_exceeded" in text or "context window" in text:
        reason = "context_exhausted"
    elif reason == "exit":
        reason = "success" if run["code"] == 0 and event["completed"] and not event["errors"] else "transient"
    signature = [fingerprint, reason, event["errors"]]
    repeated = previous.get("count", 0) + 1 if previous.get("signature") == signature else 1
    decision = {"reason": reason, "signature": signature, "count": repeated, "retry_at": None}
    if not run["stopped"] or reason == "guardian_error":
        decision["status"] = "blocked_process"
    elif reason == "usage_limit":
        # CLI error text does not promise a machine-readable reset time. An
        # operator may supply one; never guess a date from localized prose.
        decision["status"] = "waiting_usage"
        def reset_time(value):
            if isinstance(value, dict):
                for key, item in value.items():
                    if key in ("retry_at", "resets_at"):
                        try:
                            return float(item)
                        except (TypeError, ValueError):
                            try:
                                return datetime.fromisoformat(item.replace("Z", "+00:00")).timestamp()
                            except (TypeError, ValueError, AttributeError):
                                pass
                    if key == "retry_after":
                        try:
                            return time.time() + max(0, float(item))
                        except (TypeError, ValueError):
                            pass
                    found = reset_time(item)
                    if found is not None:
                        return found
            elif isinstance(value, list):
                for item in value:
                    found = reset_time(item)
                    if found is not None:
                        return found
            return None
        decision["retry_at"] = reset_time(event["errors"])
    elif reason == "success":
        decision["status"] = "agent_exited"
    elif repeated >= 2 or attempt >= maximum:
        decision["status"] = "stopped_repeated_failure"
    else:
        decision.update(status="retry", retry_at=time.time() + min(300, 5 * 2 ** (attempt - 1)))
    return decision


if __name__ == "__main__":
    guard(sys.argv[1], int(sys.argv[2]))
