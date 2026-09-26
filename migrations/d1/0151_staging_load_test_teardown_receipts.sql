-- #2663: frozen exact-run teardown contract.  This is ledger metadata only;
-- physical adapters are owned by the dependent DSR, Worker, and BYOK changes.

CREATE TABLE IF NOT EXISTS staging_load_test_teardown_locators (
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    resource_class TEXT NOT NULL CHECK (resource_class IN (
        'webhook_inbox', 'webhook_effect', 'dsr_artifact', 'signup_artifact', 'byok_artifact'
    )),
    receipt_ref TEXT NOT NULL CHECK (length(receipt_ref) = 64 AND receipt_ref NOT GLOB '*[^0-9a-f]*'),
    locator_kind TEXT NOT NULL CHECK (locator_kind IN (
        'webhook_inbox_v1', 'webhook_effect_v1', 'dsr_r2_export_v1',
        'signup_pilot_v1', 'byok_pending_synthetic_v1'
    )),
    locator_json TEXT NOT NULL CHECK (length(locator_json) BETWEEN 2 AND 4096),
    registered_at_ms INTEGER NOT NULL CHECK (registered_at_ms >= 0),
    PRIMARY KEY (run_id, scenario, resource_class, receipt_ref),
    FOREIGN KEY (run_id, scenario, resource_class, receipt_ref)
        REFERENCES staging_load_test_resources(run_id, scenario, resource_class, receipt_ref)
);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_teardown_locator_requires_open_disposable_resource
BEFORE INSERT ON staging_load_test_teardown_locators
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs AS run
    JOIN staging_load_test_resources AS resource
      ON resource.run_id = run.run_id AND resource.scenario = run.scenario
     AND resource.resource_class = NEW.resource_class AND resource.receipt_ref = NEW.receipt_ref
    WHERE run.run_id = NEW.run_id AND run.scenario = NEW.scenario
      AND run.target_environment = 'staging' AND run.state = 'open'
      AND resource.disposition = 'disposable'
)
BEGIN
    SELECT RAISE(ABORT, 'teardown locator requires an exact open disposable resource');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_teardown_locator_kind_matches_class
BEFORE INSERT ON staging_load_test_teardown_locators
FOR EACH ROW
WHEN NOT (
    (NEW.resource_class = 'webhook_inbox' AND NEW.locator_kind = 'webhook_inbox_v1') OR
    (NEW.resource_class = 'webhook_effect' AND NEW.locator_kind = 'webhook_effect_v1') OR
    (NEW.resource_class = 'dsr_artifact' AND NEW.locator_kind = 'dsr_r2_export_v1') OR
    (NEW.resource_class = 'signup_artifact' AND NEW.locator_kind = 'signup_pilot_v1') OR
    (NEW.resource_class = 'byok_artifact' AND NEW.locator_kind = 'byok_pending_synthetic_v1')
)
BEGIN
    SELECT RAISE(ABORT, 'teardown locator kind does not match resource class');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_teardown_locator_immutable
BEFORE UPDATE ON staging_load_test_teardown_locators
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'teardown locator is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_teardown_locator_no_delete
BEFORE DELETE ON staging_load_test_teardown_locators
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'teardown locator is append-only');
END;

CREATE TABLE IF NOT EXISTS staging_load_test_synthetic_tenants (
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    tenant_id TEXT NOT NULL CHECK (length(tenant_id) BETWEEN 1 AND 128),
    baseline_marker TEXT NOT NULL CHECK (baseline_marker = 'generation_zero_empty'),
    marked_at_ms INTEGER NOT NULL CHECK (marked_at_ms >= 0),
    PRIMARY KEY (run_id, scenario, tenant_id),
    UNIQUE (tenant_id),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario)
);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_synthetic_tenant_requires_open_run
BEFORE INSERT ON staging_load_test_synthetic_tenants
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs
    WHERE run_id = NEW.run_id AND scenario = NEW.scenario
      AND target_environment = 'staging' AND state = 'open'
)
BEGIN
    SELECT RAISE(ABORT, 'synthetic tenant marker requires an exact open staging run');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_synthetic_tenant_immutable
BEFORE UPDATE ON staging_load_test_synthetic_tenants
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'synthetic tenant marker is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_synthetic_tenant_no_delete
BEFORE DELETE ON staging_load_test_synthetic_tenants
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'synthetic tenant marker is append-only');
END;

CREATE TABLE IF NOT EXISTS staging_load_test_prepared_intent_reconciliations (
    operation_id TEXT PRIMARY KEY NOT NULL CHECK (length(operation_id) = 64 AND operation_id NOT GLOB '*[^0-9a-f]*'),
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    resource_class TEXT NOT NULL CHECK (resource_class IN (
        'cas_reference', 'dsr_artifact', 'audit_evidence', 'byok_artifact'
    )),
    readback_outcome TEXT NOT NULL CHECK (readback_outcome IN ('absent', 'deleted', 'quarantined')),
    reconciled_at_ms INTEGER NOT NULL CHECK (reconciled_at_ms >= 0),
    FOREIGN KEY (operation_id) REFERENCES staging_load_test_r2_intents(operation_id),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario)
);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_prepared_reconciliation_matches_uncommitted_intent
BEFORE INSERT ON staging_load_test_prepared_intent_reconciliations
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_r2_intents
    WHERE operation_id = NEW.operation_id AND run_id = NEW.run_id AND scenario = NEW.scenario
      AND resource_class = NEW.resource_class AND state IN ('prepared', 'quarantined')
)
BEGIN
    SELECT RAISE(ABORT, 'prepared reconciliation requires its exact uncommitted R2 intent');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_prepared_reconciliation_immutable
BEFORE UPDATE ON staging_load_test_prepared_intent_reconciliations
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'prepared intent reconciliation is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_prepared_reconciliation_no_delete
BEFORE DELETE ON staging_load_test_prepared_intent_reconciliations
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'prepared intent reconciliation is append-only');
END;

CREATE TABLE IF NOT EXISTS staging_load_test_teardown_receipt_counts (
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    resource_class TEXT NOT NULL CHECK (resource_class IN (
        'cas_reference', 'webhook_inbox', 'webhook_effect', 'dsr_artifact',
        'dsr_obligation', 'audit_evidence', 'billing_audit', 'signup_artifact', 'byok_artifact'
    )),
    inventory_count INTEGER NOT NULL CHECK (inventory_count >= 0),
    attempted_count INTEGER NOT NULL CHECK (attempted_count >= 0 AND attempted_count <= inventory_count),
    deleted_count INTEGER NOT NULL CHECK (deleted_count >= 0),
    preserved_count INTEGER NOT NULL CHECK (preserved_count >= 0),
    remaining_count INTEGER NOT NULL CHECK (remaining_count >= 0),
    quarantined_count INTEGER NOT NULL CHECK (quarantined_count >= 0),
    cross_run_deletion_count INTEGER NOT NULL CHECK (cross_run_deletion_count = 0),
    readback_state TEXT NOT NULL CHECK (readback_state IN ('absent', 'preserved', 'not_verified')),
    PRIMARY KEY (run_id, scenario, resource_class),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario)
);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_receipt_count_requires_teardown_without_receipt
BEFORE INSERT ON staging_load_test_teardown_receipt_counts
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs
    WHERE run_id = NEW.run_id AND scenario = NEW.scenario
      AND state IN ('teardown_started', 'failed')
) OR EXISTS (
    SELECT 1 FROM staging_load_test_teardown_receipts
    WHERE run_id = NEW.run_id AND scenario = NEW.scenario
)
BEGIN
    SELECT RAISE(ABORT, 'receipt counts require teardown before one terminal receipt');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_receipt_count_immutable
BEFORE UPDATE ON staging_load_test_teardown_receipt_counts
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'teardown receipt counts are immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_receipt_count_no_delete
BEFORE DELETE ON staging_load_test_teardown_receipt_counts
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'teardown receipt counts are append-only');
END;

CREATE TABLE IF NOT EXISTS staging_load_test_teardown_receipts (
    run_id TEXT NOT NULL,
    scenario TEXT NOT NULL,
    target_deployment_sha TEXT NOT NULL CHECK (length(target_deployment_sha) = 40 AND target_deployment_sha NOT GLOB '*[^0-9a-f]*'),
    schema_version TEXT NOT NULL CHECK (schema_version = 'corelink.staging-load-test-teardown-receipt.v2'),
    terminal_state TEXT NOT NULL CHECK (terminal_state IN ('reconciled', 'failed', 'quarantined')),
    completed_at_ms INTEGER NOT NULL CHECK (completed_at_ms >= 0),
    receipt_sha256 TEXT NOT NULL CHECK (length(receipt_sha256) = 64 AND receipt_sha256 NOT GLOB '*[^0-9a-f]*'),
    PRIMARY KEY (run_id, scenario),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario)
);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_terminal_receipt_requires_exact_terminal_contract
BEFORE INSERT ON staging_load_test_teardown_receipts
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM staging_load_test_runs
    WHERE run_id = NEW.run_id AND scenario = NEW.scenario
      AND target_deployment_sha = NEW.target_deployment_sha
      AND NEW.completed_at_ms >= admitted_at_ms
      AND (
          (NEW.terminal_state = 'reconciled' AND state = 'teardown_started')
          OR (NEW.terminal_state IN ('failed', 'quarantined') AND state IN ('teardown_started', 'failed'))
      )
) OR (SELECT count(*) FROM staging_load_test_teardown_receipt_counts
      WHERE run_id = NEW.run_id AND scenario = NEW.scenario) <> 9
   OR EXISTS (
      SELECT 1 FROM staging_load_test_teardown_receipt_counts AS receipt_count
      JOIN staging_load_test_resource_scans AS scan
        ON scan.run_id = receipt_count.run_id AND scan.scenario = receipt_count.scenario
       AND scan.resource_class = receipt_count.resource_class
      WHERE receipt_count.run_id = NEW.run_id AND receipt_count.scenario = NEW.scenario
        AND receipt_count.inventory_count <> scan.observed_count
   ) OR EXISTS (
      SELECT 1 FROM staging_load_test_teardown_receipt_counts
        WHERE run_id = NEW.run_id AND scenario = NEW.scenario
          AND resource_class IN ('cas_reference', 'dsr_obligation', 'audit_evidence', 'billing_audit')
        AND (attempted_count <> 0 OR deleted_count <> 0 OR preserved_count <> inventory_count)
   )
BEGIN
    SELECT RAISE(ABORT, 'terminal receipt requires exact inventory and retained preservation counts');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_reconciled_receipt_requires_complete_readback
BEFORE INSERT ON staging_load_test_teardown_receipts
FOR EACH ROW
WHEN NEW.terminal_state = 'reconciled' AND (
    EXISTS (
        SELECT 1 FROM staging_load_test_teardown_receipt_counts
        WHERE run_id = NEW.run_id AND scenario = NEW.scenario
          AND resource_class IN ('cas_reference', 'dsr_obligation', 'audit_evidence', 'billing_audit')
          AND (attempted_count <> 0 OR deleted_count <> 0 OR preserved_count <> inventory_count
               OR remaining_count <> 0 OR quarantined_count <> 0 OR readback_state <> 'preserved')
    ) OR EXISTS (
        SELECT 1 FROM staging_load_test_teardown_receipt_counts
        WHERE run_id = NEW.run_id AND scenario = NEW.scenario
          AND resource_class IN ('webhook_inbox', 'webhook_effect', 'dsr_artifact', 'signup_artifact', 'byok_artifact')
          AND (attempted_count <> inventory_count OR deleted_count <> inventory_count OR preserved_count <> 0
               OR remaining_count <> 0 OR quarantined_count <> 0 OR readback_state <> 'absent')
    ) OR EXISTS (
        SELECT 1 FROM staging_load_test_r2_intents
        WHERE run_id = NEW.run_id AND scenario = NEW.scenario AND state <> 'committed'
    ) OR EXISTS (
        SELECT 1 FROM staging_load_test_prepared_intent_reconciliations
        WHERE run_id = NEW.run_id AND scenario = NEW.scenario
    ) OR EXISTS (
        SELECT 1 FROM staging_load_test_resources
        WHERE run_id = NEW.run_id AND scenario = NEW.scenario
          AND ((disposition = 'retained' AND state <> 'preserved')
               OR (disposition = 'disposable' AND state <> 'deleted'))
    )
)
BEGIN
    SELECT RAISE(ABORT, 'reconciled receipt requires complete readback and no unresolved R2 intent');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_terminal_receipt_immutable
BEFORE UPDATE ON staging_load_test_teardown_receipts
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'terminal receipt is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_terminal_receipt_no_delete
BEFORE DELETE ON staging_load_test_teardown_receipts
FOR EACH ROW BEGIN
    SELECT RAISE(ABORT, 'terminal receipt is append-only');
END;
