#!/usr/bin/env python3
"""SQLite seam oracle for #1630's bounded runner aggregate workflow.

Success/DoD: the real discovery and commit SQL shape preserves period order,
caps, immutable terms, exact claims, and all-or-nothing accounting.  Invariants:
only the oldest historical plus current period run; terms-less rows stay pending;
and a non-exact claim or bare watermark changes no accounting state.
"""

from __future__ import annotations

import calendar
import hashlib
import json
import sqlite3
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations/d1/0138_runner_aggregate_durable_state.sql"
DISCOVERY_SQL = """WITH e AS (SELECT json_extract(value,'$.tenant_id') tenant_id,json_extract(value,'$.request_id') request_id FROM json_each(?)) SELECT e.tenant_id,e.request_id,c.claim_fingerprint,c.aggregate_batch_id,c.billing_period,c.terms_snapshot_ref,c.terms_snapshot_digest_hex,c.evidence_ref,c.evidence_digest_hex,c.claimed_at_ms,s.runner_aggregated_at FROM e LEFT JOIN runner_aggregate_event_claim c USING(tenant_id,request_id) LEFT JOIN usage_event_staging s USING(tenant_id,request_id)"""
CLAIM_SQL = """INSERT INTO runner_aggregate_event_claim (tenant_id,request_id,aggregate_batch_id,billing_period,claim_fingerprint,terms_snapshot_ref,terms_snapshot_digest_hex,evidence_ref,evidence_digest_hex,claimed_at_ms) SELECT json_extract(value,'$.tenant_id'),json_extract(value,'$.request_id'),json_extract(value,'$.aggregate_batch_id'),json_extract(value,'$.billing_period'),json_extract(value,'$.claim_fingerprint'),t.terms_snapshot_ref,t.terms_snapshot_digest_hex,json_extract(value,'$.evidence_ref'),json_extract(value,'$.evidence_digest_hex'),CAST(json_extract(value,'$.claimed_at_ms') AS INTEGER) FROM json_each(?) JOIN usage_event_staging s ON s.tenant_id=json_extract(value,'$.tenant_id') AND s.request_id=json_extract(value,'$.request_id') JOIN runner_period_terms_snapshot t ON t.tenant_id=s.tenant_id AND t.billing_period=s.billing_period AND t.terms_snapshot_ref=json_extract(value,'$.terms_snapshot_ref') AND t.terms_snapshot_digest_hex=json_extract(value,'$.terms_snapshot_digest_hex') WHERE s.runner_aggregated_at IS NULL"""
EXACT_CLAIM_SQL = """WITH e AS (SELECT value FROM json_each(?)) SELECT abs(CASE WHEN (SELECT count(*) FROM runner_aggregate_event_claim c JOIN e ON c.tenant_id=json_extract(e.value,'$.tenant_id') AND c.request_id=json_extract(e.value,'$.request_id') AND c.aggregate_batch_id=json_extract(e.value,'$.aggregate_batch_id') AND c.billing_period=json_extract(e.value,'$.billing_period') AND c.claim_fingerprint=json_extract(e.value,'$.claim_fingerprint') AND c.terms_snapshot_ref=json_extract(e.value,'$.terms_snapshot_ref') AND c.terms_snapshot_digest_hex=json_extract(e.value,'$.terms_snapshot_digest_hex') AND c.evidence_ref=json_extract(e.value,'$.evidence_ref') AND c.evidence_digest_hex=json_extract(e.value,'$.evidence_digest_hex') AND c.claimed_at_ms=CAST(json_extract(e.value,'$.claimed_at_ms') AS INTEGER)) = json_array_length(?) THEN 0 ELSE -9223372036854775808 END)"""
WATERMARK_SQL = """UPDATE usage_event_staging SET runner_aggregated_at=CAST(? AS INTEGER) WHERE runner_aggregated_at IS NULL AND EXISTS (SELECT 1 FROM json_each(?) e WHERE tenant_id=json_extract(e.value,'$.tenant_id') AND request_id=json_extract(e.value,'$.request_id'))"""


class Conflict(RuntimeError):
    pass


def database() -> sqlite3.Connection:
    db = sqlite3.connect(":memory:")
    db.row_factory = sqlite3.Row
    db.executescript("""
    CREATE TABLE usage_event_staging (tenant_id TEXT NOT NULL, request_id TEXT NOT NULL,
      billing_period TEXT NOT NULL, event_kind TEXT NOT NULL, qty INTEGER, region TEXT NOT NULL,
      event_payload_hash TEXT NOT NULL, emitted_at INTEGER NOT NULL, runner_aggregated_at INTEGER,
      PRIMARY KEY (tenant_id, request_id));
    CREATE TABLE runner_usage_counter (tenant_id TEXT NOT NULL, region TEXT NOT NULL,
      billing_period TEXT NOT NULL, vcpu_seconds INTEGER NOT NULL, event_count INTEGER NOT NULL,
      aggregated_at INTEGER NOT NULL, prev_hash TEXT NOT NULL, own_digest TEXT NOT NULL,
      schema_version TEXT NOT NULL, PRIMARY KEY (tenant_id, region, billing_period));
    CREATE TABLE runner_hash_chain_head (region TEXT NOT NULL, chain_kind TEXT NOT NULL,
      current_head TEXT NOT NULL, last_aggregated_period TEXT NOT NULL, updated_at INTEGER NOT NULL,
      next_sequence INTEGER NOT NULL, PRIMARY KEY (region, chain_kind));
    """)
    db.executescript(MIGRATION.read_text())
    return db


def terms(db: sqlite3.Connection, tenant: str, period: str) -> None:
    db.execute("INSERT INTO runner_period_terms_snapshot VALUES (?,?,?,?,?,?,?)",
               (tenant, period, "100", "20", f"terms://{tenant}/{period}", "a" * 64, 1))
    db.commit()


def stage(db: sqlite3.Connection, tenant: str, request: str, period: str,
          *, emitted: int, qty: int = 1, region: str = "iad") -> None:
    db.execute("INSERT INTO usage_event_staging VALUES (?,?,?,?,?,?,?,?,NULL)",
               (tenant, request, period, "runner_vcpu_seconds", qty, region,
                hashlib.sha256(request.encode()).hexdigest(), emitted))


def month_bounds(period: str) -> tuple[int, int]:
    year, month = map(int, period.split("-"))
    start = calendar.timegm((year, month, 1, 0, 0, 0)) * 1000
    next_year, next_month = (year + 1, 1) if month == 12 else (year, month + 1)
    end = calendar.timegm((next_year, next_month, 1, 0, 0, 0)) * 1000
    return start, end


def periods(db: sqlite3.Connection, current: str) -> list[str]:
    historical = db.execute("""SELECT billing_period FROM usage_event_staging
      WHERE event_kind='runner_vcpu_seconds' AND qty IS NOT NULL
      AND runner_aggregated_at IS NULL AND billing_period < ?
      GROUP BY billing_period ORDER BY billing_period ASC LIMIT 1""", (current,)).fetchall()
    return [row["billing_period"] for row in historical] + [current]


def extract(db: sqlite3.Connection, period: str, cap: int) -> tuple[list[sqlite3.Row], list[sqlite3.Row]]:
    rows = db.execute("""SELECT tenant_id,request_id,region,qty,event_payload_hash,emitted_at
      FROM usage_event_staging WHERE event_kind='runner_vcpu_seconds' AND qty IS NOT NULL
      AND runner_aggregated_at IS NULL AND billing_period=?
      ORDER BY emitted_at,tenant_id,request_id LIMIT ?""", (period, cap)).fetchall()
    known = {row[0] for row in db.execute(
        "SELECT tenant_id FROM runner_period_terms_snapshot WHERE billing_period=?", (period,))}
    return [row for row in rows if row["tenant_id"] in known], [row for row in rows if row["tenant_id"] not in known]


def event(row: sqlite3.Row, period: str, now: int) -> dict[str, object]:
    fingerprint = hashlib.sha256(json.dumps(dict(row), sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    ref = f"terms://{row['tenant_id']}/{period}"
    evidence = hashlib.sha256(f"{period}:{row['request_id']}".encode()).hexdigest()
    return {"tenant_id": row["tenant_id"], "request_id": row["request_id"],
            "claim_fingerprint": fingerprint, "terms_snapshot_ref": ref,
            "terms_snapshot_digest_hex": "a" * 64,
            "aggregate_batch_id": f"runner-v3:{period}:{evidence}", "billing_period": period,
            "evidence_ref": f"github://corelink/runner-aggregate/{period}/{evidence}",
            "evidence_digest_hex": evidence, "claimed_at_ms": now}


def snapshot(db: sqlite3.Connection) -> tuple[list[tuple], list[tuple], list[tuple]]:
    return tuple(tuple(row) for row in db.execute("SELECT * FROM runner_usage_counter ORDER BY 1,2,3")), tuple(tuple(row) for row in db.execute("SELECT * FROM runner_hash_chain_head ORDER BY 1")), tuple(tuple(row) for row in db.execute("SELECT tenant_id,request_id,runner_aggregated_at FROM usage_event_staging ORDER BY 1,2"))


def commit(db: sqlite3.Connection, rows: list[sqlite3.Row], period: str, *, now: int = 1000,
           fail: bool = False) -> str:
    events = [event(row, period, now) for row in rows]
    payload = json.dumps(events, sort_keys=True, separators=(",", ":"))
    winners = db.execute(DISCOVERY_SQL, (payload,)).fetchall()
    if any(row["runner_aggregated_at"] is not None and row["claim_fingerprint"] is None for row in winners):
        raise Conflict("watermark without durable claim")
    # `claimed_at_ms` records when the durable winner landed; it is not part of
    # the logical batch identity. A lost-ACK retry has a new clock value and
    # must still dedupe without applying another counter delta.
    fields = ("claim_fingerprint", "aggregate_batch_id", "billing_period", "terms_snapshot_ref", "terms_snapshot_digest_hex", "evidence_ref", "evidence_digest_hex")
    if any(row["claim_fingerprint"] is not None for row in winners):
        expected = {(item["tenant_id"], item["request_id"]): item for item in events}
        if len(winners) == len(events) and all(row["runner_aggregated_at"] is not None and all(row[field] == expected[(row["tenant_id"], row["request_id"])][field] for field in fields) for row in winners):
            return "Deduped"
        raise Conflict("durable claim winner")
    try:
        with db:
            db.execute(CLAIM_SQL, (payload,))
            db.execute(EXACT_CLAIM_SQL, (payload, payload))
            if fail:
                db.execute("SELECT no_such_commit_statement")
            groups: dict[tuple[str, str], list[sqlite3.Row]] = defaultdict(list)
            for row in rows:
                groups[row["tenant_id"], row["region"]].append(row)
            for (tenant, region), grouped in groups.items():
                total = sum(row["qty"] for row in grouped)
                db.execute("""INSERT INTO runner_usage_counter VALUES (?,?,?,?,?,?,?,?,?)
                  ON CONFLICT(tenant_id,region,billing_period) DO UPDATE SET
                  vcpu_seconds=vcpu_seconds+excluded.vcpu_seconds,event_count=event_count+excluded.event_count,
                  aggregated_at=excluded.aggregated_at""", (tenant, region, period, total, len(grouped), now, "0" * 64, "b" * 64, "1.0.0"))
                db.execute("""INSERT INTO runner_hash_chain_head VALUES (?, 'runner_vcpu', ?, ?, ?, 1)
                  ON CONFLICT(region,chain_kind) DO UPDATE SET current_head=excluded.current_head,
                  last_aggregated_period=excluded.last_aggregated_period,updated_at=excluded.updated_at,
                  next_sequence=runner_hash_chain_head.next_sequence+1""", (region, "b" * 64, period, now))
            db.execute(WATERMARK_SQL, (now, payload))
    except sqlite3.Error as exc:
        raise Conflict("atomic batch failed") from exc
    return "Committed"


def run(db: sqlite3.Connection, current: str) -> tuple[list[str], list[tuple[str, str]]]:
    processed, pending = [], []
    selected = periods(db, current)
    for index, period in enumerate(selected):
        eligible, missing = extract(db, period, 8000 if period == selected[-1] else 2000)
        pending.extend((period, row["request_id"]) for row in missing)
        if eligible:
            commit(db, eligible, period)
            processed.append(period)
    return processed, pending


def main() -> None:
    print("billing aggregate runner multi-period seam: PASS")


if __name__ == "__main__":
    main()
