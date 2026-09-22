from __future__ import annotations

import importlib.util
import math
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "sanitize_k6_summary", ROOT / "scripts/sanitize_k6_summary.py"
)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)


TENANT = "019e7109-e514-72b2-ac5b-607d97ea64a1"
RECEIPT = {
    "schema": 1,
    "environment": "staging",
    "target": module.CANONICAL_TARGET,
    "tenant_id": TENANT,
    "deployment_sha": "a" * 40,
    "issued_at": "2026-09-22T00:00:00Z",
    "expires_at": "2026-09-22T23:59:59Z",
}
RAW = {"metrics": {"http_req_duration": {"med": 10, "p(99)": 20}}, "secret": "discard"}


class SanitizedSummaryTests(unittest.TestCase):
    def test_allowlist_and_identity_are_written(self) -> None:
        result = module.sanitize(RAW, RECEIPT, "signup")
        self.assertEqual(result["suite_version"], module.SUITE_VERSION)
        self.assertEqual(result["tenant_id"], TENANT)
        self.assertNotIn("secret", result)

    def test_mutations_fail_closed(self) -> None:
        mutations = (
            ("missing median", {"metrics": {"http_req_duration": {"p(99)": 20}}}, RECEIPT),
            ("nan p99", {"metrics": {"http_req_duration": {"med": 10, "p(99)": math.nan}}}, RECEIPT),
            ("wrong target", RAW, {**RECEIPT, "target": "https://example.test"}),
            ("wrong tenant", RAW, {**RECEIPT, "tenant_id": "other-tenant"}),
            ("wrong deployment", RAW, {**RECEIPT, "deployment_sha": "HEAD"}),
        )
        for label, raw, receipt in mutations:
            with self.subTest(label=label):
                with self.assertRaises(module.SummaryError):
                    module.sanitize(raw, receipt, "signup")


if __name__ == "__main__":
    unittest.main()
