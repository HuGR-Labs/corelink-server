---
id: "WI-S14-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-008"]
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
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "COMPLIANCE-MATRIX"
tags: ["wi", "s14", "byok", "aws-kms", "envelope-encryption", "fips-140-3", "kms-provider-trait", "matrix-test", "high-risk"]
---

# WI-S14-004 — BYOK Adapter Trait `crates/corelink-byok` (Rust) + AWS KMS Adapter First-Class FIPS 140-3 Level 1 Documented + Envelope Encryption Flow (DEK Ephemeral Random 32 Bytes via CSPRNG [Lote 10.14 codex P1 fix; NÃO BLAKE3-derived deterministic — deterministic = compromise propagation] → Body AES-256-GCM Nonce per-Write 96-bit Random → Wrap DEK via KMS → Wrapped DEK em D1 + Body em R2; Read: Fetch Wrapped → Unwrap via KMS Network Call ≤ 30ms p99 Region-Co-Located → Decrypt Body) + DEK Cache TTL 5 Min Hard Limit (No Exception, No Advisory Mode; INV-BYOK-CRYPTO-SOVEREIGNTY Enforces) + 16-Combination Matrix Test Framework + FIPS Doc per Provider em `compliance/byok-fips-matrix.md`

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-004 |
| Título | BYOK adapter trait + AWS KMS first-class adapter FIPS 140-3 Level 1 documented + envelope encryption flow (ephemeral DEK random 32 bytes via CSPRNG [`getrandom::getrandom`; Lote 10.14 codex P1 canonical fix — NÃO BLAKE3-derived deterministic; deterministic DEK = compromise propagation across blobs same hash; re-wrap durante CMK rotation = unwrap+rewrap atomic preserving DEK identity] → body AES-256-GCM nonce per-write 96-bit random → wrap DEK via KMS → store wrapped DEK em D1 + body em R2; read: fetch wrapped DEK → unwrap via KMS network call ≤ 30ms p99 region-co-located → cache DEK 5 min TTL hard → decrypt body); DEK cache TTL 5 min hard limit (NO exception, NO advisory mode; INV-BYOK-CRYPTO-SOVEREIGNTY enforces); 16-combination matrix test framework (4 providers × 4 ops {write, read, wrap, unwrap}); FIPS 140-3 doc AWS KMS Level 1 verified em `compliance/byok-fips-matrix.md`; biggest crypto WI; cripto-load-bearing |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (BYOK introduz crypto controls — bypass = customer trust permanently lost; cripto-load-bearing), FF-HR-008 (vendor lock-in risk via KMS provider; matrix testing required) |

## 1. Intent

Implementar BYOK enterprise tier core: trait `KmsProvider` Rust + AWS KMS adapter first-class (FIPS 140-3 Level 1 verified) + envelope encryption flow (DEK ephemeral + AES-256-GCM body encryption + KMS wrap) + DEK cache TTL 5 min hard limit (INV-BYOK-CRYPTO-SOVEREIGNTY) + 16-combination matrix test framework + FIPS doc. Foundation para WI-S14-005 (GCP/Azure/Vault) + WI-S14-006 (kill switch) + WI-S14-007 (erasure attestation).

```rust
// File: crates/corelink-byok/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KmsProviderKind {
    AwsKms,
    GcpKms,
    AzureKeyVault,
    HashicorpVault,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KmsKeyId {
    pub provider: KmsProviderKind,
    pub key_arn_or_id: String, // AWS ARN, GCP resource name, Azure URI, Vault path
    pub region: String,
}

/// Wrapped DEK as returned by KMS.
/// Variable size depending on provider (~150-300 bytes typically).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedDek {
    pub provider: KmsProviderKind,
    pub key_id: KmsKeyId,
    pub ciphertext: Vec<u8>,           // KMS-wrapped DEK ciphertext
    pub encryption_context: Option<serde_json::Value>, // AAD per-provider semantics
}

/// Plaintext DEK (32 bytes; CSPRNG-generated via getrandom; NOT BLAKE3-derived; Lote 10.14 codex P1 fix).
/// Wrapped em ZeroizeOnDrop so memory cleared post-use.
/// NEVER persisted; only in-memory + DEK cache 5 min TTL hard.
#[derive(ZeroizeOnDrop, Zeroize)]
pub struct Dek {
    pub bytes: [u8; 32],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KmsAccessStatus {
    Ok,
    Revoked,        // Customer revoked CMK access (kill switch)
    Throttled,      // Provider rate limit
    ApiError(u16),  // HTTP status
    NotFound,       // CMK deleted by customer
}

#[derive(Debug, Error)]
pub enum BYOKError {
    #[error("KMS provider error: {0}")]
    Provider(String),
    #[error("CMK access revoked: provider={provider:?} key_id={key_id}")]
    CmkRevoked {
        provider: KmsProviderKind,
        key_id: String,
    },
    #[error("DEK cache TTL exceeded; re-fetch required")]
    DekCacheExpired,
    #[error("DEK cache TTL > 5 min ATTEMPTED; INV-BYOK-CRYPTO-SOVEREIGNTY violation")]
    DekCacheTtlViolation,
    #[error("envelope encryption error: {0}")]
    EnvelopeError(String),
    #[error("AES-GCM decrypt error: {0}")]
    AesGcm(String),
    #[error("FIPS compliance level mismatch: required={required:?} actual={actual:?}")]
    FipsMismatch {
        required: FipsLevel,
        actual: FipsLevel,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum FipsLevel {
    None,
    Fips140_2_L1,
    Fips140_2_L2,
    Fips140_3_L1,
    Fips140_3_L2,
}

#[async_trait]
pub trait KmsProvider: Send + Sync {
    fn provider_kind(&self) -> KmsProviderKind;
    fn region(&self) -> &str;
    fn fips_level(&self) -> FipsLevel;

    /// Wrap DEK via KMS. Network call; latency ≤ 30ms p99 region-co-located.
    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError>;

    /// Unwrap DEK via KMS. Network call; latency ≤ 30ms p99 region-co-located.
    /// Result cached em DEK cache 5 min TTL HARD limit.
    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError>;

    /// Check CMK access. Background every 60s per active BYOK tenant.
    /// Revoked → kill switch path (WI-S14-006).
    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError>;
}

/// DEK cache: 5 min TTL hard limit; NO exception, NO advisory mode.
/// INV-BYOK-CRYPTO-SOVEREIGNTY enforces.
pub struct DekCache {
    // ...
}

impl DekCache {
    /// CRITICAL: ttl_seconds MUST be ≤ 300 (5 min); else BYOKError::DekCacheTtlViolation.
    pub fn new(ttl_seconds: u64) -> Result<Self, BYOKError> {
        if ttl_seconds > 300 {
            return Err(BYOKError::DekCacheTtlViolation);
        }
        unimplemented!()
    }

    pub async fn get(&self, wrapped: &WrappedDek) -> Option<Dek> {
        unimplemented!()
    }

    pub async fn put(&self, wrapped: &WrappedDek, dek: Dek) -> Result<(), BYOKError> {
        unimplemented!()
    }

    /// Atomic eviction on revoke event (subscribe-pub from KMS access check).
    pub async fn evict_all_for_key(&self, key_id: &KmsKeyId) -> Result<(), BYOKError> {
        unimplemented!()
    }
}
```

```rust
// File: crates/corelink-byok-aws/src/lib.rs

use async_trait::async_trait;
use aws_sdk_kms::{Client, Region as AwsRegion};
use corelink_byok::*;

pub struct AwsKmsProvider {
    client: Client,
    region: String,
}

impl AwsKmsProvider {
    pub async fn new(region: &str) -> Result<Self, BYOKError> {
        let config = aws_config::from_env()
            .region(AwsRegion::new(region.to_string()))
            .load()
            .await;
        let client = Client::new(&config);
        Ok(Self {
            client,
            region: region.to_string(),
        })
    }
}

#[async_trait]
impl KmsProvider for AwsKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AwsKms
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_3_L1
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        let mut req = self.client.encrypt()
            .key_id(&key_id.key_arn_or_id)
            .plaintext(aws_sdk_kms::primitives::Blob::new(dek.bytes.to_vec()));

        if let Some(ctx) = encryption_context {
            // Convert serde_json::Value to AWS encryption_context HashMap<String, String>
            // ...
        }

        let resp = req.send().await
            .map_err(|e| BYOKError::Provider(format!("aws kms encrypt: {}", e)))?;

        let ciphertext = resp.ciphertext_blob()
            .ok_or_else(|| BYOKError::Provider("missing ciphertext".to_string()))?
            .clone()
            .into_inner();

        Ok(WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: key_id.clone(),
            ciphertext,
            encryption_context: encryption_context.cloned(),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        let resp = self.client.decrypt()
            .ciphertext_blob(aws_sdk_kms::primitives::Blob::new(wrapped.ciphertext.clone()))
            .send()
            .await
            .map_err(|e| BYOKError::Provider(format!("aws kms decrypt: {}", e)))?;

        let plaintext = resp.plaintext()
            .ok_or_else(|| BYOKError::Provider("missing plaintext".to_string()))?
            .clone()
            .into_inner();

        if plaintext.len() != 32 {
            return Err(BYOKError::EnvelopeError(format!(
                "DEK length {} != 32 expected",
                plaintext.len()
            )));
        }

        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&plaintext);

        Ok(Dek { bytes })
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        let resp = self.client.describe_key()
            .key_id(&key_id.key_arn_or_id)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let key_state = r.key_metadata()
                    .and_then(|m| m.key_state())
                    .map(|s| s.as_str())
                    .unwrap_or("unknown");
                
                match key_state {
                    "Enabled" => Ok(KmsAccessStatus::Ok),
                    "PendingDeletion" | "Disabled" => Ok(KmsAccessStatus::Revoked),
                    _ => Ok(KmsAccessStatus::ApiError(0)),
                }
            }
            Err(e) => {
                let s = e.to_string();
                if s.contains("AccessDenied") {
                    Ok(KmsAccessStatus::Revoked)
                } else if s.contains("NotFound") {
                    Ok(KmsAccessStatus::NotFound)
                } else {
                    Err(BYOKError::Provider(s))
                }
            }
        }
    }
}
```

State machine envelope encryption:
```
Write path:
  1. Generate ephemeral DEK random 32 bytes via CSPRNG (`getrandom::getrandom(&mut [0u8; 32])`; OS-backed entropy NIST SP 800-90A approved DRBG; NOT KDF-derived deterministic — Lote 10.14 codex P1 canonical fix; deterministic DEK = compromise propagation; re-wrap durante CMK rotation NÃO precisa deterministic — é unwrap+rewrap atomic preserving DEK identity)
  2. Encrypt body via AES-256-GCM with DEK; nonce per-write 96-bit random
  3. Wrap DEK via KmsProvider::wrap_dek(dek)
  4. Store: D1 row (tenant_id, blob_hash, wrapped_dek, kms_provider, kms_key_id, created_at) + R2 body ciphertext

Read path:
  1. Fetch D1 row (wrapped_dek, kms_provider, kms_key_id)
  2. Check DEK cache (5min TTL HARD); if hit → step 4
  3. If miss → KmsProvider::unwrap_dek(wrapped_dek) network call ≤ 30ms p99 → cache DEK
  4. Decrypt body via AES-256-GCM with DEK; verify nonce + tag
  5. Return plaintext

Kill switch path (composed with WI-S14-006):
  1. KMS access check background 60s detects revoked
  2. Emit audit corelink.byok.cmk_revoked
  3. Mark tenant degraded read-only
  4. Alert customer
  5. DEK cache atomic evict_all_for_key (subscribe-pub)
  6. DEK cache TTL 5 min hard expires all in-flight reads
  7. Total p99 ≤ 5 min global
  8. Hard-fail (no operator override; no advisory mode)
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

BYOK enterprise tier é **maior salto de superfície cripto-load-bearing** em CoreLink S-14: introduz controles cripto customer-controlled (CMK + envelope encryption + DEK cache + kill switch) substituindo baseline TDK CoreLink-managed (S-01..S-13). Bug em qualquer um desses paths = catastrophic — wrong CMK = data inaccessible permanente; cache TTL > 5min = customer kill switch ineffective (INV-BYOK-CRYPTO-SOVEREIGNTY violated; customer trust permanently lost); envelope encryption bug = data corruption + Schrems II breach.

**Bugs catastróficos possíveis** (todos endereçados):

1. **DEK cache TTL > 5 min (INV-BYOK-CRYPTO-SOVEREIGNTY violation)**: cache TTL 10 min "for performance" = customer kill switch SLA violated. Mitigação: `DekCache::new(ttl_seconds)` rejects > 300s with `BYOKError::DekCacheTtlViolation`; constructor enforced; CI gate static check em construtor; INV-BYOK-CRYPTO-SOVEREIGNTY runtime invariant.

2. **AES-256-GCM nonce reuse**: same nonce + same key = catastrophic crypto break. Mitigação: nonce per-write 96-bit random (cryptographically secure RNG); property test 1M nonces uniqueness; constant-time comparison.

3. **DEK ephemeral leak**: DEK em logs / traces / errors. Mitigação: `Dek` struct ZeroizeOnDrop + Zeroize traits; no Display/Debug/Serialize impls; redaction macros herdada S-03; CI grep gate.

4. **Wrong CMK used (cross-tenant or stale)**: tenant T1 blob encrypted with tenant T2 CMK = data inaccessible; cross-tenant leak via wrong unwrap. Mitigação: D1 row stores `kms_key_id` per-blob; unwrap uses stored kms_key_id; per-tenant scoping enforced; integration test cross-tenant attempt.

5. **AAD (encryption_context) skipped**: AWS KMS encryption_context = AAD; if not bound, attacker can swap wrapped DEKs cross-blob. Mitigação: encryption_context = `{tenant_id, blob_hash}` mandatory; mismatch on unwrap = BYOKError; property test 10k swap attempts.

6. **DEK cache poisoning**: attacker injects fake DEK in cache. Mitigação: cache key derived from wrapped DEK hash; insertion only via unwrap result; integrity verified.

7. **FIPS compliance drift**: AWS KMS provider downgraded silently from FIPS 140-3 L1 to non-FIPS. Mitigação: `fips_level()` returns const value; quarterly review com Crypto SME; alert if NIST CMVP module status changes (CMVP API integration future S-19).

8. **KMS API outage propagates**: AWS KMS down → all reads fail. Mitigação: DEK cache absorbs outage (5 min); degrade-mode read-only se sustained > 5 min; chaos test KMS outage scenarios.

9. **Wrap/unwrap latency exceeds 30ms p99**: cross-region KMS call. Mitigação: KMS region-co-located com R2/D1 (same region); SLO ≤ 30ms p99; alert if > 30ms sustained.

10. **Matrix test break em PR**: drift entre AWS/GCP/Azure/Vault adapters. Mitigação: 16-combination matrix test em PR; auto-fail if any cell breaks.

**Atacante adversarial scenarios** (todos validados em §15):

- **AAD bypass**: attacker swaps wrapped DEKs cross-blob; unwrap with mismatched AAD → reject. Mitigação: encryption_context bound `{tenant_id, blob_hash}`.

- **DEK cache extraction**: attacker compromises Worker memory; reads DEK cache. Mitigação: ZeroizeOnDrop on cache eviction; bounded cache size; LRU eviction.

- **Replay wrap → unwrap**: attacker replays wrap call; trying to extract DEK. Mitigação: AWS KMS rate limit + audit emit + customer alert.

- **Force AES-GCM nonce reuse**: attacker manipulates RNG. Mitigação: cryptographically secure RNG (getrandom); property test 1M nonces.

- **Side channel timing on unwrap**: attacker times unwrap to infer DEK. Mitigação: AWS KMS server-side; constant-time decrypt; AES hardware acceleration.

- **CMK substitution attack**: attacker substitutes CMK; new wrap cannot unwrap old wrapped DEKs. Mitigação: per-blob D1 stores kms_key_id; wrong CMK = unwrap fails; audit emit.

- **Compromise AWS KMS API token**: attacker uses CoreLink AWS IAM token to wrap own DEK; sign as CoreLink. Mitigação: AWS IAM role minimal scope (Encrypt + Decrypt + DescribeKey only); audit AWS CloudTrail; rotation per S-13.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: BYOK introduz CTRL-CRYPTO-005 + CTRL-KEY-010..015 (customer-controlled crypto).
- **FF-HR-008**: vendor lock-in via multi-cloud KMS; matrix testing required.
- **Reversibility**: cripto bug = data corrupted permanente; cache TTL violation = customer kill switch ineffective forever; FIPS compliance drift = SOC 2 audit fail.

11 sign-offs canonical incl. Architect (com **Crypto SME specialization MANDATORY**: cripto algorithm review per provider + envelope encryption flow + AES-256-GCM nonce strategy + DEK cache TTL hard limit + AAD binding + matrix test design + FIPS compliance attestation) + Security Lead + AppSec + Compliance Officer (NIST SP 800-57 Pt 1 Rev 5 + NIST SP 800-130 + FIPS 140-3 attestation).

## 3. Customer Impact & Journey

**Persona 1 — Customer with BYOK signup (FedRAMP / EU / financial services)**:
- Customer provisions CMK em AWS KMS account; provides CMK ARN to CoreLink.
- Per-blob: CoreLink wraps DEK via customer CMK; only customer CMK can unwrap.
- Customer can revoke CMK access anytime (kill switch WI-S14-006).
- Evidence: `corelink_byok_wrap_dek_duration_seconds_bucket{provider="aws_kms"}` p99 ≤ 30ms; FIPS 140-3 L1 documented.

**Persona 2 — SecOps lead em prospect enterprise (RFP)**:
- RFP question: "BYOK supported? FIPS compliance? Kill switch SLA?".
- Evidence: `compliance/byok-fips-matrix.md` per-provider; INV-BYOK-CRYPTO-SOVEREIGNTY enforced; matrix test 16 combinations weekly.
- Diferenciador: AWS S3+KMS / GCS / Azure offer BYOK; CoreLink ALL 4 providers + TLA+ + erasure attestation Ed25519.

**Persona 3 — Auditor SOC 2 + ISO 27001 + NIST SP 800-57 + FIPS 140-3**:
- CTRL-CRYPTO-005 + CTRL-KEY-010..015 attestation dossier.
- Evidence pack: NIST SP 800-57 Pt 1 Rev 5 §5 (key management lifecycle) + NIST SP 800-130 (cryptographic key management framework) + FIPS 140-3 (cryptographic modules) + FIPS 197 (AES) + FIPS 186-5 (Ed25519).
- Property test 10k matrix combinations green.

**SLA addendum**:
- BYOK latency overhead: p99 < 30ms (AWS KMS region-co-located baseline).
- DEK cache TTL: 5 min hard limit (no exception).
- DEK cache eviction: atomic on revoke (subscribe-pub).
- Wrap/unwrap idempotent (replay-safe via KMS native).
- 16-combination matrix test verde em staging weekly.
- FIPS 140-3 / 140-2 documented per provider; quarterly review.

## 4. Capability Mapping

- **CAP-BYOK-001** (BYOK AWS KMS integration first-class) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.2 R-S14-6 + R-S14-7 + R-S14-11` + `security_model.md §6 (CTRL-CRYPTO-005, CTRL-KEY-010..015)` + `key_management.md §3.2.1 (overlap canonical BYOK CMK 7d)` + `compliance_matrix.md (NIST SP 800-57 + FIPS 140-3)` + `invariant_registry.md §3.12 (INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL nova)`.

## 5. Tipo

Crypto trait + KMS adapter + envelope encryption + matrix test framework; HIGH_RISK; FF-HR-005 + FF-HR-008.

## 6. Escopo

### 6.1 In-scope

1. **Crate `crates/corelink-byok/`** (trait + types + DekCache):
   - `KmsProvider` trait (4 methods: wrap_dek, unwrap_dek, check_access, fips_level).
   - `KmsProviderKind`, `KmsKeyId`, `WrappedDek`, `Dek`, `KmsAccessStatus`, `BYOKError` types.
   - `DekCache` struct with 5 min TTL hard limit constructor enforcement.
   - `EnvelopeEncryptor` struct: write/read path encapsulation.

2. **Crate `crates/corelink-byok-aws/`** (AWS KMS adapter first-class):
   - `AwsKmsProvider` impl `KmsProvider`.
   - AWS SDK aws-sdk-kms (Rust SDK).
   - IAM role minimal scope (Encrypt + Decrypt + DescribeKey only).
   - Region-co-located com CoreLink R2/D1 (same region).
   - FIPS 140-3 L1 verified (AWS KMS default behavior; documented em compliance matrix).

3. **Envelope encryption flow**:
   - Write: gen ephemeral DEK random 32 bytes via CSPRNG (`getrandom::getrandom`; OS entropy NIST SP 800-90A approved DRBG; NÃO KDF-derived deterministic per Lote 10.14 codex P1 canonical fix); encrypt body AES-256-GCM (FIPS 197 + FIPS 140-3 approved) nonce per-write 96-bit random; wrap DEK via KMS; store wrapped DEK em D1 + body em R2.
   - Read: fetch wrapped DEK; check DEK cache (5 min TTL hard); if miss → unwrap via KMS network call ≤ 30ms p99 → cache; decrypt body.
   - AAD `encryption_context`: bound `{tenant_id, blob_hash}` mandatory; mismatch on unwrap = BYOKError.

4. **D1 schema migration `migrations/0XX_byok_envelope.sql`**:
   ```sql
   CREATE TABLE byok_envelope (
       tenant_id TEXT NOT NULL,
       blob_hash TEXT NOT NULL,
       wrapped_dek BLOB NOT NULL,
       kms_provider TEXT NOT NULL CHECK (kms_provider IN ('aws_kms', 'gcp_kms', 'azure_key_vault', 'hashicorp_vault')),
       kms_key_id TEXT NOT NULL,
       kms_region TEXT NOT NULL,
       encryption_context TEXT,  -- JSON-encoded
       aes_gcm_nonce BLOB NOT NULL,
       created_at_ms BIGINT NOT NULL,
       PRIMARY KEY (tenant_id, blob_hash)
   );
   CREATE INDEX idx_byok_envelope_kms_key_id ON byok_envelope(kms_provider, kms_key_id);
   ```

5. **DEK cache `DekCache`**:
   - 5 min TTL hard limit (INV-BYOK-CRYPTO-SOVEREIGNTY).
   - Bounded LRU eviction (max 10k entries per Worker).
   - `evict_all_for_key(key_id)` atomic on revoke event.
   - ZeroizeOnDrop on eviction.

6. **16-combination matrix test framework `tests/byok_matrix_framework.rs`**:
   - 4 providers × 4 ops {write, read, wrap, unwrap} = 16 combinations.
   - Framework allows adding new providers (WI-S14-005 GCP/Azure/Vault).
   - Auto-fail PR se matrix break.

7. **FIPS doc `compliance/byok-fips-matrix.md`** (this WI scoped to AWS KMS row):
   - AWS KMS: FIPS 140-3 Level 1 (default; documented).
   - GCP KMS: FIPS 140-2 (3 quando suportado) — WI-S14-005.
   - Azure: FIPS 140-2 Level 2 (Premium HSM) — WI-S14-005.
   - Vault: FIPS 140-3 Level 1 (Vault Enterprise) — WI-S14-005.

8. **Métricas underscored Prometheus** (per `observability_model.md §3.1`; label `plan` aplicável; **NUNCA per-tenant labels**):
   - `corelink_byok_wrap_dek_duration_seconds_bucket{provider, plan}` (histogram p99 ≤ 30ms target).
   - `corelink_byok_unwrap_dek_duration_seconds_bucket{provider, plan}` (histogram p99 ≤ 30ms target).
   - `corelink_byok_dek_cache_hit_total{provider, plan}` (counter).
   - `corelink_byok_dek_cache_miss_total{provider, plan}` (counter).
   - `corelink_byok_dek_cache_evict_total{provider, reason, plan}` (reason ∈ ttl_expired|cmk_revoked|capacity).
   - `corelink_byok_envelope_encrypt_total{provider, outcome, plan}` (outcome ∈ ok|aad_mismatch|aes_error|kms_error).
   - `corelink_byok_matrix_test_total{provider, op, outcome}` (op ∈ write|read|wrap|unwrap; outcome ∈ ok|fail).

9. **Observability** — trace spans `byok.{wrap, unwrap, encrypt, decrypt, cache_hit, cache_miss, cache_evict}` com attributes:
   - `byok.provider` (enum).
   - `byok.region`.
   - `byok.kms_key_id_hashed` (hashed for cardinality).
   - `byok.tenant_id_hashed`.
   - `byok.blob_hash`.
   - `byok.aad` (for debug; no PII).
   - `result` (enum).

10. **Audit emission** — CloudEvent per envelope op:
    - `corelink.byok.envelope.encrypt.{ok, fail}`.
    - `corelink.byok.envelope.decrypt.{ok, fail}`.
    - `corelink.byok.kms.wrap.{ok, fail}`.
    - `corelink.byok.kms.unwrap.{ok, fail}`.
    - `corelink.byok.dek_cache.evict.{ttl, revoke, capacity}`.
    - Atomic batch with envelope op (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER herdada).

11. **Property tests** (10k iter PR + 100k iter nightly; comprehensive crypto):
    - `prop_aes_gcm_nonce_unique`: 1M random nonces; assert uniqueness.
    - `prop_dek_blake3_deterministic`: same input {tenant_id, blob_hash, salt} → same DEK.
    - `prop_aad_binding`: 10k swap attempts wrapped DEKs across blobs; assert unwrap rejects.
    - `prop_dek_cache_ttl_5min_hard`: assert constructor rejects ttl > 300s.
    - `prop_dek_cache_eviction_atomic`: revoke event → cache evicted; verify atomic.
    - `prop_wrap_unwrap_roundtrip`: 10k random DEKs; wrap → unwrap → assert equal.
    - `prop_zeroize_on_drop`: DEK dropped; assert memory zeroized (via mock allocator).

12. **Adversarial regression tests**:
    - **AAD bypass** (CVE-class): swap wrapped DEKs cross-blob; verify unwrap rejects.
    - **DEK cache extraction** (memory dump): assert ZeroizeOnDrop; bounded cache.
    - **Replay wrap → unwrap**: AWS KMS rate limit + audit emit.
    - **AES-GCM nonce reuse force**: simulate RNG compromise; property test detects.
    - **Side channel timing**: AES hardware accel + constant-time decrypt.
    - **CMK substitution**: per-blob kms_key_id stored; wrong CMK = unwrap fails.
    - **AWS IAM token compromise**: scope minimal; CloudTrail audit; rotation S-13.

13. **Integration test E2E** (real AWS KMS staging):
    - Customer provisions test CMK em AWS staging.
    - Customer tenant signs up + provides CMK ARN.
    - CoreLink writes blob: gen DEK → encrypt body → wrap DEK → D1 + R2.
    - CoreLink reads blob: fetch wrapped → unwrap → decrypt body → return.
    - Assert latency p99 ≤ 30ms (region-co-located).
    - Assert correctness (byte-equal plaintext).

14. **FIPS attestation** `compliance/byok-fips-matrix.md`:
    - AWS KMS FIPS 140-3 Level 1 (default behavior).
    - Reference: AWS KMS FIPS 140-3 documentation + NIST CMVP module ID.
    - Quarterly review com Crypto SME (alert if status changes).

### 6.2 Out-of-scope (deferred)

- **GCP/Azure/Vault adapters**: WI-S14-005.
- **CMK kill switch + chaos drill**: WI-S14-006.
- **Erasure attestation Ed25519**: WI-S14-007.
- **DPA + Schrems II TIA**: WI-S14-008.
- **TLA+ + pentest + PRR**: WI-S14-009.
- **BYOE (Bring Your Own Encryption)**: Fase 2.
- **Customer-managed HSM on-prem**: only cloud KMS at GA.
- **Quantum-resistant crypto**: Fase 3.
- **Federated KMS multi-cloud failover**: single CMK per tenant at GA.
- **HSM-backed DEK cache**: software cache only at GA.
- **Customer-controlled DEK rotation**: per-tenant CMK rotation S-13 herdada (BYOK CMK 7d overlap).

## 7. Anti-Scope

- DEK cache TTL > 5 min (INV-BYOK-CRYPTO-SOVEREIGNTY violation).
- Skip AES-256-GCM nonce per-write 96-bit random.
- Skip ZeroizeOnDrop em DEK + DEK cache.
- Skip AAD encryption_context binding.
- Skip per-blob kms_key_id storage em D1.
- Skip 16-combination matrix test framework.
- Skip FIPS doc per provider.
- Skip property tests crypto-load-bearing 10k+ iter.
- Skip adversarial regression tests CVE-class (AAD bypass + nonce reuse + side channel).
- AWS IAM role overscope.
- DEK em logs / traces / errors.
- Plaintext CMK em CoreLink (only wrapped DEK + cached DEK).
- Direct DEK write to D1 (only wrapped DEK).
- Skip integration test E2E real AWS KMS staging.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-004 — BYOK trait + AWS KMS adapter + envelope encryption + matrix framework

  Background:
    Given KmsProvider trait operational
    Given AwsKmsProvider impl with FIPS 140-3 L1
    Given DekCache 5 min TTL hard limit
    Given D1 byok_envelope table created

  Scenario: Envelope encryption write path
    Given customer T1 with CMK ARN aws::kms::us-east-1::key/abc
    When CoreLink writes blob B1
    Then ephemeral DEK generated (BLAKE3 256-bit deterministic via KDF input {T1, B1.hash, salt})
    And body encrypted via AES-256-GCM with DEK; nonce 96-bit random
    And DEK wrapped via AWS KMS (FIPS 140-3 L1)
    And D1 byok_envelope row stored (wrapped_dek, kms_provider, kms_key_id, encryption_context)
    And R2 body ciphertext stored
    And métrica wrap_dek_duration_seconds < 30ms p99

  Scenario: Envelope encryption read path
    Given D1 byok_envelope row + R2 body for blob B1
    When CoreLink reads blob B1
    Then wrapped DEK fetched from D1
    And DEK cache check (5 min TTL hard); if hit → step 5
    And if miss → unwrap via AWS KMS ≤ 30ms p99 → cache 5 min hard
    And body decrypted via AES-256-GCM with DEK; nonce + tag verified
    And plaintext returned
    And métrica unwrap_dek_duration_seconds < 30ms p99

  Scenario: DEK cache TTL 5 min hard limit (INV-BYOK-CRYPTO-SOVEREIGNTY)
    Given DekCache::new(ttl_seconds = 600) attempt
    When constructor runs
    Then BYOKError::DekCacheTtlViolation returned
    And no cache instance created
    And INV-BYOK-CRYPTO-SOVEREIGNTY preserved

  Scenario: DEK cache atomic eviction on revoke
    Given DEK cache populated with 100 entries for kms_key_id K1
    When evict_all_for_key(K1) called (subscribe-pub from KMS access check)
    Then 100 entries evicted atomic
    And ZeroizeOnDrop called on each DEK
    And métrica dek_cache_evict_total{reason="cmk_revoked"} +100

  Scenario: AAD binding mandatory
    Given encryption_context = {"tenant_id": "T1", "blob_hash": "H1"}
    When wrap with AAD; then unwrap with mismatched AAD {"tenant_id": "T2", "blob_hash": "H1"}
    Then BYOKError::EnvelopeError returned (AAD mismatch)
    And no DEK extracted

  Scenario: Wrong CMK substitution
    Given blob B1 wrapped with CMK K1
    When unwrap with CMK K2 (substituted)
    Then KMS provider returns AccessDenied or CryptoFailure
    And BYOKError::Provider returned
    And audit emit corelink.byok.kms.unwrap.fail

  Scenario: AES-GCM nonce uniqueness
    Given 1M random nonces generated
    When property test asserts uniqueness
    Then 0 collisions detected
    And cryptographically secure RNG verified

  Scenario: DEK ZeroizeOnDrop
    Given Dek struct with bytes [u8; 32]
    When Dek dropped
    Then memory zeroized via Zeroize trait
    And no DEK material remains in memory

  Scenario: 16-combination matrix test framework
    Given 4 providers (only AWS KMS impl em this WI; 3 stubs for WI-005)
    Given 4 ops {write, read, wrap, unwrap}
    When matrix framework runs
    Then 4 cells (AWS × 4 ops) green
    And 12 cells stub-pending (GCP/Azure/Vault × 4 ops em WI-S14-005)
    And framework allows adding providers

  Scenario: FIPS 140-3 L1 AWS KMS attestation
    Given AwsKmsProvider::fips_level()
    When called
    Then FipsLevel::Fips140_3_L1 returned
    And documented em compliance/byok-fips-matrix.md
    And NIST CMVP module ID referenced

  Scenario: AWS IAM minimal scope
    Given AWS IAM role for CoreLink
    When IAM policy review
    Then permissions = ["kms:Encrypt", "kms:Decrypt", "kms:DescribeKey"]
    And no broader permissions
    And CloudTrail audit ativo

  Scenario: Property test 7 props × 10k iter green
    Given prop_aes_gcm_nonce_unique + prop_dek_blake3_deterministic + prop_aad_binding + prop_dek_cache_ttl_5min_hard + prop_dek_cache_eviction_atomic + prop_wrap_unwrap_roundtrip + prop_zeroize_on_drop
    When 10k iter run em PR
    Then 0 violations
    And nightly 100k iter green

  Scenario: Adversarial regression tests 7 scenarios green
    Given AAD bypass + DEK cache extraction + replay + nonce reuse + side channel + CMK substitution + IAM token compromise
    When red team session
    Then 7/7 scenarios mitigated
    And report committed em audit folder

  Scenario: Integration test E2E AWS KMS staging
    Given real AWS KMS staging account + test CMK
    When CoreLink writes + reads blob com BYOK
    Then plaintext byte-equal post-roundtrip
    And latency p99 ≤ 30ms (region-co-located)
    And FIPS 140-3 L1 attestation
```

## 9. Design Decisions

### 9.1 Why DEK ephemeral RANDOM via CSPRNG (Lote 10.14 codex P1 canonical fix; revert prévio "BLAKE3-derived deterministic")

- **DEK = `getrandom::getrandom(&mut [0u8; 32])` (CSPRNG; OS-backed entropy)**, NOT BLAKE3-derived from `{tenant_id, blob_hash, salt}`.
- **Why random (NÃO deterministic)**: deterministic DEK = catastrophic crypto failure — same `{tenant_id, blob_hash, salt}` → same DEK → compromise of one DEK = compromise of all blobs with same hash. Random per-write DEK preserves envelope encryption security boundary (NIST SP 800-57 Pt 1 Rev 5 §5.6.2).
- **Re-wrap (CMK rotation) does NOT require deterministic DEK**: re-wrap flow é simples — `new_cmk.wrap(old_cmk.unwrap(stored_wrapped_dek))` — DEK identity preserved through rotation; body NÃO re-encrypted; wrapped DEK em D1 atualizado atomicamente.
- **BLAKE3 NÃO é FIPS 197-equivalent**: AES é FIPS 197 (block cipher); BLAKE3 é a hash function (no FIPS classification — RFC draft only). Linguagem "FIPS 197-equivalent" prévia foi misuse — corrigida Lote 10.14 codex P1. AES-256-GCM (body encryption) é FIPS 197 + FIPS 140-3 approved; DEK CSPRNG follows NIST SP 800-90A approved DRBG (OS getrandom backed by AES-CTR-DRBG ou Hash-DRBG).
- **Original "reproducibility-as-needed" justification revoked**: re-wrap doesn't need DEK reproducibility; re-wrap is unwrap+rewrap atomic operation.

### 9.2 Why DEK cache TTL 5 min hard limit (NÃO 10 min "for performance")

- INV-BYOK-CRYPTO-SOVEREIGNTY enforces customer kill switch p99 ≤ 6 min global (Lote 10.14 codex P0 disambiguation: 60s detection + 5min DEK cache TTL hard).
- TTL > 5 min = kill switch SLA breached (cached DEK serves reads beyond contract).
- Customer trust permanently lost if cache TTL extended.
- Constructor enforced (`DekCache::new(ttl_seconds)` rejects > 300s).
- **NO exception, NO advisory mode** (Lote 10.14 codex P0 fix: spec contract §19 waiver row prévio "DEK TTL 5min → 10min com explicit risk acceptance" REMOVIDO — TTL é componente non-waivable do kill switch SLA contract; cannot be extended).

### 9.3 Why AES-256-GCM nonce per-write 96-bit random (NÃO counter)

- AES-GCM nonce reuse with same key = catastrophic (NIST FIPS 197 + SP 800-38D).
- 96-bit random nonce = 2^48 writes before collision birthday ~2.8E14 (acceptable).
- Counter-based requires synchronization; complex em multi-tenant.
- Property test 1M nonces uniqueness verifies.

### 9.4 Why AAD encryption_context binding mandatory

- AWS KMS encryption_context = AAD; if not bound, attacker can swap wrapped DEKs cross-blob.
- AAD `{tenant_id, blob_hash}` = identity binding; unwrap fails on mismatch.
- Standard AWS KMS pattern; aligned with NIST SP 800-130 §6.2.

### 9.5 Why per-blob kms_key_id storage em D1

- Per-tenant CMK = single key for all tenant blobs; rotation rare.
- Per-blob kms_key_id stored em D1 enables future per-blob CMK (BYOE Fase 2) + audit trail.
- Slight overhead (TEXT column) acceptable.

### 9.6 Why 16-combination matrix test framework

- 4 providers × 4 ops = 16 cells; full coverage.
- Auto-fail PR if matrix break = drift detected early.
- Framework allows future providers (BYOE Fase 2) without re-architecting.

### 9.7 Why FIPS 140-3 L1 (NÃO L2/L3)

- AWS KMS default = FIPS 140-3 L1; no L2/L3 available em AWS at this time.
- Customer demand for L2/L3 = HSM dedicated tenant (pós-GA).
- Documented em compliance matrix; quarterly review com Crypto SME.

### 9.8 Why AWS IAM minimal scope

- AWS IAM role permissions = ["kms:Encrypt", "kms:Decrypt", "kms:DescribeKey"] only.
- No broader permissions (no kms:CreateKey, kms:ScheduleKeyDeletion, etc.).
- CloudTrail audit ativo; rotation per S-13.

### 9.9 Why ZeroizeOnDrop em DEK + cache

- Memory dump attack potential; ZeroizeOnDrop clears on drop.
- Zeroize trait + ZeroizeOnDrop derive standard pattern.
- CI grep gate ensures all DEK-handling structs derive these.

### 9.10 Why ADR potencial?

- Sim — **ADR-XXXX**: "BYOK adapter trait + envelope encryption flow + DEK cache 5 min hard limit + AAD binding + 16-combination matrix test S-14". Decisão arquitetural cripto-load-bearing; reuse pattern em BYOE Fase 2.

## 10. Completeness Criteria SOTA

- [ ] **10.s14.004.1** Crate `corelink-byok` + trait + types + DekCache compilam (EVT-013).
- [ ] **10.s14.004.2** Crate `corelink-byok-aws` + AwsKmsProvider impl + AWS SDK integration (EVT-013).
- [ ] **10.s14.004.3** Envelope encryption flow operational (write + read paths) (EVT-024).
- [ ] **10.s14.004.4** DEK cache 5 min TTL hard limit constructor enforcement; INV-BYOK-CRYPTO-SOVEREIGNTY ratificada (EVT-022).
- [ ] **10.s14.004.5** D1 byok_envelope migration applied; per-blob kms_key_id stored (EVT-013).
- [ ] **10.s14.004.6** AAD encryption_context binding mandatory; mismatch rejects (EVT-002).
- [ ] **10.s14.004.7** 16-combination matrix test framework operational (4 cells AWS green; 12 stubs WI-005) (EVT-002).
- [ ] **10.s14.004.8** FIPS 140-3 L1 AWS KMS documented em `compliance/byok-fips-matrix.md` (EVT-044).
- [ ] **10.s14.004.9** Property tests 7 props × 10k iter PR + 100k nightly green (EVT-022).
- [ ] **10.s14.004.10** Adversarial regression tests 7 CVE-class scenarios green (EVT-040).
- [ ] **10.s14.004.11** Integration test E2E real AWS KMS staging green (EVT-024).
- [ ] **10.s14.004.12** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) (EVT-002).
- [ ] **10.s14.004.13** Cost regression gate: BYOK overhead < 15% em CAS path (EVT-002).
- [ ] **10.s14.004.14** OWASP ASVS V6 (cripto) + V7 (error/logging) 100% checklist pass.
- [ ] **10.s14.004.15** NIST SP 800-57 Pt 1 Rev 5 + NIST SP 800-130 + FIPS 140-3 attestation em PRR doc (EVT-031).
- [ ] **10.s14.004.16** ZeroizeOnDrop em DEK + cache verified (memory dump test).

## 11. DoD

- [ ] Crates `corelink-byok` + `corelink-byok-aws` compilam.
- [ ] Trait + types + AwsKmsProvider impl complete.
- [ ] DEK cache 5 min hard limit enforced.
- [ ] Envelope encryption write + read paths operational.
- [ ] D1 migration applied.
- [ ] All 13 Gherkin scenarios green em integration test.
- [ ] Property tests 7 props × 10k iter green; 100k nightly green.
- [ ] Adversarial regression tests 7 CVE-class scenarios green.
- [ ] Integration test E2E real AWS KMS staging green.
- [ ] 16-combination matrix framework operational.
- [ ] FIPS 140-3 L1 documented em compliance matrix.
- [ ] CloudEvent audit emission per envelope op.
- [ ] Métricas + trace spans operational.
- [ ] rustdoc + 4 examples (write, read, wrap, unwrap).
- [ ] ADR-XXXX (BYOK trait + envelope encryption) escrito + ratificado.
- [ ] Code review (Architect + Crypto SME folded mandatory + Security Lead + AppSec + Compliance Officer).
- [ ] PRR Architect + Crypto SME mini-sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-KEY-OVERLAP** (HIGH — registry §3.13 herdada): BYOK CMK 7d overlap canonical (key_management.md §3.2.1); rotation worker S-13 owns customer-trigger.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada S-03): envelope op audit emit em D1 atomic batch.

### Novas

- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — registry §3.12 NEW): este WI ratifies primary; DEK cache 5 min TTL hard limit enforced via constructor; KMS access check 60s (WI-S14-006); customer kill switch ≤ 5 min global. Why: sem isso BYOK = teatro; customer não tem real control. How to apply: DEK cache TTL 5 min hard + KMS access check 60s.

TLA+ alignment: registry §4.2 indica `key_lifecycle.tla` PLANNED S-13 (BYOK rotation) + `region_residency.tla` PLANNED S-14 WI-S14-009; este WI provê BYOK adapter implementation.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate corelink-byok (trait + types) | `crates/corelink-byok/src/lib.rs + types.rs + dek_cache.rs + envelope.rs` | Rust |
| Crate corelink-byok-aws | `crates/corelink-byok-aws/src/lib.rs` | Rust |
| D1 migration byok_envelope | `migrations/0XX_byok_envelope.sql` | SQL |
| Matrix test framework | `tests/byok_matrix_framework.rs` | Rust |
| Property tests | `crates/corelink-byok/tests/prop_byok.rs` | Rust |
| Adversarial regression tests | `crates/corelink-byok/tests/adversarial.rs` | Rust |
| Integration test E2E AWS KMS | `tests/e2e_byok_aws_kms.rs` | Rust |
| FIPS doc | `compliance/byok-fips-matrix.md` (AWS row) | Markdown |
| Examples | `crates/corelink-byok/examples/` (write_aws.rs, read_aws.rs, wrap_dek.rs, unwrap_dek.rs) | Rust |
| ADR-XXXX (BYOK trait + envelope) | `specs/03_architecture/adrs/ADR-XXXX-byok-trait-envelope-encryption.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s14.004.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s14.004.2** rustdoc 100% public API + 4 examples.
- **14.s14.004.3** Test coverage ≥ 95% (`cargo tarpaulin`); property tests 10k+100k.
- **14.s14.004.4** Latência: wrap/unwrap p99 ≤ 30ms region-co-located.
- **14.s14.004.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s14.004.6** Métricas RED + per-provider breakdown.
- **14.s14.004.7** ZeroizeOnDrop verified (memory dump test).
- **14.s14.004.8** Breaking changes em `KmsProvider` trait = bump major + ADR + migration plan.
- **14.s14.004.9** Memory bounded; LRU eviction; cache ≤ 10k entries per Worker.
- **14.s14.004.10** Cost regression gate em CI (BYOK overhead < 15%).
- **14.s14.004.11** NIST SP 800-57 Pt 1 Rev 5 + NIST SP 800-130 + FIPS 140-3 attestation.
- **14.s14.004.12** Constant-time compare em DEK cache key (subtle::ConstantTimeEq).
- **14.s14.004.13** AES hardware acceleration verified (AES-NI).

## 15. Chaos Experiments

1. **AAD bypass red team**: 1000 swap attempts wrapped DEKs cross-blob; verify 100% rejected.

2. **DEK cache extraction**: memory dump test; verify ZeroizeOnDrop; bounded cache.

3. **Replay wrap → unwrap**: 1000 replay attempts; AWS KMS rate limit + audit emit.

4. **AES-GCM nonce reuse force**: simulate RNG compromise; property test detects.

5. **Side channel timing**: AES-NI hardware accel + constant-time decrypt; assert no timing leak.

6. **CMK substitution**: substitute CMK; verify per-blob kms_key_id stored prevents wrong unwrap.

7. **AWS IAM token compromise**: scope minimal; CloudTrail audit; rotation S-13.

8. **DEK cache TTL 5 min hard limit attempt bypass**: try `DekCache::new(600)`; verify constructor rejects.

9. **Cache eviction race**: 1000 concurrent unwrap during eviction; verify atomic.

10. **AWS KMS API outage**: simulate 503 sustained; DEK cache absorbs 5 min; degrade-mode read-only sustained.

11. **Wrap/unwrap latency stress**: 10k concurrent ops; assert p99 ≤ 30ms.

12. **FIPS compliance drift**: simulate NIST CMVP module status change; assert alert.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Architect + Crypto SME mandatory + Security Lead + AppSec + Compliance Officer review):

- [ ] All 13 Gherkin scenarios green.
- [ ] Property tests 7 props × 10k iter PR + 100k nightly green.
- [ ] Adversarial regression tests 7 CVE-class scenarios green.
- [ ] Integration test E2E real AWS KMS staging green.
- [ ] 16-combination matrix framework operational (4 cells AWS green).
- [ ] FIPS 140-3 L1 documented em compliance matrix.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-BYOK.
- [ ] ADR-XXXX (BYOK trait + envelope) published.
- [ ] Crypto SME review (cripto algorithm review per provider + envelope encryption + AES-256-GCM nonce + DEK cache TTL hard + AAD binding + matrix design + FIPS attestation).
- [ ] Compliance Officer review (NIST SP 800-57 + NIST SP 800-130 + FIPS 140-3).
- [ ] AppSec advisor review (adversarial scenarios + AWS IAM scope + DEK cache extraction).
- [ ] Architect approval (composition with WI-S14-001..003).
- [ ] OWASP ASVS V6 + V7 100% pass.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate corelink-byok scaffold + trait + types | 2.5h |
| ST-002 | DekCache impl + 5 min TTL hard limit constructor | 2h |
| ST-003 | EnvelopeEncryptor write path (gen DEK + AES-256-GCM + wrap) | 3h |
| ST-004 | EnvelopeEncryptor read path (fetch + cache + unwrap + decrypt) | 3h |
| ST-005 | Crate corelink-byok-aws + AWS SDK integration | 3h |
| ST-006 | AwsKmsProvider impl wrap_dek + unwrap_dek + check_access | 3h |
| ST-007 | D1 migration byok_envelope + indexes | 1h |
| ST-008 | AAD encryption_context binding mandatory + tests | 2h |
| ST-009 | 16-combination matrix test framework + 4 AWS cells | 3h |
| ST-010 | FIPS 140-3 L1 doc em compliance/byok-fips-matrix.md | 1.5h |
| ST-011 | Métricas emit (7 metrics) + trace spans + audit emission | 2.5h |
| ST-012 | Property tests 7 props × 10k iter | 4h |
| ST-013 | Adversarial regression tests 7 CVE-class scenarios | 3h |
| ST-014 | Integration test E2E real AWS KMS staging | 3h |
| ST-015 | ZeroizeOnDrop verification (memory dump test) | 1.5h |
| ST-016 | rustdoc + 4 examples | 2h |
| ST-017 | AWS IAM policy minimal + CloudTrail review | 1.5h |
| ST-018 | ADR-XXXX redação | 2.5h |
| ST-019 | Code review (Architect + Crypto SME folded + Security Lead + AppSec + Compliance) | 4h |

**Total Optimistic**: ~48h. **PERT** (O=18h, M=28h, P=44h, per spec contract §12): **28.7h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- **WI-S14-001 SEALED** (4 regions infra; per-region R2 + D1).
- **WI-S14-002 SEALED** (region pinning; BYOK respects region).
- **S-01 SEALED** (CAS layer integration).
- **S-13 SEALED** (rotation framework BYOK 7d overlap; admin API).
- AWS staging account com test CMK provisioned.
- aws-sdk-kms Rust crate available.

### Soft blockers

- AWS IAM role provisioned (Encrypt + Decrypt + DescribeKey).

### Outbound

- **WI-S14-005** (GCP/Azure/Vault adapters consume trait + matrix framework).
- **WI-S14-006** (kill switch consumes DekCache::evict_all_for_key + KMS access check).
- **WI-S14-007** (erasure attestation consumes BYOK envelope on DSR).
- **WI-S14-009** (TLA+ + pentest + PRR).

## 19. Effort PERT

O: 18h, M: 28h, P: 44h → PERT **28.7h** (per spec contract §12).

## 20. Time-boxing

**32h hard limit owner**. If exceeded → escalation: split em sub-WI (trait + DEK cache vs AWS adapter vs matrix framework).

## 21. Observability

7 métricas listadas §6.1.8. Trace spans em §6.1.9. Logs structured JSON; nivel INFO em ok, WARN em cache miss + cmk access throttled, ERROR em wrap/unwrap fail.

Dashboard widget DASH-BYOK:
- Wrap/unwrap latency p99 per provider (AWS-only em este WI; 4 stacks em WI-005).
- DEK cache hit/miss ratio per provider.
- DEK cache evictions reason breakdown.
- Envelope encrypt/decrypt error rate.
- 16-combination matrix test green status.

## 22. Cost Analysis

- AWS KMS: ~$0.03 per 10k requests; per-blob 1 wrap + 1 unwrap = 2 req; 1M blobs/mês = ~$6/mês.
- DEK cache infra: bounded; ~$5/mês.
- Cloudflare Worker compute (envelope encryption): ~$10/mês.
- D1 byok_envelope storage: ~$5/mês.
- AWS CloudTrail (audit): ~$2/mês.
- **Total custo direto WI-S14-004**: ~$30/mês baseline + workload-dependent ~$100/mês.

## 23. API Contract

`KmsProvider` trait é internal Rust trait. Customer-facing API:
- `POST /v1/customer/byok/configure` body=`{kms_provider, kms_key_id, region, encryption_context}` → 200 (BYOK enabled per-tenant).
- `GET /v1/customer/byok/status` → 200 `{kms_provider, kms_key_id, fips_level, last_check_ms}`.

API semver stable post v1.0; breaking changes em `KmsProviderKind` enum = bump major + ADR + migration plan.

## 24. Post-mortem Hooks

- DEK cache TTL > 5 min detected (somehow) → CRITICAL post-mortem + INV-BYOK-CRYPTO-SOVEREIGNTY review.
- AES-GCM nonce reuse detected → CRITICAL post-mortem + crypto incident response.
- AAD binding bypass detected → CRITICAL + Architect + Crypto SME.
- Wrap/unwrap latency p99 > 30ms sustained 1h → SEV-2 + capacity review.
- FIPS compliance drift → SEV-2 + Compliance Officer.
- AWS KMS outage > 5 min → degrade mode + post-mortem + provider escalation.
- Customer CMK substitution detected → CRITICAL + Security incident.
- AWS IAM token compromise detected → CRITICAL + immediate rotation.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- DEK cache rollback: clear cache + re-fetch wrapped DEKs.
- BYOK rollback: customer disables BYOK; CoreLink falls back to TDK CoreLink-managed.
- RTO ≤ 30 min (Worker rollback).
- RPO 0 (per-blob D1 + R2 preserve all data; BYOK envelope intact).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: AWS KMS provider authenticated via IAM role; per-blob kms_key_id binding prevents CMK substitution.
- **Tampering**: AES-256-GCM auth tag + AAD binding detect tampering; INV-CAS-INTEGRITY herdada.
- **Repudiation**: per-envelope op audit emit + CloudTrail audit + 7y retention.
- **Information disclosure**: DEK ZeroizeOnDrop + cache 5 min TTL hard; DEK never em logs/traces.
- **DoS**: DEK cache absorbs KMS API outage 5 min; degrade mode > 5 min.
- **Elevation of privilege**: AWS IAM minimal scope; admin API for BYOK config requires admin role + dual-approval (S-13).

**LINDDUN delta**:
- **Linkability**: tenant_id + kms_key_id em audit (compliance accountability).
- **Identifiability**: customer email em audit (intentional CTRL-AUDIT-002).
- **Non-repudiation**: per-envelope op audit chain.
- **Detectability**: AAD bypass + nonce reuse + FIPS drift + IAM compromise alerts.
- **Disclosure**: BYOK provides customer crypto sovereignty.
- **Unawareness**: customer can audit AWS CloudTrail for their CMK usage independently.
- **Non-compliance**: SOC 2 CC6.1 + NIST SP 800-57 + NIST SP 800-130 + FIPS 140-3 + LGPD Art. 38 + GDPR Art. 32 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-byok/README.md` — overview + envelope encryption pattern + DEK cache 5 min hard.
- `crates/corelink-byok-aws/README.md` — AWS-specific adapter usage.
- ADR-XXXX — BYOK trait + envelope encryption ratification.
- Doc `docs/internal/multi-region-byok.md` (BYOK section) — sequence diagram + envelope encryption flow.
- Workshop interno (3h) com Architect + Crypto SME + Security Lead + AppSec + Compliance pós-merge.
- Onboarding test (10 questions): envelope encryption flow, DEK cache TTL 5 min hard, AAD binding, AES-256-GCM nonce, FIPS levels per provider, adversarial scenarios, ZeroizeOnDrop, AWS IAM scope, INV-BYOK-CRYPTO-SOVEREIGNTY, matrix test framework.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | DEK cache TTL > 5 min violation | L | H | CRITICAL | M | LOW | Constructor enforced + property test + INV |
| R-002 | AES-GCM nonce reuse | L | H | CRITICAL | M | LOW | RNG secure + property test 1M nonces |
| R-003 | DEK ephemeral leak (logs) | L | H | CRITICAL | M | LOW | ZeroizeOnDrop + redaction macros + CI grep |
| R-004 | AAD binding bypass | L | H | HIGH | M | LOW | Mandatory binding + property test 10k swap |
| R-005 | Wrong CMK substitution | L | M | HIGH | L | LOW | Per-blob kms_key_id stored + audit emit |
| R-006 | DEK cache poisoning | L | M | HIGH | L | LOW | Cache key derived from wrapped hash + integrity |
| R-007 | FIPS compliance drift | L | M | HIGH | L | LOW | Quarterly review + CMVP API alert |
| R-008 | AWS KMS API outage | L | M | HIGH | L | LOW | DEK cache 5 min + degrade-mode read-only |
| R-009 | Wrap/unwrap latency > 30ms | M | M | MEDIUM | M | LOW | Region-co-located + alert SEV-3 |
| R-010 | Matrix test break | M | M | MEDIUM | M | LOW | Auto-fail PR + weekly green run |
| R-011 | AWS IAM token compromise | L | M | CRITICAL | L | LOW | Scope minimal + CloudTrail + rotation |
| R-012 | KMS provider API drift | M | M | MEDIUM | M | LOW | Adapter trait isolation + matrix + version pin |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME folded review trait + envelope encryption flow + DEK cache TTL hard + AAD binding.
2. **Per-component (D+1..D+3)**: Crypto SME pair-program each crate (corelink-byok types + DekCache + EnvelopeEncryptor + AwsKmsProvider).
3. **Code (D+3)**: peer review + Crypto SME pair-program adversarial tests.
4. **Security (D+3)**: Security Lead review threat model + AWS IAM + DEK cache extraction.
5. **AppSec (D+4)**: AppSec review CVE-class adversarial scenarios + memory dump + replay.
6. **Compliance (D+4)**: Compliance Officer review NIST SP 800-57 + NIST SP 800-130 + FIPS 140-3 attestation.
7. **Property test (pre-merge D+5)**: 7 props × 10k iter green em PR; 100k nightly green.
8. **Adversarial (pre-merge D+5)**: red team session — AAD bypass + nonce reuse + side channel + CMK substitution.
9. **Integration test (D+5)**: real AWS KMS staging E2E.
10. **PRR mini (D+6)**: Architect + Crypto SME + Security Lead + AppSec + Compliance Officer sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization MANDATORY (cripto algorithm review per provider + envelope encryption + AES-256-GCM + DEK cache TTL hard + AAD binding + matrix design + FIPS attestation)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — threat model + AWS IAM + DEK cache extraction + adversarial scenarios_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — KMS API outage degrade + cost regression + chaos test_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD; emphatic — property test 10k + 100k + matrix coverage_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — NIST SP 800-57 Pt 1 Rev 5 + NIST SP 800-130 + FIPS 140-3 attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; emphatic — LGPD Art. 38 + GDPR Art. 32 + customer crypto sovereignty_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — CVE-class adversarial scenarios + AAD bypass + nonce reuse + side channel + CMK substitution + IAM compromise_ | _pending_ | _pending_ |

> Crypto SME (cripto algorithm review per provider + envelope encryption + matrix test cross-validation) folds into Architect role specialization MANDATORY (precedent: S-13 secret rotation + S-12 SLSA L3 + Cosign keyless OIDC). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-004 (cycle 12.S14.0); biggest crypto WI; mirror WI-S13-003 closely. |

## 32. Anti-patterns evitados

- DEK cache TTL > 5 min (INV-BYOK-CRYPTO-SOVEREIGNTY violation).
- Skip AES-256-GCM nonce per-write 96-bit random.
- Skip ZeroizeOnDrop em DEK + cache.
- Skip AAD encryption_context binding.
- Skip per-blob kms_key_id storage.
- Skip 16-combination matrix test framework.
- Skip FIPS doc per provider.
- Skip property tests 7 crypto-load-bearing props.
- Skip adversarial regression tests CVE-class.
- AWS IAM role overscope.
- DEK em logs / traces / errors.
- Plaintext CMK em CoreLink.
- Direct DEK write to D1.
- Skip integration test E2E real AWS KMS.
- Single-key envelope (per-blob CMK enables future BYOE).

---

**Fim WI-S14-004.** Próximo: WI-S14-005 (BYOK GCP/Azure/Vault adapters + 16-combination matrix test).
