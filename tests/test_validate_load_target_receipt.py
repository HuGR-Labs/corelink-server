from __future__ import annotations

import datetime as dt
import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("load_target_receipt", ROOT / "scripts/validate_load_target_receipt.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


NOW = dt.datetime(2026, 9, 22, 12, 0, tzinfo=dt.UTC)


def receipt() -> dict[str, object]:
    return {
        "schema": 1,
        "environment": "staging",
        "target": module.CANONICAL_TARGET,
        "tenant_id": "019e7109-e514-72b2-ac5b-607d97ea64a1",
        "deployment_sha": "a" * 40,
        "issued_at": "2026-09-22T00:00:00Z",
        "expires_at": "2026-09-22T23:59:59Z",
    }


class LoadTargetReceiptTests(unittest.TestCase):
    def test_workflow_contract_is_manual_bounded_and_canonical(self) -> None:
        workflow = (ROOT / ".github/workflows/load-test-nightly.yml").read_text()
        self.assertEqual(module.workflow_gaps(workflow), [])

    def test_workflow_mutations_are_killed(self) -> None:
        workflow = (ROOT / ".github/workflows/load-test-nightly.yml").read_text()
        mutations = (
            workflow.replace("_internal/load-test/teardown", "arbitrary.example/teardown", 1),
            workflow.replace("K6_TARGET_IDENTITY_RECEIPT", "UNBOUND_RECEIPT"),
            workflow.replace("if: steps.filter.outputs.enabled == 'true' && steps.target_host.outcome == 'success' && always()", "if: always()", 1),
        )
        for mutated in mutations:
            with self.subTest(mutated=mutated):
                self.assertNotEqual(module.workflow_gaps(mutated), [])

    def test_accepts_canonical_unexpired_receipt(self) -> None:
        result = module.validate(receipt(), module.CANONICAL_TARGET, now=NOW)
        self.assertEqual(result["deployment_sha"], "a" * 40)

    def test_rejects_arbitrary_target_and_path(self) -> None:
        for target in ("https://example.test", module.CANONICAL_TARGET + "/v1", "http://staging.corelink.humangr.com"):
            with self.subTest(target=target):
                with self.assertRaises(module.ReceiptError):
                    module.validate(receipt(), target, now=NOW)

    def test_rejects_missing_or_malformed_identity(self) -> None:
        for key, value in (("deployment_sha", "HEAD"), ("tenant_id", "other-tenant"), ("environment", "production"), ("expires_at", "2026-09-22T12:00:00Z")):
            mutated = receipt()
            mutated[key] = value
            with self.subTest(key=key):
                with self.assertRaises(module.ReceiptError):
                    module.validate(mutated, module.CANONICAL_TARGET, now=NOW)

    def test_rejects_extra_fields_and_long_lease(self) -> None:
        extra = receipt()
        extra["owner"] = "SRE Lead"
        with self.assertRaises(module.ReceiptError):
            module.validate(extra, module.CANONICAL_TARGET, now=NOW)
        long_lived = receipt()
        long_lived["expires_at"] = "2026-09-23T01:00:01Z"
        with self.assertRaises(module.ReceiptError):
            module.validate(long_lived, module.CANONICAL_TARGET, now=NOW)

    def test_cli_writes_redacted_public_receipt_and_sha_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "receipt.json"
            github_output = Path(directory) / "output"
            environment = dict(os.environ, K6_TARGET_IDENTITY_RECEIPT=json.dumps(receipt()))
            old = os.environ.copy()
            try:
                os.environ.clear()
                os.environ.update(environment)
                self.assertEqual(module.main(["--target", module.CANONICAL_TARGET, "--output", str(output), "--github-output", str(github_output)]), 0)
            finally:
                os.environ.clear()
                os.environ.update(old)
            self.assertEqual(json.loads(output.read_text())["deployment_sha"], "a" * 40)
            self.assertEqual(github_output.read_text(), f"deployment_sha={'a' * 40}\n")


if __name__ == "__main__":
    unittest.main()
