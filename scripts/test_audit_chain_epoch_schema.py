#!/usr/bin/env python3
"""Focused SQLite regression for the B-054 epoch schema.

This builds the pre-0109 ``audit_chain_head`` shape in memory, applies the
epoch-contract migrations, and exercises constraints that prevent evidence
replacement when SQLite has ``recursive_triggers=OFF``.  Runtime hash/epoch
behaviour is covered by the Rust focal tests; this script remains intentionally
independent of deployment secrets and external witness state.
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
MIGRATION = REPO_ROOT / "migrations/d1/0109_audit_chain_epoch_contract.sql"
ROW_MIGRATION = REPO_ROOT / "migrations/d1/0110_audit_chain_epoch_row_metadata.sql"
ZERO_HASH = "0" * 64
SIGNING_PUBLIC_KEY = "A" * 44
SIGNATURE = "A" * 88


def expect_integrity_error(conn: sqlite3.Connection, label: str, sql: str, params: tuple[object, ...]) -> None:
    """Require an integrity error from a deliberate invalid SQL mutation."""
    try:
        conn.execute(sql, params)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"{label}: mutation unexpectedly succeeded")


def main() -> int:
    conn = sqlite3.connect(":memory:")
    conn.execute("PRAGMA foreign_keys = ON")
    conn.execute("PRAGMA recursive_triggers = OFF")
    conn.executescript(
        """
        CREATE TABLE audit_chain_head (
            tenant_id TEXT NOT NULL,
            region TEXT NOT NULL,
            head_hash TEXT NOT NULL,
            next_sequence INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            head_signature TEXT,
            head_signed_at_ms INTEGER,
            signing_key_id INTEGER,
            PRIMARY KEY (tenant_id, region)
        );
        """
    )
    # The row migration needs the pre-existing audit_outbox shape as well as
    # the head shape.  Existing rows intentionally retain NULL metadata.
    conn.execute(
        """
        CREATE TABLE audit_outbox (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            region TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            enqueued_at INTEGER NOT NULL,
            emitted_at INTEGER,
            sequence_number INTEGER,
            prev_hash TEXT,
            chain_hash TEXT,
            canonical_jcs TEXT,
            chained_at INTEGER
        )
        """
    )
    conn.executescript(MIGRATION.read_text(encoding="utf-8"))
    conn.executescript(ROW_MIGRATION.read_text(encoding="utf-8"))
    assert conn.execute("PRAGMA recursive_triggers").fetchone() == (0,)

    required_head_columns = {
        "epoch_id",
        "head_message_version",
        "epoch_ledger_sequence",
        "epoch_ledger_hash",
        "head_witness_sequence",
        "head_witness_hash",
    }
    actual_head_columns = {row[1] for row in conn.execute("PRAGMA table_info(audit_chain_head)")}
    assert required_head_columns <= actual_head_columns
    required_row_columns = {"algorithm_id", "epoch_id", "link_key_id"}
    actual_row_columns = {row[1] for row in conn.execute("PRAGMA table_info(audit_outbox)")}
    assert required_row_columns <= actual_row_columns

    def insert_outbox_metadata(suffix: str, algorithm: object, epoch_id: object, key_id: object) -> None:
        conn.execute(
            """
            INSERT INTO audit_outbox
                (id, tenant_id, region, payload_json, enqueued_at,
                 algorithm_id, epoch_id, link_key_id)
            VALUES (?, 'tenant-a', 'wnam', '{}', 1, ?, ?, ?)
            """,
            (f"metadata-{suffix}", algorithm, epoch_id, key_id),
        )

    # Historical NULLs, explicit E0, and complete keyed metadata are the only
    # accepted row spellings. The runtime/archive readers enforce the same
    # closed set before constructing a sealed line.
    insert_outbox_metadata("legacy", None, None, None)
    insert_outbox_metadata("e0", 0, 0, None)
    insert_outbox_metadata("keyed", 1, 1, 7)
    for label, values in {
        "partial-algorithm": (0, None, None),
        "partial-epoch": (None, 0, None),
        "partial-key": (1, 1, None),
        "downgrade-key": (0, 0, 7),
        "downgrade-epoch": (1, 0, 7),
        "negative-epoch": (1, -1, 7),
        "zero-key": (1, 1, 0),
        "text-epoch": (1, "x", 7),
        "text-key": (1, 1, "x"),
    }.items():
        expect_integrity_error(
            conn,
            f"sealed row metadata {label}",
            "INSERT INTO audit_outbox "
            "(id, tenant_id, region, payload_json, enqueued_at, algorithm_id, epoch_id, link_key_id) "
            "VALUES (?, 'tenant-a', 'wnam', '{}', 1, ?, ?, ?)",
            (f"metadata-{label}", *values),
        )
    for label, sql, params in (
        (
            "partial update",
            "UPDATE audit_outbox SET algorithm_id = 1 WHERE id = ?",
            ("metadata-e0",),
        ),
        (
            "E0 to legacy downgrade",
            "UPDATE audit_outbox SET algorithm_id = NULL, epoch_id = NULL, link_key_id = NULL WHERE id = ?",
            ("metadata-e0",),
        ),
        (
            "keyed to legacy downgrade",
            "UPDATE audit_outbox SET algorithm_id = NULL, epoch_id = NULL, link_key_id = NULL WHERE id = ?",
            ("metadata-keyed",),
        ),
        (
            "keyed to E0 downgrade",
            "UPDATE audit_outbox SET algorithm_id = 0, epoch_id = 0, link_key_id = NULL WHERE id = ?",
            ("metadata-keyed",),
        ),
        (
            "keyed epoch regression",
            "UPDATE audit_outbox SET algorithm_id = 1, epoch_id = 0, link_key_id = 7 WHERE id = ?",
            ("metadata-keyed",),
        ),
        (
            "keyed partial update",
            "UPDATE audit_outbox SET algorithm_id = 1, epoch_id = 2, link_key_id = NULL WHERE id = ?",
            ("metadata-keyed",),
        ),
        (
            "keyed key relabel",
            "UPDATE audit_outbox SET algorithm_id = 1, epoch_id = 1, link_key_id = 8 WHERE id = ?",
            ("metadata-keyed",),
        ),
    ):
        expect_integrity_error(conn, f"sealed row metadata {label}", sql, params)

    # Forward promotions remain valid, including a witnessed epoch advance.
    conn.execute(
        "UPDATE audit_outbox SET algorithm_id = 0, epoch_id = 0 WHERE id = ?",
        ("metadata-legacy",),
    )
    conn.execute(
        "UPDATE audit_outbox SET algorithm_id = 1, epoch_id = 1, link_key_id = 7 WHERE id = ?",
        ("metadata-e0",),
    )
    conn.execute(
        "UPDATE audit_outbox SET algorithm_id = 1, epoch_id = 2, link_key_id = 8 WHERE id = ?",
        ("metadata-keyed",),
    )

    signing = (1, "ed25519-v1", SIGNING_PUBLIC_KEY, "root-1", 1, b"{}", SIGNATURE, 0)
    link = (1, 1, "1" * 64, 1, b"{}", SIGNATURE, 1, 0)
    ledger = (
        "tenant-a",
        "wnam",
        0,
        1,
        "epoch-genesis",
        0,
        ZERO_HASH,
        "2" * 64,
        b"{}",
        SIGNATURE,
        1,
        0,
    )
    epoch = (
        "tenant-a",
        "wnam",
        0,
        0,
        None,
        "active",
        0,
        ZERO_HASH,
        None,
        0,
        "2" * 64,
        None,
        None,
        None,
    )
    manifest = (
        "tenant-a",
        "wnam",
        0,
        0,
        0,
        0,
        1,
        0,
        None,
        ZERO_HASH,
        ZERO_HASH,
        0,
        "4" * 64,
        0,
        "2" * 64,
        1,
        "3" * 64,
        b"{}",
        SIGNATURE,
        1,
        0,
    )

    conn.execute("INSERT INTO audit_chain_signing_key_registry VALUES (?, ?, ?, ?, ?, ?, ?, ?)", signing)
    conn.execute("INSERT INTO audit_chain_link_key_registry VALUES (?, ?, ?, ?, ?, ?, ?, ?)", link)
    conn.execute("INSERT INTO audit_chain_epoch_ledger VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", ledger)
    conn.execute("INSERT INTO audit_chain_epoch VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", epoch)
    conn.execute("INSERT INTO audit_chain_archive_manifest VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", manifest)

    # Explicit epoch shape: E0 cannot be keyed, and E>0 cannot be unkeyed or
    # lack the same-partition predecessor.  The ledger FK is otherwise valid so
    # the CHECK is what rejects each mutation.
    expect_integrity_error(
        conn,
        "E0 algorithm/key pairing",
        "INSERT INTO audit_chain_epoch VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("tenant-b", "wnam", 0, 1, 1, "active", 0, ZERO_HASH, None, 0, "2" * 64, None, None, None),
    )
    expect_integrity_error(
        conn,
        "E>0 predecessor/key pairing",
        "INSERT INTO audit_chain_epoch VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("tenant-a", "wnam", 1, 1, 1, "active", 1, "2" * 64, None, 0, "2" * 64, None, None, None),
    )
    expect_integrity_error(
        conn,
        "E>0 must use keyed algorithm",
        "INSERT INTO audit_chain_epoch VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("tenant-a", "wnam", 1, 0, None, "closed", 1, "2" * 64, 0, 0, "2" * 64, 1, 1, "2" * 64),
    )
    expect_integrity_error(
        conn,
        "cross-partition predecessor",
        "INSERT INTO audit_chain_epoch VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ("tenant-b", "wnam", 1, 1, 1, "active", 1, "2" * 64, 0, 0, "2" * 64, None, None, None),
    )

    # With recursive triggers disabled, INSERT OR REPLACE would otherwise erase
    # a conflicting row without running its DELETE trigger. Each BEFORE INSERT
    # conflict guard must reject replacement before SQLite can do that erase.
    replacements = (
        ("signing registry", "INSERT OR REPLACE INTO audit_chain_signing_key_registry VALUES (?, ?, ?, ?, ?, ?, ?, ?)", signing),
        ("link registry", "INSERT OR REPLACE INTO audit_chain_link_key_registry VALUES (?, ?, ?, ?, ?, ?, ?, ?)", link),
        ("epoch ledger", "INSERT OR REPLACE INTO audit_chain_epoch_ledger VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", ledger),
        ("epoch projection", "INSERT OR REPLACE INTO audit_chain_epoch VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", epoch),
        ("archive manifest", "INSERT OR REPLACE INTO audit_chain_archive_manifest VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", manifest),
    )
    for label, sql, params in replacements:
        expect_integrity_error(conn, f"{label} INSERT OR REPLACE", sql, params)

    expect_integrity_error(
        conn,
        "epoch ledger update",
        "UPDATE audit_chain_epoch_ledger SET created_at_ms = 1 WHERE tenant_id = ?",
        ("tenant-a",),
    )
    expect_integrity_error(
        conn,
        "epoch projection rewrite",
        "UPDATE audit_chain_epoch SET open_ledger_hash = ? WHERE tenant_id = ? AND region = ? AND epoch_id = ?",
        ("5" * 64, "tenant-a", "wnam", 0),
    )
    conn.execute(
        """
        UPDATE audit_chain_epoch
        SET state = 'closed', closed_at_ms = 1, end_sequence_exclusive = 0, end_head_hash = ?
        WHERE tenant_id = ? AND region = ? AND epoch_id = ?
        """,
        (ZERO_HASH, "tenant-a", "wnam", 0),
    )
    expect_integrity_error(
        conn,
        "epoch projection delete",
        "DELETE FROM audit_chain_epoch WHERE tenant_id = ? AND region = ? AND epoch_id = ?",
        ("tenant-a", "wnam", 0),
    )

    assert conn.execute("SELECT COUNT(*) FROM audit_chain_epoch_ledger").fetchone() == (1,)
    assert conn.execute("SELECT COUNT(*) FROM audit_chain_epoch").fetchone() == (1,)
    assert conn.execute("SELECT COUNT(*) FROM audit_chain_archive_manifest").fetchone() == (1,)
    print("OK: B-054 epoch schema constraints and no-replace guards hold with recursive_triggers=OFF")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, sqlite3.DatabaseError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1)
