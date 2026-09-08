-- CoreLink D1 — migration 0110: versioned audit-chain row metadata (B-054).
--
-- Existing sealed rows are deliberately untouched.  NULL means the historical
-- pre-B-054 row shape and is interpreted only by the compatibility reader as
-- algorithm 0 / epoch 0; a new writer MUST populate all three columns in the
-- same guarded seal update.  A keyed row can therefore never be mistaken for
-- an unkeyed row merely because a verifier has a key available.
--
-- The link key is an identity only.  Secret material never enters D1; the
-- runtime resolves `link_key_id` through a write-only deployment keyring and
-- checks its public commitment before hashing.
--
-- ADDITIVE ONLY: nullable columns + an index, with no rewrite of the ~67k
-- historical sealed rows.  Epoch activation and non-NULL enforcement belong to
-- the signed epoch-ledger transition in the runtime.

ALTER TABLE audit_outbox ADD COLUMN algorithm_id INTEGER CHECK (
    algorithm_id IS NULL OR algorithm_id IN (0, 1)
);
ALTER TABLE audit_outbox ADD COLUMN epoch_id INTEGER CHECK (
    epoch_id IS NULL OR epoch_id >= 0
);
ALTER TABLE audit_outbox ADD COLUMN link_key_id INTEGER CHECK (
    link_key_id IS NULL OR link_key_id > 0
);

-- Nullable columns preserve pre-B-054 rows, but NULL is not a wildcard for a
-- new or rewritten row. Accept exactly: all-NULL legacy, explicit E0 (0,0,NULL),
-- or a complete keyed tuple (1, epoch>0, key_id>0). This trigger closes every
-- partial/downgrade shape even on SQLite deployments where ALTER TABLE cannot
-- add a table-level CHECK to the existing table.
CREATE TRIGGER IF NOT EXISTS audit_outbox_epoch_metadata_shape_insert
BEFORE INSERT ON audit_outbox
WHEN COALESCE((
    (NEW.algorithm_id IS NULL AND NEW.epoch_id IS NULL AND NEW.link_key_id IS NULL) OR
    (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
     NEW.algorithm_id = 0 AND NEW.epoch_id = 0 AND NEW.link_key_id IS NULL) OR
    (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
     typeof(NEW.link_key_id) = 'integer' AND NEW.algorithm_id = 1 AND
     NEW.epoch_id > 0 AND NEW.link_key_id > 0)
), 0) = 0
BEGIN
    SELECT RAISE(ABORT, 'invalid audit_outbox epoch metadata shape');
END;

CREATE TRIGGER IF NOT EXISTS audit_outbox_epoch_metadata_shape_update
BEFORE UPDATE OF algorithm_id, epoch_id, link_key_id ON audit_outbox
WHEN COALESCE((
    (NEW.algorithm_id IS NULL AND NEW.epoch_id IS NULL AND NEW.link_key_id IS NULL) OR
    (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
     NEW.algorithm_id = 0 AND NEW.epoch_id = 0 AND NEW.link_key_id IS NULL) OR
    (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
     typeof(NEW.link_key_id) = 'integer' AND NEW.algorithm_id = 1 AND
     NEW.epoch_id > 0 AND NEW.link_key_id > 0)
), 0) = 0
BEGIN
    SELECT RAISE(ABORT, 'invalid audit_outbox epoch metadata shape');
END;

-- Metadata transitions are forward-only. Historical all-NULL rows may be
-- promoted to explicit E0 or a keyed epoch; E0 may remain E0 or advance to a
-- keyed epoch. Once keyed, a row may not be relabelled as legacy/E0, and a
-- keyed epoch may not move backwards or change its key within the same epoch.
-- Keep this separate from the shape trigger so an already-deployed copy of
-- the older shape trigger remains compatible while this guard is added.
CREATE TRIGGER IF NOT EXISTS audit_outbox_epoch_metadata_forward_update
BEFORE UPDATE OF algorithm_id, epoch_id, link_key_id ON audit_outbox
WHEN COALESCE((
    (OLD.algorithm_id IS NULL AND OLD.epoch_id IS NULL AND OLD.link_key_id IS NULL AND (
        (NEW.algorithm_id IS NULL AND NEW.epoch_id IS NULL AND NEW.link_key_id IS NULL) OR
        (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
         NEW.algorithm_id = 0 AND NEW.epoch_id = 0 AND NEW.link_key_id IS NULL) OR
        (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
         typeof(NEW.link_key_id) = 'integer' AND NEW.algorithm_id = 1 AND
         NEW.epoch_id > 0 AND NEW.link_key_id > 0)
    )) OR
    (typeof(OLD.algorithm_id) = 'integer' AND typeof(OLD.epoch_id) = 'integer' AND
     OLD.algorithm_id = 0 AND OLD.epoch_id = 0 AND OLD.link_key_id IS NULL AND (
        (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
         NEW.algorithm_id = 0 AND NEW.epoch_id = 0 AND NEW.link_key_id IS NULL) OR
        (typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
         typeof(NEW.link_key_id) = 'integer' AND NEW.algorithm_id = 1 AND
         NEW.epoch_id > 0 AND NEW.link_key_id > 0)
    )) OR
    (typeof(OLD.algorithm_id) = 'integer' AND typeof(OLD.epoch_id) = 'integer' AND
     typeof(OLD.link_key_id) = 'integer' AND OLD.algorithm_id = 1 AND
     OLD.epoch_id > 0 AND OLD.link_key_id > 0 AND
     typeof(NEW.algorithm_id) = 'integer' AND typeof(NEW.epoch_id) = 'integer' AND
     typeof(NEW.link_key_id) = 'integer' AND NEW.algorithm_id = 1 AND
     NEW.epoch_id > 0 AND NEW.link_key_id > 0 AND
     (NEW.epoch_id > OLD.epoch_id OR
      (NEW.epoch_id = OLD.epoch_id AND NEW.link_key_id = OLD.link_key_id)))
), 0) = 0
BEGIN
    SELECT RAISE(ABORT, 'audit_outbox epoch metadata transition is not forward-only');
END;

-- Versioned sealed-tail scans retain the existing partition/sequence seek.
CREATE INDEX IF NOT EXISTS idx_audit_outbox_epoch_tail
    ON audit_outbox(tenant_id, region, epoch_id, sequence_number)
    WHERE emitted_at IS NOT NULL AND sequence_number IS NOT NULL;
