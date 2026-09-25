-- B-054 operation-bound provider approvals. A successful authorization is
-- consumed before any epoch-admin operation; the unique nonce and approval id
-- make provider artifacts single-use across retries and concurrent requests.
CREATE TABLE IF NOT EXISTS audit_chain_admin_approval (
    approval_id TEXT PRIMARY KEY NOT NULL,
    nonce_hash TEXT UNIQUE NOT NULL CHECK (
        length(nonce_hash) = 64 AND nonce_hash NOT GLOB '*[^0-9a-f]*'
    ),
    executor_subject_id TEXT NOT NULL CHECK (length(executor_subject_id) BETWEEN 1 AND 256),
    approver_subject_id TEXT NOT NULL CHECK (length(approver_subject_id) BETWEEN 1 AND 256),
    operation_digest_hex TEXT NOT NULL CHECK (
        length(operation_digest_hex) = 64 AND operation_digest_hex NOT GLOB '*[^0-9a-f]*'
    ),
    approval_jcs_b64 TEXT NOT NULL CHECK (length(approval_jcs_b64) > 0),
    approval_signature_b64 TEXT NOT NULL CHECK (length(approval_signature_b64) = 88),
    issued_at_ms INTEGER NOT NULL CHECK (issued_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK (
        expires_at_ms > issued_at_ms AND expires_at_ms - issued_at_ms <= 300000
    ),
    consumed_at_ms INTEGER NOT NULL CHECK (
        consumed_at_ms >= issued_at_ms AND consumed_at_ms < expires_at_ms
    ),
    CHECK (executor_subject_id <> approver_subject_id)
);

CREATE TRIGGER IF NOT EXISTS audit_chain_admin_approval_no_update
BEFORE UPDATE ON audit_chain_admin_approval
BEGIN
    SELECT RAISE(ABORT, 'B-054 admin approvals are append-only');
END;

CREATE TRIGGER IF NOT EXISTS audit_chain_admin_approval_no_delete
BEFORE DELETE ON audit_chain_admin_approval
BEGIN
    SELECT RAISE(ABORT, 'B-054 admin approvals are append-only');
END;
