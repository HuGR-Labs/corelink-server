from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_i1641_pagerduty_contract.py"
spec = importlib.util.spec_from_file_location("i1641_pagerduty", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class PagerDutyContractManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest = json.loads(
            (ROOT / "evidence/i1641/pagerduty-contract-manifest.json").read_text(encoding="utf-8")
        )

    def test_canonical_manifest_is_bound_to_open_external_receipt(self) -> None:
        self.assertIsNone(verifier.validate_manifest(self.manifest))

    def test_manifest_mutations_fail_closed(self) -> None:
        mutations = {
            "backlog binding": lambda data: data.__setitem__("backlog_id", "B-072"),
            "pending status": lambda data: data.__setitem__("status", "closed"),
            "receipt chain": lambda data: data["receipts"]["required_chain"].pop(),
            "owner artifact": lambda data: data["receipts"].__setitem__("external_owner_artifact", "none"),
            "safety boundary": lambda data: data["prohibited"].pop(),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                candidate = json.loads(json.dumps(self.manifest))
                mutate(candidate)
                self.assertIsNotNone(verifier.validate_manifest(candidate))


if __name__ == "__main__":
    unittest.main()
