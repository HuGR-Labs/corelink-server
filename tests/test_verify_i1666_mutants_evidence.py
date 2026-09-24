"""Adversarial static checks for the #2457 hosted shard workflow."""

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

    def test_evidence_contract_rejects_boundary_mutations(self) -> None:
        mutations = (
            ("SHARD_COUNT: 27", "SHARD_COUNT: 26"),
            ("max-parallel: 9", "max-parallel: 27"),
            ("timeout-minutes: 45", "timeout-minutes: 240"),
            ("--baseline=skip", "--baseline=run"),
            ("--sharding=round-robin", "--sharding=slice"),
            ("actions: read", "actions: write"),
            ("runs-on: ubuntu-24.04", "runs-on: self-hosted"),
            ("retention-days: 30", "retention-days: 0"),
        )
        text = WORKFLOW.read_text(encoding="utf-8")
        for before, after in mutations:
            with self.subTest(before=before):
                self.assertGreaterEqual(text.count(before), 1)
                with self.assertRaises(ValueError):
                    verify(text.replace(before, after, 1))


if __name__ == "__main__":
    unittest.main()
