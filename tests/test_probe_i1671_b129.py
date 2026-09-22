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


if __name__ == "__main__":
    unittest.main()
