"""Cheap mutation-backed tests for the B-076 backlog verifier."""
from pathlib import Path
import importlib.util
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b076", ROOT / "scripts/verify_b076_payable_subscription.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class B076VerifierTests(unittest.TestCase):
    def test_sources_are_closed(self):
        self.assertEqual(MODULE.assess(ROOT), [])


    def test_mutation_self_test(self):
        MODULE.self_test(ROOT)
