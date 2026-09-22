import tempfile
import unittest
from pathlib import Path

from scripts.verify_bot_pr_auth import CREATOR_WORKFLOWS, verify, verify_b012_backlog


ROOT = Path(__file__).resolve().parents[1]


class B012BotPrAuthTests(unittest.TestCase):
    def _candidate(self):
        tmp = tempfile.TemporaryDirectory()
        candidate = Path(tmp.name)
        (candidate / ".github/workflows").mkdir(parents=True)
        for name in (*CREATOR_WORKFLOWS, "bot-pr-has-checks.yml"):
            source = ROOT / ".github/workflows" / name
            (candidate / ".github/workflows" / name).write_text(
                source.read_text(encoding="utf-8"), encoding="utf-8"
            )
        return tmp, candidate

    def test_base_workflows_use_dedicated_token_and_gate(self):
        self.assertEqual(verify(ROOT), [])
        self.assertEqual(verify_b012_backlog(ROOT), [])

    def test_each_creator_comment_distinguishes_approval_from_execution(self):
        for name in CREATOR_WORKFLOWS:
            with self.subTest(workflow=name):
                tmp, candidate = self._candidate()
                self.addCleanup(tmp.cleanup)
                path = candidate / ".github/workflows" / name
                text = path.read_text(encoding="utf-8")
                self.assertIn("approval-required", text)
                path.write_text(
                    text.replace("approval-required", "always suppressed", 1),
                    encoding="utf-8",
                )
                self.assertTrue(
                    any("approval-required/completed-job" in error and name in error
                        for error in verify(candidate))
                )

    def test_backlog_rejects_old_zero_check_closure_claim(self):
        with tempfile.TemporaryDirectory() as tmp:
            candidate = Path(tmp)
            text = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
            self.assertIn("zero-job workflow run is not execution evidence", text)
            (candidate / "BACKLOG.md").write_text(
                text.replace(
                    "zero-job workflow run is not execution evidence",
                    "zero-job workflow run is execution evidence",
                    1,
                ),
                encoding="utf-8",
            )
            self.assertTrue(any("zero-job workflow run is not execution evidence" in error
                                for error in verify_b012_backlog(candidate)))

    def test_mutating_pr_step_back_to_github_token_is_detected(self):
        with tempfile.TemporaryDirectory() as tmp:
            candidate = Path(tmp)
            (candidate / ".github/workflows").mkdir(parents=True)
            for name in (*CREATOR_WORKFLOWS, "bot-pr-has-checks.yml"):
                source = ROOT / ".github/workflows" / name
                text = source.read_text(encoding="utf-8")
                if name == "api-reference-sync.yml":
                    text = text.replace(
                        'BOT_APP_TOKEN: ${{ steps.app-token.outputs.token }}',
                        'GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}',
                        1,
                    )
                (candidate / ".github/workflows" / name).write_text(text, encoding="utf-8")
            # The checkout token is intentionally left correct; this mutation
            # exercises the PR-creation credential itself.
            self.assertTrue(any("api-reference-sync.yml" in e for e in verify(candidate)))

    def test_pull_request_creators_are_fork_safe(self):
        for name in ("api-reference-sync.yml", "subprocessors-sync.yml"):
            with self.subTest(workflow=name):
                tmp, candidate = self._candidate()
                self.addCleanup(tmp.cleanup)
                path = candidate / ".github/workflows" / name
                text = path.read_text(encoding="utf-8")
                path.write_text(
                    text.replace(
                        "permissions:\n  contents: read\n",
                        "permissions:\n  contents: read\n  pull-requests: write\n",
                        1,
                    ),
                    encoding="utf-8",
                )
                self.assertTrue(any("pull-requests: write" in e for e in verify(candidate)))

    def test_pull_request_checkout_does_not_persist_credentials(self):
        tmp, candidate = self._candidate()
        self.addCleanup(tmp.cleanup)
        path = candidate / ".github/workflows/api-reference-sync.yml"
        text = path.read_text(encoding="utf-8").replace(
            "          persist-credentials: false\n", "", 1
        )
        path.write_text(text, encoding="utf-8")
        self.assertTrue(any("persist GITHUB_TOKEN" in e for e in verify(candidate)))

    def test_new_sixth_pr_creator_is_rejected(self):
        tmp, candidate = self._candidate()
        self.addCleanup(tmp.cleanup)
        (candidate / ".github/workflows/rogue-auto-pr.yml").write_text(
            "name: rogue\njobs:\n  run:\n    steps:\n      - run: gh pr create --base main\n",
            encoding="utf-8",
        )
        self.assertTrue(any("unexpected active bot-PR creator" in e for e in verify(candidate)))

    def test_pr_create_failure_bypass_is_rejected(self):
        tmp, candidate = self._candidate()
        self.addCleanup(tmp.cleanup)
        path = candidate / ".github/workflows/subprocessors-sync.yml"
        text = path.read_text(encoding="utf-8").replace(
            '--body-file "$BODY_FILE"\n', '--body-file "$BODY_FILE" || true\n', 1
        )
        path.write_text(text, encoding="utf-8")
        self.assertTrue(any("PR creation failure is being swallowed" in e for e in verify(candidate)))

    def test_okf_checkout_auth_bypass_is_rejected(self):
        tmp, candidate = self._candidate()
        self.addCleanup(tmp.cleanup)
        path = candidate / ".github/workflows/okf-autoreconcile.yml"
        text = path.read_text(encoding="utf-8").replace(
            "          token: ${{ steps.app-token.outputs.token }}\n", ""
        )
        path.write_text(text, encoding="utf-8")
        self.assertTrue(any("okf-autoreconcile.yml: checkout" in e for e in verify(candidate)))

    def test_actor_independent_gate_marker_bypass_is_rejected(self):
        tmp, candidate = self._candidate()
        self.addCleanup(tmp.cleanup)
        path = candidate / ".github/workflows/bot-pr-has-checks.yml"
        text = path.read_text(encoding="utf-8").replace(
            '"bot/subprocessors-sync-"', '"bot/subprocessors-sync"', 1
        )
        path.write_text(text, encoding="utf-8")
        self.assertTrue(any("gate missing marker" in e for e in verify(candidate)))

    def test_check_presence_cannot_be_reported_as_success(self):
        tmp, candidate = self._candidate()
        self.addCleanup(tmp.cleanup)
        path = candidate / ".github/workflows/bot-pr-has-checks.yml"
        text = path.read_text(encoding="utf-8").replace(
            'verdict = "present (not proof of success)"',
            'verdict = "gated"',
            1,
        )
        path.write_text(text, encoding="utf-8")
        self.assertTrue(any("count-only verdict" in e for e in verify(candidate)))

    def test_missing_approval_diagnostic_is_rejected(self):
        tmp, candidate = self._candidate()
        self.addCleanup(tmp.cleanup)
        path = candidate / ".github/workflows/bot-pr-has-checks.yml"
        text = path.read_text(encoding="utf-8").replace(
            "Inspect the PR approval banner", "Inspect the PR page", 1
        )
        path.write_text(text, encoding="utf-8")
        self.assertTrue(any("approval/startup diagnostic" in e for e in verify(candidate)))


if __name__ == "__main__":
    unittest.main()
