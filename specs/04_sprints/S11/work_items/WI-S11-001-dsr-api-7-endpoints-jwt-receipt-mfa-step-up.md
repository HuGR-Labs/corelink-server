---
id: "WI-S11-001"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.2.0"
created: "2026-04-26"
updated: "2026-05-03"
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
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s11", "dsr", "lgpd", "gdpr", "ccpa", "privacy", "mfa-step-up", "jwt-receipt", "rate-limit", "high-risk"]
---

# WI-S11-001 — DSR Self-Service API (7 endpoints LGPD Art. 18 / GDPR Art. 15-22) + Signed JWT Receipt + Status Endpoint + CTRL-AUTH-010 Step-Up MFA + Rate Limit 10/dia/subject (`crates/corelink-privacy-dsr-api`; HTTP layer entrega 7 direitos canonical em privacy_model.md §6.1 com SLAs distintos por direito — confirm 5d úteis / access 15d úteis / correction 5d úteis / **erasure 30d corridos** / portability 15d úteis / objection 15d úteis manual review / consent_revoke ≤5min — clock semantics F-11 baseada em `dsr_tickets.status='verified'` post-MFA-step-up; `CTRL-AUTH-010` WebAuthn re-auth obrigatório para erasure/correction (sensitive ops); `CTRL-PRIV-022` self-service surface; signed JWT receipt verifiable via endpoint público `GET /v1/privacy/dsr/verify?token=<jwt>`; rate limit 10 DSR/dia/subject reusing S-08 quota state machine inheritance — anti-DoS LGPD Art. 20 humane; emit fail-CLOSED audit (CTRL-AUDIT-002 + INV-AUDIT-APPEND-ONLY CRITICAL — DSR submission é regulatory-grade, NÃO tolera silent loss; distinto de WI-S10-001 billing fail-OPEN); `ticket_id` UUIDv7 idempotency dedup em Neon `dsr_tickets` (canonical pós Lote 10.11.0-bis); foundation API que WI-S11-002 erasure worker consome via queue)

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-001 |
| Título | DSR API 7 endpoints + JWT receipt + step-up MFA (CTRL-AUTH-010 canonical em `security_model.md §242` WebAuthn session-bound) + rate limit 10/dia/subject (S-08 quota inheritance) + audit fail-CLOSED CloudEvents `dev.hugr.corelink.dsr.{submitted,verified,queued,completed,failed,rejected}.v1` 6 canonical types per Lote 10.9bis P0-G prefix; SLAs canonical em `privacy_model.md §6.1` clock semantics F-11 (verified→completed; pause em legal hold/ambiguity ≤5d úteis cap) |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII/regulatory direct exposure), FF-HR-005 (CTRL-PRIV-022 + CTRL-AUTH-010 alignment), FF-HR-010 (1ª regulatory full impl LGPD/GDPR/CCPA) |

## 1. Intent

DSR Self-Service API é o **regulatory primary surface** do CoreLink Privacy Pipeline — sem isso, **CTRL-PRIV-022** (DSR self-service) não é satisfeito e LGPD Art. 18 / GDPR Art. 15-22 ficam atendidos só via support ticket manual (≠ self-service). 7 endpoints HTTP canonical entregam os 7 direitos do titular com **SLAs distintos por direito** (privacy_model.md §6.1) — confirm/correction 5d úteis, access/portability/objection 15d úteis, erasure 30d corridos, consent_revoke ≤5min. Step-up MFA WebAuthn (CTRL-AUTH-010) obrigatório para ops sensíveis (erasure, correction) — é a verificação de identidade que **inicia o relógio** F-11 (`clock_start = dsr_tickets.status='verified'`). Rate limit 10/dia/subject (S-08 quota state machine inheritance) é anti-DoS humane (LGPD Art. 20). JWT receipt signed verifiable via endpoint público dá ao titular **proof of submission** independente do nosso state — diferencial vs OneTrust/Transcend que dão só ticket ID. Foundation que WI-S11-002 erasure worker consome.

```rust
// File: crates/corelink-privacy-dsr-api/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait DsrApi: Send + Sync {
    /// Submit DSR request; returns signed JWT receipt + dsr_id ULID.
    /// Idempotency: replay-safe via (tenant_id, subject_id, request_type, request_payload_hash) UNIQUE em D1 staging.
    /// Side-effect: emit CloudEvents `dev.hugr.corelink.dsr.submitted.v1` audit fail-CLOSED.
    async fn submit_dsr(
        &self,
        tenant_ctx: &TenantCtx,                         // S-03 inheritance enforcement
        subject_ctx: &SubjectCtx,                       // PAT principal post-authn
        request: DsrRequest,                            // typed enum (NOT serde_json::Value — Lote 10.9-quinquies NEW-P0-2 absorbed)
    ) -> Result<DsrReceipt, DsrApiError>;

    /// Verify signed JWT receipt via public endpoint; no auth required (privacy notice: titular shares token).
    async fn verify_receipt(
        &self,
        token: JwtReceiptToken,
    ) -> Result<DsrReceiptVerified, DsrApiError>;

    /// Step-up MFA WebAuthn re-auth gate for sensitive ops (erasure, correction).
    /// CTRL-AUTH-010 canonical em security_model.md §242 (NÃO CTRL-PRIV-016 que é "support read access requer consent").
    /// Transitions dsr_tickets.status received → verified (canonical state names data_model.md §4.1); sets verified_at = NOW(); starts F-11 clock.
    async fn verify_step_up_mfa(
        &self,
        dsr_id: DsrId,
        webauthn_assertion: WebauthnAssertion,           // FIDO2 Ed25519 / ES256 per security_model.md §384
    ) -> Result<DsrVerifiedReceipt, DsrApiError>;

    /// Status query; returns canonical state machine state.
    async fn get_status(
        &self,
        dsr_id: DsrId,
    ) -> Result<DsrStatusResponse, DsrApiError>;
}

/// 7 DSR request types canonical per privacy_model.md §6.1.
/// CloudEvents `data.request_type` field; serialized snake_case (Rust convention).
#[derive(serde::Serialize, serde::Deserialize, strum::Display, strum::EnumIter, Clone, Debug, PartialEq)]
#[serde(tag = "request_type", rename_all = "snake_case")]
pub enum DsrRequest {
    /// LGPD Art. 18 I — confirmação de tratamento; SLA 5d úteis; sub-categoria de access (lighter-weight).
    Confirmation,
    /// LGPD Art. 18 II / GDPR Art. 15 — access; SLA 15d úteis; entrega JSON export bundle via R2 signed URL.
    Access,
    /// LGPD Art. 18 III / GDPR Art. 16 — correction; SLA 5d úteis; allowlist fields, deny outras com 422.
    Correction { fields: BTreeMap<CorrectionField, CorrectionValue> },
    /// LGPD Art. 18 IV / GDPR Art. 17 — erasure; SLA 30d corridos; soft-delete imediato + hard-erase via WI-S11-002 worker.
    Erasure { reason: ErasureReason },
    /// LGPD Art. 18 V / GDPR Art. 20 — portability; SLA 15d úteis; JSON machine-readable export.
    Portability { format: PortabilityFormat /* json | csv | jsonl */ },
    /// GDPR Art. 21 — objection; SLA 15d úteis; **manual review path** (NÃO 100% self-service per privacy_model.md §6.1).
    Objection { processing_basis: LegitimateBasisCategory, reasoning: NonEmptyString },
    /// LGPD Art. 18 VI / GDPR Art. 7.3 — consent revoke; SLA ≤5min; cascade unsubscribe via CTRL-PRIV-CONSENT-002.
    ConsentRevoke { purpose: ConsentPurpose },
}

/// CloudEvents `type` field; canonical prefix `dev.hugr.corelink.dsr.<verb>.v1` per Lote 10.9bis P0-G.
/// 6 canonical types — mapped 1:1 com states canonical em privacy_model.md §6.2 pipeline.
#[derive(strum::Display, strum::EnumIter)]
pub enum DsrAuditEventType {
    #[strum(serialize = "dev.hugr.corelink.dsr.submitted.v1")]
    Submitted,
    #[strum(serialize = "dev.hugr.corelink.dsr.verified.v1")]
    Verified,           // post step-up MFA; clock_start
    #[strum(serialize = "dev.hugr.corelink.dsr.queued.v1")]
    Queued,             // enqueue to WI-S11-002 erasure worker (erasure path) ou processor (other paths)
    #[strum(serialize = "dev.hugr.corelink.dsr.completed.v1")]
    Completed,          // clock_stop; SLA met flag emitted
    #[strum(serialize = "dev.hugr.corelink.dsr.failed.v1")]
    Failed,             // technical failure; retry via dead-letter queue
    #[strum(serialize = "dev.hugr.corelink.dsr.rejected.v1")]
    Rejected,           // identity verification failed | rate limit hit | request invalid
}

/// JWT receipt payload signed with HS256 via tenant-scoped HMAC key derived via HKDF
/// info=`corelink/v1/dsr-receipt` (security_model.md §7.2 + key_management.md §2). Public verify endpoint stateless.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DsrReceiptPayload {
    pub iss: String,                                    // "corelink.dev/privacy"
    pub aud: String,                                    // tenant_id_short (8 hex)
    pub sub: SubjectIdHash,                             // sha256(subject_id || tenant_salt) per CTRL-PRIV-014
    pub iat: i64,                                       // issued_at unix ts
    pub exp: i64,                                       // expiration: iat + max_sla(request_kind) + 90d grace
    pub jti: TicketId,                                  // UUIDv7; uniquely identifies this DSR (canonical Lote 10.11.0-bis)
    pub dsr_request_kind: DsrRequestKind,               // 7-arm enum (canonical request_kind nomenclature)
    pub submission_ts: DateTime<Utc>,                   // RFC 3339 UTC
    pub expected_completion_ts: DateTime<Utc>,          // submission + SLA(request_kind) per privacy_model.md §6.1
    pub tenant_id_short: String,                        // first 8 hex chars; full UUID NÃO inferable
}

/// Verify endpoint response — STRICT subset de DsrReceiptPayload (corrige GPT P1-4 round-1: stateless impossível
/// se response retornar fields não derivable from JWT alone). Estrutura abaixo SÓ retorna fields presentes no
/// JWT ou derivados algoritmicamente do signature verification — ZERO DB lookup, ZERO state enrichment.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DsrReceiptVerified {
    /// Echo direto do JWT payload (after signature verify passa). Nenhum field adicional.
    pub payload: DsrReceiptPayload,
    /// Resultado boolean da verification — derivado do HMAC compare em constant-time.
    pub signature_valid: bool,
    /// Status temporal: 'within_sla' (now < expected_completion_ts) | 'sla_passed' (now >= expected_completion_ts).
    /// Computed pure-function from `now()` + payload.expected_completion_ts (sem DB).
    pub temporal_status: TemporalStatus,
    /// `expired` derivado from JWT exp claim vs `now()` (sem DB).
    pub expired: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum TemporalStatus {
    WithinSla,
    SlaPassed,
}

// Note canonical: verify endpoint NUNCA retorna `dsr_tickets.status` (current pipeline state),
// `result_url`, `denial_reason`, `failure_reason`, `verified_at`, `completed_at` — esses requerem DB lookup
// e violariam contract stateless. Para esses fields, titular usa `GET /v1/privacy/dsr/{ticket_id}/status`
// que tem auth + tenant-scoped (privacy: prevent enumeration via público verify endpoint).
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + customer trust + regulatory exposure justification)

### 2.1 Contexto

Privacy regulations (LGPD Art. 18 desde 2020-09; GDPR Art. 15-22 desde 2018-05; CCPA §1798.105/115/120/125 desde 2020-01) garantem aos titulares 7 direitos não-negociáveis. Empresas que entregam apenas via support ticket são consideradas non-compliant porque (a) titular sem internet/awareness não exerce direito; (b) ticket queue overload viola SLA; (c) audit trail manual = forensically weak. **Self-service API é o standard regulatory** desde EDPB Guidelines 4/2019 §IV.B.

CoreLink processa 3 classes de dado (privacy_model.md §1): (1) conta do dev (HuGR controlador); (2) telemetria pseudonimizada (HuGR controlador); (3) blob content do tenant (HuGR operador). Para classes (1) e (2), HuGR é **controlador direto** e responde aos 7 direitos. Para (3), HuGR é **operador** e suporta DSR do tenant — DPA contratual define SLA.

### 2.2 Abordagem

7 endpoints HTTP REST POST/GET com authn via PAT (S-03 inheritance) + step-up MFA WebAuthn (CTRL-AUTH-010, security_model.md §242) para ops sensíveis (erasure, correction). Step-up MFA é o que **inicia o clock F-11** — submissions pré-verified não consomem SLA (privacy_model.md §6.1 nota). Each request gera `dsr_id` ULID + signed JWT receipt verifiable via público endpoint stateless. Status endpoint para tracking. Rate limit 10/dia/subject reusing S-08 quota state machine (CAP-RATE-001) — anti-DoS sem reduzir direitos (LGPD Art. 20 humane). Audit fail-CLOSED em CloudEvents (CTRL-AUDIT-002 + INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116) — DSR submission é regulatory-grade e **NÃO tolera silent loss** (distinto de WI-S10-001 billing fail-OPEN; lição split-tier Lote 10.6bis: cada plane tem postura própria — billing tolera lag, audit não).

### 2.3 Valor entregue

- **Regulatory baseline**: LGPD Art. 18 I-VI + GDPR Art. 15-22 + CCPA §1798.105/115/120/125 atendidos via API self-service.
- **Customer trust**: signed JWT receipt verifiable independentemente do nosso DB — titular tem proof of submission mesmo se nosso state corrompe.
- **SLO-FRESH-DSR-ERASURE foundation**: `dsr.completed.v1` event powers o SLO 99% ≤ 30d (slo_catalog.md §4.12).
- **Audit-grade**: 6 canonical CloudEvents types em audit log Object Lock 7y satisfazem auditor SOC 2 P5.1..5.2 + LGPD Art. 37 + GDPR Art. 30.

### 2.4 Principais riscos & trade-offs

- **Step-up MFA friction vs security**: titular sem WebAuthn registered não pode submeter erasure imediatamente — UI aciona `POST /v1/auth/webauthn/register` first. Trade-off: friction adicional vs evitar erasure por imposter (catastrofic — destruir conta alheia). Aceito (CTRL-AUTH-010 mandatory para ops sensíveis).
- **JWT signed via HMAC tenant-scoped vs public-key**: HMAC simpler + faster; public-key (RSA/Ed25519) traz benefit de verify offline mas exige PKI. **Decision (Local DD-001)**: HMAC por simplicidade S-11; migrate to Ed25519 em S-19 se enterprise pedir.
- **Fail-CLOSED audit ≠ Fail-OPEN billing**: cliente pode receber 5xx em DSR submit if audit emit fails — aceito porque silent DSR loss é regulatory finding (multa) > 1 cliente frustrado.
- **Rate limit 10/dia/subject vs unlimited**: 10 é heuristic anti-DoS; legitimate user raramente usa > 1/year. Trade-off cap exposed em CTRL-PRIV-022 documentation.

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **Privacy-aware data subject** (titular GDPR/LGPD): submete DSR para ver/corrigir/apagar seus dados.
- **Tenant admin/owner**: submete DSR em nome de seus customers (HuGR é operador).
- **Privacy Officer interno HuGR**: monitora queue + SLA via dashboards (S-13 admin plane).
- **External auditor SOC 2 / regulator (ANPD, Irish DPC)**: verifica trail via audit events 7y + DSR_EVIDENCE EVT-048 7y.

### 3.2 Customer journey (touchpoints)

1. Titular descobre direito via `/privacy` notice → click "Exercer meus direitos".
2. UI (S-16) chama `POST /v1/privacy/dsr/<right>` → API retorna `dsr_id` + JWT receipt.
3. Step-up MFA prompt (WebAuthn) → confirmação `POST /v1/privacy/dsr/{dsr_id}/verify-mfa`.
4. Email notification: "Seu pedido foi verificado. SLA: 30 dias corridos para erasure".
5. Titular pode poll `GET /v1/privacy/dsr/{dsr_id}/status` ou aguarda email final.
6. SLA cumprido → `dsr.completed.v1` → email com download URL (R2 signed) ou confirmação.

### 3.3 Jornadas (User Journeys) afetadas

- **Signup** (S-13): adiciona MFA WebAuthn registration prompt (CTRL-AUTH-010 alignment).
- **Account settings**: link "Privacidade & meus dados" expõe DSR API.
- **Billing portal** (S-10): action menu permite trigger consent revoke ou portability.

### 3.4 Métricas de customer-visible

- **DSR submission success rate**: ≥ 99% (excluding rate-limit + identity-verification fails).
- **DSR receipt verify roundtrip**: ≤ 200ms p95 (público endpoint stateless cache).
- **SLA hit rate per right**: tracked separately via `corelink_dsr_sla_met_total{request_type=...}` Prom counter.
- **Step-up MFA conversion**: ≥ 95% titulares completam WebAuthn within 10min do submit (drop-off é UX issue → S-16).

### 3.5 Comunicação ao customer

Email transactional em 3 locales (PT-BR/EN/ES via WI-S11-004); template `dsr-submission-confirmation.<locale>.mjml`. Notificação extra a 14d antes de SLA (warning) + 1d antes (final reminder). Out-of-SLA = email com explanation + escalation path.

### 3.6 Mitigação de fricção

- **Re-auth WebAuthn**: cached por 15min em mesma sessão; titular não re-MFA para 2 DSR consecutivas se < 15min.
- **Rate limit 10/dia/subject**: human-friendly (legitimate user nunca atinge); error msg explica + appeal path para Privacy Officer (LGPD Art. 20 humane).
- **Status polling**: ≤ 1 req/30s rate-limited; admite ETag para non-changed.

### Anti-pattern ❌

❌ Self-service via UI sem API (impede integração 3rd-party privacy tools); ❌ DSR submission sem step-up MFA (cross-tenant erasure attack via stolen PAT); ❌ Fail-OPEN audit (silent loss de DSR submission = LGPD Art. 18 violation); ❌ JWT sem expiration (token roubado fica válido indefinidamente); ❌ Confirmation flow sem expected_completion_ts (titular não pode rastrear SLA).

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-1 + R-S11-2 + R-S11-3 | DSR endpoints + JWT receipt + status |
| CAPs | CAP-PRIV-001 + CAP-PRIV-009 | DSR self-service API + DSR receipt verifiable |
| Invariantes | INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116) + INV-DATA-ERASURE-COMPLETE (CRITICAL — Lote 10.11.0-bis §3.5 L110, foundation for WI-S11-002) |
| Controles | CTRL-PRIV-022 (DSR self-service) + CTRL-AUTH-010 (MFA + session binding) + CTRL-AUDIT-002 (audit access) + CTRL-PRIV-014 (audit minimization actor_hash NÃO name) |
| Padrões | PAT-RETRY-IDEMPOTENT-001 (replay-safe DSR submit) |
| Failure modes | FM-061 (audit Object Lock vs DSR — canonical legal hold path) |
| Métricas | `corelink_dsr_request_total{request_type, status, tenant_tier}` + `corelink_dsr_sla_met_total{request_type}` + `corelink_dsr_step_up_mfa_total{outcome}` |
| Eventos | 6 CloudEvents canonical: submitted/verified/queued/completed/failed/rejected |
| Evidence | EVT-048 (DSR_EVIDENCE) + EVT-047 (AUDIT_EVENT) + EVT-001 (CI logs) |
| SLOs | SLO-FRESH-DSR-ERASURE (99% ≤30d) — slo_catalog.md §4.12 |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline` — implementação primária de direitos legais (LGPD/GDPR/CCPA).

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante; sem este WI, S-11 não tem API surface.

### 5.3 Blast radius

**Tenant-isolated** via TenantCtx; mas API é regulatory-facing — bug em authn/authz pode causar **cross-tenant DSR** (catastrofic, viola LGPD Art. 18 + leakage).

### 5.4 Reversibilidade

**Reversível** via feature flag `dsr_api_enabled` (DO config-singleton). Se severe bug detected, disable API, fallback para support ticket process documented em `legal/dsr-fallback-process.md`. SLA pause valid sob §6.1 nota indisponibilidade técnica documentada (cap 5d úteis).

### 5.5 Experiment? (A/B test, feature flag experiment)

Não — regulatory feature, sem A/B (multi-arm = compliance risk).

### 5.6 Compliance triggers

- LGPD Art. 18 I-VI (direitos do titular), Art. 37 (registros), Art. 41 (DPO).
- GDPR Art. 12-22 (DSRs), Art. 30 (records of processing).
- CCPA §1798.105/115/120/125.
- SOC 2 P5.1..5.2 (Access + correction).
- ISO/IEC 27701 §6.10 (PII subject rights).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. NEW crate `crates/corelink-privacy-dsr-api` (HTTP layer + DsrApi trait + impl).
2. 7 endpoints REST POST listed em §1 + JSON Schema validators (allow-list per request_type).
3. 1 endpoint público GET `/v1/privacy/dsr/verify?token=<jwt>` (verify receipt; no auth).
4. 1 endpoint GET `/v1/privacy/dsr/{dsr_id}/status` (auth required, tenant-scoped).
5. 1 endpoint POST `/v1/privacy/dsr/{dsr_id}/verify-mfa` (step-up MFA WebAuthn).
6. **Neon** schema migration: `dsr_tickets` table canonical (storage = Neon NOT D1 per Lote 10.11.0-bis decision; data_model.md §4.1 source-of-truth; DDL em §6.1.7 abaixo).
7. CTRL-AUTH-010 step-up MFA integration: import from `crates/corelink-auth` (S-03 inheritance); WebAuthn assertion verify.
8. Rate limit DSR 10/dia/subject: import from `crates/corelink-rate-limit` (S-08 inheritance); novo bucket key `dsr:<tenant_id>:<subject_id>:<day>`.
9. Audit fail-CLOSED emit: 6 CloudEvents types via `crates/corelink-audit-emit` (S-09 inheritance) com BlobDigest wrapper Redact-aware (Lote 10.9-quinquies P0-J + NEW-P0-2 lessons absorbed).
10. JWT receipt signing: HMAC-SHA256 com tenant-scoped key derived via HKDF (security_model.md §374); 35-char `kid` (key id) prefix derivation aligned com Lote 10.10-quaters Idempotency-Key precedent.
11. DSR ticket idempotency: `(tenant_id, subject_id, request_type, request_payload_hash)` UNIQUE em Neon staging table dedup; replay → returns same `dsr_id` + receipt.
12. DSR receipt verify endpoint stateless: re-derive HMAC via tenant_short_id + verify signature; no DB lookup needed for verify (privacy: no leakage of submission state to public).
13. Status state machine: **7 states canonical (data_model.md §4.1 source-of-truth pós Lote 10.11.0-bis)**: `received`/`verified`/`queued`/`in_progress`/`completed`/`denied`/`failed`. Transitions atomic via Neon transaction; CHECK constraints garantem `denial_reason` populated em `denied`, `failure_reason` em `failed`, `receipt_jws` em `completed`.
14. Email notification via Cloudflare Email (S-13 inheritance) — 3 locales template integration (WI-S11-004 produces locales).

#### 6.1.7 Neon schema migration `dsr_tickets` (canonical pós Lote 10.11.0-bis)

> **Storage canonical = Neon (NOT D1).** Decisão Lote 10.11.0-bis: PII regulada precisa de PG-side CHECK constraints, FK references, índices condicionais, transactions ACID. D1 SQLite não suporta CHECK com subquery + FK enforcement em tenants cross-tabela. Reviewer R5 Sonnet flagou drift D1/Neon como P0.
> 
> **State machine canonical = 7 states (data_model.md §4.1).** WI v1.0 usou nomes locais (`pending`, `rejected_with_reason`); v1.1 alinhado com canonical (`received`, `denied` + separação `failed` para system errors).

```sql
-- Neon migration N: dsr_tickets — canonical em data_model.md §4.1 entity table
-- ticket_id = UUIDv7 (RFC 9562 §5.7) embeds Unix-ms timestamp para sort-by-arrival
CREATE TABLE dsr_tickets (
  ticket_id                 UUID        PRIMARY KEY,                                  -- UUIDv7
  tenant_id                 UUID        NOT NULL REFERENCES tenant(tenant_id),
  subject_user_id           UUID        NULL REFERENCES user_account(user_id),
  subject_id_hash           TEXT        NOT NULL,                                     -- sha256(subject_id || tenant_salt) audit redact
  request_kind              TEXT        NOT NULL CHECK (request_kind IN ('access','correction','erasure','portability','objection','consent_revoke','confirmation')),
  request_payload_hash      TEXT        NOT NULL,                                     -- sha256(canonical_json(payload)) idempotency
  request_payload_encrypted BYTEA       NOT NULL,                                     -- AES-256-GCM via tenant key (CTRL-CRYPTO-002)
  status                    TEXT        NOT NULL DEFAULT 'received'
                                        CHECK (status IN ('received','verified','queued','in_progress','completed','denied','failed')),
  denial_reason             TEXT        NULL
                                        CHECK (denial_reason IN ('legal_hold','fraud_check','admin_override','jurisdiction_mismatch','duplicate_request')),
  failure_reason            TEXT        NULL,                                         -- free-text post-mortem ref
  received_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
  verified_at               TIMESTAMPTZ NULL,                                         -- post-MFA step-up; clock_start F-11
  expected_completion_at    TIMESTAMPTZ NULL,                                         -- = verified_at + sla_for(request_kind)
  completed_at              TIMESTAMPTZ NULL,                                         -- clock_stop F-11
  failed_at                 TIMESTAMPTZ NULL,
  result_url                TEXT        NULL,                                         -- R2 signed URL access/portability (24h TTL)
  legal_hold                BOOLEAN     NOT NULL DEFAULT false,                       -- pause clock per §6.1 F-11
  pause_total_minutes       INTEGER     NOT NULL DEFAULT 0,                           -- somatório pause; cap 7200 min (5d úteis)
  receipt_jws               TEXT        NULL,                                         -- JWS compact (CTRL-PRIV-DSR-RECEIPT)
  -- idempotency UNIQUE (replay-safe submit)
  UNIQUE (tenant_id, subject_user_id, request_kind, request_payload_hash),
  -- terminal-state invariants (split em CHECKs separados — Lote 10.11.0-bis fix:
  -- comma-separated dentro de single CHECK é invalid SQL syntax)
  CHECK ((status <> 'denied')    OR (denial_reason  IS NOT NULL)),
  CHECK ((status <> 'failed')    OR (failure_reason IS NOT NULL)),
  CHECK ((status <> 'completed') OR (receipt_jws    IS NOT NULL))
);

-- partial index para queries de in-flight tickets (não scan terminal states)
CREATE INDEX idx_dsr_tickets_tenant_status ON dsr_tickets(tenant_id, status)
  WHERE status NOT IN ('completed','denied','failed');
CREATE INDEX idx_dsr_tickets_subject ON dsr_tickets(subject_user_id)
  WHERE subject_user_id IS NOT NULL;
CREATE INDEX idx_dsr_tickets_expected ON dsr_tickets(expected_completion_at)
  WHERE status IN ('verified','queued','in_progress');
```

### 6.2 Componentes C4 afetados

- **Worker-CP** (control plane): novo route handlers em `crates/corelink-privacy-dsr-api`.
- **Neon** (canonical pós Lote 10.11.0-bis): novo table `dsr_tickets` (schema migration N+1 onde N é última de S-10). NOT D1.
- **R2 audit-`<region>`**: 6 canonical CloudEvents types appended (Object Lock 7y).
- **R2 evidence-dsr/**: bucket novo para EVT-048 DSR_EVIDENCE outputs (7y retention canonical aligned com privacy_model.md §8 + R2 Object Lock framework).
- **Cloudflare Email** (S-13 inheritance): templates trigger.
- **Worker `corelink-rate-limit`** (S-08): novo bucket policy DSR.

### 6.3 Arquivos do repositório

```
crates/corelink-privacy-dsr-api/
├─ Cargo.toml
├─ src/
│  ├─ lib.rs                                  # DsrApi trait
│  ├─ http_routes.rs                          # 11 routes (7 submit + 1 verify + 1 status + 1 mfa-verify + admin replay)
│  ├─ dsr_state_machine.rs                    # 7-state transitions
│  ├─ jwt_receipt.rs                          # HMAC-SHA256 sign/verify
│  ├─ idempotency.rs                          # (tenant, subject, type, payload_hash) UNIQUE dedup
│  ├─ rate_limit.rs                           # 10/dia/subject thin wrapper around S-08 bucket
│  ├─ step_up_mfa.rs                          # CTRL-AUTH-010 WebAuthn re-auth integration
│  ├─ audit_emit.rs                           # 6 CloudEvents types fail-CLOSED
│  └─ error.rs                                # DsrApiError taxonomy
├─ migrations/
│  └─ N+1__dsr_tickets.sql                    # canonical DDL §6.1.7
└─ tests/
   ├─ integration_dsr_lifecycle.rs            # signup → submit → mfa → status → verify
   ├─ property_idempotency_replay.rs          # 100k iter replay-safe
   ├─ property_jwt_verify.rs                  # 100k iter sign/verify roundtrip
   ├─ chaos_audit_emit_failure.rs             # fail-CLOSED behavior
   └─ regression_rate_limit_humane.rs         # 10/dia + appeal path
```

### 6.4 Sistemas externos tocados

- **Cloudflare Workers** (HTTP layer + Cloudflare Email).
- **Cloudflare Email Routing** (S-13 inheritance) — transactional email templates.
- **R2** (audit + evidence-dsr buckets).
- **Neon** (dsr_tickets table — canonical pós Lote 10.11.0-bis).

## 7. Anti-Scope

- ❌ DSR UI surface — entregue em S-16 (frontend admin); este WI entrega só backend API.
- ❌ Erasure logic cross-backend (D1/Neon/R2/KV/DO/Stripe/Loki) — entregue em WI-S11-002 (this WI só enqueues).
- ❌ Consent ledger (capture/revoke endpoint /v1/consent/<purpose>) — entregue em WI-S11-003.
- ❌ Privacy notice content + 3 locales translation — entregue em WI-S11-004 (this WI só consome locale code).
- ❌ Sub-processor register email broadcast — entregue em WI-S11-005.
- ❌ Breach notification runbook — entregue em WI-S11-006.
- ❌ Residency pinning routing layer — entregue em WI-S11-007.
- ❌ DPIA/LIA templates + TLA+ dsr_erasure_atomicity — entregue em WI-S11-008.
- ❌ DSR queue management dashboard — entregue em S-13 admin plane.
- ❌ BYOK crypto-erase — entregue em S-14.

### Anti-pattern ❌

❌ Endpoint que retorna dados PII em response body (vaza em logs/proxies); response sempre via R2 signed URL para access/portability. ❌ JWT receipt sem `exp` claim. ❌ Status endpoint público (vaza tenant→subject mapping); auth required. ❌ Verify receipt endpoint que faz DB lookup (vaza submission state); stateless HMAC verify only.

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

### AC-001: DSR submission happy path com step-up MFA

```gherkin
Given um titular autenticado via PAT no tenant "weur"
And o titular tem WebAuthn credential registrada (CTRL-AUTH-010)
When ele POSTa { "request_type": "erasure", "reason": "no_longer_using" } a /v1/privacy/dsr/erasure
Then a resposta é 202 com body { "dsr_id": "<ULID>", "receipt_token": "<JWT>", "expected_completion_at": "<iso8601 +30d>" }
And dsr_tickets row inserida com status = "pending"
And CloudEvents `dev.hugr.corelink.dsr.submitted.v1` emitida em audit-weur (Object Lock 7y)
When o titular POSTa /v1/privacy/dsr/{dsr_id}/verify-mfa com WebAuthn assertion válida
Then a resposta é 200 com body { "status": "verified", "verified_at": "<ts>", "clock_start": "<ts>" }
And dsr_tickets.status = "verified" e verified_at = NOW()
And expected_completion_at = verified_at + 30d (canonical SLA per request_type)
And CloudEvents `dev.hugr.corelink.dsr.verified.v1` emitida
And email enviado em locale do tenant (3 locales possíveis)
```

### AC-002: SLA por direito (7 SLAs distintos)

```gherkin
Given um titular verified em qualquer tenant
When ele submete cada um dos 7 request_types (confirmation, access, correction, erasure, portability, objection, consent_revoke)
Then expected_completion_at é setado conforme privacy_model.md §6.1:
  | request_type     | SLA                  |
  | confirmation     | 5 dias úteis         |
  | access           | 15 dias úteis        |
  | correction       | 5 dias úteis         |
  | erasure          | 30 dias corridos     |
  | portability      | 15 dias úteis        |
  | objection        | 15 dias úteis        |
  | consent_revoke   | 5 minutos            |
And business-day calculation respeita feriados nacionais BR (sam) ou EU (weur) conforme tenant.primary_region
And `corelink_dsr_sla_target_seconds{request_type=...}` Prom gauge populated
```

### AC-003: Idempotency replay-safe

```gherkin
Given um titular que já submeteu DSR erasure com payload P em tenant T
When ele submete novamente o mesmo erasure com payload P em tenant T
Then a resposta é 200 (NÃO 202) com body { "dsr_id": "<existing ULID>", "replay": true, "receipt_token": "<same JWT>" }
And dsr_tickets row NÃO duplicada
And CloudEvents `dev.hugr.corelink.dsr.submitted.v1` NÃO re-emitido (idempotent on replay)
But se payload P' ≠ P (mesmo request_type), nova row inserida com novo dsr_id
```

### AC-004: Step-up MFA gate sensitive ops

```gherkin
Given um titular autenticado via PAT mas SEM WebAuthn credential registrada
When ele POSTa /v1/privacy/dsr/erasure
Then a resposta é 401 com body { "error": "mfa_step_up_required", "remediation": { "register_webauthn": "/v1/auth/webauthn/register" } }
And nenhuma row em dsr_tickets é inserida
And CloudEvents `dev.hugr.corelink.dsr.rejected.v1` emitida com denial_reason = "identity_verification_failed"
```

### AC-005: Rate limit 10/dia/subject humane

```gherkin
Given um titular que já submeteu 10 DSRs no dia (qualquer combinação de request_types)
When ele submete a 11ª DSR no mesmo dia
Then a resposta é 429 com headers Retry-After: <secs até midnight UTC tenant_region> e body inclui appeal_url para Privacy Officer
And CloudEvents `dev.hugr.corelink.dsr.rejected.v1` emitida com denial_reason = "rate_limit_exceeded"
And `corelink_dsr_rate_limit_hits_total{tenant_id_short, subject_id_hash}` incrementado
And LGPD Art. 20 humane: appeal endpoint /v1/privacy/dsr/appeal aceita explicação + roteia para Privacy Officer ≤ 24h
```

### AC-006: JWT receipt verify público stateless

```gherkin
Given um JWT receipt válido `<token>` emitido para tenant T
When alguém (sem auth) GET /v1/privacy/dsr/verify?token=<token>
Then a resposta é 200 com body { "valid": true, "iss": "corelink.dev/privacy", "dsr_id": "<ULID>", "submission_ts": "<ts>", "expected_completion_ts": "<ts>", "request_type": "<type>" }
But `subject_id_hash` é exposto, NÃO o subject_id (CTRL-PRIV-014 audit minimization)
And NO DB lookup is performed (verify is HMAC-only stateless)
And se `<token>` é tampered ou expired, response é 401 com error = "invalid_signature" ou "expired_token"
```

### AC-007: Audit fail-CLOSED em emit failure

```gherkin
Given audit emit infrastructure (R2 audit-<region>) está temporariamente indisponível
When um titular submete DSR
Then a resposta é 503 com body { "error": "audit_emit_unavailable", "retry_after_seconds": 60 }
And NÃO insert em dsr_tickets (transação aborta)
And SEV-2 alert disparado para SRE oncall (HIGH severity per INV registry §3.6 L116 CRITICAL escalates SEV-1)
And rationale: distinto de WI-S10-001 billing fail-OPEN; DSR é regulatory-grade NÃO tolera silent loss (Lote 10.6bis split-tier discipline)
```

### AC-008: Status state machine canonical 7 states

```gherkin
Given um titular tem DSR existente com dsr_id D
When ele GET /v1/privacy/dsr/{D}/status
Then a resposta é 200 com body { "status": "<one of 7 canonical>", "submitted_at": "<ts>", "verified_at": "<ts|null>", "expected_completion_at": "<ts|null>", "completed_at": "<ts|null>", "denial_reason": "<null|enum>" }
And response cacheável via ETag (não-changed states)
And status query rate-limited 1 req/30s/dsr_id (anti-DoS)
And se requester não é tenant owner do dsr_id, response é 404 (não 403; CTRL-ISO-004 constant-time confidentiality)
```

## 9. Design Decisions

### 9.1 Decisões locais (não justificam ADR)

- **DD-001 JWT signing HMAC-SHA256 vs Ed25519**: HMAC simpler + faster; tenant-scoped key derived via HKDF (security_model.md §374); migration path para Ed25519 em S-19 (BYOK enterprise). Tenant_short_id em `aud` claim permite scoping.
- **DD-002 ULID vs UUID para dsr_id**: ULID time-ordered facilita audit chronological queries; 26 chars vs UUID 36; both work em D1 TEXT. Aligned com data_model.md §2.1 ULID for events.
- **DD-003 D1 staging table com `request_payload_encrypted BLOB`**: AES-256-GCM via tenant key (CTRL-CRYPTO-002); request_payload pode conter PII (correction fields) — encrypted at rest per LGPD Art. 32 / GDPR Art. 32.
- **DD-004 Status state machine 7 states (não 5)**: dropping `in_progress` collapses queued+failed retry semantics; mantendo separados melhora customer transparency. Aligned com privacy_model.md §6.1.
- **DD-005 Rate limit bucket scope `dsr:<tenant>:<subject>:<day>`**: anti-cross-tenant leak; UTC midnight rollover por tenant.primary_region (BR holidays vs EU holidays).

### 9.2 Decisões que justificam ADR

- **ADR-S11-001 (NEW)**: DSR Step-Up MFA WebAuthn obrigatório para erasure/correction (não para access/confirmation/portability/consent_revoke/objection). Rationale: balance friction × security; lighter ops não exigem MFA porque (a) access/portability não muta state; (b) consent_revoke é low-risk + GDPR Art. 7 exige "as easy as giving"; (c) objection é manual review by Privacy Officer (humans gate). Erasure + correction são state-mutating cross-backend = HIGH risk de account takeover.
- **ADR-S11-002 (NEW)**: DSR audit fail-CLOSED. Rationale: distinto de WI-S10-001 billing fail-OPEN (Lote 10.6bis split-tier discipline). DSR submission perdida = LGPD Art. 18 violation = multa 2% revenue; cliente aceitar 5xx temporário > regulatory finding.

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| Step-up MFA scope | Always-on para todos 7 ops | Apenas erasure+correction | B | Friction balance + GDPR Art. 7 "as easy as giving" para revoke |
| JWT verify stateless vs DB | Stateless HMAC | DB lookup com revocation | Stateless | Public endpoint = no auth = no DB exposure; revocation deferred to S-19 |
| Rate limit per-subject vs per-tenant | Per-subject 10/dia | Per-tenant 100/dia | Per-subject | Prevent intra-tenant abuse (titular A floods titular B's quota) |
| Idempotency UNIQUE scope | (tenant, subject, type, payload_hash) | (tenant, subject, type) | Quad | Allow legitimate re-submit com dados diferentes (e.g., 2 correções diferentes) |

### Anti-pattern ❌

❌ Step-up MFA em todos endpoints (friction excessiva GDPR Art. 7 violation); ❌ JWT verify com DB lookup público (vaza submission state); ❌ Rate limit per-tenant (intra-tenant abuse possible); ❌ Idempotency apenas por dsr_id (replay-unsafe na rede instável).

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** Crate `corelink-privacy-dsr-api` compila zero warnings em `cargo build --target wasm32-unknown-unknown` (CF Workers Rust per Lote 10.7bis R5 P0-3).
- [ ] **C-1.2** 11 HTTP routes implementadas com path matching.
- [ ] **C-1.3** Neon migration N+1 dsr_tickets (canonical pós Lote 10.11.0-bis) aplica em staging staging-N e roll-back.
- [ ] **C-1.4** Step-up MFA wraps `crates/corelink-auth` WebAuthn primitive (S-03 inheritance, sem reimplementar).
- [ ] **C-1.5** Rate limit reusa `crates/corelink-rate-limit` (S-08), só adiciona DSR bucket policy.
- [ ] **C-1.6** Audit emit fail-CLOSED via `crates/corelink-audit-emit` (S-09, mesmo trait que WI-S09-004).
- [ ] **C-1.7** JWT signer com HMAC tenant-scoped via HKDF (security_model.md §374 inheritance).
- [ ] **C-1.8** No `tokio::spawn` (CF Workers WASM) — fire-and-forget via `worker::send_future` (Lote 10.7bis R5 P0-3 lesson).

### 10.2 Test Completeness

- [ ] **T-2.1** Integration test full DSR lifecycle (signup→submit→mfa→status→verify) verde.
- [ ] **T-2.2** Property test idempotency 100k iter (replay-safe semantic preserved).
- [ ] **T-2.3** Property test JWT roundtrip 100k iter (sign + verify match).
- [ ] **T-2.4** Chaos test audit emit failure: fail-CLOSED behavior assertion.
- [ ] **T-2.5** Regression test rate limit 10/dia + appeal path.
- [ ] **T-2.6** Cross-tenant attack test: tenant A token tries DSR em tenant B → 403 + audit event.
- [ ] **T-2.7** SLA calculation test: 7 request types × 6 regions × business-day vs corridos × holiday calendar.
- [ ] **T-2.8** Step-up MFA bypass test (PAT alone) → 401 (must fail without WebAuthn assertion).

### 10.3 Documentation Completeness

- [ ] **D-3.1** `docs/api/privacy-dsr.md` OpenAPI 3.1 com 11 endpoints + JSON schemas.
- [ ] **D-3.2** `docs/dev/dsr-state-machine.md` com diagram (mermaid) das 7 states + transitions.
- [ ] **D-3.3** `specs/05_quality/runbooks/RB-DSR-INTAKE-FAILURE.md` SOP para SRE oncall.
- [ ] **D-3.4** Privacy notice update em WI-S11-004 referencia este endpoint.

### 10.4 Observability Completeness

- [ ] **O-4.1** 4 Prom metrics: `corelink_dsr_request_total{request_type,status,tenant_tier}`, `corelink_dsr_sla_met_total{request_type}`, `corelink_dsr_step_up_mfa_total{outcome}`, `corelink_dsr_rate_limit_hits_total`.
- [ ] **O-4.2** 1 dashboard `corelink-dsr-pipeline` em Grafana (S-09 inheritance) com 6 panels.
- [ ] **O-4.3** 6 CloudEvents canonical types emitidos em audit-`<region>` Object Lock 7y.
- [ ] **O-4.4** 1 trace per DSR submit (W3C tracecontext propagation; per Lote 10.9bis P0-A canonical).

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** Step-up MFA WebAuthn obrigatório erasure+correction (CTRL-AUTH-010).
- [ ] **S-5.2** subject_id NUNCA em logs (apenas subject_id_hash) — CTRL-PRIV-014.
- [ ] **S-5.3** request_payload encrypted at rest D1 (AES-256-GCM) — CTRL-CRYPTO-002.
- [ ] **S-5.4** JWT exp ≤ submission + max_sla + 90d grace (anti-replay).
- [ ] **S-5.5** Rate limit anti-DoS humane com appeal path (LGPD Art. 20).
- [ ] **S-5.6** Cross-tenant attack vectors mitigated via TenantCtx-only (S-03 inheritance).
- [ ] **S-5.7** DLP scan CI gate: nenhum subject_id raw em logs/metrics/traces.

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui `corelink-privacy-dsr-api` crate (ADR-0014 inheritance).
- [ ] **B-6.2** Sigstore provenance attestation (CTRL-SUPPLY-001).
- [ ] **B-6.3** Cargo deny isolation (sem novas deps direct sem ADR).

## 11. DoD

Acima 10.x checked + sign-off matrix §30 12 confirmados + chaos test 30d staging + SLO-FRESH-DSR-ERASURE foundation (event emit verde).

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-001 |
|---|---|---|---|
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | 6 CloudEvents types em audit-`<region>` Object Lock 7y; emit fail-CLOSED com transaction abort; hash chain via PAT-AUDIT-VERIFY-001 daily |
| **INV-DATA-ERASURE-COMPLETE** (foundation) | HIGH | invariant_registry.md §3.5 L110 | API enqueues to WI-S11-002 erasure worker; this WI does NOT execute erasure; invariant satisfação completa em WI-S11-002 |

## 13. Artifacts Produced

- `crates/corelink-privacy-dsr-api/` (NEW crate ~3500 LoC).
- `migrations/N+1__dsr_tickets.sql` (DDL + indexes).
- `docs/api/privacy-dsr.md` OpenAPI 3.1.
- `docs/dev/dsr-state-machine.md` (mermaid).
- `specs/05_quality/runbooks/RB-DSR-INTAKE-FAILURE.md` (NEW).
- 6 CloudEvents schemas em `schemas/cloudevents/dsr-{submitted,verified,queued,completed,failed,rejected}.v1.json`.
- Grafana dashboard JSON `dashboards/corelink-dsr-pipeline.json`.
- 4 Prom metrics + alerts em S-09 inheritance.

## 14. Quality Standards SOTA

- **14.s11.1.1** Zero PII em audit logs (CTRL-PRIV-001) — DLP CI scan + DLP runtime.
- **14.s11.1.2** Step-up MFA latency p95 ≤ 1.2s (WebAuthn navigator API roundtrip).
- **14.s11.1.3** JWT verify endpoint p95 ≤ 50ms (stateless HMAC).
- **14.s11.1.4** DSR submit p95 ≤ 800ms (D1 insert + audit emit + email enqueue).
- **14.s11.1.5** Idempotency replay 100k iter zero false-negative (replay returns same dsr_id).
- **14.s11.1.6** SLA calculation deterministic across 6 canonical regions × business-day calendar.
- **14.s11.1.7** Cross-tenant attack: 0 leaks em 100k random pairs property test.
- **14.s11.1.8** INV §3.X positions canonical verified pre-merge (Lote 10.8bis P1-13 lesson).

## 15. Chaos Experiments (8)

1. R2 audit unavailable → DSR submit returns 503 (fail-CLOSED).
2. Neon dsr_tickets unavailable → 503 com retry-after.
3. Rate limit DO offline → fallback policy: deny (fail-closed quota).
4. WebAuthn assertion validation latency > 5s → timeout + retry-after.
5. JWT signing key rotation mid-flight → both old + new keys aceitos por 24h grace.
6. Email worker queue backlog → SLA "humane warning" sent late but not lost.
7. Step-up MFA assertion replay → rejected via WebAuthn challenge nonce (S-03 inheritance).
8. Cross-region tenant routing failure → 503 + traffic mirror to peer region (S-09 inheritance).

## 16. PRR

- [ ] PRR HIGH_RISK 12 sign-offs (§30) confirmados.
- [ ] Chaos 30d em staging com SEV-2 alert real test.
- [ ] DLP scan + DLP runtime green.
- [ ] Privacy Officer + Compliance + Architect mandatory emphatic.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml + lib.rs | 1.5h |
| ST-002 | DsrApi trait + 7 request types enum | 2h |
| ST-003 | Neon migration N+1 dsr_tickets (canonical pós Lote 10.11.0-bis) DDL + tests | 1.5h |
| ST-004 | 11 HTTP routes (7 submit + verify + status + mfa-verify) | 4h |
| ST-005 | DSR state machine 7-state transitions | 2h |
| ST-006 | JWT receipt sign + verify HMAC HKDF | 2h |
| ST-007 | Idempotency UNIQUE quad + replay tests | 2h |
| ST-008 | Step-up MFA WebAuthn integration (S-03 wrap) | 2h |
| ST-009 | Rate limit DSR bucket (S-08 wrap) | 1h |
| ST-010 | Audit emit 6 CloudEvents fail-CLOSED | 1.5h |
| ST-011 | OpenAPI 3.1 + state machine doc + RB-DSR-INTAKE-FAILURE | 2h |
| ST-012 | 4 Prom metrics + Grafana dashboard | 1.5h |

**PERT total**: ~23.0h (alinha com sprint contract §12).

## 18. Dependencies

- **Hard**: S-03 SEALED (PAT auth + WebAuthn); S-09 SEALED (audit emit + obs stack); S-08 SEALED (rate limit bucket); S-10 SEALED (foundation patterns CloudEvents prefix); spec contract S-11 v1.2.0 SEALED.
- **Soft**: WI-S11-002 (erasure worker — this WI's POST /erasure enqueues there); WI-S11-004 (privacy notice locales — email templates consume).

## 19. Effort PERT: ~23.0h. ## 20. Time-boxing: 32h hard limit (lane HIGH_RISK +40% buffer).

## 21. Observability

4 Prom metrics + 1 Grafana dashboard + 6 CloudEvents types + W3C trace exemplars per Lote 10.9bis P0-A canonical.

## 22. Cost Analysis

- **Neon** (canonical Lote 10.11.0-bis): dsr_tickets table low-volume (~100/month/tenant em early stage); negligible cost; PG-side CHECK + FK adicionalmente protege regulatory invariants.
- **R2 audit-`<region>`**: 6 events/DSR avg; 7y Object Lock; ≈$0.02/tenant/year.
- **R2 evidence-dsr/**: access/portability bundles; max ~50MB/DSR; 7y retention canonical; ≈$0.05/tenant/year.
- **Cloudflare Email**: 4 emails/DSR avg (submit confirm + mfa verify + 14d warn + complete); ≈$0.001/email.
- **WebAuthn validation CPU**: amortized via S-03 worker pool; negligible delta.
- **Total estimated**: ≤ $0.10/tenant/year (well under §14 budget cap).

## 23. API Contract

OpenAPI 3.1 em `docs/api/privacy-dsr.md`. Request/response schemas validated via `validate_specs.py`. CloudEvents schemas em `schemas/cloudevents/dsr-*.v1.json`. JWT receipt format documented em `docs/api/dsr-receipt-spec.md`.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| INV-AUDIT-APPEND-ONLY violation (DSR audit event NOT em R2 7y) | CRITICAL | Privacy Officer + SecLead + SRE |
| INV-DATA-ERASURE-COMPLETE foundation gap (DSR submitted mas não enqueued) | HIGH | Privacy Officer + Architect |
| Step-up MFA bypass exploit (erasure submit sem WebAuthn) | CRITICAL | SecLead + Privacy + AppSec |
| Cross-tenant DSR leak (tenant A reads tenant B DSR) | CRITICAL | SecLead + Privacy + Compliance |
| SLA miss > 30d em production (LGPD Art. 18 violation) | HIGH | Privacy Officer + Legal escalation |
| JWT signing key compromise | CRITICAL | SecLead + Compliance + Privacy |
| Rate limit miscalibrated (legitimate user blocked) | MEDIUM | Privacy Officer (LGPD Art. 20 humane review) |
| Audit emit fail-OPEN regression (Lote 10.6bis split-tier violation) | CRITICAL | Architect + Privacy |

## 25. Rollback / Recovery

Feature flag `dsr_api_enabled` em DO config-singleton; flip → 503 com fallback message para email Privacy Officer. SLA pause valid (privacy_model.md §6.1 indisponibilidade técnica documentada cap 5d úteis somatório).

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): subject_id_hash (não plaintext) em audit per CTRL-PRIV-014.
- I(dentifiability): step-up MFA WebAuthn challenge garantia identidade — replay impossible.
- N(on-repudiation): R2 Object Lock 7y + hash chain immutable.
- D(etectability): public verify endpoint stateless — não vaza submission state via timing/error msg.
- D(isclosure): request_payload encrypted at rest AES-256-GCM (CTRL-CRYPTO-002); no PII em logs.
- U(nawareness): privacy notice (WI-S11-004) explica DSR rights + endpoint URLs.
- N(on-compliance): **LGPD Art. 18 + GDPR Art. 15-22 + CCPA §1798.105/115/120/125 + SOC 2 P5.1..5.2 compliance** via 7-endpoint canonical + audit trail 7y + 7y EVT-048 (canonical).

## 27. Knowledge Transfer

Tech talk (1h): "S-11 DSR API: 7 SLAs distintos + step-up MFA + JWT receipt + audit fail-CLOSED split-tier"; doc `docs/dev/privacy-dsr-architecture.md`; onboarding test 6 questões: 7 SLAs canonical, step-up MFA scope (apenas erasure/correction), idempotency quad, JWT verify stateless rationale, audit fail-CLOSED vs billing fail-OPEN (Lote 10.6bis), CTRL-AUTH-010 vs CTRL-PRIV-016 distinction.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-AUDIT-APPEND-ONLY violation (DSR event lost) | L | M | CRITICAL | M | LOW | Audit fail-CLOSED + transaction abort + chaos 30d; SEV-1 alert (CRITICAL) |
| R-002 | Cross-tenant DSR access (tenant A reads B) | L | M | CRITICAL | L | LOW | TenantCtx-only S-03 + property test 100k cross-tenant pairs |
| R-003 | Step-up MFA bypass via PAT alone | L | M | CRITICAL | L | LOW | CTRL-AUTH-010 mandatory erasure+correction; integration test verifies 401 sem WebAuthn |
| R-004 | JWT receipt forged via key leak | L | L | HIGH | L | LOW | HMAC tenant-scoped via HKDF; key rotation S-19; revocation list deferred |
| R-005 | SLA miscalculation (business-day vs corrido) | L | M | HIGH | M | LOW | Property test 7 types × 6 regions × holiday calendar; canonical helper `corelink_time::sla_for(request_type, region)` |
| R-006 | Rate limit blocks legitimate user (LGPD Art. 20) | M | L | MEDIUM | L | LOW | Appeal endpoint humane review + Privacy Officer SOP RB-DSR-INTAKE-FAILURE |
| R-007 | Idempotency cache miss → duplicate dsr_id | L | M | MEDIUM | L | LOW | UNIQUE quad em D1 staging; chaos test concurrent submits |
| R-008 | request_payload PII em D1 plaintext | L | L | CRITICAL | L | LOW | AES-256-GCM at-rest CTRL-CRYPTO-002; tenant key from KMS |
| R-009 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.5 L110 + §3.6 L116 verified Lote 10.11.0; ongoing maintenance via grep CI gate |
| R-010 | WebAuthn assertion replay attack | L | L | HIGH | L | LOW | Challenge nonce per assertion (S-03 inheritance) + audit |
| R-011 | Email locale mismatch (PT-BR titular receives EN) | L | L | LOW | L | LOW | Email template integration WI-S11-004; tenant.locale_default fallback |
| R-012 | Audit fail-OPEN regression (Lote 10.6bis violation) | L | L | CRITICAL | L | LOW | ADR-S11-002 explicit; integration test asserts 503 em emit failure |

## 29. Review Checkpoints

D+0 design review (Architect; split-tier audit fail-CLOSED rationale); D+1 Privacy Officer (LINDDUN + LGPD Art. 18 mapping); D+2 SecLead (CTRL-AUTH-010 + JWT signing); D+3 Legal (DPA reference at sprint level); D+4 Compliance (SOC 2 P5.1..5.2 mapping); D+5 code review; D+6 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — CTRL-AUTH-010 + STRIDE + cross-tenant attack vectors_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 30d + property test 100k cross-tenant_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 P5.1..5.2 + LGPD Art. 18 + GDPR Art. 15-22 mapping_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + CTRL-PRIV-014 audit minimization + 7 SLA correctness_ |
| 11 | Architect | _TBD; **mandatory emphatic** — split-tier fail-CLOSED audit (Lote 10.6bis) + INV §3.X verification + ADR-S11-001/002_ |
| 12 | DPO interim (Gustavo até hire) | _TBD; **mandatory emphatic** — 7-right canonical + identity verification step-up + audit 7y_ |

(Legal sign-off via DPA reference at sprint level; not per-WI per Lote 10.10-quaters R4 NEW-P1-4 lesson.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.2.0 | 2026-05-03 | Gustavo (impl SEAL) | **WI-S11-001 SEALED — implementation phase complete.** New crate `crates/corelink-dsr/` (~4447 LOC; 8 src modules + 1 prop test file): `DsrEndpoint` trait + `InMemoryDsrEndpoint` orchestrator wired to (a) `JwtReceiptIssuer` trait + `InMemoryJwtReceiptIssuer` deterministic fake (RS256 alg pinned + canonical issuer `corelink.dev/privacy` + 90d exp claim cap per anti-replay invariant + cross-key reject pinned by property test); (b) `MfaStepUpVerifier` trait + `InMemoryMfaStepUpVerifier` (CTRL-AUTH-010 gate ONLY for destructive arms — Erasure + Rectification — per ADR-S11-001 friction-vs-security trade-off); (c) `DsrRequestStore` trait + `InMemoryDsrRequestStore` + `FailingDsrRequestStore` (canonical durable mirror surface; production wiring at WI-S11-008 binds Neon `dsr_tickets` per data_model.md §4.1); (d) `DsrAuditSink` 7-event canonical taxonomy `corelink.dsr.{request_received, mfa_step_up_required, mfa_verified, request_accepted, receipt_issued, request_rejected, status_polled}` audit fail-CLOSED envelope BEFORE state mutation per ADR-S11-002 split-tier (DSR regulatory-grade, NEVER tolerates silent loss; distinct from billing fail-OPEN at Lote 10.6bis). Per-instance `Arc<Mutex<()>>` F-001 closure. Canonical 6-arm `DsrRequestKind` `#[non_exhaustive]` (Access / Portability / Rectification / Erasure / Restriction / Objection); 4-arm `DsrStatus` (Pending / InProgress / Completed / Rejected); 4-arm `DsrDecision` (RequestAccepted / RequestRejected / MfaRequired / StatusPolled); 3-arm `DsrJurisdiction` (Lgpd 15d / Gdpr 30d / Ccpa 45d) with canonical `sla_for(jurisdiction, submitted_at)` helper. Tests: 94 inline unit + 8 sanity + 11 property at 10k iter (PROPTEST_CASES env-var override per S-07 P1-2; ChaCha20Rng pinned per charter). Property coverage: `prop_jwt_receipt_verifies_post_facto` (sign+verify roundtrip + cross-key reject + tampered + expired); `prop_mfa_required_for_erasure_rectification` (CTRL-AUTH-010 + ADR-S11-001 canary); `prop_request_id_uniqueness` (UUIDv7 < 2^-64 collision); `prop_status_poll_idempotent` (n_invocations stable response); `prop_sla_deadline_correct` (15d/30d/45d × random submitted_at); `prop_tenant_isolation` (CTRL-ISO-004 constant-time confidentiality cross-tenant poll → IdentityVerificationFailed); `prop_audit_emit_per_decision_arm` (INV-AUDIT-APPEND-ONLY + ADR-S11-002 split-tier); `prop_receipt_expires_at_90d` (anti-replay cap); `prop_unsupported_kind_rejected` (typed enum boundary); `prop_destructive_predicate_stable` (6-arm taxonomy stable); `prop_idempotent_resubmission` (same `(tenant_id, request_id)` reuses prior). Quality gates verde: `cargo test -p corelink-dsr --all-targets` 113/113 pass; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` zero warnings; `validate_specs.py` 282 schema + 6 YAML-only = 288 total clean; `check_migrations_additive.py` 21 migrations all additive. Trait-abstraction-defer per charter: production CF Worker route POST `/v1/privacy/dsr/{access|portability|rectification|erasure|restriction|objection}` + GET `/v1/privacy/dsr/{request_id}/status`, real RS256 sign via `jsonwebtoken=9.3` (S-03 inheritance) + KMS-backed key rotation (HKDF info=`corelink/v1/dsr-receipt`), real WebAuthn step-up TOTP/FIDO2 binding (S-03 WI-S03-006 inheritance), Neon `dsr_tickets` durable mirror (canonical 7-state machine per data_model.md §4.1; UNIQUE quad idempotency), Cloudflare Email Routing 3-locale templates, S-08 rate-limit bucket cooperation (`dsr:<tenant>:<subject>:<day>` 10/day humane LGPD Art. 20), 6 CloudEvents fan-out to S-09 chain extension, SLA pause logic on `legal_hold`, RB-DSR-INTAKE-FAILURE runbook, OpenAPI 3.1 publication, Grafana `corelink-dsr-pipeline` dashboard — all deferred to WI-S11-008 PRR ship gate. **No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-11 corpus.** |
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **dsr_tickets storage canonical = Neon (NOT D1)** — data_model.md §4.1 source-of-truth; PG-side CHECK constraints + FK `tenant`/`user_account`; índices condicionais; CHECK garantem denial_reason populated em `denied`, failure_reason em `failed`, receipt_jws em `completed`. (b) **State machine canonical = 7 states** (received/verified/queued/in_progress/completed/denied/failed) substituindo nomes locais v1.0 (pending → received; rejected_with_reason → denied + failed split). (c) **ticket_id = UUIDv7** (RFC 9562 §5.7) substituindo ULID; embeds Unix-ms timestamp para sort-by-arrival. (d) **GPT P1-4 verify endpoint stateless rigoroso**: `DsrReceiptVerified` agora STRICT subset do JWT (signature_valid + temporal_status derived from `now()`; ZERO DB lookup; ZERO state enrichment); status pipeline state movido EXCLUSIVAMENTE para auth'd `/v1/privacy/dsr/{ticket_id}/status` endpoint (privacy: prevent enumeration via público verify endpoint). (e) **HKDF info canonical** = `corelink/v1/dsr-receipt` (security_model.md §7.2 + key_management.md §2). (f) **INV severity cascade**: INV-DATA-ERASURE-COMPLETE HIGH→CRITICAL (TLA+ commit S-11 WI-S11-008). v1.1 ainda preserva ADR-S11-001/002 sem mudança. |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-001; HIGH_RISK; SOTA pós-S-10 SEALED 9.35/10. Foundation primary surface CoreLink Privacy Pipeline: 7 endpoints REST POST + 1 verify endpoint público stateless + 1 status endpoint + 1 step-up MFA verify endpoint = 11 routes total. SLAs distintos por direito conforme privacy_model.md §6.1 clock semantics F-11 (verified→completed; pause cap 5d úteis legal hold/ambiguity). CTRL-AUTH-010 step-up MFA WebAuthn obrigatório erasure+correction (não outros — ADR-S11-001 rationale). Audit fail-CLOSED 6 CloudEvents canonical types `dev.hugr.corelink.dsr.{submitted,verified,queued,completed,failed,rejected}.v1` per Lote 10.9bis P0-G prefix — distinto de WI-S10-001 billing fail-OPEN (Lote 10.6bis split-tier discipline; ADR-S11-002). Rate limit 10/dia/subject reusing S-08 quota state machine; humane appeal path LGPD Art. 20. JWT receipt HMAC-SHA256 tenant-scoped via HKDF; verify endpoint público stateless (no DB lookup; CTRL-PRIV-014 audit minimization). Idempotency UNIQUE quad (tenant_id, subject_id, request_type, request_payload_hash) em D1 dsr_tickets; replay returns same dsr_id + receipt. INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116) + INV-DATA-ERASURE-COMPLETE (CRITICAL — Lote 10.11.0-bis §3.5 L110, foundation; satisfação completa em WI-S11-002). 8 AC scenarios + 8 chaos + 12 risks + 8 post-mortem hooks. **Lote 10.10 lessons absorbed**: (a) source-of-truth FIRST — INV positions §3.5 L110 + §3.6 L116 verified pre-merge (Lote 10.8bis P1-13); (b) typed enum NÃO serde_json::Value (Lote 10.9-quinquies NEW-P0-2); (c) sign-off cap 12 (Lote 10.8bis P1-2); (d) cascade discipline absoluta — sweep todas seções para correlação SLA/CTRL/INV; (e) split-tier discipline canonical (Lote 10.6bis fail-OPEN billing vs fail-CLOSED audit). |

## 32. Anti-patterns evitados

- ❌ Step-up MFA always-on (friction GDPR Art. 7 violation); ❌ JWT verify com DB lookup público (vaza submission state); ❌ Rate limit per-tenant (intra-tenant abuse); ❌ Idempotency apenas dsr_id (replay-unsafe); ❌ subject_id raw em logs (CTRL-PRIV-014 violation); ❌ Audit fail-OPEN (regulatory finding); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ INV §3.X position TBD (Lote 10.8bis P1-13); ❌ serde_json::Value em DsrRequest (Lote 10.9-quinquies NEW-P0-2); ❌ status query público (vaza tenant→subject mapping); ❌ DSR endpoint sem expected_completion_at (titular não consegue rastrear SLA).

---
