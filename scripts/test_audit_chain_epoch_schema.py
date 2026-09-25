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
WITNESS_MIGRATION = REPO_ROOT / "migrations/d1/0124_audit_chain_witness_receipts.sql"
ADMIN_APPROVAL_MIGRATION = REPO_ROOT / "migrations/d1/0146_b054_admin_approval_ledger.sql"
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


def exercise_archive_final_cas() -> None:
    """Prove manifest publication and the exact row watermark commit together.

    This deliberately models only the final D1 boundary. Authentication and R2
    publication happen before this transaction; a stale/missing/mutated row at
    the last CAS must therefore roll the manifest index and every watermark
    back as one unit.
    """
    db = sqlite3.connect(":memory:")
    db.executescript(
        """
        CREATE TABLE archive_row (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            region TEXT NOT NULL,
            sequence_number INTEGER NOT NULL,
            chain_hash TEXT NOT NULL,
            archived_at INTEGER
        );
        CREATE TABLE archive_manifest_index (
            manifest_hash TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            region TEXT NOT NULL,
            start_sequence INTEGER NOT NULL,
            end_sequence_exclusive INTEGER NOT NULL
        );
        CREATE TABLE archive_commit_assert (
            commit_id TEXT PRIMARY KEY,
            assertion INTEGER NOT NULL CHECK (assertion = 1)
        );
        INSERT INTO archive_row VALUES
            ('row-0', 'tenant-archive', 'weur', 0, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', NULL),
            ('row-1', 'tenant-archive', 'weur', 1, 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', NULL);
        """
    )

    def final_commit(manifest_hash: str, expected_second_hash: str) -> None:
        db.execute(
            "INSERT INTO archive_manifest_index VALUES (?, 'tenant-archive', 'weur', 0, 2)",
            (manifest_hash,),
        )
        changed = db.execute(
            """
            UPDATE archive_row SET archived_at = 7
            WHERE tenant_id = 'tenant-archive' AND region = 'weur'
              AND archived_at IS NULL
              AND ((id = 'row-0' AND sequence_number = 0 AND chain_hash = ?)
                OR (id = 'row-1' AND sequence_number = 1 AND chain_hash = ?))
            RETURNING id
            """,
            ("a" * 64, expected_second_hash),
        ).fetchall()
        exact_ids = {row[0] for row in changed} == {"row-0", "row-1"}
        db.execute(
            "INSERT INTO archive_commit_assert VALUES (?, ?)",
            (f"commit-{manifest_hash}", int(exact_ids)),
        )

    # A changed immutable input makes the exact CAS affect only a subset. The
    # assertion latch aborts and rolls back both that partial watermark and the
    # manifest index inserted earlier in the transaction.
    try:
        db.execute("BEGIN")
        final_commit("1" * 64, "c" * 64)
        raise AssertionError("archive final CAS accepted a mutated row")
    except sqlite3.IntegrityError:
        db.rollback()
    assert db.execute("SELECT COUNT(*) FROM archive_manifest_index").fetchone() == (0,)
    assert db.execute(
        "SELECT COUNT(*) FROM archive_row WHERE archived_at IS NOT NULL"
    ).fetchone() == (0,)

    # A row claimed between R2 publication and final D1 commit is also stale.
    # Its pre-existing state remains, while the new manifest/partial update is
    # rolled back.
    db.execute("UPDATE archive_row SET archived_at = 3 WHERE id = 'row-1'")
    db.commit()
    try:
        db.execute("BEGIN")
        final_commit("2" * 64, "b" * 64)
        raise AssertionError("archive final CAS accepted a missing/stale row")
    except sqlite3.IntegrityError:
        db.rollback()
    assert db.execute("SELECT COUNT(*) FROM archive_manifest_index").fetchone() == (0,)
    assert db.execute("SELECT archived_at FROM archive_row WHERE id = 'row-0'").fetchone() == (None,)
    assert db.execute("SELECT archived_at FROM archive_row WHERE id = 'row-1'").fetchone() == (3,)

    # Restore the fixture and prove the exact set can commit. The latch itself
    # is transaction-local bookkeeping and is removed before COMMIT.
    db.execute("UPDATE archive_row SET archived_at = NULL WHERE id = 'row-1'")
    db.commit()
    db.execute("BEGIN")
    final_commit("3" * 64, "b" * 64)
    db.execute("DELETE FROM archive_commit_assert WHERE commit_id = ?", (f"commit-{'3' * 64}",))
    db.commit()
    assert db.execute("SELECT manifest_hash FROM archive_manifest_index").fetchone() == ("3" * 64,)
    assert db.execute("SELECT COUNT(*) FROM archive_row WHERE archived_at = 7").fetchone() == (2,)
    assert db.execute("SELECT COUNT(*) FROM archive_commit_assert").fetchone() == (0,)
    db.close()


def main() -> int:
    exercise_archive_final_cas()
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
    conn.executescript(WITNESS_MIGRATION.read_text(encoding="utf-8"))
    conn.executescript(ADMIN_APPROVAL_MIGRATION.read_text(encoding="utf-8"))
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

    # V2 head shape and the transaction assertion latch are load-bearing: a
    # zero-row CAS/partial row update must abort the same transaction that
    # attempted to persist the independently signed receipt.
    tenant_v2 = "00000000-0000-7000-8000-00000000aaaa"
    conn.execute(
        """
        INSERT INTO audit_chain_head
          (tenant_id,region,head_hash,next_sequence,updated_at,head_signature,
           head_signed_at_ms,signing_key_id,epoch_id,head_message_version,
           epoch_ledger_sequence,epoch_ledger_hash,head_witness_sequence,head_witness_hash)
        VALUES (?, 'weur', ?, 0, 0, ?, 0, 1, 0, 2, 0, ?, 0, ?)
        """,
        (tenant_v2, ZERO_HASH, SIGNATURE, "2" * 64, "3" * 64),
    )
    expect_integrity_error(
        conn,
        "partial v2 head",
        "INSERT INTO audit_chain_head (tenant_id,region,head_hash,next_sequence,updated_at,epoch_id) "
        "VALUES ('partial', 'weur', ?, 0, 0, 0)",
        (ZERO_HASH,),
    )
    expect_integrity_error(
        conn,
        "v2 head all-NULL downgrade",
        "UPDATE audit_chain_head SET epoch_id=NULL, head_message_version=NULL, "
        "epoch_ledger_sequence=NULL, epoch_ledger_hash=NULL, "
        "head_witness_sequence=NULL, head_witness_hash=NULL "
        "WHERE tenant_id=? AND region='weur'",
        (tenant_v2,),
    )
    expect_integrity_error(
        conn,
        "v2 witness sequence jump",
        "UPDATE audit_chain_head SET head_witness_sequence=2, head_witness_hash=? "
        "WHERE tenant_id=? AND region='weur'",
        ("4" * 64, tenant_v2),
    )
    expect_integrity_error(
        conn,
        "v2 head replay under a fresh witness index",
        "UPDATE audit_chain_head SET head_witness_sequence=1, head_witness_hash=? "
        "WHERE tenant_id=? AND region='weur'",
        ("4" * 64, tenant_v2),
    )
    receipt = (
        tenant_v2, "weur", 1, "4" * 64, "3" * 64, "5" * 64,
        b"{}", b"{}", SIGNATURE, "security-witness-1", 1, 1,
    )
    conn.commit()
    try:
        conn.execute("BEGIN")
        conn.execute(
            "INSERT INTO audit_chain_witness_receipt VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            receipt,
        )
        conn.execute(
            "UPDATE audit_chain_head SET head_hash=?, next_sequence=1, "
            "head_witness_sequence=1, head_witness_hash=? "
            "WHERE tenant_id=? AND region='weur'",
            ("6" * 64, "4" * 64, tenant_v2),
        )
        conn.execute(
            "INSERT INTO audit_chain_v2_tx_assert(commit_id, assertion) VALUES ('forced-zero-cas', 0)"
        )
        raise AssertionError("v2 transaction assertion unexpectedly accepted zero")
    except sqlite3.IntegrityError:
        conn.rollback()
    assert conn.execute(
        "SELECT head_witness_sequence FROM audit_chain_head WHERE tenant_id=?", (tenant_v2,)
    ).fetchone() == (0,)
    assert conn.execute("SELECT COUNT(*) FROM audit_chain_witness_receipt").fetchone() == (0,)

    conn.execute("INSERT INTO audit_chain_witness_receipt VALUES (?,?,?,?,?,?,?,?,?,?,?,?)", receipt)
    for label, sql in (
        ("replace", "INSERT OR REPLACE INTO audit_chain_witness_receipt VALUES (?,?,?,?,?,?,?,?,?,?,?,?)"),
        ("update", "UPDATE audit_chain_witness_receipt SET committed_at_ms=2 WHERE tenant_id=?"),
        ("delete", "DELETE FROM audit_chain_witness_receipt WHERE tenant_id=?"),
    ):
        params = receipt if label == "replace" else (tenant_v2,)
        expect_integrity_error(conn, f"witness receipt {label}", sql, params)

    # Exercise the exact forward-only E0->E1 database shape. A failure after
    # closing E0 must roll every projection/ledger/head mutation back; the same
    # shape then succeeds when the transaction assertion is true.
    tenant_transition = "00000000-0000-7000-8000-00000000bbbb"
    conn.execute(
        "INSERT INTO audit_chain_epoch_ledger VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            tenant_transition, "weur", 0, 1, "epoch-genesis", 0, ZERO_HASH,
            "8" * 64, b"{}", SIGNATURE, 1, 0,
        ),
    )
    conn.execute(
        "INSERT INTO audit_chain_epoch VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            tenant_transition, "weur", 0, 0, None, "active", 0, ZERO_HASH,
            None, 0, "8" * 64, None, None, None,
        ),
    )
    conn.execute(
        """
        INSERT INTO audit_chain_head
          (tenant_id,region,head_hash,next_sequence,updated_at,head_signature,
           head_signed_at_ms,signing_key_id,epoch_id,head_message_version,
           epoch_ledger_sequence,epoch_ledger_hash,head_witness_sequence,head_witness_hash)
        VALUES (?, 'weur', ?, 1, 0, ?, 0, 1, 0, 2, 0, ?, 0, ?)
        """,
        (tenant_transition, "c" * 64, SIGNATURE, "8" * 64, "9" * 64),
    )
    conn.commit()

    def apply_e1_projection(assertion: int) -> None:
        conn.execute(
            "INSERT INTO audit_chain_epoch_ledger VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                tenant_transition, "weur", 1, 1, "epoch-transition", 1,
                "8" * 64, "a" * 64, b"{}", SIGNATURE, 1, 1,
            ),
        )
        conn.execute(
            "UPDATE audit_chain_epoch SET state='closed',closed_at_ms=1,"
            "end_sequence_exclusive=1,end_head_hash=? "
            "WHERE tenant_id=? AND region='weur' AND epoch_id=0",
            ("c" * 64, tenant_transition),
        )
        conn.execute(
            "INSERT INTO audit_chain_epoch VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                tenant_transition, "weur", 1, 1, 1, "active", 1, "c" * 64,
                0, 1, "a" * 64, None, None, None,
            ),
        )
        conn.execute(
            "UPDATE audit_chain_head SET epoch_id=1,epoch_ledger_sequence=1,"
            "epoch_ledger_hash=?,head_signature=?,head_witness_sequence=1,"
            "head_witness_hash=? WHERE tenant_id=? AND region='weur'",
            ("a" * 64, SIGNATURE, "b" * 64, tenant_transition),
        )
        conn.execute(
            "INSERT INTO audit_chain_v2_tx_assert(commit_id, assertion) VALUES (?, ?)",
            (f"e1-{assertion}", assertion),
        )
        conn.execute(
            "DELETE FROM audit_chain_v2_tx_assert WHERE commit_id=?",
            (f"e1-{assertion}",),
        )

    try:
        conn.execute("BEGIN")
        apply_e1_projection(0)
        raise AssertionError("E1 forced assertion failure unexpectedly committed")
    except sqlite3.IntegrityError:
        conn.rollback()
    assert conn.execute(
        "SELECT state FROM audit_chain_epoch WHERE tenant_id=? AND epoch_id=0",
        (tenant_transition,),
    ).fetchone() == ("active",)
    assert conn.execute(
        "SELECT epoch_id,head_witness_sequence FROM audit_chain_head WHERE tenant_id=?",
        (tenant_transition,),
    ).fetchone() == (0, 0)
    assert conn.execute(
        "SELECT COUNT(*) FROM audit_chain_epoch_ledger WHERE tenant_id=?",
        (tenant_transition,),
    ).fetchone() == (1,)

    conn.execute("BEGIN")
    apply_e1_projection(1)
    conn.commit()
    assert conn.execute(
        "SELECT epoch_id,head_witness_sequence FROM audit_chain_head WHERE tenant_id=?",
        (tenant_transition,),
    ).fetchone() == (1, 1)

    assert conn.execute("SELECT COUNT(*) FROM audit_chain_epoch_ledger").fetchone() == (3,)
    assert conn.execute("SELECT COUNT(*) FROM audit_chain_epoch").fetchone() == (3,)
    assert conn.execute("SELECT COUNT(*) FROM audit_chain_archive_manifest").fetchone() == (1,)
    assert conn.execute("SELECT COUNT(*) FROM audit_chain_witness_receipt").fetchone() == (1,)
    approval = (
        "approval-1", "a" * 64, "opaque-sre-1", "opaque-security-2", "b" * 64,
        "e30=", SIGNATURE, 1_000, 2_000, 1_500,
    )
    conn.execute(
        "INSERT INTO audit_chain_admin_approval (approval_id,nonce_hash,executor_subject_id,approver_subject_id,operation_digest_hex,approval_jcs_b64,approval_signature_b64,issued_at_ms,expires_at_ms,consumed_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?)",
        approval,
    )
    expect_integrity_error(
        conn, "approval replay",
        "INSERT INTO audit_chain_admin_approval (approval_id,nonce_hash,executor_subject_id,approver_subject_id,operation_digest_hex,approval_jcs_b64,approval_signature_b64,issued_at_ms,expires_at_ms,consumed_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?)",
        ("approval-2", "a" * 64, *approval[2:]),
    )
    expect_integrity_error(conn, "approval update", "UPDATE audit_chain_admin_approval SET consumed_at_ms=1600 WHERE approval_id='approval-1'", ())
    expect_integrity_error(conn, "approval delete", "DELETE FROM audit_chain_admin_approval WHERE approval_id='approval-1'", ())
    print("OK: B-054 epoch schema constraints and no-replace guards hold with recursive_triggers=OFF")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, sqlite3.DatabaseError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1)
