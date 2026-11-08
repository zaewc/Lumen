"""Separate a check's attached SHA from the SHA actually checked out in CI."""

import re

from state import command


def checkout_shas(log):
    result = []
    pending = False
    for line in log.splitlines():
        line = re.sub(r"\x1b\[[0-9;]*m", "", line)
        if "[command]" in line and "git log -1 --format=%H" in line:
            pending = True
        elif pending:
            match = re.search(r"\s([0-9a-f]{40,64})\s*$", line)
            if match:
                result.append(match[1])
            pending = False
    return result


def actions_evidence(github, check, pr, status_sha):
    prefix = f"https://github.com/{github.repository}/actions/runs/"
    link = check.get("link") or ""
    if not link.startswith(prefix):
        return {"target_sha": status_sha, "source": "commit-status"}
    match = re.fullmatch(r"(\d+)(?:/attempts/\d+)?/job/(\d+)", link[len(prefix):])
    if not match:
        raise RuntimeError("unrecognized Actions job URL; cannot prove tested SHA")
    run_id, job_id = match.groups()
    run = github.api(f"actions/runs/{run_id}")
    head, base, merge = pr["head"]["sha"], pr["base"]["sha"], pr["merge_commit_sha"]
    if run["head_sha"] != head:
        raise RuntimeError("Actions run belongs to a different PR head")
    if run["event"] == "pull_request":
        linked = [item for item in run["pull_requests"] if item["number"] == pr["number"]]
        if not linked or any(item["head"]["sha"] != head or item["base"]["sha"] != base for item in linked):
            raise RuntimeError("Actions run belongs to a stale PR base/head")
        event_sha = merge
    elif run["event"] == "push":
        event_sha = head
    else:
        raise RuntimeError("Actions event needs an explicit target-SHA adapter")
    log = github.job_log(job_id)
    shas = checkout_shas(log)
    if any(sha not in (head, merge) for sha in shas):
        raise RuntimeError("CI checkout differs from current head/test merge")
    return {"target_sha": shas[-1] if shas else None, "checkout_shas": shas,
            "event_sha": event_sha, "run_id": run_id, "job_id": job_id,
            "source": "checkout-log" if shas else "event-without-checkout"}


def job_log(github, job_id):
    # gh normally rejects terminal escape bytes in raw logs. Capture privately,
    # strip only ANSI colours for parsing, and never display log text as commands.
    return command(["gh", "api", "--allow-escape-sequences",
                    f"repos/{github.repository}/actions/jobs/{job_id}/logs"],
                   github.cwd, timeout=60).decode("utf-8", "replace")
