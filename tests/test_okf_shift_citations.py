#!/usr/bin/env python3
"""Focused, hermetic contract tests for the B-124 citation shifter."""

from __future__ import annotations

import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/okf_shift_citations.py"
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location("okf_shift_citations", SCRIPT)
assert spec and spec.loader
shifter = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = shifter
spec.loader.exec_module(shifter)


class ShiftHarness(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="okf-shift-"))
        self.addCleanup(shutil.rmtree, self.tmp)
        self.scripts = self.tmp / "scripts"
        self.scripts.mkdir()
        for name in ("okf_shift_citations.py", "validate_okf.py", "okf_git_batch.py"):
            shutil.copy(ROOT / "scripts" / name, self.scripts / name)
        self._git("init", "-q")
        self._git("config", "user.email", "tests@example.invalid")
        self._git("config", "user.name", "B-124 tests")
        self._write_source(["zero", "alpha", "beta", "gamma", "delta"])
        self._write_concept("src.txt:3", "base claim")
        self._git("add", ".")
        self._git("commit", "-qm", "base")

    def _git(self, *args: str) -> str:
        return subprocess.run(
            ["git", *args], cwd=self.tmp, check=True, text=True, capture_output=True
        ).stdout

    def _write_source(self, lines: list[str]) -> None:
        (self.tmp / "src.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")

    def _write_concept(self, citation: str, description: str) -> None:
        docs = self.tmp / "docs/knowledge/auth"
        docs.mkdir(parents=True, exist_ok=True)
        (self.tmp / "docs/knowledge/index.md").write_text(
            "---\ntype: Index\nokf_version: '0.1'\nprofile_version: '0.1'\n---\n"
            "- [auth](/auth/x.md)\n",
            encoding="utf-8",
        )
        (docs / "x.md").write_text(
            "---\n"
            "type: AuthMechanism\n"
            "title: B-124 test\n"
            "description: content based shifting\n"
            "source_files:\n"
            "  - src.txt\n"
            "---\n\n"
            f"{description}. The source is `src.txt`.\n\n"
            f"# Citations\n- the claim (`{citation}`).\n",
            encoding="utf-8",
        )

    def _run(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(self.scripts / "okf_shift_citations.py"), *args],
            cwd=self.tmp,
            text=True,
            capture_output=True,
        )

    def test_real_shift_apply_is_content_verified_and_second_run_refuses(self) -> None:
        self._write_source(["inserted", "zero", "alpha", "beta", "gamma", "delta"])
        dry = self._run("--base-ref", "HEAD", "src.txt")
        self.assertEqual(dry.returncode, 0, dry.stdout + dry.stderr)
        self.assertIn("rewritten: 1", dry.stdout)
        self.assertIn("src.txt:3", (self.tmp / "docs/knowledge/auth/x.md").read_text())

        applied = self._run("--base-ref", "HEAD", "--apply", "src.txt")
        self.assertEqual(applied.returncode, 0, applied.stdout + applied.stderr)
        concept = self.tmp / "docs/knowledge/auth/x.md"
        shifted = concept.read_text(encoding="utf-8")
        self.assertIn("`src.txt:4`", shifted)
        self.assertNotIn("`src.txt:5`", shifted)

        # The second run must not apply another +1. It refuses the already
        # changed coordinate, and leaves the first result byte-for-byte intact.
        second = self._run("--base-ref", "HEAD", "--apply", "src.txt")
        self.assertNotEqual(second.returncode, 0)
        self.assertIn("already differ from base", second.stdout)
        self.assertEqual(shifted, concept.read_text(encoding="utf-8"))

    def test_partial_hand_edit_refuses_whole_file_without_mutation(self) -> None:
        self._write_source(["inserted", "zero", "alpha", "beta", "gamma", "delta"])
        self._write_concept("src.txt:4", "hand corrected")
        self._git("add", ".")
        self._git("commit", "-qm", "partial hand edit")
        before = (self.tmp / "docs/knowledge/auth/x.md").read_text(encoding="utf-8")
        result = self._run("--base-ref", "HEAD~1", "--apply", "src.txt")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already differ from base", result.stdout)
        self.assertEqual(before, (self.tmp / "docs/knowledge/auth/x.md").read_text(encoding="utf-8"))

    def test_ambiguous_content_is_reported_without_guessing(self) -> None:
        # Repeat the complete base sequence: even context widening cannot choose
        # between the two byte-identical locations.
        self._write_source(
            ["inserted", "zero", "alpha", "beta", "gamma", "delta",
             "zero", "alpha", "beta", "gamma", "delta"]
        )
        result = self._run("--base-ref", "HEAD", "--apply", "src.txt")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ambiguous", result.stdout)
        self.assertIn("`src.txt:3`", (self.tmp / "docs/knowledge/auth/x.md").read_text())

    def test_body_edit_before_citation_uses_current_span_without_corruption(self) -> None:
        self._write_source(["inserted", "zero", "alpha", "beta", "gamma", "delta"])
        concept = self.tmp / "docs/knowledge/auth/x.md"
        concept.write_text(
            concept.read_text(encoding="utf-8").replace(
                "base claim", "THIS IS A LONG HAND-EDITED DESCRIPTION before base claim"
            ),
            encoding="utf-8",
        )
        result = self._run("--base-ref", "HEAD", "--apply", "src.txt")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        shifted = concept.read_text(encoding="utf-8")
        self.assertIn("THIS IS A LONG HAND-EDITED DESCRIPTION before base claim", shifted)
        self.assertIn("`src.txt:4`", shifted)

    def test_mixed_edit_in_one_concept_blocks_every_concept_for_the_file(self) -> None:
        second = self.tmp / "docs/knowledge/auth/y.md"
        second.write_text(
            (self.tmp / "docs/knowledge/auth/x.md").read_text(encoding="utf-8").replace(
                "B-124 test", "B-124 second test"
            ),
            encoding="utf-8",
        )
        self._git("add", ".")
        self._git("commit", "-qm", "two concepts")
        self._write_source(["inserted", "zero", "alpha", "beta", "gamma", "delta"])
        second.write_text(
            second.read_text(encoding="utf-8").replace("`src.txt:3`", "`src.txt:4`"),
            encoding="utf-8",
        )
        first_before = (self.tmp / "docs/knowledge/auth/x.md").read_text(encoding="utf-8")
        result = self._run("--base-ref", "HEAD", "--apply", "src.txt")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("whole file", result.stdout)
        self.assertEqual(first_before, (self.tmp / "docs/knowledge/auth/x.md").read_text(encoding="utf-8"))
        self.assertIn("`src.txt:4`", second.read_text(encoding="utf-8"))

    def test_atomic_write_failure_preserves_original_and_cleans_temp(self) -> None:
        target = self.tmp / "docs/knowledge/auth/x.md"
        original = target.read_text(encoding="utf-8")
        with mock.patch.object(shifter.os, "replace", side_effect=OSError("simulated interruption")):
            with self.assertRaises(OSError):
                shifter._atomic_write_text(target, "replacement")
        self.assertEqual(original, target.read_text(encoding="utf-8"))
        self.assertEqual([], list(target.parent.glob(f".{target.name}.*")))


if __name__ == "__main__":
    unittest.main()
