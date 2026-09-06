"""Mutation teeth for the executable B-144 contract guard."""
from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "verify_b144_pat_revoke.py"
SPEC = importlib.util.spec_from_file_location("verify_b144_pat_revoke", SCRIPT)
assert SPEC and SPEC.loader
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class B144VerifierTests(unittest.TestCase):
    def test_current_contract_is_done(self) -> None:
        self.assertEqual(VERIFY.assess(ROOT), [])

    def test_mutations_cannot_escape(self) -> None:
        VERIFY.mutation_checks(ROOT)


if __name__ == "__main__":
    unittest.main()
