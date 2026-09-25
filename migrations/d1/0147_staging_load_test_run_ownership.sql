-- #2161 prerequisite: the server-owned staging load-test attribution ledger.
--
-- This migration deliberately mounts no teardown route.  A future handler may
-- delete only `disposable` resources registered under one exact open run and
-- scenario, after every resource-class scan is complete.  `retained` records
-- (including DSR obligations and audit/billing evidence) are receipt inputs,
-- never deletion candidates.  `cas_reference` names a run-owned reference;
-- its adapter must preserve a physically shared CAS object until its reference
-- count is zero.

CREATE TABLE IF NOT EXISTS staging_load_test_runs (
    run_id TEXT NOT NULL CHECK (
        length(run_id) BETWEEN 1 AND 20
        AND run_id NOT GLOB '*[^0-9]*'
        AND substr(run_id, 1, 1) <> '0'
    ),
    scenario TEXT NOT NULL CHECK (scenario IN (
        'signup', 'webhook', 'dsr', 'cas', 'byok', 'endurance-2h', 'b103-cargo-write'
    )),
    target_environment TEXT NOT NULL CHECK (target_environment = 'staging'),
    target_deployment_sha TEXT NOT NULL CHECK (
        length(target_deployment_sha) = 40
        AND target_deployment_sha NOT GLOB '*[^0-9a-f]*'
    ),
    state TEXT NOT NULL CHECK (state IN ('open', 'sealed', 'teardown_started', 'reconciled', 'failed')),
    admitted_at_ms INTEGER NOT NULL CHECK (admitted_at_ms >= 0),
    PRIMARY KEY (run_id, scenario)
);

CREATE TABLE IF NOT EXISTS staging_load_test_resource_scans (
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    resource_class TEXT NOT NULL CHECK (resource_class IN (
        'cas_reference', 'webhook_inbox', 'webhook_effect', 'dsr_artifact',
        'dsr_obligation', 'audit_evidence', 'billing_audit', 'signup_artifact', 'byok_artifact'
    )),
    state TEXT NOT NULL CHECK (state IN ('complete', 'incomplete')),
    observed_count INTEGER NOT NULL CHECK (observed_count >= 0),
    scanned_at_ms INTEGER NOT NULL CHECK (scanned_at_ms >= 0),
    PRIMARY KEY (run_id, scenario, resource_class),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario)
);

CREATE TABLE IF NOT EXISTS staging_load_test_resources (
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    resource_class TEXT NOT NULL CHECK (resource_class IN (
        'cas_reference', 'webhook_inbox', 'webhook_effect', 'dsr_artifact',
        'dsr_obligation', 'audit_evidence', 'billing_audit', 'signup_artifact', 'byok_artifact'
    )),
    receipt_ref TEXT NOT NULL CHECK (
        length(receipt_ref) = 64 AND receipt_ref NOT GLOB '*[^0-9a-f]*'
    ),
    opaque_handle TEXT NOT NULL CHECK (length(opaque_handle) BETWEEN 1 AND 512),
    disposition TEXT NOT NULL CHECK (disposition IN ('disposable', 'retained')),
    state TEXT NOT NULL CHECK (state IN ('registered', 'delete_started', 'deleted', 'preserved', 'quarantined')),
    registered_at_ms INTEGER NOT NULL CHECK (registered_at_ms >= 0),
    PRIMARY KEY (run_id, scenario, resource_class, receipt_ref),
    UNIQUE (resource_class, opaque_handle),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario),
    CHECK (
        (disposition = 'retained' AND state IN ('registered', 'preserved')) OR
        (resource_class NOT IN ('dsr_obligation', 'audit_evidence', 'billing_audit')
            AND disposition = 'disposable' AND state IN ('registered', 'delete_started', 'deleted', 'quarantined'))
    )
);

CREATE INDEX IF NOT EXISTS idx_staging_load_test_resources_run_state
    ON staging_load_test_resources (run_id, scenario, disposition, state);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_run_state_forward_only
BEFORE UPDATE OF state ON staging_load_test_runs
FOR EACH ROW
WHEN NOT (
    OLD.state = NEW.state
    OR (OLD.state = 'open' AND NEW.state IN ('sealed', 'failed'))
    OR (OLD.state = 'sealed' AND NEW.state IN ('teardown_started', 'failed'))
    OR (OLD.state = 'teardown_started' AND NEW.state IN ('reconciled', 'failed'))
)
BEGIN
    SELECT RAISE(ABORT, 'load-test run state cannot move backward');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_run_identity_immutable
BEFORE UPDATE OF run_id, scenario, target_environment, target_deployment_sha, admitted_at_ms
ON staging_load_test_runs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'load-test run identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_run_requires_reconciled_scans
BEFORE UPDATE OF state ON staging_load_test_runs
FOR EACH ROW
WHEN OLD.state = 'open' AND NEW.state = 'sealed' AND (
    (SELECT count(*) FROM staging_load_test_resource_scans
     WHERE run_id = NEW.run_id AND scenario = NEW.scenario) <> 9
    OR EXISTS (
        SELECT 1 FROM staging_load_test_resource_scans AS scan
        WHERE scan.run_id = NEW.run_id AND scan.scenario = NEW.scenario
          AND (scan.state <> 'complete' OR scan.observed_count <> (
              SELECT count(*) FROM staging_load_test_resources AS resource
              WHERE resource.run_id = scan.run_id AND resource.scenario = scan.scenario
                AND resource.resource_class = scan.resource_class
          ))
    )
)
BEGIN
    SELECT RAISE(ABORT, 'load-test run cannot seal with incomplete inventory');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_run_requires_terminal_resources
BEFORE UPDATE OF state ON staging_load_test_runs
FOR EACH ROW
WHEN OLD.state = 'teardown_started' AND NEW.state = 'reconciled' AND EXISTS (
    SELECT 1 FROM staging_load_test_resources
    WHERE run_id = NEW.run_id AND scenario = NEW.scenario
      AND state NOT IN ('deleted', 'preserved')
)
BEGIN
    SELECT RAISE(ABORT, 'load-test run cannot reconcile unfinished resources');
END;

-- Foreign-key enforcement can be disabled per SQLite connection.  The
-- trigger makes registration fail closed even on such a connection and closes
-- the ledger once inventory sealing begins.
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_resource_requires_open_run
BEFORE INSERT ON staging_load_test_resources
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs
    WHERE run_id = NEW.run_id AND scenario = NEW.scenario AND state = 'open'
)
BEGIN
    SELECT RAISE(ABORT, 'load-test resource requires an exact open staging run');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_scan_requires_open_run
BEFORE INSERT ON staging_load_test_resource_scans
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs
    WHERE run_id = NEW.run_id AND scenario = NEW.scenario AND state = 'open'
)
BEGIN
    SELECT RAISE(ABORT, 'load-test scan requires an exact open staging run');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_scan_frozen_after_seal
BEFORE UPDATE ON staging_load_test_resource_scans
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs
    WHERE run_id = OLD.run_id AND scenario = OLD.scenario AND state = 'open'
)
BEGIN
    SELECT RAISE(ABORT, 'load-test scan is frozen after sealing');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_scan_identity_immutable
BEFORE UPDATE OF run_id, scenario, resource_class ON staging_load_test_resource_scans
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'load-test scan identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_scan_state_forward_only
BEFORE UPDATE OF state ON staging_load_test_resource_scans
FOR EACH ROW
WHEN NOT (OLD.state = NEW.state OR (OLD.state = 'incomplete' AND NEW.state = 'complete'))
BEGIN
    SELECT RAISE(ABORT, 'load-test scan state cannot move backward');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_resource_identity_immutable
BEFORE UPDATE OF run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition
ON staging_load_test_resources
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'load-test resource attribution is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_resource_state_forward_only
BEFORE UPDATE OF state ON staging_load_test_resources
FOR EACH ROW
WHEN NOT (
    OLD.state = NEW.state
    OR (OLD.disposition = 'retained' AND OLD.state = 'registered' AND NEW.state = 'preserved'
        AND EXISTS (SELECT 1 FROM staging_load_test_runs WHERE run_id = OLD.run_id AND scenario = OLD.scenario AND state = 'teardown_started'))
    OR (OLD.disposition = 'disposable' AND OLD.state = 'registered' AND NEW.state IN ('delete_started', 'quarantined')
        AND EXISTS (SELECT 1 FROM staging_load_test_runs WHERE run_id = OLD.run_id AND scenario = OLD.scenario AND state = 'teardown_started'))
    OR (OLD.disposition = 'disposable' AND OLD.state = 'delete_started' AND NEW.state IN ('deleted', 'quarantined')
        AND EXISTS (SELECT 1 FROM staging_load_test_runs WHERE run_id = OLD.run_id AND scenario = OLD.scenario AND state = 'teardown_started'))
)
BEGIN
    SELECT RAISE(ABORT, 'load-test resource state cannot skip or reverse deletion');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_resource_no_delete
BEFORE DELETE ON staging_load_test_resources
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'load-test resource ledger is append-only');
END;
