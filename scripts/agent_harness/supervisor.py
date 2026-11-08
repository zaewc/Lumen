"""Checkpointed transitions, independent of an agent's handoff or exit code."""

from pathlib import Path
import subprocess
import time

from github import remote_head
from processes import events, launch, policy
from state import atomic, conflicts, git, inspect, read, snapshot


class Supervisor:
    def __init__(self, path, github, lock_fd):
        self.path = Path(path)
        self.state = read(path)
        self.github = github
        self.lock_fd = lock_fd
        self.directory = self.path.parent
        self.worktree = self.state["worktree"]

    def save(self):
        self.state["snapshot"] = snapshot(self.worktree)
        self.state["updated_at"] = time.time()
        atomic(self.path, self.state)

    def stop(self, status, reason):
        self.state.update(status=status, reason=reason)
        self.save()
        raise RuntimeError(str(reason))

    def reconcile(self, accept_changes=False):
        state = self.state
        current = snapshot(self.worktree)
        problem = conflicts(self.worktree)
        state["ci"] = None  # Invalidate even if the following network read fails.
        if state["status"] == "verified":
            state["status"] = "waiting_ci"
        atomic(self.path, state)  # Keep the old snapshot until reconciliation.
        # GitHub and remote reads happen on every resume, even after an agent
        # exits successfully. No checkpoint is proof of a remote side effect.
        pr = self.github.discover(state["branch"], state["base"])
        state["remote_sha"] = remote_head(self.worktree, state["branch"])
        state["actual_changes"] = inspect(self.worktree, self.directory)
        state["change_summary"] = git(self.worktree, "diff", state["base_sha"], "--stat", "--no-ext-diff", "--no-textconv")
        if problem:
            self.stop("blocked_git", problem)
        if current["branch"] != state["branch"]:
            self.stop("blocked_git", "worktree branch changed")
        try:
            git(self.worktree, "merge-base", "--is-ancestor", state["base_sha"], current["head"])
        except subprocess.CalledProcessError:
            self.stop("blocked_git", "HEAD no longer descends from the checkpoint base")
        changed = current["fingerprint"] != state["snapshot"]["fingerprint"]
        if changed:
            state.setdefault("recoveries", []).append({"before": state["snapshot"], "after": current,
                                                       "observed_at": time.time()})
            if not state.get("active") and not accept_changes:
                state["interrupted_status"] = state.get("interrupted_status", state["status"])
                self.stop("needs_inspection", "unexpected Git changes; inspect diff.json, then --accept-changes")
            state["next_step"] = "Inspect actual_changes and diff.json; continue the original request from the real worktree."
        if accept_changes and state.get("interrupted_status"):
            state["status"] = state.pop("interrupted_status")
        if state.get("active"):
            self.finish()
        if pr:
            state["pr"] = {"number": pr["number"], "head_sha": pr["head"]["sha"], "state": pr["state"]}
            if pr["state"] != "open":
                self.stop("closed_pr", "existing PR is closed; preserve it for review")
        state.pop("reason", None)
        self.save()

    def execute(self, kind, argv, timeout):
        state = self.state
        sequence = state.get("sequence", 0) + 1
        state["sequence"] = sequence
        name = f"{sequence:04d}-{kind}"
        before = snapshot(self.worktree)
        state["active"] = {"name": name, "kind": kind, "argv": argv,
                           "before": before, "started_at": time.time()}
        state["status"] = "running" if kind == "agent" else "checking"
        self.save()  # Pending intent survives even a crash in the spawn window.
        launch(self.directory, name, argv, self.worktree,
               Path(self.worktree) / "target" / "agent-harness", timeout, self.lock_fd)
        return self.finish()

    def finish(self):
        state = self.state
        active = state["active"]
        name = active["name"]
        result_path = self.directory / (name + ".result.json")
        if not result_path.exists():
            self.stop("blocked_process", "missing guardian result; verify old processes stopped before recovery")
        result = read(result_path)
        after = snapshot(self.worktree)
        record = dict(active, result=result, after=after,
                      stdout=str(self.directory / (name + ".jsonl")),
                      stderr=str(self.directory / (name + ".stderr")))
        state.setdefault("history", []).append(record)
        if not result["stopped"]:
            self.stop("blocked_process", "previous process group not confirmed stopped")
        if active["kind"] == "agent":
            event = events(record["stdout"], record["stderr"] if result["code"] != 0 and Path(record["stderr"]).exists() else None)
            record["events"] = event
            state["session_id"] = event["session_id"]
            state["handoff"] = event["handoff"]
            state["policy"] = policy(result, event, after["fingerprint"], state.get("policy", {}),
                                     state["attempts"], state["max_attempts"])
            state["status"] = state["policy"]["status"]
        else:
            state["status"] = "checked"
        state["active"] = None
        self.save()
        return record

    def checks(self):
        state = self.state
        fingerprint = snapshot(self.worktree)["fingerprint"]
        if state.get("validated_fingerprint") == fingerprint:
            return True
        for argv in state["checks"]:
            # Recover a completed check without repeating it after a crash.
            reusable = any(record["kind"] == "check" and record["argv"] == argv
                           and record["before"]["fingerprint"] == fingerprint
                           and record["after"]["fingerprint"] == fingerprint
                           and record["result"]["code"] == 0 and record["result"]["reason"] == "exit"
                           and record["result"]["stopped"] for record in state.get("history", []))
            if reusable:
                continue
            record = self.execute("check", argv, state["check_timeout"])
            if (record["result"]["code"] != 0 or record["result"]["reason"] != "exit"
                    or record["after"]["fingerprint"] != fingerprint):
                state["status"] = "checks_failed_or_changed"
                self.save()
                return False
        state["validated_fingerprint"] = fingerprint
        state["status"] = "local_validated"
        self.save()
        return True

    def prompt(self):
        state = self.state
        last = [record for record in state.get("history", []) if record["kind"] == "check"][-1:]
        handoff = ("" if state.get("policy", {}).get("reason") == "context_exhausted" else
                   "\nHandoff (unverified reference only):\n" + state.get("handoff", "")[-4000:])
        return ("Read current AGENTS.md and relevant nested instructions. Preserve all existing changes. "
                "Do not push, create a PR, merge, release, or bypass checks. Work only in this worktree.\n"
                "Original request and completion conditions:\n" + state["request"] + "\n" + state["conditions"]
                + "\nActual Git changes:\n" + state.get("change_summary", "") + "\n" + state.get("actual_changes", "")
                + "\nRead the actual local diff before editing; the supervisor inspected it.\n"
                + "Last check/run evidence:\n" + str([{"argv": r["argv"][:1], "result": r["result"],
                                                       "fingerprint": r["after"]["fingerprint"]} for r in last])
                + "\nNext step:\n" + state["next_step"] + handoff)

    def step(self, accept_changes=False):
        self.reconcile(accept_changes)
        state = self.state
        if state.get("pr"):
            if not self.checks():
                return
            state.update(ci=None, status="waiting_ci")
            self.save()
            state["ci"] = self.github.ci(state["pr"]["number"])
            same = state["ci"]["head_sha"] == snapshot(self.worktree)["head"]
            clean = not git(self.worktree, "status", "--porcelain")
            state["status"] = "verified" if state["ci"]["verified"] and same and clean else "waiting_ci"
            self.save()
            return
        decision = state.get("policy", {})
        if decision.get("status") in ("retry", "waiting_usage"):
            if decision.get("retry_at") is None or time.time() < decision["retry_at"]:
                state["status"] = decision["status"]
                self.save()
                return
        if state["status"] in ("agent_exited", "checked", "local_validated", "checks_failed_or_changed"):
            self.checks()
            return
        if state["status"].startswith(("blocked", "stopped")):
            return
        if state["attempts"] >= state["max_attempts"]:
            self.stop("stopped_attempt_limit", "maximum agent attempts reached")
        state["attempts"] += 1
        self.execute("agent", [*state["agent"], "exec", "--json", "--sandbox", "workspace-write",
                               "--cd", self.worktree, self.prompt()], state["agent_timeout"])
        if state["status"] == "agent_exited":
            self.checks()
