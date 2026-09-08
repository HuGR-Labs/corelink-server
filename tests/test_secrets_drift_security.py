"""Focused security tests for the self-hosted secrets-drift boundary."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "secrets-drift.yml"
TRUSTED_VALIDATOR = REPO_ROOT / "scripts" / "validate_secrets_matrix.py"
TRUSTED_BASH_VERIFIER = REPO_ROOT / "scripts" / "secrets-checklist-verify.sh"


class SecretsDriftSecurityTests(unittest.TestCase):
    def test_workflow_checks_out_trusted_base_and_never_runs_candidate_scripts(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("pull_request_target:", text)
        self.assertIn("path: .trusted", text)
        self.assertIn("path: .candidate", text)
        self.assertEqual(text.count("persist-credentials: false"), 2)
        for script in (
            "validate_secrets_matrix.py",
            "check-env-contract.py",
            "secrets-checklist-verify.sh",
        ):
            self.assertIn(f'"${{GITHUB_WORKSPACE}}/.trusted/scripts/{script}"', text)
        self.assertNotIn("python3 scripts/validate_secrets_matrix.py", text)
        self.assertNotIn("python3 scripts/check-env-contract.py", text)
        self.assertNotIn("bash scripts/secrets-checklist-verify.sh", text)
        self.assertIn(
            "github.event_name != 'workflow_dispatch' || github.ref == 'refs/heads/main'",
            text,
        )

    def test_adversarial_candidate_script_is_data_only(self) -> None:
        """A PR replacement of the validator cannot execute on the runner."""
        with tempfile.TemporaryDirectory() as temp:
            candidate = Path(temp)
            (candidate / "docs/internal").mkdir(parents=True)
            (candidate / "scripts").mkdir()
            (candidate / "docs/internal/secrets-checklist.md").write_text(
                "# empty candidate matrix\n", encoding="utf-8"
            )
            (candidate / ".github/workflows").mkdir(parents=True)
            (candidate / ".github/workflows/perf-production-evidence.yml").write_text(
                "env: { CORELINK_FRESH_SESSION: ${{ secrets.CORELINK_FRESH_SESSION }}, "
                "CORELINK_PERF_PAT: ${{ secrets.CORELINK_PERF_PAT }}}\n",
                encoding="utf-8",
            )
            (candidate / "scripts/collect_b102_b107_measurements.py").write_text(
                "# trusted manifest fixture\nCORELINK_FRESH_SESSION\nCORELINK_PERF_PAT\n",
                encoding="utf-8",
            )
            (candidate / "scripts/collect_b105_same_lane.py").write_text(
                "# trusted manifest fixture\nCORELINK_PERF_PAT\n", encoding="utf-8"
            )
            marker = candidate / "candidate-script-executed"
            (candidate / "scripts/validate_secrets_matrix.py").write_text(
                f"from pathlib import Path\nPath({str(marker)!r}).touch()\nraise SystemExit(97)\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    sys.executable,
                    str(TRUSTED_VALIDATOR),
                    "--repo-root",
                    str(candidate),
                    "--dry-run",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(marker.exists(), result.stdout)

    def test_synthetic_corelink_http_exclusions_are_path_scoped(self) -> None:
        """The raw-curl fixture is exempt, but a production mutation is not."""
        names = (
            "CORELINK_HTTP_PORT_FILE",
            "CORELINK_HTTP_REQUEST_FILE",
            "CORELINK_HTTP_STATUS",
        )
        source = "\n".join(f"console.log(process.env.{name});" for name in names) + "\n"
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fixture = root / "apps/docs/tests/fixtures/raw-curl-http-server.mjs"
            fixture.parent.mkdir(parents=True)
            fixture.write_text(source, encoding="utf-8")
            (root / "docs/internal").mkdir(parents=True)
            (root / "docs/internal/secrets-checklist.md").write_text(
                "".join(
                    f"| {index} | fixture-only | `UNRELATED_FIXTURE_KEY_{index}` |\n"
                    for index in range(1, 21)
                ),
                encoding="utf-8",
            )

            result = subprocess.run(
                ["bash", str(TRUSTED_BASH_VERIFIER), "--repo-root", str(root)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

            production = root / "apps/admin-ui/src/http-leak.mjs"
            production.parent.mkdir(parents=True)
            production.write_text(source, encoding="utf-8")
            result = subprocess.run(
                ["bash", str(TRUSTED_BASH_VERIFIER), "--repo-root", str(root)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            for name in names:
                self.assertIn(name, result.stderr)


if __name__ == "__main__":
    unittest.main()
