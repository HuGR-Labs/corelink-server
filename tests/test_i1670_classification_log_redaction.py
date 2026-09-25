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


if __name__ == "__main__":
    unittest.main()
