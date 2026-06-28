-- 0079_erasure_attestation_signed_columns.sql
--
-- Artifact 1 (OKF brutal-review H1 closure): make the GDPR erasure attestation a
-- REAL signed + served certificate, not the unsigned evidence-digest the prior
-- impl persisted (which `routes/dsr/attestation.rs` correctly fail-CLOSED rather
-- than ship as "theater"). The signer in `crates/corelink-erasure-attestation`
-- already produces an Ed25519 `signature_ed25519` over the RFC-8785 JCS-canonical
-- payload bytes (`canonical_payload_jcs`), but the 0032 `erasure_attestations`
-- index table had no columns to persist them — so a verifier could never check a
-- signature. Add them (additive, nullable):
--
--  - signature_ed25519   — base64 Ed25519 signature over canonical_payload_jcs.
--  - canonical_payload_jcs — the EXACT RFC-8785 JCS bytes that were signed (the
--    verifier re-hashes/verifies THESE bytes; the verify.rs H2 fix binds the
--    typed payload to them, so a row carrying both is independently verifiable).
--
-- Both NULLABLE: SQLite `ADD COLUMN NOT NULL` needs a DEFAULT, and the erase-set
-- is additive-only (auth-migrations-additive-only). NEW rows always populate BOTH;
-- the verifier endpoint (GET /v1/public/attestation/{request_id}) treats a row
-- with either column NULL as "not a verifiable signed attestation" (404), so the
-- pre-0079 unsigned rows are never served as if signed.
--
-- Additive + idempotent guard: D1 has no `ADD COLUMN IF NOT EXISTS`, so these run
-- exactly once via the d1_migrations ledger (d1-migrations-ledger-desync).

ALTER TABLE erasure_attestations ADD COLUMN signature_ed25519 TEXT;
ALTER TABLE erasure_attestations ADD COLUMN canonical_payload_jcs TEXT;
