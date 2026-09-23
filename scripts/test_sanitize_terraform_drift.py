#!/usr/bin/env python3
"""Tests for the Terraform drift evidence boundary.

The fixture intentionally contains a secret and resource payload.  The test
invokes the same sanitizer used by the workflow and inspects the bytes that
would be uploaded, rather than checking only YAML path names.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import stat
import subprocess
import tempfile
import unittest
import zipfile
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


def workflow_step(workflow: str, name: str) -> str:
    """Return one step by its exact name prefix, bounded by the next step."""
    lines = workflow.splitlines()
    start = next(
        index
        for index, line in enumerate(lines)
        if line.startswith("      - name: ") and name in line
    )
    end = next(
        (
            index
            for index in range(start + 1, len(lines))
            if lines[index].startswith("      - name: ")
        ),
        len(lines),
    )
    return "\n".join(lines[start:end])


def run_block(step: str) -> str:
    """Extract a literal YAML `run: |` script without executing other steps."""
    lines = step.splitlines()
    run_at = next(i for i, line in enumerate(lines) if line == "        run: |")
    body = []
    for line in lines[run_at + 1 :]:
        if line and not line.startswith("          "):
            break
        body.append(line[10:] if line.startswith("          ") else "")
    return "\n".join(body).replace("${{ matrix.region }}", "wnam")


def upload_inputs(workflow: str) -> tuple[str, str, str]:
    step = workflow_step(workflow, "Upload sanitized drift summary")
    values = {}
    for key in ("path", "retention-days", "if-no-files-found"):
        match = re.search(rf"(?m)^          {re.escape(key)}: (.+)$", step)
        if match:
            values[key] = match.group(1)
    return values["path"], values["retention-days"], values["if-no-files-found"]


def run_workflow_evidence_fixture(
    testcase: unittest.TestCase, exit_code: int
) -> tuple[dict[str, object], bytes, str, list[str]]:
    """Execute the real plan helper and sanitizer workflow step with fake Terraform."""
    canary = "TERRAFORM_CANARY_SECRET_1722"
    resource_changes = (
        []
        if exit_code == 0
        else [
            {
                "address": "cloudflare_worker_script.private",
                "change": {
                    "actions": ["update"],
                    "before": {"token": canary},
                    "after": {"token": canary},
                },
            },
            {
                "address": "cloudflare_r2_bucket.private",
                "change": {"actions": ["create"]},
            },
            {
                "address": "cloudflare_worker_route.private",
                "change": {"actions": ["delete"]},
            },
            {
                "address": "cloudflare_d1_database.replace",
                "change": {"actions": ["delete", "create"]},
            },
        ]
    )
    fixture = {
        "variables": {"sensitive_token": {"value": canary}},
        "configuration": {"root_module": {"resources": [{"address": "cloudflare_worker_script.private"}]}},
        "prior_state": {"values": {"root_module": {"resources": [{"address": "cloudflare_worker_script.private", "values": {"token": canary}}]}}},
        "resource_changes": resource_changes,
    }

    with tempfile.TemporaryDirectory() as directory:
        temp = Path(directory)
        workspace = temp / "workspace"
        root = workspace / "infra/terraform/regions/wnam"
        bin_dir = temp / "bin"
        (workspace / "scripts").mkdir(parents=True)
        root.mkdir(parents=True)
        bin_dir.mkdir()
        shutil.copy2(SANITIZER, workspace / "scripts/sanitize_terraform_drift.py")
        plan_json = temp / "fake-plan.json"
        plan_json.write_text(json.dumps(fixture), encoding="utf-8")
        fake_terraform = bin_dir / "terraform"
        fake_terraform.write_text(
            r'''#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == -chdir=* ]]; then root="${1#-chdir=}"; shift; else root="$PWD"; fi
case "$1" in
  plan)
    shift
    out=
    for arg in "$@"; do [[ "$arg" == -out=* ]] && out="${arg#-out=}"; done
    printf '%s' "$FAKE_CANARY" > "$root/${out}"
    printf 'token = (sensitive value)\n'
    exit "$FAKE_TF_EXIT"
    ;;
  show) cat "$FAKE_PLAN_JSON" ;;
  *) exit 97 ;;
esac
''',
            encoding="utf-8",
        )
        fake_terraform.chmod(fake_terraform.stat().st_mode | stat.S_IXUSR)

        outputs = temp / "github-output"
        env = os.environ.copy()
        env.update(
            {
                "PATH": f"{bin_dir}:{env['PATH']}",
                "GITHUB_OUTPUT": str(outputs),
                "GITHUB_WORKSPACE": str(workspace),
                "GITHUB_RUN_ID": "1722",
                "GITHUB_RUN_ATTEMPT": "1",
                "FAKE_TF_EXIT": str(exit_code),
                "FAKE_CANARY": canary,
                "FAKE_PLAN_JSON": str(plan_json),
                "TF_BACKEND_BUCKET": "test-bucket",
                "TF_BACKEND_ENDPOINT": "https://example.invalid",
                "AWS_ACCESS_KEY_ID": "test-key-id",
                "AWS_SECRET_ACCESS_KEY": "test-secret",
                "CLOUDFLARE_API_TOKEN": "test-provider-token",
                "TF_VAR_cf_account_id": "test-account",
                "TF_VAR_cf_zone_id": "test-zone",
            }
        )

        # First run the production helper used by the Terraform plan step. Its
        # fake CLI writes a real canary into the runner-local saved plan while
        # exposing only Terraform's usual `(sensitive value)` placeholder.
        planned = subprocess.run(
            ["bash", str(ROOT / "scripts/terraform_drift_plan.sh"), "wnam", str(root)],
            cwd=ROOT,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        plan_output = outputs.read_text(encoding="utf-8") if outputs.exists() else ""
        local_log = root / "plan-wnam.log"
        local_plan = root / "plan-wnam.tfplan"
        testcase.assertIn(canary.encode(), local_plan.read_bytes())
        testcase.assertIn("(sensitive value)", local_log.read_text(encoding="utf-8"))

        # Execute the actual sanitizer step from the production workflow, with
        # the plan helper's exit output wired in as Actions does.
        workflow = WORKFLOW.read_text(encoding="utf-8")
        sanitize_script = run_block(
            workflow_step(workflow, "Sanitize Terraform drift evidence")
        )
        sanitize_env = env.copy()
        sanitize_env["PLAN_EXIT"] = str(exit_code if exit_code in (0, 1, 2) else 1)
        sanitized = subprocess.run(
            ["bash", "-e", "-c", sanitize_script],
            cwd=workspace,
            env=sanitize_env,
            text=True,
            capture_output=True,
            check=False,
        )
        logs = planned.stdout + planned.stderr + sanitized.stdout + sanitized.stderr
        testcase.assertEqual(planned.returncode, exit_code)
        testcase.assertEqual(sanitized.returncode, 0, logs)
        testcase.assertNotIn(canary, logs)
        testcase.assertNotIn("(sensitive value)", logs)
        testcase.assertNotIn(canary, plan_output)

        # The real EXIT trap must remove every raw artifact before the upload
        # action runs. The uploaded bytes below come from the path expression
        # in the workflow, not from a separately hand-picked test fixture.
        testcase.assertFalse(local_plan.exists())
        testcase.assertFalse(local_log.exists())
        testcase.assertFalse(workspace.joinpath("terraform-plan-wnam.json").exists())
        path, retention, missing = upload_inputs(workflow)
        testcase.assertEqual(
            path, "terraform-drift-summary-${{ matrix.region }}.json"
        )
        testcase.assertEqual(retention, "7")
        testcase.assertEqual(missing, "error")
        uploaded_files = list(
            workspace.glob(path.replace("${{ matrix.region }}", "wnam"))
        )
        testcase.assertEqual(
            [p.name for p in uploaded_files], ["terraform-drift-summary-wnam.json"]
        )
        archive = temp / "artifact.zip"
        with zipfile.ZipFile(archive, "w") as bundle:
            for item in uploaded_files:
                bundle.write(item, item.name)
        with zipfile.ZipFile(archive) as bundle:
            testcase.assertEqual(
                bundle.namelist(), ["terraform-drift-summary-wnam.json"]
            )
            uploaded = bundle.read(bundle.namelist()[0])
        summary = json.loads(uploaded)
        testcase.assertEqual(set(summary), ALLOWED_KEYS)
        testcase.assertNotIn(canary.encode(), archive.read_bytes())
        return summary, uploaded, logs, ["terraform-drift-summary-wnam.json"]


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
        upload = workflow_step(workflow, "Upload sanitized drift summary")
        path, retention, missing = upload_inputs(workflow)
        self.assertEqual(path, "terraform-drift-summary-${{ matrix.region }}.json")
        self.assertEqual(retention, "7")
        self.assertEqual(missing, "error")
        self.assertIn("if: always()", upload)
        self.assertNotIn(".tfplan", path)
        self.assertNotIn(".log", path)
        self.assertNotRegex(workflow, r"(?m)^\s*terraform apply(?:\s|$)")

    def test_real_workflow_steps_package_only_redacted_summary_for_all_exit_codes(self) -> None:
        expected = {
            0: (
                "clean",
                False,
                {"create": 0, "update": 0, "delete": 0, "replace": 0, "read": 0},
            ),
            1: (
                "error",
                False,
                {"create": 0, "update": 0, "delete": 0, "replace": 0, "read": 0},
            ),
            2: (
                "drift",
                True,
                {"create": 1, "update": 1, "delete": 1, "replace": 1, "read": 0},
            ),
        }
        for exit_code, (result, detected, actions) in expected.items():
            with self.subTest(terraform_exit_code=exit_code):
                summary, artifact, logs, members = run_workflow_evidence_fixture(self, exit_code)
                self.assertEqual(members, ["terraform-drift-summary-wnam.json"])
                self.assertEqual(summary["terraform_exit_code"], exit_code)
                self.assertEqual(summary["result"], result)
                self.assertEqual(summary["drift_detected"], detected)
                self.assertEqual(summary["action_counts"], actions)
                self.assertEqual(set(summary), ALLOWED_KEYS)
                self.assertNotIn(b"TERRAFORM_CANARY_SECRET_1722", artifact)
                self.assertNotIn(b"cloudflare_", artifact)
                self.assertNotIn("TERRAFORM_CANARY_SECRET_1722", logs)
                self.assertNotIn("(sensitive value)", logs)

    def test_contract_and_runbook_only_promise_sanitized_evidence(self) -> None:
        work_item = (ROOT / "specs/04_sprints/S13/work_items/WI-S13-004-terraform-drift-detection-daily-rb-fm-206.md").read_text(encoding="utf-8")
        runbook = (ROOT / "specs/05_quality/runbooks/RB-FM-206-terraform-drift.md").read_text(encoding="utf-8")
        self.assertIn("plan_summary_artifact_url TEXT", work_item)
        self.assertIn("sanitized summary artifact", work_item.lower())
        self.assertIn(
            "raw plan files, plan JSON, and terminal logs never leave the runner",
            work_item,
        )
        self.assertIn("sanitized summary artifact (7 days)", runbook)
        self.assertIn("Do not download or reconstruct a raw plan", runbook)
        self.assertNotIn("upload plan artifact", work_item.lower())
        self.assertNotIn("View full plan", runbook)


if __name__ == "__main__":
    unittest.main()
