"""Keep audit archive comments aligned with the observed native R2 lock."""

from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/audit-chain-daily-verify.yml"
ARCHIVE = ROOT / "crates/corelink-container/src/routes/audit_archive.rs"
RUNBOOK = ROOT / "docs/knowledge/ops/r2-object-lock-probe.md"


def _assert_current_claims(workflow: str, archive: str, runbook: str) -> None:
    assert "2026-09-13" in workflow and "native Bucket Lock rule" in workflow
    assert "enabled, all prefixes, after 2557 days" in workflow
    assert "removable bucket configuration, NOT S3 Object Lock" in workflow
    assert "7-year Object Lock" not in workflow
    assert "native 2557-day Bucket Lock rule" in archive
    assert "administrator-removable rule is not" in archive
    assert "Bucket Lock rule (`corelink-audit-7y-retention`" in archive
    assert "7-year Object Lock" not in archive
    assert "Object Lock gateway" not in archive
    assert "2026-09-13" in runbook
    assert "enabled, all prefixes, `after 2557 days`" in runbook
    assert "B-046/B-154 therefore remain open" in runbook


def test_live_audit_bucket_lock_claims_are_bounded() -> None:
    _assert_current_claims(
        WORKFLOW.read_text(encoding="utf-8"),
        ARCHIVE.read_text(encoding="utf-8"),
        RUNBOOK.read_text(encoding="utf-8"),
    )


def test_object_lock_compliance_regression_fails() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8").replace(
        "native Bucket Lock rule", "7-year Object Lock rule", 1
    )
    with pytest.raises(AssertionError):
        _assert_current_claims(
            workflow,
            ARCHIVE.read_text(encoding="utf-8"),
            RUNBOOK.read_text(encoding="utf-8"),
        )
