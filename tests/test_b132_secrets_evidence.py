"""Focused B-132 contract and mutation tests.

These tests intentionally stay stdlib-only so the evidence controls can be
checked on the same minimal runner image that executes the workflows.
"""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCANNER_WORKFLOW = ROOT / ".github" / "workflows" / "secrets-drift.yml"
WATCHDOG_WORKFLOW = ROOT / ".github" / "workflows" / "secrets-drift-evidence-watchdog.yml"
EVIDENCE_SCRIPT = ROOT / "scripts" / "check_secrets_drift_evidence.py"


def _load_evidence_module():
    spec = importlib.util.spec_from_file_location("check_secrets_drift_evidence", EVIDENCE_SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class B132WorkflowContractTest(unittest.TestCase):
    def setUp(self) -> None:
        self.scanner = SCANNER_WORKFLOW.read_text(encoding="utf-8")
        self.watchdog = WATCHDOG_WORKFLOW.read_text(encoding="utf-8")

    def test_scanner_and_watchdog_use_hosted_linux_for_credentialless_metadata(self) -> None:
        self.assertIn("cron: '0 4 * * *'", self.scanner)
        self.assertIn("runs-on: ubuntu-latest", self.scanner)
        self.assertIn("runs-on: ubuntu-latest", self.watchdog)
        self.assertNotIn("runs-on: corelink", self.scanner)
        self.assertNotIn("runs-on: corelink", self.watchdog)
        self.assertNotIn("HOSTED_ACTIONS_AVAILABLE", self.scanner)
        self.assertNotIn("HOSTED_ACTIONS_AVAILABLE", self.watchdog)
        self.assertNotIn("secrets.", self.scanner)
        self.assertNotIn("secrets.", self.watchdog)
        self.assertIn("persist-credentials: false", self.scanner)
        self.assertIn("persist-credentials: false", self.watchdog)

    def test_scanner_trigger_census_covers_every_secret_input_surface(self) -> None:
        # The Python scanner reads these populations; a path omission would let
        # a new credential land without a PR gate (the daily run is not enough
        # for timely review).
        expected = {
            "Cargo.toml",
            "Cargo.lock",
            "crates/**/*.rs",
            "apps/**/*.ts",
            "apps/**/*.tsx",
            "worker/**/*.ts",
            "**/wrangler.toml",
            ".github/workflows/**",
            "docs/internal/secrets-checklist.md",
            "scripts/validate_secrets_matrix.py",
            "scripts/secrets-checklist-verify.sh",
            "scripts/check-env-contract.py",
            "worker/src/durable_object.ts",
            "crates/corelink-container/src/**",
        }
        for path in expected:
            self.assertIn(f"      - '{path}'", self.scanner, path)

    def test_scanner_artifact_is_retained_and_missing_files_fail(self) -> None:
        self.assertIn("name: secrets-drift-report", self.scanner)
        self.assertIn("retention-days: 90", self.scanner)
        self.assertIn("if-no-files-found: error", self.scanner)
        self.assertIn("test -s artifacts/secrets-drift-report.json", self.scanner)
        self.assertIn("path: artifacts/secrets-drift-report.json", self.scanner)
        self.assertIn("name: secrets-drift-run-manifest", self.scanner)
        self.assertIn("path: artifacts/secrets-drift-run.json", self.scanner)
        self.assertIn("refusing evidence upload", self.scanner)

    def test_both_failure_paths_raise_a_stable_issue(self) -> None:
        marker = "<!-- secrets-drift-evidence-gap -->"
        self.assertIn(marker, self.watchdog)
        self.assertIn("issues: write", self.watchdog)
        self.assertNotIn("issues: write", self.scanner)
        self.assertNotIn("github-script", self.scanner)
        self.assertIn("if: steps.evidence.outcome == 'failure'", self.watchdog)
        self.assertIn("Fail closed on evidence gap", self.watchdog)
        self.assertIn("runs-on: ubuntu-latest", self.watchdog)
        self.assertNotIn("vars.HOSTED_ACTIONS_AVAILABLE", self.watchdog)

    def test_mutations_remove_each_load_bearing_control(self) -> None:
        def assert_hosted(text: str) -> None:
            self.assertIn("runs-on: ubuntu-latest", text)
            self.assertNotIn("runs-on: corelink", text)

        assert_hosted(self.scanner)
        assert_hosted(self.watchdog)
        hosted_mutant = self.scanner.replace("runs-on: ubuntu-latest", "runs-on: corelink")
        with self.assertRaises(AssertionError):
            assert_hosted(hosted_mutant)
        watchdog_mutant = self.watchdog.replace("runs-on: ubuntu-latest", "runs-on: corelink")
        with self.assertRaises(AssertionError):
            assert_hosted(watchdog_mutant)
        retention_mutant = self.scanner.replace("retention-days: 90", "retention-days: 1")
        self.assertNotIn("retention-days: 90", retention_mutant)
        upload_mutant = self.scanner.replace("if-no-files-found: error", "if-no-files-found: warn")
        self.assertNotIn("if-no-files-found: error", upload_mutant)
        alarm_mutant = self.watchdog.replace("<!-- secrets-drift-evidence-gap -->", "")
        self.assertNotIn("<!-- secrets-drift-evidence-gap -->", alarm_mutant)
        report_mutant = self.scanner.replace(
            "test -s artifacts/secrets-drift-report.json",
            "test -s artifacts/secrets-drift-run.json",
        )
        self.assertNotIn("test -s artifacts/secrets-drift-report.json", report_mutant)
        self.assertIn("test -s artifacts/secrets-drift-run.json", report_mutant)
        self.assertIn("`- Status: ${report.status}`", self.watchdog)
        self.assertNotIn("`- Status: `${report.status}``", self.watchdog)


class B132EvidenceInspectionTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.module = _load_evidence_module()
        cls.now = datetime(2026, 9, 5, 5, tzinfo=timezone.utc)
        cls.run_payload = {
            "id": 42,
            "run_number": 7,
            "event": "schedule",
            "head_branch": "main",
            "status": "completed",
            "conclusion": "success",
            "created_at": "2026-09-05T04:00:00Z",
            "html_url": "https://github.example/actions/runs/42",
        }

    def inspect(self, run=None, artifacts=None, now=None):
        return self.module.inspect_evidence(
            {"workflow_runs": [run or self.run_payload]},
            {"artifacts": artifacts} if artifacts is not None else {"artifacts": []},
            now=now or self.now,
            max_age=timedelta(hours=26),
        )

    def test_success_requires_the_exact_unexpired_report_artifact(self) -> None:
        healthy, reason, metadata = self.inspect(artifacts=[{"name": "secrets-drift-report", "expired": False}])
        self.assertTrue(healthy)
        self.assertIn("retained", reason)
        self.assertEqual(metadata["artifact_name"], "secrets-drift-report")

    def test_absence_failure_and_expiry_are_gaps(self) -> None:
        for artifacts in ([], [{"name": "other-report", "expired": False}], [{"name": "secrets-drift-report", "expired": True}]):
            with self.subTest(artifacts=artifacts):
                healthy, _, _ = self.inspect(artifacts=artifacts)
                self.assertFalse(healthy)
        failed = dict(self.run_payload, conclusion="failure")
        self.assertFalse(self.inspect(run=failed, artifacts=[{"name": "secrets-drift-report", "expired": False}])[0])
        self.assertFalse(self.inspect(artifacts=[{"name": "secrets-drift-report"}])[0])

    def test_no_run_wrong_event_and_stale_run_are_gaps(self) -> None:
        self.assertFalse(self.module.inspect_evidence(
            {"workflow_runs": []}, {"artifacts": []}, now=self.now, max_age=timedelta(hours=26)
        )[0])
        wrong_event = dict(self.run_payload, event="workflow_dispatch")
        self.assertFalse(self.inspect(run=wrong_event, artifacts=[])[0])
        wrong_branch = dict(self.run_payload, head_branch="release")
        self.assertFalse(self.inspect(run=wrong_branch, artifacts=[])[0])
        stale = dict(self.run_payload, created_at="2026-09-03T00:00:00Z")
        self.assertFalse(self.inspect(run=stale, artifacts=[])[0])

    def test_latest_failed_run_cannot_be_hidden_by_an_older_green_run(self) -> None:
        latest_failed = dict(
            self.run_payload,
            id=43,
            created_at="2026-09-05T04:01:00Z",
            conclusion="failure",
        )
        healthy, _, _ = self.module.inspect_evidence(
            {"workflow_runs": [self.run_payload, latest_failed]},
            {"artifacts": [{"name": "secrets-drift-report", "expired": False}]},
            now=self.now,
            max_age=timedelta(hours=26),
        )
        self.assertFalse(healthy)

    def test_api_unknown_still_leaves_a_retained_diagnostic_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "watchdog.json"
            rc = self.module.main(["--repo", "HuGR-Labs/corelink-server", "--report", str(report)])
            self.assertEqual(rc, 2)
            payload = json.loads(report.read_text(encoding="utf-8"))
            self.assertEqual(payload["status"], "watchdog_error")
            self.assertIn("reason", payload)


if __name__ == "__main__":
    unittest.main()
