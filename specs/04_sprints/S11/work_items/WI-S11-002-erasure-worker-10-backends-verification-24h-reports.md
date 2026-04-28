---
id: "WI-S11-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-26"
updated: "2026-04-28"
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
  - "STORAGE-SEMANTICS-MATRIX"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "KEY-MANAGEMENT"
tags: ["wi", "s11", "erasure", "dsr", "lgpd-art-18", "gdpr-art-17", "cross-backend", "object-lock-pseudonymization", "edpb-5-2020", "high-risk"]
---

# WI-S11-002 — DSR Erasure Worker Cross-Backend (12 Backends Canonical Lote 10.11.0-bis: 8 Effective + 4 Pseudonymized via legal_hold) + Verification Job 24h Sweep + Report R2 evidence-dsr 7y + Tombstones + FM-450/452 Declarations (`crates/corelink-privacy-erasure-worker`; consume queue de WI-S11-001 com `dsr.queued.v1` payload; orchestra erasure cross-backend canonical 12 (8 effective: Neon multi-tabela + Neon billing fiscal exception + R2 CAS refcount-aware + R2 AC + D1 + KV + Stripe Customer.update + Loki — privacy_model.md §6.2 source-of-truth pós Lote 10.11.0-bis) + pseudonymization 4 canonical Lote 10.11.0-bis (R2 audit Object Lock 7y / Neon PITR backup 30d / R2 CAS legal_hold partition governance mode / R2 evidence-* buckets 7y — privacy_model.md §6.2 source-of-truth; Object Lock WORM regulatory immutability GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization)); INV-DATA-ERASURE-COMPLETE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008) §3.5 L110 satisfaction; verification job 24h sweep posts EVT-048 DSR_EVIDENCE em R2 evidence-dsr/<dsr_id>/erasure-report.json retain 7y (canonical); tombstone em D1 `dsr_erasure_log` por backend; emit fail-CLOSED audit per backend completion; SLO-FRESH-DSR-ERASURE 99% ≤30d sustainment; **NEW FM-450 erasure-incomplete cross-backend + FM-452 consent-tampering-detected** declared em failure_modes.md; idempotency replay-safe via dsr_id ULID + per-backend idempotency_key 35-char; PAT-RETRY-IDEMPOTENT-001 erasure replays 100×)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-002 |
| Título | Erasure worker 12 backends canonical Lote 10.11.0-bis (8 effective + 4 pseudonymized via legal_hold) — extends privacy_model.md §6.2 reescrita pós Lote 10.11.0-bis como source-of-truth: 12 backends canonical (8 effective + 4 pseudonymized via legal_hold); pseudonymization rule `tenant_id_hash = sha256(tenant_id \|\| erasure_salt)` + marker `pii_redacted=true` per GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization) escape valve regulatório vs INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116 7y immutability; verification job 24h sweep posts per-backend status enum `erased\|pseudonymized\|partial_failure\|failed\|not_applicable` em EVT-048 DSR_EVIDENCE 7y retention canonical; tombstone D1 `dsr_erasure_log` per backend; CloudEvents `dev.hugr.corelink.dsr.erasure.{started,backend_completed,verification_passed,verification_failed,completed}.v1` 5 canonical types per Lote 10.9bis P0-G prefix; idempotency `(dsr_id, backend)` UNIQUE em D1 staging table per-backend dedup; replay-safe via PAT-RETRY-IDEMPOTENT-001 erasure replays 100× (sprint contract §9 14.s11.4) |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII direct exposure regulatory), FF-HR-005 (CTRL-PRIV-030 erasure pipeline), FF-HR-010 (1ª regulatory full impl LGPD/GDPR/CCPA) |

## 1. Intent

DSR Erasure Worker é **o coração regulatory** do CoreLink Privacy Pipeline — sem isso, **INV-DATA-ERASURE-COMPLETE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008) §3.5 L110** é violated by design e LGPD Art. 18 IV / GDPR Art. 17 ficam unfulfilled (multa 2-4% revenue). Worker consome queue de WI-S11-001 com `dsr.queued.v1` payload e orquestra erasure cross-backend em **12 backends canonical Lote 10.11.0-bis** (privacy_model.md §6.2 source-of-truth):
**8 effective slots** (erasure física possível): (1) Neon multi-tabela (`dsr_tickets/account/tenant/user_account/consent_ledger/subscription`), (2) Neon billing fiscal exception sob legal_hold, (3) R2 CAS refcount-aware (subject_unaffiliated vs subject_dedicated), (4) R2 AC, (5) D1 (`blob_meta/ac_meta`), (6) KV session+metadata, (7) Stripe `Customer.update` (PII nullified, NOT delete — PCI scope), (8) Loki/Grafana log deletion API.
**4 pseudonymized slots** (legal_hold canonical, retention obrigatório): (i) R2 audit Object Lock 7y (HKDF info=`corelink/v1/audit-pseudonym`), (ii) Neon PITR backup 30d (tombstone replay em restore), (iii) R2 CAS legal_hold partition governance mode, (iv) R2 evidence-* buckets 7y → aplica **pseudonymization** per GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization) escape valve regulatório.

Verification job 24h sweep posta per-backend status em EVT-048 DSR_EVIDENCE 7y retention canonical. Tombstones em D1 `dsr_erasure_log` per backend permitem audit forensic. Emit fail-CLOSED audit per backend completion (split-tier discipline Lote 10.6bis: erasure events são regulatory-grade NÃO toleram silent loss; distinto de WI-S10-001 billing fail-OPEN).

```rust
// File: crates/corelink-privacy-erasure-worker/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait ErasureWorker: Send + Sync {
    /// Consume `dsr.queued.v1` from queue → orchestrate cross-backend erasure.
    /// Idempotency: (dsr_id, backend) UNIQUE em dsr_erasure_log;
    /// replay-safe (PAT-RETRY-IDEMPOTENT-001 — sprint contract §9 14.s11.4).
    async fn process_erasure(
        &self,
        dsr_event: DsrQueuedEvent,
    ) -> Result<ErasureProcessReport, ErasureWorkerError>;

    /// Per-backend erasure step; encapsulates effective vs pseudonymized branch.
    /// Audit emit fail-CLOSED on `dev.hugr.corelink.dsr.erasure.backend_completed.v1`.
    async fn erase_backend(
        &self,
        dsr_id: DsrId,
        tenant_id: TenantId,
        subject_id: SubjectId,
        backend: ErasureBackend,
    ) -> Result<BackendErasureOutcome, ErasureWorkerError>;

    /// Verification job: 24h post-erasure-completed sweep all 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis).
    /// Posts EVT-048 DSR_EVIDENCE em R2 evidence-dsr/<dsr_id>/erasure-report.json retain 7y (canonical).
    async fn verify_erasure(
        &self,
        dsr_id: DsrId,
    ) -> Result<VerificationReport, ErasureWorkerError>;
}

/// 12 canonical backends pós Lote 10.11.0-bis (privacy_model.md §6.2 source-of-truth).
/// Effective (8) + Pseudonymized (4) — total 12 canonical entries.
#[derive(strum::Display, strum::EnumIter, serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum ErasureBackend {
    // ===== Effective backends (8 canonical pós Lote 10.11.0-bis) =====
    /// Neon multi-tabela (`dsr_tickets/account/tenant/user_account/consent_ledger/subscription`):
    /// SQL DELETE WHERE subject_user_id; PG-side CHECK + FK enforcement; tombstone em dsr_erasure_log.
    NeonMain,
    /// Neon billing fiscal exception (`invoice/usage_event`): preserve legal_hold=true rows
    /// (LGPD Art. 16 fiscal 5y); pseudonymize PII em retained rows; full DELETE só pós retention expiry.
    NeonBilling,
    /// R2 CAS refcount-aware (CAS blobs mutable):
    /// subject_unaffiliated → decrement refcount apenas (blob shared);
    /// subject_dedicated → tombstone + GC sweep grace 72h (privacy_model.md §6.2).
    R2Cas,
    /// R2 AC (Action Cache mutable): DELETE entries WHERE owner_tenant_id; per-region pinned.
    R2Ac,
    /// D1 (`blob_meta/ac_meta`): subject-scoped row delete; refcount sync com R2.
    D1,
    /// KV (sessions + cached metadata): DELETE keys matching tenant + subject prefix; eventual consistency.
    Kv,
    /// Stripe `Customer.update` (NOT delete): PII nullified em customer.metadata + email/name/address;
    /// preserva invoice integrity per PCI scope GAAP ASC 606 + LGPD Art. 16 fiscal compliance.
    Stripe,
    /// Loki/Grafana log deletion API: `/loki/api/v1/delete` por subject; retention compaction trigger.
    Loki,
    // ===== Pseudonymized backends (4 canonical pós Lote 10.11.0-bis; legal_hold WORM regulatory immutability) =====
    /// R2 audit Object Lock 7y (CTRL-AUDIT-IMMUTABILITY): substitui `subject_id` por
    /// `erased_<HMAC(salt, subject_id)>` via HKDF info=`corelink/v1/audit-pseudonym`; payload original preserved.
    R2AuditPseudo,
    /// Neon PITR backup 30d (Point-In-Time Recovery): rotação natural; tombstone replay em restore;
    /// auto-expira após 30d retention window (no manual delete).
    NeonPitrPseudo,
    /// R2 CAS legal_hold partition (governance mode): conteúdo retido se sob hold ativo;
    /// pseudonymize index references; release pós legal_hold expiry.
    R2CasLegalHoldPseudo,
    /// R2 evidence-* buckets 7y (DPIA/LIA/DSR evidence): retain por SLA framework
    /// (EVT-046 LIA + EVT-049 consent record + EVT-048 DSR evidence); subject_id pseudonymized.
    R2EvidencePseudo,
}

impl ErasureBackend {
    /// Marker para verification report; effective backends esperam 0 records, pseudonymized esperam 100% pii_redacted.
    pub fn is_effective(&self) -> bool {
        match self {
            // 8 effective canonical pós Lote 10.11.0-bis
            Self::NeonMain | Self::NeonBilling | Self::R2Cas | Self::R2Ac
            | Self::D1 | Self::Kv | Self::Stripe | Self::Loki => true,
            // 4 pseudonymized canonical pós Lote 10.11.0-bis
            Self::R2AuditPseudo | Self::NeonPitrPseudo
            | Self::R2CasLegalHoldPseudo | Self::R2EvidencePseudo => false,
        }
    }
}

/// Per-backend outcome — 5-arm enum aligned com sprint contract §5.2 R-S11-6 status enum.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BackendErasureOutcome {
    /// Effective backend: 0 records remaining post-DELETE.
    Erased { records_deleted: u64, completed_at: DateTime<Utc> },
    /// Pseudonymized backend: PII fields substituted by sha256(subject_id || erasure_salt) + pii_redacted=true marker.
    Pseudonymized { records_redacted: u64, completed_at: DateTime<Utc> },
    /// Some records erased, others failed (e.g., legal_hold=true preserved per CTRL-PRIV-033).
    PartialFailure { records_succeeded: u64, records_failed: u64, error_classes: Vec<ErrorClass>, completed_at: DateTime<Utc> },
    /// Total failure; backend unavailable; retry queue activates.
    Failed { error: ErasureWorkerError, retry_after_seconds: u64 },
    /// Backend not applicable for this dsr_id (e.g., tenant has no Stripe customer).
    NotApplicable { reason: String },
}

/// CloudEvents `type` field; canonical prefix `dev.hugr.corelink.dsr.erasure.<verb>.v1` per Lote 10.9bis P0-G.
#[derive(strum::Display, strum::EnumIter)]
pub enum ErasureAuditEventType {
    #[strum(serialize = "dev.hugr.corelink.dsr.erasure.started.v1")]
    Started,
    #[strum(serialize = "dev.hugr.corelink.dsr.erasure.backend_completed.v1")]
    BackendCompleted,                                // emit por backend × dsr_id
    #[strum(serialize = "dev.hugr.corelink.dsr.erasure.verification_passed.v1")]
    VerificationPassed,
    #[strum(serialize = "dev.hugr.corelink.dsr.erasure.verification_failed.v1")]
    VerificationFailed,
    #[strum(serialize = "dev.hugr.corelink.dsr.erasure.completed.v1")]
    Completed,                                       // dsr_tickets.status → 'completed'
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + customer cost protection + service protection discipline justification)

### 2.1 Contexto

Privacy regulations exigem erasure efetiva: LGPD Art. 18 IV ("eliminação dos dados pessoais tratados com o consentimento do titular"), GDPR Art. 17.1 ("right to erasure / right to be forgotten"), CCPA §1798.105 ("delete personal information"). GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization) reconhece tensão entre Art. 17 + immutable audit logs (regulatory necessity per SOC 2 + LGPD Art. 37) e estabelece **pseudonymization como escape valve** quando deletion física é impossibilitada por outra obrigação legal.

CoreLink stack: 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) contém PII potential. Privacy_model.md §6.2 reescrito como source-of-truth pós Lote 10.11.0-bis: 12 backends canonical (8 effective: Neon multi-tabela, Neon billing fiscal exception, R2 CAS refcount-aware, R2 AC, D1, KV, Stripe, Loki + 4 pseudonymized: R2 audit, Neon PITR, R2 CAS legal_hold, R2 evidence-*). Sem extension explícita, R-S09-5/10 + R-S10-1 introduziriam silent erasure-incomplete violations.

### 2.2 Abordagem

Worker `corelink-privacy-erasure-worker` consome queue Cloudflare Queue (introduzido S-09 inheritance) com payload `dsr.queued.v1` (`{dsr_id, tenant_id, subject_id, request_type='erasure'}`). Orquestra erasure em 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) via async pipeline (per-backend independent + per-dsr atomic via dsr_erasure_log tombstones). Effective (8 — Lote 10.11.0-bis canonical) → Neon multi-tabela DELETE / R2 CAS refcount-aware scrub / Stripe Customer.update PII nullify / Loki log deletion API. Pseudonymized (4) → substituir PII fields por sha256(subject_id || erasure_salt) + marker `pii_redacted=true`. Audit emit fail-CLOSED por backend completion (split-tier discipline Lote 10.6bis). Idempotency `(dsr_id, backend)` UNIQUE em `dsr_erasure_log` permite replay-safe (PAT-RETRY-IDEMPOTENT-001). Verification job 24h sweep posts EVT-048 DSR_EVIDENCE 7y; verifica per backend conforme `is_effective()`. Tombstone permite forensic audit pelo regulator.

### 2.3 Valor entregue

- **Regulatory baseline absoluto**: LGPD Art. 18 IV + GDPR Art. 17 + CCPA §1798.105 atendidos via 12-backend canonical (Lote 10.11.0-bis) coverage.
- **GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization techniques) alignment**: pseudonymization escape valve documentada + auditable; defensible em ANPD/Irish DPC investigations. (Lote 10.11.0-ter: corrigida citação errada anterior "EDPB 5/2020 §74" — Guidelines 5/2020 são sobre consent, não erasure pseudonymization).
- **INV-DATA-ERASURE-COMPLETE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008) §3.5 L110**: erasure cross-backend efetiva em 12/12 backends canonical (8 effective + 4 pseudonymized; Lote 10.11.0-bis); 0 records remanescentes 30d post-request (effective) ou 100% pseudonymized (Object Lock).
- **Customer trust**: verification job 24h posts EVT-048 7y — customer pode auditor independent self-service via WI-S11-001 receipt URL.
- **SLO-FRESH-DSR-ERASURE foundation**: 99% ≤30d sustained 90d (slo_catalog.md §4.12).

### 2.4 Principais riscos & trade-offs

- **R2 mutable scrub vs S-07 dedup**: blob shared entre tenants via dedup S-07 — DELETE pelo titular não pode quebrar outro tenant. **Mitigation**: refcount-aware soft-delete (chunks table FK); blob deletado apenas se refcount==1 + owner_tenant_id == subject's tenant_id. Decisão: shared blobs marcados `subject_unaffiliated` (sem subject_id remanescente em metadata) = legitimate sharing.
- **Pseudonymization vs full anonymization**: pseudonymization permite re-identification by attacker com erasure_salt. **Mitigation**: erasure_salt único per-tenant + per-DSR + stored em tenant-controlled vault (NÃO no nosso DB) — customer holds o key. Aceito risk per GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques).
- **Cross-backend atomic vs eventual consistency**: 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) ≠ database — 2PC inviável. **Decisão**: per-backend atomic + dsr_erasure_log tombstone serializa progress; verification job 24h gates `dsr.completed.v1`. Failure de 1 backend não rollbacka outros (compensating-rollback inviável — Stripe customer.delete é irreversível). Aceito eventual consistency com 24h verification.
- **Stripe customer.delete preserva invoice**: GAAP ASC 606 + LGPD Art. 16 fiscal 5y obriga preservar invoice. Pseudonymize customer.email/name/address; invoice data permanece com tenant_id mantido (revenue continuity for HuGR + audit). Aceito trade-off documented em DPA.

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **Privacy-aware data subject**: receives `dsr.completed.v1` notification email com link para evidence-dsr/<dsr_id>/erasure-report.json (signed URL 24h).
- **Privacy Officer interno**: monitora SLO-FRESH-DSR-ERASURE dashboard; investiga partial_failure outliers.
- **External auditor (ANPD, Irish DPC, SOC 2)**: revisa EVT-048 reports + dsr_erasure_log tombstones.

### 3.2 Customer journey (touchpoints)

1. WI-S11-001 emite `dsr.queued.v1` → worker consume queue.
2. Worker emite `dsr.erasure.started.v1` → tenant receives email (locale per tenant.primary_region).
3. Per backend erasure (10 ops paralelo bounded async). Each emite `backend_completed.v1`.
4. dsr_erasure_log tombstones inserted; `dsr.erasure.completed.v1` emitido.
5. After 24h: verification job sweep → `verification_passed.v1` ou `verification_failed.v1`.
6. EVT-048 erasure-report.json em R2 evidence-dsr/ (signed URL 24h enviada via email).
7. dsr_tickets.status → 'completed'; clock_stop F-11.

### 3.3 Jornadas (User Journeys) afetadas

- DSR queue depth visualizada em S-13 admin plane.
- BYOK customer (S-14 future): crypto-erase invocada via destroy customer-managed key; covered em separate WI-S14.

### 3.4 Métricas de customer-visible

- **Erasure completion rate per backend**: ≥ 99% per `corelink_dsr_erasure_backend_outcome_total{backend, status}`.
- **24h verification pass rate**: ≥ 99% sustained 90d.
- **Per-backend p95 latency**: D1 ≤ 5s, Neon ≤ 30s, R2 mutable ≤ 5min, Stripe ≤ 60s, Loki ≤ 30min.
- **Pseudonymization 24h verification 100% pii_redacted=true marker presence**.

### 3.5 Comunicação ao customer

3 emails per DSR: (1) `erasure.started` "Sua eliminação foi iniciada"; (2) `erasure.completed` "12/12 backends canonical (8 effective + 4 pseudonymized; Lote 10.11.0-bis) processados"; (3) `verification_passed` (24h post) "Verificação independente confirmou eliminação"; locale conforme WI-S11-004.

### 3.6 Mitigação de fricção

- **Verification 24h grace**: tenant pode usar produto até verification (não bloqueado durante worker progress).
- **Partial failure transparency**: customer recebe `verification_failed` com detalhes per-backend (não silent).
- **Appeal path**: per LGPD Art. 18 + Art. 20, customer pode disputar via Privacy Officer review (24h SLA).

### Anti-pattern ❌

❌ Cross-backend 2PC (inviável Stripe API); ❌ Silent partial_failure (regulatory exposure); ❌ Pseudonymization sem erasure_salt customer-controlled (GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) violation); ❌ R2 mutable scrub sem refcount check (cross-tenant break); ❌ Verification job sem 24h delay (Loki cold archive não settled); ❌ Stripe customer.delete sem invoice preservation (GAAP violation).

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-4 + R-S11-4-PSEUDO + R-S11-5 + R-S11-6 | Erasure 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) + crypto-erase + verification 24h |
| CAPs | CAP-PRIV-002 | Erasure automation cross-backend |
| Invariantes | INV-DATA-ERASURE-COMPLETE (CRITICAL — Lote 10.11.0-bis §3.5 L110) + INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116; pseudonymization preserves) |
| Controles | CTRL-PRIV-030 (erasure pipeline) + CTRL-PRIV-033 (legal hold override) + CTRL-PRIV-014 (audit minimization) + CTRL-CRYPTO-002 (encryption at rest) |
| Padrões | PAT-RETRY-IDEMPOTENT-001 (replay-safe; sprint contract §9 14.s11.4) |
| Failure modes | **FM-450 (NEW S-11)** erasure-incomplete cross-backend; **FM-452 (NEW S-11)** consent-tampering-detected; FM-061 (audit Object Lock vs DSR — canonical legal hold path); FM-105 (inter-region latency Loki cold archive) |
| Métricas | `corelink_dsr_erasure_backend_outcome_total{backend, status, tenant_tier}` + `corelink_dsr_erasure_p95_seconds{backend}` + `corelink_dsr_erasure_verification_total{outcome}` + `corelink_dsr_erasure_pseudonymization_marker_total{backend}` |
| Eventos | 5 CloudEvents canonical: erasure.started/backend_completed/verification_passed/verification_failed/completed |
| Evidence | EVT-048 (DSR_EVIDENCE 7y canonical) + EVT-042 (ERASURE_TEST CI) + EVT-047 (AUDIT_EVENT 7y) + EVT-002 (CI integration) |
| SLOs | SLO-FRESH-DSR-ERASURE (99% ≤30d sustained 90d) — slo_catalog.md §4.12 |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline` — implementação primária do erasure pipeline cross-backend.

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante; sem este WI, S-11 não satisfaz INV-DATA-ERASURE-COMPLETE.

### 5.3 Blast radius

**Tenant-isolated** via TenantCtx + `(dsr_id, backend)` UNIQUE; bug em refcount logic R2 mutable pode causar cross-tenant blob break (catastrofic).

### 5.4 Reversibilidade

**Compensating-rollback inviável** após `dsr.erasure.completed.v1` (Stripe customer.delete + R2 mutable DELETE são irreversíveis). Feature flag `dsr_erasure_worker_enabled` pode pausar worker; backlog acumula em queue dentro do SLA 30d. Decisão documented em DPA + privacy notice.

### 5.5 Experiment? (A/B test, feature flag experiment)

Não — regulatory feature, sem A/B (multi-arm = compliance risk).

### 5.6 Compliance triggers

- LGPD Art. 18 IV (eliminação), Art. 16 (legal hold fiscal), Art. 32 (segurança), Art. 37 (registros).
- GDPR Art. 17 (right to erasure), Art. 32 (security), Art. 30 (records of processing).
- CCPA §1798.105 (delete).
- SOC 2 P4.1..4.3 (use, retention, disposal).
- GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization) (pseudonymization escape valve).
- ISO/IEC 27701 §6.10 (PII subject rights).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. NEW crate `crates/corelink-privacy-erasure-worker` (queue consumer + per-backend erase + verification).
2. 12 backend adapters canonical pós Lote 10.11.0-bis (effective: NeonMain, NeonBilling, R2Cas refcount-aware, R2Ac, D1, Kv, Stripe Customer.update, Loki; pseudonymized: R2AuditPseudo, NeonPitrPseudo, R2CasLegalHoldPseudo, R2EvidencePseudo).
3. D1 schema migration: `dsr_erasure_log` table per-backend tombstones (DDL §6.1.7).
4. R2 bucket NEW `evidence-dsr-`<region>`` para EVT-048 DSR_EVIDENCE outputs (7y retention canonical + R2 Object Lock governance mode framework (privacy_model.md §6.2 4-pseudonymized canonical) — porque report pode ser regenerated).
5. Cloudflare Queue consumer integration (S-09 inheritance).
6. Per-backend idempotency_key derivation aligned com Lote 10.10-quaters Idempotency-Key 35-char canonical: `corelink-{dsr_id_short(8)}-{backend}-{retry_count(3)}` formato; UNIQUE em dsr_erasure_log.
7. Audit emit fail-CLOSED via `crates/corelink-audit-emit` (S-09 inheritance) — 5 CloudEvents types em audit-`<region>` Object Lock 7y.
8. Pseudonymization helper `crates/corelink-privacy-pseudonymize` (NEW crate; sha256(subject_id || erasure_salt) + marker `pii_redacted=true` insertion).
9. Verification job 24h cron worker — sweep todos 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) per dsr_id; posts EVT-048 erasure-report.json em R2 evidence-dsr/.
10. RB-DSR-ERASURE-INCOMPLETE runbook NEW (sprint contract §6 DoD listed).
11. **FM-450 (NEW)** + **FM-452 (NEW)** declarations em failure_modes.md (sprint contract v1.2.0 commitment).
12. Stripe::Customer pseudonymize helper: keep customer object ativo (invoice continuity) mas overwrite email/name/address fields per Stripe API.
13. Loki delete API integration `/loki/api/v1/delete` per privacy_model.md §6.2 step 4f.
14. R2 CAS legal_hold partition pseudonymize: index references replaced via HKDF info=`corelink/v1/audit-pseudonym`; release pós legal_hold expiry (canonical pós Lote 10.11.0-bis).
15. Refcount-aware R2 mutable scrub: integrate com S-07 chunks table + dedup logic.
16. Erasure_salt management: tenant-controlled vault interface (S-14 BYOK foundation prep — actual KMS integration deferred); até S-14, fallback é per-tenant random salt em D1 (acceptable interim per ADR-S11-003).

#### 6.1.7 D1 schema migration `dsr_erasure_log`

```sql
-- D1 migration N+2: dsr_erasure_log — per-backend tombstones
CREATE TABLE dsr_erasure_log (
  log_id                    TEXT        PRIMARY KEY,                  -- ULID 26 chars
  dsr_id                    TEXT        NOT NULL,                     -- FK dsr_tickets.dsr_id
  tenant_id                 TEXT        NOT NULL,
  subject_id_hash           TEXT        NOT NULL,                     -- sha256(subject_id || tenant_salt) — never plaintext (CTRL-PRIV-014)
  backend                   TEXT        NOT NULL CHECK (backend IN (
                                                          'neon_main','neon_billing','r2_cas','r2_ac','d1','kv','stripe','loki',                              -- 8 effective canonical Lote 10.11.0-bis
                                                          'r2_audit_pseudo','neon_pitr_pseudo','r2_cas_legalhold_pseudo','r2_evidence_pseudo'  -- 4 pseudonymized canonical
                                                       )),
  outcome                   TEXT        NOT NULL CHECK (outcome IN ('erased','pseudonymized','partial_failure','failed','not_applicable')),
  records_affected          INTEGER     NOT NULL DEFAULT 0,           -- count_deleted ou count_redacted
  error_classes             TEXT        NULL,                         -- JSON array enum (apenas se partial_failure ou failed)
  retry_count               INTEGER     NOT NULL DEFAULT 0 CHECK (retry_count >= 0 AND retry_count <= 5),
  idempotency_key           TEXT        NOT NULL,                     -- 35-char canonical
  started_at                TEXT        NOT NULL,                     -- ISO 8601 UTC
  completed_at              TEXT        NULL,
  -- idempotency UNIQUE per (dsr_id, backend) → replay-safe
  UNIQUE (dsr_id, backend)
);

CREATE INDEX idx_dsr_erasure_log_dsr ON dsr_erasure_log(dsr_id, backend);
CREATE INDEX idx_dsr_erasure_log_tenant_outcome ON dsr_erasure_log(tenant_id, outcome, completed_at DESC);
```

#### 6.1.11 FM-450 + FM-452 declarations (failure_modes.md additions S-11)

```
| FM-450 | DSR erasure-incomplete cross-backend (≥1 of 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) fails verification 24h) | 3 | 4 | 5 | 60 | P0 (S=5 → upgrade FF-HR-010) | INV-DATA-ERASURE-COMPLETE + verification job 24h + RB-DSR-ERASURE-INCOMPLETE + alert SEV-1 |
| FM-452 | Consent record tampering detected (notice_text_hash mismatch on verify) | 2 | 3 | 5 | 30 | P1 (S=5 → upgrade) | INV-CONSENT-PROOF-VERIFIABLE + signature HMAC + RB-CONSENT-TAMPERING |
```

### 6.2 Componentes C4 afetados

- **Worker `corelink-privacy-erasure-worker`** (NEW; queue consumer).
- **D1**: novo table `dsr_erasure_log`.
- **R2 evidence-dsr-`<region>`**: NEW bucket (não Object Lock — reports podem ser regenerated).
- **R2 audit-`<region>`**: 5 canonical CloudEvents types appended.
- **R2 cas-`<region>`**: scrub manifests + chunks via S-07 dedup.
- **R2 ac-`<region>`**: DELETE entries.
- **Neon multi-tabela**: DELETE cascade across `dsr_tickets/account/tenant/user_account/consent_ledger/subscription`.
- **Neon billing fiscal exception**: pseudonymize PII em legal_hold rows (LGPD Art. 16 5y).
- **R2 CAS refcount-aware**: subject_unaffiliated → refcount decrement; subject_dedicated → tombstone + GC sweep.
- **R2 AC**: DELETE entries per owner_tenant_id.
- **D1 (`blob_meta`/`ac_meta`)**: subject-scoped row delete; refcount sync com R2.
- **KV**: DELETE keys matching tenant + subject prefix.
- **Stripe `Customer.update`** (NOT delete; PCI scope): PII nullified em customer.metadata + email/name/address.
- **Loki**: `/loki/api/v1/delete` query API + retention compaction trigger.
- **R2 audit Object Lock 7y**: pseudonymize subject_id via HKDF info=`corelink/v1/audit-pseudonym` (legal hold preserved).
- **Neon PITR backup 30d**: rotação natural; tombstone replay em qualquer restore.
- **R2 CAS legal_hold partition**: governance mode preservation; pseudonymize index refs.
- **R2 evidence-* buckets 7y**: subject_id pseudonymized; retain por SLA framework.
- **Cloudflare Queue** (S-09 inheritance): consumer.
- **Cron worker (S-10 inheritance)**: 24h verification sweep.

### 6.3 Arquivos do repositório

```
crates/corelink-privacy-erasure-worker/
├─ Cargo.toml
├─ src/
│  ├─ lib.rs                                  # ErasureWorker trait
│  ├─ queue_consumer.rs                       # Cloudflare Queue dsr.queued.v1
│  ├─ orchestrator.rs                         # 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) parallel bounded
│  ├─ backends/                               # 12 canonical adapters (8 effective + 4 pseudonymized; Lote 10.11.0-bis)
│  │  ├─ neon_main.rs                         # 1. Neon multi-tabela DELETE cascade (dsr_tickets/account/tenant/user_account/consent_ledger/subscription)
│  │  ├─ neon_billing.rs                      # 2. Neon billing fiscal exception pseudonymize (LGPD Art. 16 5y)
│  │  ├─ r2_cas.rs                            # 3. R2 CAS refcount-aware (subject_unaffiliated/subject_dedicated; S-07 dedup integration)
│  │  ├─ r2_ac.rs                             # 4. R2 AC DELETE entries owner_tenant_id
│  │  ├─ d1.rs                                # 5. D1 blob_meta + ac_meta subject-scoped row delete; refcount sync com R2
│  │  ├─ kv.rs                                # 6. KV DELETE keys tenant + subject prefix
│  │  ├─ stripe.rs                            # 7. Stripe `Customer.update` PII nullify (NOT delete; PCI scope)
│  │  ├─ loki.rs                              # 8. Loki `/loki/api/v1/delete` API + retention compaction trigger
│  │  ├─ r2_audit_pseudo.rs                   # 9. R2 audit Object Lock 7y; HKDF info=corelink/v1/audit-pseudonym
│  │  ├─ neon_pitr_pseudo.rs                  # 10. Neon PITR backup 30d; tombstone replay em restore
│  │  ├─ r2_cas_legalhold_pseudo.rs           # 11. R2 CAS legal_hold partition governance mode
│  │  └─ r2_evidence_pseudo.rs                # 12. R2 evidence-* buckets 7y; subject_id pseudonymized
│  ├─ verification_job.rs                     # 24h cron sweep
│  ├─ pseudonymize.rs                         # sha256(subject_id || erasure_salt) helper
│  ├─ idempotency.rs                          # (dsr_id, backend) UNIQUE
│  ├─ audit_emit.rs                           # 5 CloudEvents types fail-CLOSED
│  └─ error.rs                                # ErasureWorkerError taxonomy
├─ migrations/
│  └─ N+2__dsr_erasure_log.sql                # canonical DDL §6.1.7
└─ tests/
   ├─ integration_erasure_lifecycle.rs        # signup → 30d use → DSR → 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) → verification
   ├─ property_idempotency_replay_100x.rs     # 100k iter replay-safe per backend
   ├─ property_pseudonymization_correctness.rs # 100k iter sha256 + marker invariants
   ├─ chaos_per_backend_failure.rs            # cada backend down independent
   ├─ regression_refcount_aware_scrub.rs      # S-07 dedup not broken
   ├─ regression_stripe_invoice_preserved.rs  # GAAP ASC 606 + LGPD Art. 16 compliance
   └─ verification_job_24h.rs                 # cron sweep + EVT-048 generation

crates/corelink-privacy-pseudonymize/         # NEW small crate
├─ Cargo.toml
├─ src/
│  └─ lib.rs                                  # sha256 + marker insertion + verify helper
└─ tests/
   └─ pseudonymization_invariants.rs

specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md   # NEW canonical
specs/05_quality/runbooks/RB-CONSENT-TAMPERING.md        # NEW canonical (FM-452 mitigation)
```

### 6.4 Sistemas externos tocados

- **Cloudflare Queue** (consumer + producer dead-letter).
- **Neon** (multi-tabela DELETE cascade + billing fiscal pseudonymize + PITR backup 30d pseudonymized).
- **R2** (CAS refcount-aware mutable + AC mutable + audit Object Lock 7y pseudonymized + CAS legal_hold partition pseudonymized + evidence-* buckets 7y pseudonymized + evidence-dsr).
- **D1** (blob_meta + ac_meta + dsr_erasure_log).
- **KV** (DELETE keys tenant + subject prefix).
- **Stripe API** (`Customer.update` PII nullify via wrapper S-10 StripeClient — NOT delete; PCI scope per Lote 10.11.0-bis).
- **Loki API** (`/loki/api/v1/delete` + retention compaction trigger).

## 7. Anti-Scope

- ❌ DSR API HTTP layer — entregue em WI-S11-001.
- ❌ Consent ledger (capture/revoke endpoints) — entregue em WI-S11-003.
- ❌ Privacy notice content + 3 locales translation — entregue em WI-S11-004.
- ❌ Sub-processor register email broadcast — entregue em WI-S11-005.
- ❌ Breach notification runbook — entregue em WI-S11-006.
- ❌ Residency pinning routing — entregue em WI-S11-007.
- ❌ DPIA + LIA + TLA+ dsr_erasure_atomicity — entregue em WI-S11-008.
- ❌ BYOK crypto-erase NIST SP 800-88 ceremony — deferred to S-14 (this WI documents interface stub).
- ❌ Customer-controlled erasure_salt vault KMS integration — deferred to S-14; interim is per-tenant random salt em D1 (ADR-S11-003).
- ❌ Cross-account DSR (A pede erasure de seus dados em B's tenant) — anti-scope per privacy_model.md §6.3 ("não atendido").

### Anti-pattern ❌

❌ Cross-backend 2PC ❌ Silent partial_failure (regulatory exposure) ❌ Pseudonymization shared salt cross-tenant ❌ R2 mutable scrub sem refcount (cross-tenant break) ❌ Stripe customer.delete sem GAAP fallback ❌ Audit fail-OPEN (regulatory finding); ❌ Erasure sem tombstone (forensic gap); ❌ Verification antes de 24h Loki settle.

## 8. Acceptance Criteria (Gherkin) — 12 scenarios

### AC-001: Happy path 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) erasure-effective + pseudonymized

```gherkin
Given um titular verified com dados em todos 12 backends canonical pós Lote 10.11.0-bis (8 effective: Neon multi-tabela + Neon billing fiscal + R2 CAS refcount-aware + R2 AC + D1 + KV + Stripe Customer.update + Loki; 4 pseudonymized: R2 audit + Neon PITR + R2 CAS legal_hold + R2 evidence-*)
And dsr_tickets row com dsr_id D, request_type='erasure', status='queued'
When o queue consumer processa `dev.hugr.corelink.dsr.queued.v1` com payload {dsr_id: D}
Then 12 backend ops canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) são executadas em parallel bounded async
And cada backend cria 1 row em dsr_erasure_log com (dsr_id=D, backend=<backend>, outcome='erased' OR 'pseudonymized')
And `dev.hugr.corelink.dsr.erasure.backend_completed.v1` emitido por cada backend (12 eventos canonical)
And `dev.hugr.corelink.dsr.erasure.completed.v1` emitido após todos 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) sucessful
And dsr_tickets.status → 'completed' atomic
```

### AC-002: Pseudonymization correctness per GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques)

```gherkin
Given um titular verified com data em R2 audit-weur Object Lock 7y
When erasure worker processa backend=R2AuditPseudo
Then cada audit event original com subject_id = S permanece em R2 (Object Lock immutable)
And índice secundário em D1 é atualizado: subject_id → sha256(S || erasure_salt) com marker pii_redacted=true
And erasure_salt é único per-tenant + per-DSR (não shared cross-tenant)
And sha256 hex 64 chars; marker é boolean field separate column
And query subsequente "audit events where subject_id = S" retorna empty
And forensic query com erasure_salt customer-held re-correlates (defensible per GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques))
```

### AC-003: Refcount-aware R2 cas mutable scrub (S-07 dedup safety)

```gherkin
Given um blob B em R2 cas-weur com refcount=2 (shared via S-07 dedup entre tenants T1 e T2)
And titular S em T1 tem ownership de blob B (chunk lineage)
When erasure worker processa backend=R2Cas (refcount-aware) para subject_id=S em tenant_id=T1
Then chunks table FK row (S, B) é DELETE
And blob B refcount decrement to 1 (T2 ainda owns)
And blob B NÃO é deletado fisicamente em R2 (cross-tenant break prevented)
And metadata: subject_unaffiliated=true em chunk → blob mapping para T1
But se refcount==0 post-decrement, blob B é DELETE em R2 + chunks table row removed
```

### AC-004: Stripe customer pseudonymize (GAAP ASC 606 + LGPD Art. 16 compliance)

```gherkin
Given um tenant T tem Stripe customer C com email=foo@bar.com, name="John Doe", address="123 Main St"
And tenant T tem invoices históricas em Stripe (não-deletáveis por GAAP)
When erasure worker processa backend=Stripe para subject_id=S
Then Stripe API call: Customer.update(C, { email: 'pseudo@redacted.tld', name: 'erased_<sha256_short>', address: null, metadata: { pii_redacted: 'true', erasure_dsr_id: D } })
And Stripe::Customer::delete is NOT called (preserva invoice continuity)
And invoice records preservam tenant_id (revenue continuity)
And dsr_erasure_log row inserida com outcome='pseudonymized' + records_affected=1
```

### AC-005: Verification job 24h sweep happy path

```gherkin
Given um dsr_id D processado via AC-001 (12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis)) e dsr.erasure.completed.v1 emitido em ts T
When 24h após T, verification cron worker dispara para dsr_id D
Then sweep query per backend: 8 effective expect COUNT(*)==0; 4 pseudonymized expect 100% pii_redacted=true marker
And EVT-048 erasure-report.json gerado em R2 evidence-dsr-weur/<D>/erasure-report.json com per-backend status
And `dev.hugr.corelink.dsr.erasure.verification_passed.v1` emitido
And signed URL R2 24h TTL enviado por email para titular
And `corelink_dsr_erasure_verification_total{outcome='passed'}` incrementado
```

### AC-006: Verification job partial_failure → SEV-1 alert + appeal

```gherkin
Given um dsr_id D processado mas Loki cold archive ainda tem 5 logs com PII raw (settle delay)
When 24h verification sweep detecta backend=R2EvidencePseudo com pii_redacted=true marker presence < 100%
Then EVT-048 erasure-report.json marca outcome='partial_failure' para R2EvidencePseudo
And `dev.hugr.corelink.dsr.erasure.verification_failed.v1` emitido com error_classes=['evidence_settle_delay']
And SEV-1 alert disparado para Privacy Officer + SecLead + SRE oncall (FM-450 P0)
And RB-DSR-ERASURE-INCOMPLETE runbook acionado
And dsr_tickets.status NÃO transiciona para 'completed' até segunda verification 48h post-original ts
And titular notificado via email "Verificação ainda em progresso; expectativa 7d adicional"
```

### AC-007: Idempotency replay-safe 100×

```gherkin
Given um dsr_id D já processado backend=D1 com outcome='erased', records_affected=42
When erasure worker é re-disparado (queue redelivery, manual replay) para mesma combinação (D, D1)
Then dsr_erasure_log UNIQUE (dsr_id, backend) constraint dispara → idempotent skip
And NÃO há SQL DELETE re-executed (records já erased)
And NÃO há novo CloudEvents `backend_completed.v1` emitido
And response retorna existing log_id + outcome
And property test 100k iter random combinations (dsr_id, backend) verifies idempotency invariant
```

### AC-008: Audit emit fail-CLOSED por backend completion

```gherkin
Given audit emit infrastructure (R2 audit-`<region>`) está temporariamente indisponível
When erasure worker tenta processar backend=Stripe para dsr_id D
Then Stripe Customer.update NÃO é executado (transaction abort BEFORE backend mutation)
And dsr_erasure_log row NÃO é inserido
And worker re-enqueues message com retry_after=60s + retry_count++
And SEV-2 alert disparado se retry_count atinge 5 (max_retries; FM-061 mapping)
And distinct from WI-S10-001 billing fail-OPEN (Lote 10.6bis split-tier discipline; ADR-S11-002 cross-WI)
```

### AC-009: Cross-tenant attack mitigation

```gherkin
Given um attacker tenta forge `dsr.queued.v1` payload com dsr_id pertencente a tenant T1 mas attacker é em T2
When queue consumer processa o payload
Then pre-check verifica tenant_id em payload === tenant_id de dsr_tickets[dsr_id] (DB lookup)
And mismatch detected → message rejected + SEV-1 alert + audit `dsr.erasure.rejected.v1` emitted
And NÃO há nenhuma backend mutation
And property test 100k random (attacker_tenant, target_tenant) pairs: 0 cross-tenant erasures
```

### AC-010: Legal hold override CTRL-PRIV-033

```gherkin
Given um tenant T com flag legal_hold=true em dsr_tickets[dsr_id D] (set by Privacy Officer pre-DSR)
When erasure worker processa backend=Neon
Then SELECT pre-check: WHERE legal_hold = false AND tenant_id = T → empty
And worker skip backend=Neon com outcome='not_applicable' (reason='legal_hold_active')
And dsr_erasure_log row outcome='not_applicable' inserida
And audit event emitido com legal_hold_active context
And EVT-044 LEGAL_REVIEW evidence linked em report
```

### AC-011: SLO-FRESH-DSR-ERASURE clock semantics F-11 alignment

```gherkin
Given um dsr_id D verified em ts V (post step-up MFA per WI-S11-001)
And expected_completion_at = V + 30 dias corridos
When erasure worker completes 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) em ts C
Then `corelink_dsr_resolution_hours{request='erasure'}` histogram observado com value (C - V) em horas
And SLI: 99% within 30d (720h) per slo_catalog.md §4.12
And se C - V > 720h, outlier → SEV-2 + post-mortem hook (sprint contract §18 SLA miss)
And legal hold pause time NOT counted (pause_total_minutes subtracted from clock)
```

### AC-012: FM-450 + FM-452 registered + RB stub created

```gherkin
Given `failure_modes.md` pre-WI-S11-002 não tem FM-450 nem FM-452
When WI-S11-002 commit applied
Then failure_modes.md tem FM-450 entry: "DSR erasure-incomplete cross-backend" P0 (S=5 → upgrade FF-HR-010)
And failure_modes.md tem FM-452 entry: "Consent record tampering detected" P1 (S=5 → upgrade)
And specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md existe com canonical SOP (decision tree per backend × failure class × remediation)
And specs/05_quality/runbooks/RB-CONSENT-TAMPERING.md existe com canonical SOP (notice_text_hash verify + investigation flow)
And validate_references.py: zero NEW dangling S-11 FM/RB introduced by this WI (pre-existing repo-wide dangling refs catalogued separately em audit log; not S-11 scope)
```

## 9. Design Decisions

### 9.1 Decisões locais (não justificam ADR)

- **DD-001 Per-backend independent vs orchestrated atomic**: per-backend independent (com tombstones serializing progress); 2PC inviável por Stripe API + Object Lock characteristics. Verification 24h gates `dsr.completed.v1` ensures regulatory closure. Aceito eventual consistency.
- **DD-002 Pseudonymization sha256 vs SHA3 vs BLAKE3**: SHA-256 padrão LGPD/GDPR (suitable per FIPS 180-4); BLAKE3 mais rápido mas NIST não-aprovado yet for regulatory contexts; trade-off: SHA-256 wins por defensibilidade legal (Privacy Officer signature em ADR docs).
- **DD-003 erasure_salt scope**: per-tenant + per-DSR (não global). Rationale: prevent cross-DSR correlation by attacker; per-DSR rotation provides forward secrecy se 1 salt leaked.
- **DD-004 R2 mutable refcount-aware**: integrate com S-07 chunks table; soft-delete first (24h grace) + GC (S-06) sweep. Decisão: soft-delete only para titular's chunks ownership; blob shared survives if other tenants reference.
- **DD-005 Verification job 24h vs 48h**: 24h é minimum para Loki cold archive settle (S-09 R-S09-5 mention "≤30min lag" but with daily batch = up to 24h); 48h alternative more conservative mas adds 1d to SLO clock perception. Decisão: 24h with retry-once-at-48h for partial_failure recovery.

### 9.2 Decisões que justificam ADR

- **ADR-S11-003 (NEW)**: erasure_salt management interim (S-11 a S-13) é per-tenant random em D1 vault encrypted; full KMS BYOK integration deferred to S-14. Rationale: S-14 não SEALED; S-11 não pode block on dependency externa. Privacy Officer + Compliance Officer sign-off na decisão (interim acceptable; documented em DPA + privacy notice).
- **ADR-S11-004 (NEW)**: Cross-backend erasure é eventual consistency (NÃO atomic 2PC) com 24h verification window. Rationale: 2PC sobre 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) heterogêneos (Stripe API + Object Lock R2 + Loki HTTP API) é inviável; per-backend tombstone + 24h verification é industry standard (OneTrust, Transcend, DataGrail). Aceito EDPB Guidelines + DPA documentation.

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| Cross-backend atomicity | 2PC orchestrator | Per-backend + tombstone | B | 2PC inviável Stripe + Object Lock |
| Verification window | 24h | 48h | 24h + retry@48h | Loki settle minimum + customer perception |
| erasure_salt scope | Global per-tenant | Per-DSR | Per-DSR | Forward secrecy + cross-DSR correlation prevention |
| R2 mutable scrub depth | Hard delete owner | Soft delete + GC | Soft + GC | S-07 dedup safety + reversibility within 24h |
| Stripe API approach | Customer.delete | Customer pseudonymize | Pseudonymize | GAAP ASC 606 + LGPD Art. 16 fiscal preservation |
| Pseudonymization hash | SHA-256 | BLAKE3 | SHA-256 | NIST FIPS 180-4 + regulatory defensibility |

### Anti-pattern ❌

❌ 2PC cross-backend (inviável); ❌ Pseudonymization global salt; ❌ Hard delete sem refcount (cross-tenant break); ❌ Stripe customer.delete (GAAP violation); ❌ BLAKE3 em pseudonymization (regulatory non-defensible); ❌ Verification < 24h (Loki settle not complete); ❌ Audit fail-OPEN (regulatory finding); ❌ Salt em D1 plaintext (encrypted at rest mandatory).

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** Crate `corelink-privacy-erasure-worker` compila zero warnings em `cargo build --target wasm32-unknown-unknown`.
- [ ] **C-1.2** 12 backend adapter modules implementados (8 effective + 4 pseudonymized).
- [ ] **C-1.3** D1 migration N+2 dsr_erasure_log aplica em staging + roll-back + index covered queries verde.
- [ ] **C-1.4** Stripe adapter wraps S-10 StripeClient trait (sem reimplementar HTTP).
- [ ] **C-1.5** Loki adapter HTTP delete API integrated com retry exponential backoff.
- [ ] **C-1.6** Pseudonymize crate `corelink-privacy-pseudonymize` separate (NEW; no deps em S-11 core to enable S-14 BYOK refactor).
- [ ] **C-1.7** Audit emit fail-CLOSED 5 CloudEvents canonical types via S-09 inheritance.
- [ ] **C-1.8** No `tokio::spawn` em CF Workers; `worker::send_future` para fire-and-forget per backend (Lote 10.7bis R5 P0-3 lesson).
- [ ] **C-1.9** Cron worker 24h verification gated por feature flag `dsr_erasure_verification_enabled` em DO config-singleton.
- [ ] **C-1.10** RB-DSR-ERASURE-INCOMPLETE + RB-CONSENT-TAMPERING runbooks ativados (NEW canonical).
- [ ] **C-1.11** failure_modes.md FM-450 + FM-452 entries committed + cross-referenced em §6.1.11.

### 10.2 Test Completeness

- [ ] **T-2.1** Integration test full lifecycle (signup → 30d use → DSR → 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) → verification 24h) verde.
- [ ] **T-2.2** Property test idempotency replay 100k iter; (dsr_id, backend) UNIQUE invariant preserved.
- [ ] **T-2.3** Property test pseudonymization correctness 100k iter; sha256(subject_id || erasure_salt) deterministic + marker present.
- [ ] **T-2.4** Chaos test per-backend failure independent; verifica audit emit fail-CLOSED + dead-letter queue routing.
- [ ] **T-2.5** Regression test refcount-aware R2 mutable scrub: blob shared entre 3 tenants → erasure 1 não quebra outros 2.
- [ ] **T-2.6** Regression test Stripe invoice preserved post-pseudonymize (GAAP ASC 606 + LGPD Art. 16 simulação).
- [ ] **T-2.7** Verification job 24h cron simulado em staging tempo-acelerado (1h staging = 24h prod via test fixture).
- [ ] **T-2.8** Cross-tenant attack test: 100k random pairs (attacker_tenant, target_tenant) → 0 leaks.
- [ ] **T-2.9** Legal hold pause test: titular com legal_hold=true → outcome='not_applicable' for backend=Neon.
- [ ] **T-2.10** SLO-FRESH-DSR-ERASURE histogram observation test (`corelink_dsr_resolution_hours{request='erasure'}`).

### 10.3 Documentation Completeness

- [ ] **D-3.1** `docs/dev/dsr-erasure-architecture.md` com diagrama 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) + atomicity model.
- [ ] **D-3.2** `specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md` SOP com decision tree.
- [ ] **D-3.3** `specs/05_quality/runbooks/RB-CONSENT-TAMPERING.md` SOP investigation flow.
- [ ] **D-3.4** `docs/dev/pseudonymization-pattern.md` GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) alignment.
- [ ] **D-3.5** ADR-S11-003 (erasure_salt interim) + ADR-S11-004 (eventual consistency cross-backend) committed.

### 10.4 Observability Completeness

- [ ] **O-4.1** 4 Prom metrics: `corelink_dsr_erasure_backend_outcome_total{backend,status,tenant_tier}`, `corelink_dsr_erasure_p95_seconds{backend}`, `corelink_dsr_erasure_verification_total{outcome}`, `corelink_dsr_erasure_pseudonymization_marker_total{backend}`.
- [ ] **O-4.2** 1 dashboard `corelink-dsr-erasure-pipeline` em Grafana (S-09 inheritance) com 10 panels (1 per backend) + 2 summary (overall completion + verification pass rate).
- [ ] **O-4.3** 5 CloudEvents canonical types emitidos em audit-`<region>` Object Lock 7y.
- [ ] **O-4.4** 1 trace per dsr_id E2E (W3C tracecontext propagation; 10 child spans per backend).

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** subject_id NUNCA em logs (CTRL-PRIV-014); apenas subject_id_hash sha256.
- [ ] **S-5.2** erasure_salt encrypted at rest D1 (AES-256-GCM via tenant key; CTRL-CRYPTO-002).
- [ ] **S-5.3** Pseudonymization hash sha256 deterministic verifiable forensic-grade.
- [ ] **S-5.4** Cross-tenant attack mitigation: dsr_id × tenant_id pre-check no queue consumer.
- [ ] **S-5.5** Stripe pseudonymize não vaza email/name em metadata.pii_redacted=true marker.
- [ ] **S-5.6** Loki delete API auth via PAT-RETRY-IDEMPOTENT-001 (idempotent retries).
- [ ] **S-5.7** DLP scan CI gate: 0 raw PII em audit events post-pseudonymize.

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui `corelink-privacy-erasure-worker` + `corelink-privacy-pseudonymize` crates.
- [ ] **B-6.2** Sigstore provenance attestation (CTRL-SUPPLY-001).
- [ ] **B-6.3** Cargo deny isolation Stripe SDK wrapping (S-10 inheritance pattern).

## 11. DoD

10.x checked + sign-off matrix §30 12 confirmados + chaos 30d staging + verification job 24h verde em staging com SLA simulation + EVT-048 + EVT-042 retain.

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-002 |
|---|---|---|---|
| **INV-DATA-ERASURE-COMPLETE** | CRITICAL (Lote 10.11.0-bis) | invariant_registry.md §3.5 L110 | 12/12 backends canonical (8 effective + 4 pseudonymized; Lote 10.11.0-bis) erasure-effective ou pseudonymized; verification 24h sweep; EVT-048 retain 7y; tombstones em dsr_erasure_log per backend |
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | 5 CloudEvents emit fail-CLOSED em audit-`<region>` Object Lock 7y; pseudonymization preserva audit immutability via secondary index update (não deleta original) |
| **INV-CONSENT-PROOF-VERIFIABLE** (foundation) | CRITICAL (Lote 10.11.0-bis) | invariant_registry.md §3.12 L168 | Erasure cobertura para R2 audit pseudonymizes consent events sem quebrar notice_text_hash signature; full satisfação em WI-S11-003 |

## 13. Artifacts Produced

- `crates/corelink-privacy-erasure-worker/` (NEW; ~6000 LoC).
- `crates/corelink-privacy-pseudonymize/` (NEW; ~800 LoC).
- `migrations/N+2__dsr_erasure_log.sql` (DDL + indexes).
- 5 CloudEvents schemas em `schemas/cloudevents/dsr-erasure-{started,backend_completed,verification_passed,verification_failed,completed}.v1.json`.
- `docs/dev/dsr-erasure-architecture.md` + `docs/dev/pseudonymization-pattern.md`.
- `specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md` (NEW).
- `specs/05_quality/runbooks/RB-CONSENT-TAMPERING.md` (NEW).
- `specs/03_architecture/adrs/ADR-S11-003-erasure-salt-interim.md` (NEW).
- `specs/03_architecture/adrs/ADR-S11-004-cross-backend-eventual-consistency.md` (NEW).
- failure_modes.md FM-450 + FM-452 entries.
- Grafana dashboard JSON `dashboards/corelink-dsr-erasure-pipeline.json`.
- 4 Prom metrics + alerts em S-09 inheritance.

## 14. Quality Standards SOTA

- **14.s11.2.1** Erasure cross-backend coverage 12/12 backends canonical (8 effective + 4 pseudonymized; Lote 10.11.0-bis) — INV-DATA-ERASURE-COMPLETE absolute.
- **14.s11.2.2** Pseudonymization sha256 deterministic; 100k iter property test invariant.
- **14.s11.2.3** Idempotency replay 100× per backend × 100k random combinations.
- **14.s11.2.4** Verification 24h pass rate ≥ 99% sustained 90d.
- **14.s11.2.5** Stripe invoice preserved post-pseudonymize (GAAP ASC 606 simulation).
- **14.s11.2.6** R2 mutable refcount-aware: 0 cross-tenant blob breaks em property test 100k.
- **14.s11.2.7** Audit fail-CLOSED 5 CloudEvents types — distinct from billing fail-OPEN (split-tier).
- **14.s11.2.8** INV §3.X positions canonical verified pre-merge (Lote 10.8bis P1-13 lesson absorbed).
- **14.s11.2.9** Cross-tenant attack: 0 leaks em 100k random pairs property test.
- **14.s11.2.10** EVT-048 + EVT-042 evidence retention compliance (7y EVT-048 canonical aligned com privacy_model.md §8 + 3y EVT-042 ERASURE_TEST CI per privacy_model.md §2).

## 15. Chaos Experiments (10)

1. R2 audit-`<region>` unavailable → backend_completed.v1 NÃO emitido → worker retries (fail-CLOSED).
2. Stripe API rate-limited → exponential backoff retry; SEV-2 alert se retry_count==5.
3. D1 connection pool exhausted → message DLQ → SRE intervention RB-DSR-ERASURE-INCOMPLETE.
4. Loki delete API timeout > 30min → partial_failure outcome + verification job retries 48h post-original.
5. Concurrent erasure + GC sweep race em S-06 chunks table → refcount race (mitigated por pessimistic lock + WHERE refcount > 0).
6. CF Analytics Engine export job before pseudo drop → verification flags PII presence → block dsr.completed.v1.
7. Cross-tenant queue inject (forge dsr_id) → pre-check fails → SEV-1 + audit reject event.
8. Legal hold flipped during erasure (pre + post check) → backend skipped + audit log + Privacy Officer review.
9. erasure_salt rotation mid-DSR (impossible by design — per-DSR salt frozen) → property test verifies invariant.
10. Pseudonymization hash collision (theoretically infeasible; SHA-256 2^128 birthday bound) → property test fixture; would require ADR if observed.

## 16. PRR

PRR HIGH_RISK 12 sign-offs (§30) + chaos 30d staging + verification job 24h verde + DLP CI scan + Privacy + Compliance + Architect + DPO mandatory emphatic.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml + ErasureWorker trait | 2h |
| ST-002 | D1 migration N+2 dsr_erasure_log DDL + tests | 1.5h |
| ST-003 | Pseudonymize crate `corelink-privacy-pseudonymize` (NEW) | 2h |
| ST-004 | Backend adapter D1 + tests | 1h |
| ST-005 | Backend adapter Neon + VACUUM strategy + tests | 2h |
| ST-006 | Backend adapter R2 CAS refcount-aware (S-07 integration) + tests | 2.5h |
| ST-007 | Backend adapter R2 AC + tests | 1h |
| ST-008 | Backend adapter KV + tests | 1h |
| ST-009 | Backend adapter Stripe `Customer.update` pseudonymize (S-10 wrap; NOT delete; PCI scope) + tests | 1.5h |
| ST-010 | Backend adapter Loki `/loki/api/v1/delete` API + tests | 1.5h |
| ST-010b | Backend adapter Neon multi-tabela DELETE cascade + Neon billing fiscal pseudonymize + tests | 2h |
| ST-011 | 4 pseudonymized backend adapters canonical pós Lote 10.11.0-bis (R2 audit Object Lock 7y + Neon PITR 30d + R2 CAS legal_hold partition + R2 evidence-* 7y) | 3h |
| ST-012 | Queue consumer + orchestrator parallel bounded | 2h |
| ST-013 | Verification job 24h cron worker | 2h |
| ST-014 | Audit emit 5 CloudEvents fail-CLOSED | 1h |
| ST-015 | Idempotency UNIQUE (dsr_id, backend) tests | 1.5h |
| ST-016 | RB-DSR-ERASURE-INCOMPLETE + RB-CONSENT-TAMPERING runbooks | 1.5h |
| ST-017 | failure_modes.md FM-450 + FM-452 declarations + cross-ref | 0.5h |
| ST-018 | ADR-S11-003 (erasure_salt interim) + ADR-S11-004 (eventual consistency) | 1h |
| ST-019 | 4 Prom metrics + Grafana dashboard 12 panels | 2h |
| ST-020 | Documentation 5 docs + cross-ref | 1.7h |

**PERT total**: ~28.7h (alinha com sprint contract §12).

## 18. Dependencies

- **Hard**: WI-S11-001 SEALED (DSR API enqueues queue); S-09 SEALED (audit emit + Loki + queue + CF Analytics); S-10 SEALED (Stripe StripeClient + R2 billing-events Object Lock); S-07 SEALED (chunks dedup table + refcount); S-08 SEALED (DO patterns); spec contract S-11 v1.2.0 SEALED (Lote 10.11.0).
- **Soft**: WI-S11-008 (TLA+ dsr_erasure_atomicity validates this WI's eventual consistency model); S-14 BYOK (final erasure_salt vault — interim per ADR-S11-003).

## 19. Effort PERT: ~28.7h. ## 20. Time-boxing: 40h hard limit (lane HIGH_RISK +40% buffer; cross-backend complexity warrant).

## 21. Observability

4 Prom metrics + 1 Grafana dashboard 12 panels + 5 CloudEvents canonical + W3C trace E2E 10 child spans per backend (Lote 10.9bis P0-A canonical inheritance).

## 22. Cost Analysis

- **Cloudflare Queue**: 1 message/DSR consumer + 10 retries worst-case; ≈$0.001/DSR.
- **D1 dsr_erasure_log**: ~10 rows/DSR; negligible.
- **R2 evidence-dsr-`<region>`**: 1 erasure-report.json per DSR; max ~50KB; 7y retention canonical; ≈$0.001/DSR/year.
- **Stripe API**: 1 Customer.update per DSR pseudonymize; included em current S-10 budget.
- **Loki delete API**: 1 call per DSR; included em S-09 budget.
- **D1 erasure_salt vault**: ~64 bytes per tenant + per-DSR salt; negligible.
- **Cron worker 24h verification**: 1 invocation per DSR; ~30s CPU; ≈$0.0001/DSR.
- **Total estimated**: ≤ $0.05/DSR processed (well under §14 budget cap; volume-bounded).

## 23. API Contract

5 CloudEvents schemas em `schemas/cloudevents/dsr-erasure-*.v1.json`. dsr_erasure_log DDL canonical em §6.1.7. Per-backend adapter trait `BackendErasureAdapter` em `crates/corelink-privacy-erasure-worker/src/backends/mod.rs`.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| FM-450 erasure-incomplete cross-backend | CRITICAL | Privacy Officer + SecLead + Compliance + Legal escalation (LGPD Art. 18 + ANPD breach notification consideration) |
| INV-DATA-ERASURE-COMPLETE violation (record remanescente 30d post-DSR) | CRITICAL (Lote 10.11.0-bis-prime: HIGH→CRITICAL severity cascade) | Privacy Officer + Architect; **SEV-1 alert** (severity escalation Lote 10.10-sextus lesson absorbed) |
| Pseudonymization marker missing post-24h verification | HIGH | Privacy Officer + SecLead |
| R2 mutable refcount cross-tenant break | CRITICAL | Architect + SecLead + S-07 owner; emergency rollback |
| Stripe customer.delete invoked accidentally (GAAP violation) | CRITICAL | Compliance + Finance + Legal + Privacy |
| erasure_salt leak (interim D1 vault compromise) | CRITICAL | SecLead + Privacy + Compliance; rotate all salts + ADR-S11-003 review |
| Verification job 24h job failure rate > 5% | HIGH | SRE + Privacy Officer |
| Queue consumer message redelivery loop | MEDIUM | SRE + Architect |
| Audit fail-OPEN regression (Lote 10.6bis violation) | CRITICAL | Architect + Privacy + Compliance |
| Legal hold override misapplied | HIGH | Privacy Officer + Legal review CTRL-PRIV-033 |

## 25. Rollback / Recovery

Feature flag `dsr_erasure_worker_enabled` em DO config-singleton; flip → backlog em Cloudflare Queue (TTL 7d); SLA pause valid (privacy_model.md §6.1 indisponibilidade técnica documentada cap 5d úteis somatório). **Compensating-rollback inviável** após `dsr.erasure.completed.v1` (Stripe pseudonymize + R2 mutable DELETE são irreversíveis). Decisão documented em DPA + privacy notice.

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): pseudonymization sha256(subject_id || erasure_salt) per-DSR salt forward secrecy.
- I(dentifiability): forensic re-identification only via customer-held erasure_salt (GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) defensible).
- N(on-repudiation): R2 Object Lock 7y preservation + tombstones em dsr_erasure_log forensic-grade.
- D(etectability): verification job 24h sweep gates dsr.completed.v1; partial_failure SEV-1 alert.
- D(isclosure): erasure_salt encrypted at rest AES-256-GCM CTRL-CRYPTO-002; Stripe pseudonymize via metadata not delete.
- U(nawareness): per-backend status em EVT-048 erasure-report.json transparente para titular.
- N(on-compliance): **LGPD Art. 18 IV + GDPR Art. 17 + CCPA §1798.105 + SOC 2 P4.1..4.3 + GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization) + GAAP ASC 606 + LGPD Art. 16 fiscal compliance** via 12-backend canonical (Lote 10.11.0-bis) coverage + pseudonymization escape valve + 24h verification + 7y EVT-048 (canonical).

## 27. Knowledge Transfer

Tech talk (1.5h): "S-11 DSR Erasure Worker: 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis), GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) pseudonymization, 24h verification, refcount-aware dedup safety, Stripe GAAP preservation"; doc `docs/dev/dsr-erasure-architecture.md`; onboarding test 8 questões: 12 backends canonical Lote 10.11.0-bis (8 effective + 4 pseudonymized), GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) escape valve rationale, refcount-aware R2 mutable scrub (S-07 dedup safety), Stripe pseudonymize vs delete (GAAP), pseudonymization sha256(subject_id || erasure_salt) per-DSR salt scope, verification 24h vs 48h trade-off, audit fail-CLOSED vs billing fail-OPEN (Lote 10.6bis split-tier), eventual consistency cross-backend (ADR-S11-004).

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-DATA-ERASURE-COMPLETE violation (1+ backends fail verification 24h) | M | M | HIGH | M | LOW | FM-450 + verification 24h + retry@48h + RB-DSR-ERASURE-INCOMPLETE; SEV-2 alert (HIGH severity → SEV-2 canonical post Lote 10.10-sextus lesson) |
| R-002 | Pseudonymization marker missing (pii_redacted != true post-process) | L | M | HIGH | M | LOW | Property test 100k iter sha256 + marker invariant; verification 24h sweep |
| R-003 | R2 mutable refcount cross-tenant break (S-07 integration regression) | L | M | CRITICAL | L | LOW | Refcount-aware soft-delete + 24h grace + GC sweep + property test 100k |
| R-004 | Stripe customer.delete invoked (GAAP ASC 606 violation) | L | L | CRITICAL | L | LOW | Customer.update only (pseudonymize); regression test simulação invoice retention |
| R-005 | erasure_salt leak (interim D1 vault) | L | L | CRITICAL | L | LOW | AES-256-GCM at-rest + tenant key rotation S-19; ADR-S11-003 interim documented; S-14 BYOK final solution |
| R-006 | Queue redelivery loop (idempotency cache miss) | L | L | MEDIUM | L | LOW | UNIQUE (dsr_id, backend) constraint; chaos test concurrent submits |
| R-007 | Verification job 24h false-positive partial_failure (Loki settle delay) | M | L | LOW | L | LOW | Retry-once-at-48h before SEV-1; tunable per-backend tolerance |
| R-008 | Cross-tenant erasure attack (forged queue payload) | L | L | CRITICAL | L | LOW | Pre-check tenant_id payload === dsr_tickets[dsr_id].tenant_id; property test 100k pairs |
| R-009 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.5 L110 + §3.6 L116 + §3.12 L168 verified Lote 10.11.0; ongoing maintenance grep CI gate |
| R-010 | Audit fail-OPEN regression (Lote 10.6bis violation) | L | L | CRITICAL | L | LOW | ADR-S11-002 cross-WI; integration test asserts retry on emit failure |
| R-011 | Legal hold override misapplied (titular bloqueado erasure quando NÃO deveria) | L | M | HIGH | M | LOW | CTRL-PRIV-033 + Privacy Officer dual-approval para set legal_hold=true; quarterly audit |
| R-012 | Stripe API breaking change (S-10 wrapper insufficient) | L | L | MEDIUM | L | LOW | StripeClient trait abstraction (S-10 inheritance); migration tests semi-annual |

## 29. Review Checkpoints

D+0 design review (Architect; eventual consistency model + GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) alignment); D+1 Privacy Officer (LINDDUN + 12-backend canonical (Lote 10.11.0-bis) canonical mapping); D+2 SecLead (cross-tenant attack vectors + erasure_salt scope); D+3 Compliance (SOC 2 P4.1..4.3 + LGPD Art. 18/16 + GAAP ASC 606); D+4 Legal (DPA reference + GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) defensibility); D+5 SRE (Cloudflare Queue + cron worker patterns); D+6 code review; D+7 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — cross-tenant attack + erasure_salt + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 30d + property test 100k cross-tenant + refcount safety_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 P4.1..4.3 + LGPD Art. 18/16 + GAAP ASC 606 + GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques)_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + 12-backend canonical (Lote 10.11.0-bis) canonical + pseudonymization correctness_ |
| 11 | Architect | _TBD; **mandatory emphatic** — eventual consistency cross-backend (ADR-S11-004) + split-tier audit fail-CLOSED + INV §3.X verification_ |
| 12 | DPO interim (Gustavo até hire) | _TBD; **mandatory emphatic** — INV-DATA-ERASURE-COMPLETE + GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB (anonymization techniques) escape valve defensibility_ |

(Legal sign-off via DPA reference at sprint level; not per-WI per Lote 10.10-quaters R4 NEW-P1-4 lesson.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **Cardinalidade 12 backends canonical Lote 10.11.0-bis** (8 effective: Neon multi-tabela / Neon billing fiscal exception / R2 CAS refcount-aware (subject_unaffiliated vs subject_dedicated) / R2 AC / D1 / KV / Stripe Customer.update / Loki + 4 pseudonymized: R2 audit Object Lock 7y com HKDF audit-pseudonym / Neon PITR backup 30d / R2 CAS legal_hold partition / R2 evidence-* buckets 7y) — substituindo "10 backends total" v1.0. (b) **EDPB 5/2020 §74 → GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by EDPB** (anonymization) — Guidelines 5/2020 são sobre consent não erasure pseudonymization (GPT P0-5). (c) **INV-DATA-ERASURE-COMPLETE HIGH→CRITICAL** + TLA+ obrigatório via PAT-FORMAL-VERIFICATION-001 (S-11 WI-S11-008 commit). (d) **HKDF info canonical** = `corelink/v1/erasure-salt` para pseudonymization + `corelink/v1/audit-pseudonym` para subject_id em audit retained. |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-002; HIGH_RISK; SOTA pós-S-10 SEALED 9.35/10. **Coração regulatory** do CoreLink Privacy Pipeline — 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) totais (8 effective slots: D1, Neon, R2 cas mutable, R2 ac mutable, KV, DO, Stripe, Loki + 4 pseudonymized: R2 audit-`<region>` 7y, R2 billing-events-`<region>` 7y, Loki cold archive R2, CF Analytics Engine 30d) — extends privacy_model.md §6.2 baseline canonical 6 com S-07/S-08/S-09/S-10 inheritance. INV-DATA-ERASURE-COMPLETE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008) §3.5 L110 satisfaction. GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB (anonymization) pseudonymization escape valve regulatório vs INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116 7y immutability. Verification job 24h sweep posts EVT-048 DSR_EVIDENCE 7y retention canonical em R2 evidence-dsr (aligned cycle 16)/<dsr_id>/erasure-report.json com per-backend status enum (`erased\|pseudonymized\|partial_failure\|failed\|not_applicable`). Tombstone em D1 dsr_erasure_log per backend forensic-grade. Audit emit fail-CLOSED 5 CloudEvents canonical types `dev.hugr.corelink.dsr.erasure.{started,backend_completed,verification_passed,verification_failed,completed}.v1` per Lote 10.9bis P0-G prefix — split-tier discipline (Lote 10.6bis fail-OPEN billing vs fail-CLOSED audit; ADR-S11-002 cross-WI). Idempotency UNIQUE (dsr_id, backend) per-backend dedup; replay-safe 100× (PAT-RETRY-IDEMPOTENT-001; sprint contract §9 14.s11.4). Refcount-aware R2 mutable scrub (S-07 dedup safety) — blob shared entre tenants não quebra. Stripe customer pseudonymize NÃO delete (GAAP ASC 606 + LGPD Art. 16 fiscal preservation). erasure_salt per-tenant + per-DSR scope forward secrecy (ADR-S11-003 interim D1 vault até S-14 BYOK KMS final). Cross-backend eventual consistency (ADR-S11-004 — 2PC inviável Stripe + Object Lock heterogeneity). 12 AC scenarios + 10 chaos + 12 risks + 10 post-mortem hooks. NEW FM-450 (erasure-incomplete cross-backend P0 S=5→upgrade FF-HR-010) + FM-452 (consent tampering P1 S=5→upgrade) declared em failure_modes.md. NEW RB-DSR-ERASURE-INCOMPLETE + RB-CONSENT-TAMPERING runbooks. NEW pseudonymize crate `corelink-privacy-pseudonymize` separate (foundation S-14 BYOK refactor). **Lote 10.10 lessons absorbed**: (a) source-of-truth FIRST — INV positions §3.5 L110 + §3.6 L116 + §3.12 L168 verified pre-merge (Lote 10.8bis P1-13); (b) typed enum NÃO serde_json::Value (Lote 10.9-quinquies NEW-P0-2); (c) sign-off cap 12 (Lote 10.8bis P1-2); (d) cascade discipline absoluta — sweep 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis) × 8 sub-systems (effective + pseudonymized + verification + tombstone + audit + idempotency + refcount + erasure_salt); (e) split-tier discipline canonical (Lote 10.6bis fail-OPEN billing vs fail-CLOSED audit); (f) corelink_time canonical helper for SLA calculations (Lote 10.10-quaters lesson); (g) typed enum Region (canonical 6-region); (h) PrimaryKey 4-tuple inclusion via dsr_erasure_log (similar pattern to WI-S10-002 PK 4-tuple). |

## 32. Anti-patterns evitados

- ❌ Cross-backend 2PC (inviável Stripe + Object Lock); ❌ Pseudonymization shared salt cross-tenant (forward secrecy violation); ❌ Hard delete sem refcount (cross-tenant break); ❌ Stripe customer.delete (GAAP violation); ❌ BLAKE3 em pseudonymization (NIST não-aprovado regulatory); ❌ Verification < 24h (Loki cold archive não settled); ❌ erasure_salt em D1 plaintext (encrypted at rest mandatory); ❌ Audit fail-OPEN (regulatory finding; Lote 10.6bis split-tier violation); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ INV §3.X position TBD (Lote 10.8bis P1-13); ❌ serde_json::Value em ErasureBackend (Lote 10.9-quinquies NEW-P0-2); ❌ Erasure sem tombstone (forensic gap); ❌ Verification job sem retry-once-at-48h (false-positive partial_failure); ❌ Stripe pseudonymize sem metadata.pii_redacted=true marker (forensic-grade não-defensible); ❌ R2 mutable scrub direct (sem soft-delete + GC grace).

---
