#!/usr/bin/env python3
"""SQLite oracle for the runner aggregate durable-state migration and batch order."""

from __future__ import annotations

import hashlib
import sqlite3
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations/d1/0135_runner_aggregate_durable_state.sql"
TENANT = "00000000-0000-0000-0000-000000000001"
PERIOD = "2026-09"
TERMS_REF = "terms://tenant/2026-09/v1"
TERMS_DIGEST = "a" * 64
EVIDENCE_REF = "r2://runner-aggregate/batch"
EVIDENCE_DIGEST = "b" * 64


def database(*, include_terms: bool = True) -> sqlite3.Connection:
    db = sqlite3.connect(":memory:")
    db.executescript(
        """
        CREATE TABLE usage_event_staging (
          tenant_id TEXT NOT NULL,
          request_id TEXT NOT NULL,
          billing_period TEXT NOT NULL,
          runner_aggregated_at INTEGER,
          PRIMARY KEY (tenant_id, request_id)
        );
        CREATE TABLE runner_usage_counter (
          tenant_id TEXT NOT NULL,
          region TEXT NOT NULL,
          billing_period TEXT NOT NULL,
          vcpu_seconds INTEGER NOT NULL,
          PRIMARY KEY (tenant_id, region, billing_period)
        );
        CREATE TABLE runner_hash_chain_head (
          region TEXT PRIMARY KEY,
          current_head TEXT NOT NULL,
          next_sequence INTEGER NOT NULL
        );
        """
    )
    db.executescript(MIGRATION.read_text(encoding="utf-8"))
    if include_terms:
        db.execute(
            "INSERT INTO runner_period_terms_snapshot "
            "(tenant_id, billing_period, allowance_vcpu_seconds, rate_cents_per_vcpu_hour, "
            "terms_snapshot_ref, terms_snapshot_digest_hex, captured_at_ms) "
            "VALUES (?, ?, '360000', '20', ?, ?, 1000)",
            (TENANT, PERIOD, TERMS_REF, TERMS_DIGEST),
        )
    db.execute(
        "INSERT INTO runner_usage_counter VALUES (?, 'iad', ?, 0)", (TENANT, PERIOD)
    )
    db.execute(
        "INSERT INTO runner_hash_chain_head VALUES ('iad', ?, 0)", ("0" * 64,)
    )
    db.commit()
    return db


def stage(db: sqlite3.Connection, request_id: str) -> None:
    db.execute(
        "INSERT INTO usage_event_staging VALUES (?, ?, ?, NULL)", (TENANT, request_id, PERIOD)
    )
    db.commit()


def claim(db: sqlite3.Connection, request_id: str, batch_id: str, fingerprint: str) -> None:
    # This SELECT is the authority seam for the future writer: it cannot infer a
    # historical rate from mutable current plan state, and it binds evidence to
    # the exact snapshot it read.
    cursor = db.execute(
        """
        INSERT INTO runner_aggregate_event_claim (
          tenant_id, request_id, aggregate_batch_id, billing_period, claim_fingerprint,
          terms_snapshot_ref, terms_snapshot_digest_hex,
          evidence_ref, evidence_digest_hex, claimed_at_ms
        )
        SELECT s.tenant_id, s.request_id, ?, s.billing_period, ?,
               t.terms_snapshot_ref, t.terms_snapshot_digest_hex, ?, ?, 2000
        FROM usage_event_staging AS s
        JOIN runner_period_terms_snapshot AS t
          ON t.tenant_id = s.tenant_id AND t.billing_period = s.billing_period
        WHERE s.tenant_id = ? AND s.request_id = ?
          AND s.runner_aggregated_at IS NULL
        """,
        (batch_id, fingerprint, EVIDENCE_REF, EVIDENCE_DIGEST, TENANT, request_id),
    )
    if cursor.rowcount != 1:
        raise ValueError("missing authoritative terms or aggregate source event")


def retry_disposition(db: sqlite3.Connection, request_id: str, batch_id: str) -> str:
    fingerprint = claim_fingerprint(batch_id, request_id)
    winner = db.execute(
        "SELECT aggregate_batch_id, claim_fingerprint, terms_snapshot_ref, terms_snapshot_digest_hex, "
        "evidence_ref, evidence_digest_hex FROM runner_aggregate_event_claim "
        "WHERE tenant_id = ? AND request_id = ?",
        (TENANT, request_id),
    ).fetchone()
    expected = (batch_id, fingerprint, TERMS_REF, TERMS_DIGEST, EVIDENCE_REF, EVIDENCE_DIGEST)
    return "Deduped" if winner == expected else "Conflict"


def claim_fingerprint(batch_id: str, request_id: str) -> str:
    """A valid deterministic 64-hex fixture value; the production writer owns BLAKE3."""
    return hashlib.sha256(f"{batch_id}\x1f{request_id}".encode()).hexdigest()


def apply_logical_batch(db: sqlite3.Connection, request_ids: list[str], batch_id: str) -> None:
    """One D1 logical batch: all claims precede accounting state changes."""
    with db:
        for request_id in request_ids:
            claim(db, request_id, batch_id, claim_fingerprint(batch_id, request_id))
        db.execute(
            "UPDATE runner_usage_counter SET vcpu_seconds = vcpu_seconds + ? "
            "WHERE tenant_id = ? AND region = 'iad' AND billing_period = ?",
            (len(request_ids), TENANT, PERIOD),
        )
        db.execute(
            "UPDATE runner_hash_chain_head SET current_head = ?, next_sequence = next_sequence + 1 "
            "WHERE region = 'iad'",
            ("c" * 64,),
        )
        db.executemany(
            "UPDATE usage_event_staging SET runner_aggregated_at = 2000 "
            "WHERE tenant_id = ? AND request_id = ?",
            [(TENANT, request_id) for request_id in request_ids],
        )


def accounting_state(db: sqlite3.Connection) -> tuple[int, str, int, list[tuple[str, int | None]]]:
    return (
        db.execute("SELECT vcpu_seconds FROM runner_usage_counter").fetchone()[0],
        *db.execute("SELECT current_head, next_sequence FROM runner_hash_chain_head").fetchone(),
        db.execute(
            "SELECT request_id, runner_aggregated_at FROM usage_event_staging ORDER BY request_id"
        ).fetchall(),
    )


def assert_immutable_terms(db: sqlite3.Connection) -> None:
    for sql in (
        "UPDATE runner_period_terms_snapshot SET rate_cents_per_vcpu_hour = '21'",
        "DELETE FROM runner_period_terms_snapshot",
    ):
        try:
            db.execute(sql)
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError(f"immutable terms mutation succeeded: {sql}")


def assert_stale_overlap_aborts_before_accounting() -> None:
    db = database()
    stage(db, "fresh")
    stage(db, "stale")
    with db:
        claim(db, "stale", "older-batch", "d" * 64)
    before = accounting_state(db)
    try:
        apply_logical_batch(db, ["fresh", "stale"], "newer-batch")
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("stale overlapping claim was accepted")
    assert accounting_state(db) == before
    assert db.execute(
        "SELECT count(*) FROM runner_aggregate_event_claim WHERE request_id = 'fresh'"
    ).fetchone()[0] == 0


def assert_missing_terms_aborts_before_accounting() -> None:
    db = database(include_terms=False)
    stage(db, "missing-terms")
    before = accounting_state(db)
    try:
        apply_logical_batch(db, ["missing-terms"], "batch-missing")
    except ValueError:
        pass
    else:
        raise AssertionError("aggregate advanced without an authoritative terms snapshot")
    assert accounting_state(db) == before


def assert_lost_ack_retry_is_deduped_without_reapplying() -> None:
    db = database()
    stage(db, "lost-ack")
    apply_logical_batch(db, ["lost-ack"], "batch-1")
    committed = accounting_state(db)
    # #1635's Deduped disposition applies only after the retry compares the
    # durable winner's exact fingerprint. It performs no counter/head/watermark write.
    assert retry_disposition(db, "lost-ack", "batch-1") == "Deduped"
    assert retry_disposition(db, "lost-ack", "other-batch") == "Conflict"
    assert accounting_state(db) == committed


def main() -> None:
    db = database()
    assert_immutable_terms(db)
    assert_stale_overlap_aborts_before_accounting()
    assert_missing_terms_aborts_before_accounting()
    assert_lost_ack_retry_is_deduped_without_reapplying()
    print("runner aggregate durable state: PASS (immutable terms; overlap rollback; lost-ACK dedupe)")


if __name__ == "__main__":
    main()
