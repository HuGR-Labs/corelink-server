#!/usr/bin/env python3
"""Focal, stdlib-only contract tests for the B-126 validator."""

from __future__ import annotations

import importlib.util
import re
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).parents[1] / "scripts" / "validate_file_size_ratchet.py"
SPEC = importlib.util.spec_from_file_location("file_size_ratchet", SCRIPT)
assert SPEC and SPEC.loader
ratchet = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = ratchet
SPEC.loader.exec_module(ratchet)


class BaselineContractTests(unittest.TestCase):
    def test_plus_one_growth_is_blocked(self) -> None:
        bad, policy = ratchet.evaluate(
            {"old.rs": 1001}, {"old.rs": 1001}, {"old.rs": 1002}
        )
        self.assertEqual(bad, [])
        self.assertTrue(any("CRESCEU" in item for item in policy))

    def test_new_1001_file_is_blocked(self) -> None:
        bad, policy = ratchet.evaluate({}, {}, {"new.rs": 1001})
        self.assertEqual(bad, [])
        self.assertTrue(any("ARQUIVO NOVO" in item for item in policy))

    def test_baseline_addition_is_blocked(self) -> None:
        bad = ratchet.validate_baseline_transition(
            {"old.rs": 1001}, {"old.rs": 1001, "new.rs": 1001}, {"old.rs": 1001, "new.rs": 1001}
        )
        self.assertTrue(any("entrada nova" in item for item in bad))

    def test_baseline_increase_is_blocked(self) -> None:
        bad = ratchet.validate_baseline_transition(
            {"old.rs": 1001}, {"old.rs": 1002}, {"old.rs": 1002}
        )
        self.assertTrue(any("aumentou" in item for item in bad))

    def test_baseline_duplicate_is_indeterminate(self) -> None:
        with self.assertRaises(ratchet.MeasurementError):
            ratchet.parse_baseline("1001\told.rs\n1002\told.rs\n")

    def test_baseline_missing_row_is_blocked_until_graduation(self) -> None:
        bad = ratchet.validate_baseline_transition({"old.rs": 1001}, {}, {"old.rs": 1001})
        self.assertTrue(any("removida antes" in item for item in bad))
        self.assertEqual(ratchet.validate_baseline_transition({"old.rs": 1001}, {}, {"old.rs": 1000}), [])

    def test_baseline_malformed_is_indeterminate(self) -> None:
        with self.assertRaises(ratchet.MeasurementError):
            ratchet.parse_baseline("not-a-row\n")

    def test_baseline_traversal_is_indeterminate(self) -> None:
        with self.assertRaises(ratchet.MeasurementError):
            ratchet.parse_baseline("1001\t../outside.rs\n")

    def test_baseline_non_source_is_indeterminate(self) -> None:
        with self.assertRaises(ratchet.MeasurementError):
            ratchet.parse_baseline("1001\tREADME.md\n")

    def test_trusted_base_floor_allows_inherited_drift_but_not_new_growth(self) -> None:
        bad, policy = ratchet.evaluate(
            {"old.rs": 1001},
            {"old.rs": 1001},
            {"old.rs": 1100},
            trusted_base_counts={"old.rs": 1100},
        )
        self.assertEqual(bad, [])
        self.assertEqual(policy, [])
        _bad, policy = ratchet.evaluate(
            {"old.rs": 1001},
            {"old.rs": 1001},
            {"old.rs": 1101},
            trusted_base_counts={"old.rs": 1100},
        )
        self.assertTrue(any("CRESCEU" in item for item in policy))


class SourceAndTrustBoundaryTests(unittest.TestCase):
    def test_workflow_covers_fork_pull_requests_without_skipping_the_gate(self) -> None:
        workflow = (SCRIPT.parents[1] / ".github/workflows/file-size-ratchet.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("pull_request_target:", workflow)
        self.assertIn("push:", workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("      - '**'", workflow)
        self.assertNotIn("author_association", workflow)
        self.assertIn("head.repo.full_name", workflow)
        self.assertIn("fetch-depth: 0", workflow)
        self.assertIn("persist-credentials: false", workflow)
        self.assertIn("BASE_REF", workflow)
        self.assertIn("HEAD_REF", workflow)
        base_assignments = [
            line.split(":", 1)[1].strip()
            for line in workflow.splitlines()
            if line.lstrip().startswith("BASE_REF:")
        ]
        self.assertEqual(
            base_assignments,
            ["${{ github.event.pull_request.base.sha || github.event.before || github.sha }}"],
        )
        validator_shows = [
            line.strip()
            for line in workflow.splitlines()
            if re.search(r"\bgit\s+show\b", line)
            and "validate_file_size_ratchet.py" in line
        ]
        self.assertEqual(
            validator_shows,
            [
                'git show "${BASE_REF}:scripts/validate_file_size_ratchet.py" > '
                '"$RUNNER_TEMP/validate_file_size_ratchet.py"'
            ],
        )

    def test_nul_safe_tree_parser_preserves_path(self) -> None:
        raw = b"100644 blob " + (b"a" * 40) + b"\tpath with space.rs\0"
        entries = ratchet.parse_tree(raw)
        self.assertEqual(entries[0].path, "path with space.rs")

    def test_symlink_is_indeterminate(self) -> None:
        entry = ratchet.TreeEntry("120000", "blob", "a" * 40, "link.rs")
        with self.assertRaises(ratchet.MeasurementError):
            ratchet.source_counts({"link.rs": entry})

    def test_unreadable_blob_is_indeterminate(self) -> None:
        entry = ratchet.TreeEntry("100644", "blob", "a" * 40, "gone.rs")
        with patch.object(ratchet, "blob_line_counts", side_effect=ratchet.MeasurementError("unreadable")):
            with self.assertRaises(ratchet.MeasurementError):
                ratchet.source_counts({"gone.rs": entry})

    def test_census_includes_omitted_language_family(self) -> None:
        self.assertTrue(ratchet._is_source("new.go"))
        self.assertTrue(ratchet._is_source("new.mjs"))
        self.assertFalse(ratchet._is_source("README.md"))
        _, policy = ratchet.evaluate({}, {}, {"new.go": 1001})
        self.assertTrue(any("ARQUIVO NOVO" in item for item in policy))

    def test_git_absent_is_indeterminate(self) -> None:
        with patch.object(ratchet, "_git", side_effect=ratchet.MeasurementError("git missing")):
            self.assertEqual(ratchet.main(["--head-ref", "HEAD"]), 2)

    def test_missing_baseline_is_indeterminate(self) -> None:
        with patch.object(ratchet, "baseline_text", side_effect=ratchet.MeasurementError("baseline missing")):
            self.assertEqual(ratchet.main(["--head-ref", "HEAD"]), 2)

    def test_run_uses_base_and_head_refs_for_baselines(self) -> None:
        calls: list[str | None] = []

        def baseline(ref: str | None) -> str:
            calls.append(ref)
            return "1001\told.rs\n"

        entry = ratchet.TreeEntry("100644", "blob", "a" * 40, "old.rs")
        with patch.object(ratchet, "baseline_text", side_effect=baseline), patch.object(
            ratchet, "source_entries", return_value={"old.rs": entry}
        ), patch.object(ratchet, "source_counts", return_value={"old.rs": 1001}):
            self.assertEqual(ratchet.run("trusted-base", "candidate-head"), 0)
        self.assertEqual(calls, ["trusted-base", "candidate-head"])


if __name__ == "__main__":
    unittest.main()
