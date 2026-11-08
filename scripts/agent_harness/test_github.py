from copy import deepcopy
from pathlib import Path
from unittest.mock import patch

from github import GitHub, publish
from state import git, snapshot
from test_state import Repository


class FakeGitHub(GitHub):
    def __init__(self):
        super().__init__("owner/repo", None)
        self.pr = {"number": 3, "state": "open", "merge_commit_sha": "merge",
                   "head": {"sha": "head", "ref": "feat/task", "repo": {"full_name": "owner/repo"}},
                   "base": {"sha": "base", "ref": "develop"}}
        self.required = [{"name": "test", "bucket": "pass", "state": "SUCCESS", "link": "job"}]
        self.target = "merge"
        self.created = False

    def api(self, path):
        if path.startswith("pulls/"):
            return deepcopy(self.pr)
        return {"parents": [{"sha": "head"}, {"sha": "base"}]}

    def gh(self, *args, **kwargs):
        if args[1] == "checks":
            return self.required
        return dict(headRefOid=self.pr["head"]["sha"], baseRefOid="base", mergeStateStatus="CLEAN")

    def checks_at(self, sha):
        if sha != self.target:
            return [], []
        return [{"name": "test", "head_sha": sha, "html_url": "job", "status": "completed",
                 "conclusion": "success"}], []

    def pages(self, path, field=None):
        return [deepcopy(self.pr)] if self.created else []


class GitHubTests(Repository):
    def test_current_required_checks_include_head_and_actual_merge_target(self):
        gh = FakeGitHub()
        result = gh.ci(3)
        self.assertTrue(result["verified"])
        self.assertEqual((result["head_sha"], result["target_sha"]), ("head", "merge"))
        gh.pr["head"]["sha"] = "new-head"
        self.assertFalse(gh.ci(3)["verified"])
        gh.pr["head"]["sha"] = "head"
        gh.target = "old-head"
        self.assertFalse(gh.ci(3)["verified"])
        gh.target = "merge"
        gh.required = []
        self.assertFalse(gh.ci(3)["verified"])

    def test_discovery_checks_repository_branch_and_base(self):
        gh = FakeGitHub()
        gh.created = True
        self.assertEqual(gh.discover("feat/task", "develop")["number"], 3)
        gh.pr["head"]["repo"]["full_name"] = "fork/repo"
        self.assertIsNone(gh.discover("feat/task", "develop"))

    def test_pr_created_before_supervisor_crash_is_reused(self):
        gh = FakeGitHub()
        state = {"worktree": str(self.root), "branch": "feat/fixture", "base": "develop",
                 "publish_allowed": True, "repository": "owner/repo",
                 "validated_fingerprint": snapshot(self.root)["fingerprint"]}
        gh.pr["head"]["ref"] = state["branch"]
        head = git(self.root, "rev-parse", "HEAD")
        gh.pr["head"]["sha"] = head
        creates = []
        def action(argv, cwd):
            creates.append(argv)
            gh.created = True
            raise SystemExit("simulate SIGKILL immediately after server created PR")
        with patch("github.remote_head", return_value=head), patch("github.git", wraps=git) as g, \
                patch("github.command", side_effect=action):
            def run_git(cwd, *args):
                return "https://github.com/owner/repo.git" if args[:2] == ("remote", "get-url") else git(cwd, *args)
            g.side_effect = run_git
            with self.assertRaises(SystemExit):
                publish(state, lambda: None, gh, Path("body.md"))
            self.assertNotIn("pr", state)
            publish(state, lambda: None, gh, Path("body.md"))
        self.assertEqual(len(creates), 1)
        self.assertEqual(state["pr"]["number"], 3)

    def test_publish_is_opt_in_and_never_overwrites_changed_remote(self):
        state = {"publish_allowed": False}
        with self.assertRaisesRegex(RuntimeError, "not authorized"):
            publish(state, lambda: None, FakeGitHub(), Path("body.md"))
        state = {"publish_allowed": True, "worktree": str(self.root), "branch": "feat/fixture",
                 "base": "develop", "repository": "owner/repo",
                 "validated_fingerprint": snapshot(self.root)["fingerprint"],
                 "publication": {"head": git(self.root, "rev-parse", "HEAD"), "expected_remote": "old"}}
        def run_git(cwd, *args):
            return "https://github.com/owner/repo.git" if args[:2] == ("remote", "get-url") else git(cwd, *args)
        with patch("github.remote_head", return_value="unexpected-remote"), \
                patch("github.git", side_effect=run_git), patch("github.command") as mutation:
            with self.assertRaisesRegex(RuntimeError, "remote changed"):
                publish(state, lambda: None, FakeGitHub(), Path("body.md"))
            mutation.assert_not_called()
