from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b169_evidence_pack.py"
INDEX = ROOT / "marketing/sales/legal-questionnaires/EVIDENCE-PACK-INDEX.md"
BASE = "b91ec17c8d559ae0323ba00a80eb309c900d599e"

spec = importlib.util.spec_from_file_location("b169_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B169EvidencePackTests(unittest.TestCase):
    def run_cli(self, index: Path, expect: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--index", str(index), "--expect", expect],
            text=True,
            capture_output=True,
            check=False,
        )

    def with_text(self, text: str) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        temp = tempfile.TemporaryDirectory()
        index = Path(temp.name) / "EVIDENCE-PACK-INDEX.md"
        index.write_text(text, encoding="utf-8")
        return temp, index

    def test_pre_fix_exact_main_is_open_and_repaired_tree_is_done(self) -> None:
        before = subprocess.check_output(
            ["git", "show", f"{BASE}:{INDEX.relative_to(ROOT)}"], cwd=ROOT
        )
        temp, index = self.with_text(before.decode())
        with temp:
            opened = self.run_cli(index, "open")
            self.assertEqual(opened.returncode, 0, opened.stderr + opened.stdout)
            self.assertIn("merge-marker-start", opened.stdout)
            self.assertIn("row-88-duplicate", opened.stdout)
            self.assertIn("row-89-public-url", opened.stdout)
        repaired = self.run_cli(INDEX, "done")
        self.assertEqual(repaired.returncode, 0, repaired.stderr + repaired.stdout)
        self.assertEqual(verifier.assess(INDEX), [])

    def test_each_merge_marker_family_reopens_the_guard(self) -> None:
        text = INDEX.read_text(encoding="utf-8")
        mutations = {
            "merge-marker-start": "<<<<<<< mutation",
            "merge-marker-separator": "=======",
            "merge-marker-end": ">>>>>>> mutation",
        }
        for family, marker in mutations.items():
            with self.subTest(family=family):
                temp, index = self.with_text(marker + "\n" + text)
                with temp:
                    gaps = verifier.assess(index)
                    self.assertIn(family, gaps)
                    self.assertEqual(self.run_cli(index, "done").returncode, 1)

    def test_duplicate_and_missing_rows_reopen_by_their_named_reason(self) -> None:
        text = INDEX.read_text(encoding="utf-8")
        rows = {
            number: next(line for line in text.splitlines() if line.startswith(f"| {number} |"))
            for number in ("88", "89")
        }
        mutations = {
            "row-88-duplicate": text + "\n" + rows["88"] + "\n",
            "row-89-duplicate": text + "\n" + rows["89"] + "\n",
            "row-88-missing": text.replace(rows["88"] + "\n", "", 1),
            "row-89-missing": text.replace(rows["89"] + "\n", "", 1),
        }
        for reason, mutated in mutations.items():
            with self.subTest(reason=reason):
                temp, index = self.with_text(mutated)
                with temp:
                    self.assertIn(reason, verifier.assess(index))
                    self.assertEqual(self.run_cli(index, "done").returncode, 1)

    def test_public_slo_url_mutation_is_rejected_as_false_evidence(self) -> None:
        text = INDEX.read_text(encoding="utf-8")
        old = next(line for line in text.splitlines() if line.startswith("| 89 |"))
        public = "| 89 | SLO — public summary | https://corelink-docs.humangr.com/slo | PUBLIC |"
        temp, index = self.with_text(text.replace(old, public, 1))
        with temp:
            gaps = verifier.assess(index)
            self.assertIn("row-89-public-url", gaps)
            self.assertIn("row-89-truthful-internal-reference", gaps)
            result = self.run_cli(index, "done")
            self.assertEqual(result.returncode, 1)
            self.assertIn("row-89-public-url", result.stdout)

    def test_missing_index_is_an_instrument_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            result = self.run_cli(Path(directory) / "missing.md", "done")
        self.assertEqual(result.returncode, 2)
        self.assertIn("instrument error", result.stderr)


if __name__ == "__main__":
    unittest.main()
