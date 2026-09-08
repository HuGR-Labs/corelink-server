"""Adversarial tests for the BASE-owned backlog PR gate."""

from __future__ import annotations

import re
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import datetime as dt
from pathlib import Path

from scripts import backlog_verify


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts" / "backlog_verify.py"


class BacklogVerifyTrustBoundaryTests(unittest.TestCase):
    def test_workflow_uses_base_control_and_credentialless_data_checkouts(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "backlog-verify.yml").read_text(encoding="utf-8")
        self.assertIn("pull_request_target:", workflow)
        self.assertNotIn("\n  pull_request:\n", workflow)
        self.assertGreaterEqual(workflow.count("persist-credentials: false"), 2)
        self.assertIn("github.event.pull_request.head.sha || github.sha", workflow)
        self.assertIn("github.event.pull_request.base.sha || github.sha", workflow)
        self.assertNotIn("github.event.pull_request.head.ref", workflow)
        self.assertNotIn("github.event.pull_request.base.ref", workflow)
        self.assertNotIn("GH_TOKEN", workflow)
        self.assertIn("path: _candidate", workflow)
        self.assertIn("path: _base", workflow)
        self.assertIn("working-directory: _base", workflow)
        self.assertIn("--candidate-file", workflow)
        self.assertIn("--trusted-file", workflow)
        self.assertIn("--trusted-semantic", workflow)
        self.assertIn("if: github.event_name == 'push' || github.event_name == 'schedule'", workflow)

    @staticmethod
    def item(item_id: str, *, status: str = "open", verify: str = '"true"', owner: str = "tl") -> backlog_verify.Item:
        return backlog_verify.Item(
            raw={
                "id": item_id,
                "repo": "corelink-server",
                "owner": owner,
                "status": status,
                "verify": verify,
                "verify-means": "proof",
                "last-verified": "2026-08-23",
            },
            line=1,
            id=item_id,
        )

    def test_new_id_must_be_open_but_open_implementation_id_is_allowed(self) -> None:
        base = [self.item("B-001")]
        self.assertEqual(
            backlog_verify.validate_candidate_transitions(
                [self.item("B-001"), self.item("B-002")], base, dt.date(2026, 8, 23)
            ),
            [],
        )
        errors = backlog_verify.validate_candidate_transitions(
            [self.item("B-001"), self.item("B-002", status="done")],
            base,
            dt.date(2026, 8, 23),
        )
        self.assertTrue(any("new item B-002 must start status: open" in error for error in errors))

    def test_block_scalar_verify_and_field_status_manipulation_are_rejected(self) -> None:
        base = [self.item("B-001", status="open", verify="python3 scripts/verify.py\n")]
        candidate = [self.item("B-001", status="done", verify="python3 scripts/verify.py\n# mutation\n")]
        errors = backlog_verify.validate_candidate_transitions(
            candidate, base, dt.date(2026, 8, 23)
        )
        self.assertTrue(any("immutable field 'verify' changed" in error for error in errors))
        self.assertFalse(any("status transition" in error for error in errors))
        errors = backlog_verify.validate_candidate_transitions(
            [self.item("B-001", status="open", owner="owner")],
            base,
            dt.date(2026, 8, 23),
        )
        self.assertTrue(any("owner may change" in error for error in errors))

    def test_deleting_highest_base_id_is_rejected(self) -> None:
        errors = backlog_verify.validate_candidate_transitions(
            [self.item("B-001")],
            [self.item("B-001"), self.item("B-002")],
            dt.date(2026, 8, 23),
        )
        self.assertTrue(any("deleted BASE item(s): B-002" in error for error in errors))

    def test_mutable_workflow_ref_is_rejected_as_data(self) -> None:
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        text = workflow.read_text(encoding="utf-8").replace(
            "github.event.pull_request.head.sha || github.sha",
            "github.event.pull_request.head.ref",
        )
        workflow.write_text(text, encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "candidate workflow policy"):
            backlog_verify.validate_candidate_workflow(self.candidate)

    def test_transitive_control_mutation_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            trusted = Path(directory) / "trusted"
            candidate = Path(directory) / "candidate"
            (trusted / "scripts").mkdir(parents=True)
            (candidate / "scripts").mkdir(parents=True)
            shutil.copy2(ROOT / "scripts" / "backlog_verify.py", trusted / "scripts" / "backlog_verify.py")
            shutil.copy2(ROOT / "scripts" / "backlog_verify.py", candidate / "scripts" / "backlog_verify.py")
            (trusted / "scripts" / "check.py").write_text("from scripts import helper\n", encoding="utf-8")
            (trusted / "scripts" / "helper.py").write_text("VALUE = 1\n", encoding="utf-8")
            for name in ("check.py", "helper.py"):
                shutil.copy2(trusted / "scripts" / name, candidate / "scripts" / name)
            items = [self.item("B-001", verify="python3 scripts/check.py")]
            candidate.joinpath("scripts/helper.py").write_text("VALUE = 2\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "helper.py"):
                backlog_verify.check_candidate_controls(candidate, trusted, items)

    def test_dynamic_import_control_mutation_is_rejected(self) -> None:
        trusted_items = backlog_verify.parse((ROOT / "BACKLOG.md").read_text(encoding="utf-8"))
        controls = backlog_verify._candidate_control_paths(ROOT, trusted_items)
        self.assertIn("scripts/b155_backlog_grep_parser.py", controls)
        parser = self.candidate / "scripts" / "b155_backlog_grep_parser.py"
        parser.write_text(parser.read_text(encoding="utf-8") + "\n# candidate mutation\n", encoding="utf-8")

        result = self.run_gate()

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("b155_backlog_grep_parser.py", result.stdout + result.stderr)

    def test_trusted_semantic_mode_reproduces_stale_b001(self) -> None:
        result = subprocess.run(
            [sys.executable, str(VERIFIER), "--trusted-semantic", "--id", "B-001", "--today", "2026-09-08"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            env={**os.environ, "GITHUB_EVENT_NAME": "push"},
            check=False,
        )
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("STALE", result.stdout + result.stderr)

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.candidate = Path(self.temp.name) / "candidate"
        self.candidate.mkdir()
        shutil.copy2(ROOT / "BACKLOG.md", self.candidate / "BACKLOG.md")
        trusted_items = backlog_verify.parse((ROOT / "BACKLOG.md").read_text(encoding="utf-8"))
        controls = backlog_verify._candidate_control_paths(ROOT, trusted_items)
        for relative in controls:
            source = ROOT / relative
            target = self.candidate / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        workflow.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / ".github" / "workflows" / "backlog-verify.yml", workflow)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def run_gate(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(VERIFIER),
                "--candidate-file",
                str(self.candidate / "BACKLOG.md"),
                "--trusted-file",
                str(ROOT / "BACKLOG.md"),
                "--candidate-root",
                str(self.candidate),
                "--trusted-root",
                str(ROOT),
                "--today",
                "2026-09-08",
                "--id",
                "B-044",
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_mutated_candidate_verify_string_cannot_execute_or_green(self) -> None:
        text = (self.candidate / "BACKLOG.md").read_text(encoding="utf-8")
        marker = Path(self.temp.name) / "candidate-verify-executed"
        payload = (
            'python3 -c "from pathlib import Path; '
            f"Path({str(marker)!r}).write_text('pwned')"
            '"'
        )
        mutated, count = re.subn(
            r"(### B-044\b.*?^verify:) .*?$",
            rf"\1 {payload}",
            text,
            count=1,
            flags=re.MULTILINE | re.DOTALL,
        )
        self.assertEqual(count, 1)
        (self.candidate / "BACKLOG.md").write_text(mutated, encoding="utf-8")

        result = self.run_gate()

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(marker.exists(), "candidate verify payload was executed")
        self.assertIn("immutable field 'verify' changed", result.stdout + result.stderr)

    def test_mutated_candidate_verifier_cannot_green(self) -> None:
        verifier = self.candidate / "scripts" / "verify_b044_orphan_teardown_wp.py"
        self.assertTrue(verifier.is_file())
        verifier.write_text(verifier.read_text(encoding="utf-8") + "\n# candidate mutation\n", encoding="utf-8")

        result = self.run_gate()

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("mutated trusted backlog control", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
