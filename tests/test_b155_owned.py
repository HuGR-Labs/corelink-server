"""Lightweight regression tests for the ten B-155 semantic proof adapters."""
from __future__ import annotations

import subprocess
import sys
import urllib.error
import unittest
from unittest.mock import patch
from pathlib import Path

from scripts import verify_b155_owned as verifier


ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {
    "B-014": "done", "B-015": "done", "B-035": "open", "B-039": "open",
    "B-045": "done", "B-047": "done", "B-050": "done", "B-051": "done",
    "B-052": "done", "B-060": "done",
}


class B155OwnedSemanticTests(unittest.TestCase):
    def test_each_focal_mutation_adapter_is_live(self) -> None:
        helper = ROOT / "scripts/verify_b155_owned.py"
        for ident, polarity in EXPECTED.items():
            with self.subTest(ident=ident):
                result = subprocess.run(
                    [sys.executable, str(helper), "--id", ident, "--expect", polarity,
                     "--offline", "--self-test"],
                    cwd=ROOT, text=True, capture_output=True, timeout=15,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("mutations=22 rejected", result.stdout)

    def test_backlog_wires_only_the_owned_ids_to_helper(self) -> None:
        backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
        for ident, polarity in EXPECTED.items():
            needle = f"python3 scripts/verify_b155_owned.py --id {ident} --expect {polarity}"
            self.assertEqual(backlog.count(needle), 1, ident)
            if ident == "B-039":
                self.assertEqual(backlog.count(needle + " --offline"), 1, ident)

    def test_b039_mirror_forbidden_fails_closed(self) -> None:
        forbidden = urllib.error.HTTPError(
            verifier.MIRROR_URL, 403, "forbidden", {}, None
        )
        with patch.object(verifier.urllib.request, "urlopen", side_effect=forbidden):
            with self.assertRaisesRegex(verifier.VerificationError, "mirror query failed"):
                verifier._b039(ROOT, live=True)


if __name__ == "__main__":
    unittest.main()
