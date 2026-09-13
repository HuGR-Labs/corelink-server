#!/usr/bin/env python3
"""Regression for the B-160 verifier's mutation teeth."""

import os
import subprocess
import sys
import tempfile
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
        self.assertIn("12/12 rejected", result.stdout)

    def test_missing_pnpm_is_red(self):
        environment = os.environ.copy()
        environment.pop("B160_PNPM", None)
        with tempfile.TemporaryDirectory() as path:
            environment["PATH"] = path
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

    def test_pnpm_executable_override_is_ignored(self):
        with tempfile.TemporaryDirectory() as path:
            directory = Path(path)
            sentinel = directory / "sentinel"
            shim = directory / "pnpm-shim"
            shim.write_text(
                f"#!/bin/sh\nprintf invoked > {sentinel}\nprintf '10.32.1\\n'\n",
                encoding="utf-8",
            )
            shim.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = path
            environment["B160_PNPM"] = str(shim)
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
            self.assertFalse(sentinel.exists())


if __name__ == "__main__":
    unittest.main()
