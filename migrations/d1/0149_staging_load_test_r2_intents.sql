-- #2623: durable handoff for staging ownership around non-transactional R2 writes.
-- A writer first inserts `prepared`, performs the R2 operation, then commits the
-- intent and its migration-0147 resource registration in one D1 batch.
CREATE TABLE IF NOT EXISTS staging_load_test_r2_intents (
    operation_id TEXT PRIMARY KEY NOT NULL CHECK (
        length(operation_id) = 64 AND operation_id NOT GLOB '*[^0-9a-f]*'
    ),
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    target_deployment_sha TEXT NOT NULL CHECK (
        length(target_deployment_sha) = 40
        AND target_deployment_sha NOT GLOB '*[^0-9a-f]*'
    ),
    resource_class TEXT NOT NULL CHECK (resource_class IN (
        'cas_reference', 'dsr_artifact', 'audit_evidence', 'byok_artifact'
    )),
    receipt_ref TEXT NOT NULL CHECK (
        length(receipt_ref) = 64 AND receipt_ref NOT GLOB '*[^0-9a-f]*'
    ),
    opaque_handle TEXT NOT NULL CHECK (length(opaque_handle) BETWEEN 1 AND 512),
    disposition TEXT NOT NULL CHECK (disposition IN ('disposable', 'retained')),
    state TEXT NOT NULL CHECK (state IN ('prepared', 'committed', 'quarantined')),
    prepared_at_ms INTEGER NOT NULL CHECK (prepared_at_ms >= 0),
    committed_at_ms INTEGER CHECK (committed_at_ms >= prepared_at_ms),
    UNIQUE (run_id, scenario, resource_class, receipt_ref),
    UNIQUE (resource_class, opaque_handle),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario),
    CHECK (
        (state = 'prepared' AND committed_at_ms IS NULL)
        OR (state = 'committed' AND committed_at_ms IS NOT NULL)
        OR (state = 'quarantined' AND committed_at_ms IS NULL)
    ),
    CHECK (
        resource_class NOT IN ('cas_reference', 'audit_evidence')
        OR disposition = 'retained'
    )
);

CREATE INDEX IF NOT EXISTS idx_staging_load_test_r2_intents_reconcile
    ON staging_load_test_r2_intents (state, prepared_at_ms, operation_id);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_r2_intent_requires_open_run
BEFORE INSERT ON staging_load_test_r2_intents
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs
    WHERE run_id = NEW.run_id
      AND scenario = NEW.scenario
      AND target_environment = 'staging'
      AND target_deployment_sha = NEW.target_deployment_sha
      AND state = 'open'
)
BEGIN
    SELECT RAISE(ABORT, 'R2 ownership intent requires an exact open staging run');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_r2_intent_identity_immutable
BEFORE UPDATE OF operation_id, run_id, scenario, target_deployment_sha,
    resource_class, receipt_ref, opaque_handle, disposition, prepared_at_ms
ON staging_load_test_r2_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'R2 ownership intent identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_r2_intent_forward_only
BEFORE UPDATE OF state, committed_at_ms ON staging_load_test_r2_intents
FOR EACH ROW
WHEN NOT (
    (OLD.state = NEW.state AND OLD.committed_at_ms IS NEW.committed_at_ms)
    OR (OLD.state = 'prepared' AND NEW.state = 'committed'
        AND NEW.committed_at_ms IS NOT NULL)
    OR (OLD.state = 'prepared' AND NEW.state = 'quarantined'
        AND NEW.committed_at_ms IS NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'R2 ownership intent transition is invalid');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_r2_intent_commit_requires_resource
BEFORE UPDATE OF state ON staging_load_test_r2_intents
FOR EACH ROW
WHEN OLD.state = 'prepared' AND NEW.state = 'committed' AND NOT EXISTS (
    SELECT 1 FROM staging_load_test_resources
    WHERE run_id = OLD.run_id
      AND scenario = OLD.scenario
      AND resource_class = OLD.resource_class
      AND receipt_ref = OLD.receipt_ref
      AND opaque_handle = OLD.opaque_handle
      AND disposition = OLD.disposition
      AND state = 'registered'
)
BEGIN
    SELECT RAISE(ABORT, 'R2 ownership intent cannot commit without registration');
END;

-- A prepared or quarantined operation may already have created an R2 object.
-- Do not freeze the run inventory around that unresolved external state. The
-- operator may move the run to `failed`, but sealing requires every intent to
-- have reached `committed`, whose trigger above proves the resource row exists.
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_run_seal_requires_resolved_r2
BEFORE UPDATE OF state ON staging_load_test_runs
FOR EACH ROW
WHEN OLD.state = 'open' AND NEW.state = 'sealed' AND EXISTS (
    SELECT 1 FROM staging_load_test_r2_intents
    WHERE run_id = OLD.run_id
      AND scenario = OLD.scenario
      AND state <> 'committed'
)
BEGIN
    SELECT RAISE(ABORT, 'load-test run cannot seal with unresolved R2 ownership intent');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_r2_intent_no_delete
BEFORE DELETE ON staging_load_test_r2_intents
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'R2 ownership intent ledger is append-only');
END;
