---
id: "WI-S11-003"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.2.0"
created: "2026-04-26"
updated: "2026-05-13"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-003", "FF-HR-005", "FF-HR-010"]
parent: "S-11"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "AUTH-MODEL"
  - "KEY-MANAGEMENT"
tags: ["wi", "s11", "consent-ledger", "gdpr-art-7", "lgpd-art-8", "proof-of-informed", "symmetric-revoke", "hmac", "high-risk"]
---

# WI-S11-003 — Consent Ledger Neon Schema (canonical pós Lote 10.11.0-bis) + Proof of Informed Consent (notice_text_hash + version + locale + wording_id + ui_capture_ts + submission_ts) + 5 Endpoints (capture/revoke/list/verify/admin) + Symmetric Revocation Schema (Lote 9.4 Opus H-05) + HMAC Signature + CTRL-PRIV-CONSENT-001..006 Wiring (`crates/corelink-privacy-consent-ledger`; Neon tables `consent_ledger` (canonical pós Lote 10.11.0-bis; privacy_model.md §5.6 L255) + `consent_revocation` 6-field schema simétrico per Lote 9.4 Opus H-05 — grant e revoke carregam mesmos campos de proof; INV-CONSENT-PROOF-VERIFIABLE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ symmetry) §3.12 L168 satisfaction; CTRL-PRIV-CONSENT-001 (opt-in proof of informed) + 002 (revogação imediata ≤5min cascade) + 003 (audit immutable) + 004 (LIA) + 005 (notice versioning) + 006 (proof of display screenshot opcional); HMAC-SHA256 per-consent signature via tenant-scoped HKDF; verify endpoint público stateless re-derives signature; cascade unsubscribe ≤24h downstream notify; GDPR Art. 7 + LGPD Art. 8 alignment "freely, specific, informed, unambiguous"; emit fail-CLOSED audit `dev.hugr.corelink.consent.{granted,revoked}.v1` 2 CloudEvents canonical types per privacy_model.md §5.6 L257)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-003 |
| Título | Consent ledger Neon schema canonical pós Lote 10.11.0-bis (consent_ledger + consent_revocation symmetric per Lote 9.4 H-05) + 5 endpoints REST (POST /v1/consent/<purpose>, DELETE /v1/consent/<purpose>, GET /v1/consent, GET /v1/consent/verify?signature=X, GET /v1/consent/revocation/verify?revocation_id=X) + HMAC-SHA256 tenant-scoped via HKDF (security_model.md §374 + key_management.md inheritance) + CTRL-PRIV-CONSENT-001..006 wiring (privacy_model.md §5.6 L255-260 canonical) + GDPR Art. 7 + LGPD Art. 8 "freely/specific/informed/unambiguous" alignment + 2 CloudEvents canonical `dev.hugr.corelink.consent.{granted,revoked}.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y CTRL-PRIV-CONSENT-003 |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII regulatory), FF-HR-005 (CTRL-PRIV-CONSENT-001..006), FF-HR-010 (1ª regulatory full impl) |

## 1. Intent

Consent Ledger é o **proof-of-informed regulatory cornerstone** do CoreLink — sem isso, **CTRL-PRIV-CONSENT-001** falha e GDPR Art. 7 / LGPD Art. 8 ficam unfulfilled (consent record sem proof de "informed"). Schema 6-field per evento (notice_text_hash + notice_version + locale + wording_id + ui_capture_ts + submission_ts) é o **Lote 6.5 G-07 expanded payload** que distingue consent record que prova "houve um evento" de consent record que prova "titular viu o notice certo com palavras certas no idioma certo" (privacy_model.md §5.6 L249-251).

**Schema simétrico grant ↔ revoke** (Lote 9.4 Opus H-05): tabela `consent_revocation` espelha `consent_ledger` 6-field — revoke carrega mesmo proof de notice em vigor no momento da revoga. **Why simetria**: GDPR Art. 7.3 exige revoke "as easy as giving consent"; sem proof simétrico, revoke é cryptographically weaker que grant → defensibility gap.

HMAC-SHA256 per-consent signature via tenant-scoped HKDF permite verify endpoint público stateless (titular pode auditar self-service sem auth — defensibility independente de DB state). Cascade unsubscribe ≤24h downstream notify garante CTRL-PRIV-CONSENT-002 satisfação. 2 CloudEvents canonical em audit-`<region>` Object Lock 7y CTRL-PRIV-CONSENT-003 + INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116.

```rust
// File: crates/corelink-privacy-consent-ledger/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait ConsentLedger: Send + Sync {
    /// POST /v1/consent/<purpose> — capture consent grant com 6-field proof.
    /// Idempotency: (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts) UNIQUE.
    async fn grant_consent(
        &self,
        tenant_ctx: &TenantCtx,
        subject_ctx: &SubjectCtx,
        purpose: ConsentPurpose,
        proof: ConsentProofPayload,                     // 6-field canonical
    ) -> Result<ConsentGrantReceipt, ConsentLedgerError>;

    /// DELETE /v1/consent/<purpose> — capture consent revoke com schema simétrico (Lote 9.4 H-05).
    /// Cascade unsubscribe ≤24h downstream notify; CTRL-PRIV-CONSENT-002 alignment.
    async fn revoke_consent(
        &self,
        tenant_ctx: &TenantCtx,
        subject_ctx: &SubjectCtx,
        purpose: ConsentPurpose,
        revoke_proof: ConsentProofPayload,              // 6-field; mirrors grant schema
    ) -> Result<ConsentRevokeReceipt, ConsentLedgerError>;

    /// GET /v1/consent — list all consents + history per subject.
    async fn list_consents(
        &self,
        tenant_ctx: &TenantCtx,
        subject_ctx: &SubjectCtx,
    ) -> Result<ConsentListResponse, ConsentLedgerError>;

    /// GET /v1/consent/verify?signature=X — público stateless verify grant.
    async fn verify_grant_signature(
        &self,
        signature: HmacSignature,
        verification_params: VerificationParams,
    ) -> Result<VerifyResponse, ConsentLedgerError>;

    /// GET /v1/consent/revocation/verify?revocation_id=X — público stateless verify revoke (Lote 9.4 H-05).
    async fn verify_revocation_signature(
        &self,
        revocation_id: RevocationId,
        signature: HmacSignature,
    ) -> Result<VerifyResponse, ConsentLedgerError>;
}

/// 6-field canonical proof payload per privacy_model.md §5.6 L255 CTRL-PRIV-CONSENT-001.
/// Schema usado SIMÉTRICO em grant + revoke (Lote 9.4 Opus H-05).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ConsentProofPayload {
    pub notice_text_hash: NoticeTextHash,               // SHA-256 hex64 do texto exato mostrado ao titular
    pub notice_version: SemverVersion,                  // semver "1.2.0" major bump force re-consent
    pub locale: LocaleBcp47,                            // "pt-BR" | "en-US" | "es-MX" — must match Accept-Language CTRL-PRIV-CONSENT-005
    pub wording_id: WordingId,                          // UUIDv7 do A/B test variant (e.g., "consent-analytics-v3")
    pub ui_capture_ts: DateTime<Utc>,                   // browser ts quando UI renderizou notice
    pub submission_ts: DateTime<Utc>,                   // server ts quando recebido (HuGR clock authoritative)
}

/// 12 canonical purposes (privacy_model.md §5.6.1 source-of-truth pós Lote 10.11.0-bis).
/// Cada purpose tem `legal_basis` fixo (NÃO swap dinamicamente — endereça GPT P0-1 round-1).
/// Ordem: 3 contract + 1 legal_obligation + 2 legitimate_interest + 6 consent.
#[derive(serde::Serialize, serde::Deserialize, strum::Display, strum::EnumIter, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConsentPurpose {
    // ──── basis = "contract" (LGPD Art. 7§V / GDPR 6(1)(b)) — não revogável sem encerrar contrato
    /// Operações core CAS/AC/exec (sem opt-out).
    ServiceDelivery,
    /// Auth, billing, tenant admin (sem opt-out).
    AccountManagement,
    // ──── basis = "legal_obligation" (LGPD Art. 7§II / GDPR 6(1)(c))
    /// Audit retention, DSR fulfillment, breach reporting, sub-processor notifications.
    RegulatoryCompliance,
    // ──── basis = "legitimate_interest" (LGPD Art. 10 / GDPR 6(1)(f)) — LIA req'd; objection respected
    /// Anomaly detection, abuse prevention, fraud prevention.
    SecurityMonitoring,
    /// Métricas anonimizadas cross-tenant (k≥50, privacy budget).
    AnalyticsAggregated,
    // ──── basis = "consent" (LGPD Art. 7§I / GDPR 6(1)(a)) — revogável ≤ 5min
    /// Per-tenant dashboards com PII reidentificável.
    AnalyticsPersonalized,
    /// Newsletter, product updates.
    MarketingEmail,
    /// Surveys, NPS, user research.
    MarketingResearch,
    /// Preview/experimental features (telemetry enriched).
    BetaFeatures,
    /// Stripe, GitHub, custom webhooks (data egress controlled; per-integration granular).
    ThirdPartyIntegrations,
    /// Share aggregated metrics em leaderboards/benchmarks.
    CrossTenantBenchmarks,
    /// Opt-in para ML cache prediction (data minimized + k-anon).
    TrainingMlModels,
}

/// Canonical legal basis enum (privacy_model.md §5.6.1 alinhamento).
/// Lote 10.11.0-bis: NO fail-open swap entre `consent` e `legitimate_interest` em consent expiry.
#[derive(serde::Serialize, serde::Deserialize, strum::Display, strum::EnumIter, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LegalBasis {
    Contract,
    LegalObligation,
    LegitimateInterest,
    Consent,
}

/// Mapping canonical purpose → legal_basis (fixed).
impl ConsentPurpose {
    pub fn legal_basis(&self) -> LegalBasis {
        match self {
            Self::ServiceDelivery
            | Self::AccountManagement                  => LegalBasis::Contract,
            Self::RegulatoryCompliance                 => LegalBasis::LegalObligation,
            Self::SecurityMonitoring
            | Self::AnalyticsAggregated                => LegalBasis::LegitimateInterest,
            Self::AnalyticsPersonalized
            | Self::MarketingEmail
            | Self::MarketingResearch
            | Self::BetaFeatures
            | Self::ThirdPartyIntegrations
            | Self::CrossTenantBenchmarks
            | Self::TrainingMlModels                   => LegalBasis::Consent,
        }
    }

    /// Revogável apenas se basis = consent. Endereça GPT P0-1 round-1.
    pub fn is_revocable(&self) -> bool {
        matches!(self.legal_basis(), LegalBasis::Consent)
    }
}

/// CloudEvents `type` field; canonical prefix `dev.hugr.corelink.consent.<verb>.v1` per Lote 10.9bis P0-G.
/// Aligned com privacy_model.md §5.6 L257 CTRL-PRIV-CONSENT-003 canonical.
#[derive(strum::Display, strum::EnumIter)]
pub enum ConsentAuditEventType {
    #[strum(serialize = "dev.hugr.corelink.consent.granted.v1")]
    Granted,
    #[strum(serialize = "dev.hugr.corelink.consent.revoked.v1")]
    Revoked,
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + customer cost protection + service protection discipline justification)

### 2.1 Contexto

GDPR Art. 7.1 exige consent "freely given, specific, informed, unambiguous"; LGPD Art. 8 § 1º espelha. EDPB Guidelines 5/2020 explica "informed" requer **proof** que titular viu (a) texto correto, (b) na versão correta, (c) no idioma correto, (d) em momento traceable. Empresas que armazenam apenas `(ts, principal, purpose, basis_legal)` provam só "houve um evento" — não "informed consent". Audit findings F-07 + G-07 do Lote 5/6 levaram à expansion do CTRL-PRIV-CONSENT-001 com 6-field payload canonical.

GDPR Art. 7.3 exige revoke "as easy as giving consent". Lote 9.4 Opus audit H-05 identificou gap: schema de revoke era weaker que grant (apenas `(ts, principal, purpose, action='revoke')`) → defensibility cryptographic gap. Solution: **schema simétrico** — `consent_revocation` espelha `consent_ledger` 6-field. Revoke carrega proof do notice em vigor no momento da revoga (geralmente "unsubscribe" wording_id distinto).

### 2.2 Abordagem

Neon schema 2 tables canonical (`consent_ledger` grant + `consent_revocation` revoke) com 6-field proof simétrico. HMAC-SHA256 per-record signature via tenant-scoped HKDF (security_model.md §374). 5 endpoints REST: 2 mutating (grant POST + revoke DELETE) + 2 verify públicos stateless (grant + revocation) + 1 list (auth). Cascade unsubscribe ≤ 24h via downstream notify (Cloudflare Queue fanout para sistemas que subscribed em consent purpose). 2 CloudEvents canonical em audit-`<region>` Object Lock 7y CTRL-PRIV-CONSENT-003. Idempotency via UNIQUE (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts) — replay-safe.

### 2.3 Valor entregue

- **GDPR Art. 7 + LGPD Art. 8 alignment absoluto**: 6-field proof of informed defensible em ANPD/Irish DPC.
- **Symmetric revoke (Lote 9.4 H-05)**: cryptographic strength simétrica grant ↔ revoke; revoke "as easy as giving" GDPR Art. 7.3.
- **CTRL-PRIV-CONSENT-001..006 satisfação**: opt-in proof + revogação ≤5min cascade ≤24h + audit immutable + LIA support + notice versioning enforcement + screenshot opcional.
- **INV-CONSENT-PROOF-VERIFIABLE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ symmetry) §3.12 L168**: notice_text_hash verifiable post-facto + tampering detected via signature.
- **Customer trust**: verify endpoint público stateless permite titular auditar consent record self-service sem nosso DB.

### 2.4 Principais riscos & trade-offs

- **HMAC vs digital signature (Ed25519)**: HMAC simpler tenant-scoped via HKDF; titular não pode verify cross-tenant (tenant key private). Trade-off: simplicidade vs portabilidade. **Decisão (DD-001)**: HMAC suffice para CoreLink scope (titular sempre tem same tenant context); migration to Ed25519 em S-19 se BYOK enterprise pedir.
- **Locale enforcement strict vs permissive**: CTRL-PRIV-CONSENT-005 exige locale match com Accept-Language; rigoroso = rejeita submit (better defensibility); permissivo = aceita best-effort com warning. **Decisão**: rigoroso (defensibility primary).
- **Cascade unsubscribe ≤ 24h**: downstream sistemas (S-13 admin plane, S-09 Loki labels) precisam notify. Risk de propagation lag → titular vê email após revoke. Mitigation: SLA 24h + SEV-2 alert se > 48h.
- **Notice versioning major bump force re-consent**: aggressivo (potential mass re-consent friction); permissive (e.g., 24h grace para titular re-confirmar) reduce friction mas gap regulatory. **Decisão (DD-002)**: strict force per-CTRL-PRIV-CONSENT-005.

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **Privacy-aware data subject**: opt-in/out controle granular per-purpose; verify proof self-service.
- **Marketing/Analytics teams interno**: query consent state via API (S-13 admin plane).
- **Privacy Officer**: revisão legal review LIA per CTRL-PRIV-CONSENT-004.
- **External auditor**: re-compute notice_text_hash + verify HMAC.

### 3.2 Customer journey (touchpoints)

1. Titular signup → notice em locale tenant.primary_region; UI checkboxes per-purpose default false (CTRL-PRIV-CONSENT-001).
2. Titular click "I agree" → UI POST /v1/consent/analytics com 6-field payload.
3. Server: HMAC sign + insert consent_ledger + emit `dev.hugr.corelink.consent.granted.v1` audit + return signature em receipt.
4. Titular pode anytime DELETE /v1/consent/analytics → cascade unsubscribe ≤24h.
5. Notice major version bump → email "Privacy notice updated; please re-consent"; old consent invalidated.
6. Titular pode GET /v1/consent → list com history + diffs per major version.

### 3.3 Jornadas (User Journeys) afetadas

- **Signup** (S-13): adiciona consent UI per-purpose com 6-field capture.
- **Settings**: link "Privacidade & meus consents" → list + revoke flows.
- **Privacy notice update** (WI-S11-004): trigger re-consent flow para affected subjects.

### 3.4 Métricas de customer-visible

- **Consent grant rate per purpose**: tracked via `corelink_consent_grant_total{purpose, locale, tenant_tier}`.
- **Revocation latency**: ≤ 5min p95 (CTRL-PRIV-CONSENT-002 SLA).
- **Cascade unsubscribe completion**: ≤ 24h p99.
- **Verify endpoint p95 latency**: ≤ 50ms (público stateless HMAC).

### 3.5 Comunicação ao customer

Email locale conforme tenant.primary_region: confirmação de grant + revoke + notice version major bump trigger. Templates 3 locales (PT-BR/EN/ES via WI-S11-004).

### 3.6 Mitigação de fricção

- **Notice version major bump**: 30d notice grace antes de force re-consent (titular tem tempo); cap a 90d hard.
- **Cascade ≤ 24h**: titular notificado se cascade > 24h via SEV-2 alert + email "downstream propagation delayed; manual review".
- **Verify endpoint público**: titular pode auditar sem precisar logar — proof permanente.

### Anti-pattern ❌

❌ Consent capture sem 6-field proof (GDPR Art. 7 violation); ❌ Revoke schema asimétrico (Lote 9.4 H-05 regression); ❌ HMAC sem tenant-scoping (cross-tenant signature forge); ❌ Notice version bump sem force re-consent (CTRL-PRIV-CONSENT-005 violation); ❌ Cascade unsubscribe sem audit trail (regulatory gap); ❌ Locale enforcement permissive (defensibility weak).

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-7 + R-S11-8 + R-S11-9 | Consent capture + revoke + list endpoints |
| CAPs | CAP-PRIV-003 | Consent ledger com proof of informed |
| Invariantes | INV-CONSENT-PROOF-VERIFIABLE (CRITICAL — Lote 10.11.0-bis §3.12 L168) + INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116) |
| Controles | CTRL-PRIV-CONSENT-001..006 (privacy_model.md §5.6 L255-260) + CTRL-AUDIT-002 + CTRL-CRYPTO-002 |
| Padrões | PAT-RETRY-IDEMPOTENT-001 (consent capture replay-safe) |
| Failure modes | **FM-452 (NEW S-11; declarado em WI-S11-002)** consent record tampering; FM-105 (inter-region latency cascade unsubscribe) |
| Métricas | `corelink_consent_grant_total{purpose,locale,tenant_tier}` + `corelink_consent_revoke_total{purpose,locale}` + `corelink_consent_revoke_latency_seconds{purpose}` + `corelink_consent_cascade_completion_seconds{purpose}` + `corelink_consent_verify_total{outcome}` |
| Eventos | 2 CloudEvents canonical: consent.granted/revoked |
| Evidence | EVT-049 (CONSENT_EVENT 7y) + EVT-026 (schema validate purpose_tag) + EVT-024 (load test revoke latency) + EVT-046 (LIA legal review) |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline` — implementação primária consent management.

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante; sem este WI, S-11 não satisfaz CTRL-PRIV-CONSENT-001..006.

### 5.3 Blast radius

**Tenant-isolated** via TenantCtx; bug em HMAC signing pode causar **forgery de consent record** (catastrofic, viola GDPR Art. 7).

### 5.4 Reversibilidade

Feature flag `consent_ledger_enabled` em DO config-singleton; flip → API returns 503 com **stop-processing** fail-CLOSED (Lote 10.11.0-bis-prime fix codex P0-4: NÃO fallback "legitimate interest temp valid" — viola legal_basis fixed per purpose canonical privacy_model.md §5.6.1). Cascade unsubscribe é state-mutating; rollback inviável post-emit; documented em DPA. Recovery: re-enable flag + reconciliation de consent state.

### 5.5 Experiment? (A/B test, feature flag experiment)

Apenas wording_id A/B em UI (S-16) — multiple wording_id valid em parallel; consent record carrega specific wording_id used. NÃO é experiment do consent system itself.

### 5.6 Compliance triggers

- LGPD Art. 8 (consentimento), Art. 7 § 5º (basis legal).
- GDPR Art. 6 (lawfulness), Art. 7 (conditions for consent), Art. 7.3 (revoke as easy as giving).
- CCPA/CPRA §1798.120 (opt-out of sale).
- SOC 2 P2.1 (consent).
- EDPB Guidelines 5/2020 (consent).
- ISO/IEC 27701 §6.10.2.4 (consent management).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. NEW crate `crates/corelink-privacy-consent-ledger`.
2. Neon schema migration: `consent_ledger` (canonical pós Lote 10.11.0-bis) + `consent_revocation` 2 tables 6-field symmetric (DDL §6.1.7).
3. 5 HTTP routes implementadas: POST /v1/consent/<purpose>, DELETE /v1/consent/<purpose>, GET /v1/consent, GET /v1/consent/verify, GET /v1/consent/revocation/verify.
4. ConsentPurpose enum 12 canonical values.
5. HMAC-SHA256 signing via tenant-scoped HKDF (security_model.md §374); separate key from JWT receipt key (key separation principle).
6. Verify endpoint stateless público (no DB lookup; HMAC re-derive).
7. Cascade unsubscribe ≤ 24h via Cloudflare Queue fanout to downstream subscribers (S-13 admin plane, S-09 Loki label dropper, S-10 Stripe metadata updater).
8. 2 CloudEvents canonical types em audit-`<region>` Object Lock 7y; emit fail-CLOSED (CTRL-PRIV-CONSENT-003 alignment).
9. Idempotency UNIQUE (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts) — replay-safe.
10. Notice version major bump force re-consent flow: pré-fetch consent_ledger; flag stale_consent=true se notice_version < current major; UI prompts re-consent; old consent NÃO deleted (audit trail preservado).
11. Locale enforcement: Accept-Language header obrigatório match com payload locale; mismatch → 422.
12. Screenshot evidence opcional (CTRL-PRIV-CONSENT-006): UI SDK pode upload PNG hash-addressed para R2 evidence-consent-screenshots/; consent_ledger.evidence_screenshot_hash NULL se opt-out.

#### 6.1.7 Neon schema migration `consent_ledger` (canonical pós Lote 10.11.0-bis) + `consent_revocation` symmetric

```sql
-- Neon migration N+3: consent_ledger (canonical pós Lote 10.11.0-bis; PG-side CHECK + FK enforcement) (CTRL-PRIV-CONSENT-001 + 003 alignment)
CREATE TABLE consent_ledger (
  consent_id                TEXT        PRIMARY KEY,                  -- ULID 26 chars
  tenant_id                 TEXT        NOT NULL,                     -- UUIDv7 hex32
  subject_id                TEXT        NOT NULL,                     -- UUIDv7 hex32
  subject_id_hash           TEXT        NOT NULL,                     -- sha256(subject_id || tenant_salt) audit redact
  purpose                   TEXT        NOT NULL CHECK (purpose IN ('analytics','marketing','email_transactional','beta_features','cross_product_data_sharing','third_party_integration','personalization','surveys_research','ai_training_data_contribution','public_profile','telemetry_export','sub_processor_notifications')),
  basis_legal               TEXT        NOT NULL CHECK (basis_legal IN ('contract','legal_obligation','legitimate_interest','consent')),  -- 4 canonical (privacy_model.md §5.6.1; vital/public_interest removidos pós Lote 10.11.0-bis-prime — não usados em CoreLink)
  -- 6-field proof of informed canonical (CTRL-PRIV-CONSENT-001)
  notice_text_hash          TEXT        NOT NULL,                     -- SHA-256 hex64
  notice_version            TEXT        NOT NULL,                     -- semver "1.2.0"
  locale                    TEXT        NOT NULL CHECK (locale IN ('pt-BR','en-US','es-MX')),  -- 3 canonical at GA
  wording_id                TEXT        NOT NULL,                     -- UUIDv7 do A/B variant
  ui_capture_ts             TEXT        NOT NULL,                     -- ISO 8601 UTC browser ts
  submission_ts             TEXT        NOT NULL,                     -- ISO 8601 UTC server ts
  -- HMAC signature for verify endpoint
  signature                 TEXT        NOT NULL,                     -- HMAC-SHA256 hex64 tenant-scoped
  signature_kid             TEXT        NOT NULL,                     -- key id for rotation; tenant_short_id derived
  -- Optional screenshot evidence (CTRL-PRIV-CONSENT-006)
  evidence_screenshot_hash  TEXT        NULL,                         -- R2 hash-addressed key
  -- Optional context
  ip_country                TEXT        NULL,                         -- ISO 3166-1 alpha-2 (NÃO ip raw — CTRL-PRIV-001)
  user_agent_class          TEXT        NULL,                         -- enum top-20 class (CTRL-PRIV-012)
  -- idempotency UNIQUE (replay-safe)
  UNIQUE (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts)
);

CREATE INDEX idx_consent_ledger_subject ON consent_ledger(tenant_id, subject_id, submission_ts DESC);
CREATE INDEX idx_consent_ledger_purpose ON consent_ledger(tenant_id, purpose, notice_version);

-- Neon migration N+3: consent_revocation (Lote 9.4 Opus H-05 schema simétrico; canonical pós Lote 10.11.0-bis)
CREATE TABLE consent_revocation (
  revocation_id             TEXT        PRIMARY KEY,                  -- ULID 26 chars
  tenant_id                 TEXT        NOT NULL,
  subject_id                TEXT        NOT NULL,
  subject_id_hash           TEXT        NOT NULL,
  purpose                   TEXT        NOT NULL CHECK (purpose IN ('analytics','marketing','email_transactional','beta_features','cross_product_data_sharing','third_party_integration','personalization','surveys_research','ai_training_data_contribution','public_profile','telemetry_export','sub_processor_notifications')),
  -- FK to grant being revoked (NULLable se titular revoga purpose nunca granted; defensive)
  revokes_consent_id        TEXT        NULL REFERENCES consent_ledger(consent_id),
  -- 6-field proof of informed REVOCATION (mirror schema)
  notice_text_hash          TEXT        NOT NULL,                     -- hash do unsubscribe notice em vigor
  notice_version            TEXT        NOT NULL,                     -- semver atual no momento da revoga
  locale                    TEXT        NOT NULL CHECK (locale IN ('pt-BR','en-US','es-MX')),
  wording_id                TEXT        NOT NULL,                     -- "unsubscribe-analytics-v3"
  ui_capture_ts             TEXT        NOT NULL,
  submission_ts             TEXT        NOT NULL,
  -- HMAC signature
  signature                 TEXT        NOT NULL,
  signature_kid             TEXT        NOT NULL,
  -- Cascade tracking
  cascade_started_at        TEXT        NOT NULL,                     -- triggered Cloudflare Queue fanout
  cascade_completed_at      TEXT        NULL,                         -- all downstream notified ≤24h SLA
  cascade_status            TEXT        NOT NULL DEFAULT 'pending' CHECK (cascade_status IN ('pending','in_progress','completed','partial_failure','failed')),
  ip_country                TEXT        NULL,
  user_agent_class          TEXT        NULL,
  -- idempotency UNIQUE
  UNIQUE (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts)
);

CREATE INDEX idx_consent_revocation_subject ON consent_revocation(tenant_id, subject_id, submission_ts DESC);
CREATE INDEX idx_consent_revocation_cascade ON consent_revocation(cascade_status, cascade_started_at) WHERE cascade_status != 'completed';
```

### 6.2 Componentes C4 afetados

- **Worker-CP**: novo route handlers em `crates/corelink-privacy-consent-ledger`.
- **Neon** (canonical pós Lote 10.11.0-bis): 2 novas tables (`consent_ledger` + `consent_revocation`); PG-side CHECK + FK + transactions ACID. NOT D1.
- **R2 audit-`<region>`**: 2 canonical CloudEvents types appended Object Lock 7y.
- **R2 evidence-consent-screenshots/** (NEW bucket; opcional CTRL-PRIV-CONSENT-006): hash-addressed PNG.
- **Cloudflare Queue** (S-09 inheritance): cascade fanout consumer/producer.
- **Cloudflare Email** (S-13 inheritance): grant/revoke confirmation templates.
- **Worker `corelink-rate-limit`** (S-08): rate limit policy consent submit (anti-spam: 100/dia/subject).

### 6.3 Arquivos do repositório

```
crates/corelink-privacy-consent-ledger/
├─ Cargo.toml
├─ src/
│  ├─ lib.rs                                  # ConsentLedger trait
│  ├─ http_routes.rs                          # 5 routes
│  ├─ schema.rs                               # ConsentProofPayload + ConsentPurpose enum
│  ├─ hmac_sign.rs                            # HMAC tenant-scoped HKDF
│  ├─ verify_stateless.rs                     # público verify endpoints
│  ├─ cascade.rs                              # ≤24h Cloudflare Queue fanout
│  ├─ idempotency.rs                          # UNIQUE quint constraint
│  ├─ notice_version_check.rs                 # major bump force re-consent flow
│  ├─ locale_enforce.rs                       # Accept-Language match
│  ├─ audit_emit.rs                           # 2 CloudEvents fail-CLOSED
│  └─ error.rs                                # ConsentLedgerError taxonomy
├─ migrations/
│  └─ N+3__consent_ledger_revocation.sql      # canonical DDL §6.1.7
└─ tests/
   ├─ integration_consent_lifecycle.rs        # signup → grant → revoke → cascade
   ├─ property_idempotency_replay.rs          # 100k iter replay-safe
   ├─ property_hmac_roundtrip.rs              # 100k iter sign + verify
   ├─ regression_symmetric_schema.rs          # Lote 9.4 H-05 grant ↔ revoke schema parity
   ├─ chaos_audit_emit_failure.rs             # fail-CLOSED behavior
   ├─ regression_locale_enforce.rs            # CTRL-PRIV-CONSENT-005
   └─ regression_notice_version_force_reconsent.rs # major bump
```

### 6.4 Sistemas externos tocados

- **Neon** (consent_ledger + consent_revocation tables canonical pós Lote 10.11.0-bis). NOT D1.
- **R2** (audit + evidence-consent-screenshots).
- **Cloudflare Queue** (cascade fanout).
- **Cloudflare Email** (transactional templates).
- **Worker `corelink-rate-limit`** (rate limit policy).

## 7. Anti-Scope

- ❌ DSR API HTTP layer — entregue em WI-S11-001 (consent_revoke é DSR sub-route mas usa este consent ledger).
- ❌ Erasure worker cross-backend — entregue em WI-S11-002.
- ❌ Privacy notice content + 3 locales — entregue em WI-S11-004 (this WI consome notice_text_hash).
- ❌ Sub-processor register — entregue em WI-S11-005.
- ❌ Breach notification — entregue em WI-S11-006.
- ❌ Residency pinning — entregue em WI-S11-007.
- ❌ DPIA + LIA + TLA+ — entregue em WI-S11-008 (TLA+ `dsr_erasure_atomicity.tla` includes `InvConsentSymmetry` action).
- ❌ Consent UI surface — entregue em S-16 frontend admin.
- ❌ A/B test framework de wording — anti-scope total CoreLink (apenas wording_id field carregado).
- ❌ Customer-controlled consent vault BYOK — deferred to S-14.

### Anti-pattern ❌

❌ Consent record sem 6-field proof; ❌ Revoke schema asimétrico (Lote 9.4 H-05 regression); ❌ HMAC sem tenant-scoping; ❌ Verify endpoint stateful (DB lookup público); ❌ Notice version bump silent (no re-consent flow); ❌ Cascade unsubscribe sem audit trail; ❌ Locale enforcement permissive.

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

### AC-001: Consent grant happy path com 6-field proof

```gherkin
Given um titular autenticado em tenant T com Accept-Language: pt-BR
When ele POSTa /v1/consent/analytics com payload {
  notice_text_hash: "sha256_of_pt-br_v1.2.0_text",
  notice_version: "1.2.0",
  locale: "pt-BR",
  wording_id: "consent-analytics-v3",
  ui_capture_ts: "2026-04-26T10:15:00Z",
  submission_ts: "2026-04-26T10:15:02Z",
  basis_legal: "consent"
}
Then a resposta é 201 com body { "consent_id": "<ULID>", "signature": "<HMAC-SHA256 hex64>", "verify_url": "/v1/consent/verify?signature=X&consent_id=<ULID>" }
And consent_ledger row inserida com all 14 fields populated
And `dev.hugr.corelink.consent.granted.v1` emitido em audit-`<region>` Object Lock 7y
And `corelink_consent_grant_total{purpose='analytics',locale='pt-BR',tenant_tier='free'}` incrementado
```

### AC-002: Schema simétrico grant ↔ revoke (Lote 9.4 H-05)

```gherkin
Given um titular tem consent grant existente para purpose='marketing' em tenant T
When ele DELETEa /v1/consent/marketing com payload {
  notice_text_hash: "sha256_of_unsubscribe_pt-br_v1.2.0_text",
  notice_version: "1.2.0",
  locale: "pt-BR",
  wording_id: "unsubscribe-marketing-v3",
  ui_capture_ts: "2026-04-26T11:00:00Z",
  submission_ts: "2026-04-26T11:00:02Z"
}
Then a resposta é 200 com body { "revocation_id": "<ULID>", "signature": "<HMAC>", "cascade_eta": "2026-04-27T11:00:02Z" }
And consent_revocation row inserida com mesmos 6-field proof + signature + revokes_consent_id FK
And schema parity: consent_ledger.notice_text_hash field type/length === consent_revocation.notice_text_hash field type/length (REGRESSION TEST verifies SQL DDL parity)
And `dev.hugr.corelink.consent.revoked.v1` emitido
And cascade_status='pending'; downstream queue fanout enqueued
```

### AC-003: Cascade unsubscribe ≤ 24h SLA

```gherkin
Given um consent_revocation row com cascade_status='pending', cascade_started_at = ts_T
When 24h após ts_T, cascade SLA cron sweep verifica cascade_status
Then se cascade_status == 'completed', `corelink_consent_cascade_completion_seconds{purpose}` histogram observed value ≤ 86400
And se cascade_status != 'completed' após 24h, SEV-2 alert disparado para Privacy Officer
And se cascade_status == 'completed' but downstream missed (e.g., S-13 admin plane delayed), SEV-3 + manual reconcile workflow
And p99 cascade completion ≤ 24h sustained 90d (CTRL-PRIV-CONSENT-002 SLA)
```

### AC-004: Verify endpoint público stateless (HMAC re-derive)

```gherkin
Given um signature `<sig>` válida emitida para consent_id `<id>` em tenant T
When alguém (sem auth) GET /v1/consent/verify?signature=<sig>&consent_id=<id>&tenant_short=<short>
Then a resposta é 200 com body {
  "valid": true,
  "consent_id": "<id>",
  "purpose": "<purpose>",
  "notice_version": "<semver>",
  "locale": "<locale>",
  "submission_ts": "<ts>",
  "tenant_short_id": "<short>"
}
But subject_id_hash exposto NÃO subject_id raw (CTRL-PRIV-014)
And NO DB lookup performed (HMAC re-derive only)
And se signature tampered ou kid mismatched, response 401 com error="invalid_signature"
And p95 latency ≤ 50ms (stateless HMAC compute)
```

### AC-005: HMAC tenant-scoping isolation

```gherkin
Given tenant T1 e T2 com signature keys distintas via HKDF
When um attacker tenta verify signature emitida em T1 contra T2
Then HMAC re-derive em T2 fails (key mismatch)
And response 401 com error="invalid_signature"
And property test 100k random (T1, T2) pairs: 0 cross-tenant signature validations
```

### AC-006: Notice version major bump force re-consent

```gherkin
Given um titular tem consent grant em notice_version='1.5.0' para purpose='analytics'
And privacy notice é atualizada para '2.0.0' (major bump per CTRL-PRIV-CONSENT-005)
When backend roda nightly stale_consent_check job
Then consent_ledger row antigo é flagged stale_consent=true (NÃO deleted; audit preserved)
And titular recebe email "Privacy notice updated; please re-consent within 30d grace"
And UI próxima visita prompts re-consent flow para todos affected purposes
And se titular não re-consent within 30d grace + 60d cap = 90d total, purpose entra em estado canonical `consent_lapsed` (privacy_model.md §5.6.1) — **processing PARA**; **NUNCA downgrade para legitimate_interest** (corrige GPT P0-1 round-1 reintroduzido em bis cycle 1; legal_basis fixed per purpose, no fail-open swap). Cascade unsubscribe synchronous + dados purpose-scoped marcados read-only até re-consent OR hard delete via DSR erasure.
```

### AC-007: Locale enforcement strict (CTRL-PRIV-CONSENT-005)

```gherkin
Given um titular submete consent com payload.locale='pt-BR'
And HTTP request header Accept-Language='en-US'
When server valida CTRL-PRIV-CONSENT-005 strict locale match
Then response 422 com body { "error": "locale_mismatch", "details": "Accept-Language header must match payload.locale; titular provavelmente viu notice em locale errado" }
And consent_ledger row NÃO inserida
And `corelink_consent_grant_total{purpose,locale='pt-BR',outcome='locale_mismatch'}` incrementado
And `dev.hugr.corelink.consent.granted.v1` NÃO emitido (rejection NÃO é grant)
```

### AC-008: Idempotency replay-safe consent submit

```gherkin
Given um titular submeteu grant analytics com proof P em tenant T
When ele submete novamente o mesmo grant analytics com proof P em tenant T
Then UNIQUE constraint (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts) dispara → 200 (NÃO 201) com body { "consent_id": "<existing ULID>", "replay": true, "signature": "<same HMAC>" }
And consent_ledger row NÃO duplicada
And `dev.hugr.corelink.consent.granted.v1` NÃO re-emitted
But se proof P' ≠ P (e.g., novo wording_id + ui_capture_ts), nova row inserida (legitimate re-consent flow)
```

### AC-009: Audit fail-CLOSED em emit failure

```gherkin
Given audit emit infrastructure (R2 audit-`<region>`) está temporariamente indisponível
When um titular submete consent grant
Then consent_ledger Neon INSERT é executed primeiro (idempotency guard)
But audit emit fails → transaction rollback via Neon transaction atomic (canonical pós Lote 10.11.0-bis)
And response 503 com retry-after=60s
And SEV-2 alert disparado (HIGH severity per INV §3.6 L116 CRITICAL escalates SEV-1 only post-incident classification)
And distinct from WI-S10-001 billing fail-OPEN (split-tier discipline; ADR-S11-002 cross-WI)
```

### AC-010: Public verify list consent history per subject

```gherkin
Given um titular auth-ed em tenant T com 5 grants + 2 revokes históricos
When ele GET /v1/consent
Then response 200 com body { "consents": [<5 grant rows>], "revocations": [<2 revoke rows>], "current_state": [{ purpose, granted_or_revoked, last_updated_ts }] }
And history ordered submission_ts DESC
And para cada record, signature + verify_url retornados
And titular pode use verify_url cada um para self-service audit
```

## 9. Design Decisions

### 9.1 Decisões locais

- **DD-001 HMAC vs Ed25519**: HMAC simpler tenant-scoped via HKDF; Ed25519 BYOK enterprise feature → S-19 future. CTRL-PRIV-CONSENT-003 satisfeito por either.
- **DD-002 Notice version major bump strict force re-consent**: 30d grace + 60d cap = 90d total; após = purpose entra `consent_lapsed` state canonical (privacy_model.md §5.6.1; processing PARA). **NUNCA fallback para legitimate_interest** — corrige GPT P0-1 round-1: legal_basis é fixed per purpose, no dynamic swap. Privacy Officer manual review apenas para break-glass extension (cap absoluto 180d com ANPD/EDPB pre-notification).
- **DD-003 Locale enforcement strict (não permissive)**: CTRL-PRIV-CONSENT-005 alignment; titular precisa estar consciente do idioma. Permissive seria defensibility weak.
- **DD-004 12 canonical purposes (não unlimited string)**: closed enum aligned com `failure_modes.md` cardinality discipline (post Lote 10.10-quaters); novos purposes requerem ADR.
- **DD-005 Cascade ≤ 24h via Cloudflare Queue (não synchronous)**: synchronous fanout blocks revoke endpoint p95 latency CTRL-PRIV-CONSENT-002 ≤ 5min; Queue async + 24h SLA é industry standard.

### 9.2 Decisões que justificam ADR

- **ADR-S11-005 (NEW)**: Schema simétrico grant ↔ revoke (Lote 9.4 Opus H-05). Rationale: GDPR Art. 7.3 exige revoke "as easy as giving"; cryptographic strength simétrica; defensibility independente em ANPD/Irish DPC. Alternative (assymmetric) considered + rejected (defensibility weak).
- **ADR-S11-006 (NEW)**: 12 canonical ConsentPurpose enum closed (cardinality discipline). Rationale: novos purposes downstream impact (cascade fanout subscribers, audit retention scope, DPA references); ADR per new purpose forces deliberate analysis.

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| Signature primitive | HMAC-SHA256 | Ed25519 | A | Simpler tenant-scoped; BYOK migration path S-19 |
| Cascade sync vs async | Synchronous fanout | Async ≤24h SLA | B | CTRL-PRIV-CONSENT-002 ≤5min revoke endpoint p95 |
| Locale enforcement | Strict | Permissive | Strict | CTRL-PRIV-CONSENT-005 defensibility |
| Notice version bump | Force re-consent | Auto-renew | Force | CTRL-PRIV-CONSENT-005 + GDPR Art. 7 |
| Purpose enum | Closed 12 | Open string | Closed | Cardinality + ADR discipline |
| Revoke schema | Asymmetric | Symmetric (Lote 9.4 H-05) | Symmetric | GDPR Art. 7.3 defensibility |

### Anti-pattern ❌

❌ Asymmetric schema grant/revoke (Lote 9.4 H-05 regression); ❌ Synchronous cascade (latency violation); ❌ Permissive locale; ❌ Open string purpose; ❌ Auto-renew sem re-consent.

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** Crate compila zero warnings em `cargo build --target wasm32-unknown-unknown`.
- [ ] **C-1.2** 5 HTTP routes com path matching.
- [ ] **C-1.3** Neon migration N+3 aplica + roll-back; schema simétrico grant/revoke parity verified (canonical pós Lote 10.11.0-bis).
- [ ] **C-1.4** HMAC signing via HKDF wraps `crates/corelink-keys` (key_management.md inheritance).
- [ ] **C-1.5** Cascade Cloudflare Queue producer + consumer integrated.
- [ ] **C-1.6** Audit emit fail-CLOSED 2 CloudEvents canonical types.
- [ ] **C-1.7** No `tokio::spawn` em CF Workers; `worker::send_future` para cascade fire-and-forget.
- [ ] **C-1.8** Locale enforce strict middleware.
- [ ] **C-1.9** Notice version major bump nightly job em S-09 cron worker inheritance.

### 10.2 Test Completeness

- [ ] **T-2.1** Integration test consent lifecycle (signup → grant → revoke → cascade → verify) verde.
- [ ] **T-2.2** Property test idempotency 100k iter replay-safe.
- [ ] **T-2.3** Property test HMAC roundtrip 100k iter (sign + verify match).
- [ ] **T-2.4** Regression test schema simétrico grant ↔ revoke parity (Lote 9.4 H-05).
- [ ] **T-2.5** Chaos test audit emit failure: fail-CLOSED behavior assertion.
- [ ] **T-2.6** Regression test locale enforcement strict (Accept-Language match).
- [ ] **T-2.7** Regression test notice version major bump force re-consent flow.
- [ ] **T-2.8** Cross-tenant HMAC isolation: 100k random (T1, T2) pairs → 0 cross-tenant validations.
- [ ] **T-2.9** Cascade SLA test: ≤24h p99 simulated em staging tempo-acelerado.
- [ ] **T-2.10** Schema parity test: SQL DDL diff `consent_ledger` 6-field block === `consent_revocation` 6-field block (CI lint).

### 10.3 Documentation Completeness

- [ ] **D-3.1** `docs/api/privacy-consent.md` OpenAPI 3.1 com 5 endpoints.
- [ ] **D-3.2** `docs/dev/consent-ledger-architecture.md` com diagrama symmetric schema.
- [ ] **D-3.3** ADR-S11-005 (schema simétrico) + ADR-S11-006 (purpose enum closed) committed.
- [ ] **D-3.4** Privacy notice WI-S11-004 referencia este endpoint.

### 10.4 Observability Completeness

- [ ] **O-4.1** 5 Prom metrics: grant_total, revoke_total, revoke_latency, cascade_completion, verify_total.
- [ ] **O-4.2** 1 dashboard `corelink-consent-ledger` em Grafana (S-09 inheritance) com 8 panels.
- [ ] **O-4.3** 2 CloudEvents types em audit-`<region>` Object Lock 7y.
- [ ] **O-4.4** 1 trace per consent submit/revoke (W3C tracecontext).

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** subject_id NUNCA em logs (CTRL-PRIV-014); apenas subject_id_hash.
- [ ] **S-5.2** HMAC key tenant-scoped via HKDF; key separation from JWT receipt key.
- [ ] **S-5.3** CTRL-PRIV-CONSENT-001..006 wired completamente (validators).
- [ ] **S-5.4** GDPR Art. 7 + LGPD Art. 8 alignment validated em legal review pre-merge.
- [ ] **S-5.5** Cross-tenant HMAC: 0 leaks em 100k property test.
- [ ] **S-5.6** DLP scan CI gate: 0 raw PII em logs/metrics/traces.

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui `corelink-privacy-consent-ledger`.
- [ ] **B-6.2** Sigstore provenance attestation.

## 11. DoD

10.x checked + sign-off matrix §30 12 confirmados + chaos 30d staging + 100k property test verde.

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-003 |
|---|---|---|---|
| **INV-CONSENT-PROOF-VERIFIABLE** | CRITICAL (Lote 10.11.0-bis: TLA+ symmetry) | invariant_registry.md §3.12 L168 | 6-field proof + HMAC signature + verify endpoint stateless público; tampering detected via signature mismatch |
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | 2 CloudEvents canonical em audit-`<region>` Object Lock 7y; emit fail-CLOSED |

## 13. Artifacts Produced

- `crates/corelink-privacy-consent-ledger/` (NEW; ~3000 LoC).
- `migrations/N+3__consent_ledger_revocation.sql` (DDL + indexes).
- `docs/api/privacy-consent.md` OpenAPI 3.1.
- `docs/dev/consent-ledger-architecture.md`.
- ADR-S11-005 (schema simétrico) + ADR-S11-006 (purpose enum closed).
- 2 CloudEvents schemas em `schemas/cloudevents/consent-{granted,revoked}.v1.json`.
- Grafana dashboard JSON `dashboards/corelink-consent-ledger.json`.
- 5 Prom metrics + alerts.

## 14. Quality Standards SOTA

- **14.s11.3.1** GDPR Art. 7 + LGPD Art. 8 alignment legal review pre-merge.
- **14.s11.3.2** Schema simétrico grant ↔ revoke (Lote 9.4 H-05) lint CI gate.
- **14.s11.3.3** HMAC tenant-scoped 100k cross-tenant property test 0 leaks.
- **14.s11.3.4** Cascade ≤24h p99 sustained 90d.
- **14.s11.3.5** Locale enforcement strict (CTRL-PRIV-CONSENT-005).
- **14.s11.3.6** Notice version major bump force re-consent flow (CTRL-PRIV-CONSENT-005).
- **14.s11.3.7** Verify endpoint p95 ≤ 50ms (stateless).
- **14.s11.3.8** INV §3.X positions canonical verified pre-merge (Lote 10.8bis P1-13).

## 15. Chaos Experiments (8)

1. R2 audit unavailable → consent submit returns 503 (fail-CLOSED).
2. Neon unavailable → 503 com retry-after (canonical pós Lote 10.11.0-bis).
3. Cloudflare Queue cascade backlog → SEV-2 alert se > 24h SLA.
4. HMAC key rotation mid-flight → both old + new keys aceitos por 24h grace (kid-based).
5. Notice version major bump nightly job failure → SEV-2 alert; manual sweep next morning.
6. Cross-tenant signature forgery attempt → 401 + audit `consent.verify_failed.v1` event.
7. Locale enforcement bypass attempt → 422 + alert.
8. Cascade downstream consumer offline (e.g., S-13 admin plane) → cascade_status='partial_failure' + SEV-3.

## 16. PRR

PRR HIGH_RISK 12 sign-offs + chaos 30d + DLP scan + Privacy + Compliance + Architect + DPO mandatory emphatic.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | Crate scaffold + ConsentLedger trait | 1h |
| ST-002 | Neon migration N+3 (2 tables symmetric) + parity tests (canonical pós Lote 10.11.0-bis) | 1.5h |
| ST-003 | ConsentProofPayload + 12 ConsentPurpose enum | 1h |
| ST-004 | 5 HTTP routes implementation | 2.5h |
| ST-005 | HMAC signing tenant-scoped HKDF + key separation | 1.5h |
| ST-006 | Verify endpoint stateless público | 1h |
| ST-007 | Cascade Cloudflare Queue fanout | 2h |
| ST-008 | Idempotency UNIQUE quint + tests | 1h |
| ST-009 | Notice version major bump nightly job | 1h |
| ST-010 | Locale enforcement middleware | 0.7h |
| ST-011 | Audit emit 2 CloudEvents fail-CLOSED | 1h |
| ST-012 | OpenAPI 3.1 + architecture doc + 2 ADRs | 1.5h |
| ST-013 | 5 Prom metrics + Grafana dashboard | 1h |

**PERT total**: ~16.7h (alinha com sprint contract §12).

## 18. Dependencies

- **Hard**: S-03 SEALED (PAT auth); S-09 SEALED (audit emit + Cloudflare Queue); S-08 SEALED (rate limit policy); spec contract S-11 v1.2.0 SEALED.
- **Soft**: WI-S11-001 (DSR consent_revoke endpoint chama este consent ledger DELETE); WI-S11-004 (privacy notice locales generate notice_text_hash inputs); WI-S11-008 (TLA+ `dsr_erasure_atomicity.tla` includes `InvConsentSymmetry` action validates schema simétrico).

## 19. Effort PERT: ~16.7h. ## 20. Time-boxing: 24h hard limit (lane HIGH_RISK +40% buffer).

## 21. Observability

5 Prom metrics + 1 Grafana dashboard 8 panels + 2 CloudEvents canonical + W3C trace exemplars.

## 22. Cost Analysis

- **Neon consent tables** (canonical pós Lote 10.11.0-bis): low-volume (~20/month/tenant em early stage); negligible cost; PG-side CHECK + FK protege regulatory invariants.
- **R2 audit-`<region>`**: 2 events/consent action; 7y Object Lock; ≈$0.005/tenant/year.
- **R2 evidence-consent-screenshots/** (CTRL-PRIV-CONSENT-006 opcional): hash-addressed; ~50KB/screenshot; ≈$0.001/screenshot.
- **Cloudflare Queue cascade**: 1 message/revoke × N downstream subscribers; ≈$0.001/revoke.
- **HMAC compute**: amortized; negligible.
- **Total estimated**: ≤ $0.05/tenant/year.

## 23. API Contract

OpenAPI 3.1 em `docs/api/privacy-consent.md`. 2 CloudEvents schemas em `schemas/cloudevents/consent-*.v1.json`. ConsentProofPayload schema em `schemas/json/consent-proof-payload.json`.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| FM-452 consent record tampering detected | HIGH | Privacy Officer + SecLead + Compliance |
| INV-CONSENT-PROOF-VERIFIABLE violation | CRITICAL (Lote 10.11.0-bis-prime: HIGH→CRITICAL severity cascade) | Privacy Officer + Architect; **SEV-1 alert** |
| Cascade SLA miss > 24h sustained | HIGH | Privacy Officer + SRE |
| HMAC key compromise | CRITICAL | SecLead + Privacy + Compliance |
| Schema simétrico drift (grant ≠ revoke schema) | HIGH | Architect + Privacy + Lote 9.4 H-05 review |
| Notice version major bump silent (re-consent flow miss) | HIGH | Privacy Officer + Compliance |
| Locale enforcement bypass exploit | HIGH | SecLead + Privacy |
| Audit fail-OPEN regression | CRITICAL | Architect + Privacy + Compliance |

## 25. Rollback / Recovery

Feature flag `consent_ledger_enabled` em DO config-singleton; flip → API 503 com **stop-processing** (fail-CLOSED canonical pós Lote 10.11.0-bis; corrige GPT P0-1 round-1: NÃO há fallback "legitimate_interest temp valid" — consent_ledger off-line significa que processing baseado em consent PARA até cure). Recovery: re-enable flag + reconciliation de consent state. NUNCA processing under fallback basis.

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): subject_id_hash em audit (CTRL-PRIV-014).
- I(dentifiability): HMAC tenant-scoped impede cross-tenant correlation.
- N(on-repudiation): R2 Object Lock 7y + signature verifiable.
- D(etectability): verify endpoint stateless permite audit independent.
- D(isclosure): notice_text_hash exposes structure but não content; signature kid public.
- U(nawareness): privacy notice (WI-S11-004) explica consent purposes + revoke flow.
- N(on-compliance): **GDPR Art. 7 + LGPD Art. 8 + CCPA §1798.120 + SOC 2 P2.1 + EDPB 5/2020 compliance** via 6-field proof + symmetric schema + verify endpoint + 7y audit.

## 27. Knowledge Transfer

Tech talk (1h): "S-11 Consent Ledger: 6-field proof of informed + symmetric schema (Lote 9.4 H-05) + HMAC tenant-scoped + cascade ≤24h"; doc `docs/dev/consent-ledger-architecture.md`; onboarding test 6 questões: 6-field proof rationale (privacy_model.md §5.6 G-07), schema simétrico GDPR Art. 7.3, HMAC vs Ed25519 trade-off (DD-001), notice version major bump force re-consent (CTRL-PRIV-CONSENT-005), 12 canonical purposes closed enum (ADR-S11-006), cascade ≤24h SLA via Cloudflare Queue.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-CONSENT-PROOF-VERIFIABLE violation (signature forge) | L | M | HIGH | M | LOW | HMAC tenant-scoped HKDF + cross-tenant property test 100k; SEV-2 alert |
| R-002 | Schema simétrico drift (grant ≠ revoke) | L | M | HIGH | M | LOW | DDL parity CI lint + regression test (Lote 9.4 H-05) |
| R-003 | Cascade SLA miss > 24h | M | L | HIGH | M | LOW | Cloudflare Queue + SEV-2 alert + manual reconcile workflow |
| R-004 | Notice version major bump silent | L | L | HIGH | L | LOW | Nightly stale_consent_check job + email re-consent prompt |
| R-005 | Locale enforcement bypass | L | M | MEDIUM | L | LOW | Strict middleware + 422 + audit |
| R-006 | HMAC key compromise | L | L | CRITICAL | L | LOW | Tenant-scoped via HKDF; key rotation S-19; revocation deferred |
| R-007 | Cross-tenant signature forge | L | L | CRITICAL | L | LOW | HMAC tenant-scoped; property test 100k pairs |
| R-008 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.6 L116 + §3.12 L168 verified Lote 10.11.0 |
| R-009 | Audit fail-OPEN regression (Lote 10.6bis violation) | L | L | CRITICAL | L | LOW | ADR-S11-002 cross-WI; integration test |
| R-010 | Idempotency cache miss → duplicate consent | L | L | LOW | L | LOW | UNIQUE quint constraint + chaos test |
| R-011 | Cascade downstream consumer offline | L | M | LOW | L | LOW | cascade_status='partial_failure' + SEV-3; manual reconcile |
| R-012 | Wording_id A/B test mismatch (UI ≠ DB) | L | L | LOW | L | LOW | wording_id em UI build pinned; CI lint |

## 29. Review Checkpoints

D+0 Architect (symmetric schema + HMAC); D+1 Privacy Officer (LINDDUN + GDPR Art. 7); D+2 Legal (DPA reference); D+3 SecLead (HMAC tenant-scoping + STRIDE); D+4 Compliance (SOC 2 P2.1 + EDPB 5/2020); D+5 code review; D+6 PRR.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ |
| 4 | Security Lead | _TBD; **mandatory** — HMAC + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — 100k property test + schema parity_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 P2.1 + GDPR Art. 7 + EDPB 5/2020_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — CTRL-PRIV-CONSENT-001..006 + Lote 9.4 H-05 schema simétrico_ |
| 11 | Architect | _TBD; **mandatory emphatic** — ADR-S11-005/006 + INV §3.X verification_ |
| 12 | DPO interim | _TBD; **mandatory emphatic** — proof of informed defensibility ANPD/DPC_ |

(Legal sign-off via DPA reference at sprint level; not per-WI.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **ConsentPurpose 12-enum aligned com privacy_model.md §5.6.1 canonical** (NEW section em arquitetura) — substituindo nomes locais v1.0 (Analytics → AnalyticsAggregated/Personalized; Marketing → MarketingEmail/Research; SubProcessorNotifications removido pois é mandatory regulatory_compliance basis NÃO consent). Ordem canonical: 3 contract + 1 legal_obligation + 2 legitimate_interest + 6 consent. (b) **NEW LegalBasis enum** + impl `legal_basis()` + `is_revocable()` per purpose — endereça GPT P0-1 round-1 (NO fail-open swap entre consent e legitimate_interest em consent expiry; purpose entra `consent_lapsed` state). (c) **HKDF info canonical** = `corelink/v1/consent-hmac` (security_model.md §7.2). (d) **INV-CONSENT-PROOF-VERIFIABLE HIGH→CRITICAL** + TLA+ symmetry InvConsentSymmetry (WI-S11-008). |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-003; HIGH_RISK; SOTA pós-S-10 SEALED. **Proof-of-informed regulatory cornerstone** GDPR Art. 7 + LGPD Art. 8 alignment. Neon schema 2 tables canonical (`consent_ledger` + `consent_revocation`) com **6-field proof simétrico** (notice_text_hash + notice_version + locale + wording_id + ui_capture_ts + submission_ts) per Lote 9.4 Opus H-05 — schema parity grant ↔ revoke essencial defensibility GDPR Art. 7.3 "as easy as giving". 5 endpoints REST: 2 mutating (POST /v1/consent/<purpose> + DELETE /v1/consent/<purpose>) + 2 verify públicos stateless (grant + revocation) + 1 list. HMAC-SHA256 tenant-scoped via HKDF (security_model.md §374); key separation from JWT receipt key (key separation principle). Verify endpoint stateless público (no DB lookup; HMAC re-derive). Cascade unsubscribe ≤ 24h SLA via Cloudflare Queue fanout downstream subscribers (CTRL-PRIV-CONSENT-002 alignment). 2 CloudEvents canonical `dev.hugr.corelink.consent.{granted,revoked}.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y CTRL-PRIV-CONSENT-003. Idempotency UNIQUE (tenant_id, subject_id, purpose, notice_text_hash, ui_capture_ts) replay-safe. 12 canonical ConsentPurpose enum closed (ADR-S11-006 cardinality discipline). Locale enforcement strict (CTRL-PRIV-CONSENT-005 — Accept-Language match payload locale). Notice version major bump force re-consent flow (30d grace + 60d cap = 90d total). INV-CONSENT-PROOF-VERIFIABLE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ symmetry) §3.12 L168 satisfaction + INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116 cobertura. CTRL-PRIV-CONSENT-001..006 wiring completo (privacy_model.md §5.6 L255-260). 10 AC scenarios + 8 chaos + 12 risks + 8 post-mortem hooks. NEW ADR-S11-005 (schema simétrico Lote 9.4 H-05) + ADR-S11-006 (purpose enum closed). **Lote 10.10 lessons absorbed**: source-of-truth FIRST INV positions verified pre-merge; typed enum (12 purposes closed); sign-off cap 12; cascade discipline absoluta; split-tier audit fail-CLOSED; corelink_time canonical helper; HMAC HKDF inheritance from key_management.md. |

## 32. Anti-patterns evitados

- ❌ Asymmetric schema grant/revoke (Lote 9.4 H-05 regression); ❌ HMAC sem tenant-scoping (cross-tenant forge); ❌ Verify endpoint stateful (DB lookup público vaza state); ❌ Notice version bump silent (CTRL-PRIV-CONSENT-005 violation); ❌ Open string purpose (cardinality drift); ❌ Synchronous cascade (latency violation); ❌ Permissive locale (defensibility weak); ❌ Audit fail-OPEN (Lote 10.6bis split-tier violation); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ INV §3.X position TBD (Lote 10.8bis P1-13); ❌ subject_id raw em logs (CTRL-PRIV-014).

---
