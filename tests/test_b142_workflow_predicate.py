"""B-142 semantic runner/job predicate and mutation tests."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b142_workflows as contract  # noqa: E402


class B142WorkflowPredicateTest(unittest.TestCase):
    def _mutated_root(self, relative: str, old: str, new: str, suffix: str = "") -> Path:
        directory = Path(tempfile.mkdtemp(prefix="b142-workflow-"))
        target = directory / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        source = (ROOT / relative).read_text(encoding="utf-8")
        self.assertIn(old, source)
        target.write_text(source.replace(old, new, 1) + suffix, encoding="utf-8")
        return directory

    def test_current_codeql_and_secrets_drift_population_passes(self) -> None:
        observed = contract.verify(ROOT)
        self.assertEqual(len(observed), 4)
        self.assertEqual(observed[".github/workflows/codeql.yml:analyze"], "ubuntu-latest")
        self.assertEqual(observed[".github/workflows/secrets-drift.yml:secrets-drift"], "corelink")

    def test_runner_label_mutation_is_rejected(self) -> None:
        root = self._mutated_root(
            ".github/workflows/codeql.yml", "runs-on: ubuntu-latest", "runs-on: self-hosted"
        )
        with self.assertRaises(contract.ContractError):
            contract.check_workflow(root, ".github/workflows/codeql.yml", "analyze", "ubuntu-latest")

    def test_missing_job_and_comment_bait_are_rejected(self) -> None:
        root = self._mutated_root(
            ".github/workflows/secrets-drift.yml",
            "  secrets-drift:\n",
            "  renamed-secrets-drift:\n",
            "\n# secrets-drift:\n#   runs-on: corelink\n",
        )
        with self.assertRaises(contract.ContractError):
            contract.check_workflow(root, ".github/workflows/secrets-drift.yml", "secrets-drift", "corelink")

    def test_watchdog_hosted_mutation_is_rejected(self) -> None:
        root = self._mutated_root(
            ".github/workflows/secrets-drift-evidence-watchdog.yml",
            contract.HOSTED_FALLBACK,
            "ubuntu-latest",
        )
        with self.assertRaises(contract.ContractError):
            contract.check_workflow(
                root,
                ".github/workflows/secrets-drift-evidence-watchdog.yml",
                "inspect",
                "corelink-fallback",
            )


if __name__ == "__main__":
    unittest.main()
