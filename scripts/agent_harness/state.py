"""Local, atomic supervisor records; Git remains the source of truth."""

import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile


def command(argv, cwd, **kwargs):
    return subprocess.run(argv, cwd=cwd, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, **kwargs).stdout


def git(cwd, *args):
    return command(["git", *args], cwd).decode("utf-8", "surrogateescape").strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def atomic(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix=".checkpoint-")
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(value, stream, ensure_ascii=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def read(path):
    return json.loads(Path(path).read_text())


def git_dir(worktree):
    return Path(git(worktree, "rev-parse", "--absolute-git-dir"))


@contextlib.contextmanager
def lock(worktree):
    # A guardian inherits this open-file description. Never LOCK_UN: closing the
    # parent's descriptor must leave the guardian's lock intact after a crash.
    with (git_dir(worktree) / "agent-harness.lock").open("a+") as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise RuntimeError("another supervisor/guardian owns this worktree") from error
        yield stream.fileno()


def snapshot(worktree):
    worktree = Path(worktree)
    tracked = command(["git", "ls-files", "--stage", "-z"], worktree)
    if any(row.startswith(b"160000 ") for row in tracked.split(b"\0")):
        raise RuntimeError("submodules need a separate recovery policy")
    unknown = command(["git", "ls-files", "--others", "--exclude-standard", "-z"], worktree)
    files = []
    for raw in sorted(filter(None, unknown.split(b"\0"))):
        name = os.fsdecode(raw)
        path = worktree / name
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            content = os.fsencode(os.readlink(path))
        elif stat.S_ISREG(mode):
            content = path.read_bytes()
        else:
            raise RuntimeError(f"unsupported untracked file: {name!r}")
        files.append([name, mode, digest(content)])
    result = {
        "head": git(worktree, "rev-parse", "HEAD"),
        "branch": git(worktree, "rev-parse", "--abbrev-ref", "HEAD"),
        "staged": digest(command(["git", "diff", "--cached", "--binary", "--no-ext-diff", "--no-textconv"], worktree)),
        "unstaged": digest(command(["git", "diff", "--binary", "--no-ext-diff", "--no-textconv"], worktree)),
        "untracked": digest(json.dumps(files, ensure_ascii=True).encode()),
        "untracked_files": files,
        "index": digest(tracked),
    }
    result["fingerprint"] = digest(json.dumps(result, sort_keys=True).encode())
    return result


def conflicts(worktree):
    directory = git_dir(worktree)
    markers = [name for name in ("MERGE_HEAD", "rebase-merge", "rebase-apply",
                                "CHERRY_PICK_HEAD", "REVERT_HEAD", "sequencer", "index.lock")
               if (directory / name).exists()]
    unresolved = git(worktree, "diff", "--name-only", "--diff-filter=U")
    return {"operations": markers, "unmerged": unresolved} if markers or unresolved else None


def inspect(worktree, directory):
    """Retain patches losslessly; re-encode strings with UTF-8/surrogateescape."""
    directory = Path(directory)
    patch = command(["git", "diff", "HEAD", "--binary", "--no-ext-diff", "--no-textconv"], worktree)
    staged = command(["git", "diff", "--cached", "--binary", "--no-ext-diff", "--no-textconv"], worktree)
    atomic(directory / "diff.json", {"working_patch": patch.decode("utf-8", "surrogateescape"),
                                      "staged_patch": staged.decode("utf-8", "surrogateescape")})
    return git(worktree, "status", "--short", "--untracked-files=all")
