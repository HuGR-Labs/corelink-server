from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import verify_regional_capacity as verifier


class RegionalCapacityVerifierTests(unittest.TestCase):
    def test_repository_budget_matches_five_regions_and_runner_fleet(self) -> None:
        model = verifier.declared_budget()
        self.assertEqual(model["cache_reservation_vcpu"], 250.0)
        self.assertEqual(model["runner_reservation_vcpu"], 1000.0)
        self.assertEqual(model["reservation_vcpu"], 1250.0)
        self.assertEqual(model["headroom_vcpu"], 250)

    def test_unknown_instance_type_fails_closed(self) -> None:
        raw = verifier.BUDGET.read_text(encoding="utf-8")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "budget.json"
            payload = json.loads(raw)
            payload["cache"]["instance_type"] = "unknown"
            path.write_text(json.dumps(payload), encoding="utf-8")
            with mock.patch.object(verifier, "BUDGET", path):
                with self.assertRaises(verifier.CapacityError):
                    verifier.declared_budget()

    def test_provider_evidence_requires_read_only_marker_and_exact_limit(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            path.write_text(json.dumps({"schema_version": 1, "read_only": False, "total_vcpu": 1500,
                                        "vcpu_per_deployment": 4, "total_memory_mib": 6291456}), encoding="utf-8")
            with self.assertRaises(verifier.CapacityError):
                verifier.verify_provider(path, model)

    def test_provider_mismatch_fails_closed(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            path.write_text(json.dumps({"schema_version": 1, "read_only": True, "total_vcpu": 1499,
                                        "vcpu_per_deployment": 4, "total_memory_mib": 6291456}), encoding="utf-8")
            with self.assertRaises(verifier.CapacityError):
                verifier.verify_provider(path, model)
