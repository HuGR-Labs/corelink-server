"""Focal tests for the B-074 static/adversarial contract guard."""

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b074_money_path_auth", ROOT / "scripts/verify_b074_money_path_auth.py"
)
GUARD = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(GUARD)


class B074GuardTest(unittest.TestCase):
    def test_current_contract_is_open_and_wired(self):
        self.assertEqual(
            GUARD.verify(ROOT),
            {"rust_routes": 2, "worker_wiring": 3, "behavioral_tests": 2},
        )

    def test_adversarial_mutations_are_rejected(self):
        GUARD.self_test(ROOT)


if __name__ == "__main__":
    unittest.main()
