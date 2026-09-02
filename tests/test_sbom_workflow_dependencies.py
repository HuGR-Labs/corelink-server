"""Regression tests for the SBOM verifier's isolated dependency boundary."""

from __future__ import annotations

import json
from fnmatch import fnmatchcase
import importlib.util
import re
import subprocess
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "sbom.yml"
PYTHON_TESTS_WORKFLOW = ROOT / ".github" / "workflows" / "python-tests.yml"
SBOM_REQUIREMENTS = ROOT / "requirements-sbom.txt"
CI_REQUIREMENTS = ROOT / "requirements-ci.txt"

SBOM_TRIGGER_CENSUS = {
    "Cargo.lock",
    "Cargo.toml",
    ".sbom/cyclonedx-rust.json",
    "requirements-sbom.txt",
    ".github/workflows/sbom.yml",
    "tests/verify_rust_sbom.py",
    "tests/test_sbom_workflow_dependencies.py",
}
WORKSPACE_MANIFEST_GLOB = "**/Cargo.toml"


def _has_complete_sbom_census(workflow: str) -> bool:
    return all(workflow.count(f"      - '{path}'") == 2 for path in SBOM_TRIGGER_CENSUS)


def _workspace_manifest_paths() -> set[str]:
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
            cwd=ROOT,
            text=True,
        )
    )
    return {
        str(Path(package["manifest_path"]).resolve().relative_to(ROOT.resolve())).replace("\\", "/")
        for package in metadata["packages"]
    }


def _workflow_path_entries(workflow: str) -> set[str]:
    return set(re.findall(r"(?m)^      - '([^']+)'$", workflow))


def _matches_workflow_path(pattern: str, path: str) -> bool:
    # GitHub's `**/Cargo.toml` includes nested manifests. `fnmatchcase` does
    # not treat `**/` as matching zero directories, so retain the explicit
    # root anchor and model the nested part here.
    if pattern == WORKSPACE_MANIFEST_GLOB:
        return path == "Cargo.toml" or path.endswith("/Cargo.toml")
    return fnmatchcase(path, pattern)


def _has_complete_workspace_manifest_coverage(workflow: str) -> bool:
    entries = _workflow_path_entries(workflow)
    return all(
        any(_matches_workflow_path(pattern, path) for pattern in entries)
        for path in _workspace_manifest_paths()
    )


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

    def test_regular_pr_trigger_covers_every_workspace_manifest(self) -> None:
        workflow = PYTHON_TESTS_WORKFLOW.read_text(encoding="utf-8")
        manifests = _workspace_manifest_paths()
        self.assertGreater(len(manifests), 1)
        self.assertEqual(workflow.count(f"      - '{WORKSPACE_MANIFEST_GLOB}'"), 2)
        self.assertTrue(_has_complete_workspace_manifest_coverage(workflow))
        # A future member is covered without requiring a hand-edited allowlist.
        self.assertTrue(_matches_workflow_path(WORKSPACE_MANIFEST_GLOB, "crates/future-member/Cargo.toml"))

    def test_workspace_manifest_trigger_mutation_is_rejected(self) -> None:
        """Removing the recursive trigger must expose a nested member gap."""
        workflow = PYTHON_TESTS_WORKFLOW.read_text(encoding="utf-8")
        mutant = workflow.replace(f"      - '{WORKSPACE_MANIFEST_GLOB}'\n", "")
        self.assertFalse(_has_complete_workspace_manifest_coverage(mutant))
        self.assertFalse(_matches_workflow_path("Cargo.toml", "crates/future-member/Cargo.toml"))

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

    def test_checksum_mutation_is_rejected(self) -> None:
        spec = importlib.util.spec_from_file_location("verify_rust_sbom", ROOT / "tests" / "verify_rust_sbom.py")
        self.assertIsNotNone(spec)
        verifier = importlib.util.module_from_spec(spec)
        self.assertIsNotNone(spec.loader)
        spec.loader.exec_module(verifier)
        sbom = json.loads((ROOT / ".sbom" / "cyclonedx-rust.json").read_text(encoding="utf-8"))
        mutated = next(
            property_
            for component in sbom["components"]
            for property_ in component["properties"]
            if property_["name"] == "corelink:cargo:checksum"
        )
        mutated["value"] = "0" * 64
        with self.assertRaises(SystemExit):
            verifier.verify(sbom)


if __name__ == "__main__":
    unittest.main()
