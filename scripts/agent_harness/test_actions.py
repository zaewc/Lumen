import unittest

from actions import actions_evidence, checkout_shas


class ActionsTests(unittest.TestCase):
    head = "a" * 40
    base = "b" * 40
    merge = "c" * 40

    def fixture(self):
        head, base, merge = self.head, self.base, self.merge
        class GitHub:
            repository = "owner/repo"
            def api(self, path):
                return {"event": "pull_request", "head_sha": head, "pull_requests": [
                    {"number": 3, "head": {"sha": head}, "base": {"sha": base}}]}
            def job_log(self, job_id):
                return "2026-11-08T08:51:50Z [command]/usr/bin/git log -1 --format=%H\n2026-11-08T08:51:51Z " + merge
        pr = {"number": 3, "head": {"sha": head}, "base": {"sha": base}, "merge_commit_sha": merge}
        check = {"link": "https://github.com/owner/repo/actions/runs/1/job/2"}
        return GitHub(), pr, check

    def test_status_on_head_can_test_merge_sha(self):
        github, pr, check = self.fixture()
        evidence = actions_evidence(github, check, pr, self.head)
        self.assertEqual(evidence["target_sha"], self.merge)
        self.assertEqual(evidence["source"], "checkout-log")
        pr["head"]["sha"] = "d" * 40
        with self.assertRaisesRegex(RuntimeError, "different PR head"):
            actions_evidence(github, check, pr, self.head)

    def test_stale_checkout_or_base_is_rejected(self):
        github, pr, check = self.fixture()
        pr["merge_commit_sha"] = "d" * 40
        with self.assertRaisesRegex(RuntimeError, "checkout differs"):
            actions_evidence(github, check, pr, self.head)
        pr["base"]["sha"] = "e" * 40
        with self.assertRaisesRegex(RuntimeError, "stale PR base"):
            actions_evidence(github, check, pr, self.head)

    def test_metadata_job_does_not_claim_checkout_and_ansi_is_stripped(self):
        github, pr, check = self.fixture()
        github.job_log = lambda job_id: "PR title accepted"
        evidence = actions_evidence(github, check, pr, self.head)
        self.assertIsNone(evidence["target_sha"])
        self.assertEqual(evidence["source"], "event-without-checkout")
        self.assertEqual(checkout_shas("[command]git log -1 --format=%H\nT \x1b[32m" + self.merge + "\x1b[0m"), [self.merge])
