from __future__ import annotations

import runpy
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class CasOwnershipWriterBoundaryTests(unittest.TestCase):
    def test_owned_writer_census_and_fail_closed_context_flow(self) -> None:
        verifier = runpy.run_path(str(ROOT / "scripts/verify_issue_2579_cas_ownership.py"))
        verifier["verify"]()

    def test_only_cas_reference_class_is_mapped_in_this_family(self) -> None:
        source = (ROOT / "crates/corelink-container/src/storage/cas_write_fence.rs").read_text()
        self.assertIn("StagingLoadTestResourceClass::CasReference", source)
        self.assertNotIn("StagingLoadTestResourceClass::ByokArtifact", source)
        self.assertNotIn("StagingLoadTestResourceClass::DsrArtifact", source)

    def test_issue_proof_pack_is_the_only_new_non_rust_scope(self) -> None:
        workflow = (ROOT / ".github/workflows/issue-2579-cas-ownership.yml").read_text()
        self.assertIn("EXPECTED_HEAD", workflow)
        self.assertIn("EXPECTED_BASE", workflow)
        self.assertIn("cargo check --locked -p corelink-server --lib", workflow)
        self.assertIn("cas_write_fence::tests::retry_after_r2_failure_reuses_verified_durable_handle_intent", workflow)
        self.assertIn("tests/test_issue_2579_cas_ownership.py", workflow)


if __name__ == "__main__":
    unittest.main()
