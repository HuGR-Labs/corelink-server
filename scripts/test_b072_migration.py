#!/usr/bin/env python3
"""SQLite lifecycle regression for the B-072 delivery migration."""

from __future__ import annotations

import sqlite3
from pathlib import Path

from check_migrations_additive import scan_sql


ROOT = Path(__file__).resolve().parents[1]
LIFECYCLE_TABLE = "synthetic_page_drills_b072"


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

    # The migration is additive: the legacy table and every copied row remain,
    # while the receiver's forward-compatible projection gets the same data.
    old_count = db.execute("SELECT COUNT(*) FROM synthetic_page_drills").fetchone()[0]
    new_count = db.execute(f"SELECT COUNT(*) FROM {LIFECYCLE_TABLE}").fetchone()[0]
    assert old_count == new_count == 1
    assert db.execute(
        "SELECT drill_id, region, severity, emit_ts_ms, outcome, correlation_id "
        "FROM synthetic_page_drills"
    ).fetchall() == db.execute(
        f"SELECT drill_id, region, severity, emit_ts_ms, outcome, correlation_id "
        f"FROM {LIFECYCLE_TABLE}"
    ).fetchall()

    # Boundary handoff is deferred; a non-deferred boundary row is rejected by
    # the schema, as is a deferred row for an ordinary region.
    db.execute(
        f"INSERT INTO {LIFECYCLE_TABLE} "
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
                f"INSERT INTO {LIFECYCLE_TABLE} "
                "(drill_id, region, severity, emit_ts_ms, outcome, correlation_id, delivery_mode, scheduled_at_ms) "
                "VALUES (?, ?, 'sev2_synthetic', 1, 'unacked', ?, ?, 1)",
                (values[0], values[1], f"PAT-CORRELATION-ID-001:{values[0]}", values[2]),
            )
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError(f"invalid lifecycle row accepted: {values}")

    pending = db.execute(
        f"SELECT drill_id FROM {LIFECYCLE_TABLE} "
        "WHERE delivery_mode = 'deferred' AND delivered_at_ms IS NULL "
        "AND outcome = 'unacked' AND emit_ts_ms <= ?",
        (1_785_905_940_000,),
    ).fetchall()
    assert pending == [("SP-1785905940000",)]

    db.execute(
        f"UPDATE {LIFECYCLE_TABLE} SET delivered_at_ms = ?, outcome = 'unacked' "
        "WHERE drill_id = ? AND delivery_mode = 'deferred' AND delivered_at_ms IS NULL",
        (1_785_905_940_000, "SP-1785905940000"),
    )
    pending_after_delivery = db.execute(
        f"SELECT drill_id FROM {LIFECYCLE_TABLE} "
        "WHERE delivery_mode = 'deferred' AND delivered_at_ms IS NULL "
        "AND outcome = 'unacked' AND emit_ts_ms <= ?",
        (1_785_905_940_000,),
    ).fetchall()
    assert pending_after_delivery == []
    row = db.execute(
        f"SELECT delivery_mode, delivered_at_ms, outcome FROM {LIFECYCLE_TABLE} WHERE drill_id = ?",
        ("SP-1785905940000",),
    ).fetchone()
    assert row == ("deferred", 1_785_905_940_000, "unacked")
    migration = (ROOT / "migrations/d1/0116_synthetic_page_delivery_lifecycle.sql").read_text()
    executable = "\n".join(line for line in migration.splitlines() if not line.lstrip().startswith("--"))
    assert "DROP TABLE" not in executable.upper()
    assert "RENAME TO" not in executable.upper()

    # Additivity mutation cases: the gate must reject a future table rebuild,
    # while comment bait must remain non-executable and therefore harmless.
    for mutation in (
        "DROP TABLE synthetic_page_drills;",
        "ALTER TABLE synthetic_page_drills_b072 RENAME TO synthetic_page_drills;",
    ):
        assert scan_sql(migration + "\n" + mutation), mutation
    assert not scan_sql(migration + "\n-- DROP TABLE synthetic_page_drills;")

    # Replay mutation: removing INSERT OR IGNORE makes the second application
    # fail on the copied primary key, proving the idempotency check is live.
    replay_mutation = migration.replace(
        "INSERT OR IGNORE INTO synthetic_page_drills_b072",
        "INSERT INTO synthetic_page_drills_b072",
        1,
    )
    replay_db = sqlite3.connect(":memory:")
    replay_db.executescript((ROOT / "migrations/d1/0043_synthetic_page_drills.sql").read_text())
    replay_db.execute(
        "INSERT INTO synthetic_page_drills "
        "(drill_id, region, severity, emit_ts_ms, outcome, correlation_id) "
        "VALUES ('SP-1785844800000', 'americas', 'sev2_synthetic', 1, 'unacked', "
        "'PAT-CORRELATION-ID-001:SP-1785844800000')"
    )
    replay_db.executescript(replay_mutation)
    try:
        replay_db.executescript(replay_mutation)
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("non-idempotent INSERT mutation unexpectedly replayed")

    # Replay is ledger-safe and must not duplicate the copied population.
    db.executescript(migration)
    assert db.execute(f"SELECT COUNT(*) FROM {LIFECYCLE_TABLE}").fetchone()[0] == old_count + 1
    print("B-072 migration lifecycle: PASS (additive copy/cardinality + CHECKs + no second delivery)")


if __name__ == "__main__":
    main()
