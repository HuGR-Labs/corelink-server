#!/usr/bin/env python3
"""SQLite lifecycle regression for the B-072 delivery migration."""

from __future__ import annotations

import sqlite3
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    db = sqlite3.connect(":memory:")
    db.executescript((ROOT / "migrations/d1/0043_synthetic_page_drills.sql").read_text())
    db.execute(
        "INSERT INTO synthetic_page_drills "
        "(drill_id, region, severity, emit_ts_ms, outcome, correlation_id) "
        "VALUES (?, 'americas', 'sev2_synthetic', ?, 'unacked', ?)",
        ("SP-1785844800000", 1_785_844_800_000, "PAT-CORRELATION-ID-001:SP-1785844800000"),
    )
    db.executescript((ROOT / "migrations/d1/0116_synthetic_page_delivery_lifecycle.sql").read_text())

    # Boundary handoff is deferred; a non-deferred boundary row is rejected by
    # the schema, as is a deferred row for an ordinary region.
    db.execute(
        "INSERT INTO synthetic_page_drills "
        "(drill_id, region, severity, emit_ts_ms, outcome, correlation_id, delivery_mode, scheduled_at_ms) "
        "VALUES (?, 'boundary_handoff', 'sev2_synthetic', ?, 'unacked', ?, 'deferred', ?)",
        ("SP-1785905940000", 1_785_905_940_000, "PAT-CORRELATION-ID-001:SP-1785905940000", 1_785_844_800_000),
    )
    for values in (
        ("SP-1785905940001", "boundary_handoff", "immediate"),
        ("SP-1785905940002", "americas", "deferred"),
    ):
        try:
            db.execute(
                "INSERT INTO synthetic_page_drills "
                "(drill_id, region, severity, emit_ts_ms, outcome, correlation_id, delivery_mode, scheduled_at_ms) "
                "VALUES (?, ?, 'sev2_synthetic', 1, 'unacked', ?, ?, 1)",
                (values[0], values[1], f"PAT-CORRELATION-ID-001:{values[0]}", values[2]),
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError(f"invalid lifecycle row accepted: {values}")

    pending = db.execute(
        "SELECT drill_id FROM synthetic_page_drills "
        "WHERE delivery_mode = 'deferred' AND delivered_at_ms IS NULL "
        "AND outcome = 'unacked' AND emit_ts_ms <= ?",
        (1_785_905_940_000,),
    ).fetchall()
    assert pending == [("SP-1785905940000",)]

    db.execute(
        "UPDATE synthetic_page_drills SET delivered_at_ms = ?, outcome = 'unacked' "
        "WHERE drill_id = ? AND delivery_mode = 'deferred' AND delivered_at_ms IS NULL",
        (1_785_905_940_000, "SP-1785905940000"),
    )
    pending_after_delivery = db.execute(
        "SELECT drill_id FROM synthetic_page_drills "
        "WHERE delivery_mode = 'deferred' AND delivered_at_ms IS NULL "
        "AND outcome = 'unacked' AND emit_ts_ms <= ?",
        (1_785_905_940_000,),
    ).fetchall()
    assert pending_after_delivery == []
    row = db.execute(
        "SELECT delivery_mode, delivered_at_ms, outcome FROM synthetic_page_drills WHERE drill_id = ?",
        ("SP-1785905940000",),
    ).fetchone()
    assert row == ("deferred", 1_785_905_940_000, "unacked")
    print("B-072 migration lifecycle: PASS (CHECKs + no second delivery)")


if __name__ == "__main__":
    main()
