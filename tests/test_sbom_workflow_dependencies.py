"""Regression tests for the SBOM verifier's isolated dependency boundary."""

from __future__ import annotations

import json
import re
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "sbom.yml"
PYTHON_TESTS_WORKFLOW = ROOT / ".github" / "workflows" / "python-tests.yml"
SBOM_REQUIREMENTS = ROOT / "requirements-sbom.txt"
CI_REQUIREMENTS = ROOT / "requirements-ci.txt"

SBOM_TRIGGER_CENSUS = {
    "requirements-sbom.txt",
    ".github/workflows/sbom.yml",
    "tests/verify_rust_sbom.py",
    "tests/test_sbom_workflow_dependencies.py",
}


def _has_complete_sbom_census(workflow: str) -> bool:
    return all(workflow.count(f"      - '{path}'") == 2 for path in SBOM_TRIGGER_CENSUS)


class SbomWorkflowDependencyTest(unittest.TestCase):
    def test_committed_sbom_preserves_lock_population_and_inactive_entries(self) -> None:
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))["package"]
        sbom = json.loads((ROOT / ".sbom" / "cyclonedx-rust.json").read_text(encoding="utf-8"))
        self.assertEqual(len(lock), 652)
        self.assertEqual(len(sbom["components"]), 652)
        inactive = {
            "deadpool-postgres",
            "der_derive",
            "flagset",
            "tls_codec",
            "tls_codec_derive",
            "tokio-postgres-rustls",
            "x509-cert",
        }
        self.assertEqual(sum(package["name"] in inactive for package in lock), 7)

    def test_sbom_lock_is_the_complete_hashed_runtime_closure(self) -> None:
        requirements = SBOM_REQUIREMENTS.read_text(encoding="utf-8")
        entries = re.findall(
            r"(?m)^([A-Za-z0-9_.-]+)==([0-9.]+) \\\n"
            r"    --hash=sha256:([0-9a-f]{64})$",
            requirements,
        )
        self.assertEqual(
            entries,
            [
                (
                    "license-expression",
                    "30.4.4",
                    "421788fdcadb41f049d2dc934ce666626265aeccefddd25e162a26f23bcbf8a4",
                ),
                (
                    "boolean.py",
                    "5.0",
                    "ef28a70bd43115208441b53a045d1549e2f0ec6e3d08a9d142cbc41c1938e8d9",
                ),
            ],
        )

    def test_sbom_job_uses_only_job_private_hashed_venv(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn('python3 -m venv "$SBOM_VENV"', workflow)
        self.assertIn('SBOM_VENV="${RUNNER_TEMP}/corelink-sbom-venv"', workflow)
        self.assertIn('"$SBOM_PYTHON" -m pip install', workflow)
        self.assertIn("--require-hashes", workflow)
        self.assertIn("--only-binary=:all:", workflow)
        self.assertIn("--no-deps", workflow)
        self.assertIn("-r requirements-sbom.txt", workflow)
        self.assertIn('"$SBOM_PYTHON" tests/verify_rust_sbom.py --check', workflow)
        self.assertNotIn("python3 -m pip install --quiet -r requirements-ci.txt", workflow)

    def test_regular_pr_trigger_has_exact_sbom_input_census(self) -> None:
        workflow = PYTHON_TESTS_WORKFLOW.read_text(encoding="utf-8")
        self.assertTrue(_has_complete_sbom_census(workflow))
        for path in SBOM_TRIGGER_CENSUS:
            with self.subTest(path=path):
                self.assertEqual(workflow.count(f"      - '{path}'"), 2)
        self.assertIn(".venv/bin/python3 tests/verify_rust_sbom.py --check", workflow)
        self.assertIn("tests/*.py", workflow)
        self.assertIn("SBOM dependency teeth file was not collected", workflow)

    def test_census_mutation_removes_each_load_bearing_entry(self) -> None:
        """Removing any exact census line must make the census contract fail."""
        workflow = PYTHON_TESTS_WORKFLOW.read_text(encoding="utf-8")
        for path in SBOM_TRIGGER_CENSUS:
            with self.subTest(path=path):
                mutant = workflow.replace(f"      - '{path}'\n", "")
                self.assertFalse(_has_complete_sbom_census(mutant))

    def test_general_ci_requirements_do_not_pull_sbom_parser(self) -> None:
        requirements = CI_REQUIREMENTS.read_text(encoding="utf-8")
        self.assertNotIn("license-expression", requirements)
        self.assertNotIn("boolean.py", requirements)


if __name__ == "__main__":
    unittest.main()
