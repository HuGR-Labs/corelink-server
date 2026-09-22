"""Mutation coverage for the bounded timing-artifact checker."""

from pathlib import Path

import pytest

from scripts.verify_d03_timing_artifact import TimingError, verify_reads, verify_writes


def _write_artifact(path: Path, *, reads: bool = False, identities: bool = False) -> None:
    if not reads:
        identity = " request_id=req-123 cf_ray=abc123-SJC" if identities else ""
        rows = [
            f"ordinal={i} status=200 bytes=1024 wall_s=0.020{identity}\n"
            "Server-Timing: ostore;dur=2, oaccounting;dur=3"
            for i in range(1, 4)
        ]
    else:
        header = (
            "Server-Timing: qtier;dur=1, qdo;dur=2, qbatch;dur=3, qresid;dur=4, "
            "qcontrol;dur=5, ohop;dur=4, opat;dur=1, oquota;dur=1, ostore;dur=1, "
            "oaccounting;dur=1, oargon;dur=1, opermit;dur=1, ortier;dur=1, oaudit;dur=1, "
            "oratelimit;dur=1, ohandler;dur=1, wdb;dur=15, origin;dur=14, total;dur=30"
        )
        rows = [f"sample={i} status=200 wall_s=0.030\n{header}" for i in range(1, 11)]
    path.write_text("\n".join(rows) + "\n", encoding="utf-8")


def test_valid_write_and_read_artifacts(tmp_path: Path) -> None:
    writes = tmp_path / "writes.txt"
    reads = tmp_path / "reads.txt"
    _write_artifact(writes)
    _write_artifact(reads, reads=True)
    verify_writes(writes)
    verify_reads(reads)


def test_write_identity_gate_requires_request_and_colo_receipts(tmp_path: Path) -> None:
    artifact = tmp_path / "writes.txt"
    _write_artifact(artifact, identities=True)
    verify_writes(artifact, require_identities=True)

    _write_artifact(artifact)
    with pytest.raises(TimingError, match="request/colo identity"):
        verify_writes(artifact, require_identities=True)

    _write_artifact(artifact, identities=True)
    artifact.write_text(
        artifact.read_text(encoding="utf-8").replace("cf_ray=abc123-SJC", "cf_ray=invalid/value"),
        encoding="utf-8",
    )
    with pytest.raises(TimingError):
        verify_writes(artifact, require_identities=True)


def test_write_phase_mutation_is_red(tmp_path: Path) -> None:
    artifact = tmp_path / "writes.txt"
    _write_artifact(artifact)
    artifact.write_text(artifact.read_text(encoding="utf-8").replace("oaccounting;dur=3", "oaccounting;dur=abc"), encoding="utf-8")
    with pytest.raises(TimingError):
        verify_writes(artifact)


def test_read_reconciliation_and_alias_mutations_are_red(tmp_path: Path) -> None:
    artifact = tmp_path / "reads.txt"
    _write_artifact(artifact, reads=True)
    artifact.write_text(artifact.read_text(encoding="utf-8").replace("qcontrol;dur=5", "qcontrol;dur=6"), encoding="utf-8")
    with pytest.raises(TimingError):
        verify_reads(artifact)

    _write_artifact(artifact, reads=True)
    populated = artifact.read_text(encoding="utf-8")
    for phase in ("opat", "oquota", "ostore", "oaccounting", "oargon", "opermit", "ortier", "oaudit", "oratelimit", "ohandler"):
        populated = populated.replace(f", {phase};dur=1", "")
    populated = populated.replace("origin;dur=14", "origin;dur=4")
    artifact.write_text(populated, encoding="utf-8")
    with pytest.raises(TimingError):
        verify_reads(artifact)

    _write_artifact(artifact, reads=True)
    artifact.write_text(artifact.read_text(encoding="utf-8").replace("ohandler;dur=1", "ohandler;dur=1, oother;dur=2"), encoding="utf-8")
    with pytest.raises(TimingError):
        verify_reads(artifact)
