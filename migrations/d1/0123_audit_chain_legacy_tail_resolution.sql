-- CoreLink D1 — migration 0123: signed, append-only legacy-tail resolution.
--
-- B-125 found two legacy partitions whose maximum sequence has two branches.
-- Existing audit rows are evidence: they MUST NOT be deleted, rewritten, or
-- re-sequenced. The normal drain therefore remains fail-closed on an ambiguous
-- maximum. This table permits only an explicit exceptional continuation when
-- an operator-held Ed25519 key signs the complete candidate-set commitment and
-- selects the one tail already authenticated by the live v1 signed checkpoint.
--
-- It does not create an epoch and cannot authorize a head-ahead/head-behind
-- fork. Those require the independently witnessed B-054 v2 runtime. Applying
-- this migration alone is inert: the table starts empty and no production data
-- is changed.
--
-- Prefix 0123 was coordinated after B083 (0118-0121) and B127 (0122).
-- ADDITIVE ONLY: one table, one index, and append-only triggers.

CREATE TABLE IF NOT EXISTS audit_chain_legacy_tail_resolution (
    tenant_id TEXT NOT NULL,
    region TEXT NOT NULL CHECK (region IN ('wnam','enam','weur','sam','apac','afr')),
    resolution_version INTEGER NOT NULL CHECK (resolution_version = 1),
    -- Only duplicated genesis can currently be proven to its zero anchor.
    -- Later sequences require an independently witnessed prefix/epoch proof.
    tail_sequence INTEGER NOT NULL CHECK (tail_sequence = 0),
    selected_row_id TEXT NOT NULL,
    selected_chain_hash TEXT NOT NULL CHECK (
        length(selected_chain_hash) = 64 AND
        selected_chain_hash NOT GLOB '*[^0-9a-f]*'
    ),
    checkpoint_head_hash TEXT NOT NULL CHECK (
        length(checkpoint_head_hash) = 64 AND
        checkpoint_head_hash NOT GLOB '*[^0-9a-f]*'
    ),
    checkpoint_next_sequence INTEGER NOT NULL CHECK (checkpoint_next_sequence > 0),
    checkpoint_head_signature TEXT NOT NULL CHECK (length(checkpoint_head_signature) = 88),
    signing_key_id INTEGER NOT NULL CHECK (signing_key_id > 0),
    candidate_count INTEGER NOT NULL CHECK (candidate_count >= 2 AND candidate_count <= 32),
    candidate_set_hash TEXT NOT NULL CHECK (
        length(candidate_set_hash) = 64 AND
        candidate_set_hash NOT GLOB '*[^0-9a-f]*'
    ),
    resolution_signature TEXT NOT NULL CHECK (length(resolution_signature) = 88),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    PRIMARY KEY (tenant_id, region),
    UNIQUE (selected_row_id),
    CHECK (checkpoint_next_sequence = tail_sequence + 1),
    CHECK (selected_chain_hash = checkpoint_head_hash)
);

CREATE INDEX IF NOT EXISTS idx_audit_chain_legacy_tail_resolution_created
    ON audit_chain_legacy_tail_resolution(created_at_ms);

CREATE TRIGGER IF NOT EXISTS audit_chain_legacy_tail_resolution_no_replace
BEFORE INSERT ON audit_chain_legacy_tail_resolution
WHEN EXISTS (
    SELECT 1 FROM audit_chain_legacy_tail_resolution AS old
    WHERE (old.tenant_id = NEW.tenant_id AND old.region = NEW.region)
       OR old.selected_row_id = NEW.selected_row_id
)
BEGIN
    SELECT RAISE(ABORT, 'legacy tail resolution replacement is forbidden');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_legacy_tail_resolution_no_update
BEFORE UPDATE ON audit_chain_legacy_tail_resolution
BEGIN
    SELECT RAISE(ABORT, 'legacy tail resolution is append-only');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_legacy_tail_resolution_no_delete
BEFORE DELETE ON audit_chain_legacy_tail_resolution
BEGIN
    SELECT RAISE(ABORT, 'legacy tail resolution is append-only');
END;

-- Freeze exactly the committed candidate sequence after a resolution exists.
-- Later rows in the partition remain writable and archivable. This closes the
-- honest-writer TOCTOU between resolution verification and checkpoint CAS.
CREATE TRIGGER IF NOT EXISTS audit_chain_legacy_tail_resolution_candidate_no_insert
BEFORE INSERT ON audit_outbox
WHEN EXISTS (
    SELECT 1 FROM audit_chain_legacy_tail_resolution AS resolution
    WHERE resolution.tenant_id = NEW.tenant_id
      AND resolution.region = NEW.region
      AND resolution.tail_sequence = NEW.sequence_number
)
BEGIN
    SELECT RAISE(ABORT, 'resolved legacy tail candidate set is frozen');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_legacy_tail_resolution_candidate_no_update
BEFORE UPDATE ON audit_outbox
WHEN EXISTS (
    SELECT 1 FROM audit_chain_legacy_tail_resolution AS resolution
    WHERE (resolution.tenant_id = OLD.tenant_id
       AND resolution.region = OLD.region
       AND resolution.tail_sequence = OLD.sequence_number)
       OR (resolution.tenant_id = NEW.tenant_id
       AND resolution.region = NEW.region
       AND resolution.tail_sequence = NEW.sequence_number)
)
BEGIN
    SELECT RAISE(ABORT, 'resolved legacy tail candidate set is frozen');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_legacy_tail_resolution_candidate_no_delete
BEFORE DELETE ON audit_outbox
WHEN EXISTS (
    SELECT 1 FROM audit_chain_legacy_tail_resolution AS resolution
    WHERE resolution.tenant_id = OLD.tenant_id
      AND resolution.region = OLD.region
      AND resolution.tail_sequence = OLD.sequence_number
)
BEGIN
    SELECT RAISE(ABORT, 'resolved legacy tail candidate set is frozen');
END;
