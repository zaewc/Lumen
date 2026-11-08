"""Dependency-free local harness entry point (Python 3.9+, macOS/Linux)."""

import argparse
from datetime import datetime
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

from github import GitHub, publish
from prepare import prepare
from state import lock, read
from supervisor import Supervisor


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="action", required=True)
    init = commands.add_parser("init", help="create/reuse an issue worktree and checkpoint")
    init.add_argument("--source", default=".")
    init.add_argument("--repository", help="GitHub OWNER/REPO; default: source's repository")
    init.add_argument("--issue", type=positive, required=True)
    init.add_argument("--worktree", required=True, help="destination if this branch has no worktree")
    init.add_argument("--branch", help="reuse an existing unlinked task branch")
    init.add_argument("--request", required=True, help="original request text file")
    init.add_argument("--conditions", required=True, help="completion conditions text file")
    init.add_argument("--check", action="append", type=shlex.split, required=True,
                      help="trusted targeted check argv; repeatable; no shell expansion")
    init.add_argument("--timeout", type=positive, default=1800)
    init.add_argument("--check-timeout", type=positive, default=900)
    init.add_argument("--max-attempts", type=positive, default=4)
    init.add_argument("--allow-publish", action="store_true", help="explicit authorization for this task")
    for name in ("resume", "check", "publish", "status"):
        command = commands.add_parser(name)
        command.add_argument("checkpoint", type=Path)
        if name != "status":
            command.add_argument("--accept-changes", action="store_true",
                                 help="acknowledge inspected external Git changes")
        if name == "resume":
            command.add_argument("--watch", action="store_true", help="wait for scheduled retries/CI without agents")
            command.add_argument("--retry-at", help="operator-confirmed usage reset: ISO 8601 with timezone")
            command.add_argument("--next-step", help="update instructions after inspecting real changes")
        if name == "publish":
            command.add_argument("--body-file", required=True, type=Path, help="reviewed PR template body")
    return result


def report(path):
    state = read(path)
    print(json.dumps({key: state.get(key) for key in
                      ("issue", "status", "reason", "worktree", "attempts", "policy", "pr", "ci")}, indent=2))
    print(f"checkpoint: {path}")


def main(argv=None):
    args = parser().parse_args(argv)
    if args.action == "init":
        if any(not check for check in args.check):
            raise ValueError("checks must not be empty")
        print(prepare(args))
        return 0
    path = args.checkpoint.resolve()
    if args.action == "status":
        report(path)
        return 0
    state = read(path)
    with lock(state["worktree"]) as fd:
        supervisor = Supervisor(path, GitHub(state["repository"], state["worktree"]), fd)
        if args.action == "resume":
            if args.retry_at:
                reset = datetime.fromisoformat(args.retry_at.replace("Z", "+00:00"))
                if reset.tzinfo is None or supervisor.state.get("policy", {}).get("status") != "waiting_usage":
                    raise ValueError("--retry-at requires waiting_usage and an explicit timezone")
                supervisor.state["policy"]["retry_at"] = reset.timestamp()
            if args.next_step:
                supervisor.state["next_step"] = args.next_step
            # Do not snapshot before reconciliation: that would hide external edits.
            while True:
                supervisor.step(args.accept_changes)
                report(path)
                status = supervisor.state["status"]
                retry_at = supervisor.state.get("policy", {}).get("retry_at")
                if not args.watch or status not in ("retry", "waiting_usage", "waiting_ci"):
                    break
                if status == "waiting_usage" and retry_at is None:
                    break
                delay = max(0.1, min(60, retry_at - time.time())) if status != "waiting_ci" else 30
                time.sleep(delay)
        else:
            supervisor.reconcile(args.accept_changes)
            if not supervisor.checks():
                report(path)
                return 1
            if args.action == "publish":
                publish(supervisor.state, supervisor.save, supervisor.github, args.body_file.resolve())
            report(path)
        return 0 if supervisor.state["status"] in ("local_validated", "verified", "waiting_ci") else 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (RuntimeError, ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"harness: {error}", file=sys.stderr)
        sys.exit(1)
