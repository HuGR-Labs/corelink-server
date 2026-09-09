#!/usr/bin/env python3
"""Execute the receiver's terminal SQL and kill an always-true race mutant."""

from __future__ import annotations

import sqlite3
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "apps/synthetic-pager-worker/src/index.ts"
MIGRATION = ROOT / "migrations/d1/0116_synthetic_page_delivery_lifecycle.sql"
LEGACY = ROOT / "migrations/d1/0043_synthetic_page_drills.sql"
DRILL_ID = "SP-1785765600000"
CORRELATION_ID = f"PAT-CORRELATION-ID-001:{DRILL_ID}"
EMIT_AT_MS = 1_785_765_600_000


def sql_literal(source: str, marker: str) -> str:
    marker_at = source.index(marker)
    start = source.rfind("`", 0, marker_at)
    end = source.index("`", marker_at)
    if start < 0:
        raise AssertionError(f"missing SQL literal start for {marker}")
    return source[start + 1 : end]


def database() -> sqlite3.Connection:
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    db.executescript(LEGACY.read_text())
    db.executescript(MIGRATION.read_text())
    db.execute(
        "INSERT INTO synthetic_page_drills_b072 "
        "(drill_id, region, severity, emit_ts_ms, outcome, correlation_id, "
        "delivery_mode, scheduled_at_ms, delivered_at_ms) "
        "VALUES (?, 'americas', 'sev2_synthetic', ?, 'unacked', ?, 'immediate', ?, ?)",
        (DRILL_ID, EMIT_AT_MS, CORRELATION_ID, EMIT_AT_MS, EMIT_AT_MS + 100),
    )
    db.commit()
    return db


def terminalize(
    db: sqlite3.Connection,
    audit_sql: str,
    update_sql: str,
    *,
    event_id: str,
    outcome: str,
    occurred_at_ms: int,
) -> None:
    vector = "escalation" if outcome == "escalated" else "mobile_push"
    db.execute("BEGIN IMMEDIATE")
    db.execute(
        audit_sql,
        (event_id, DRILL_ID, outcome, occurred_at_ms, CORRELATION_ID, event_id, "eng-001", DRILL_ID),
    )
    db.execute(
        update_sql,
        (outcome, "eng-001", occurred_at_ms, occurred_at_ms - EMIT_AT_MS, vector, DRILL_ID),
    )
    db.commit()


def observed(db: sqlite3.Connection) -> tuple[str, list[tuple[str]]]:
    outcome = db.execute(
        "SELECT outcome FROM synthetic_page_drills_b072 WHERE drill_id = ?", (DRILL_ID,)
    ).fetchone()[0]
    events = db.execute(
        "SELECT event_type FROM synthetic_page_audit_events "
        "WHERE drill_id = ? AND event_type IN ('acked', 'escalated') ORDER BY occurred_at_ms",
        (DRILL_ID,),
    ).fetchall()
    return outcome, events


def main() -> None:
    source = SOURCE.read_text()
    audit_sql = sql_literal(source, "source_event_id, engineer_slug)")
    update_sql = sql_literal(source, "SET outcome = ?, engineer_slug = ?")

    # D1 batches are serialized transactions. Whichever terminal event commits
    # first owns both the row and audit event; the loser writes neither.
    db = database()
    terminalize(db, audit_sql, update_sql, event_id="pd-ack", outcome="acked", occurred_at_ms=EMIT_AT_MS + 1_000)
    terminalize(db, audit_sql, update_sql, event_id="pd-escalate", outcome="escalated", occurred_at_ms=EMIT_AT_MS + 2_000)
    assert observed(db) == ("acked", [("acked",)])

    # Semantic mutation: retaining every expected token but appending OR 1=1
    # must demonstrably let the loser overwrite/audit, so it is killed here.
    guard = "AND delivered_at_ms IS NOT NULL"
    mutant_audit = audit_sql.replace(guard, f"{guard} OR 1=1")
    mutant_update = update_sql.replace(guard, f"{guard} OR 1=1")
    mutant = database()
    terminalize(mutant, mutant_audit, mutant_update, event_id="pd-ack", outcome="acked", occurred_at_ms=EMIT_AT_MS + 1_000)
    terminalize(mutant, mutant_audit, mutant_update, event_id="pd-escalate", outcome="escalated", occurred_at_ms=EMIT_AT_MS + 2_000)
    assert observed(mutant) != ("acked", [("acked",)]), "always-true terminal guard mutant survived"

    print("B-072 terminal race: PASS (winner-only row+audit; OR-true mutant killed)")


if __name__ == "__main__":
    main()
