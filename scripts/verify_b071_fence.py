#!/usr/bin/env python3
"""Focused B071 fence proof: writer blocking, retry, finalize, and idempotency.

This intentionally uses only the host Python SQLite engine.  It is a cheap
contract test for the migration and SQL guard; the bundled CI job remains the
authority for compiling the Rust adapters.
"""

from __future__ import annotations

import pathlib
import re
import sqlite3
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations/d1/0115_gc_purge_fence.sql"
CONTROL_MIGRATION = ROOT / "migrations/d1/0142_gc_accounting_legal_hold.sql"
FORWARD_CONTROL_MIGRATION = ROOT / "migrations/d1/0143_gc_accounting_region_upgrade.sql"
CAS_QUERY = ROOT / "crates/corelink-meta/src/cas_query.rs"
CAS_FENCE = ROOT / "crates/corelink-container/src/storage/cas_write_fence.rs"
CAS_OPS = ROOT / "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs"
CAS_WIRING = ROOT / "crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs"
GC_SWEEP = ROOT / "crates/corelink-container/src/gc_sweep/part-03.rs"
CANONICAL_DIGEST = "d" * 64


def legacy_control_migration() -> str:
    """Return the deployed 0118 trigger bodies before the gc-region repair."""
    current = CONTROL_MIGRATION.read_text()
    legacy, replacements = re.subn(
        r"""region = \(
\s+SELECT gc_region FROM gc_purge_intent
\s+WHERE tenant_id = OLD\.tenant_id
\s+AND digest = OLD\.digest
\s+AND state = 'r2_deleted'
\s+\)""",
        "region = OLD.region",
        current,
    )
    assert replacements == 2, "0118 legacy trigger fixture drifted"
    return legacy


def make_db(control_migration: str | None = None) -> sqlite3.Connection:
    db = sqlite3.connect(":memory:")
    db.executescript(
        """
        CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, primary_region TEXT NOT NULL);
        CREATE TABLE blob_meta (
          tenant_id TEXT NOT NULL, digest TEXT NOT NULL, size_bytes INTEGER NOT NULL,
          refcount INTEGER NOT NULL, created_at INTEGER NOT NULL,
          last_accessed_at INTEGER NOT NULL, deleted_at INTEGER,
          region TEXT NOT NULL, PRIMARY KEY (tenant_id, digest)
        );
        CREATE TABLE gc_candidates (
          tenant_id TEXT NOT NULL, digest TEXT NOT NULL, mark_run_id TEXT NOT NULL,
          status TEXT NOT NULL, physically_deleted_at_ms INTEGER,
          PRIMARY KEY (tenant_id, digest, mark_run_id)
        );
        CREATE TABLE audit_outbox (
          id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, digest TEXT,
          request_id TEXT NOT NULL, event_type TEXT NOT NULL,
          payload_json TEXT NOT NULL, enqueued_at INTEGER NOT NULL, region TEXT NOT NULL,
          UNIQUE(request_id, event_type)
        );
        CREATE TABLE tenant_legal_hold (
          tenant_id TEXT PRIMARY KEY, reason TEXT NOT NULL, held_at_ms INTEGER NOT NULL
        );
        CREATE TABLE tenant_storage_state (
          tenant_id TEXT NOT NULL, region TEXT NOT NULL, bytes_used INTEGER NOT NULL,
          bytes_quota INTEGER NOT NULL, bytes_used_updated_at_ms INTEGER NOT NULL,
          last_synced_at_ms INTEGER NOT NULL, last_evict_at_ms INTEGER,
          bytes_reclaimed_lifetime INTEGER NOT NULL, created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL, PRIMARY KEY (tenant_id, region)
        );
        """
    )
    db.executescript(MIGRATION.read_text())
    db.executescript(control_migration or CONTROL_MIGRATION.read_text())
    # The forward migration replaces the bodies left behind by an already
    # applied 0118. Replaying it must preserve the same fenced definitions.
    db.executescript(FORWARD_CONTROL_MIGRATION.read_text())
    db.executescript(FORWARD_CONTROL_MIGRATION.read_text())
    db.executescript(MIGRATION.read_text())
    db.execute("INSERT INTO tenant VALUES ('t1', 'wnam')")
    # tenant_storage_state.region is the five-region GC partition, while
    # blob_meta.region is the tenant macro-residency region.  Keep them
    # intentionally different so the accounting trigger cannot regress to
    # comparing the wrong region domain.
    db.execute(
        "INSERT INTO tenant_storage_state VALUES ('t1', 'iad', 10, 100, 1, 1, NULL, 0, 1, 1)"
    )
    db.execute(
        "INSERT INTO blob_meta VALUES ('t1', ?, 10, 0, 1, 1, 1, 'wnam')",
        (CANONICAL_DIGEST,),
    )
    db.execute(
        "INSERT INTO gc_candidates VALUES ('t1', ?, 'run1', 'swept', NULL)",
        (CANONICAL_DIGEST,),
    )
    db.commit()
    return db


def acquire(db: sqlite3.Connection, now: int = 10_000) -> int | None:
    row = db.execute(
        """
        INSERT INTO gc_purge_intent
          (tenant_id,digest,epoch,state,r2_key,gc_region,residency_region,
           mark_run_id,started_at,updated_at)
        SELECT 't1',?1,COALESCE((SELECT MAX(epoch) FROM gc_purge_intent
                                   WHERE tenant_id='t1' AND digest=?1),0)+1,
               'purging','iad/p/d1','iad',
               t.primary_region,'run1',?2,?2
        FROM blob_meta AS bm JOIN tenant AS t ON t.tenant_id=bm.tenant_id
        WHERE bm.tenant_id='t1' AND bm.digest=?1 AND bm.refcount=0
          AND bm.deleted_at IS NOT NULL AND bm.region=t.primary_region
          AND NOT EXISTS (SELECT 1 FROM gc_purge_intent AS pi
                          WHERE pi.tenant_id=bm.tenant_id AND pi.digest=bm.digest
                            AND pi.state IN ('purging','r2_deleted','retry'))
          AND NOT EXISTS (SELECT 1 FROM cas_write_intent AS wi
                                  WHERE wi.tenant_id=bm.tenant_id AND wi.digest=bm.digest
                                    AND wi.state='writing'
                                    AND (?2 - wi.updated_at) <= 900000)
        ON CONFLICT (tenant_id,digest) DO NOTHING RETURNING epoch
        """,
        (CANONICAL_DIGEST, now),
    ).fetchone()
    return None if row is None else int(row[0])


def begin_writer(db: sqlite3.Connection, now: int = 20_000) -> str | None:
    request_id = f"cas-write:t1:{CANONICAL_DIGEST}:{now}"
    row = db.execute(
        """
        INSERT INTO cas_write_intent
          (tenant_id,digest,request_id,state,started_at,updated_at)
        SELECT 't1',?3,?1,'writing',?2,?2
        WHERE NOT EXISTS (SELECT 1 FROM gc_purge_intent AS pi
                          WHERE pi.tenant_id='t1' AND pi.digest=?3
                            AND pi.state IN ('purging','r2_deleted','retry'))
          AND NOT EXISTS (SELECT 1 FROM blob_meta AS bm
                          WHERE bm.tenant_id='t1' AND bm.digest=?3
                            AND bm.deleted_at IS NOT NULL)
          AND NOT EXISTS (SELECT 1 FROM cas_write_intent AS wi
                          WHERE wi.tenant_id='t1' AND wi.digest=?3
                            AND wi.state='writing')
        RETURNING request_id
        """,
        (request_id, now, CANONICAL_DIGEST),
    ).fetchone()
    return None if row is None else str(row[0])


def commit_writer(db: sqlite3.Connection, request_id: str, now: int = 20_001) -> bool:
    row = db.execute(
        """
        INSERT INTO blob_meta
          (tenant_id,digest,size_bytes,refcount,created_at,last_accessed_at,region)
        SELECT 't1',?3,10,1,?2,?2,
               (SELECT primary_region FROM tenant WHERE tenant_id='t1')
        WHERE EXISTS (SELECT 1 FROM cas_write_intent AS wi
                      WHERE wi.tenant_id='t1' AND wi.digest=?3
                        AND wi.request_id=?1 AND wi.state='writing')
          AND NOT EXISTS (SELECT 1 FROM gc_purge_intent AS pi
                          WHERE pi.tenant_id='t1' AND pi.digest=?3
                            AND pi.state IN ('purging','r2_deleted','retry'))
        ON CONFLICT (tenant_id,digest) DO UPDATE SET
          refcount=blob_meta.refcount+1,last_accessed_at=excluded.last_accessed_at
        WHERE blob_meta.deleted_at IS NULL
        RETURNING refcount
        """,
        (request_id, now, CANONICAL_DIGEST),
    ).fetchone()
    return row is not None


def release_writer(db: sqlite3.Connection, request_id: str) -> None:
    db.execute(
        """DELETE FROM cas_write_intent
           WHERE tenant_id='t1' AND digest=? AND request_id=? AND state='writing'""",
        (CANONICAL_DIGEST, request_id),
    )


def live_wiring_errors(ops: str, wiring: str, gc: str) -> list[str]:
    errors = []
    put_tokens = ("self.client.put_if_absent(", "self.client.put(")
    try:
        put_index = min(ops.index(token) for token in put_tokens)
        if ops.index("fence.begin(") >= put_index:
            errors.append("fence is claimed after R2")
    except ValueError:
        errors.append("fence begin or R2 PUT is missing")
    if "fence.commit(" not in ops:
        errors.append("metadata commit is missing")
    if "self.client.delete(&key)" not in ops:
        errors.append("fresh-R2 compensation is missing")
    if "ByokBodyPlan::Random { ctx }" not in ops or "record_reconciliation_intent" not in ops:
        errors.append("Mode-B metadata failure has no durable reconciliation hand-off")
    if "if dedup_eligible && r2_fresh" not in ops:
        errors.append("metadata failure may delete an ambiguous Mode-B object")
    elif "record_reconciliation_intent" in ops and ops.index("record_reconciliation_intent") > ops.index("if dedup_eligible && r2_fresh"):
        errors.append("Mode-B reconciliation intent is recorded after compensation")
    if ".with_cas_write_fence(cas_write_fence)" not in wiring:
        errors.append("production builder does not wire the fence")
    if (
        "cas_write_intent AS wi" not in gc
        or "wi.state = 'writing'" not in gc
        or "updated_at) <= {CAS_WRITE_LEASE_MS}" not in gc
        or "CAS_WRITE_LEASE_MS" not in gc
    ):
        errors.append("GC does not exclude live writers with the bounded lease")
    return errors


def main() -> None:
    sql = CAS_QUERY.read_text()
    increment = sql.split("pub const UPDATE_BLOB_META_INCREMENT_REFCOUNT", 1)[1].split(
        "pub const", 1
    )[0]
    required = ("NOT EXISTS", "gc_purge_intent", "'purging'", "'r2_deleted'", "'retry'")
    assert all(token in increment for token in required), "writer SQL is not fenced"
    mutated = increment.replace("AND NOT EXISTS", "AND /* mutation */ 1=1", 1)
    assert not all(token in mutated for token in required), "guard mutation escaped detection"

    fence = CAS_FENCE.read_text()
    for token in (
        "trait CasWriteFence",
        "fn begin(",
        "fn commit(",
        "fn abort(",
        "cas_write_intent",
        "gc_purge_intent",
        "deleted_at IS NOT NULL",
        "state IN ('purging', 'r2_deleted', 'retry')",
        "request_id = ?5",
        "blob_meta",
    ):
        assert token in fence, f"live D1 fence missing load-bearing token: {token}"
    ops = CAS_OPS.read_text()
    wiring = CAS_WIRING.read_text()
    gc = GC_SWEEP.read_text()
    migration = MIGRATION.read_text()
    control_migration = CONTROL_MIGRATION.read_text()
    forward_control_migration = FORWARD_CONTROL_MIGRATION.read_text()
    assert not live_wiring_errors(ops, wiring, gc), live_wiring_errors(ops, wiring, gc)
    for token in (
        "trg_gc_purge_reclaim_stale_cas_writer",
        "NEW.started_at - updated_at",
        "cas_reconciliation_intent",
        "corelink.cas.reconciliation_required",
        "digest GLOB 'blake3:[0-9a-f]*'",
        "trg_blob_meta_digest_canonical_insert",
        "trg_gc_candidates_digest_canonical_insert",
    ):
        assert token in migration, f"migration missing B071 adjudicated invariant: {token}"
    for token in (
        "trg_gc_purge_legal_hold_guard",
        "gc_purge_blocked_by_legal_hold",
        "trg_gc_purge_accounting_required",
        "gc_purge_accounting_state_missing",
        "trg_gc_purge_finalize_accounting",
        "bytes_reclaimed_lifetime",
        "MAX(0, bytes_used - OLD.size_bytes)",
    ):
        assert token in control_migration, f"GC control migration missing {token}"
    for token in (
        "DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required",
        "DROP TRIGGER IF EXISTS trg_gc_purge_finalize_accounting",
        "gc_region FROM gc_purge_intent",
        "MAX(0, bytes_used - OLD.size_bytes)",
        "bytes_reclaimed_lifetime",
    ):
        assert token in forward_control_migration, f"GC forward migration missing {token}"
    assert "algorithm-tagged digest" not in fence, \
        "writer fence still requires an algorithm prefix at the metadata boundary"
    assert "&canonical_meta_digest(&req.claimed_hash)" in ops, \
        "CAS metadata path does not use the shared plain digest representation"
    for name, mutant in (
        ("removed begin", ops.replace("fence.begin(", "", 1)),
        ("removed commit", ops.replace("fence.commit(", "")),
        ("removed production wiring", wiring.replace(
            ".with_cas_write_fence(cas_write_fence)", "", 1
        )),
        ("GC writer exclusion", gc.replace("cas_write_intent AS wi", "", 1)),
        ("Mode-B reconciliation", ops.replace("record_reconciliation_intent", "removed_reconciliation_intent", 1)),
    ):
        mutant_errors = live_wiring_errors(
            mutant if name != "removed production wiring" and name != "GC writer exclusion" else ops,
            mutant if name == "removed production wiring" else wiring,
            mutant if name == "GC writer exclusion" else gc,
        )
        assert mutant_errors, f"wiring mutation unexpectedly passed: {name}"
    assert ops.count("fence.begin(") == 1, "fence begin must have one acquisition site"
    assert "with_cas_write_fence(cas_write_fence)" not in wiring.replace(
        ".with_cas_write_fence(cas_write_fence)", "", 1
    ), "fence wiring appears more than once"

    # Fresh installs execute the corrected 0118 followed by 0143. Existing
    # installs execute the historic 0118 bodies followed by 0143. Exercise
    # the latter sequence below because it is the deployed-upgrade seam.
    fresh = make_db()
    for trigger in (
        "trg_gc_purge_accounting_required",
        "trg_gc_purge_finalize_accounting",
    ):
        trigger_sql = fresh.execute(
            "SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?", (trigger,)
        ).fetchone()[0]
        assert "gc_region" in trigger_sql and "OLD.region" not in trigger_sql, \
            f"fresh install retained the obsolete {trigger} body"

    db = make_db(legacy_control_migration())
    # The purge acquisition boundary is transactional: a lease older than the
    # bounded TTL is deleted by the migration trigger in the same transaction
    # that creates epoch 1, while a lease exactly at the boundary still wins
    # over GC.  A late stale writer token cannot commit after reclamation.
    db.execute("UPDATE blob_meta SET deleted_at=1, refcount=0")
    db.execute(
        "INSERT INTO cas_write_intent VALUES ('t1', ?, 'stale-epoch-1', 'writing', 1, 1)",
        (CANONICAL_DIGEST,),
    )
    assert acquire(db, now=901_001) == 1, "expired writer lease blocked purge acquisition"
    assert db.execute("SELECT COUNT(*) FROM cas_write_intent").fetchone()[0] == 0
    assert not commit_writer(db, "stale-epoch-1", now=901_002), \
        "stale writer token committed after purge acquired a new epoch"

    boundary = make_db()
    boundary.execute("UPDATE blob_meta SET deleted_at=1, refcount=0")
    boundary.execute(
        "INSERT INTO cas_write_intent VALUES ('t1', ?, 'boundary', 'writing', 1, 1)",
        (CANONICAL_DIGEST,),
    )
    assert acquire(boundary, now=900_001) is None, \
        "writer lease exactly at TTL boundary was reclaimed too early"
    assert boundary.execute("SELECT COUNT(*) FROM cas_write_intent").fetchone()[0] == 1

    # A strictly older lease is then reclaimed and acquisition proceeds.
    assert acquire(boundary, now=900_002) == 1, \
        "strictly expired writer lease was not reclaimed"
    assert boundary.execute("SELECT COUNT(*) FROM cas_write_intent").fetchone()[0] == 0

    # Continue with the live-writer race on a clean metadata row.
    db = make_db()
    db.execute("UPDATE blob_meta SET deleted_at=1")
    assert begin_writer(db) is None, "writer bypassed an existing tombstone before R2"
    db.execute("UPDATE blob_meta SET deleted_at=NULL, refcount=1")
    # A live writer wins the D1 race and blocks GC acquisition. Its metadata
    # commit increments the real row and the trigger releases only its lease.
    writer = begin_writer(db)
    assert writer is not None
    assert acquire(db) is None, "GC acquired while a real CAS writer held its lease"
    assert commit_writer(db, writer), "live CAS metadata commit was rejected"
    assert db.execute("SELECT refcount FROM blob_meta").fetchone()[0] == 2
    release_writer(db, writer)
    assert db.execute("SELECT COUNT(*) FROM cas_write_intent").fetchone()[0] == 0

    # An active purge blocks a new live writer before R2. Tombstoned metadata
    # remains non-resurrectable even when the purge row is removed.
    db.execute("UPDATE blob_meta SET deleted_at=1, refcount=0")
    epoch = acquire(db)
    assert epoch == 1
    assert begin_writer(db) is None, "writer bypassed active GC purge intent"
    assert acquire(db) is None, "two concurrent owners acquired one epoch"

    # The canonical writer predicate fails closed in every active state.
    for state in ("purging", "r2_deleted", "retry"):
        db.execute("UPDATE gc_purge_intent SET state=? WHERE epoch=1", (state,))
        changed = db.execute(
            """UPDATE blob_meta SET refcount=refcount+1, deleted_at=NULL
               WHERE tenant_id='t1' AND digest=? AND deleted_at IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM gc_purge_intent AS pi
                   WHERE pi.tenant_id=blob_meta.tenant_id AND pi.digest=blob_meta.digest
                     AND pi.state IN ('purging','r2_deleted','retry'))""",
            (CANONICAL_DIGEST,),
        ).rowcount
        assert changed == 0, f"writer bypassed {state} fence"

    # R2 failure is durable and leaves the D1 row intact.
    db.execute("UPDATE gc_purge_intent SET state='retry', last_error='5xx' WHERE epoch=1")
    assert db.execute("SELECT 1 FROM blob_meta").fetchone() is not None

    # Retry success + conditional finalize: trigger performs audit, candidate,
    # and intent transitions in the same SQLite transaction as the DELETE.
    db.execute("UPDATE gc_purge_intent SET state='r2_deleted', epoch=2, updated_at=20000")
    # A stale worker carrying epoch 1 cannot finalize the newer attempt.
    db.execute(
        """DELETE FROM blob_meta WHERE tenant_id='t1' AND digest=?
           AND EXISTS (SELECT 1 FROM gc_purge_intent WHERE tenant_id='t1'
             AND digest=? AND epoch=1 AND state='r2_deleted')""",
        (CANONICAL_DIGEST, CANONICAL_DIGEST),
    )
    assert db.execute("SELECT 1 FROM blob_meta").fetchone() is not None
    db.execute(
        """DELETE FROM blob_meta WHERE tenant_id='t1' AND digest=?
           AND refcount=0 AND deleted_at IS NOT NULL
           AND EXISTS (SELECT 1 FROM gc_purge_intent WHERE tenant_id='t1'
             AND digest=? AND epoch=2 AND state='r2_deleted')""",
        (CANONICAL_DIGEST, CANONICAL_DIGEST),
    )
    assert db.execute("SELECT 1 FROM blob_meta").fetchone() is None
    assert db.execute("SELECT COUNT(*) FROM audit_outbox").fetchone()[0] == 1
    assert db.execute("SELECT status FROM gc_candidates").fetchone()[0] == "physically_deleted"
    assert db.execute("SELECT state FROM gc_purge_intent").fetchone()[0] == "finalized"
    assert db.execute(
        "SELECT bytes_used, bytes_reclaimed_lifetime FROM tenant_storage_state"
    ).fetchone() == (0, 10)

    # A hold placed after acquisition still blocks the irreversible D1
    # boundary.  The intent remains recoverable and the metadata/accounting
    # rows remain untouched.
    held = make_db()
    held.execute("UPDATE blob_meta SET deleted_at=1, refcount=0")
    assert acquire(held) == 1
    held.execute(
        "INSERT INTO tenant_legal_hold VALUES ('t1', 'case-1651', 20000)"
    )
    held.execute("UPDATE gc_purge_intent SET state='r2_deleted', updated_at=20000")
    try:
        held.execute(
            """DELETE FROM blob_meta WHERE tenant_id='t1' AND digest=?
               AND refcount=0 AND deleted_at IS NOT NULL
               AND EXISTS (SELECT 1 FROM gc_purge_intent WHERE tenant_id='t1'
                 AND digest=? AND state='r2_deleted')""",
            (CANONICAL_DIGEST, CANONICAL_DIGEST),
        )
    except sqlite3.IntegrityError as error:
        assert "legal_hold" in str(error), f"unexpected hold rejection: {error}"
    else:
        raise AssertionError("GC finalized a blob under an active legal hold")
    assert held.execute("SELECT 1 FROM blob_meta").fetchone() is not None
    assert held.execute(
        "SELECT bytes_used FROM tenant_storage_state"
    ).fetchone()[0] == 10

    # A replay cannot delete or emit a second event.
    db.execute(
        "DELETE FROM blob_meta WHERE tenant_id='t1' AND digest=?",
        (CANONICAL_DIGEST,),
    )
    assert db.execute("SELECT COUNT(*) FROM audit_outbox").fetchone()[0] == 1

    # A Mode-B metadata failure has a durable, auditable recovery hand-off.
    reconcile = make_db()
    reconcile.execute(
        """INSERT INTO cas_reconciliation_intent
           (tenant_id, digest, surface, physical_r2_key, state, reason,
            attempts, created_at_ms, updated_at_ms)
           VALUES ('t1', ?, 'cas', 'iad/t/payload', 'pending',
                   'mode-b metadata commit failed', 0, 30, 30)""",
        (CANONICAL_DIGEST,),
    )
    assert reconcile.execute(
        "SELECT state, reason FROM cas_reconciliation_intent"
    ).fetchone() == ("pending", "mode-b metadata commit failed")
    assert reconcile.execute(
        "SELECT event_type FROM audit_outbox"
    ).fetchone()[0] == "corelink.cas.reconciliation_required"

    # Legacy blake3 prefixes are backfilled; a different algorithm prefix is
    # intentionally not collapsed and new ambiguous values are rejected.
    legacy = sqlite3.connect(":memory:")
    legacy.executescript(
        """
        CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, primary_region TEXT NOT NULL);
        CREATE TABLE blob_meta (
          tenant_id TEXT NOT NULL, digest TEXT NOT NULL, size_bytes INTEGER NOT NULL,
          refcount INTEGER NOT NULL, created_at INTEGER NOT NULL,
          last_accessed_at INTEGER NOT NULL, deleted_at INTEGER, region TEXT NOT NULL,
          PRIMARY KEY (tenant_id, digest));
        CREATE TABLE gc_candidates (
          tenant_id TEXT NOT NULL, digest TEXT NOT NULL, mark_run_id TEXT NOT NULL,
          status TEXT NOT NULL, physically_deleted_at_ms INTEGER,
          PRIMARY KEY (tenant_id, digest, mark_run_id));
        CREATE TABLE audit_outbox (
          id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, digest TEXT,
          request_id TEXT NOT NULL, event_type TEXT NOT NULL, payload_json TEXT NOT NULL,
          enqueued_at INTEGER NOT NULL, region TEXT NOT NULL,
          UNIQUE(request_id, event_type));
        INSERT INTO tenant VALUES ('t1', 'wnam');
        """
    )
    old = "blake3:" + CANONICAL_DIGEST
    other = "sha256:" + CANONICAL_DIGEST
    legacy.execute("INSERT INTO blob_meta VALUES ('t1', ?, 1, 1, 1, 1, NULL, 'wnam')", (old,))
    legacy.execute("INSERT INTO gc_candidates VALUES ('t1', ?, 'run', 'candidate', NULL)", (old,))
    legacy.execute("INSERT INTO blob_meta VALUES ('t1', ?, 1, 1, 1, 1, NULL, 'wnam')", (other,))
    legacy.executescript(MIGRATION.read_text())
    assert legacy.execute("SELECT digest FROM blob_meta WHERE digest=?", (CANONICAL_DIGEST,)).fetchone()
    assert legacy.execute(
        "SELECT digest FROM gc_candidates WHERE digest=?", (CANONICAL_DIGEST,)
    ).fetchone()
    assert legacy.execute("SELECT digest FROM blob_meta WHERE digest=?", (other,)).fetchone()
    try:
        legacy.execute(
            "INSERT INTO blob_meta VALUES ('t1', ?, 1, 1, 1, 1, NULL, 'wnam')",
            ("sha256:" + "e" * 64,),
        )
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("ambiguous algorithm-prefixed metadata insert was accepted")
    print("B071 fence PASS: writer blocked; retry preserved D1; finalize/audit atomic; replay idempotent")


if __name__ == "__main__":
    main()
