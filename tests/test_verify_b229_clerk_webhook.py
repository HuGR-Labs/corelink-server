import tempfile
import unittest
from pathlib import Path

from scripts.verify_b229_clerk_webhook import MUTATIONS, verify_mutations, verify_repo


class B229ClerkWebhookEvidenceTests(unittest.TestCase):
    def test_evidence_is_complete(self):
        verify_repo()

    def test_mutations_fail_closed(self):
        self.assertEqual(verify_mutations(), len(MUTATIONS))

    def test_missing_evidence_fails_closed(self):
        with self.assertRaises(Exception):
            verify_repo(Path(tempfile.gettempdir()) / "b229-evidence-does-not-exist")


if __name__ == "__main__":
    unittest.main()
