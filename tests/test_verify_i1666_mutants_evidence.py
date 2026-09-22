"""Adversarial checks for the hosted mutants evidence contract."""

import unittest
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts.verify_i1666_mutants_evidence import verify


WORKFLOW = ROOT / ".github/workflows/issue-1863-mutants-hosted.yml"


class HostedMutantsEvidenceContractTest(unittest.TestCase):
    def test_live_hosted_mutants_evidence_contract_passes(self) -> None:
        verify(WORKFLOW.read_text(encoding="utf-8"))

    def test_evidence_contract_rejects_mutations(self) -> None:
        mutations = (
            ("retention-days: 30", "retention-days: 0"),
            (
                "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
                "actions/upload-artifact@v7",
            ),
            ("permissions:\n  contents: read", "permissions:\n  contents: write"),
            (
                '"mutation_output": "mutants.out/"',
                '"mutation_output": "missing-mutants-output/"',
            ),
        )
        text = WORKFLOW.read_text(encoding="utf-8")
        for before, after in mutations:
            with self.subTest(before=before):
                self.assertEqual(text.count(before), 1)
                with self.assertRaises(ValueError):
                    verify(text.replace(before, after, 1))


if __name__ == "__main__":
    unittest.main()
