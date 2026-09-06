-- CoreLink D1 — B071 fenced GC purge protocol.
--
-- The R2 object and D1 metadata live in different systems, so a delete is
-- deliberately represented as a durable intent.  A writer must not revive a
-- row while an intent is active; a crashed worker can therefore resume from
-- `purging`, `retry`, or `r2_deleted` without guessing what happened remotely.
-- This migration is additive and idempotent.  D1 migrations run in an
-- implicit transaction; no BEGIN/COMMIT is used here.

CREATE TABLE IF NOT EXISTS gc_purge_intent (
    tenant_id          TEXT NOT NULL,
    digest             TEXT NOT NULL,
    epoch              INTEGER NOT NULL CHECK (epoch > 0),
    state              TEXT NOT NULL CHECK (state IN ('purging', 'r2_deleted', 'retry', 'finalized')),
    r2_key             TEXT NOT NULL,
    gc_region          TEXT NOT NULL CHECK (gc_region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),
    residency_region   TEXT NOT NULL CHECK (residency_region IN ('wnam', 'enam', 'weur', 'sam', 'apac', 'afr')),
    mark_run_id        TEXT NOT NULL,
    started_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    last_error         TEXT,
    PRIMARY KEY (tenant_id, digest),
    UNIQUE (tenant_id, digest, epoch)
);

CREATE INDEX IF NOT EXISTS idx_gc_purge_intent_state
    ON gc_purge_intent (state, updated_at);

-- CAS re-reference/write paths use this predicate.  A row in any non-final
-- state is fenced; callers must retry after reconciliation rather than clear
-- `deleted_at` behind the GC worker's epoch.
CREATE INDEX IF NOT EXISTS idx_gc_purge_intent_active
    ON gc_purge_intent (tenant_id, digest)
    WHERE state IN ('purging', 'r2_deleted', 'retry');

-- Native R2 writes cannot hold a D1 transaction across the remote PUT.  This
-- short-lived lease is the reciprocal side of the purge fence: GC acquisition
-- excludes `state = 'writing'`, while the CAS metadata commit consumes the
-- exact request token.  The request token also fences a crashed writer after
-- its bounded lease is reclaimed by a retrying writer.
CREATE TABLE IF NOT EXISTS cas_write_intent (
    tenant_id  TEXT NOT NULL,
    digest     TEXT NOT NULL,
    request_id TEXT NOT NULL,
    state      TEXT NOT NULL CHECK (state = 'writing'),
    started_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, digest),
    UNIQUE (tenant_id, digest, request_id)
);

CREATE INDEX IF NOT EXISTS idx_cas_write_intent_active
    ON cas_write_intent (tenant_id, digest, state, updated_at);

-- The purge INSERT carries the caller-supplied `started_at` clock.  Its
-- trigger runs inside the same SQLite transaction as that INSERT, so a stale
-- writer lease is removed atomically with acquisition.  The matching
-- `NOT EXISTS` in gc_sweep.rs ignores the same stale boundary before this
-- trigger fires; live leases (age <= 900000 ms) still block acquisition.
CREATE TRIGGER IF NOT EXISTS trg_gc_purge_reclaim_stale_cas_writer
BEFORE INSERT ON gc_purge_intent
FOR EACH ROW
BEGIN
    DELETE FROM cas_write_intent
    WHERE tenant_id = NEW.tenant_id
      AND digest = NEW.digest
      AND state = 'writing'
      AND (NEW.started_at - updated_at) > 900000;
END;

-- Metadata and GC use one canonical identity: plain, lowercase 64-hex.  The
-- algorithm is selected at the external route/parser boundary.  Existing
-- deployments that used the old `blake3:<hex>` spelling are backfilled; a
-- `sha256:<hex>` (or any other prefixed/ambiguous row) is intentionally left
-- untouched and therefore fails closed at the typed read boundary rather than
-- silently collapsing two algorithms into one key.
UPDATE blob_meta
SET digest = substr(digest, 8)
WHERE digest GLOB 'blake3:[0-9a-f]*'
  AND length(digest) = 71
  AND substr(digest, 8) NOT GLOB '*[^0-9a-f]*';

UPDATE gc_candidates
SET digest = substr(digest, 8)
WHERE digest GLOB 'blake3:[0-9a-f]*'
  AND length(digest) = 71
  AND substr(digest, 8) NOT GLOB '*[^0-9a-f]*';

-- SQLite cannot add a CHECK constraint to an already deployed table without
-- rebuilding it.  These equivalent triggers keep every future write
-- canonical while preserving additive migration semantics.
CREATE TRIGGER IF NOT EXISTS trg_blob_meta_digest_canonical_insert
BEFORE INSERT ON blob_meta
FOR EACH ROW
WHEN length(NEW.digest) <> 64 OR NEW.digest GLOB '*[^0-9a-f]*'
BEGIN
    SELECT RAISE(ABORT, 'blob_meta.digest must be plain lowercase 64-hex');
END;

CREATE TRIGGER IF NOT EXISTS trg_blob_meta_digest_canonical_update
BEFORE UPDATE OF digest ON blob_meta
FOR EACH ROW
WHEN length(NEW.digest) <> 64 OR NEW.digest GLOB '*[^0-9a-f]*'
BEGIN
    SELECT RAISE(ABORT, 'blob_meta.digest must be plain lowercase 64-hex');
END;

CREATE TRIGGER IF NOT EXISTS trg_gc_candidates_digest_canonical_insert
BEFORE INSERT ON gc_candidates
FOR EACH ROW
WHEN length(NEW.digest) <> 64 OR NEW.digest GLOB '*[^0-9a-f]*'
BEGIN
    SELECT RAISE(ABORT, 'gc_candidates.digest must be plain lowercase 64-hex');
END;

CREATE TRIGGER IF NOT EXISTS trg_gc_candidates_digest_canonical_update
BEFORE UPDATE OF digest ON gc_candidates
FOR EACH ROW
WHEN length(NEW.digest) <> 64 OR NEW.digest GLOB '*[^0-9a-f]*'
BEGIN
    SELECT RAISE(ABORT, 'gc_candidates.digest must be plain lowercase 64-hex');
END;

-- A Mode-B object can outlive a failed metadata commit because its blind R2
-- PUT is not safely compensable.  This is the durable reconciliation queue;
-- the AFTER INSERT trigger emits an audit_outbox event in the same D1
-- transaction, so the orphan is never silent debt.
CREATE TABLE IF NOT EXISTS cas_reconciliation_intent (
    tenant_id        TEXT NOT NULL,
    digest           TEXT NOT NULL,
    surface          TEXT NOT NULL CHECK (surface = 'cas'),
    physical_r2_key  TEXT NOT NULL,
    state            TEXT NOT NULL CHECK (state IN ('pending', 'resolved', 'quarantined')),
    reason           TEXT NOT NULL,
    attempts         INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    created_at_ms    INTEGER NOT NULL,
    updated_at_ms    INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, digest, surface),
    CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*')
);

CREATE INDEX IF NOT EXISTS idx_cas_reconciliation_pending
    ON cas_reconciliation_intent (state, updated_at_ms);

CREATE TRIGGER IF NOT EXISTS trg_cas_reconciliation_intent_audit
AFTER INSERT ON cas_reconciliation_intent
FOR EACH ROW
BEGIN
    INSERT OR IGNORE INTO audit_outbox
        (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, region)
    VALUES (
        'cas-reconcile:' || NEW.tenant_id || ':' || NEW.digest || ':' || NEW.surface,
        NEW.tenant_id,
        NEW.digest,
        'cas-reconcile:' || NEW.tenant_id || ':' || NEW.digest || ':' || NEW.surface,
        'corelink.cas.reconciliation_required',
        '{"event_type":"corelink.cas.reconciliation_required","tenant_id":"' || NEW.tenant_id || '","digest":"' || NEW.digest || '","surface":"' || NEW.surface || '","reason":"' || NEW.reason || '","physical_r2_key":"' || NEW.physical_r2_key || '"}',
        NEW.created_at_ms,
        COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id), 'wnam')
    );
END;

-- The writer releases its own request token after this guarded metadata
-- statement succeeds.  Do not use a tenant/digest-only trigger here: another
-- metadata path may re-reference the same blob while the native writer is
-- waiting on R2, and must not be able to release the native writer's lease.
-- If the owner crashes before releasing, the bounded lease is reclaimed by a
-- later writer; the exact request token still fences late completion.

-- The final D1 operation is one conditional DELETE.  This trigger makes the
-- metadata deletion, audit outbox insertion, candidate transition, and intent
-- completion one SQLite transaction.  If audit validation fails, SQLite
-- aborts the DELETE, preserving the intent for idempotent recovery.
CREATE TRIGGER IF NOT EXISTS trg_gc_purge_finalize_atomic
AFTER DELETE ON blob_meta
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM gc_purge_intent
    WHERE tenant_id = OLD.tenant_id
      AND digest = OLD.digest
      AND state = 'r2_deleted'
)
BEGIN
    INSERT OR IGNORE INTO audit_outbox
        (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, region)
    SELECT
        'gc-purge:' || i.tenant_id || ':' || i.digest || ':' || i.epoch,
        i.tenant_id,
        i.digest,
        'gc-purge:' || i.tenant_id || ':' || i.digest || ':' || i.epoch,
        'corelink.gc.physical_deleted',
        '{"event_type":"corelink.gc.physical_deleted","tenant_id":"' || i.tenant_id || '","digest":"' || i.digest || '","gc_region":"' || i.gc_region || '","residency_region":"' || i.residency_region || '","run_id":"' || i.mark_run_id || '","reason":"physical_deleted","now_ms":' || i.updated_at || '}',
        i.updated_at,
        i.residency_region
    FROM gc_purge_intent AS i
    WHERE i.tenant_id = OLD.tenant_id
      AND i.digest = OLD.digest
      AND i.state = 'r2_deleted';

    UPDATE gc_candidates
    SET status = 'physically_deleted',
        physically_deleted_at_ms = (
            SELECT updated_at FROM gc_purge_intent
            WHERE tenant_id = OLD.tenant_id AND digest = OLD.digest AND state = 'r2_deleted'
        )
    WHERE tenant_id = OLD.tenant_id
      AND digest = OLD.digest
      AND mark_run_id = (
          SELECT mark_run_id FROM gc_purge_intent
          WHERE tenant_id = OLD.tenant_id AND digest = OLD.digest AND state = 'r2_deleted'
      )
      AND status = 'swept';

    UPDATE gc_purge_intent
    SET state = 'finalized',
        updated_at = (
            SELECT updated_at FROM gc_purge_intent
            WHERE tenant_id = OLD.tenant_id AND digest = OLD.digest AND state = 'r2_deleted'
        ),
        last_error = NULL
    WHERE tenant_id = OLD.tenant_id
      AND digest = OLD.digest
      AND state = 'r2_deleted';
END;
