-- CoreLink D1 — migration 0124: independently witnessed v2 head commits.
-- Range 0118..0123 is intentionally reserved by B-083/B-127/B-125.

CREATE TABLE IF NOT EXISTS audit_chain_witness_receipt (
    tenant_id TEXT NOT NULL,
    region TEXT NOT NULL,
    witness_sequence INTEGER NOT NULL CHECK (witness_sequence >= 0),
    witness_record_hash TEXT NOT NULL CHECK (
        length(witness_record_hash) = 64 AND witness_record_hash NOT GLOB '*[^0-9a-f]*'
    ),
    previous_witness_hash TEXT NOT NULL CHECK (
        length(previous_witness_hash) = 64 AND previous_witness_hash NOT GLOB '*[^0-9a-f]*'
    ),
    head_record_hash TEXT NOT NULL CHECK (
        length(head_record_hash) = 64 AND head_record_hash NOT GLOB '*[^0-9a-f]*'
    ),
    witness_jcs BLOB NOT NULL CHECK (length(witness_jcs) > 0),
    receipt_jcs BLOB NOT NULL CHECK (length(receipt_jcs) > 0),
    receipt_signature_b64 TEXT NOT NULL CHECK (length(receipt_signature_b64) = 88),
    witness_id TEXT NOT NULL CHECK (length(witness_id) > 0),
    witness_key_id INTEGER NOT NULL CHECK (witness_key_id > 0),
    committed_at_ms INTEGER NOT NULL CHECK (committed_at_ms >= 0),
    PRIMARY KEY (tenant_id, region, witness_sequence),
    UNIQUE (tenant_id, region, witness_record_hash)
);

CREATE TRIGGER IF NOT EXISTS audit_chain_witness_receipt_no_replace
BEFORE INSERT ON audit_chain_witness_receipt
WHEN EXISTS (
    SELECT 1 FROM audit_chain_witness_receipt AS old
    WHERE (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND
           old.witness_sequence = NEW.witness_sequence)
       OR (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND
           old.witness_record_hash = NEW.witness_record_hash)
)
BEGIN
    SELECT RAISE(ABORT, 'audit-chain witness receipt replacement is forbidden');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_witness_receipt_no_update
BEFORE UPDATE ON audit_chain_witness_receipt
BEGIN
    SELECT RAISE(ABORT, 'audit-chain witness receipt is append-only');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_witness_receipt_no_delete
BEFORE DELETE ON audit_chain_witness_receipt
BEGIN
    SELECT RAISE(ABORT, 'audit-chain witness receipt is append-only');
END;

-- A v2 head is never allowed to exist partially. Legacy heads keep all six
-- 0109 fields NULL until their explicit, witnessed bootstrap transaction.
CREATE TRIGGER IF NOT EXISTS audit_chain_head_v2_shape_insert
BEFORE INSERT ON audit_chain_head
WHEN (NEW.epoch_id IS NOT NULL OR NEW.head_message_version IS NOT NULL OR
      NEW.epoch_ledger_sequence IS NOT NULL OR NEW.epoch_ledger_hash IS NOT NULL OR
      NEW.head_witness_sequence IS NOT NULL OR NEW.head_witness_hash IS NOT NULL)
 AND NOT (NEW.epoch_id IS NOT NULL AND NEW.head_message_version = 2 AND
          NEW.epoch_ledger_sequence IS NOT NULL AND NEW.epoch_ledger_hash IS NOT NULL AND
          NEW.head_witness_sequence IS NOT NULL AND NEW.head_witness_hash IS NOT NULL AND
          NEW.head_signature IS NOT NULL AND NEW.signing_key_id IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'audit-chain v2 head metadata must be complete');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_head_v2_shape_update
BEFORE UPDATE ON audit_chain_head
WHEN (NEW.epoch_id IS NOT NULL OR NEW.head_message_version IS NOT NULL OR
      NEW.epoch_ledger_sequence IS NOT NULL OR NEW.epoch_ledger_hash IS NOT NULL OR
      NEW.head_witness_sequence IS NOT NULL OR NEW.head_witness_hash IS NOT NULL)
 AND NOT (NEW.epoch_id IS NOT NULL AND NEW.head_message_version = 2 AND
          NEW.epoch_ledger_sequence IS NOT NULL AND NEW.epoch_ledger_hash IS NOT NULL AND
          NEW.head_witness_sequence IS NOT NULL AND NEW.head_witness_hash IS NOT NULL AND
          NEW.head_signature IS NOT NULL AND NEW.signing_key_id IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'audit-chain v2 head metadata must be complete');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_head_v2_forward_only
BEFORE UPDATE ON audit_chain_head
-- CASE is deliberate: a v2 -> all-NULL downgrade makes ordinary boolean
-- predicates NULL, but CASE WHEN NULL selects ELSE and therefore aborts.
-- An advance is exactly one of: (a) a non-empty in-epoch seal with the same
-- ledger root, or (b) one adjacent epoch transition at the unchanged chain
-- boundary with the immediately next ledger entry.
WHEN OLD.head_message_version = 2 AND CASE WHEN (
    NEW.head_message_version = 2 AND
    NEW.head_witness_sequence = OLD.head_witness_sequence + 1 AND
    NEW.head_witness_hash <> OLD.head_witness_hash AND
    (
        (NEW.epoch_id = OLD.epoch_id AND
         NEW.epoch_ledger_sequence = OLD.epoch_ledger_sequence AND
         NEW.epoch_ledger_hash = OLD.epoch_ledger_hash AND
         NEW.next_sequence > OLD.next_sequence AND
         NEW.head_hash <> OLD.head_hash)
        OR
        (NEW.epoch_id = OLD.epoch_id + 1 AND
         NEW.epoch_ledger_sequence = OLD.epoch_ledger_sequence + 1 AND
         NEW.epoch_ledger_hash <> OLD.epoch_ledger_hash AND
         NEW.next_sequence = OLD.next_sequence AND
         NEW.head_hash = OLD.head_hash)
    )
) THEN 0 ELSE 1 END = 1
BEGIN
    SELECT RAISE(ABORT, 'audit-chain v2 head transition is not forward-only');
END;

-- Transaction-local assertion latch. The v2 drain inserts assertion=1 only
-- after proving the guarded head CAS, every sealed row and exact receipt are
-- present, then deletes the latch in the same D1 batch. assertion=0 violates
-- this CHECK and rolls the entire REST batch back.
CREATE TABLE IF NOT EXISTS audit_chain_v2_tx_assert (
    commit_id TEXT PRIMARY KEY,
    assertion INTEGER NOT NULL CHECK (assertion = 1)
);
