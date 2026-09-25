"""Credentialless adversarial checks for the B-129 receipt contract."""

import importlib.util
import json
import pathlib
import unittest


ROOT = pathlib.Path(__file__).parents[1]
SPEC = importlib.util.spec_from_file_location("probe_i1671_b129", ROOT / "scripts/probe_i1671_b129.py")
assert SPEC and SPEC.loader
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)


class ProbeContractTests(unittest.TestCase):
    def _row(self):
        values = {name: 1.0 for name in (*PROBE.Q, "ohop", *PROBE.ORIGIN)}
        values.update({"wdb": 5.0, "origin": 11.0, "total": 17.0})
        headers = {
            "x-corelink-deployed-sha": "0123456789abcdef0123456789abcdef01234567",
            "x-corelink-server-timing-wdb-detail": "on",
            "x-corelink-deployed-region": "Sam",
            "x-request-id": "req-1",
            "cf-ray": "ray-1",
        }
        return values, headers

    def test_required_origin_phases_and_region_are_fail_closed(self):
        values, headers = self._row()
        self.assertLess(PROBE.validate_sample(200, headers, values, 0.02, "0123456789abcdef0123456789abcdef01234567", "Sam"), 10)
        for name in ("ostore", "oaccounting", "ohandler"):
            mutated = dict(values)
            del mutated[name]
            with self.assertRaises(RuntimeError):
                PROBE.validate_sample(200, headers, mutated, 0.02, "0123456789abcdef0123456789abcdef01234567", "Sam")
        with self.assertRaises(RuntimeError):
            PROBE.validate_sample(200, {**headers, "x-corelink-deployed-region": "Enam"}, values, 0.02, "0123456789abcdef0123456789abcdef01234567", "Sam")

    def test_median_gate_is_not_replaced_by_maximum_gate(self):
        rows = [{"residual_pct": value} for value in (1.0, 2.0, 3.0, 30.0, 40.0)]
        self.assertEqual(PROBE.residual_median(rows), 3.0)
        self.assertLess(PROBE.residual_median(rows), 10.0)

    def test_empty_receipt_fails_closed(self):
        with self.assertRaises(RuntimeError):
            PROBE.residual_median([])

    def test_receipt_keeps_evidence_and_redacts_sensitive_inputs(self):
        tenant_id = "123e4567-e89b-42d3-a456-426614174000"
        cargo_key = "read-only-cargo-key"
        body = "private response body"
        rows = [{"request_id": "req-1", "cf_ray": "ray-1", "phases_ms": {"wdb": 5.0}}]
        receipt = PROBE.build_receipt(
            target_origin="https://api.example.test",
            tenant_id=tenant_id,
            cargo_key=cargo_key,
            deployed_sha="0123456789abcdef0123456789abcdef01234567",
            deployed_region="Sam",
            captured_at="2026-09-25T12:00:00+00:00",
            sample_count=1,
            residual_median_pct=3.0,
            residual_max_pct=3.0,
            rows=rows,
        )
        serialized = json.dumps(receipt, sort_keys=True)

        self.assertEqual(receipt["deployed_sha"], "0123456789abcdef0123456789abcdef01234567")
        self.assertEqual(receipt["deployed_region"], "Sam")
        self.assertEqual(receipt["captured_at"], "2026-09-25T12:00:00+00:00")
        self.assertEqual(receipt["rows"], rows)
        for sensitive in (tenant_id, cargo_key, body):
            with self.subTest(sensitive=sensitive):
                self.assertNotIn(sensitive, serialized)
        self.assertNotIn('"tenant"', serialized)
        self.assertNotIn("cargo_key", receipt)
        self.assertNotIn("cargo_key_sha256", receipt)

    def test_conflicting_legacy_alias_fails_closed(self):
        with self.assertRaises(RuntimeError):
            PROBE.timing("oother;dur=4,ohandler;dur=5")

    def test_malformed_duration_fails_closed(self):
        with self.assertRaises(RuntimeError):
            PROBE.timing("wdb;dur=NaN")

    def test_quoted_description_can_contain_commas_and_escapes(self):
        raw = r'wdb;dur=5;desc="worker, \"primary\"",qdo;dur=2;desc="C:\\worker"'
        self.assertEqual(PROBE.timing(raw), {"wdb": 5.0, "qdo": 2.0})

    def test_quoted_parameter_boundaries_are_not_confused_with_phase_commas(self):
        self.assertEqual(
            PROBE.timing('wdb;dur=5;desc="qdo;dur=999, still wdb", qdo;dur=2'),
            {"wdb": 5.0, "qdo": 2.0},
        )

    def test_unterminated_or_trailing_escaped_quotes_fail_closed(self):
        for malformed in (
            'wdb;dur=5;desc="worker, qdo;dur=2',
            r'wdb;dur=5;desc="worker\\',
            'wdb;dur=5;desc="worker"garbage,qdo;dur=2',
            "wdb;dur=5,,qdo;dur=2",
        ):
            with self.subTest(header=malformed), self.assertRaises(RuntimeError):
                PROBE.timing(malformed)


if __name__ == "__main__":
    unittest.main()
