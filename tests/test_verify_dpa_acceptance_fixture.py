#!/usr/bin/env python3
"""Regression tests for the DPA acceptance fixture guard."""

import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VERIFY = ROOT / "scripts/verify_dpa_acceptance_fixture.py"


class DpaAcceptanceFixtureGuardTest(unittest.TestCase):
    def test_comment_safe_mutation_teeth(self) -> None:
        result = subprocess.run(
            [sys.executable, str(VERIFY), "--self-test"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("4/4 mutations rejected", result.stdout)

    def test_live_source_contract(self) -> None:
        result = subprocess.run(
            [sys.executable, str(VERIFY)],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("source contract valid", result.stdout)


if __name__ == "__main__":
    unittest.main()
