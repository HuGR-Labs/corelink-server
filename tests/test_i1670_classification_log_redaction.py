import importlib.util
import re
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "redact_i1670_classification_log",
    ROOT / "scripts/redact_i1670_classification_log.py",
)
assert SPEC and SPEC.loader
redactor = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = redactor
SPEC.loader.exec_module(redactor)


class I1670ClassificationLogRedactionTests(unittest.TestCase):
    def test_removes_tmpfs_and_linker_paths_while_preserving_failure_evidence(self) -> None:
        raw = """\
dd: error writing '/dev/shm/corelink-issue-1670-36111499390/write.bin': No space left on device
classification: ENOSPC
summary: runner storage exhausted; original exit 1 is preserved
::warning title=Infrastructure failure::ENOSPC; original gate status preserved
/usr/bin/ld: final link failed: No space left on device
collect2: error: ld returned 1 exit status
"""

        redacted = redactor.redact_paths(raw)

        self.assertNotIn("/dev/shm/corelink-issue-1670-36111499390/write.bin", redacted)
        self.assertNotIn("/usr/bin/ld", redacted)
        self.assertIsNone(re.search(r"(?<![A-Za-z0-9_./:])/(?:[A-Za-z0-9_.@+-]+/)*[A-Za-z0-9_.@+-]+", redacted))
        self.assertIn("error writing '<redacted-path>': No space left on device", redacted)
        self.assertIn("classification: ENOSPC", redacted)
        self.assertIn("original exit 1 is preserved", redacted)
        self.assertIn("::warning title=Infrastructure failure::ENOSPC", redacted)
        self.assertIn("final link failed: No space left on device", redacted)
        self.assertIn("ld returned 1 exit status", redacted)

    def test_preserves_urls_and_non_path_classification_text(self) -> None:
        line = "classification: TEST_FAILURE; docs https://example.test/a/b"

        self.assertEqual(redactor.redact_paths(line), line)

    def test_hosted_workflow_publishes_redacted_logs_and_no_filesystem_location(self) -> None:
        workflow = (ROOT / ".github/workflows/issue-1670-hosted-classification.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("scripts/redact_i1670_classification_log.py", workflow)
        self.assertNotIn('"filesystem": "/dev/shm tmpfs"', workflow)
        self.assertNotIn('echo "- bounded filesystem: /dev/shm', workflow)
        self.assertNotIn('cp "$enospc_log" "$GITHUB_WORKSPACE/issue-1670-enospc.log"', workflow)
        self.assertNotIn('cp "$linker_log" "$GITHUB_WORKSPACE/issue-1670-linker.log"', workflow)

    def test_hosted_receipt_and_summary_omit_runner_identity(self) -> None:
        workflow = (ROOT / ".github/workflows/issue-1670-hosted-classification.yml").read_text(
            encoding="utf-8"
        )

        self.assertNotIn('"runner":', workflow)
        self.assertNotIn('echo "- runner:', workflow)
        self.assertIn('assert "runner" not in report', workflow)
        self.assertIn('assert "runner" not in summary.casefold()', workflow)

    def test_campaign_fixture_publishes_redacted_run_bound_capacity_receipt(self) -> None:
        workflow = (ROOT / ".github/workflows/campaign-ci.yml").read_text(encoding="utf-8")
        gate = (ROOT / ".github/workflows/issue-1670-receipt-redaction.yml").read_text(
            encoding="utf-8"
        )
        start = workflow.index("      - name: i1670 bounded ENOSPC and linker classification")
        end = workflow.index("      - name: i1680 credentialless schema", start)
        lane = workflow[start:end]

        self.assertIn(".github/workflows/campaign-ci.yml", gate)
        self.assertIn("python3 scripts/redact_i1670_classification_log.py", lane)
        self.assertIn('--input "$root/enospc.raw.log"', lane)
        self.assertIn('--input "$root/linker.raw.log"', lane)
        self.assertIn('--output artifacts/campaign-ci/i1670/enospc.log', lane)
        self.assertIn('--output artifacts/campaign-ci/i1670/linker.log', lane)
        self.assertNotIn('>artifacts/campaign-ci/i1670/enospc.log', lane)
        self.assertNotIn('>artifacts/campaign-ci/i1670/linker.log', lane)
        self.assertIn("GITHUB_STEP_SUMMARY=/dev/null CORELINK_CLASSIFICATION_ARTIFACT=", lane)
        self.assertNotIn('"runner":', lane)
        self.assertNotIn('"filesystem":', lane)
        self.assertIn('run_id = int(os.environ["I1670_RUN_ID"])', lane)
        self.assertIn('"run_id": run_id', lane)
        self.assertIn('"commit_sha": commit_sha', lane)
        self.assertIn('"fixture_removed_before_post_measurement": True', lane)
        self.assertIn('"cleanup": {"fixture_empty": True}', lane)
        self.assertLess(lane.index('rm -rf "$fs_root"\n          test ! -e "$fs_root"'), lane.index('shm_post_free='))


if __name__ == "__main__":
    unittest.main()
