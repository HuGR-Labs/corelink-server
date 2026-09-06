from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b098_repo_hygiene.py"
spec = importlib.util.spec_from_file_location("b098_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B098VerifierTests(unittest.TestCase):
    def test_repository_is_open_only_for_the_missing_external_release(self) -> None:
        result = verifier.audit(ROOT)
        self.assertEqual(result.issues, ())
        self.assertEqual(result.status, "open")
        self.assertEqual(result.semver_tags, ())
        self.assertEqual(result.package_count, 95)
        self.assertEqual(result.crate_dir_count, 75)
        self.assertEqual(result.okf_count, 170)
        self.assertEqual((result.specs_schema_count, result.specs_yaml_only_count), (479, 11))

    def test_lint_inheritance_mutation_reopens_the_guard(self) -> None:
        tracker = (ROOT / "crates/corelink-runbook-tracker/Cargo.toml").read_text()
        mutated = tracker.replace(
            "[lints]\nworkspace = true",
            '[lints.rust]\nunsafe_code = "forbid"',
            1,
        )
        result = verifier.audit(ROOT, tracker_text=mutated)
        self.assertTrue(any("inherit" in issue for issue in result.issues))
        self.assertTrue(any("local rust/clippy" in issue for issue in result.issues))

    def test_each_documented_population_count_is_mutation_sensitive(self) -> None:
        claude = (ROOT / "CLAUDE.md").read_text()
        mutated = claude.replace("**170 OKF concepts**", "**169 OKF concepts**", 1)
        result = verifier.audit(ROOT, claude_text=mutated)
        self.assertTrue(any("okf_count" in issue for issue in result.issues))

    def test_semver_classifier_does_not_confuse_cli_tags_with_release_tags(self) -> None:
        self.assertTrue(verifier.SEMVER_TAG.fullmatch("v1.0.0"))
        self.assertTrue(verifier.SEMVER_TAG.fullmatch("v1.2.3-rc.1"))
        self.assertFalse(verifier.SEMVER_TAG.fullmatch("cli-v0.1.0"))
        self.assertFalse(verifier.SEMVER_TAG.fullmatch("v1.0"))
        self.assertFalse(verifier.SEMVER_TAG.fullmatch("v01.2.3"))

    def test_missing_population_guard_fails_closed(self) -> None:
        claude = (ROOT / "CLAUDE.md").read_text()
        with self.assertRaises(verifier.VerificationError):
            verifier.audit(ROOT, claude_text=claude.replace("**95 Rust packages**", "95 packages", 1))

    def test_cli_reports_open_without_mutating_refs(self) -> None:
        before = subprocess.check_output(["git", "show-ref", "--tags"], cwd=ROOT, text=True)
        completed = subprocess.run(
            [sys.executable, str(SCRIPT)], cwd=ROOT, text=True, capture_output=True, check=False
        )
        self.assertEqual(completed.returncode, 0)
        self.assertIn("B-098 open", completed.stdout)
        after = subprocess.check_output(["git", "show-ref", "--tags"], cwd=ROOT, text=True)
        self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
