"""Idempotent publishing and current GitHub-required-check evidence."""

import json
import subprocess
from urllib.parse import urlencode

from actions import actions_evidence, job_log
from state import command, git, snapshot


class GitHub:
    def __init__(self, repository, cwd):
        self.repository = repository
        self.cwd = cwd

    def gh(self, *args, allowed=(0,)):
        result = subprocess.run(["gh", *args], cwd=self.cwd, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True, timeout=60)
        if result.returncode not in allowed:
            raise RuntimeError(f"gh {args[0]} failed ({result.returncode}): {result.stderr.strip()}")
        return json.loads(result.stdout)

    def api(self, path):
        return self.gh("api", f"repos/{self.repository}/{path}")

    def pages(self, path, field=None):
        pages = self.gh("api", "--paginate", "--slurp", f"repos/{self.repository}/{path}")
        return [item for page in pages for item in (page[field] if field else page)]

    def discover(self, branch, base):
        owner = self.repository.split("/")[0]
        query = urlencode(dict(state="all", head=f"{owner}:{branch}", base=base, per_page=100))
        matches = [pr for pr in self.pages("pulls?" + query)
                   if pr["head"]["ref"] == branch and pr["base"]["ref"] == base
                   and pr["head"].get("repo")
                   and pr["head"]["repo"]["full_name"].lower() == self.repository.lower()]
        if len(matches) > 1:
            raise RuntimeError("multiple historical PRs for this head/base; choose a fresh issue branch")
        return matches[0] if matches else None

    def checks_at(self, sha):
        checks = self.pages(f"commits/{sha}/check-runs?per_page=100", "check_runs")
        statuses = self.pages(f"commits/{sha}/statuses?per_page=100")
        return checks, statuses

    def job_log(self, job_id):
        return job_log(self, job_id)

    def ci(self, number):
        pr = self.api(f"pulls/{number}")
        head, base, merge = pr["head"]["sha"], pr["base"]["sha"], pr["merge_commit_sha"]
        required = self.gh("pr", "checks", str(number), "--repo", self.repository,
                           "--required", "--json", "name,bucket,link,state", allowed=(0, 1, 8))
        target = head
        records = self.checks_at(head)
        if merge and pr["state"] == "open":
            parents = self.api(f"git/commits/{merge}")["parents"]
            if {head, base} != {parent["sha"] for parent in parents}:
                return {"verified": False, "head_sha": head, "reason": "stale test merge"}
            merged_records = self.checks_at(merge)
            if any(merged_records):
                target, records = merge, merged_records
        evidence = []
        for check in required:
            runs = [run for run in records[0] if run["name"] == check["name"]
                    and run["head_sha"] == target
                    and check["link"] in (run.get("html_url"), run.get("details_url"))]
            statuses = [status for status in records[1] if status["context"] == check["name"]
                        and status.get("target_url") == check["link"]]
            if check["bucket"] not in ("pass", "skipping") or not (runs or statuses):
                break
            if any(run["status"] != "completed" or run["conclusion"] not in
                   ("success", "neutral", "skipped") for run in runs):
                break
            if statuses and statuses[0]["state"] != "success":
                break
            evidence.append(dict(check, status_sha=target, **actions_evidence(self, check, pr, target)))
        current = self.api(f"pulls/{number}")
        gate = self.gh("pr", "view", str(number), "--repo", self.repository,
                       "--json", "headRefOid,baseRefOid,mergeStateStatus")
        stable = (current["head"]["sha"], current["base"]["sha"], current["merge_commit_sha"]) == (head, base, merge)
        verified = bool(required) and len(evidence) == len(required) and stable
        # GitHub's gate also accounts for expected check-app identities/rulesets.
        verified = verified and gate == dict(headRefOid=head, baseRefOid=base, mergeStateStatus="CLEAN")
        targets = sorted({item["target_sha"] for item in evidence if item["target_sha"]})
        return {"verified": bool(verified), "head_sha": head, "base_sha": base,
                "status_sha": target, "target_sha": targets[0] if len(targets) == 1 else None,
                "target_shas": targets, "merge_sha": merge, "required": required,
                "evidence": evidence, "gate": gate, "pr_state": pr["state"]}


def remote_head(worktree, branch):
    output = git(worktree, "ls-remote", "--heads", "origin", f"refs/heads/{branch}")
    return output.split()[0] if output else None


def publish(state, save, github, body_file):
    if not state["publish_allowed"]:
        raise RuntimeError("publishing was not authorized for this task")
    worktree, branch = state["worktree"], state["branch"]
    current = snapshot(worktree)
    if git(worktree, "status", "--porcelain") or current["fingerprint"] != state["validated_fingerprint"]:
        raise RuntimeError("publishing requires a clean HEAD with current local checks")
    if branch in ("main", "develop") or state["base"] != "develop":
        raise RuntimeError("only issue branches into develop may be published")
    url = git(worktree, "remote", "get-url", "--push", "origin")
    repo = state["repository"]
    if url.removesuffix(".git") not in (f"https://github.com/{repo}", f"git@github.com:{repo}"):
        raise RuntimeError("origin push URL does not match the checkpoint repository")
    pr = github.discover(branch, state["base"])
    if pr and pr["state"] != "open":
        raise RuntimeError("existing PR is closed; preserve it and use a new task branch")
    remote = remote_head(worktree, branch)
    intent = state.setdefault("publication", {"head": current["head"], "expected_remote": remote})
    if intent["head"] != current["head"]:
        intent = state["publication"] = {"head": current["head"], "expected_remote": remote}
    save()  # Durable intent before either remote side effect.
    if remote != current["head"]:
        if remote != intent["expected_remote"]:
            raise RuntimeError("remote changed since push intent; refusing to overwrite")
        if remote:
            git(worktree, "fetch", "origin", f"refs/heads/{branch}")
            git(worktree, "merge-base", "--is-ancestor", remote, current["head"])
        command(["git", "push", "origin", f"HEAD:refs/heads/{branch}"], worktree)
    intent["pushed_sha"] = remote_head(worktree, branch)
    if intent["pushed_sha"] != current["head"]:
        raise RuntimeError("remote SHA changed during push")
    save()
    # Repeat discovery after push and on every resume, including a crash directly
    # after GitHub created a PR but before its number reached the checkpoint.
    pr = github.discover(branch, state["base"])
    if not pr:
        command(["gh", "pr", "create", "--repo", repo, "--head", branch,
                 "--base", state["base"], "--title", git(worktree, "log", "-1", "--format=%s"),
                 "--body-file", str(body_file)], worktree)
        pr = github.discover(branch, state["base"])
    if not pr:
        raise RuntimeError("PR creation outcome unknown; resume to rediscover")
    state["pr"] = {"number": pr["number"], "head_sha": pr["head"]["sha"]}
    state["status"] = "waiting_ci"
    save()
