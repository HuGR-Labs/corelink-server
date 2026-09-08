"""Focused fail-closed tests for the redacted B-013 closure evidence."""

from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b013_key_deletion.py"
SPEC = importlib.util.spec_from_file_location("verify_b013_key_deletion", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class B013EvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.data = json.loads(MODULE.EVIDENCE.read_text(encoding="utf-8"))

    def test_canonical_evidence_passes(self) -> None:
        self.assertEqual(MODULE.check_evidence(self.data), {"targets": 3})

    def test_cli_and_mutation_self_test_pass(self) -> None:
        result = subprocess.run(
            [sys.executable, "-S", str(SCRIPT), "--self-test"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("exact 3-target closure record verified", result.stdout)
        self.assertIn("mutation(s) rejected", result.stdout)

    def test_exact_population_and_schema_fail_closed(self) -> None:
        for mutation in ("missing", "extra", "duplicate"):
            with self.subTest(mutation=mutation):
                mutated = copy.deepcopy(self.data)
                if mutation == "missing":
                    mutated["targets"].pop()
                elif mutation == "extra":
                    mutated["targets"].append(copy.deepcopy(mutated["targets"][0]))
                else:
                    mutated["targets"][1] = copy.deepcopy(mutated["targets"][0])
                with self.assertRaises(MODULE.VerificationError):
                    MODULE.check_evidence(mutated)

        unknown = copy.deepcopy(self.data)
        unknown["unreviewed_host"] = "host detail"
        with self.assertRaises(MODULE.VerificationError):
            MODULE.check_evidence(unknown)

        missing_field = copy.deepcopy(self.data)
        del missing_field["targets"][0]["size_before_bytes"]
        with self.assertRaises(MODULE.VerificationError):
            MODULE.check_evidence(missing_field)

    def test_exact_basename_sizes_and_command_fail_closed(self) -> None:
        for index, field, value in (
            (0, "basename", "other.pem"),
            (1, "size_before_bytes", 1676),
            (2, "deletion_command", "rm -f"),
        ):
            with self.subTest(index=index, field=field):
                mutated = copy.deepcopy(self.data)
                mutated["targets"][index][field] = value
                with self.assertRaises(MODULE.VerificationError):
                    MODULE.check_evidence(mutated)

    def test_positive_postcondition_polarity_is_required(self) -> None:
        for field in ("public_key_preserved", "all_targets_absent_after"):
            mutated = copy.deepcopy(self.data)
            mutated[field] = False
            with self.subTest(field=field), self.assertRaises(MODULE.VerificationError):
                MODULE.check_evidence(mutated)

        exists = copy.deepcopy(self.data)
        exists["targets"][0]["exists_after"] = True
        with self.assertRaises(MODULE.VerificationError):
            MODULE.check_evidence(exists)

        operator = copy.deepcopy(self.data)
        operator["operator"] = "Codex"
        with self.assertRaises(MODULE.VerificationError):
            MODULE.check_evidence(operator)

    def test_duplicate_json_keys_and_secret_material_fail_closed(self) -> None:
        with self.assertRaises(MODULE.VerificationError):
            MODULE._parse('{"schema_version": 1, "schema_version": 1}')

        secret = copy.deepcopy(self.data)
        secret["operator"] = "-----BEGIN RSA PRIVATE KEY-----"
        with self.assertRaises(MODULE.VerificationError):
            MODULE.check_evidence(secret)


if __name__ == "__main__":
    unittest.main()
