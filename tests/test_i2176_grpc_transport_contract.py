#!/usr/bin/env python3
"""Hosted regression entry point for the #2176 fail-closed decision."""

from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent


class GrpcTransportContractTests(unittest.TestCase):
    def test_contract_and_adversarial_mutations(self) -> None:
        result = subprocess.run(
            [sys.executable, "-S", "scripts/verify_i2176_grpc_transport.py", "--self-test"],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("PASS (blocked/fail-closed)", result.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
