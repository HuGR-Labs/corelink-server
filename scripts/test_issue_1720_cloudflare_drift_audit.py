#!/usr/bin/env python3
"""Static and adversarial checks for the bounded #1720 audit workflow."""

from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
AUDIT_WORKFLOW = ROOT / ".github/workflows/issue-1720-cloudflare-drift-audit.yml"
DRIFT_WORKFLOW = ROOT / ".github/workflows/terraform-drift.yml"

READ_ENDPOINTS = (
    "/accounts/$CF_ACCOUNT_ID/r2/buckets?per_page=1",
    "/accounts/$CF_ACCOUNT_ID/d1/database?per_page=1",
    "/accounts/$CF_ACCOUNT_ID/storage/kv/namespaces?per_page=1",
    "/accounts/$CF_ACCOUNT_ID/workers/scripts?per_page=1",
    "/zones/$CF_ZONE_ID/dns_records?per_page=1",
    "/zones/$CF_ZONE_ID/workers/routes?per_page=1",
)


def require(source: str, marker: str) -> None:
    if marker not in source:
        raise AssertionError(f"missing required marker: {marker}")


def verify_audit_workflow(source: str) -> None:
    """Reject auth, dispatch, redaction, and cleanup contract regressions."""
    for marker in (
        "workflow_dispatch:",
        "issue-1720-audit-drift-token",
        "actions: write",
        "contents: read",
        "runs-on: ubuntu-24.04",
        "CLOUDFLARE_API_TOKEN: ${{ secrets.CF_TERRAFORM_DRIFT_API_TOKEN }}",
        "CF_ACCOUNT_ID: ${{ secrets.CF_ACCOUNT_ID }}",
        "CF_ZONE_ID: ${{ vars.CF_ZONE_ID }}",
        'Authorization: Bearer $CLOUDFLARE_API_TOKEN',
        "read_receipt \"$label\" \"$endpoint\"",
        "account_reads=4 zone_reads=2",
        'repos/${GITHUB_REPOSITORY}/git/ref/heads/main',
        "main_sha=\"\"",
        "if ! main_sha=\"$(gh api",
        "$(gh api",
        "--jq '.object.sha'",
        '[[ ! "$main_sha" =~ ^[0-9a-f]{40}$ ]]',
        '[[ "$GITHUB_REF" != "refs/heads/main" ]]',
        '[[ "$main_sha" != "$GITHUB_SHA" ]]',
        "if ! gh workflow run terraform-drift.yml",
        '--repo "$GITHUB_REPOSITORY"',
        "--ref main",
        '--field expected_sha="$main_sha"',
        "; then",
        'DRIFT_WORKFLOW: terraform-drift.yml',
        'AUDIT_WORKFLOW: issue-1720-cloudflare-drift-audit.yml',
        "if ! gh api --method PUT",
        "/actions/workflows/${workflow}/disable",
        "--jq '.state'",
        "disabled_manually",
        "trap cleanup_disable_workflows EXIT",
        "trap - EXIT",
        'exit "$cleanup_status"',
    ):
        require(source, marker)

    if source.count("${{ secrets.CF_TERRAFORM_DRIFT_API_TOKEN }}") != 1:
        raise AssertionError("the preinstalled dedicated token must be consumed once")
    dispatch_step = source[source.index("Dispatch one exact-ref Terraform drift run") :]
    if '--ref "$GITHUB_SHA"' in dispatch_step or '--ref "$main_sha"' in dispatch_step:
        raise AssertionError("drift dispatch must use the symbolic main ref")
    if dispatch_step.count("gh workflow run terraform-drift.yml") != 1:
        raise AssertionError("the bounded audit must dispatch drift exactly once")
    for endpoint in READ_ENDPOINTS:
        require(source, endpoint)
    if source.count('"Workers') < 2:
        raise AssertionError("the six-read list must retain named account resources")

    for forbidden in (
        "CF_API_TOKEN",
        "CF_CLIENT_ID",
        "CF_CLIENT_SECRET",
        "gh secret set",
        "/actions/secrets",
        "/user/tokens",
        "--request POST",
        "--method POST",
        "|| true",
    ):
        if forbidden in source:
            raise AssertionError(f"forbidden credential or ignored failure marker: {forbidden}")

    if not re.search(r"account_receipt=.*redact_id", source):
        raise AssertionError("account ID must be emitted only through redaction")
    if not re.search(r"zone_receipt=.*redact_id", source):
        raise AssertionError("zone ID must be emitted only through redaction")
    require(source, "cleanup_failed=1")
    require(source, "cleanup_status=1")


def verify_drift_workflow(source: str) -> None:
    """Ensure every Terraform drift job runs on the hosted Ubuntu image."""
    runners = re.findall(r"(?m)^\s+runs-on:\s+([^\s#]+)", source)
    if runners != ["ubuntu-24.04", "ubuntu-24.04", "ubuntu-24.04"]:
        raise AssertionError(f"unexpected Terraform drift runners: {runners}")
    if "runs-on: corelink" in source:
        raise AssertionError("Terraform drift must not depend on the local runner")
    if re.search(r"(?m)^\s*terraform apply(?:\s|$)", source):
        raise AssertionError("Terraform drift must remain plan-only")
    for marker in (
        "expected_sha:",
        "Verify expected dispatch revision",
        "inputs.expected_sha != ''",
        'EXPECTED_SHA: ${{ inputs.expected_sha }}',
        '[[ "$EXPECTED_SHA" == "$GITHUB_SHA" ]]',
    ):
        require(source, marker)


class Issue1720CloudflareDriftAuditTests(unittest.TestCase):
    def test_audit_workflow_contract(self) -> None:
        verify_audit_workflow(AUDIT_WORKFLOW.read_text(encoding="utf-8"))

    def test_drift_jobs_are_hosted(self) -> None:
        verify_drift_workflow(DRIFT_WORKFLOW.read_text(encoding="utf-8"))

    def test_disable_failure_mutations_are_rejected(self) -> None:
        source = AUDIT_WORKFLOW.read_text(encoding="utf-8")
        mutations = (
            source.replace("if ! gh api --method PUT", "if gh api --method PUT", 1),
            source.replace("disabled_manually", "active"),
            source.replace("trap cleanup_disable_workflows EXIT", "trap cleanup_disable_workflows RETURN", 1),
            source.replace("cleanup_status=1", "cleanup_status=0", 1),
            source.replace(READ_ENDPOINTS[0], "/accounts/$CF_ACCOUNT_ID/r2/removed?per_page=1", 1),
            source.replace("[[ \"$main_sha\" != \"$GITHUB_SHA\" ]]", "[[ \"$main_sha\" == \"$GITHUB_SHA\" ]]", 1),
            source.replace('if ! main_sha="$(gh api', 'if main_sha="$(gh api', 1),
            source.replace('"repos/${GITHUB_REPOSITORY}/git/ref/heads/main"', '"repos/${GITHUB_REPOSITORY}/git/ref/heads/missing"', 1),
            source.replace(
                "            --ref main " + "\\" + "\n",
                '            --ref "$main_sha" ' + "\\" + "\n",
                1,
            ),
            source.replace("if ! gh workflow run terraform-drift.yml", "if gh workflow run terraform-drift.yml", 1),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with self.assertRaises(AssertionError):
                    verify_audit_workflow(mutation)

    def test_dispatch_revision_mutations_are_rejected(self) -> None:
        source = DRIFT_WORKFLOW.read_text(encoding="utf-8")
        mutations = (
            source.replace("expected_sha:", "unexpected_sha:", 1),
            source.replace("inputs.expected_sha != ''", "inputs.expected_sha == ''", 1),
            source.replace(
                '[[ "$EXPECTED_SHA" == "$GITHUB_SHA" ]]',
                '[[ "$EXPECTED_SHA" != "$GITHUB_SHA" ]]',
                1,
            ),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with self.assertRaises(AssertionError):
                    verify_drift_workflow(mutation)


if __name__ == "__main__":
    unittest.main()
