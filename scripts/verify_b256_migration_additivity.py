#!/usr/bin/env python3
"""Focal behavioral and mutation verifier for B-256 / D1 migration 0062.

The verifier deliberately exercises a real SQLite database.  It proves that
0062 widens both legacy CHECKs in place, keeps the dependent view, indexes,
foreign key, columns, and seeded rows unchanged, and is safe to replay.  The
negative mutation cases keep the additive detector honest and ensure a partial
catalog edit fails closed through the migration's TEMP postcondition guard.
"""

from __future__ import annotations

import hashlib
import re
import sqlite3
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations/d1/0062_expand_tier_selections_6tier.sql"
PROPERTY = ROOT / "crates/corelink-ops/tests/migrations_prop_migration_additivity.rs"
REGRESSION_SEED = 12030011770417089905
EXPECTED_MIGRATION_BLOB = "cda560abd23ceb2df03b75cdade92cdf6f5a6086"
EXPECTED_PROPERTY_BLOB = "ccd9dacabcd97235f80cf1ca4d52b170802d600f"

SIX_TIERS = ("free", "solo", "starter", "team", "pro", "max", "enterprise")
CHECKOUT_TIERS = ("solo", "starter", "team", "pro", "max")
FORBIDDEN_PREFIXES = (
    "DROP TABLE",
    "DROP COLUMN",
    "DROP INDEX",
    "DROP VIEW",
    "DROP TRIGGER",
    "DROP CONSTRAINT",
    "ALTER COLUMN",
    "ALTER TABLE RENAME",
    "RENAME TO",
    "RENAME TABLE",
    "RENAME COLUMN",
)


def _blob_id(data: bytes) -> str:
    return hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()


def _strip_comments(sql: str) -> str:
    """Blank comments and quoted SQL literals before keyword scanning."""
    out: list[str] = []
    i = 0
    line = block = False
    quote: str | None = None
    while i < len(sql):
        char = sql[i]
        nxt = sql[i + 1] if i + 1 < len(sql) else ""
        if line:
            if char == "\n":
                line = False
                out.append(char)
            i += 1
            continue
        if block:
            if char == "*" and nxt == "/":
                block = False
                i += 2
            else:
                i += 1
            continue
        if quote is not None:
            if char == quote and nxt == quote:
                out.extend((" ", " "))
                i += 2
                continue
            if char == quote:
                quote = None
            out.append("\n" if char == "\n" else " ")
            i += 1
            continue
        if char in ("'", '"'):
            quote = char
            out.append(" ")
            i += 1
            continue
        if char == "-" and nxt == "-":
            line = True
            i += 2
            continue
        if char == "/" and nxt == "*":
            block = True
            i += 2
            continue
        out.append(char)
        i += 1
    return "".join(out)


def _normalize(sql: str) -> str:
    return re.sub(r"\s+", " ", _strip_comments(sql).upper())


def _violations(sql: str) -> list[str]:
    normalized = _normalize(sql)
    violations: list[str] = []
    for statement in normalized.split(";"):
        trimmed = statement.strip()
        if not trimmed:
            continue
        if any(trimmed.startswith(prefix) for prefix in FORBIDDEN_PREFIXES) or re.search(
            r"\bRENAME\s+TO\b", trimmed
        ):
            violations.append(trimmed[:120])
    return violations


def _snapshot(conn: sqlite3.Connection) -> dict[str, object]:
    objects = conn.execute(
        "SELECT type, name, sql FROM sqlite_master "
        "WHERE name IN ('tier_selections', 'stripe_checkout_sessions', "
        "'stripe_tier_drift_view', 'idx_tenant_active_subscription', "
        "'idx_tier_selections_tier', 'idx_tier_selections_state', "
        "'idx_stripe_sessions_tenant', 'idx_stripe_sessions_created') "
        "ORDER BY type, name"
    ).fetchall()
    return {
        "objects": objects,
        "tier_info": conn.execute("PRAGMA table_info(tier_selections)").fetchall(),
        "checkout_info": conn.execute(
            "PRAGMA table_info(stripe_checkout_sessions)"
        ).fetchall(),
        "foreign_keys": conn.execute(
            "PRAGMA foreign_key_list(stripe_checkout_sessions)"
        ).fetchall(),
        "tier_rows": conn.execute(
            "SELECT * FROM tier_selections ORDER BY tenant_id"
        ).fetchall(),
        "checkout_rows": conn.execute(
            "SELECT * FROM stripe_checkout_sessions ORDER BY session_id"
        ).fetchall(),
    }


def _new_legacy_database() -> sqlite3.Connection:
    conn = sqlite3.connect(":memory:")
    migrations = ROOT / "migrations/d1"
    conn.executescript((migrations / "0039_tier_selection.sql").read_text())
    conn.executescript((migrations / "0048_stripe_billing_materializer.sql").read_text())
    conn.executescript(
        "INSERT INTO tier_selections VALUES "
        "('tenant-legacy-team','team','active','cus_legacy',1700000000000,7,'corr-team');"
        "INSERT INTO tier_selections VALUES "
        "('tenant-legacy-free','free','inactive',NULL,NULL,1,'corr-free');"
        "INSERT INTO stripe_checkout_sessions VALUES "
        "('sess-legacy-team','tenant-legacy-team','team',1700000000100,'corr-team');"
    )
    return conn


def _apply(conn: sqlite3.Connection, sql: str) -> None:
    # Keep the migration's transaction boundary explicit, as D1 does.
    conn.executescript("BEGIN;\n" + sql + "\nCOMMIT;")


def _must_fail_partial_update(sql: str) -> None:
    executable = _strip_comments(sql)
    updates = list(re.finditer(r"UPDATE sqlite_master.*?;", executable, re.DOTALL))
    if len(updates) != 2:
        raise AssertionError(f"expected exactly two guarded catalog updates, got {len(updates)}")
    for index, update in enumerate(updates):
        mutated = executable[: update.start()] + executable[update.end() :]
        conn = _new_legacy_database()
        try:
            _apply(conn, mutated)
        except sqlite3.Error:
            continue
        raise AssertionError(f"partial catalog mutation was accepted: update {index}")


def _must_fail_catalog_comment_bait(sql: str) -> None:
    """The postcondition must inspect executable CHECKs, not catalog prose."""
    conn = _new_legacy_database()
    old_checks = (
        (
            "tier_selections",
            "CHECK (tier IN ('free', 'starter', 'team', 'pro', 'enterprise'))",
            "CHECK (tier IN ('free'))",
        ),
        (
            "stripe_checkout_sessions",
            "CHECK (tier IN ('starter', 'team', 'pro'))",
            "CHECK (tier IN ('team'))",
        ),
    )
    conn.execute("PRAGMA writable_schema = ON")
    for table, old, narrow in old_checks:
        catalog_sql = conn.execute(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?",
            (table,),
        ).fetchone()[0]
        if old not in catalog_sql:
            raise AssertionError(f"legacy CHECK fixture missing from {table}")
        conn.execute(
            "UPDATE sqlite_master SET sql = ? WHERE type = 'table' AND name = ?",
            (catalog_sql.replace(old, narrow) + f" -- bait {old}\n", table),
        )
    conn.execute("PRAGMA writable_schema = OFF")
    conn.execute("PRAGMA schema_version = 0")
    conn.execute("PRAGMA schema_version = 1")
    try:
        _apply(conn, sql)
    except sqlite3.Error:
        return
    raise AssertionError("catalog comment bait was accepted without widening CHECKs")


def _must_reject_scanner_mutations(sql: str) -> None:
    mutations = (
        "DROP VIEW IF EXISTS stripe_tier_drift_view;",
        "DROP TABLE tier_selections;",
        "ALTER TABLE tier_selections_new RENAME TO tier_selections;",
        "RENAME COLUMN tier TO old_tier;",
    )
    for mutation in mutations:
        if not _violations(sql + "\n" + mutation):
            raise AssertionError(f"destructive mutation was not detected: {mutation}")
    for bait in (
        "-- ALTER TABLE tier_selections_new RENAME TO tier_selections;",
        "SELECT 'ALTER TABLE tier_selections_new RENAME TO tier_selections';",
    ):
        if _violations(sql + "\n" + bait):
            raise AssertionError(f"comment/string bait was treated as SQL: {bait}")


def main() -> int:
    if not MIGRATION.is_file() or not PROPERTY.is_file():
        raise AssertionError("B-256 migration/property fixture is missing")
    migration = MIGRATION.read_text(encoding="utf-8")
    property_bytes = PROPERTY.read_bytes()
    if _blob_id(migration.encode("utf-8")) != EXPECTED_MIGRATION_BLOB:
        raise AssertionError("migration blob changed; review the executable contract")
    if _blob_id(property_bytes) != EXPECTED_PROPERTY_BLOB:
        raise AssertionError("additivity property blob changed; review the contract")
    if "prop_inv_auth_migration_additive_no_drop_alter_rename" not in property_bytes.decode():
        raise AssertionError("canonical additive property is missing")
    if _violations(migration):
        raise AssertionError(f"live migration still has destructive statements: {_violations(migration)}")
    for tier in SIX_TIERS:
        if f"''{tier}''" not in migration:
            raise AssertionError(f"six-tier value missing from migration: {tier}")
    for tier in CHECKOUT_TIERS:
        if f"''{tier}''" not in migration:
            raise AssertionError(f"checkout tier missing from migration: {tier}")
    if REGRESSION_SEED != 12030011770417089905:
        raise AssertionError("reviewed regression seed drifted")

    _must_reject_scanner_mutations(migration)
    _must_fail_partial_update(migration)
    _must_fail_catalog_comment_bait(migration)

    conn = _new_legacy_database()
    before = _snapshot(conn)
    _apply(conn, migration)
    after = _snapshot(conn)
    if before["tier_rows"] != after["tier_rows"] or before["checkout_rows"] != after["checkout_rows"]:
        raise AssertionError("0062 changed seeded rows")
    for value in SIX_TIERS:
        conn.execute(
            "INSERT INTO tier_selections "
            "(tenant_id,tier,subscription_state,subscription_started_at_ms,correlation_id) "
            "VALUES (?,?, 'inactive', NULL, ?)",
            (f"tenant-{value}", value, f"corr-{value}"),
        )
    for index, value in enumerate(CHECKOUT_TIERS):
        conn.execute(
            "INSERT INTO stripe_checkout_sessions "
            "(session_id,tenant_id,tier,created_at_ms,correlation_id) VALUES (?,?,?,?,?)",
            (f"sess-{value}", f"tenant-{value}", value, 1800000000000 + index, f"corr-{value}"),
        )
    for value in ("unknown", "runner"):
        try:
            conn.execute(
                "INSERT INTO tier_selections "
                "(tenant_id,tier,subscription_state,correlation_id) VALUES (?,?, 'inactive', ?)",
                (f"tenant-invalid-{value}", value, f"corr-invalid-{value}"),
            )
        except sqlite3.IntegrityError:
            continue
        raise AssertionError(f"tier CHECK was weakened for invalid value: {value}")
    for value in ("free", "enterprise"):
        try:
            conn.execute(
                "INSERT INTO stripe_checkout_sessions "
                "(session_id,tenant_id,tier,created_at_ms,correlation_id) VALUES (?,?,?,?,?)",
                (f"sess-invalid-{value}", "tenant-free", value, 1800000009999, "corr-invalid"),
            )
        except sqlite3.IntegrityError:
            continue
        raise AssertionError(f"checkout CHECK was weakened for invalid value: {value}")
    stable = _snapshot(conn)
    if before["tier_info"] != after["tier_info"] or before["checkout_info"] != after["checkout_info"]:
        raise AssertionError("0062 changed the legacy column schema")
    if before["foreign_keys"] != after["foreign_keys"]:
        raise AssertionError("0062 changed the checkout foreign key")
    before_objects = {(kind, name): definition for kind, name, definition in before["objects"]}  # type: ignore[misc]
    after_objects = {(kind, name): definition for kind, name, definition in after["objects"]}  # type: ignore[misc]
    if set(before_objects) != set(after_objects):
        raise AssertionError("0062 changed the durable object population")
    for key, definition in before_objects.items():
        if key[0] != "table" and after_objects[key] != definition:
            raise AssertionError(f"0062 changed index/view definition: {key[1]}")
    for table in ("tier_selections", "stripe_checkout_sessions"):
        if "solo" not in after_objects[("table", table)]:
            raise AssertionError(f"0062 did not widen {table}")

    _apply(conn, migration)
    if _snapshot(conn) != stable:
        raise AssertionError("0062 replay was not idempotent")

    print(
        "verify_b256_migration_additivity: PASS "
        f"(seed {REGRESSION_SEED}; rows/schema/view preserved; six tiers; "
        "replay-safe; 7 destructive/partial/comment-bait mutations rejected)"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, sqlite3.Error) as exc:
        print(f"verify_b256_migration_additivity: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1)
