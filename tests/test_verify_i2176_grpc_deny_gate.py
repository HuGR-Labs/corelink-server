"""Adversarial regressions for the trusted #2176 base-branch verifier."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_i2176_grpc_deny_gate as verify


class TrustedGrpcDenyGateTests(unittest.TestCase):
    def test_adversarial_fixture_mutations_fail_closed(self) -> None:
        verify.self_test()


if __name__ == "__main__":
    unittest.main()
