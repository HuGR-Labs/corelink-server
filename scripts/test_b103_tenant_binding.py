#!/usr/bin/env python3
"""Credentialless contract tests for the B-103 receipt/tenant boundary."""

from __future__ import annotations

import contextlib
import datetime as dt
import hashlib
import importlib.util
import io
import json
import os
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "load_target_receipt", ROOT / "scripts/validate_load_target_receipt.py"
)
assert spec and spec.loader
receipt_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(receipt_module)


TENANT = "019e7109-e514-72b2-ac5b-607d97ea64a1"
OTHER_TENANT = "019e7109-e514-72b2-ac5b-607d97ea64a2"
DEPLOYMENT_SHA = "a" * 40


def receipt() -> dict[str, object]:
    return {
        "schema": 1,
        "environment": "staging",
        "target": receipt_module.CANONICAL_TARGET,
        "tenant_id": TENANT,
        "deployment_sha": DEPLOYMENT_SHA,
        "issued_at": "2026-01-01T00:00:00Z",
        "expires_at": "2099-01-01T00:00:00Z",
    }


class B103TenantBindingTests(unittest.TestCase):
    def invoke_validator(self, configured_tenant: str, directory: str) -> tuple[int, str, Path, Path]:
        receipt_path = Path(directory) / "receipt.json"
        github_output = Path(directory) / "github-output"
        old_environment = os.environ.copy()
        os.environ.update(
            {
                "K6_TARGET_IDENTITY_RECEIPT": json.dumps(receipt()),
                "B103_TENANT_ID": configured_tenant,
            }
        )
        output = io.StringIO()
        try:
            with contextlib.redirect_stdout(output):
                result = receipt_module.main(
                    [
                        "--target",
                        receipt_module.CANONICAL_TARGET,
                        "--receipt-env",
                        "K6_TARGET_IDENTITY_RECEIPT",
                        "--tenant-env",
                        "B103_TENANT_ID",
                        "--redact-tenant",
                        "--output",
                        str(receipt_path),
                        "--github-output",
                        str(github_output),
                    ]
                )
        finally:
            os.environ.clear()
            os.environ.update(old_environment)
        return result, output.getvalue(), receipt_path, github_output

    def test_stale_tenant_variable_fails_before_receipt_or_load_contract(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result, log, receipt_path, github_output = self.invoke_validator(OTHER_TENANT, directory)

            self.assertEqual(result, 1)
            self.assertFalse(receipt_path.exists())
            self.assertFalse(github_output.exists())
            self.assertIn("configured tenant does not match", log)
            for sensitive in (TENANT, OTHER_TENANT, "corelink_pat_test", "cloudflare-token-test", "account-id-test"):
                self.assertNotIn(sensitive, log)

    def test_authorized_tenant_emits_only_stable_hash_reference(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result, log, receipt_path, github_output = self.invoke_validator(TENANT, directory)

            self.assertEqual(result, 0)
            public_receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
            tenant_hash = hashlib.sha256(TENANT.encode("ascii")).hexdigest()
            self.assertEqual(public_receipt["tenant_sha256"], tenant_hash)
            self.assertNotIn("tenant_id", public_receipt)
            self.assertEqual(
                github_output.read_text(encoding="utf-8"),
                f"deployment_sha={DEPLOYMENT_SHA}\ntenant_sha256={tenant_hash}\n",
            )
            for emitted in (receipt_path.read_text(encoding="utf-8"), github_output.read_text(encoding="utf-8"), log):
                self.assertNotIn(TENANT, emitted)

    def test_protected_workflow_binds_before_any_load_or_tail_step(self) -> None:
        workflow = (ROOT / ".github/workflows/b103-cargo-write-reproducer.yml").read_text(encoding="utf-8")
        reproducer = (ROOT / "scripts/reproduce_b103_cargo_write.py").read_text(encoding="utf-8")
        receipt_step = workflow.index("- name: validate canonical staging receipt")
        binding = workflow.index("--tenant-env B103_TENANT_ID", receipt_step)
        redaction = workflow.index("--redact-tenant", binding)
        load_step = workflow.index("- name: run warm sequence and bounded matrix")
        self.assertLess(receipt_step, binding)
        self.assertLess(binding, redaction)
        self.assertLess(redaction, load_step)
        self.assertIn(
            "github.repository == 'HuGR-dev/corelink-server' && github.ref == 'refs/heads/main' && github.ref_protected",
            workflow,
        )
        self.assertIn("environment: staging", workflow)
        self.assertNotIn("  schedule:", workflow)
        self.assertIn('"tenant_sha256": tenant_sha256(args.tenant)', reproducer)
        self.assertNotIn('"tenant_id": args.tenant', reproducer)


if __name__ == "__main__":
    unittest.main()
