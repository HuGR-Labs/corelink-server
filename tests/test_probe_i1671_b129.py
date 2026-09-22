"""Credentialless adversarial checks for the B-129 receipt contract."""

import importlib.util
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

    def test_conflicting_legacy_alias_fails_closed(self):
        with self.assertRaises(RuntimeError):
            PROBE.timing("oother;dur=4,ohandler;dur=5")

    def test_malformed_duration_fails_closed(self):
        with self.assertRaises(RuntimeError):
            PROBE.timing("wdb;dur=NaN")

    def test_phase_duration_must_consume_the_entire_server_timing_item(self):
        self.assertEqual(
            PROBE.timing('wdb;dur=5;desc="worker database"'),
            {"wdb": 5.0},
        )
        for malformed in (
            "wdb;dur=5ms",
            "wdb;dur=5junk",
            "prefix wdb;dur=5",
            "wdb;dur=5 trailing",
            "wdb;dur=5;desc=worker database",
            'wdb;dur=5;desc="unterminated',
            'wdb;dur=5;desc="worker"garbage',
            'wdb;dur=5;desc="worker";extra="value"',
        ):
            with self.subTest(header=malformed), self.assertRaises(RuntimeError):
                PROBE.timing(malformed)


if __name__ == "__main__":
    unittest.main()
