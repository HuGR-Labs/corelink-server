"""Mutation and closed-world tests for the bounded B-087 questionnaire guard."""

from __future__ import annotations

import importlib.util
import shutil
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b087_questionnaires", ROOT / "scripts/verify_b087_questionnaires.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def fixture_tree(tmp_path: Path) -> Path:
    """Copy only the bounded guard population and its source controls."""
    for relative, _ in MODULE.SOURCE_CHECKS:
        source = ROOT / relative
        target = tmp_path / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    for relative in (MODULE.CAIQ, MODULE.SIG):
        source = ROOT / relative
        target = tmp_path / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    return tmp_path


def mutate_line(path: Path, needle: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    assert text.count(needle) >= 1, needle
    path.write_text(text.replace(needle, replacement, 1), encoding="utf-8")


def test_live_population_passes_with_owner_actions_only() -> None:
    result = MODULE.verify(ROOT)
    assert result["ok"] is True
    assert result["population"] == {"caiq": 22, "sig_lite": 15}
    assert len(result["owner_actions"]) == 4


@pytest.mark.parametrize(
    ("document", "needle", "replacement", "reason"),
    [
        (
            MODULE.CAIQ,
            "| AIS-04.1 | Application security testing performed? | P |",
            "| AIS-04.1 | Application security testing performed? | Y |",
            "unsupported answer",
        ),
        (
            MODULE.CAIQ,
            "| CEK-10.1 | Kill-switch / key-revocation supported? | N |",
            "| CEK-10.1 | Kill-switch / key-revocation supported? | Y |",
            "unsupported answer",
        ),
        (
            MODULE.SIG,
            "| J.4 | Is 24×7 incident detection in place? | P |",
            "| J.4 | Is 24×7 incident detection in place? | Y |",
            "unsupported answer",
        ),
    ],
)
def test_restoring_unsupported_positive_answer_fails(
    tmp_path: Path, document: Path, needle: str, replacement: str, reason: str
) -> None:
    root = fixture_tree(tmp_path)
    mutate_line(root / document, needle, replacement)
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any(reason in failure for failure in result["failures"])


def test_restoring_stale_sast_claim_fails_for_claim_reason(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    mutate_line(
        root / MODULE.SIG,
        "| G.10 | Are vulnerability scans performed at least quarterly? | P | **Partial cadence.",
        "| G.10 | Are vulnerability scans performed at least quarterly? | P | CodeQL + Semgrep custom rules on every PR.",
    )
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("unsupported positive claim" in failure for failure in result["failures"])


def test_missing_or_renamed_row_fails_closed(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / MODULE.CAIQ
    text = path.read_text(encoding="utf-8")
    original = "| STA-11.1 | Build provenance verifiable? | N |"
    assert text.count(original) == 1
    path.write_text(text.replace(original, "| STA-11-renamed | Build provenance verifiable? | N |"), encoding="utf-8")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("missing named row" in failure and "STA-11.1" in failure for failure in result["failures"])


def test_missing_shipped_reality_marker_fails_closed(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / "crates/corelink-container/src/routes/byok_admin.rs"
    text = path.read_text(encoding="utf-8")
    assert text.count("byok_not_available") >= 1
    path.write_text(text.replace("byok_not_available", "byok_unavailable_mutation"), encoding="utf-8")
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("source control changed or missing" in failure for failure in result["failures"])


def test_owner_packet_keeps_legal_and_external_actions_explicit() -> None:
    packet = (MODULE.ROOT / "docs/internal/b087-questionnaire-owner-actions.md").read_text()
    assert "DPA" in packet and "SLA" in packet
    assert "PagerDuty" in packet
    assert "no notification is claimed here" in packet


def test_substantiated_signed_commit_control_remains_positive(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    mutate_line(
        root / MODULE.CAIQ,
        "| CCC-07.1 | Source-code repositories access-controlled? | Y |",
        "| CCC-07.1 | Source-code repositories access-controlled? | N |",
    )
    result = MODULE.verify(root)
    assert result["ok"] is False
    assert any("CCC-07.1" in failure and "expected Y" in failure for failure in result["failures"])
