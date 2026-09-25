-- #2546: append-only, one-time nonce consumption for verified staging claims.
-- The nonce value never crosses the storage boundary; only its
-- domain-separated SHA-256 digest is persisted.

CREATE TABLE IF NOT EXISTS staging_load_test_admission_nonces (
    nonce_digest TEXT NOT NULL CHECK (
        length(nonce_digest) = 64
        AND nonce_digest NOT GLOB '*[^0-9a-f]*'
    ),
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
    issued_at_ms INTEGER NOT NULL CHECK (issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK (
        expires_at_ms > issued_at_ms
        AND expires_at_ms - issued_at_ms <= 900000
    ),
    admitted_at_ms INTEGER NOT NULL CHECK (admitted_at_ms >= issued_at_ms AND admitted_at_ms < expires_at_ms),
    PRIMARY KEY (nonce_digest),
    UNIQUE (run_id, scenario),
    FOREIGN KEY (run_id, scenario) REFERENCES staging_load_test_runs(run_id, scenario)
);

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_admission_nonce_no_update
BEFORE UPDATE ON staging_load_test_admission_nonces
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'staging admission nonce record is immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_staging_load_test_admission_nonce_no_delete
BEFORE DELETE ON staging_load_test_admission_nonces
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'staging admission nonce record is append-only');
END;
