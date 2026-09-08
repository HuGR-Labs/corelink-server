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


class B110WorkflowVerifierTests(unittest.TestCase):
    def _mutated_text(self, target: str, old: str, new: str) -> object:
        original = verifier.text

        def read(candidate: str) -> str:
            source = original(candidate)
            if candidate == target:
                source = source.replace(old, new, 1)
            return source

        return patch.object(verifier, "text", side_effect=read)

    def test_baseline_trust_and_capacity_are_green(self) -> None:
        verifier.verify_b110()

    def test_untrusted_trigger_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/coverage.yml",
            "  workflow_dispatch:\n",
            "  push:\n",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_unprotected_mutation_writer_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/mutation-nightly.yml",
            " && github.ref_protected",
            "",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_or_bypass_in_mutation_guard_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/mutation-nightly.yml",
            " && github.repository == 'HuGR-Labs/corelink-server'",
            " || github.repository == 'HuGR-Labs/corelink-server'",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_always_or_trust_predicate_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/mutation-nightly.yml",
            "if: always() && github.repository",
            "if: always() || github.repository",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_or_bypass_in_oidc_guard_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/cas_foundation.yml",
            " && github.ref_protected",
            " || github.ref_protected",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_multiline_or_bypass_in_every_trusted_guard_is_red(self) -> None:
        mutations = (
            (
                ".github/workflows/cas_foundation.yml",
                "if: github.event_name == 'workflow_dispatch' && github.repository == 'HuGR-Labs/corelink-server' && github.ref == 'refs/heads/main' && github.ref_protected",
            ),
            (
                ".github/workflows/mutation-nightly.yml",
                "if: always() && github.repository == 'HuGR-Labs/corelink-server' && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.ref_protected",
            ),
            (
                ".github/workflows/semgrep.yml",
                "if: github.repository == 'HuGR-Labs/corelink-server' && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.ref_protected",
            ),
        )
        for target, old in mutations:
            with self.subTest(target=target):
                mutation = self._mutated_text(
                    target,
                    old,
                    old + "\n      || github.event_name == 'workflow_dispatch'",
                )
                with mutation:
                    with self.assertRaises(verifier.CheckError):
                        verifier.verify_b110()

    def test_folded_multiline_or_bypass_is_red(self) -> None:
        target = ".github/workflows/semgrep.yml"
        old = "if: github.repository == 'HuGR-Labs/corelink-server' && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.ref_protected"
        folded = (
            "if: >-\n"
            "      github.repository == 'HuGR-Labs/corelink-server' && github.event_name == 'workflow_dispatch' &&\n"
            "      github.ref == 'refs/heads/main' && github.ref_protected ||\n"
            "      github.event_name == 'workflow_dispatch'"
        )
        mutation = self._mutated_text(target, old, folded)
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_ref_split_or_cancelling_heavy_build_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/cas_foundation.yml",
            'group: "corelink-heavy-cargo-build"',
            'group: "corelink-heavy-cargo-build-${{ github.ref }}"',
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_dead_write_permission_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/coverage.yml",
            "  contents: read\n",
            "  contents: read\n  pull-requests: write\n",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_semgrep_permission_expansion_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/semgrep.yml",
            "  contents: read\n",
            "  contents: read\n  actions: read\n",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_unguarded_corelink_job_with_write_permission_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/ffi-matrix-ci.yml",
            "  contents: read\n",
            "  contents: write\n",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()

    def test_inline_write_permission_on_new_corelink_job_is_red(self) -> None:
        mutation = self._mutated_text(
            ".github/workflows/coverage.yml",
            "  coverage:\n",
            "  injected-write:\n"
            "    runs-on: corelink\n"
            "    permissions: { contents: write }\n"
            "    steps:\n"
            "      - run: echo injected\n"
            "  coverage:\n",
        )
        with mutation:
            with self.assertRaises(verifier.CheckError):
                verifier.verify_b110()


if __name__ == "__main__":
    unittest.main()
