---
id: "WI-S14-007"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-003"]
parent: "S-14"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "PRIVACY-MODEL"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "COMPLIANCE-MATRIX"
tags: ["wi", "s14", "byok", "erasure-attestation", "ed25519", "nist-sp-800-88", "fips-186-5", "7y-retention", "high-risk"]
---

# WI-S14-007 — Erasure Attestation Ed25519 (FIPS 186-5) Signing per-Region (Per-Region Ed25519 Key 30d Overlap Canonical per `key_management.md §3.2.1` + ADR-0018; Rotation Worker S-13 Owns) + Per-DSR-Erasure de BYOK Tenant (S-11 Integration) Destrói CMK Access + Emite Signed Attestation `{tenant_id, request_id, destroyed_ts, kms_provider, evidence_hash, signature_ed25519}` + 7y Retention em R2 Audit Bucket + Verify Endpoint `GET /v1/public/attestation/{request_id}` via Public-Key Endpoint `GET /v1/public/keys/erasure/{region}.pub` + NIST SP 800-88 Rev.1 Crypto-Erase Mode + INV-ERASURE-ATTESTATION-SIGNED HIGH Ratificada

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-007 |
| Título | Erasure attestation Ed25519 (FIPS 186-5 EdDSA) signing per-region (per-region Ed25519 key 30d overlap canonical per `key_management.md §3.2.1` + ADR-0018; rotation worker S-13 owns generation + rotation); per-DSR-erasure (S-11 integration) de BYOK tenant destrói CMK access + emite signed attestation `{tenant_id, request_id, destroyed_ts, kms_provider, evidence_hash, signature_ed25519}`; retained 7y em R2 audit bucket; verify endpoint `GET /v1/public/attestation/{request_id}` via public-key endpoint `GET /v1/public/keys/erasure/{region}.pub`; NIST SP 800-88 Rev.1 §2.4 crypto-erase mode compliant; INV-ERASURE-ATTESTATION-SIGNED HIGH ratificada em registry §3.12 |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (erasure attestation cripto-load-bearing customer-attested), FF-HR-003 (LGPD Art. 17 GDPR Art. 17 erasure right regulatory absoluto) |

## 1. Intent

Erasure attestation Ed25519-signed verifiable post-facto: customer + auditor exigem cryptographic proof of erasure (NIST SP 800-88 Rev.1 §2.4 crypto-erase mode); CoreLink fornece per-DSR-erasure signed attestation com per-region Ed25519 signing key (30d overlap canonical), 7y retention em R2 audit bucket, public-key endpoint para offline verification. Foundation: per-region Ed25519 keys generated + rotated por S-13 secret rotation framework (BYOK + audit chain key adapters reused para Ed25519 attestation key); erasure trigger via S-11 DSR (worker destroy CMK access + atomic emit attestation); verify endpoint serves public key + per-attestation signature lookup.

```rust
// File: crates/corelink-erasure-attestation/src/lib.rs

#![forbid(unsafe_code)]

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::ZeroizeOnDrop;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Region {
    Wnam,
    Enam,
    Weur,
    Sam,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureAttestationPayload {
    pub tenant_id: String,
    pub request_id: String,
    pub destroyed_ts: u64,
    pub kms_provider: String,
    pub kms_key_id: String,
    pub evidence_hash: String, // SHA-256 of evidence bundle (audit chain ref + KMS destroy confirmation)
    pub region: Region,
    pub attestation_key_id: u64, // Ed25519 key ID used to sign (per-region overlap)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureAttestation {
    pub payload: ErasureAttestationPayload,
    pub signature_ed25519: String, // Base64-encoded Ed25519 signature
    pub canonical_payload_jcs: String, // RFC 8785 JSON Canonicalization Scheme (deterministic)
}

#[derive(ZeroizeOnDrop)]
pub struct ErasureSigningKey {
    pub key_id: u64,
    pub region: Region,
    pub created_at_ms: u64,
    pub overlap_until_ms: u64, // 30d overlap canonical per key_management.md §3.2.1
    pub signing_key: SigningKey,
}

#[derive(Debug, Clone)]
pub struct ErasurePublicKey {
    pub key_id: u64,
    pub region: Region,
    pub created_at_ms: u64,
    pub overlap_until_ms: u64,
    pub verifying_key: VerifyingKey,
    pub pem: String, // PEM-encoded public key for /v1/public/keys/erasure/{region}.pub
}

#[derive(Debug, Error)]
pub enum AttestationError {
    #[error("Ed25519 signing error: {0}")]
    Sign(String),
    #[error("Ed25519 verify error: {0}")]
    Verify(String),
    #[error("JCS canonicalization error: {0}")]
    Canonicalization(String),
    #[error("attestation key not found for region {region:?} at ts {ts_ms}")]
    KeyNotFound { region: Region, ts_ms: u64 },
    #[error("attestation key rotated; verify uses old key during overlap window")]
    KeyRotated,
    #[error("R2 storage error: {0}")]
    Storage(String),
    #[error("D1 storage error: {0}")]
    D1(String),
}

pub struct ErasureAttester {
    // ...
}

impl ErasureAttester {
    /// Generate per-region Ed25519 attestation per DSR erasure of BYOK tenant.
    /// Atomic with KMS destroy confirmation + S-09 audit chain emit.
    /// Stored in R2 audit bucket 7y retention; D1 index for verify endpoint.
    pub async fn attest_erasure(
        &self,
        payload: ErasureAttestationPayload,
    ) -> Result<ErasureAttestation, AttestationError> {
        // 1. Get current active signing key for region
        let signing_key = self.get_active_signing_key(payload.region).await?;

        // 2. Canonicalize payload via RFC 8785 JCS
        let canonical = serde_jcs::to_string(&payload)
            .map_err(|e| AttestationError::Canonicalization(e.to_string()))?;

        // 3. Sign with Ed25519
        let sig: Signature = signing_key.signing_key.sign(canonical.as_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());

        let attestation = ErasureAttestation {
            payload: payload.clone(),
            signature_ed25519: sig_b64,
            canonical_payload_jcs: canonical,
        };

        // 4. Persist em R2 audit bucket per-region 7y retention
        self.persist_to_r2_audit_bucket(&attestation).await?;

        // 5. D1 index INSERT (request_id → R2 key + region + key_id) for verify endpoint
        self.index_in_d1(&attestation).await?;

        // 6. Emit audit chain corelink.byok.erasure.attested (atomic)
        self.emit_audit_chain(&attestation).await?;

        Ok(attestation)
    }

    /// Verify endpoint: GET /v1/public/attestation/{request_id}
    /// Returns attestation; client verifies signature with public key from
    /// GET /v1/public/keys/erasure/{region}.pub
    pub async fn lookup_attestation(
        &self,
        request_id: &str,
    ) -> Result<ErasureAttestation, AttestationError> {
        unimplemented!()
    }

    /// Public-key endpoint: GET /v1/public/keys/erasure/{region}.pub
    /// Returns ALL active public keys for region (current + overlap).
    /// Customer/auditor uses to verify attestation signature offline.
    pub async fn list_public_keys(
        &self,
        region: Region,
    ) -> Result<Vec<ErasurePublicKey>, AttestationError> {
        unimplemented!()
    }

    async fn get_active_signing_key(
        &self,
        region: Region,
    ) -> Result<ErasureSigningKey, AttestationError> {
        // Lookup current active signing key for region from rotation worker S-13
        // Active state per key_lifecycle state machine
        unimplemented!()
    }

    async fn persist_to_r2_audit_bucket(
        &self,
        attestation: &ErasureAttestation,
    ) -> Result<(), AttestationError> {
        // R2 PUT corelink-audit-{region}/erasure_attestations/{request_id}.json
        // 7y retention via R2 lifecycle policy
        unimplemented!()
    }

    async fn index_in_d1(
        &self,
        attestation: &ErasureAttestation,
    ) -> Result<(), AttestationError> {
        // D1 INSERT erasure_attestations row (request_id, tenant_id, region, key_id, r2_key, signed_at_ms)
        unimplemented!()
    }

    async fn emit_audit_chain(
        &self,
        attestation: &ErasureAttestation,
    ) -> Result<(), AttestationError> {
        // CloudEvent corelink.byok.erasure.attested atomic batch
        unimplemented!()
    }
}
```

State machine erasure attestation:
```
Customer files DSR erasure (S-11 herdada)
    ↓
Cooling-off period 7d (S-11)
    ↓
DSR erasure worker triggers (S-11)
    ↓
For BYOK tenant: destroy CMK access (revoke via KMS provider API; OR customer revokes pre-DSR)
    ↓
ErasureAttester.attest_erasure(payload):
    1. Get active Ed25519 signing key per-region
    2. Canonicalize payload via RFC 8785 JCS (deterministic)
    3. Sign with Ed25519 (FIPS 186-5)
    4. Persist to R2 audit bucket per-region (7y retention)
    5. D1 index INSERT for verify endpoint
    6. Emit audit chain corelink.byok.erasure.attested atomic
    ↓
Customer + auditor receives signed attestation
    ↓
Verify offline:
    GET /v1/public/attestation/{request_id} → fetch attestation
    GET /v1/public/keys/erasure/{region}.pub → fetch public keys (current + overlap)
    Verify Ed25519 signature on canonical_payload_jcs
    NIST SP 800-88 Rev.1 §2.4 crypto-erase mode compliant
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Erasure attestation Ed25519-signed = primeira CoreLink crypto-load-bearing customer-attested feature: customer fornece DSR erasure request; CoreLink destrói CMK access + emite signed attestation Ed25519; customer + auditor verifica offline via public key endpoint. NIST SP 800-88 Rev.1 §2.4 crypto-erase mode compliant (zeroes entire data via key destroy NÃO via overwrite). 7y retention covers compliance audit windows (SOC 2 + GDPR + LGPD). Per-region Ed25519 key 30d overlap canonical (key_management.md §3.2.1; rotation worker S-13 owns) — long overlap pra retain verifiability post-rotation; verify endpoint accepts BOTH keys during overlap window.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Ed25519 signing key rotation breaks verifiability**: rotation completes; old signature unverifiable post-window. Mitigação: 30d overlap canonical (long); verify endpoint serves BOTH active + overlap keys; client verifies against either; INV-KEY-OVERLAP herdada.

2. **JCS canonicalization non-deterministic**: same payload → different bytes → signature mismatch. Mitigação: serde_jcs crate (RFC 8785 + Unicode NFC); property test 1k payloads serialize twice byte-equal (INV-AUDIT-CHAIN-HASH-DETERMINISTIC herdada).

3. **Signature forgery via key compromise**: attacker compromises Ed25519 signing key; signs fake attestation. Mitigação: per-region signing key isolated em Cloudflare Workers Secret; 30d rotation; audit chain integrity + chain hash detect tampering; emergency rotation procedure (break-glass).

4. **Verifier accepts forged signature**: verify endpoint logic flawed; rejects valid OR accepts invalid. Mitigação: verify uses public key endpoint; client-side standard ed25519-dalek verify; property test 10k forge attempts 0 false-pass.

5. **R2 audit bucket retention < 7y**: lifecycle policy mistake; attestation expires < 7y. Mitigação: R2 lifecycle policy 7y minimum; quarterly config audit verifies; alert if retention drift.

6. **D1 index lost (R2 attestation orphaned)**: D1 entry deleted; R2 attestation cannot be looked up. Mitigação: D1 index INSERT em D1 atomic batch with R2 PUT (NÃO truly atomic em distributed system; mas R2 PUT first → D1 INSERT after; if D1 fails, R2 has attestation but no index → quarterly D1 reconcile rebuild from R2).

7. **Public key endpoint compromise**: attacker serves fake public key; client verifies fake signature as valid. Mitigação: public key endpoint signed by deploy chain (Cosign keyless OIDC S-12 herdada); customer can pin public key out-of-band (DPA addendum lists fingerprint).

8. **NIST SP 800-88 Rev.1 evidence_hash incomplete**: evidence_hash should bind audit chain ref + KMS destroy confirmation; if incomplete, attestation is not crypto-erase mode compliant. Mitigação: evidence_hash = SHA-256 of canonical bundle (audit chain segment IDs + KMS destroy timestamp + KMS key_id + tenant_id); property test 1k bundles deterministic.

9. **BYOK CMK destroy fails (provider API error)**: erasure not actually destroyed; attestation fraudulent. Mitigação: attestation only emitted post-destroy success confirmation; provider API error = retry + audit emit failure event; customer notified.

**Atacante adversarial scenarios**:

- **Forge signature attack**: 10k forge attempts; verify rejects all. Mitigação: Ed25519 256-bit security; ed25519-dalek constant-time verify.

- **Replay attestation**: attacker replays valid attestation for new request_id. Mitigação: request_id unique (UUID v4); audit chain detects replay via chain hash.

- **Public key substitution**: attacker substitutes public key. Mitigação: customer pin public key fingerprint out-of-band; DPA addendum lists fingerprint.

- **Time tampering on destroyed_ts**: attacker manipulates timestamp. Mitigação: destroyed_ts derived from server-side ts (CloudFlare clock); audit chain ts independent verification.

- **Tenant impersonation**: attacker forges tenant_id. Mitigação: tenant_id derived from authenticated DSR (S-11 herdada); audit chain integrity binds.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: erasure attestation cripto-load-bearing customer-attested.
- **FF-HR-003**: LGPD Art. 17 + GDPR Art. 17 + LGPD Art. 18 erasure right regulatory absoluto.
- **Reversibility**: attestation forgery detected = post-mortem CRITICAL + Security incident + customer trust loss permanent.

11 sign-offs canonical incl. Architect (com **Crypto SME folded mandatory**: Ed25519 signing flow + JCS canonicalization + key rotation overlap + verify endpoint + NIST SP 800-88 Rev.1 evidence binding) + Security Lead + AppSec + Compliance Officer (NIST SP 800-88 + FIPS 186-5 attestation) + Privacy Officer (LGPD Art. 17/18 + GDPR Art. 17 alignment).

## 3. Customer Impact & Journey

**Persona 1 — Customer enterprise filing DSR erasure**:
- Customer files DSR erasure via S-11 portal.
- Cooling-off 7d (S-11); CMK destroyed via KMS provider; attestation generated.
- Customer receives signed attestation via email + dashboard download.
- Customer verifies offline using public key endpoint.

**Persona 2 — Auditor SOC 2 + ISO 27001 + GDPR DPO**:
- INV-ERASURE-ATTESTATION-SIGNED HIGH ratificada; verify endpoint operational; 7y retention.
- NIST SP 800-88 Rev.1 §2.4 crypto-erase mode attestation.
- FIPS 186-5 EdDSA Ed25519 verification.

**Persona 3 — Internal SRE on-call**:
- DASH-ERASURE shows attestation signing rate + verify endpoint health.
- Runbook RB-ERASURE-VERIFY (forensic verification procedure).
- Quarterly reconcile D1 index ↔ R2 attestation.

**SLA addendum**:
- Ed25519 signing latency ≤ 10ms p99.
- 7y retention em R2 audit bucket (CTRL-AUDIT-005).
- Per-region Ed25519 key 30d overlap canonical (key_management.md §3.2.1).
- Verify endpoint availability ≥ 99.9% (public-facing endpoint).
- Public key endpoint signed by deploy chain (Cosign S-12 herdada).
- Quarterly reconcile D1 ↔ R2.

## 4. Capability Mapping

- **CAP-BYOK-006** (erasure attestation Ed25519-signed) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.2 R-S14-10` + `security_model.md §6 (CTRL-KEY-015 Ed25519 attestation)` + `key_management.md §3.2.1 (Ed25519 30d overlap)` + `privacy_model.md (CTRL-PRIV-031 + DSR integration)` + `compliance_matrix.md (NIST SP 800-88 Rev.1 + FIPS 186-5)` + `invariant_registry.md §3.12 (INV-ERASURE-ATTESTATION-SIGNED HIGH nova)`.

## 5. Tipo

Crypto signing + verify endpoint + 7y retention; HIGH_RISK; FF-HR-005 + FF-HR-003.

## 6. Escopo

### 6.1 In-scope

1. **Crate `crates/corelink-erasure-attestation/`**:
   - `ErasureAttester` struct + signing flow.
   - Per-region Ed25519 key (consumed from S-13 rotation worker).
   - JCS canonicalization (serde_jcs).
   - R2 audit bucket persistence (per-region; 7y retention).
   - D1 index for verify endpoint lookup.
   - Atomic audit chain emit.

2. **Crate `crates/corelink-public-keys-api/`**:
   - `GET /v1/public/keys/erasure/{region}.pub` — returns active + overlap public keys (PEM format).
   - `GET /v1/public/attestation/{request_id}` — returns attestation JSON.
   - Public-facing endpoint (no auth required; cacheable).
   - Cosign-signed deploy ensures key authenticity (S-12 herdada).

3. **D1 schema migration `migrations/0XX_erasure_attestation.sql`**:
   ```sql
   CREATE TABLE erasure_attestations (
       request_id TEXT NOT NULL PRIMARY KEY,
       tenant_id TEXT NOT NULL,
       region TEXT NOT NULL CHECK (region IN ('wnam', 'enam', 'weur', 'sam')),
       attestation_key_id BIGINT NOT NULL,
       r2_key TEXT NOT NULL,        -- corelink-audit-{region}/erasure_attestations/{request_id}.json
       signed_at_ms BIGINT NOT NULL,
       kms_provider TEXT NOT NULL,
       kms_key_id TEXT NOT NULL,
       evidence_hash TEXT NOT NULL  -- SHA-256 hex
   );
   CREATE INDEX idx_erasure_attestations_tenant_id ON erasure_attestations(tenant_id);
   CREATE INDEX idx_erasure_attestations_region ON erasure_attestations(region);
   
   CREATE TABLE erasure_public_keys (
       key_id BIGINT NOT NULL,
       region TEXT NOT NULL CHECK (region IN ('wnam', 'enam', 'weur', 'sam')),
       state TEXT NOT NULL CHECK (state IN ('active', 'overlap', 'retired')),
       created_at_ms BIGINT NOT NULL,
       overlap_until_ms BIGINT NOT NULL,  -- 30d canonical
       public_key_pem TEXT NOT NULL,
       PRIMARY KEY (key_id, region)
   );
   CREATE INDEX idx_erasure_public_keys_region_state ON erasure_public_keys(region, state);
   ```

4. **R2 lifecycle policy** for `corelink-audit-{region}` bucket:
   - 7y retention (2557 days) for `erasure_attestations/` prefix.
   - Quarterly config audit verifies.

5. **S-13 rotation framework integration**:
   - New rotation adapter `Erasure attestation key adapter` (Ed25519 30d overlap).
   - Reuse `RotationAdapter` trait pattern.
   - Per-region rotation; staggered cron (avoid simultaneous transitions).

6. **S-11 DSR integration**:
   - DSR erasure worker (S-11) for BYOK tenant calls `KmsProvider::destroy_cmk_access` (or signals customer-revoke completion).
   - Atomic with `ErasureAttester::attest_erasure(payload)`.
   - Failure rolls back DSR completion (customer notified retry).

7. **Métricas underscored Prometheus** (per `observability_model.md §3.1`; label `plan` aplicável):
   - `corelink_erasure_attestation_signed_total{region, kms_provider, plan}` (counter).
   - `corelink_erasure_attestation_signing_duration_seconds_bucket{region, plan}` (histogram p99 ≤ 10ms target).
   - `corelink_erasure_attestation_verify_total{outcome}` (outcome ∈ ok|sig_invalid|key_not_found|key_rotated).
   - `corelink_erasure_attestation_r2_persistence_total{region, outcome}` (counter).
   - `corelink_erasure_public_keys_endpoint_total{region, outcome}` (counter).

8. **Observability** — trace spans `erasure.{attest, sign, canonicalize, persist_r2, index_d1, emit_audit, verify, lookup_public_keys}` com attributes:
   - `erasure.region` (enum).
   - `erasure.tenant_id_hashed`.
   - `erasure.request_id`.
   - `erasure.attestation_key_id`.
   - `erasure.kms_provider`.
   - `result` (enum).

9. **Audit emission** — CloudEvent per attestation:
   - `corelink.byok.erasure.attested` payload `{tenant_id_hashed, request_id, destroyed_ts, kms_provider, evidence_hash, attestation_key_id, region, signature_ed25519_b64}`.
   - Atomic batch with D1 INSERT + R2 PUT (eventual consistency reconcile quarterly).
   - 7y retention (composed via CTRL-AUDIT-005).

10. **Property tests** (10k iter PR + 100k iter nightly):
    - `prop_jcs_deterministic`: 1k payloads serialize twice byte-equal.
    - `prop_ed25519_sign_verify_roundtrip`: 10k random payloads sign + verify.
    - `prop_signature_forge_rejected`: 10k forge attempts; 0 false-pass.
    - `prop_overlap_window_verify_both_keys`: simulate rotation; verify endpoint accepts both keys during overlap.
    - `prop_evidence_hash_deterministic`: 1k evidence bundles deterministic SHA-256.
    - `prop_replay_request_id_rejected`: same request_id replayed; verify second attempt fails (UNIQUE constraint).

11. **Adversarial regression tests**:
    - Forge signature attempt (10k random sigs).
    - Replay attestation for new request_id.
    - Public key substitution.
    - Time tampering on destroyed_ts.
    - Tenant impersonation.
    - JCS canonicalization tamper (Unicode normalization bypass).
    - Signing key compromise (emergency rotation procedure).

12. **Integration test E2E**:
    - Provision BYOK tenant em staging; populate audit log.
    - File DSR erasure (S-11 mock); destroy CMK; emit attestation.
    - Verify attestation via public key endpoint (offline verification).
    - Quarterly reconcile D1 ↔ R2 dry-run.

13. **Customer-facing documentation**:
    - `docs/customer/byok-erasure-attestation.md` (sanitized).
    - Verification procedure offline (using public key + ed25519-dalek client).
    - Retention SLA (7y).
    - Public key fingerprint pinning (DPA addendum).

### 6.2 Out-of-scope (deferred)

- **DPA + Schrems II TIA legal review**: WI-S14-008.
- **TLA+ + pentest + PRR**: WI-S14-009.
- **Customer-facing verify UI**: S-16 admin UI.
- **Quarterly reconcile automation**: manual quarterly review at GA; automated post-GA.
- **Multi-attestation per tenant (e.g., per-blob granular)**: per-tenant only at GA.
- **Federated attestation chain (cross-region)**: per-region only at GA.
- **Quantum-resistant signature (ML-DSA / SPHINCS+)**: Fase 3.
- **HSM-backed Ed25519 signing key**: software signing at GA.

## 7. Anti-Scope

- Skip JCS canonicalization (RFC 8785 mandatory; property test deterministic).
- Skip 30d overlap canonical (key_management.md §3.2.1 violation).
- Skip 7y retention (CTRL-AUDIT-005 violation).
- Skip verify endpoint (customer cannot offline verify).
- Skip public key endpoint signed deploy (Cosign S-12 herdada).
- Skip evidence_hash binding audit chain ref + KMS destroy.
- Emit attestation pre-destroy (fraudulent attestation).
- Skip property tests crypto-load-bearing.
- Skip adversarial regression tests forge + replay + substitution.
- Skip integration test E2E real DSR + KMS destroy.
- Skip quarterly reconcile D1 ↔ R2.
- Skip customer doc verification procedure.
- Direct Ed25519 signing key access em CoreLink staff (rotation worker only).
- Plaintext signing key em logs / traces.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-007 — Erasure attestation Ed25519 + 7y retention + verify endpoint

  Background:
    Given S-13 rotation framework operational (Ed25519 attestation key adapter)
    Given S-11 DSR erasure flow operational
    Given 4 BYOK providers operational (WI-S14-004 + WI-S14-005)
    Given D1 schema applied + R2 audit bucket lifecycle policy 7y

  Scenario: Per-DSR-erasure attestation signed
    Given customer T1 BYOK AWS KMS files DSR erasure
    Given S-11 cooling-off 7d elapsed
    When DSR erasure worker triggers
    Then CMK access destroyed em AWS KMS
    And ErasureAttester.attest_erasure called atomic
    And payload {tenant_id, request_id, destroyed_ts, kms_provider, evidence_hash, signature_ed25519, region, attestation_key_id} signed
    And persisted to R2 corelink-audit-weur/erasure_attestations/{request_id}.json
    And D1 INSERT erasure_attestations row
    And audit chain emit corelink.byok.erasure.attested atomic

  Scenario: JCS canonicalization deterministic
    Given payload serialized via serde_jcs (RFC 8785)
    When same payload serialized twice
    Then byte-equal output
    And Unicode NFC normalization applied
    And property test 1k payloads green

  Scenario: Ed25519 signing FIPS 186-5 EdDSA
    Given active per-region signing key
    When sign(canonical_payload_jcs)
    Then signature 64 bytes Ed25519
    And verify with public key returns valid
    And FIPS 186-5 attestation

  Scenario: Verify endpoint public attestation lookup
    Given GET /v1/public/attestation/{request_id}
    When client requests
    Then attestation JSON returned
    And signature + canonical_payload_jcs included
    And client verifies offline using public key

  Scenario: Public-key endpoint per-region active + overlap
    Given GET /v1/public/keys/erasure/weur.pub
    When client requests
    Then list of active + overlap public keys (PEM format)
    And 30d overlap canonical respected
    And client uses appropriate key based on key_id em attestation

  Scenario: 7y retention R2 audit bucket
    Given R2 corelink-audit-{region} bucket lifecycle policy
    When 7y retention configured
    Then attestations persist 2557 days minimum
    And quarterly config audit verifies
    And alert if retention drift

  Scenario: Per-region Ed25519 key 30d overlap canonical
    Given S-13 rotation worker rotates Ed25519 attestation key
    When new key promoted; old key state = overlap (30d)
    Then verify endpoint accepts both keys during overlap
    And attestation signed pre-rotation verifiable post-rotation
    And INV-KEY-OVERLAP herdada

  Scenario: Forge signature attempt rejected
    Given 10k random forged signatures
    When verify_signature called
    Then 0 false-pass
    And property test prop_signature_forge_rejected green

  Scenario: Replay request_id rejected
    Given attestation for request_id R1 emitted
    When second attestation for R1 attempted
    Then UNIQUE constraint rejects
    And audit chain detects replay attempt

  Scenario: NIST SP 800-88 Rev.1 §2.4 crypto-erase mode compliant
    Given CMK destroyed (key destroy via KMS API; no overwrite needed since key destroy = data inaccessible)
    When attestation generated
    Then evidence_hash binds audit chain segment IDs + KMS destroy timestamp + KMS key_id + tenant_id
    And NIST SP 800-88 Rev.1 §2.4 attestation

  Scenario: Property tests 6 props × 10k iter green
    Given prop_jcs_deterministic + prop_ed25519_sign_verify_roundtrip + prop_signature_forge_rejected + prop_overlap_window_verify_both_keys + prop_evidence_hash_deterministic + prop_replay_request_id_rejected
    When 10k iter run em PR
    Then 0 violations
    And nightly 100k iter green

  Scenario: Adversarial regression tests 7+ scenarios green
    Given forge sig + replay + public key substitution + time tampering + tenant impersonation + JCS tamper + signing key compromise
    When red team session
    Then 7+ scenarios mitigated
    And report committed em audit folder

  Scenario: Integration test E2E real DSR + KMS destroy
    Given staging BYOK tenant + DSR erasure flow
    When E2E test runs
    Then CMK destroyed + attestation emitted + verifiable
    And quarterly reconcile D1 ↔ R2 dry-run executable

  Scenario: Customer offline verification
    Given customer receives attestation via email
    When customer downloads public key from /v1/public/keys/erasure/{region}.pub
    Then ed25519-dalek verify returns valid
    And NIST SP 800-88 Rev.1 attestation evidence pack

  Scenario: Atomic emit_audit_chain composed atomic batch
    Given attestation generation
    When D1 INSERT + R2 PUT + audit chain emit
    Then atomic batch (D1 transactional + R2 PUT first)
    And quarterly reconcile rebuild from R2 if D1 entry orphaned
```

## 9. Design Decisions

### 9.1 Why Ed25519 (NÃO RSA / ECDSA)

- FIPS 186-5 EdDSA Ed25519 standard (NIST approved 2023).
- Compact signature 64 bytes (vs RSA-2048 256 bytes); 7y retention cost-efficient.
- Constant-time verify (side-channel resistant).
- Faster sign + verify than ECDSA P-256.
- Industry standard pattern (TUF, sigstore, OpenSSH).

### 9.2 Why per-region key 30d overlap (NÃO global / 24h)

- Per-region: failure isolation; chain integrity per-region (S-09 herdada).
- 30d overlap: long for verifiability post-rotation (Ed25519 attestation is permanent record; not authentication path; long overlap acceptable).
- 24h overlap = customer might receive attestation post-rotation; verify endpoint cannot resolve OLD key signature 1 month later.
- key_management.md §3.2.1 + ADR-0018 documented canonical.

### 9.3 Why JCS canonicalization (RFC 8785; NÃO custom)

- Deterministic JSON serialization for signature stability.
- RFC 8785 industry standard (used em DSSE, sigstore, COSE).
- serde_jcs Rust crate available + audit-grade.
- Unicode NFC normalization built-in.

### 9.4 Why 7y retention em R2 audit bucket (NÃO 1y)

- SOC 2 Type II audit retention 7y.
- GDPR Art. 17 erasure right requires forensic proof; 7y covers regulatory window.
- LGPD Art. 18 same.
- CTRL-AUDIT-005 herdada (composed pattern).

### 9.5 Why verify endpoint public (NÃO authenticated)

- Customer + auditor needs offline verification.
- Public key endpoint also public (signed by Cosign deploy).
- No authentication = no tenant context leak; just attestation lookup by request_id.
- Cacheable (CDN-friendly).

### 9.6 Why evidence_hash bind audit chain ref + KMS destroy

- NIST SP 800-88 Rev.1 §2.4 crypto-erase mode requires evidence of destroy.
- evidence_hash = SHA-256 of {audit_chain_segment_ids, kms_destroy_ts, kms_key_id, tenant_id}.
- Customer + auditor verifies binding via audit chain query (S-09 herdada).

### 9.7 Why attestation key separate from audit chain key

- Audit chain key (S-09) covers integrity of audit events (24h overlap; high churn).
- Erasure attestation key separate (30d overlap; low churn; customer-facing).
- Per-region scoping per S-09 + S-14 herdada.

### 9.8 Why ADR potencial?

- Sim — **ADR-XXXX**: "Erasure attestation Ed25519 (FIPS 186-5) + JCS canonicalization + per-region 30d overlap + verify endpoint + NIST SP 800-88 Rev.1 §2.4 crypto-erase mode S-14". Decisão arquitetural cripto-load-bearing customer-attested.

## 10. Completeness Criteria SOTA

- [ ] **10.s14.007.1** Crate `corelink-erasure-attestation` + ErasureAttester + Ed25519 signing operational (EVT-013).
- [ ] **10.s14.007.2** JCS canonicalization deterministic; serde_jcs integration (EVT-002).
- [ ] **10.s14.007.3** Per-region Ed25519 key 30d overlap canonical (S-13 rotation framework integration) (EVT-022).
- [ ] **10.s14.007.4** R2 audit bucket persistence 7y retention; lifecycle policy applied (EVT-013).
- [ ] **10.s14.007.5** D1 erasure_attestations + erasure_public_keys migrations applied (EVT-013).
- [ ] **10.s14.007.6** Public key endpoint `GET /v1/public/keys/erasure/{region}.pub` (EVT-013).
- [ ] **10.s14.007.7** Verify endpoint `GET /v1/public/attestation/{request_id}` (EVT-013).
- [ ] **10.s14.007.8** S-11 DSR integration: erasure worker calls attest_erasure atomic com KMS destroy (EVT-024).
- [ ] **10.s14.007.9** NIST SP 800-88 Rev.1 §2.4 crypto-erase mode evidence_hash binding (EVT-044).
- [ ] **10.s14.007.10** Property tests 6 props × 10k iter PR + 100k nightly green (EVT-022).
- [ ] **10.s14.007.11** Adversarial regression tests 7+ scenarios green (EVT-040).
- [ ] **10.s14.007.12** Integration test E2E real DSR + KMS destroy + customer offline verification green (EVT-024).
- [ ] **10.s14.007.13** Quarterly reconcile D1 ↔ R2 dry-run executable.
- [ ] **10.s14.007.14** ADR-XXXX (erasure attestation Ed25519 + JCS + 30d overlap + NIST SP 800-88) ratificada.
- [ ] **10.s14.007.15** SOC 2 + ISO 27001 + LGPD Art. 17/18 + GDPR Art. 17 + NIST SP 800-88 + FIPS 186-5 attestation em PRR doc (EVT-044).
- [ ] **10.s14.007.16** INV-ERASURE-ATTESTATION-SIGNED HIGH ratificada em registry §3.12 (EVT-022).

## 11. DoD

- [ ] Crates `corelink-erasure-attestation` + `corelink-public-keys-api` compilam.
- [ ] D1 migrations applied.
- [ ] R2 lifecycle policy 7y configured.
- [ ] S-13 rotation framework integration (Ed25519 attestation key adapter).
- [ ] S-11 DSR integration (atomic destroy + attest).
- [ ] All 16 Gherkin scenarios green em integration test.
- [ ] Property tests 6 props × 10k iter green; 100k nightly green.
- [ ] Adversarial regression tests 7+ scenarios green.
- [ ] Integration test E2E real DSR + KMS destroy + customer offline verification green.
- [ ] CloudEvent audit emission per attestation atomic batch.
- [ ] Métricas + trace spans operational.
- [ ] Verify endpoint + public key endpoint operational.
- [ ] Customer doc `docs/customer/byok-erasure-attestation.md`.
- [ ] ADR-XXXX (erasure attestation) escrito + ratificado.
- [ ] Code review (Architect + Crypto SME folded mandatory + Security Lead + AppSec + Compliance + Privacy).
- [ ] PRR Architect + Crypto SME mini-sign-off.
- [ ] INV-ERASURE-ATTESTATION-SIGNED ratificada.

## 12. Invariants Validated

### Mantidas

- **INV-KEY-OVERLAP** (HIGH — registry §3.13 herdada): Ed25519 attestation key 30d overlap canonical (key_management.md §3.2.1 + ADR-0018); rotation worker S-13 owns.
- **INV-AUDIT-CHAIN-HASH-DETERMINISTIC** (HIGH — registry §3.14 herdada): JCS canonicalization deterministic.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada): attestation emit atomic batch.
- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH — registry §3.12 herdada): chain hash unbroken across attestation events.

### Novas

- **INV-ERASURE-ATTESTATION-SIGNED** (HIGH — registry §3.12 NEW): este WI ratifies primary; per-DSR-erasure de BYOK tenant produz attestation Ed25519-signed verifiable. Why: customer + auditor exigem proof. How: per-erasure attestation generation + 7y retention + verify endpoint + per-region Ed25519 30d overlap.

TLA+ alignment: registry §4.2 indica `key_lifecycle.tla` PLANNED S-13 (rotation) covers Ed25519 attestation key rotation; este WI provê attestation implementation.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate corelink-erasure-attestation | `crates/corelink-erasure-attestation/src/lib.rs` | Rust |
| Crate corelink-public-keys-api | `crates/corelink-public-keys-api/src/lib.rs` | Rust |
| D1 migrations | `migrations/0XX_erasure_attestation.sql` | SQL |
| R2 lifecycle policy | `infra/terraform/modules/corelink-region/audit_lifecycle.tf` | HCL |
| S-13 rotation adapter Ed25519 | `crates/corelink-rotation-adapters/src/erasure_attestation.rs` | Rust |
| Property tests | `crates/corelink-erasure-attestation/tests/prop_attestation.rs` | Rust |
| Adversarial tests | `crates/corelink-erasure-attestation/tests/adversarial.rs` | Rust |
| Integration test E2E | `tests/e2e_erasure_attestation_dsr.rs` | Rust |
| Verify offline client example | `crates/corelink-erasure-attestation/examples/verify_offline.rs` | Rust |
| Customer doc | `docs/customer/byok-erasure-attestation.md` | Markdown |
| ADR-XXXX (erasure attestation) | `specs/03_architecture/adrs/ADR-XXXX-erasure-attestation-ed25519-jcs.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s14.007.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s14.007.2** rustdoc 100% public API + 4 examples (sign, verify offline, lookup, list_public_keys).
- **14.s14.007.3** Test coverage ≥ 95%; property tests 10k+100k.
- **14.s14.007.4** Latência: signing ≤ 10ms p99; verify ≤ 5ms p99.
- **14.s14.007.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s14.007.6** ZeroizeOnDrop em ErasureSigningKey.
- **14.s14.007.7** Constant-time signature verify (ed25519-dalek default).
- **14.s14.007.8** Breaking changes em attestation schema = bump major + ADR + re-sign migration plan.
- **14.s14.007.9** Memory bounded; 7y retention via R2 lifecycle.
- **14.s14.007.10** Cost regression gate em CI.
- **14.s14.007.11** NIST SP 800-88 Rev.1 §2.4 + FIPS 186-5 + LGPD Art. 17/18 + GDPR Art. 17 attestation.
- **14.s14.007.12** RFC 8785 JCS canonicalization mandatory.

## 15. Chaos Experiments

1. **Forge signature attempt**: 10k random sigs; verify rejects all.

2. **Replay attestation**: replay valid attestation for new request_id; UNIQUE rejects.

3. **Public key substitution**: substitute public key endpoint; customer pin fingerprint detects.

4. **Time tampering on destroyed_ts**: server-side ts independent verification.

5. **Tenant impersonation**: forged tenant_id; audit chain integrity binds.

6. **JCS canonicalization tamper**: Unicode normalization bypass attempt; serde_jcs rejects.

7. **Signing key compromise simulation**: emergency rotation procedure (break-glass); audit chain detect.

8. **D1 entry orphan (R2 only)**: simulate D1 INSERT failure; quarterly reconcile rebuilds.

9. **R2 retention drift**: simulate lifecycle policy mistake; quarterly config audit detects.

10. **30d overlap window edge**: rotation completes; verify accepts both keys; post-overlap accepts new only.

11. **NIST SP 800-88 evidence_hash incomplete**: bundle missing element; verify rejects attestation.

12. **Customer offline verification flow**: simulate customer downloads + verifies; assert success.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Architect + Crypto SME mandatory + Security Lead + AppSec + Compliance + Privacy review):

- [ ] All 16 Gherkin scenarios green.
- [ ] Property tests 6 props × 10k iter green; 100k nightly green.
- [ ] Adversarial regression tests 7+ scenarios green.
- [ ] Integration test E2E real DSR + KMS destroy green.
- [ ] Verify endpoint + public key endpoint operational.
- [ ] 7y retention R2 lifecycle policy verified.
- [ ] Quarterly reconcile dry-run.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-ERASURE.
- [ ] ADR-XXXX (erasure attestation) published.
- [ ] Crypto SME review (Ed25519 + JCS + key rotation overlap + verify flow + NIST SP 800-88 evidence binding).
- [ ] Compliance Officer review (NIST SP 800-88 + FIPS 186-5 + LGPD Art. 17/18 + GDPR Art. 17).
- [ ] Privacy Officer review (LGPD + GDPR + customer offline verification UX).
- [ ] AppSec review (forge + replay + substitution + JCS tamper).
- [ ] Architect approval (composition with WI-S14-004..006 + S-11 DSR + S-13 rotation).
- [ ] INV-ERASURE-ATTESTATION-SIGNED ratificada runtime + property test.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate corelink-erasure-attestation scaffold + types | 2h |
| ST-002 | Ed25519 signing flow + JCS canonicalization integration | 3h |
| ST-003 | R2 audit bucket persistence 7y retention | 2h |
| ST-004 | D1 schema migrations + indexes | 1.5h |
| ST-005 | S-13 rotation adapter Ed25519 attestation key (per-region 30d overlap) | 2h |
| ST-006 | S-11 DSR integration: atomic destroy + attest | 2h |
| ST-007 | Crate corelink-public-keys-api + verify + public key endpoints | 2.5h |
| ST-008 | NIST SP 800-88 Rev.1 evidence_hash binding | 1.5h |
| ST-009 | Métricas emit (5 metrics) + trace spans + audit emission | 2h |
| ST-010 | Property tests 6 props × 10k iter | 4h |
| ST-011 | Adversarial regression tests 7+ scenarios | 3h |
| ST-012 | Integration test E2E real DSR + KMS destroy | 3h |
| ST-013 | Customer offline verification example | 1.5h |
| ST-014 | Customer doc `docs/customer/byok-erasure-attestation.md` | 1.5h |
| ST-015 | Quarterly reconcile D1 ↔ R2 dry-run procedure | 1h |
| ST-016 | rustdoc + 4 examples | 2h |
| ST-017 | ADR-XXXX redação | 2.5h |
| ST-018 | Code review (Architect + Crypto SME folded + Security + AppSec + Compliance + Privacy) | 3h |

**Total Optimistic**: ~40h. **PERT** (O=10h, M=16h, P=26h, per spec contract §12): **16.7h**.

## 18. Dependencies

### Hard blockers

- **WI-S14-004 SEALED** (BYOK trait + AwsKmsProvider).
- **WI-S14-005 SEALED** (3 additional providers).
- **WI-S14-006 SEALED** (kill switch SLA enforced).
- **S-09 SEALED** (audit chain per-region + atomic batch).
- **S-11 SEALED** (DSR erasure worker + cooling-off + tooling).
- **S-13 SEALED** (rotation framework; Ed25519 attestation key adapter integration).
- ed25519-dalek + serde_jcs Rust crates available.

### Soft blockers

- R2 lifecycle policy support (Cloudflare R2 lifecycle).

### Outbound

- **WI-S14-008** (DPA amendment references erasure attestation + 7y retention).
- **WI-S14-009** (TLA+ + pentest covers attestation + verify endpoint).

## 19. Effort PERT

O: 10h, M: 16h, P: 26h → PERT **16.7h** (per spec contract §12).

## 20. Time-boxing

**24h hard limit owner**. If exceeded → escalation: split em sub-WI (signing vs endpoints vs DSR integration).

## 21. Observability

5 métricas listadas §6.1.7. Trace spans em §6.1.8. Logs structured JSON.

Dashboard widget DASH-ERASURE (sub-section em DASH-BYOK):
- Attestation signing rate per region (gauge over time).
- Attestation signing latency p99 per region.
- Verify endpoint health (5xx rate; alert > 0).
- Public key endpoint health.
- Quarterly reconcile D1 ↔ R2 status.

## 22. Cost Analysis

- Ed25519 signing compute: negligible.
- R2 7y retention: ~$10/mês baseline (small attestation files).
- D1 erasure_attestations + erasure_public_keys storage: ~$5/mês.
- Public key endpoint serving: cacheable; ~$2/mês.
- S-11 DSR worker integration: composed; ~$3/mês.
- **Total custo direto WI-S14-007**: ~$20/mês.

## 23. API Contract

Customer-facing public endpoints:
- `GET /v1/public/attestation/{request_id}` → 200 attestation JSON.
- `GET /v1/public/keys/erasure/{region}.pub` → 200 PEM-encoded public keys list.

Internal API:
- `ErasureAttester::attest_erasure(payload)` → atomic emit attestation.

API semver stable post v1.0; breaking changes em attestation schema = bump major + ADR + re-sign migration plan.

## 24. Post-mortem Hooks

- Forge signature detected → CRITICAL post-mortem + Security incident + emergency rotation.
- 7y retention drift detected → SEV-2 + Compliance Officer.
- Verify endpoint outage > 30 min → SEV-3 + customer notification.
- D1 entry orphan (R2 only) detected > 1% rate → SEV-2 + reconcile review.
- Signing key compromise suspected → CRITICAL + emergency rotation + customer notification.
- NIST SP 800-88 evidence_hash incomplete → CRITICAL + ADR review.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- Attestation rollback: NÃO rollback possible (attestation = forensic record).
- Verify endpoint rollback: revert Worker; re-deploy.
- Emergency rotation: break-glass procedure (Security Lead + Architect + Crypto SME approval + ADR retroactive).
- RTO ≤ 30 min (Worker rollback).
- RPO 0 (R2 + D1 preserve all attestations).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: signing key per-region rotated; public key endpoint signed deploy (Cosign).
- **Tampering**: JCS canonicalization deterministic; signature integrity verifiable; chain hash detect.
- **Repudiation**: per-attestation signed Ed25519 + 7y retention.
- **Information disclosure**: attestation contains tenant_id + request_id + KMS key_id (intentional customer evidence; pseudonymous via tenant_id hash em audit chain).
- **DoS**: verify endpoint cacheable + bounded latency.
- **Elevation of privilege**: signing key only accessible by erasure worker service account.

**LINDDUN delta**:
- **Linkability**: attestation links tenant_id + request_id (intentional forensic evidence).
- **Identifiability**: tenant_id em attestation (intentional; customer-attested).
- **Non-repudiation**: cripto property intentional (Ed25519 + 7y retention).
- **Detectability**: forge attempts detected; replay rejected.
- **Disclosure**: customer + auditor receive signed attestation; public key endpoint enables offline verification.
- **Unawareness**: customer notified pre-erasure (S-11 7d cooling-off); attestation delivered post-erasure.
- **Non-compliance**: SOC 2 + LGPD Art. 17/18 + GDPR Art. 17 + NIST SP 800-88 Rev.1 + FIPS 186-5 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-erasure-attestation/README.md` — overview + sign + verify flow.
- ADR-XXXX — erasure attestation ratification.
- Doc `docs/customer/byok-erasure-attestation.md` — customer offline verification procedure.
- Doc `docs/internal/multi-region-byok.md` (erasure section) — sequence diagram per-DSR-erasure.
- Workshop interno (2h) com Architect + Crypto SME + Security Lead + AppSec + Compliance + Privacy + on-call.
- Onboarding test (10 questions): Ed25519 + JCS canonicalization, 30d overlap canonical, 7y retention, verify endpoint, public key endpoint, NIST SP 800-88 evidence binding, INV-ERASURE-ATTESTATION-SIGNED, FIPS 186-5, atomic emit, customer offline verification.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Forge signature attempt | L | H | CRITICAL | M | LOW | Ed25519 256-bit + property test 10k forge + audit |
| R-002 | Replay attestation | L | M | HIGH | L | LOW | UNIQUE constraint + audit chain |
| R-003 | Public key substitution | L | M | HIGH | L | LOW | Cosign signed deploy + customer pin fingerprint |
| R-004 | JCS non-deterministic | L | H | CRITICAL | M | LOW | serde_jcs + property test 1k payloads |
| R-005 | 7y retention drift | L | M | HIGH | L | LOW | R2 lifecycle policy + quarterly audit |
| R-006 | D1 entry orphan | M | L | LOW | L | LOW | Quarterly reconcile from R2 |
| R-007 | Signing key compromise | L | H | CRITICAL | M | LOW | 30d rotation + audit chain detect + emergency procedure |
| R-008 | Time tampering destroyed_ts | L | M | MEDIUM | L | LOW | Server-side ts + audit chain ts independent |
| R-009 | Tenant impersonation | L | M | HIGH | L | LOW | DSR auth (S-11) + audit chain integrity |
| R-010 | NIST SP 800-88 evidence incomplete | L | M | HIGH | L | LOW | Property test bundle + ADR review |
| R-011 | Verify endpoint outage | L | L | MEDIUM | L | LOW | Cloudflare CDN cache + redundancy |
| R-012 | Customer pre-erasure CMK substitution | L | L | LOW | L | LOW | KMS destroy confirmation pre-attestation |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME folded review Ed25519 flow + JCS + 30d overlap + verify endpoint + NIST SP 800-88 evidence binding.
2. **Code (D+1)**: peer review + Crypto SME pair-program adversarial tests.
3. **Security (D+2)**: Security Lead review threat model + signing key access scope + emergency rotation procedure.
4. **AppSec (D+2)**: AppSec review CVE-class scenarios + forge + replay + substitution.
5. **Privacy (D+3)**: Privacy Officer review LGPD Art. 17/18 + GDPR Art. 17 + customer offline verification UX.
6. **Compliance (D+3)**: Compliance Officer review NIST SP 800-88 Rev.1 §2.4 + FIPS 186-5 + 7y retention.
7. **Property test (pre-merge D+4)**: 6 props × 10k iter green; 100k nightly green.
8. **Adversarial (pre-merge D+4)**: red team session — forge + replay + substitution + JCS tamper.
9. **Integration test (D+4)**: real DSR + KMS destroy + customer offline verification.
10. **PRR mini (D+5)**: Architect + Crypto SME + Security + AppSec + Compliance + Privacy sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization MANDATORY (Ed25519 + JCS canonicalization + 30d overlap + verify flow + NIST SP 800-88 evidence binding + emergency rotation)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — signing key access scope + emergency rotation + threat model_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — verify endpoint + 7y retention + quarterly reconcile_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD; emphatic — property test 10k + 100k + adversarial coverage_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — NIST SP 800-88 Rev.1 §2.4 + FIPS 186-5 + LGPD Art. 17/18 + GDPR Art. 17 + 7y retention attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; emphatic — LGPD + GDPR + customer offline verification UX + erasure right_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — CVE-class adversarial + forge + replay + substitution + JCS tamper_ | _pending_ | _pending_ |

> Crypto SME (Ed25519 + JCS + 30d overlap + verify + NIST SP 800-88 evidence binding cross-validation) folds into Architect role specialization MANDATORY.

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-007 (cycle 12.S14.0); Ed25519 + 7y + verify + NIST SP 800-88 Rev.1 §2.4. |

## 32. Anti-patterns evitados

- Skip JCS canonicalization (RFC 8785 mandatory).
- Skip 30d overlap canonical.
- Skip 7y retention.
- Skip verify endpoint public.
- Skip public key endpoint signed deploy.
- Skip evidence_hash binding audit chain + KMS destroy.
- Emit attestation pre-destroy.
- Skip property tests crypto-load-bearing.
- Skip adversarial regression forge + replay + substitution.
- Skip integration test E2E real DSR + KMS destroy.
- Skip quarterly reconcile.
- Skip customer doc verification procedure.
- Direct signing key access em CoreLink staff.
- Plaintext signing key em logs.

---

**Fim WI-S14-007.** Próximo: WI-S14-008 (DPA amendment + Schrems II TIA + Legal review).
