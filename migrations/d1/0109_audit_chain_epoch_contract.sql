-- CoreLink D1 — migration 0109: B-054 epoch-contract foundation.
--
-- This migration deliberately creates NO runtime behaviour.  In particular,
-- the deployed drain still writes the v1 signed-head tuple and still seals
-- unkeyed links.  The B-054 implementation PR must make the v2 signed-head,
-- ledger, registry and manifest writes transactional before it enables an
-- epoch-enforced partition.  Until then these tables are inert schema, not a
-- claim that keyed audit links, WORM storage, or an external witness exist.
--
-- Evidence tables are append-only at the application boundary.  The triggers
-- below stop accidental UPDATE/DELETE and `INSERT OR REPLACE` by the D1
-- application role, including when SQLite has recursive_triggers=OFF. They do
-- not turn a privileged D1 administrator into an immutable-storage boundary.
-- A verifier must therefore verify the Ed25519 signature chain and require the
-- separately retained witness/archive object named by the contract. Missing
-- evidence is INDETERMINATE, never a clean empty ledger.
--
-- ADDITIVE ONLY: new tables/indexes/triggers plus nullable head columns.  No
-- existing row is rewritten.  SQLite cannot add a NOT NULL column without a
-- default, so epoch enforcement (and the non-NULL requirements) is owned by
-- the future runtime/verifier after a signed E0 bootstrap.

-- The historical v1 head signature covered only tenant, region, hash, and
-- sequence.  v2 is a different signed message and additionally binds these
-- nullable-until-enforced fields.  Do not treat `epoch_id IS NULL` as E0.
ALTER TABLE audit_chain_head ADD COLUMN epoch_id INTEGER CHECK (epoch_id IS NULL OR epoch_id >= 0);
ALTER TABLE audit_chain_head ADD COLUMN head_message_version INTEGER CHECK (head_message_version IS NULL OR head_message_version IN (1, 2));
ALTER TABLE audit_chain_head ADD COLUMN epoch_ledger_sequence INTEGER CHECK (epoch_ledger_sequence IS NULL OR epoch_ledger_sequence >= 0);
ALTER TABLE audit_chain_head ADD COLUMN epoch_ledger_hash TEXT CHECK (
    epoch_ledger_hash IS NULL OR
    (length(epoch_ledger_hash) = 64 AND epoch_ledger_hash NOT GLOB '*[^0-9a-f]*')
);
-- Index-only copy of the independently witnessed record.  It is not trusted
-- without fetching the external record and proving exact equality with this
-- head's v2 JCS bytes and Ed25519 signature.
ALTER TABLE audit_chain_head ADD COLUMN head_witness_sequence INTEGER CHECK (head_witness_sequence IS NULL OR head_witness_sequence >= 0);
ALTER TABLE audit_chain_head ADD COLUMN head_witness_hash TEXT CHECK (
    head_witness_hash IS NULL OR
    (length(head_witness_hash) = 64 AND head_witness_hash NOT GLOB '*[^0-9a-f]*')
);

-- Public head-signing keys are authenticated by an operator-held registry
-- trust root.  No private key material belongs in D1.
CREATE TABLE IF NOT EXISTS audit_chain_signing_key_registry (
    signing_key_id INTEGER NOT NULL PRIMARY KEY CHECK (signing_key_id > 0),
    algorithm TEXT NOT NULL CHECK (algorithm = 'ed25519-v1'),
    public_key_b64 TEXT NOT NULL CHECK (length(public_key_b64) = 44),
    trust_root_key_id TEXT NOT NULL CHECK (length(trust_root_key_id) > 0),
    registry_version INTEGER NOT NULL CHECK (registry_version = 1),
    registry_jcs BLOB NOT NULL CHECK (length(registry_jcs) > 0),
    registry_signature_b64 TEXT NOT NULL CHECK (length(registry_signature_b64) = 88),
    registered_at_ms INTEGER NOT NULL CHECK (registered_at_ms >= 0),
    UNIQUE (public_key_b64)
);

-- A link-key registry entry contains only an irreversible commitment and a
-- stable key id.  It MUST NOT contain a link key, a seed, or a Cloudflare
-- secret name.  The runtime recomputes the commitment from its write-only
-- keyring before it uses that id.
CREATE TABLE IF NOT EXISTS audit_chain_link_key_registry (
    link_key_id INTEGER NOT NULL PRIMARY KEY CHECK (link_key_id > 0),
    algorithm_id INTEGER NOT NULL CHECK (algorithm_id = 1),
    key_commitment_hex TEXT NOT NULL CHECK (
        length(key_commitment_hex) = 64 AND
        key_commitment_hex NOT GLOB '*[^0-9a-f]*'
    ),
    registry_version INTEGER NOT NULL CHECK (registry_version = 1),
    registry_jcs BLOB NOT NULL CHECK (length(registry_jcs) > 0),
    registry_signature_b64 TEXT NOT NULL CHECK (length(registry_signature_b64) = 88),
    signing_key_id INTEGER NOT NULL REFERENCES audit_chain_signing_key_registry(signing_key_id),
    registered_at_ms INTEGER NOT NULL CHECK (registered_at_ms >= 0),
    UNIQUE (algorithm_id, key_commitment_hex)
);

-- The ledger is the append-only, signed authority.  `audit_chain_epoch` is a
-- query projection of these entries; no verifier may trust the projection
-- unless it matches this ledger and the signed v2 head.
CREATE TABLE IF NOT EXISTS audit_chain_epoch_ledger (
    tenant_id TEXT NOT NULL,
    region TEXT NOT NULL,
    ledger_sequence INTEGER NOT NULL CHECK (ledger_sequence >= 0),
    ledger_version INTEGER NOT NULL CHECK (ledger_version = 1),
    entry_type TEXT NOT NULL CHECK (entry_type IN ('epoch-genesis', 'epoch-transition')),
    epoch_id INTEGER NOT NULL CHECK (epoch_id >= 0),
    previous_ledger_hash TEXT NOT NULL CHECK (
        length(previous_ledger_hash) = 64 AND
        previous_ledger_hash NOT GLOB '*[^0-9a-f]*'
    ),
    ledger_hash TEXT NOT NULL CHECK (
        length(ledger_hash) = 64 AND
        ledger_hash NOT GLOB '*[^0-9a-f]*'
    ),
    entry_jcs BLOB NOT NULL CHECK (length(entry_jcs) > 0),
    signature_b64 TEXT NOT NULL CHECK (length(signature_b64) = 88),
    signing_key_id INTEGER NOT NULL REFERENCES audit_chain_signing_key_registry(signing_key_id),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    PRIMARY KEY (tenant_id, region, ledger_sequence),
    UNIQUE (tenant_id, region, epoch_id),
    UNIQUE (tenant_id, region, ledger_hash),
    CHECK (
        (ledger_sequence = 0 AND entry_type = 'epoch-genesis' AND epoch_id = 0 AND
         previous_ledger_hash = '0000000000000000000000000000000000000000000000000000000000000000') OR
        (ledger_sequence > 0 AND entry_type = 'epoch-transition' AND epoch_id > 0)
    )
);

CREATE TABLE IF NOT EXISTS audit_chain_epoch (
    tenant_id TEXT NOT NULL,
    region TEXT NOT NULL,
    epoch_id INTEGER NOT NULL CHECK (epoch_id >= 0),
    algorithm_id INTEGER NOT NULL CHECK (algorithm_id IN (0, 1)),
    link_key_id INTEGER REFERENCES audit_chain_link_key_registry(link_key_id),
    state TEXT NOT NULL CHECK (state IN ('active', 'closed')),
    start_sequence INTEGER NOT NULL CHECK (start_sequence >= 0),
    start_prev_hash TEXT NOT NULL CHECK (
        length(start_prev_hash) = 64 AND start_prev_hash NOT GLOB '*[^0-9a-f]*'
    ),
    predecessor_epoch_id INTEGER,
    open_ledger_sequence INTEGER NOT NULL CHECK (open_ledger_sequence >= 0),
    open_ledger_hash TEXT NOT NULL CHECK (
        length(open_ledger_hash) = 64 AND open_ledger_hash NOT GLOB '*[^0-9a-f]*'
    ),
    closed_at_ms INTEGER,
    end_sequence_exclusive INTEGER,
    end_head_hash TEXT CHECK (
        end_head_hash IS NULL OR
        (length(end_head_hash) = 64 AND end_head_hash NOT GLOB '*[^0-9a-f]*')
    ),
    PRIMARY KEY (tenant_id, region, epoch_id),
    UNIQUE (tenant_id, region, start_sequence),
    FOREIGN KEY (tenant_id, region, predecessor_epoch_id)
        REFERENCES audit_chain_epoch(tenant_id, region, epoch_id),
    FOREIGN KEY (tenant_id, region, open_ledger_sequence)
        REFERENCES audit_chain_epoch_ledger(tenant_id, region, ledger_sequence),
    CHECK (
        (algorithm_id = 0 AND link_key_id IS NULL) OR
        (algorithm_id = 1 AND link_key_id IS NOT NULL)
    ),
    CHECK (
        (epoch_id = 0 AND algorithm_id = 0 AND link_key_id IS NULL AND
         predecessor_epoch_id IS NULL AND start_sequence = 0 AND
         start_prev_hash = '0000000000000000000000000000000000000000000000000000000000000000') OR
        (epoch_id > 0 AND algorithm_id = 1 AND link_key_id IS NOT NULL AND
         predecessor_epoch_id IS NOT NULL AND predecessor_epoch_id = epoch_id - 1)
    ),
    CHECK (
        (state = 'active' AND closed_at_ms IS NULL AND end_sequence_exclusive IS NULL AND end_head_hash IS NULL) OR
        (state = 'closed' AND closed_at_ms IS NOT NULL AND end_sequence_exclusive IS NOT NULL AND
         end_sequence_exclusive >= start_sequence AND end_head_hash IS NOT NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS audit_chain_epoch_one_active
    ON audit_chain_epoch(tenant_id, region) WHERE state = 'active';

-- One signed manifest covers one contiguous range inside one epoch.  Empty
-- coverage is represented explicitly so a verifier never treats an omitted
-- object listing as a clean empty range.
CREATE TABLE IF NOT EXISTS audit_chain_archive_manifest (
    tenant_id TEXT NOT NULL,
    region TEXT NOT NULL,
    epoch_id INTEGER NOT NULL,
    start_sequence INTEGER NOT NULL CHECK (start_sequence >= 0),
    end_sequence_exclusive INTEGER NOT NULL CHECK (end_sequence_exclusive >= start_sequence),
    record_count INTEGER NOT NULL CHECK (record_count >= 0),
    is_empty INTEGER NOT NULL CHECK (is_empty IN (0, 1)),
    algorithm_id INTEGER NOT NULL CHECK (algorithm_id IN (0, 1)),
    link_key_id INTEGER REFERENCES audit_chain_link_key_registry(link_key_id),
    start_prev_hash TEXT NOT NULL CHECK (
        length(start_prev_hash) = 64 AND start_prev_hash NOT GLOB '*[^0-9a-f]*'
    ),
    end_head_hash TEXT NOT NULL CHECK (
        length(end_head_hash) = 64 AND end_head_hash NOT GLOB '*[^0-9a-f]*'
    ),
    end_head_witness_sequence INTEGER NOT NULL CHECK (end_head_witness_sequence >= 0),
    end_head_witness_hash TEXT NOT NULL CHECK (
        length(end_head_witness_hash) = 64 AND end_head_witness_hash NOT GLOB '*[^0-9a-f]*'
    ),
    epoch_ledger_sequence INTEGER NOT NULL CHECK (epoch_ledger_sequence >= 0),
    epoch_ledger_hash TEXT NOT NULL CHECK (
        length(epoch_ledger_hash) = 64 AND epoch_ledger_hash NOT GLOB '*[^0-9a-f]*'
    ),
    manifest_version INTEGER NOT NULL CHECK (manifest_version = 1),
    manifest_hash TEXT NOT NULL CHECK (
        length(manifest_hash) = 64 AND manifest_hash NOT GLOB '*[^0-9a-f]*'
    ),
    manifest_jcs BLOB NOT NULL CHECK (length(manifest_jcs) > 0),
    signature_b64 TEXT NOT NULL CHECK (length(signature_b64) = 88),
    signing_key_id INTEGER NOT NULL REFERENCES audit_chain_signing_key_registry(signing_key_id),
    published_at_ms INTEGER NOT NULL CHECK (published_at_ms >= 0),
    PRIMARY KEY (tenant_id, region, epoch_id, start_sequence),
    UNIQUE (tenant_id, region, manifest_hash),
    FOREIGN KEY (tenant_id, region, epoch_id)
        REFERENCES audit_chain_epoch(tenant_id, region, epoch_id),
    FOREIGN KEY (tenant_id, region, epoch_ledger_sequence)
        REFERENCES audit_chain_epoch_ledger(tenant_id, region, ledger_sequence),
    CHECK (
        (algorithm_id = 0 AND link_key_id IS NULL) OR
        (algorithm_id = 1 AND link_key_id IS NOT NULL)
    ),
    CHECK (
        (is_empty = 0 AND end_sequence_exclusive > start_sequence AND
         record_count = end_sequence_exclusive - start_sequence) OR
        (is_empty = 1 AND end_sequence_exclusive = start_sequence AND record_count = 0)
    )
);

-- Application-role guardrails.  `INSERT OR REPLACE` normally deletes the
-- conflicting row before inserting a replacement; SQLite does not run DELETE
-- triggers for that implicit deletion with recursive_triggers=OFF.  A BEFORE
-- INSERT conflict guard therefore aborts first, making replacement impossible
-- independently of that SQLite setting. A privileged D1 writer can remove these,
-- so integrity still rests on the signed ledger plus its external witness.
CREATE TRIGGER IF NOT EXISTS audit_chain_epoch_ledger_no_replace
BEFORE INSERT ON audit_chain_epoch_ledger
WHEN EXISTS (
    SELECT 1 FROM audit_chain_epoch_ledger AS old
    WHERE (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND old.ledger_sequence = NEW.ledger_sequence)
       OR (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND old.epoch_id = NEW.epoch_id)
       OR (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND old.ledger_hash = NEW.ledger_hash)
)
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_epoch_ledger replacement is forbidden');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_epoch_ledger_no_update
BEFORE UPDATE ON audit_chain_epoch_ledger
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_epoch_ledger is append-only');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_epoch_ledger_no_delete
BEFORE DELETE ON audit_chain_epoch_ledger
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_epoch_ledger is append-only');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_signing_key_registry_no_replace
BEFORE INSERT ON audit_chain_signing_key_registry
WHEN EXISTS (
    SELECT 1 FROM audit_chain_signing_key_registry AS old
    WHERE old.signing_key_id = NEW.signing_key_id
       OR old.public_key_b64 = NEW.public_key_b64
)
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_signing_key_registry replacement is forbidden');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_signing_key_registry_no_update
BEFORE UPDATE ON audit_chain_signing_key_registry
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_signing_key_registry is append-only');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_signing_key_registry_no_delete
BEFORE DELETE ON audit_chain_signing_key_registry
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_signing_key_registry is append-only');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_link_key_registry_no_replace
BEFORE INSERT ON audit_chain_link_key_registry
WHEN EXISTS (
    SELECT 1 FROM audit_chain_link_key_registry AS old
    WHERE old.link_key_id = NEW.link_key_id
       OR (old.algorithm_id = NEW.algorithm_id AND old.key_commitment_hex = NEW.key_commitment_hex)
)
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_link_key_registry replacement is forbidden');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_link_key_registry_no_update
BEFORE UPDATE ON audit_chain_link_key_registry
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_link_key_registry is append-only');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_link_key_registry_no_delete
BEFORE DELETE ON audit_chain_link_key_registry
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_link_key_registry is append-only');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_epoch_no_replace
BEFORE INSERT ON audit_chain_epoch
WHEN EXISTS (
    SELECT 1 FROM audit_chain_epoch AS old
    WHERE (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND old.epoch_id = NEW.epoch_id)
       OR (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND old.start_sequence = NEW.start_sequence)
       OR (NEW.state = 'active' AND old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND old.state = 'active')
)
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_epoch replacement is forbidden');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_epoch_no_delete
BEFORE DELETE ON audit_chain_epoch
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_epoch deletion is forbidden');
END;
-- A projection may close exactly once so a successor can become active.  Every
-- identity, algorithm, boundary, predecessor, and opening-ledger field is
-- immutable; any other UPDATE (including UPDATE OR REPLACE) is rejected before
-- SQLite's conflict handler can replace another projection row.
CREATE TRIGGER IF NOT EXISTS audit_chain_epoch_only_close
BEFORE UPDATE ON audit_chain_epoch
WHEN NOT (
    OLD.tenant_id IS NEW.tenant_id AND
    OLD.region IS NEW.region AND
    OLD.epoch_id IS NEW.epoch_id AND
    OLD.algorithm_id IS NEW.algorithm_id AND
    OLD.link_key_id IS NEW.link_key_id AND
    OLD.start_sequence IS NEW.start_sequence AND
    OLD.start_prev_hash IS NEW.start_prev_hash AND
    OLD.predecessor_epoch_id IS NEW.predecessor_epoch_id AND
    OLD.open_ledger_sequence IS NEW.open_ledger_sequence AND
    OLD.open_ledger_hash IS NEW.open_ledger_hash AND
    OLD.state = 'active' AND NEW.state = 'closed' AND
    OLD.closed_at_ms IS NULL AND OLD.end_sequence_exclusive IS NULL AND OLD.end_head_hash IS NULL AND
    NEW.closed_at_ms IS NOT NULL AND NEW.end_sequence_exclusive IS NOT NULL AND NEW.end_head_hash IS NOT NULL
)
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_epoch permits only active-to-closed transition');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_archive_manifest_no_replace
BEFORE INSERT ON audit_chain_archive_manifest
WHEN EXISTS (
    SELECT 1 FROM audit_chain_archive_manifest AS old
    WHERE (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND
           old.epoch_id = NEW.epoch_id AND old.start_sequence = NEW.start_sequence)
       OR (old.tenant_id = NEW.tenant_id AND old.region = NEW.region AND old.manifest_hash = NEW.manifest_hash)
)
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_archive_manifest replacement is forbidden');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_archive_manifest_no_update
BEFORE UPDATE ON audit_chain_archive_manifest
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_archive_manifest is append-only');
END;
CREATE TRIGGER IF NOT EXISTS audit_chain_archive_manifest_no_delete
BEFORE DELETE ON audit_chain_archive_manifest
BEGIN
    SELECT RAISE(ABORT, 'audit_chain_archive_manifest is append-only');
END;
