"""Adversarial checks for the hosted mutants evidence contract."""

from pathlib import Path

import pytest

from scripts.verify_i1666_mutants_evidence import verify


WORKFLOW = Path(".github/workflows/issue-1863-mutants-hosted.yml")


def test_live_hosted_mutants_evidence_contract_passes() -> None:
    verify(WORKFLOW.read_text(encoding="utf-8"))


@pytest.mark.parametrize(
    ("before", "after"),
    (
        ("retention-days: 30", "retention-days: 0"),
        (
            "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
            "actions/upload-artifact@v7",
        ),
        ("permissions:\n  contents: read", "permissions:\n  contents: write"),
        ("mutants.out/", "missing-mutants-output/"),
    ),
)
def test_evidence_contract_rejects_mutations(before: str, after: str) -> None:
    text = WORKFLOW.read_text(encoding="utf-8")
    assert text.count(before) == 1
    with pytest.raises(ValueError):
        verify(text.replace(before, after, 1))
