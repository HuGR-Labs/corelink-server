from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import verify_regional_capacity as verifier


class RegionalCapacityVerifierTests(unittest.TestCase):
    @staticmethod
    def provider_receipt(**overrides: object) -> dict[str, object]:
        receipt: dict[str, object] = {
            "schema_version": 1,
            "schema": verifier.PROVIDER_RECEIPT_SCHEMA,
            "issue": verifier.PROVIDER_RECEIPT_ISSUE,
            "read_only": True,
            "endpoint": verifier.PROVIDER_RECEIPT_ENDPOINT,
            "quota": {
                "total_vcpu": 1500,
                "vcpu_per_deployment": 4,
                "memory_mib_per_deployment": 4096,
                "total_memory_mib": 6291456,
            },
            "usage": dict(verifier.UNAVAILABLE_MEASUREMENT),
            "concurrency": dict(verifier.UNAVAILABLE_MEASUREMENT),
        }
        receipt.update(overrides)
        return receipt

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

    def test_provider_evidence_requires_the_exact_read_only_receipt_contract(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            path.write_text(json.dumps(self.provider_receipt(read_only=False)), encoding="utf-8")
            with self.assertRaises(verifier.CapacityError):
                verifier.verify_provider(path, model)

            path.write_text(json.dumps({"schema_version": 1, "read_only": True, "total_vcpu": 1500,
                                        "vcpu_per_deployment": 4, "total_memory_mib": 6291456}), encoding="utf-8")
            with self.assertRaises(verifier.CapacityError):
                verifier.verify_provider(path, model)

    def test_provider_receipt_accepts_only_approved_quota_aggregates(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            path.write_text(json.dumps(self.provider_receipt(
                captured_at="2026-09-23T00:00:00+00:00",
                account_id_redacted="6a1f...f5cd",
                provider_api_version="v4",
            )), encoding="utf-8")
            verifier.verify_provider(path, model)

    def test_provider_mismatch_fails_closed(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            receipt = self.provider_receipt()
            quota = receipt["quota"]
            assert isinstance(quota, dict)
            receipt["quota"] = {**quota, "total_vcpu": 1499}
            path.write_text(json.dumps(receipt), encoding="utf-8")
            with self.assertRaises(verifier.CapacityError):
                verifier.verify_provider(path, model)

    def test_provider_vcpu_mismatch_fails_closed(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            receipt = self.provider_receipt()
            quota = receipt["quota"]
            assert isinstance(quota, dict)
            receipt["quota"] = {**quota, "vcpu_per_deployment": 2}
            path.write_text(json.dumps(receipt), encoding="utf-8")
            with self.assertRaises(verifier.CapacityError):
                verifier.verify_provider(path, model)

    def test_provider_boolean_numeric_field_fails_closed(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            receipt = self.provider_receipt()
            quota = receipt["quota"]
            assert isinstance(quota, dict)
            receipt["quota"] = {**quota, "vcpu_per_deployment": True}
            path.write_text(json.dumps(receipt), encoding="utf-8")
            with self.assertRaises(verifier.CapacityError):
                verifier.verify_provider(path, model)

    def test_provider_nonfinite_or_measurement_claim_fails_closed(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            for receipt in (
                self.provider_receipt(quota={
                    "total_vcpu": 1500,
                    "vcpu_per_deployment": float("nan"),
                    "memory_mib_per_deployment": 4096,
                    "total_memory_mib": 6291456,
                }),
                self.provider_receipt(usage={"status": "measured"}),
            ):
                path.write_text(json.dumps(receipt), encoding="utf-8")
                with self.assertRaises(verifier.CapacityError):
                    verifier.verify_provider(path, model)

    def test_unknown_receipt_or_quota_claim_field_fails_closed(self) -> None:
        model = verifier.declared_budget()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "provider.json"
            quota = self.provider_receipt()["quota"]
            assert isinstance(quota, dict)
            for receipt in (
                self.provider_receipt(tenant_concurrency={"active": 1}),
                self.provider_receipt(measured_usage={"used_vcpu": 1}),
                self.provider_receipt(quota={**quota, "tenant_concurrency": 1}),
                self.provider_receipt(quota={**quota, "measured_usage": 1}),
            ):
                path.write_text(json.dumps(receipt), encoding="utf-8")
                with self.assertRaises(verifier.CapacityError):
                    verifier.verify_provider(path, model)
