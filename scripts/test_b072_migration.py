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
    provider_migration = ROOT / "migrations/d1/0146_b072_provider_deferred_receipts.sql"
    db.executescript(provider_migration.read_text())

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
    # Provider-deferred is a separate typed terminal receipt with a matching
    # audit event; replay is idempotent and every provenance value is bound.
    provider_id = "SP-1787580000000"
    db.execute(
        f"INSERT INTO {LIFECYCLE_TABLE} "
        "(drill_id, region, severity, emit_ts_ms, outcome, correlation_id, delivery_mode, scheduled_at_ms) "
        "VALUES (?, 'boundary_handoff', 'sev2_synthetic', 1788134340000, 'unacked', ?, 'deferred', 1787580000000)",
        (provider_id, f"PAT-CORRELATION-ID-001:{provider_id}"),
    )
    receipt = (provider_id, 1787580000000, f"PAT-CORRELATION-ID-001:{provider_id}", "provider_deferred",
               "provider_deferred", "cf-scheduler-7", "0123456789abcdef0123456789abcdef01234567",
               "cf-receiver-9", "persisted_provider_deferred", 1788134341000)
    db.execute(
        "INSERT OR IGNORE INTO synthetic_page_provider_receipts "
        "(drill_id, scheduled_at_ms, correlation_id, provider_mode, outcome, scheduler_worker_revision, "
        "serving_sha, receiver_worker_revision, receiver_result, recorded_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", receipt)
    db.execute(
        "INSERT OR IGNORE INTO synthetic_page_provider_audit_events "
        "(event_id, drill_id, event_type, occurred_at_ms, correlation_id, source_event_id) VALUES (?, ?, ?, ?, ?, ?)",
        (f"provider-deferred:{provider_id}", provider_id, "provider_deferred", 1788134341000,
         f"PAT-CORRELATION-ID-001:{provider_id}", f"provider-deferred:{provider_id}"))
    db.executescript(provider_migration.read_text())
    assert db.execute("SELECT outcome, serving_sha, receiver_result FROM synthetic_page_provider_receipts WHERE drill_id = ?", (provider_id,)).fetchone() == (
        "provider_deferred", receipt[6], "persisted_provider_deferred")
    assert db.execute("SELECT event_type FROM synthetic_page_provider_audit_events WHERE drill_id = ?", (provider_id,)).fetchone() == ("provider_deferred",)
    for invalid in (
        ("provider_deferred", "acked", receipt[5], receipt[6], receipt[7], receipt[8]),
        ("provider_deferred", "provider_deferred", receipt[5], "bad-sha", receipt[7], receipt[8]),
    ):
        try:
            db.execute(
                "INSERT INTO synthetic_page_provider_receipts "
                "(drill_id, scheduled_at_ms, correlation_id, provider_mode, outcome, scheduler_worker_revision, "
                "serving_sha, receiver_worker_revision, receiver_result, recorded_at_ms) "
                "VALUES ('SP-1787580000001', 1, 'c', ?, ?, ?, ?, ?, ?, 1)", invalid)
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError(f"invalid provider terminal receipt accepted: {invalid}")
    migration = (ROOT / "migrations/d1/0116_synthetic_page_delivery_lifecycle.sql").read_text()
    executable = "\n".join(line for line in migration.splitlines() if not line.lstrip().startswith("--"))
    assert "DROP TABLE" not in executable.upper()
    assert "RENAME TO" not in executable.upper()
    provider_migration_source = provider_migration.read_text()
    assert not scan_sql(provider_migration_source), "provider receipt migration must remain additive"

    # Additivity mutation cases: the gate must reject a future table rebuild,
    # while comment bait must remain non-executable and therefore harmless.
    for migration_source in (migration, provider_migration_source):
        for mutation in (
            "DROP TABLE synthetic_page_drills;",
            "ALTER TABLE synthetic_page_drills_b072 RENAME TO synthetic_page_drills;",
        ):
            assert scan_sql(migration_source + "\n" + mutation), mutation
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
    assert db.execute(f"SELECT COUNT(*) FROM {LIFECYCLE_TABLE}").fetchone()[0] == old_count + 2
    print("B-072 migration lifecycle: PASS (additive copy/cardinality + CHECKs + no second delivery)")


if __name__ == "__main__":
    main()
