"""Regression tests for the B-067 execution-gate verifier."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b067_auth_ci", ROOT / "scripts/verify_b067_auth_ci.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class B067AuthCiTests(unittest.TestCase):
    def test_live_contract(self) -> None:
        MODULE.verify_tree()

    def test_mutations_have_teeth(self) -> None:
        MODULE.run_self_tests()


if __name__ == "__main__":
    unittest.main()
