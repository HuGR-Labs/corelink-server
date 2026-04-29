---
id: "WI-S14-005"
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
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "COMPLIANCE-MATRIX"
tags: ["wi", "s14", "byok", "gcp-kms", "azure-key-vault", "hashicorp-vault", "matrix-test", "fips-140-2", "fips-140-3", "high-risk"]
---

# WI-S14-005 — BYOK GCP KMS Adapter (FIPS 140-2 / 140-3 Quando Suportado) + Azure Key Vault Premium HSM Adapter (FIPS 140-2 Level 2) + HashiCorp Vault Transit Adapter (FIPS 140-3 Level 1 Vault Enterprise; mTLS Auth Customer-Hosted) Batch Implementation + 16-Combination Matrix Test `tests/byok_matrix_test.rs` (4 Providers × 4 Ops {write, read, wrap, unwrap}) Verde em Staging Weekly + Auto-Fail PR Se Matrix Break + FIPS Doc per Provider em `compliance/byok-fips-matrix.md`

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-005 |
| Título | BYOK GCP KMS adapter (FIPS 140-2; 140-3 quando GCP suportar) + Azure Key Vault Premium HSM adapter (FIPS 140-2 Level 2 mandatory para Premium HSM tier) + HashiCorp Vault Transit adapter (FIPS 140-3 Level 1 Vault Enterprise; mTLS auth customer-hosted; transit secrets engine) batch implementation reusing trait WI-S14-004; 16-combination matrix test (4 providers × 4 ops {write, read, wrap, unwrap}) verde em staging weekly auto-fail PR if matrix break; FIPS doc per provider em `compliance/byok-fips-matrix.md`; staging deploy 3 providers + integration tests E2E |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (BYOK crypto controls per provider), FF-HR-008 (multi-cloud BYOK vendor lock-in risk; matrix testing required) |

## 1. Intent

Implementar 3 BYOK adapters (GCP KMS + Azure Key Vault + HashiCorp Vault) reusing trait WI-S14-004; complete 16-combination matrix test; FIPS doc per provider. Foundation para WI-S14-006 (kill switch covers 4 providers) + WI-S14-009 (pentest covers AWS primary + 3 secondaries).

```rust
// File: crates/corelink-byok-gcp/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;
use corelink_byok::*;
use google_kms1::api::{EncryptRequest, DecryptRequest};

pub struct GcpKmsProvider {
    client: google_kms1::CloudKMS,
    project_id: String,
    region: String,
}

#[async_trait]
impl KmsProvider for GcpKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::GcpKms
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // GCP KMS default = FIPS 140-2 L1; FIPS 140-3 quando GCP suportar.
        // Quarterly review com Crypto SME.
        FipsLevel::Fips140_2_L1
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        // Encrypt via GCP KMS API
        // additional_authenticated_data = JSON-encoded encryption_context
        unimplemented!()
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        // Decrypt via GCP KMS API
        // verify additional_authenticated_data matches stored encryption_context
        unimplemented!()
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        // GET key resource; check enabled state
        unimplemented!()
    }
}

// File: crates/corelink-byok-azure/src/lib.rs

pub struct AzureKeyVaultProvider {
    client: azure_security_keyvault_keys::KeyClient,
    vault_url: String,
    region: String,
}

#[async_trait]
impl KmsProvider for AzureKeyVaultProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AzureKeyVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // Azure Key Vault Premium HSM tier = FIPS 140-2 Level 2 mandatory.
        // Standard tier = FIPS 140-2 L1; CoreLink requires Premium for BYOK.
        FipsLevel::Fips140_2_L2
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        // Wrap key via Azure Key Vault wrapKey API; algorithm = RSA-OAEP-256 ou A256KW
        unimplemented!()
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        unimplemented!()
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        unimplemented!()
    }
}

// File: crates/corelink-byok-vault/src/lib.rs

pub struct VaultProvider {
    client: vault_client::Client,
    transit_engine_path: String,
    region: String,
    mtls_cert: Vec<u8>,
}

#[async_trait]
impl KmsProvider for VaultProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::HashicorpVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // Vault Enterprise w/ FIPS 140-3 L1 build.
        // Customer-hosted; mTLS auth enforced.
        FipsLevel::Fips140_3_L1
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        // POST {transit_engine_path}/encrypt/{key_name}
        // context = base64(JSON-encoded encryption_context)
        unimplemented!()
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        // POST {transit_engine_path}/decrypt/{key_name}
        unimplemented!()
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        // GET {transit_engine_path}/keys/{key_name}; check status
        unimplemented!()
    }
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

3 BYOK adapters + 16-combination matrix test = sprint-defining moment para enterprise tier multi-cloud BYOK posture. Cada provider tem semantics diferentes (AWS KMS encryption_context AAD; GCP KMS additional_authenticated_data; Azure wrapKey RSA-OAEP-256 ou A256KW; Vault transit context base64); adapter trait isolation (WI-S14-004) ensures CoreLink core code is provider-agnostic; matrix test ensures parity.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Provider semantic drift** (e.g., GCP KMS API breaking change): adapter implementations break. Mitigação: matrix test weekly em staging; auto-fail PR if any cell breaks; version-pin SDKs (cargo-deny + Cargo.lock).

2. **AAD/AdditionalAAD/EncryptionContext semantics inconsistency**: AWS uses `encryption_context` (HashMap); GCP uses `additional_authenticated_data` (bytes); Azure wrapKey doesn't support AAD (RSA-OAEP); Vault uses `context` (base64). Mitigação: per-provider adapter encapsulates semantics; common `encryption_context: Option<serde_json::Value>` API; Azure wrap = inner-encrypt with AAD via AES-GCM antes Azure wrapKey (custom flow documented em ADR).

3. **FIPS compliance level mismatch per provider**: customer expects FIPS 140-3 mas Azure provides 140-2 L2 only. Mitigação: `fips_level()` returns const value per provider; documented em compliance matrix; customer notified at signup.

4. **Customer-hosted Vault unreachable**: customer's Vault instance offline = CoreLink reads fail. Mitigação: DEK cache 5 min absorbs (WI-S14-004); chaos test Vault outage scenarios; runbook RB-BYOK-PROVIDER-OUTAGE.

5. **mTLS cert expiry em Vault adapter**: customer's mTLS cert expires; Vault rejects connections. Mitigação: cert expiry check + alert customer 30d before; integration test cert renewal flow.

6. **Cross-provider key migration**: customer wants migrate AWS KMS → Vault. Mitigação: BYOK rotation framework (S-13 herdada) supports cross-provider migration via re-wrap; ADR documents flow; chaos test migration scenario.

7. **Matrix test flakiness**: GCP KMS API rate limit causes test failures. Mitigação: retry policy + exponential backoff; rate limit budget per test run; alert if persistent failure.

8. **Provider-specific bugs hidden by abstraction**: trait abstracts; provider-specific bugs missed. Mitigação: matrix test 16 cells weekly + adversarial regression per provider; provider-specific integration tests.

**Atacante adversarial scenarios**:

- **Provider impersonation**: attacker fakes GCP KMS endpoint. Mitigação: SDK uses official endpoints + TLS verify + IAM auth.

- **mTLS cert compromise**: attacker steals customer Vault mTLS cert. Mitigação: cert rotation + alert; per-cert audit trail.

- **AAD/AdditionalAAD bypass**: attacker swaps providers cross-blob. Mitigação: per-blob `kms_provider` stored em D1; mismatch on unwrap = BYOKError.

- **Replay wrap → unwrap em Vault transit engine**: attacker replays. Mitigação: Vault rate limit + audit emit + customer alert.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: BYOK crypto controls per provider (3 additional providers).
- **FF-HR-008**: multi-cloud vendor lock-in risk; matrix testing critical.
- **Reversibility**: provider drift detected via matrix test weekly; remediation possible.

11 sign-offs canonical incl. Architect (com **Crypto SME folded mandatory**: cripto algorithm review per provider + AAD semantics + matrix test design) + Security Lead + AppSec + Compliance Officer (FIPS 140-2/3 + NIST SP 800-57 + customer notification semantics).

## 3. Customer Impact & Journey

**Persona 1 — Customer with multi-cloud BYOK requirement (enterprise)**:
- Customer chooses provider (AWS / GCP / Azure / Vault) at BYOK config time.
- Provider FIPS level documented; customer notified at signup.
- Per-provider latency overhead documented (region-co-located ≤ 30ms p99).

**Persona 2 — Auditor SOC 2 + ISO 27001 + FIPS 140-2/3**:
- Compliance matrix per provider; NIST SP 800-57 + NIST SP 800-130 + FIPS 140-3 attestation.
- 16-combination matrix test weekly green; CI gate active.
- Quarterly review com Crypto SME.

**Persona 3 — DevSecOps em prospect enterprise (RFP)**:
- RFP question: "Multi-cloud BYOK supported? Vault customer-hosted?".
- Evidence: 4 providers supported; matrix test 16 cells; mTLS customer-hosted Vault.
- Diferenciador: AWS S3+KMS supports AWS only; Azure Blob supports Azure only; CoreLink ALL 4.

**SLA addendum**:
- 4 providers supported (AWS / GCP / Azure / Vault).
- 16-combination matrix test verde weekly.
- FIPS 140-2 / 140-3 documented per provider.
- mTLS auth for Vault customer-hosted.
- Cert expiry alert 30d before for Vault.
- Quarterly review com Crypto SME.

## 4. Capability Mapping

- **CAP-BYOK-002** (BYOK GCP KMS) — IMPLEMENTA primary.
- **CAP-BYOK-003** (BYOK Azure Key Vault) — IMPLEMENTA primary.
- **CAP-BYOK-004** (BYOK HashiCorp Vault) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.2 R-S14-6 + R-S14-11 + R-S14-12` + `security_model.md §6 (CTRL-CRYPTO-005, CTRL-KEY-010..015)` + `compliance_matrix.md (NIST SP 800-57 + FIPS 140-2/3)`.

## 5. Tipo

KMS adapter batch + matrix test; HIGH_RISK; FF-HR-005 + FF-HR-008.

## 6. Escopo

### 6.1 In-scope

1. **Crate `crates/corelink-byok-gcp/`**:
   - `GcpKmsProvider` impl `KmsProvider`.
   - GCP SDK (google-kms1 or grpcio-based).
   - IAM service account minimal scope.
   - FIPS 140-2 L1 default (140-3 quando GCP suportar).
   - additional_authenticated_data semantics → encryption_context map.

2. **Crate `crates/corelink-byok-azure/`**:
   - `AzureKeyVaultProvider` impl `KmsProvider`.
   - Azure SDK (azure_security_keyvault_keys).
   - Premium HSM tier mandatory (FIPS 140-2 L2).
   - **Custom AAD flow**: Azure wrapKey doesn't support AAD; CoreLink encrypts inner with AES-GCM-AAD antes wrapKey wraps the AES-GCM result. Documented em ADR.
   - Managed Identity OR Service Principal auth.

3. **Crate `crates/corelink-byok-vault/`**:
   - `VaultProvider` impl `KmsProvider`.
   - Vault HTTP API client (vault-client crate).
   - mTLS auth (customer-hosted; cert pinned).
   - Transit secrets engine path configurable.
   - FIPS 140-3 L1 (Vault Enterprise FIPS build).

4. **16-combination matrix test `tests/byok_matrix_test.rs`**:
   - 4 providers × 4 ops {write, read, wrap, unwrap} = 16 cells.
   - All 4 cells green em CI per PR.
   - Weekly cron staging run all 16 cells.
   - Auto-fail PR if any cell breaks.
   - Provider-specific assertions (FIPS level, latency, AAD semantics).

5. **FIPS doc `compliance/byok-fips-matrix.md`** (3 providers + AWS row from WI-004):
   - AWS KMS: FIPS 140-3 Level 1 (WI-S14-004).
   - GCP KMS: FIPS 140-2 L1 default.
   - Azure Key Vault Premium HSM: FIPS 140-2 Level 2.
   - Vault Enterprise: FIPS 140-3 Level 1.
   - Per-provider NIST CMVP module ID referenced.
   - Quarterly review cadence documented.

6. **Métricas underscored Prometheus** (per `observability_model.md §3.1`; label `plan` aplicável):
   - Reuse `corelink_byok_*{provider, plan}` metrics from WI-S14-004.
   - `corelink_byok_matrix_test_total{provider, op, outcome}` (16 cells × outcomes).
   - `corelink_byok_provider_latency_p99_seconds{provider}` (gauge from histogram).
   - `corelink_byok_fips_compliance_status{provider, fips_level}` (gauge; alert on drift).

7. **Observability** — trace spans `byok.{wrap, unwrap}.{aws, gcp, azure, vault}` with provider attribute.

8. **Audit emission** — CloudEvent per envelope op com `provider` field; reuse WI-S14-004 schema.

9. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_aad_binding_per_provider`: 4 providers × 10k iter swap attempts; assert reject.
   - `prop_wrap_unwrap_roundtrip_per_provider`: 4 providers × 10k iter wrap+unwrap; assert byte-equal.
   - `prop_fips_level_const_per_provider`: assert `fips_level()` returns const value.
   - `prop_check_access_revoke_detection_per_provider`: 4 providers × revoke scenarios.

10. **Adversarial regression tests per provider**:
    - GCP: additional_authenticated_data swap; service account JWT replay.
    - Azure: wrapKey AAD bypass via custom flow; Managed Identity rotation.
    - Vault: mTLS cert compromise; transit context base64 tampering.
    - Cross-provider: kms_provider field tampering em D1; verify per-blob provider stored.

11. **Integration test E2E per provider**:
    - GCP staging account + test KMS key; full E2E.
    - Azure staging tenant + test Premium Key Vault; full E2E.
    - Vault customer-hosted mock instance + mTLS cert; full E2E.
    - Latency p99 ≤ 30ms region-co-located each.

12. **Cross-provider migration ADR-XXXX**:
    - Document migration flow: customer rotates BYOK provider.
    - Re-wrap envelope via S-13 rotation framework + new provider.
    - 7d overlap window (BYOK CMK 7d herdada).

### 6.2 Out-of-scope (deferred)

- **CMK kill switch + chaos drill**: WI-S14-006 (covers 4 providers).
- **Erasure attestation**: WI-S14-007.
- **DPA + Schrems II TIA**: WI-S14-008.
- **TLA+ + pentest + PRR**: WI-S14-009.
- **Federated KMS multi-cloud failover**: single CMK per tenant at GA.
- **Quantum-resistant crypto**: Fase 3.
- **Customer-managed HSM on-prem (non-Vault)**: only cloud KMS at GA.

## 7. Anti-Scope

- Skip per-provider FIPS doc.
- Skip 16-combination matrix test weekly.
- Skip auto-fail PR on matrix break.
- Provider-specific bugs hidden by trait abstraction (matrix + integration tests catch).
- Skip Azure wrapKey custom AAD flow (mandatory; documented em ADR).
- Skip Vault mTLS cert pinning.
- Cross-provider DEK leak (per-provider scope enforced em D1).
- Skip property tests per provider.
- Skip adversarial regression per provider.
- Skip integration test E2E per provider.
- Skip quarterly review com Crypto SME.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-005 — BYOK 3 providers + 16-combination matrix test

  Background:
    Given trait KmsProvider operational (WI-S14-004)
    Given AwsKmsProvider impl green (WI-S14-004)
    Given DEK cache 5 min hard limit operational
    Given GCP/Azure/Vault staging accounts provisioned

  Scenario: GCP KMS adapter wrap + unwrap + check_access
    Given GcpKmsProvider configured with project_id + region + service_account
    When wrap_dek + unwrap_dek + check_access ops
    Then 3 ops green; latency ≤ 30ms p99 region-co-located
    And FIPS 140-2 L1 documented
    And additional_authenticated_data binding mandatory

  Scenario: Azure Key Vault adapter Premium HSM
    Given AzureKeyVaultProvider configured with vault_url + Managed Identity
    When wrap (custom AAD flow) + unwrap + check_access ops
    Then 3 ops green; latency ≤ 30ms p99
    And FIPS 140-2 L2 mandatory (Premium HSM tier)
    And custom AAD flow via inner AES-GCM antes wrapKey

  Scenario: Vault Transit adapter customer-hosted
    Given VaultProvider configured with mTLS cert + transit_engine_path
    When wrap + unwrap + check_access ops
    Then 3 ops green; latency ≤ 30ms p99
    And FIPS 140-3 L1 (Vault Enterprise FIPS build)
    And mTLS auth verified

  Scenario: 16-combination matrix test em CI per PR
    Given 4 providers × 4 ops = 16 cells
    When matrix test runs em PR
    Then 16/16 cells green
    And auto-fail PR if any cell breaks

  Scenario: 16-combination matrix weekly cron staging
    Given weekly cron staging
    When matrix test runs
    Then 16/16 cells green
    And report committed em audit folder
    And alert SEV-2 if any cell breaks

  Scenario: FIPS doc per provider
    Given compliance/byok-fips-matrix.md
    When reviewed
    Then 4 rows present (AWS / GCP / Azure / Vault)
    And NIST CMVP module ID per row
    And quarterly review cadence documented

  Scenario: Provider-specific FIPS level constant
    Given fips_level() called per provider
    Then AWS = Fips140_3_L1 (WI-004)
    And GCP = Fips140_2_L1 (default)
    And Azure = Fips140_2_L2 (Premium HSM)
    And Vault = Fips140_3_L1 (Enterprise FIPS)

  Scenario: AAD binding per provider
    Given encryption_context = {"tenant_id": "T1", "blob_hash": "H1"}
    When wrap with AAD per provider; unwrap with mismatched AAD
    Then 4 providers × reject
    And BYOKError::EnvelopeError returned each

  Scenario: Cross-provider kms_provider field tampering
    Given D1 row stored kms_provider = "aws_kms"
    When attacker tampers D1 to "gcp_kms"
    Then unwrap fails (wrong provider for stored wrapped DEK)
    And BYOKError returned + audit emit

  Scenario: Vault mTLS cert expiry alert 30d before
    Given Vault adapter mTLS cert expires em 30d
    When daily cert check runs
    Then alert customer SEV-3 fires
    And renewal flow documented em runbook

  Scenario: Property tests 4 props × 10k iter green
    Given prop_aad_binding_per_provider + prop_wrap_unwrap_roundtrip + prop_fips_level_const + prop_check_access_revoke
    When 10k iter run em PR
    Then 0 violations across 4 providers
    And nightly 100k iter green

  Scenario: Adversarial regression tests per provider green
    Given GCP service account JWT replay + Azure Managed Identity rotation + Vault mTLS compromise + cross-provider tampering
    When red team session
    Then 4+ scenarios mitigated per provider = 16+ total
    And report committed em audit folder

  Scenario: Integration test E2E per provider green
    Given GCP staging + Azure staging + Vault staging
    When E2E test runs
    Then 3 providers + 1 (AWS WI-004) = 4 providers green em staging
    And latency p99 ≤ 30ms each

  Scenario: Quarterly review com Crypto SME
    Given quarterly cycle
    When Crypto SME reviews FIPS compliance + provider drift
    Then report committed
    And NIST CMVP status verified per provider
```

## 9. Design Decisions

### 9.1 Why batch 3 providers em 1 WI (NÃO 1 WI per provider)

- Trait abstraction (WI-S14-004) makes per-provider impl ~3-5h each.
- Matrix test framework reused; 12 additional cells.
- Single ADR for cross-provider semantics + custom AAD flow Azure.
- Reduces sprint overhead.

### 9.2 Why Azure custom AAD flow (NÃO native AAD)

- Azure Key Vault wrapKey API doesn't support AAD natively (RSA-OAEP wraps key, not data).
- CoreLink encrypts inner with AES-GCM-AAD antes wrapKey wraps the AES-GCM result.
- Documented em ADR + per-provider integration test.
- Adds ~2ms overhead but preserves AAD binding semantics.

### 9.3 Why Vault customer-hosted mTLS auth

- HashiCorp Vault Transit engine = customer-hosted; not SaaS.
- mTLS = mutual auth; CoreLink presents cert + verifies Vault cert.
- Cert pinning prevents MITM.
- Cert expiry alert 30d before.

### 9.4 Why 16-combination matrix test weekly em staging

- Provider API drift detected weekly (vs nightly = 7× cost; vs monthly = 1× drift exposure).
- Auto-fail PR on cell break = pre-merge regression.
- Adversarial regression weekly red team rotation.

### 9.5 Why FIPS level const per provider

- FIPS level is fixed property of provider tier (e.g., Azure Premium HSM = 140-2 L2).
- Const eliminates runtime drift.
- Quarterly review com Crypto SME ensures NIST CMVP module status remains valid.

### 9.6 Why ADR potencial?

- Sim — **ADR-XXXX**: "BYOK 4 providers (AWS + GCP + Azure + Vault) + 16-combination matrix test + Azure custom AAD flow + Vault mTLS S-14". Decisão arquitetural cripto-load-bearing; reuse pattern em BYOE Fase 2.

## 10. Completeness Criteria SOTA

- [ ] **10.s14.005.1** Crate `corelink-byok-gcp` + GcpKmsProvider impl green (EVT-013).
- [ ] **10.s14.005.2** Crate `corelink-byok-azure` + AzureKeyVaultProvider impl + custom AAD flow green (EVT-013).
- [ ] **10.s14.005.3** Crate `corelink-byok-vault` + VaultProvider impl + mTLS auth green (EVT-013).
- [ ] **10.s14.005.4** 16-combination matrix test em CI per PR (16/16 cells green) (EVT-002).
- [ ] **10.s14.005.5** Weekly cron staging matrix run green; report committed (EVT-002) *(GA Evidence Gate D+60)*.
- [ ] **10.s14.005.6** Auto-fail PR if any cell breaks (EVT-013).
- [ ] **10.s14.005.7** FIPS doc 4 rows em compliance/byok-fips-matrix.md (EVT-044).
- [ ] **10.s14.005.8** Property tests 4 props × 10k iter PR + 100k nightly green (EVT-022).
- [ ] **10.s14.005.9** Adversarial regression 4+ scenarios per provider green (EVT-040).
- [ ] **10.s14.005.10** Integration test E2E 3 providers staging green (EVT-024).
- [ ] **10.s14.005.11** Vault mTLS cert expiry alert 30d before (EVT-044).
- [ ] **10.s14.005.12** Quarterly Crypto SME review cadence documented.
- [ ] **10.s14.005.13** ADR-XXXX (cross-provider semantics + Azure custom AAD + Vault mTLS) ratificada.
- [ ] **10.s14.005.14** Cost regression gate: 3 providers infra ≤ $50/mês baseline.

## 11. DoD

- [ ] 3 crates compilam (gcp + azure + vault).
- [ ] 3 adapters impl `KmsProvider` trait.
- [ ] 16-combination matrix test em CI + weekly cron.
- [ ] FIPS doc 4 rows committed.
- [ ] All 14 Gherkin scenarios green.
- [ ] Property tests 4 props × 10k iter green; 100k nightly green.
- [ ] Adversarial regression 4+ scenarios per provider green.
- [ ] Integration tests E2E 3 providers green.
- [ ] Vault mTLS cert expiry alert.
- [ ] ADR-XXXX (cross-provider) ratificada.
- [ ] Code review (Architect + Crypto SME folded mandatory + Security Lead + AppSec + Compliance).
- [ ] PRR Architect + Crypto SME mini-sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — registry §3.12 herdada WI-S14-004): este WI extends to 3 additional providers; DEK cache 5 min hard limit applies all 4.
- **INV-KEY-OVERLAP** (HIGH — registry §3.13 herdada): BYOK CMK 7d overlap canonical applies all 4.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada): per-provider audit emit em D1 atomic.

### Novas

Nenhuma direta neste WI; reforça INVs herdadas WI-S14-004.

TLA+ alignment: registry §4.2 indica `key_lifecycle.tla` PLANNED S-13 + `region_residency.tla` PLANNED S-14 WI-S14-009.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate corelink-byok-gcp | `crates/corelink-byok-gcp/src/lib.rs` | Rust |
| Crate corelink-byok-azure | `crates/corelink-byok-azure/src/lib.rs` | Rust |
| Crate corelink-byok-vault | `crates/corelink-byok-vault/src/lib.rs` | Rust |
| 16-combination matrix test | `tests/byok_matrix_test.rs` | Rust |
| Weekly cron staging | `.github/workflows/byok_matrix_weekly.yml` | YAML |
| FIPS doc | `compliance/byok-fips-matrix.md` (4 rows) | Markdown |
| Property tests | `tests/prop_byok_per_provider.rs` | Rust |
| Adversarial tests | `tests/adversarial_byok_per_provider.rs` | Rust |
| Integration tests E2E | `tests/e2e_byok_{gcp,azure,vault}.rs` | Rust |
| ADR-XXXX (cross-provider) | `specs/03_architecture/adrs/ADR-XXXX-byok-cross-provider-azure-aad-vault-mtls.md` | Markdown |
| Quarterly review template | `specs/_audits/templates/byok-quarterly-review.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s14.005.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s14.005.2** rustdoc 100% public API + 3 examples per provider.
- **14.s14.005.3** Test coverage ≥ 90%; property tests 10k+100k.
- **14.s14.005.4** Latência: wrap/unwrap p99 ≤ 30ms region-co-located per provider.
- **14.s14.005.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s14.005.6** Métricas RED + per-provider breakdown.
- **14.s14.005.7** Vault mTLS cert pinning + expiry alert.
- **14.s14.005.8** Breaking changes per-adapter = bump major + ADR.
- **14.s14.005.9** Memory bounded; ZeroizeOnDrop em DEK.
- **14.s14.005.10** Cost regression gate em CI.
- **14.s14.005.11** NIST SP 800-57 + FIPS 140-2/3 + customer notification per provider.
- **14.s14.005.12** Quarterly Crypto SME review.

## 15. Chaos Experiments

1. **GCP KMS API outage**: simulate 503 sustained; DEK cache absorbs 5 min.

2. **Azure Key Vault outage**: same pattern.

3. **Vault customer-hosted unreachable**: simulate network partition; CoreLink reads fail post-cache; degrade-mode read-only.

4. **Vault mTLS cert expiry**: simulate cert expires; CoreLink reads fail; alert customer.

5. **Cross-provider kms_provider field tampering**: D1 row tampered to wrong provider; unwrap fails + audit.

6. **GCP service account JWT replay**: inject replay; verify rate limit + audit.

7. **Azure Managed Identity rotation**: simulate rotation mid-flight; verify graceful re-auth.

8. **Matrix test cell break**: inject synthetic break em GCP unwrap; verify auto-fail PR.

9. **FIPS compliance drift Azure**: simulate Premium HSM downgrade to Standard tier; verify alert SEV-2.

10. **Cross-provider migration**: customer rotates AWS → Vault; verify re-wrap flow + 7d overlap.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Architect + Crypto SME mandatory + Security Lead + AppSec + Compliance review):

- [ ] All 14 Gherkin scenarios green.
- [ ] Property tests 4 props × 10k iter green; 100k nightly green.
- [ ] Adversarial regression 4+ per provider × 3 = 12+ scenarios green.
- [ ] Integration tests E2E 3 providers green.
- [ ] 16-combination matrix test em CI + weekly cron.
- [ ] FIPS doc 4 rows + NIST CMVP IDs.
- [ ] Vault mTLS cert pinning + expiry alert.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-BYOK (4 providers stacked).
- [ ] ADR-XXXX (cross-provider) published.
- [ ] Crypto SME review (per-provider algorithm review + custom AAD Azure + Vault mTLS).
- [ ] Compliance Officer review (FIPS 140-2/3 per provider + NIST SP 800-57).
- [ ] AppSec review (provider impersonation + mTLS compromise).
- [ ] Architect approval (composition with WI-S14-004).

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate corelink-byok-gcp scaffold + GcpKmsProvider impl | 4h |
| ST-002 | Crate corelink-byok-azure scaffold + AzureKeyVaultProvider impl | 4h |
| ST-003 | Azure custom AAD flow (inner AES-GCM antes wrapKey) | 3h |
| ST-004 | Crate corelink-byok-vault scaffold + VaultProvider impl | 4h |
| ST-005 | Vault mTLS auth + cert pinning + expiry alert | 3h |
| ST-006 | 16-combination matrix test framework completion (12 additional cells) | 4h |
| ST-007 | Weekly cron staging matrix workflow | 1.5h |
| ST-008 | FIPS doc 3 additional rows + NIST CMVP IDs | 2h |
| ST-009 | Property tests 4 props × 10k iter (per provider) | 4h |
| ST-010 | Adversarial regression tests 4+ per provider × 3 = 12+ scenarios | 5h |
| ST-011 | Integration test E2E GCP staging | 3h |
| ST-012 | Integration test E2E Azure staging | 3h |
| ST-013 | Integration test E2E Vault staging (mock customer-hosted) | 3h |
| ST-014 | Cross-provider kms_provider field tampering test | 1.5h |
| ST-015 | Vault mTLS cert expiry alert 30d before + runbook | 1.5h |
| ST-016 | rustdoc + 3 examples per provider | 3h |
| ST-017 | ADR-XXXX (cross-provider + Azure custom AAD + Vault mTLS) redação | 3h |
| ST-018 | Quarterly Crypto SME review template | 1h |
| ST-019 | Code review (Architect + Crypto SME folded + Security Lead + AppSec + Compliance) | 4h |

**Total Optimistic**: ~57h. **PERT** (O=20h, M=32h, P=50h, per spec contract §12): **32.7h**.

## 18. Dependencies

### Hard blockers

- **WI-S14-004 SEALED** (BYOK trait + AwsKmsProvider + DEK cache + envelope encryption flow).
- GCP staging account com test KMS key.
- Azure staging tenant com test Premium Key Vault.
- Vault Enterprise FIPS staging instance OR mock with mTLS.

### Soft blockers

- google-kms1, azure_security_keyvault_keys, vault-client crates available.

### Outbound

- **WI-S14-006** (kill switch covers 4 providers; reuses check_access).
- **WI-S14-007** (erasure attestation per-provider).
- **WI-S14-009** (TLA+ + pentest covers AWS primary + 3 secondaries).

## 19. Effort PERT

O: 20h, M: 32h, P: 50h → PERT **32.7h** (per spec contract §12).

## 20. Time-boxing

**40h hard limit owner**. If exceeded → escalation: split em sub-WI (GCP vs Azure vs Vault each).

## 21. Observability

Reuse WI-S14-004 metrics with per-provider breakdown. New metrics:
- `corelink_byok_matrix_test_total{provider, op, outcome}` (16 cells × outcomes).
- `corelink_byok_provider_latency_p99_seconds{provider}`.
- `corelink_byok_fips_compliance_status{provider, fips_level}`.

Dashboard widget DASH-BYOK:
- 16-combination matrix test green status.
- Per-provider latency p99 (4-stack).
- FIPS compliance status per provider.
- Vault mTLS cert expiry countdown.

## 22. Cost Analysis

- GCP KMS: ~$0.06 per 10k requests; ~$10/mês baseline.
- Azure Key Vault Premium HSM: ~$3/mês per key + $0.03 per 10k ops; ~$15/mês baseline.
- Vault customer-hosted: customer-paid; CoreLink-side mTLS cert ~$5/mês.
- Matrix test CI weekly: ~$10/mês.
- **Total custo direto WI-S14-005**: ~$40/mês baseline + workload-dependent ~$200/mês.

## 23. API Contract

Per-provider customer config via `POST /v1/customer/byok/configure` (extension WI-S14-004 API):
- AWS: `{kms_provider: "aws_kms", kms_key_id: "arn:...", region: "us-east-1"}`.
- GCP: `{kms_provider: "gcp_kms", kms_key_id: "projects/.../locations/.../keyRings/.../cryptoKeys/...", region: "us-east1"}`.
- Azure: `{kms_provider: "azure_key_vault", kms_key_id: "https://vault-name.vault.azure.net/keys/key-name", region: "eastus"}`.
- Vault: `{kms_provider: "hashicorp_vault", kms_key_id: "transit/keys/key-name", region: "us-east-1", vault_url: "https://vault.customer.com:8200", mtls_cert_pem: "..."}`.

## 24. Post-mortem Hooks

- Matrix test cell break sustained 1h → SEV-2 + post-mortem.
- Provider FIPS compliance drift detected → SEV-2 + Compliance Officer.
- Vault mTLS cert expiry not renewed em 7d → SEV-2 + customer notification.
- Cross-provider kms_provider tampering detected → CRITICAL + Security incident.
- GCP/Azure/Vault API outage > 30 min → post-mortem + provider escalation.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- Per-provider rollback: customer disables specific provider; falls back to other configured providers.
- RTO ≤ 30 min (Worker rollback).
- RPO 0.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: per-provider IAM/auth (GCP service account, Azure Managed Identity, Vault mTLS).
- **Tampering**: per-blob kms_provider stored em D1; cross-provider tampering detected.
- **Repudiation**: per-provider audit emit + provider-side audit logs (CloudTrail / GCP Audit Logs / Azure Monitor / Vault audit log).
- **Information disclosure**: customer crypto sovereignty per provider; CoreLink stores wrapped DEK only.
- **DoS**: matrix test detects provider drift; chaos test outage scenarios.
- **Elevation of privilege**: provider-specific IAM minimal scope.

**LINDDUN delta**:
- **Linkability**: tenant_id + kms_provider + kms_key_id em audit.
- **Identifiability**: customer email per provider config.
- **Non-repudiation**: per-provider audit trail dual-side (CoreLink + provider).
- **Detectability**: matrix test failures alert.
- **Disclosure**: BYOK provides customer crypto sovereignty per provider.
- **Unawareness**: customer notified at signup of FIPS level per provider.
- **Non-compliance**: FIPS 140-2 L2 (Azure Premium) + 140-3 L1 (Vault Enterprise) + 140-2 L1 (GCP) attestation.

## 27. Knowledge Transfer

- `crates/corelink-byok-gcp/README.md` — GCP-specific usage.
- `crates/corelink-byok-azure/README.md` — Azure-specific (custom AAD flow).
- `crates/corelink-byok-vault/README.md` — Vault-specific (mTLS).
- ADR-XXXX — cross-provider ratification.
- Doc `docs/internal/multi-region-byok.md` (4-provider section) — sequence diagrams per provider.
- Workshop interno (3h) com Architect + Crypto SME + Security Lead + AppSec + Compliance pós-merge.
- Onboarding test (10 questions): Azure custom AAD flow, Vault mTLS, FIPS levels per provider, matrix test 16 cells, quarterly review, cross-provider migration, mTLS cert expiry, INV-BYOK-CRYPTO-SOVEREIGNTY 4 providers.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Provider semantic drift (API change) | M | M | MEDIUM | M | LOW | Matrix test weekly + version-pin SDKs |
| R-002 | Azure custom AAD flow bug | L | M | HIGH | L | LOW | Property test + ADR + integration test |
| R-003 | Vault mTLS cert expiry | M | L | HIGH | M | LOW | Alert 30d before + auto-renewal flow |
| R-004 | Vault customer-hosted unreachable | M | L | HIGH | M | LOW | DEK cache 5 min + degrade-mode |
| R-005 | Cross-provider kms_provider tampering | L | M | HIGH | L | LOW | Per-blob stored + audit emit |
| R-006 | FIPS compliance drift per provider | L | M | HIGH | L | LOW | Quarterly review + CMVP API alert |
| R-007 | Matrix test flakiness | M | L | LOW | L | LOW | Retry policy + rate limit budget |
| R-008 | GCP service account JWT replay | L | M | HIGH | L | LOW | Rate limit + audit emit + rotation |
| R-009 | Azure Managed Identity rotation race | L | L | MEDIUM | L | LOW | Graceful re-auth + retry |
| R-010 | Cost regression em multi-provider | M | L | MEDIUM | L | LOW | Cost regression gate + benchmark |
| R-011 | Per-provider integration test flake | M | L | LOW | L | LOW | Retry + bounded duration |
| R-012 | Quarterly Crypto SME review missed | L | M | MEDIUM | L | LOW | Calendar reminder + accountability |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME folded review per-provider semantics + Azure custom AAD + Vault mTLS.
2. **Per-provider impl (D+1..D+4)**: Crypto SME pair-program each adapter (GCP + Azure + Vault).
3. **Code (D+4)**: peer review + Crypto SME pair-program adversarial tests per provider.
4. **Security (D+4)**: Security Lead review threat model per provider + provider impersonation + mTLS compromise.
5. **AppSec (D+5)**: AppSec review CVE-class scenarios per provider.
6. **Compliance (D+5)**: Compliance Officer review FIPS 140-2/3 per provider + NIST SP 800-57.
7. **Property test (pre-merge D+6)**: 4 props × 10k iter green em PR; 100k nightly green.
8. **Adversarial (pre-merge D+6)**: red team session per provider — service account replay + Managed Identity + mTLS compromise.
9. **Matrix test (pre-merge D+6)**: 16/16 cells green em CI.
10. **PRR mini (D+7)**: Architect + Crypto SME + Security Lead + AppSec + Compliance Officer sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization MANDATORY (per-provider algorithm review + Azure custom AAD + Vault mTLS + matrix design)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — provider impersonation + mTLS compromise + threat model per provider_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — provider outage degrade + chaos test + matrix weekly_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD; emphatic — property test 10k + 100k + matrix coverage 16 cells_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — FIPS 140-2/3 per provider + NIST SP 800-57 + customer notification_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; emphatic — LGPD + GDPR + customer crypto sovereignty per provider_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — CVE-class adversarial per provider + cross-provider tampering_ | _pending_ | _pending_ |

> Crypto SME (per-provider algorithm review + Azure custom AAD + Vault mTLS + matrix cross-validation) folds into Architect role specialization MANDATORY.

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-005 (cycle 12.S14.0); 3 BYOK adapters batch + 16-combination matrix completion. |

## 32. Anti-patterns evitados

- Skip per-provider FIPS doc.
- Skip 16-combination matrix test weekly.
- Skip auto-fail PR on matrix break.
- Skip Azure wrapKey custom AAD flow.
- Skip Vault mTLS cert pinning.
- Cross-provider DEK leak.
- Skip property tests per provider.
- Skip adversarial regression per provider.
- Skip integration test E2E per provider.
- Skip quarterly Crypto SME review.
- Single ADR for all 4 providers + cross-provider semantics.
- Skip cert expiry alert.

---

**Fim WI-S14-005.** Próximo: WI-S14-006 (CMK revocation kill switch + chaos drill).
