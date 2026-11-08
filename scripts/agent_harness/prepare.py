"""Create/reuse an issue worktree without modifying the caller's checkout."""

import fcntl
from pathlib import Path
import re
import subprocess

from github import GitHub
from state import atomic, command, git, lock, snapshot


def prepare(args):
    source = Path(args.source).resolve()
    common = Path(git(source, "rev-parse", "--git-common-dir"))
    common = (source / common).resolve()
    directory = common / "agent-harness"
    directory.mkdir(mode=0o700, exist_ok=True)
    with (directory / "init.lock").open("a+") as guard:
        try:
            fcntl.flock(guard, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RuntimeError("another issue is being prepared; retry later") from error
        return initialize(args, source, directory)


def initialize(args, source, directory):
    repository = args.repository or GitHub("", source).gh("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repository):
        raise ValueError("expected GitHub OWNER/REPO")
    github = GitHub(repository, source)
    issue = github.gh("issue", "view", str(args.issue), "--repo", repository, "--json", "number,title")
    prs = github.gh("pr", "list", "--repo", repository, "--state", "open", "--limit", "1000",
                    "--json", "headRefName,baseRefName,closingIssuesReferences,headRepositoryOwner")
    if len(prs) == 1000:
        raise RuntimeError("PR listing truncated; select a branch after manual inspection")
    linked = [pr for pr in prs if any(item["number"] == args.issue for item in pr["closingIssuesReferences"])]
    if len(linked) > 1:
        raise RuntimeError("multiple PRs close this issue; select the task after inspection")
    branch = args.branch or (linked[0]["headRefName"] if linked else f"feat/issue-{args.issue}")
    if not re.fullmatch(r"(?:feat|fix|chore|docs|test|refactor|ci)/[A-Za-z0-9._/-]+", branch):
        raise ValueError("use a type/slug issue branch")
    git(source, "check-ref-format", "--branch", branch)
    if linked and (linked[0]["baseRefName"] != "develop" or
                   linked[0]["headRepositoryOwner"]["login"].lower() != repository.split("/")[0].lower()):
        raise RuntimeError("linked PR must use this repository and develop")
    existing_pr = github.discover(branch, "develop")
    if existing_pr and existing_pr["state"] != "open":
        raise RuntimeError("this branch already has a closed PR; choose a new branch")
    path = directory / "issues" / str(args.issue) / "state.json"
    if path.exists():
        return path  # Never replace an existing request, authorization or checkpoint.
    git(source, "fetch", "origin", "develop")
    base_sha = git(source, "rev-parse", "origin/develop")
    worktrees = command(["git", "worktree", "list", "--porcelain", "-z"], source)
    worktree = None
    for record in worktrees.split(b"\0\0"):
        fields = dict(item.decode().split(" ", 1) for item in record.split(b"\0") if b" " in item)
        if fields.get("branch") == "refs/heads/" + branch:
            worktree = Path(fields["worktree"])
    if worktree is None:
        worktree = Path(args.worktree).resolve()
        exists = subprocess.run(["git", "show-ref", "--verify", "--quiet", "refs/heads/" + branch], cwd=source)
        if exists.returncode not in (0, 1):
            raise RuntimeError("cannot inspect local branch")
        if exists.returncode == 0:
            git(source, "worktree", "add", str(worktree), branch)
        elif existing_pr:
            git(source, "fetch", "origin", f"refs/heads/{branch}:refs/remotes/origin/{branch}")
            git(source, "worktree", "add", "-b", branch, str(worktree), f"origin/{branch}")
        else:
            git(source, "worktree", "add", "-b", branch, str(worktree), base_sha)
    with lock(worktree):
        base_sha = git(worktree, "merge-base", base_sha, "HEAD")
        atomic(path, {
            "version": 1, "issue": issue["number"], "repository": repository,
            "request": Path(args.request).read_text(), "conditions": Path(args.conditions).read_text(),
            "next_step": "Read repository instructions and implement the original request.",
            "worktree": str(worktree.resolve()), "branch": branch, "base": "develop", "base_sha": base_sha,
            "cargo_target_dir": str(worktree.resolve() / "target" / "agent-harness"),
            "snapshot": snapshot(worktree), "checks": args.check, "status": "ready", "active": None,
            "attempts": 0, "max_attempts": args.max_attempts, "agent": ["codex"],
            "agent_timeout": args.timeout, "check_timeout": args.check_timeout,
            "history": [], "publish_allowed": args.allow_publish, "pr": None,
        })
    return path
