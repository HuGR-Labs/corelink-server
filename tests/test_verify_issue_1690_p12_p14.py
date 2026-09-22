from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))

import verify_issue_1690_p12_p14 as verifier  # noqa: E402


MANIFEST = json.loads((REPO / "scripts/issue_1690_p12_p14_provenance.json").read_text())
HEAD = "a" * 40


class Issue1690VerifierContractTests(unittest.TestCase):
    def run_verifier(self, current_tree: str) -> dict:
        squash = MANIFEST["squash_provenance"]
        merge = squash["merge_commit"]
        paths = squash["path_blobs"]
        merge_paths = sorted(paths)

        def fake_git(*args: str) -> str:
            if args[:2] == ("rev-parse", f"{merge}^"):
                return squash["merge_parent"]
            if args[:3] == ("show", "-s", "--format=%T"):
                return squash["tree"] if args[3] == merge else current_tree
            if args[:4] == ("diff-tree", "--no-commit-id", "--name-only", "-r"):
                return "\n".join(merge_paths if args[4] == merge else [])
            if args[0] == "rev-parse" and ":" in args[1]:
                _, path = args[1].split(":", 1)
                return paths[path]
            self.fail(f"unexpected git invocation: {args!r}")

        def fake_git_optional(*args: str) -> str | None:
            return fake_git(*args)

        def fake_commit_exists(commit: str) -> bool:
            return commit == merge

        def fake_ancestor(commit: str, head: str) -> bool:
            return commit == merge and head == HEAD

        with (
            patch.object(verifier, "git", side_effect=fake_git),
            patch.object(verifier, "git_optional", side_effect=fake_git_optional),
            patch.object(verifier, "commit_exists", side_effect=fake_commit_exists),
            patch.object(verifier, "ancestor", side_effect=fake_ancestor),
        ):
            return verifier.verify(json.loads(json.dumps(MANIFEST)), HEAD, HEAD)

    def test_exact_tree_match_passes_with_immutable_sha_receipt(self) -> None:
        report = self.run_verifier(MANIFEST["historical_test"]["tree"])
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["head_sha"], HEAD)
        self.assertEqual(report["github_sha"], HEAD)
        self.assertEqual(report["immutable_sha"], HEAD)
        self.assertEqual(report["expected_tree"], MANIFEST["historical_test"]["tree"])
        self.assertEqual(report["observed_tree"], MANIFEST["historical_test"]["tree"])
        self.assertTrue(report["tree_matches"])
        self.assertTrue(report["contract"]["historical_tree_matches_current_main"])

    def test_stale_historical_tree_fails_but_retains_tree_and_ancestry(self) -> None:
        current_tree = "b" * 40
        report = self.run_verifier(current_tree)
        self.assertEqual(report["status"], "FAIL_EXACT_TREE_MISMATCH")
        self.assertEqual(report["expected_tree"], MANIFEST["historical_test"]["tree"])
        self.assertEqual(report["observed_tree"], current_tree)
        self.assertFalse(report["tree_matches"])
        self.assertEqual(report["contract"]["historical_test_tree"], MANIFEST["historical_test"]["tree"])
        self.assertEqual(report["contract"]["current_main_tree"], current_tree)
        self.assertFalse(report["contract"]["historical_tree_matches_current_main"])
        self.assertTrue(report["contract"]["squash_merge_is_ancestor"])
        self.assertEqual(len(report["path_inventory"]), 11)

    def test_unavailable_historical_objects_are_reported_not_used_as_a_skip(self) -> None:
        report = self.run_verifier("b" * 40)
        self.assertEqual(set(report["historical_source_object_evidence"]), {"p12", "p13", "p14"})
        self.assertTrue(
            all(not row["object_present_in_hosted_checkout"] for row in report["historical_source_object_evidence"].values())
        )
        self.assertEqual(set(report["squash_source_commits"]), {"p12", "p13", "p14"})

    def test_head_mismatch_is_rejected(self) -> None:
        with self.assertRaisesRegex(verifier.VerificationError, "differs from GITHUB_SHA"):
            verifier.verify(MANIFEST, HEAD, "c" * 40)

    def test_mutated_historical_tree_anchor_is_rejected(self) -> None:
        mutated = json.loads(json.dumps(MANIFEST))
        mutated["historical_test"]["tree"] = "d" * 40
        with self.assertRaisesRegex(verifier.VerificationError, "historical tree differs from the verifier anchor"):
            verifier.verify(mutated, HEAD, HEAD)

    def test_manual_lane_uploads_a_sha_bound_receipt_even_on_failure(self) -> None:
        workflow = (REPO / ".github/workflows/issue-1690-p12-p14-verifier.yml").read_text()
        self.assertIn("workflow_dispatch:", workflow)
        self.assertNotIn("schedule:", workflow)
        self.assertIn("if: always()", workflow)
        self.assertIn("p12-p14-main-verification-${{ github.run_id }}", workflow)
        self.assertIn("--github-sha \"${GITHUB_SHA}\"", workflow)
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        self.assertNotIn("continue-on-error:", workflow)


if __name__ == "__main__":
    unittest.main(verbosity=2)
