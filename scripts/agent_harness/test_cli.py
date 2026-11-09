from pathlib import Path
import tempfile
from unittest.mock import patch

from cli import main, parser
from prepare import prepare
from state import git, read, snapshot
from test_state import Repository


class CLIRepository(Repository):
    def test_init_preserves_caller_and_reuses_issue_worktree_checkpoint(self):
        remote = self.root / ".git" / "remote.git"
        git(self.root, "init", "--bare", str(remote))
        git(self.root, "remote", "add", "origin", str(remote))
        git(self.root, "push", "-q", "origin", "HEAD:refs/heads/develop")
        (self.root / "tracked").write_text("caller's staged changes")
        git(self.root, "add", "tracked")
        (self.root / "tracked").write_text("caller's unstaged changes")
        (self.root / "untracked").write_text("preserve me")
        before = snapshot(self.root)
        request = self.root / ".git" / "request.txt"
        conditions = self.root / ".git" / "conditions.txt"
        request.write_text("original request")
        conditions.write_text("acceptance conditions")
        extra = tempfile.TemporaryDirectory()
        self.addCleanup(extra.cleanup)
        destination = Path(extra.name) / "issue-42"
        args = parser().parse_args(["init", "--source", str(self.root), "--repository", "owner/repo",
                                    "--issue", "42", "--worktree", str(destination), "--request", str(request),
                                    "--conditions", str(conditions), "--check", "python3 -c 'print(42)'"])
        class GitHubFixture:
            def __init__(self, *args):
                pass
            def gh(self, *args):
                return {"number": 42, "title": "fixture"} if args[0] == "issue" else []
            def discover(self, *args):
                return None
        with patch("prepare.GitHub", GitHubFixture):
            checkpoint = prepare(args)
            state = read(checkpoint)
            self.assertEqual(state["branch"], "feat/issue-42")
            self.assertEqual(state["request"], "original request")
            self.assertFalse(state["publish_allowed"])
            (destination / "tracked").write_text("agent's interrupted changes")
            self.assertEqual(prepare(args), checkpoint)
            self.assertEqual((destination / "tracked").read_text(), "agent's interrupted changes")
        self.assertEqual(snapshot(self.root), before)
        self.assertEqual(main(["status", str(checkpoint)]), 0)

    def test_check_argv_has_no_shell_expansion(self):
        args = parser().parse_args(["init", "--issue", "42", "--worktree", "/tmp/unused",
                                    "--request", "request.txt", "--conditions", "conditions.txt",
                                    "--check", "printf '$HOME; $(not-a-command)'"])
        self.assertEqual(args.check, [["printf", "$HOME; $(not-a-command)"]])
