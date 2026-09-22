"""Credentialless contract tests for issue #1649's restricted test-key lane."""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "stripe_test_mode_evidence.py"
WORKFLOW = ROOT / ".github" / "workflows" / "issue-1649-stripe-test-mode.yml"
SPEC = importlib.util.spec_from_file_location("stripe_test_mode_evidence", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class StripeRestrictedKeyContractTests(unittest.TestCase):
    def test_restricted_test_key_runs_only_through_mocked_requests(self) -> None:
        responses = [
            (200, {"livemode": False}),
            (200, {"livemode": False, "id": "cus_fixture"}),
            (200, {"livemode": False, "id": "cus_fixture"}),
            (200, {"livemode": False, "id": "cus_fixture"}),
            (200, {"deleted": True, "id": "cus_fixture"}),
        ]
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "receipt.json"
            with patch.object(MODULE, "request_json", side_effect=responses) as request:
                result = MODULE.run_probe("rk_test_fixture", "123456", output)

        self.assertEqual(result, 0)
        self.assertEqual(request.call_count, 5)
        receipt_text = output.read_text(encoding="utf-8")
        receipt = json.loads(receipt_text)
        self.assertIs(receipt["livemode"], False)
        self.assertTrue(receipt["idempotency_replayed"])
        self.assertTrue(receipt["cleanup"]["succeeded"])
        self.assertNotIn("rk_test_fixture", receipt_text)

    def test_non_restricted_or_non_test_prefixes_fail_before_requests(self) -> None:
        for key in ("sk_test_fixture", "sk_live_fixture", "rk_live_fixture", "malformed"):
            with self.subTest(key_class=key.split("_", maxsplit=2)[0:2]):
                with patch.object(MODULE, "request_json") as request:
                    with self.assertRaises(MODULE.ProbeError):
                        MODULE.run_probe(key, "123456", Path("unused-receipt.json"))
                request.assert_not_called()

    def test_workflow_contract_rejects_weak_prefix_or_missing_permissions(self) -> None:
        original = WORKFLOW.read_text(encoding="utf-8")
        mutations = (
            original.replace("rk_test_", "sk_test_"),
            original.replace("Accounts: Read", "Accounts: Write"),
            original.replace("Customers: Write", "Customers: Read"),
        )
        for index, mutated in enumerate(mutations):
            with self.subTest(mutation=index):
                with tempfile.TemporaryDirectory() as directory:
                    path = Path(directory) / "workflow.yml"
                    path.write_text(mutated, encoding="utf-8")
                    output = io.StringIO()
                    with contextlib.redirect_stdout(output):
                        self.assertEqual(MODULE.contract_check(path), 1)


if __name__ == "__main__":
    unittest.main()
