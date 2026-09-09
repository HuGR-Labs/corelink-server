"""Executable coverage for the fail-closed B-126-M2 wiring guard."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GUARD_PATH = ROOT / "scripts/verify_b126_m2_wiring.py"
SPEC = importlib.util.spec_from_file_location("verify_b126_m2_wiring", GUARD_PATH)
assert SPEC is not None and SPEC.loader is not None
GUARD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GUARD)


class B126M2WiringGuardTests(unittest.TestCase):
    def test_closed_population_and_real_g4b_identity(self) -> None:
        self.assertEqual(
            GUARD.verify(ROOT),
            {"roots": 6, "fragments": 53, "g4b_tests": 5},
        )

    def test_audit_root_include_population_is_exact_and_ordered(self) -> None:
        self.assertEqual(
            GUARD.ROOT_INCLUDES["crates/corelink-container/src/routes/audit_drain.rs"],
            (
                "audit_drain/b054_witness.rs",
                "audit_drain/b054_epoch_admin.rs",
                "audit_drain/b126_m2_impl_01.rs",
                "audit_drain/b126_m2_impl_01_part2.rs",
                "audit_drain/b126_m2_impl_02.rs",
                "audit_drain/b126_m2_impl_02_part2.rs",
                "audit_drain/b126_m2_impl_03.rs",
            ),
        )
        self.assertEqual(
            GUARD.ROOT_FRAGMENTS[
                "crates/corelink-container/src/routes/audit_drain.rs"
            ][:6],
            (
                "crates/corelink-container/src/routes/audit_drain/b054_witness.rs",
                "crates/corelink-container/src/routes/audit_drain/b054_witness_runtime.rs",
                "crates/corelink-container/src/routes/audit_drain/b054_witness_tests.rs",
                "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin.rs",
                "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin_authorities.rs",
                "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin_part2.rs",
            ),
        )

    def test_adversarial_wiring_mutations_are_rejected(self) -> None:
        GUARD.self_test(ROOT)


if __name__ == "__main__":
    unittest.main()
