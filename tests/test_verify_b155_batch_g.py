"""Focused semantic tests for B-126's closed zero-oversized predicate."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b155_batch_g.py"
SPEC = importlib.util.spec_from_file_location("b155_batch_g", SCRIPT)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = verifier
SPEC.loader.exec_module(verifier)


class B126BatchVerifierTests(unittest.TestCase):
    def test_baseline_population_is_green(self) -> None:
        verifier.verify_b126()

    def test_empty_population_is_red(self) -> None:
        with patch.object(verifier, "code_files", return_value=[]):
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b126()

    def test_synthetic_oversized_population_is_red(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "synthetic.py"
            path.write_text("pass\n" * 1001, encoding="utf-8")
            with patch.object(verifier, "code_files", return_value=[path]):
                with self.assertRaises(verifier.CheckError):
                    verifier.verify_b126()

    def test_restored_at_limit_population_is_green(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "synthetic.py"
            path.write_text("pass\n" * 1000, encoding="utf-8")
            with patch.object(verifier, "code_files", return_value=[path]):
                verifier.verify_b126()


if __name__ == "__main__":
    unittest.main()
