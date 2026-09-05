#!/usr/bin/env python3
"""Regression for the B-160 verifier's mutation teeth."""

import os
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VERIFY = ROOT / "scripts/verify_b160_pat_limit.py"


class B160VerifierTest(unittest.TestCase):
    def test_mutations_are_rejected(self):
        result = subprocess.run(
            [sys.executable, str(VERIFY), "--self-test"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("3/3 rejected", result.stdout)

    def test_missing_pnpm_is_red(self):
        environment = os.environ.copy()
        environment["B160_PNPM"] = "/definitely/missing/pnpm"
        result = subprocess.run(
            [sys.executable, str(VERIFY)],
            cwd=ROOT,
            env=environment,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("B-160 DRIFTED", result.stderr)


if __name__ == "__main__":
    unittest.main()
