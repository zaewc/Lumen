import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from state import atomic, conflicts, git, lock, read, snapshot


class Repository(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", "-b", "feat/fixture", str(self.root)], check=True)
        (self.root / "tracked").write_text("base\n")
        git(self.root, "add", "tracked")
        # Identity is per invocation in isolated fixtures, never repository config.
        git(self.root, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "commit", "-qm", "test: fixture")


class StateTests(Repository):
    def test_fingerprints_preserve_all_three_change_layers(self):
        clean = snapshot(self.root)
        (self.root / "tracked").write_text("staged\n")
        git(self.root, "add", "tracked")
        staged = snapshot(self.root)
        (self.root / "tracked").write_text("unstaged\n")
        (self.root / "new\nfile").write_bytes(b"\x00untracked")
        os.symlink("missing", self.root / "link")
        mixed = snapshot(self.root)
        self.assertNotEqual(clean["staged"], staged["staged"])
        self.assertEqual(staged["staged"], mixed["staged"])
        self.assertNotEqual(staged["unstaged"], mixed["unstaged"])
        self.assertNotEqual(staged["untracked"], mixed["untracked"])
        self.assertEqual(git(self.root, "show", ":tracked"), "staged")
        self.assertEqual(snapshot(self.root), mixed)

    def test_atomic_checkpoint_and_exclusive_canonical_lock(self):
        path = self.root / ".git" / "record.json"
        atomic(path, {"phase": "running"})
        self.assertEqual(read(path), {"phase": "running"})
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        with lock(self.root):
            with self.assertRaisesRegex(RuntimeError, "owns this worktree"):
                with lock(self.root / "."):
                    self.fail("second writer acquired lock")
        with lock(self.root):
            pass

    def test_interrupted_git_operations_are_reported_without_modification(self):
        marker = self.root / ".git" / "MERGE_HEAD"
        marker.write_text(git(self.root, "rev-parse", "HEAD"))
        before = snapshot(self.root)
        self.assertEqual(conflicts(self.root)["operations"], ["MERGE_HEAD"])
        self.assertEqual(before, snapshot(self.root))


if __name__ == "__main__":
    unittest.main()
