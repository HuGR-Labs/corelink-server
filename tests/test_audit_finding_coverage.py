"""Regression tests for the B-101 finding-to-decision ingestion gate."""

from __future__ import annotations

import copy
import json
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_audit_finding_coverage as coverage  # noqa: E402


def _manifest() -> dict:
    return json.loads((ROOT / coverage.MANIFEST_RELATIVE).read_text(encoding="utf-8"))


def _write_manifest(tmp_path: Path, manifest: dict) -> Path:
    path = tmp_path / "decisions.json"
    path.write_text(json.dumps(manifest), encoding="utf-8")
    return path


def test_live_inputs_rederive_the_documented_live_population() -> None:
    report = coverage.verify(ROOT)

    # B-101's historical 66 omitted the report's 21 separately-confirmed
    # headings.  The gate makes the live population explicit rather than
    # silently dropping them to reproduce the stale headline.
    assert report == {
        "documents": {
            "due_diligence_2026_06_15": 87,
            "go_live_2026_08_26": 20,
            "pilot_identity_2026_07_02": 3,
        },
        "total": 110,
        "tracked": 32,
        "declined": 78,
        "status": "complete",
    }


def test_missing_decision_is_red_not_a_document_name_false_green(tmp_path: Path) -> None:
    manifest = _manifest()
    manifest["decisions"] = manifest["decisions"][1:]

    with pytest.raises(coverage.CoverageError, match="new or unassigned findings: DD-001"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_duplicate_decision_is_ambiguous_and_red(tmp_path: Path) -> None:
    manifest = _manifest()
    manifest["decisions"].append(copy.deepcopy(manifest["decisions"][0]))

    with pytest.raises(coverage.CoverageError, match="duplicate decisions are ambiguous: DD-001"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_noncanonical_or_missing_backlog_id_is_red(tmp_path: Path) -> None:
    manifest = _manifest()
    decision = next(item for item in manifest["decisions"] if item["kind"] == "tracked")
    decision["backlog_id"] = "B-000"

    with pytest.raises(coverage.CoverageError, match="existing canonical B-ID"):
        coverage.verify(ROOT, _write_manifest(tmp_path, manifest))


def test_truncated_or_renumbered_due_diligence_findings_are_red() -> None:
    source = (ROOT / "docs/security/2026-06-15-launch-due-diligence-audit.md").read_text(encoding="utf-8")

    with pytest.raises(coverage.CoverageError, match="contiguous DD-001"):
        coverage.parse_due_diligence(source.replace("### 1. [HIGH]", "### 99. [HIGH]", 1), "fixture.md")
