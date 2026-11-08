import sys
from unittest.mock import patch

from state import atomic, git, lock, read, snapshot
from supervisor import Supervisor
from test_state import Repository


class OfflineGitHub:
    def discover(self, branch, base):
        return None


class SupervisorTests(Repository):
    def setUp(self):
        super().setUp()
        self.directory = self.root / ".git" / "harness"
        self.path = self.directory / "state.json"
        self.head = git(self.root, "rev-parse", "HEAD")
        self.check = [sys.executable, "-c", "print('targeted check passed')"]
        self.initial = {
            "worktree": str(self.root), "branch": "feat/fixture", "base": "develop",
            "base_sha": self.head, "snapshot": snapshot(self.root), "status": "ready",
            "request": "Implement fixture", "conditions": "Keep all changes", "next_step": "Continue",
            "attempts": 0, "max_attempts": 4, "checks": [self.check],
            "check_timeout": 10, "agent_timeout": 10, "agent": ["codex"], "history": [],
        }
        atomic(self.path, self.initial)

    def test_changed_files_require_inspection_and_rerun_checks(self):
        with lock(self.root) as fd, patch("supervisor.remote_head", return_value=None):
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            supervisor.state["status"] = "agent_exited"
            supervisor.save()
            supervisor.step()
            self.assertEqual(len(supervisor.state["history"]), 1)
            supervisor.step()
            self.assertEqual(len(supervisor.state["history"]), 1)
            (self.root / "tracked").write_text("changed after check")
            with self.assertRaisesRegex(RuntimeError, "unexpected Git changes"):
                supervisor.step()
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            supervisor.step(accept_changes=True)
            self.assertEqual(len(supervisor.state["history"]), 2)
            self.assertEqual(supervisor.state["validated_fingerprint"], snapshot(self.root)["fingerprint"])

    def test_recovers_agent_result_and_real_diff_without_handoff(self):
        before = snapshot(self.root)
        (self.root / "tracked").write_text("staged")
        git(self.root, "add", "tracked")
        (self.root / "tracked").write_text("unstaged")
        (self.root / "new").write_text("untracked")
        state = self.initial
        state.update(status="running", attempts=1, active={"name": "0001-agent", "kind": "agent",
                     "argv": ["codex"], "before": before})
        atomic(self.path, state)
        atomic(self.directory / "0001-agent.result.json", {"code": -9, "reason": "exit", "stopped": True})
        (self.directory / "0001-agent.jsonl").write_text('{"type":"thread.started","thread_id":"killed"}\n')
        with lock(self.root) as fd, patch("supervisor.remote_head", return_value=None):
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            supervisor.reconcile()
            self.assertEqual(supervisor.state["session_id"], "killed")
            self.assertEqual(supervisor.state["handoff"], "")
            self.assertIn("tracked", supervisor.state["actual_changes"])
            self.assertIn("unstaged", read(self.directory / "diff.json")["working_patch"])
            self.assertIn("staged", read(self.directory / "diff.json")["staged_patch"])
            self.assertEqual(supervisor.state["snapshot"], snapshot(self.root))
            self.assertIn("new", supervisor.prompt())

    def test_zero_exit_does_not_complete_task_and_usage_wait_runs_no_agent(self):
        state = self.initial
        state.update(status="waiting_usage", policy={"status": "waiting_usage", "retry_at": None})
        atomic(self.path, state)
        with lock(self.root) as fd, patch("supervisor.remote_head", return_value=None), \
                patch("supervisor.launch") as launch:
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            supervisor.step()
            launch.assert_not_called()
            self.assertEqual(supervisor.state["status"], "waiting_usage")
            supervisor.state.update(status="agent_exited", policy={})
            supervisor.save()
        with lock(self.root) as fd, patch("supervisor.remote_head", return_value=None):
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            supervisor.step()
            self.assertEqual(supervisor.state["status"], "local_validated")
            self.assertIsNone(supervisor.state["ci"])

    def test_ci_wait_never_starts_agent_and_rechecks_new_pr_head(self):
        class Published(OfflineGitHub):
            head = self.head
            def discover(inner, branch, base):
                return {"number": 3, "state": "open", "head": {"sha": inner.head}}
            def ci(inner, number):
                return {"head_sha": inner.head, "target_sha": "merge", "verified": True}
        gh = Published()
        with lock(self.root) as fd, patch("supervisor.remote_head", return_value=self.head):
            supervisor = Supervisor(self.path, gh, fd)
            supervisor.step()
            self.assertEqual(supervisor.state["status"], "verified")
            gh.head = "new-remote-head"
            supervisor.step()
            self.assertEqual(supervisor.state["status"], "waiting_ci")
            self.assertEqual(supervisor.state["attempts"], 0)

    def test_unproven_spawn_outcome_blocks_new_writer(self):
        self.initial["active"] = {"name": "lost", "kind": "agent", "argv": ["codex"],
                                  "before": self.initial["snapshot"]}
        atomic(self.path, self.initial)
        with lock(self.root) as fd, patch("supervisor.remote_head", return_value=None):
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            with self.assertRaisesRegex(RuntimeError, "verify old processes"):
                supervisor.reconcile()
            self.assertEqual(read(self.path)["status"], "blocked_process")

    def test_network_failure_invalidates_previous_ci_evidence(self):
        self.initial.update(status="verified", ci={"verified": True, "head_sha": self.head})
        atomic(self.path, self.initial)
        github = OfflineGitHub()
        with lock(self.root) as fd, patch.object(github, "discover", side_effect=RuntimeError("offline")):
            supervisor = Supervisor(self.path, github, fd)
            with self.assertRaisesRegex(RuntimeError, "offline"):
                supervisor.step()
        self.assertIsNone(read(self.path)["ci"])
        self.assertEqual(read(self.path)["status"], "waiting_ci")

    def test_fresh_session_continues_all_change_layers_after_sigkill(self):
        script = self.directory / "fake_codex.py"
        script.write_text('''import json, os, signal, subprocess
from pathlib import Path
marker = Path('.git/first-attempt')
if not marker.exists():
    print(json.dumps({'type':'thread.started','thread_id':'session-one'}), flush=True)
    Path('tracked').write_text('staged')
    subprocess.run(['git','add','tracked'], check=True)
    Path('tracked').write_text('unstaged')
    Path('new').write_text('untracked')
    marker.touch()
    os.kill(os.getpid(), signal.SIGKILL)
else:
    assert subprocess.check_output(['git','show',':tracked']).decode() == 'staged'
    assert Path('tracked').read_text() == 'unstaged'
    assert Path('new').read_text() == 'untracked'
    print(json.dumps({'type':'thread.started','thread_id':'session-two'}), flush=True)
    Path('continued').write_text('continued from real files')
    print(json.dumps({'type':'turn.completed','usage':{'input_tokens':1}}), flush=True)
''')
        self.initial["agent"] = [sys.executable, str(script)]
        atomic(self.path, self.initial)
        with lock(self.root) as fd, patch("supervisor.remote_head", return_value=None):
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            supervisor.step()
            self.assertEqual(supervisor.state["status"], "retry")
            self.assertEqual(supervisor.state["session_id"], "session-one")
            supervisor.state["policy"]["retry_at"] = 0
            supervisor.save()
            supervisor = Supervisor(self.path, OfflineGitHub(), fd)
            supervisor.step()
            self.assertEqual(supervisor.state["session_id"], "session-two")
            self.assertEqual(supervisor.state["status"], "local_validated")
            self.assertTrue((self.root / "continued").exists())
