"""Focused static adversarial controls for issue #2581's writer map."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_issue_2581_dsr_audit", ROOT / "scripts/verify_issue_2581_dsr_audit.py"
)
VERIFY = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(VERIFY)


class Issue2581DsrAuditTests(unittest.TestCase):
    def test_census_and_context_bound_writer_guards_pass(self):
        self.assertTrue(all(VERIFY.required.values()))

    def test_contextless_compatibility_is_kept_for_shared_seams(self):
        self.assertIn("self.emit_attributed(record, None)", VERIFY.AUDIT)
        self.assertIn("return self.set_outcome_snapshot(dsr_id, outcome_json)", VERIFY.LEDGER)

    def test_raw_context_and_identity_are_not_logged_as_ownership_receipts(self):
        self.assertIn("receipt references are produced", VERIFY.CENSUS.lower())
        self.assertNotIn("nonce_digest", VERIFY.CENSUS)
        self.assertNotIn("personal email", VERIFY.CENSUS.lower())


if __name__ == "__main__":
    unittest.main()
