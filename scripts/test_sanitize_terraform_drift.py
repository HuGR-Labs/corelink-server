#!/usr/bin/env python3
"""Tests for the Terraform drift evidence boundary.

The fixture intentionally contains a secret and resource payload.  The test
invokes the same sanitizer used by the workflow and inspects the bytes that
would be uploaded, rather than checking only YAML path names.
"""

from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SANITIZER = ROOT / "scripts" / "sanitize_terraform_drift.py"
WORKFLOW = ROOT / ".github" / "workflows" / "terraform-drift.yml"
ALLOWED_KEYS = {
    "schema_version",
    "workflow_run_id",
    "workflow_run_attempt",
    "detected_at",
    "region",
    "terraform_exit_code",
    "result",
    "drift_detected",
    "action_counts",
}


def run_sanitizer(
    root: Path, exit_code: int, plan: dict[str, object] | None
) -> dict[str, object]:
    plan_path = root / "raw-plan.json"
    output_path = root / "summary.json"
    if plan is not None:
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
    command = [
        "python3",
        str(SANITIZER),
        "--region",
        "wnam",
        "--run-id",
        "12345",
        "--run-attempt",
        "2",
        "--detected-at",
        "2026-09-21T12:34:56Z",
        "--exit-code",
        str(exit_code),
        "--output",
        str(output_path),
    ]
    if plan is not None:
        command.extend(["--plan-json", str(plan_path)])
    subprocess.run(command, check=True, cwd=ROOT, capture_output=True, text=True)
    return json.loads(output_path.read_text(encoding="utf-8"))


class TerraformDriftEvidenceTests(unittest.TestCase):
    def test_fixture_secret_and_resource_payload_are_not_uploaded(self) -> None:
        canary = "TERRAFORM_CANARY_SECRET_1722"
        fixture = {
            "variables": {"sensitive_token": {"value": canary}},
            "configuration": {"root_module": {"resources": [{"address": "cloudflare_worker_script.secret"}]}},
            "prior_state": {"values": {"root_module": {"resources": [{"address": "cloudflare_worker_script.secret", "values": {"token": canary}}]}}},
            "resource_changes": [
                {
                    "address": "cloudflare_worker_script.secret",
                    "change": {"actions": ["update"], "before": {"token": canary}, "after": {"token": canary}},
                },
                {"address": "cloudflare_r2_bucket.private", "change": {"actions": ["create"]}},
                {"address": "cloudflare_worker_route.private", "change": {"actions": ["delete"]}},
                {"address": "cloudflare_d1_database.replace", "change": {"actions": ["delete", "create"]}},
            ],
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            summary = run_sanitizer(root, 2, fixture)
            uploaded = (root / "summary.json").read_bytes()

        self.assertNotIn(canary.encode(), uploaded)
        self.assertNotIn(b"cloudflare_worker_script", uploaded)
        self.assertEqual(set(summary), ALLOWED_KEYS)
        self.assertEqual(
            summary["action_counts"],
            {"create": 1, "update": 1, "delete": 1, "replace": 1, "read": 0},
        )
        self.assertTrue(summary["drift_detected"])
        self.assertEqual(summary["result"], "drift")

    def test_exit_codes_preserve_clean_error_and_drift_categories(self) -> None:
        empty_plan = {"resource_changes": []}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            clean = run_sanitizer(root, 0, empty_plan)
            error = run_sanitizer(root, 1, None)
            drift = run_sanitizer(root, 2, {"resource_changes": [{"change": {"actions": ["read"]}}]})
        self.assertEqual((clean["result"], clean["drift_detected"]), ("clean", False))
        self.assertEqual((error["result"], error["drift_detected"]), ("error", False))
        self.assertEqual((drift["result"], drift["drift_detected"]), ("drift", True))

    def test_workflow_uploads_only_sanitized_summary_and_fails_closed(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        upload = workflow.split("uses: actions/upload-artifact@", 1)[1].split("#", 1)[0]
        self.assertIn("terraform-drift-summary-${{ matrix.region }}.json", upload)
        self.assertNotIn(".tfplan", upload)
        self.assertNotIn(".log", upload)
        self.assertIn("if-no-files-found: error", upload)
        self.assertIn("retention-days: 7", upload)
        self.assertNotRegex(workflow, r"terraform plan[\s\S]{0,500}tee")
        self.assertNotRegex(workflow, r"(?m)^\s*terraform apply(?:\s|$)")


if __name__ == "__main__":
    unittest.main()
