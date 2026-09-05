from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b155_backlog_grep_population.py"
spec = importlib.util.spec_from_file_location("b155_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B155VerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")

    def test_census_is_complete_and_population_is_not_closed(self) -> None:
        result = verifier.census(self.backlog)
        self.assertEqual(result.records, 168)
        self.assertEqual(result.command_records + result.manual_records, result.records)
        self.assertGreater(result.grep_invocations, len(result.assertions))
        self.assertGreater(len(result.unsafe), 0)
        self.assertGreater(len(result.indeterminate), 0)

    def test_real_unanchored_member_mutation_changes_semantic_verdict(self) -> None:
        baseline = verifier.census(self.backlog)
        marker = 'grep -q "byok"'
        self.assertEqual(self.backlog.count(marker), 1)
        mutated = self.backlog.replace(marker, 'grep -q "^byok"', 1)
        changed = verifier.census(mutated)
        self.assertLess(len(changed.unsafe), len(baseline.unsafe))

    def test_parser_rejects_empty_population_instead_of_returning_done(self) -> None:
        with self.assertRaises(verifier.InstrumentError):
            verifier.census("no fenced backlog records")

    def test_mutation_self_test_covers_fixture_and_completeness_guard(self) -> None:
        verifier.mutation_self_test(self.backlog)


if __name__ == "__main__":
    unittest.main()
