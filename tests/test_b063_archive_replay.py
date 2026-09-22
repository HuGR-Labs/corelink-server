"""Offline replay checks for the B-063 non-draining partition contract."""

from __future__ import annotations

import importlib.util
from pathlib import Path

import blake3
import pytest


SPEC = importlib.util.spec_from_file_location(
    "verify_b063_archive_replay",
    Path(__file__).parents[1] / "scripts/verify_b063_archive_replay.py",
)
assert SPEC and SPEC.loader
REPLAY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPLAY)


def _rows(count: int = 3) -> list[dict[str, object]]:
    previous = "00" * 32
    rows: list[dict[str, object]] = []
    for sequence in range(count):
        canonical = f'{{"n":{sequence}}}'
        chain = blake3.blake3(bytes.fromhex(previous) + canonical.encode()).hexdigest()
        rows.append(
            {
                "id": f"row-{sequence}",
                "tenant_id": "tenant-redacted",
                "region": "enam",
                "sequence_number": sequence,
                "prev_hash": previous,
                "chain_hash": chain,
                "enqueued_at": 1_787_824_088_488,
                "canonical_jcs": canonical,
                "algorithm_id": None,
                "epoch_id": None,
                "link_key_id": None,
            }
        )
        previous = chain
    return rows


def _document(rows: list[dict[str, object]]) -> dict[str, object]:
    return {
        "partition": {"tenant_id": "tenant-redacted", "region": "enam"},
        "rows": rows,
    }


def test_replay_proves_a_complete_legacy_partition_without_writes() -> None:
    result = REPLAY.replay(_document(_rows()))
    assert result["verdict"] == "drainable"
    assert result["verifying_prefix_rows"] == 3
    assert result["writes"] == {"d1": 0, "r2": 0}


def test_replay_reports_a_chain_gap_as_quarantine_required() -> None:
    rows = _rows()
    rows[2]["sequence_number"] = 4
    result = REPLAY.replay(_document(rows))
    assert result["verdict"] == "quarantine_required"
    assert result["verifying_prefix_rows"] == 2
    assert result["break"] == {
        "index": 2,
        "reason": "sequence_gap:expected=2,found=4",
    }


def test_replay_rejects_non_deterministic_duplicate_order() -> None:
    rows = _rows()
    rows[1]["sequence_number"] = rows[0]["sequence_number"]
    rows[0]["id"], rows[1]["id"] = "z-row", "a-row"
    with pytest.raises(REPLAY.ReplayError, match="deterministic"):
        REPLAY.replay(_document(rows))


def test_replay_fails_closed_on_cross_tenant_rows() -> None:
    rows = _rows()
    rows[1]["tenant_id"] = "other-tenant"
    with pytest.raises(REPLAY.ReplayError, match="tenant partition"):
        REPLAY.replay(_document(rows))
