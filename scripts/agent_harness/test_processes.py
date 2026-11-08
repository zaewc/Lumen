import json
from pathlib import Path
import signal
import subprocess
import sys
import time

from processes import events, launch, policy, process_table
from state import lock, read, snapshot
from test_state import Repository


class ProcessTests(Repository):
    def test_killed_agent_keeps_index_worktree_and_untracked(self):
        directory = self.root / ".git" / "runs"
        directory.mkdir()
        source = """from pathlib import Path
import os, signal, subprocess
Path('tracked').write_text('staged')
subprocess.run(['git', 'add', 'tracked'], check=True)
Path('tracked').write_text('unstaged')
Path('new').write_text('untracked')
os.kill(os.getpid(), signal.SIGKILL)
"""
        with lock(self.root) as fd:
            run = launch(directory, "agent", [sys.executable, "-c", source], self.root,
                         directory / "target", 10, fd)
        self.assertEqual(run["signal"], signal.SIGKILL)
        self.assertTrue(run["stopped"])
        for suffix in ("jsonl", "stderr", "guardian.stderr"):
            self.assertEqual((directory / ("agent." + suffix)).stat().st_mode & 0o777, 0o600)
        before = snapshot(self.root)
        with lock(self.root) as fd:
            resumed = launch(directory, "resumed", [sys.executable, "-c",
                             "from pathlib import Path; assert Path('new').read_text() == 'untracked'"],
                             self.root, directory / "target", 10, fd)
        self.assertEqual(resumed["code"], 0)
        self.assertEqual(before, snapshot(self.root))

    def test_supervisor_sigkill_guardian_retains_lock_and_cleans_children(self):
        directory = self.root / ".git" / "runs"
        directory.mkdir()
        module = str(Path(__file__).parent)
        source = """import subprocess, sys, time
from pathlib import Path
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
Path('ready').write_text(str(child.pid))
time.sleep(60)
"""
        supervisor = """import sys
sys.path.insert(0, sys.argv[1])
from state import lock
from processes import launch
with lock(sys.argv[2]) as fd:
    launch(sys.argv[3], 'crash', [sys.executable, '-c', sys.argv[4]], sys.argv[2], sys.argv[3], 30, fd)
"""
        parent = subprocess.Popen([sys.executable, "-c", supervisor, module, str(self.root),
                                   str(directory), source])
        self.addCleanup(lambda: parent.poll() is None and parent.kill())
        deadline = time.monotonic() + 10
        while not (self.root / "ready").exists() and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertTrue((self.root / "ready").exists())
        with self.assertRaises(RuntimeError):
            with lock(self.root):
                pass
        parent.kill()
        parent.wait()
        result = directory / "crash.result.json"
        while not result.exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertTrue(read(result)["stopped"])
        self.assertEqual(read(result)["reason"], "supervisor_lost")
        with lock(self.root):
            pass

    def test_timeout_and_usage_and_repeated_failure(self):
        directory = self.root / ".git" / "runs"
        directory.mkdir()
        with lock(self.root) as fd:
            run = launch(directory, "timeout", [sys.executable, "-c", "import time; time.sleep(60)"],
                         self.root, directory, 0.1, fd)
        self.assertEqual(run["reason"], "timeout")
        self.assertTrue(run["stopped"])
        event = {"errors": [{"message": "usage limit reached"}], "completed": False}
        self.assertEqual(policy(run, event, "fp", {}, 1)["status"], "waiting_usage")
        event["errors"][0]["retry_at"] = 2000000000
        self.assertEqual(policy(run, event, "fp", {}, 1)["retry_at"], 2000000000)
        event["errors"] = [{"code": "context_length_exceeded"}]
        first = policy(run, event, "fp", {}, 1)
        self.assertEqual(first["reason"], "context_exhausted")
        self.assertEqual(first["status"], "retry")
        self.assertEqual(policy(run, event, "fp", first, 2)["status"], "stopped_repeated_failure")
        event["errors"] = []
        self.assertEqual(policy(run, event, "fp", {}, 4)["status"], "stopped_repeated_failure")

    def test_jsonl_retains_session_usage_and_handoff_without_using_prose_as_policy(self):
        path = self.root / ".git" / "events.jsonl"
        path.write_text('\n'.join(json.dumps(e) for e in [
            {"type": "thread.started", "thread_id": "fixture-session"},
            {"type": "item.completed", "item": {"type": "agent_message", "text": "usage limit"}},
            {"type": "turn.completed", "usage": {"input_tokens": 20}},
        ]) + '\n{truncated')
        event = events(path)
        self.assertEqual(event["session_id"], "fixture-session")
        self.assertEqual(event["usage"], [{"input_tokens": 20}])
        self.assertEqual(policy({"reason": "exit", "code": 0, "stopped": True}, event,
                                "fp", {}, 1)["status"], "agent_exited")

    def test_observed_child_in_separate_session_is_also_stopped(self):
        directory = self.root / ".git" / "runs"
        directory.mkdir()
        source = """import subprocess, sys, time
from pathlib import Path
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'], start_new_session=True)
Path('escaped-child').write_text(str(child.pid))
time.sleep(60)
"""
        with lock(self.root) as fd:
            result = launch(directory, "escaped", [sys.executable, "-c", source], self.root, directory, 0.5, fd)
        child = int((self.root / "escaped-child").read_text())
        self.assertTrue(result["stopped"])
        self.assertIn(str(child), result["observed_processes"])
        remaining = process_table().get(child)
        self.assertTrue(remaining is None or remaining["state"].startswith("Z"))

    def test_cli_startup_limit_on_stderr_waits_without_json_events(self):
        output = self.root / ".git" / "output.jsonl"
        errors = self.root / ".git" / "output.stderr"
        output.write_text("")
        errors.write_text("ERROR: You've hit your usage limit")
        event = events(output, errors)
        run = {"reason": "exit", "code": 1, "stopped": True}
        self.assertEqual(policy(run, event, "fp", {}, 1)["status"], "waiting_usage")
        self.assertIsNone(policy(run, event, "fp", {}, 1)["retry_at"])
