"""Focused tests for the B-170 external-owner evidence boundary."""

from __future__ import annotations

import shutil
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b170_owner_actions as verifier  # noqa: E402


def _packet_fixture(root: Path) -> None:
    destination = root / verifier.PACKET
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / verifier.PACKET, destination)
    template = root / verifier.RESIDENCY_TEMPLATE
    template.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / verifier.RESIDENCY_TEMPLATE, template)


def _evidence_fixture(root: Path, *, content: str = "owner evidence\n") -> None:
    for relative in verifier.EVIDENCE:
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content, encoding="utf-8")


def test_repository_state_is_truthfully_open() -> None:
    result = verifier.verify(ROOT)
    assert result["status"] == "open"
    assert result["missing"] == [path.as_posix() for path in verifier.EVIDENCE]


def test_backlog_does_not_call_residency_template_executed() -> None:
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    section = backlog.split("### B-170 —", 1)[1].split("### B-171 —", 1)[0]
    assert "status: open" in section
    assert "pending residency-amendment template" in section
    assert "executed DPA/SLA/residency-amendment claims" not in section


def test_all_present_artifacts_force_manual_owner_validation(tmp_path: Path) -> None:
    _packet_fixture(tmp_path)
    _evidence_fixture(tmp_path)
    result = verifier.verify(tmp_path)
    assert result["status"] == "DRIFTED"
    assert result["missing"] == []


def test_empty_artifact_is_an_instrument_error(tmp_path: Path) -> None:
    _packet_fixture(tmp_path)
    _evidence_fixture(tmp_path)
    (tmp_path / verifier.EVIDENCE[1]).write_text(" \n", encoding="utf-8")
    with pytest.raises(verifier.PacketError, match="empty"):
        verifier.verify(tmp_path)


def test_symlink_evidence_cannot_satisfy_presence(tmp_path: Path) -> None:
    _packet_fixture(tmp_path)
    target = tmp_path / "outside-evidence.txt"
    target.write_text("owner evidence\n", encoding="utf-8")
    destination = tmp_path / verifier.EVIDENCE[0]
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.symlink_to(target)
    with pytest.raises(verifier.PacketError, match="symlink path component"):
        verifier.verify(tmp_path)


def test_symlink_evidence_parent_cannot_escape_root(tmp_path: Path) -> None:
    _packet_fixture(tmp_path)
    outside = tmp_path / "outside-reports"
    for relative in verifier.EVIDENCE:
        destination = outside / relative.relative_to("reports")
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text("owner evidence\n", encoding="utf-8")
    (tmp_path / "reports").symlink_to(outside, target_is_directory=True)
    with pytest.raises(verifier.PacketError, match="symlink path component"):
        verifier.verify(tmp_path)


def test_packet_marker_loss_fails_closed(tmp_path: Path) -> None:
    _packet_fixture(tmp_path)
    packet = tmp_path / verifier.PACKET
    packet.write_text(
        packet.read_text(encoding="utf-8").replace("no notification is claimed here", "notification status unknown", 1),
        encoding="utf-8",
    )
    with pytest.raises(verifier.PacketError, match="canonical action markers"):
        verifier.verify(tmp_path)


def test_residency_template_cannot_be_called_executed(tmp_path: Path) -> None:
    _packet_fixture(tmp_path)
    template = tmp_path / verifier.RESIDENCY_TEMPLATE
    template.write_text(
        template.read_text(encoding="utf-8").replace(
            'doc_status: "PENDING_LEGAL_REVIEW"', 'doc_status: "EXECUTED"', 1
        ),
        encoding="utf-8",
    )
    with pytest.raises(verifier.PacketError, match="instrument status changed"):
        verifier.verify(tmp_path)


def test_packet_symlink_cannot_satisfy_the_contract(tmp_path: Path) -> None:
    target = tmp_path / "packet-copy.md"
    target.write_text((ROOT / verifier.PACKET).read_text(encoding="utf-8"), encoding="utf-8")
    destination = tmp_path / verifier.PACKET
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.symlink_to(target)
    with pytest.raises(verifier.PacketError, match="symlink path component"):
        verifier.verify(tmp_path)


def test_packet_parent_symlink_cannot_escape_root(tmp_path: Path) -> None:
    outside = tmp_path / "outside-internal"
    outside.mkdir()
    packet_copy = outside / "b087-questionnaire-owner-actions.md"
    packet_copy.write_text(
        (ROOT / verifier.PACKET).read_text(encoding="utf-8"), encoding="utf-8"
    )
    internal = tmp_path / "docs" / "internal"
    internal.parent.mkdir(parents=True, exist_ok=True)
    internal.symlink_to(outside, target_is_directory=True)
    with pytest.raises(verifier.PacketError, match="symlink path component"):
        verifier.verify(tmp_path)
