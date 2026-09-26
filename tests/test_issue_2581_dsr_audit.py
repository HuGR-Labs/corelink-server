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

    def test_rectification_keeps_retained_audit_before_domain_update(self):
        (
            audit_precedes_update,
            _,
            returns_updated_rows,
        ) = VERIFY.rectification_contract(VERIFY.ACCESS)
        self.assertTrue(audit_precedes_update)
        self.assertTrue(returns_updated_rows)

    def test_rectification_guard_rejects_disposable_batch_registration(self):
        unsafe = (
            "pub(super) fn run_rectification(\n"
            "    audit_dsr_event(d1);\n"
            "    D1BatchStatement::new(&plan.sql, params);\n"
            "    StagingLoadTestResourceClass::DsrArtifact;\n"
            "}\n\n#[cfg(test)]\n#[allow("
        )
        (
            audit_precedes_update,
            no_disposable_batch,
            returns_updated_rows,
        ) = VERIFY.rectification_contract(unsafe)
        self.assertFalse(audit_precedes_update)
        self.assertFalse(no_disposable_batch)
        self.assertFalse(returns_updated_rows)


if __name__ == "__main__":
    unittest.main()
