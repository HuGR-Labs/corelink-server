---
id: "WI-S03-007"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-03"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "OBSERVABILITY-MODEL"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s03", "auth", "audit", "evt-047", "cloudevents", "chain-integrity", "high-risk"]
---

# WI-S03-007 — Audit Events EVT-047 (auth.*) + CloudEvents 1.0 Envelope + Chain Integrity Alignment S-09

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-03](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S03-007 |
| Título | Audit events `auth.*` taxonomia EVT-047 + CloudEvents 1.0 envelope + chain integrity alignment S-09 + PII redact + per-tenant retention |
| Sprint | S-03 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (audit gap = compliance + forensic loss), FF-HR-005 (chain integrity = cripto SHA-256 hash chain), FF-HR-009 (Layer 5 of 5-layer defense — audit cross-check) |

## 1. Intent

Implementar taxonomia completa de eventos `auth.*` (EVT-047 family) emitidos por todos os WIs auth (S-03 WI-001 ... 006) consumidos pelo S-09 audit chain (forward dependency). Garantir:

1. **CloudEvents 1.0 envelope canônico** (W3C-compatible).
2. **Chain integrity alignment**: cada evento contribui ao hash chain S-09 com `prev_hash + content_hash` SHA-256.
3. **PII redaction macros**: `email_hash`, `principal_id_hash`, `pat_id_hash` derivados; raw PII NUNCA em chain.
4. **Per-tenant retention**: tenant tier dictates retention (solo: 30d; team: 90d; business: 1y; enterprise: 7y default + custom).
5. **Audit emit ordering**: pre-handler (validate, denied) + post-handler (success, error) reuse outbox pattern WI-S01-005.

```rust
// crates/corelink-audit/src/events.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthEvent {
    // CloudEvents 1.0 mandatory fields
    pub specversion: String,        // "1.0"
    pub id: String,                 // UUID v7 — global event id
    pub source: String,             // URI: corelink://region/auth/<wi-id>
    pub type_: AuthEventType,       // enum (vide §1.2)
    pub time: SystemTime,           // RFC 3339
    pub datacontenttype: String,    // "application/json"
    pub subject: Option<String>,    // tenant_id em pseudonymous form

    // CoreLink extension fields
    pub tenant_id: TenantId,
    pub principal_id_hash: String,  // sha256(principal_id) prefix 16 hex chars
    pub region: Region,
    pub request_id: RequestId,      // correlation across handler chain
    pub data: AuthEventData,        // type-specific payload

    // Chain integrity (S-09 forward; este WI provê fields)
    pub prev_hash: Option<String>,  // SHA-256 previous event in chain
    pub content_hash: String,       // SHA-256 of (specversion + id + ... + data)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "fields")]
pub enum AuthEventType {
    // EVT-047 family — token lifecycle
    #[serde(rename = "auth.token.issued")]
    TokenIssued,
    #[serde(rename = "auth.token.validated")]
    TokenValidated,
    #[serde(rename = "auth.token.revoked")]
    TokenRevoked,
    #[serde(rename = "auth.token.expired")]
    TokenExpired,

    // Auth flow events
    #[serde(rename = "auth.session.created")]
    SessionCreated,            // Clerk login
    #[serde(rename = "auth.session.revoked")]
    SessionRevoked,
    #[serde(rename = "auth.denied.scope")]
    DeniedScope,
    #[serde(rename = "auth.denied.rate_limit")]
    DeniedRateLimit,
    #[serde(rename = "auth.denied.invalid")]
    DeniedInvalid,

    // Tenant + membership lifecycle
    #[serde(rename = "auth.tenant.provisioned")]
    TenantProvisioned,
    #[serde(rename = "auth.account.created")]
    AccountCreated,
    #[serde(rename = "auth.membership.added")]
    MembershipAdded,
    #[serde(rename = "auth.membership.removed")]
    MembershipRemoved,
    #[serde(rename = "auth.membership.role_changed")]
    MembershipRoleChanged,

    // WebAuthn events (WI-S03-006)
    #[serde(rename = "auth.webauthn.registered")]
    WebauthnRegistered,
    #[serde(rename = "auth.webauthn.authenticated")]
    WebauthnAuthenticated,
    #[serde(rename = "auth.webauthn.deleted")]
    WebauthnDeleted,
    #[serde(rename = "auth.webauthn.sign_count_regression")]
    WebauthnSignCountRegression,   // SEV-1 alert
    #[serde(rename = "auth.webauthn.origin_attack_attempt")]
    WebauthnOriginAttackAttempt,    // active attack signal
    #[serde(rename = "auth.webauthn.new_device_used")]
    WebauthnNewDeviceUsed,          // anomaly — passkey synced via iCloud Keychain to new device (P0 fix Lote 10.3bis cross-WI taxonomy gap)

    // DSR / lifecycle events (P0 fix Lote 10.3bis added)
    #[serde(rename = "auth.account.deleted")]
    AccountDeleted,                 // DSR erasure trigger; LGPD Art. 18
    #[serde(rename = "auth.tenant.deleted")]
    TenantDeleted,                  // tenant lifecycle erasure
    #[serde(rename = "auth.pat.scope_escalated")]
    PatScopeEscalated,              // anomaly — PAT scope mutation detected (privileged op via WI-S03-004 OR insider abuse)

    // Auth.denied.* sub-types (P0 fix Lote 10.3bis — granularidade SIEM)
    #[serde(rename = "auth.denied.signature_invalid")]
    DeniedSignatureInvalid,
    #[serde(rename = "auth.denied.expired")]
    DeniedExpired,
    #[serde(rename = "auth.denied.scope_insufficient")]
    DeniedScopeInsufficient,        // (replaces generic DeniedScope; mais granular)
    #[serde(rename = "auth.denied.not_found")]
    DeniedNotFound,
    #[serde(rename = "auth.denied.malformed")]
    DeniedMalformed,
    #[serde(rename = "auth.denied.revoked")]
    DeniedRevoked,

    // Admin op events
    #[serde(rename = "auth.admin_op.webauthn_authenticated")]
    AdminOpWebauthnAuthenticated,
    #[serde(rename = "auth.admin_op.mass_revoke")]
    AdminOpMassRevoke,

    // Anomaly events
    #[serde(rename = "auth.anomaly.cross_region_burst")]
    CrossRegionBurst,
    #[serde(rename = "auth.anomaly.token_replay_detected")]
    TokenReplayDetected,
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Audit é o último elo da defesa: se prevention falha, audit detecta + atribui + responsabiliza. Bug em audit = forensic gap + compliance breach (SOC 2 + LGPD + GDPR todos requerem completeness). Bugs catastróficos:

1. **Audit emission gap em hot path**: handler succeeds mas audit emit falha silently → compliance breach undetected. Mitigação: outbox pattern (reuse WI-S01-005 — audit_outbox INSERT em mesma D1 batch que TenantCtx commit; falha = transação rolls back; cliente vê 503 não silent gap). Drain worker drains pós-response; at-least-once.

2. **PII em audit chain (LGPD/GDPR violation)**: `email`, raw `principal_id`, raw `pat_id` em chain = mass PII exposure se chain leak. Mitigação: `redact_pat!` macro mandatory; principal_id_hash, pat_id_hash derived sha256 prefix 16 hex; raw values NUNCA inserted em chain. CI lint enforces.

3. **Chain integrity break**: prev_hash mismatch entre eventos consecutivos = chain tampered OR ordering bug. Mitigação: S-09 forward implements hash chain validation; este WI emite com `content_hash = sha256(serialized_event_minus_chain_fields)`; consumer recomputes + verifies.

4. **Event type drift**: dev adds new event type sem registrar em taxonomia → consumer downstream (S-09 chain, SIEM) ignora. Mitigação: enum exhaustive em Rust; sprint contract sign-off requires updating event registry doc.

5. **Per-tenant retention violation**: solo tier promised 30d; bug retains 365d → privacy issue (DSR erasure complications). Mitigação: retention worker (S-11 forward) honors `tenant.tier`; este WI annotates events com tenant_tier hint para downstream.

6. **Backward compatibility em event schema**: change to `AuthEventType` enum breaks consumer parsers. Mitigação: `#[non_exhaustive]` + adicionar variants; never delete; deprecation policy 1 yr.

7. **Volume + cost**: 10M req/dia × 1 audit event = 10M events/dia × 200 bytes/event = 2 GB/dia × 365 = 700 GB/yr storage. Solo tier 30d retention manageable; enterprise 7y = 5 TB/tenant. Mitigação: tenant tier × retention budget; cold storage S-09 forward; sampling for non-CRITICAL events (admin opt-in).

8. **Anomaly detection signal loss**: `auth.anomaly.*` events designed para SIEM correlation; bug em fan-out (drain worker → SIEM) = blind spot. Mitigação: dual fan-out (S-09 chain + direct SIEM webhook); alert se SIEM lag > 5min.

9. **Cross-region event ordering**: events em múltiplas regiões com clock skew; chain ordering depends on accurate timestamps. Mitigação: `time` em CloudEvents é authoritative server clock (NTP-synced); mismatched clocks alertados via SLO.

10. **Pseudonymization reversibility**: `principal_id_hash` is one-way; auditor cannot reverse-lookup who. For internal incident response, hashed_principal → real lookup via privileged query (logged + audit chain shows access).

**Atacante adversarial scenarios**:

- **Audit chain tampering**: insider with chain DB write tries to remove event. S-09 chain integrity (forward) detects via hash chain validation; tampering = chain hash break alarm.
- **Audit log poisoning**: adversário tries inject fake events. Mitigação: events emitted only by app services (TenantCtx authenticated); outbox INSERT requires app DB role; cliente cannot direct write.
- **Audit denial of service**: adversary floods with `auth.denied.*` events; chain bloats. Mitigação: per-PAT rate limit S-08 caps em authentication path; audit emit é proportional.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: audit gap = forensic loss em cross-tenant incident.
- **FF-HR-005**: chain integrity é cripto-coordenado (SHA-256 chain); break = cripto correctness boundary.
- **FF-HR-009**: Layer 5 of 5-layer defense; cross-check + last-line accountability.
- **Reversibility**: audit gap detected late (compliance audit OR incident); proativo via property test + chain integrity validation.

13 sign-offs incl. Compliance (mandatory; SOC 2 + LGPD), Privacy (PII redact), Crypto SME (chain hash + redact macro).

## 3. Customer Impact & Journey

**Persona 1 — Compliance auditor**:
- Audit query `SELECT * FROM audit_chain WHERE type LIKE 'auth.%' AND tenant_id = ? AND time > ?`.
- Returns CloudEvents 1.0 JSON; PII redacted; chain integrity validated.
- SOC 2 evidence + LGPD Art. 38 (registro de operações) report generation.

**Persona 2 — Security analyst investigando incident**:
- Filter chain: `auth.anomaly.*` + `auth.webauthn.sign_count_regression` + `auth.denied.*` per tenant.
- Correlate via `request_id` across multiple events (e.g., login → token.validated → admin_op → mass_revoke).
- Forensic timeline reconstruction.

**Persona 3 — DSR (LGPD erasure) processing**:
- User requests erasure → DSR worker (S-11) traverses chain por user_account_id_hash.
- Pseudonymization preserves chain integrity sem re-identifying.
- LGPD Art. 18 (right to erasure) satisfied; audit chain retained for legal hold.

**SLA addendum**:
- Audit visibility lag ≤ 60s p99 (outbox drain cadence; documented em SLA).
- Per-tenant retention: solo 30d / team 90d / business 1y / enterprise 7y default.
- Chain integrity validation real-time via S-09 (forward).

## 4. Capability Mapping

- **CAP-AUDIT-001** (Audit event taxonomy + emission) — IMPLEMENTA primary.
- **CAP-AUDIT-002** (Chain integrity alignment) — IMPLEMENTA primary (alignment; full chain em S-09).
- **CAP-AUDIT-003** (Per-tenant retention) — IMPLEMENTA primary (annotations; worker em S-11).
- **CAP-PRIVACY-003** (PII redaction em chain) — IMPLEMENTA primary.
- Trace: `observability_model.md §3 (Audit events)` + `auth_model.md §3.13.7 (Auth events)` + `privacy_model.md §3.5 (PII redaction)` + `security_model.md CTRL-AUDIT-001/002`.

## 5. Tipo

Audit emit infrastructure + taxonomy + envelope; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-audit/` workspace member**:
   - `lib.rs` exposes `AuthEvent`, `AuthEventType`, `AuthEventData`, `Emitter`, `Redactor`.
   - Cargo.toml: `serde`, `serde_jcs = "0.1"` (RFC 8785 JCS — JSON Canonicalization Scheme; audited; deterministic + Unicode NFC native; P0 fix Lote 10.3bis), `serde_json` (apenas para non-canonical paths), `uuid` (v7), `sha2`, `subtle`.

2. **CloudEvents 1.0 envelope** (W3C compliant):
   - All required fields (`specversion`, `id`, `source`, `type`, `time`, `datacontenttype`).
   - CoreLink extensions: `tenant_id`, `principal_id_hash`, `region`, `request_id`, `data`, `prev_hash`, `content_hash`.
   - JSON serialization (UTF-8); deterministic field ordering for hash stability.

3. **AuthEventType taxonomy completa** (vide §1):
   - 33 event types (Lote 10.3bis P0 expansion: +6 denied granular + 3 lifecycle + new_device_used) (token lifecycle + flow + tenant + membership + WebAuthn + admin + anomaly).
   - Each variant has corresponding `AuthEventData` payload struct.
   - Enum `#[non_exhaustive]` para forward compat.
   - Documented em `docs/internal/auth-event-taxonomy.md`.

4. **PII redaction macros**:
   - `redact_pat!(plaintext) -> &str` returns `"[REDACTED-PAT]"` placeholder; CI lint forces use em error paths.
   - `hash_principal_id(principal_id) -> String` returns sha256 prefix 16 hex.
   - `hash_pat_id(pat_id) -> String` similar.
   - `hash_email(email) -> String` reuse `email_hash` from WI-S03-005.
   - All hashes prefix-truncated 16 hex (64-bit; pseudonymous mas non-reversible at scale).

5. **Chain integrity field computation**:
   - `content_hash = sha256(serialized_event_without_chain_fields)`.
   - `prev_hash` provided by chain consumer (S-09); este WI sets `None` em emit; consumer fills.
   - Deterministic JSON serialization required (canonical JSON; sorted keys).

6. **Emitter trait + impl**:
   - `pub trait Emitter { async fn emit(&self, event: AuthEvent) -> Result<(), EmitError>; }`.
   - Impl `OutboxEmitter` writes to `audit_outbox` (D1 reuse from WI-S01-005).
   - Impl `DirectSiemEmitter` (forward S-09) sends webhook directly (dual fan-out for SEV-1 events).
   - `MultiplexEmitter` composes: outbox + direct para anomaly events.

7. **Per-tenant retention annotations**:
   - `AuthEvent.data.retention_hint` campo: `Solo30d` | `Team90d` | `Business1y` | `Enterprise7y` | `Custom(Duration)`.
   - Determined em emit-time from tenant.tier (lookup em Neon).
   - S-11 retention worker (forward) honors hint; este WI provê metadata.

8. **Integration emit hooks** (downstream consumers):
   - WI-S03-001 ClerkAdapter::validate → emit `auth.token.validated` OR `auth.denied.invalid`.
   - WI-S03-002 PatVerifier::verify → emit similar.
   - WI-S03-003 middleware → emit pre-handler `auth.token.validated` + post-handler `auth.{ok,denied}`.
   - WI-S03-004 RevocationDo::revoke → emit `auth.token.revoked` OR `auth.session.revoked`.
   - WI-S03-005 schema NOTIFY/LISTEN → emit `auth.tenant.provisioned` + `auth.membership.*`.
   - WI-S03-006 WebAuthnAdapter → emit `auth.webauthn.*`.

9. **Anomaly detection emit** (cross-event correlation):
   - `auth.anomaly.cross_region_burst`: same principal_id_hash logging em > 3 regions em 5 min → emit anomaly.
   - `auth.anomaly.token_replay_detected`: sign_count regression OR challenge replay → emit + SEV-1 alert.
   - These detections em downstream (S-09 chain processor); este WI provê event types.

10. **Métricas**:
    - `corelink.audit.events_emitted_total{type, tenant_tier}` (counter).
    - `corelink.audit.outbox_lag_seconds` (gauge; reuse WI-S01-005 metric).
    - `corelink.audit.redact_violations_total` (counter; alert > 0; CI gate violations).
    - `corelink.audit.chain_hash_compute_duration_us` (histogram; target p99 < 100µs).
    - `corelink.audit.event_payload_size_bytes_bucket` (histogram).

11. **Property tests** (10k iter):
    - `prop_event_serialization_deterministic`: 1000 events; serialize twice; assert byte-equal (deterministic JSON).
    - `prop_content_hash_stable`: serialize event; compute hash; modify any field; assert hash differs.
    - `prop_pii_redaction_complete`: 1000 events with synthetic PII em fields; assert serialized output contains 0 PII tokens (regex match).
    - `prop_event_type_exhaustive`: enumerate AuthEventType; assert all have AuthEventData impl + serde tag.

12. **CI gate — PII leak detection**:
    - Custom clippy lint OR cargo-style script checks for `format!("{}", principal_id)` em audit emit paths; flags as violation.
    - Grep CI gate em audit emit code paths verifies redact macro usage.

13. **rustdoc + 4 examples**:
    - `examples/emit_token_validated.rs`.
    - `examples/redaction_macros.rs`.
    - `examples/chain_hash_compute.rs`.
    - `examples/anomaly_emit.rs`.

### 6.2 Out-of-scope (deferred)

- **Full chain integrity validation** (`prev_hash` ↔ `content_hash` chain replay): WI-S09-001 (S-09 sprint).
- **SIEM connector implementation** (Splunk, Datadog, CrowdStrike): WI-S09-003.
- **Real-time anomaly detection engine** (correlation across events): WI-S09-005.
- **Audit chain export to CSV/Parquet**: WI-S11-XXX (DSR pipeline).
- **Encrypted audit chain at rest** (KMS-wrapped): pós-GA.
- **Event replay tool** (forensic): pós-GA.

## 7. Anti-Scope

- ❌ Raw PII em chain (LGPD/GDPR violation).
- ❌ Sync emit em hot path (use outbox).
- ❌ Best-effort emit (at-least-once mandatory).
- ❌ Custom event format (CloudEvents 1.0 only).
- ❌ Fire-and-forget sem persistência (outbox durability).
- ❌ Event type drift (enum exhaustive + sign-off).
- ❌ Per-event encryption (chain integrity é hash-based; payload em clear text com PII redacted).
- ❌ Cross-tenant event leak (events tagged with tenant_id; queries scoped).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Audit events EVT-047 + CloudEvents 1.0 + chain integrity

  Background:
    Given AuthEvent emitter configured (OutboxEmitter wired to audit_outbox table)
    And redact macros enforced via clippy lint
    And tenant T_1 tier = "team" (90d retention)

  Scenario: Emit auth.token.validated happy path
    Given user authenticates with PAT corelink_pat_xyz em region wnam
    When middleware (WI-S03-003) constructs TenantCtx + emits AuthEvent
    Then event payload:
      specversion = "1.0"
      id = UUID v7
      source = "corelink://wnam/auth/middleware"
      type = "auth.token.validated"
      time = now() RFC 3339
      tenant_id = T_1
      principal_id_hash = sha256(principal)[:16 hex]
      region = "wnam"
      request_id = <UUID>
      data.retention_hint = "Team90d"
      content_hash = sha256(serialized_minus_chain_fields)
      prev_hash = None  (S-09 fills downstream)
    And event INSERT em audit_outbox (atomic com TenantCtx commit)
    And metric corelink.audit.events_emitted_total{type="auth.token.validated", tenant_tier="team"} += 1

  Scenario: PII redaction enforced
    Given event with synthetic PII attempt: data.principal_id = "user@example.com"
    When CI lint runs
    Then violation detected: "raw email em audit data"
    And PR fails

  Scenario: Chain hash deterministic
    Given AuthEvent E_1 with all fields populated
    When E_1.content_hash computed twice
    Then both hashes byte-equal (deterministic JSON; canonical form)
    When ANY field modified (e.g., time +1s)
    Then content_hash differs

  Scenario: Multi-region event correlation via request_id
    Given user request creates auth.session.created em wnam at T+0
    Given subsequent admin_op em wnam at T+5s; auth.admin_op.webauthn_authenticated
    Given mass_revoke at T+6s; auth.token.revoked × 1000
    When all events emitted with same request_id
    Then audit query JOIN on request_id correlates all 1003 events
    And forensic timeline reconstructable

  Scenario: Anomaly detection emit
    Given same principal_id_hash logging em wnam, enam, weur, eeur em 5 min window
    When S-09 chain processor (forward) detects > 3 regions
    Then auth.anomaly.cross_region_burst event emitted
    And SEV-2 alert fires
    And user notified via dashboard banner

  Scenario: Per-tenant retention hint
    Given event for tenant solo-tier
    Then data.retention_hint = "Solo30d"
    Given event for tenant enterprise-tier
    Then data.retention_hint = "Enterprise7y"
    And S-11 retention worker (forward) honors hint

  Scenario: Outbox emission failure rolls back transaction
    Given D1 batch attempts: TenantCtx commit + audit_outbox INSERT
    When audit_outbox INSERT fails (transient)
    Then entire batch rolls back; TenantCtx not committed
    And handler returns 503 (not silent gap)
    And metric corelink.auth.middleware.requests_total{result="backend_unavailable"} += 1

  Scenario: Event type backward compat
    Given consumer (S-09 chain) parses AuthEventType
    When new variant added em sprint S-04 (e.g., auth.token.refresh)
    Then existing parser handles via #[non_exhaustive] catch-all OR explicit deprecation
    And no breaking change

  Scenario: Audit chain pseudonymization preserved em DSR erasure
    Given user U_1 requested erasure
    Given audit chain has 1000 events with principal_id_hash = H_U1
    When DSR worker (S-11) processes
    Then chain events retained com principal_id_hash unchanged (one-way; not reversible)
    And separate user_account.email DELETED em DB
    And LGPD Art. 18 + chain integrity both satisfied
```

## 9. Design Decisions

### 9.1 Why CloudEvents 1.0 (não custom format)

- Industry standard W3C-compatible.
- SIEM/log aggregator support natively (Datadog, Splunk).
- Mature: schema validators, connectors, transports.
- Deterministic JSON ordering for hash stability is supported.

### 9.2 Why outbox pattern (não direct emit)

Direct emit:
- Sync RPC em hot path = +20-50ms latency tax.
- Failure = gap OR retry storm.

Outbox (reuse WI-S01-005):
- Atomic com handler transaction.
- Drain worker async.
- At-least-once + dedup.

### 9.3 Why principal_id_hash sha256 prefix 16 hex (não raw UUID)

- Raw principal_id (UUID v7) é traceable cross-events; insider with chain access can correlate user behavior.
- Hash sha256 prefix 16 hex = 64-bit pseudonymous; non-reversible at scale (rainbow tables prohibitive); preserves correlation within hashed space.
- Trade-off: collision birthday-bound 2^32 (4B); CoreLink scale (M users) negligible collision probability.
- Internal forensic: privileged query via PII-access role logs reverse lookup.

### 9.4 Why 33 event types (Lote 10.3bis P0 expansion: +6 denied granular + 3 lifecycle + new_device_used) (não fewer)

Each WI emit distinct event types for:
- Compliance specificity (auditor distinguishes "validated" vs "denied" vs "expired").
- Anomaly detection tractability (e.g., spike em `auth.denied.invalid` signals attack).
- Forensic timeline clarity.

Trade-off: more types = more taxonomy maintenance; 23 is balance.

### 9.5 Why redact_pat! macro (não runtime check)

Compile-time enforcement via macro expansion:
- `redact_pat!(plaintext)` expands to `"[REDACTED-PAT]"`.
- Caller cannot accidentally `format!("{}", plaintext)` em audit data.
- CI lint detects raw PII patterns em audit emit paths.

### 9.6 Why chain_hash em este WI (não em S-09)

Both: este WI computes `content_hash` (per-event); S-09 chain processor computes `prev_hash` linkage during ingestion. Split:
- Per-event hash em emit-time = bounded compute (O(payload size)).
- Chain linkage em ingestion = central; consistent ordering.

### 9.7 Why per-tenant retention hint em event (não global policy)

Solo tier 30d retention promise must be honored. Hint em event = retention worker (S-11) reads + applies. Alternative (global policy) breaks tenant differentiation.

### 9.8 Why dual fan-out para anomaly events

`auth.anomaly.*` + `auth.webauthn.sign_count_regression` are SEV-1 signals; lag em outbox drain (60s) = unacceptable for incident response. Direct SIEM webhook em emit-time + outbox parallel = belt-and-suspenders.

Trade-off: cost ↑ (extra HTTP request); applied só em SEV-1 events (~0.01% volume).

### 9.9 Why deterministic JSON canonicalization via serde_jcs (RFC 8785; P0 fix Lote 10.3bis)

For hash stability:
- Sorted keys.
- No trailing whitespace.
- UTF-8.
- Numbers em canonical form (no leading zeros, etc.).
- **`serde_jcs` crate (RFC 8785 JCS)**: audited; deterministic key ordering; Unicode NFC normalization native; floating-point IEEE 754 canonical form. Industry-standard.
- **NOT `serde_json`** alone: `serde_json` does NOT guarantee key ordering in HashMap/BTreeMap; deterministic apenas em struct field order at compile-time.
- **NOT `canonical_json` crate**: deprecated; known issues com floating-point + Unicode normalization.
- Property test 10.5.3 deve test contra known JCS test vectors (RFC 8785 Annex B); não apenas "serialize twice byte-equal" (which só prova determinism on this codebase).

Without determinism: same event serialized differently → different hash → chain break.

### 9.10 ADR potencial?

Sim — **ADR-0033**: "Audit event taxonomy EVT-047 + CloudEvents 1.0 envelope + per-event hash chain alignment". Documenta trade-offs (CloudEvents vs custom; outbox vs sync; pseudonymization scheme; retention hint mechanism). Whitelist em validate_references.py.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** Property tests 10k iter (PR) + 100k iter (nightly) → 0 panics, deterministic serialization (EVT-002).
- [ ] **10.5.2** PII redaction CI gate: synthetic PR with raw email em audit emit → CI red (EVT-022).
- [ ] **10.5.3** Chain integrity test (P0 fix Lote 10.3bis): (a) serialize 1000 events via `serde_jcs`; compute hashes; verify deterministic + immutable; (b) round-trip RFC 8785 Annex B test vectors via `serde_jcs`; assert byte-equal expected; (c) chain link formula explicit: `sha256(prev_chain_hash || content_hash)` — never re-serialize for chain (avoid double canonicalization issue) (EVT-002).
- [ ] **10.5.4** Cross-event correlation: 100 simulated request flows; verify request_id propagation works (EVT-002).
- [ ] **10.5.5** Anomaly detection emit: synthetic burst → emit fires; SEV-2 alert (EVT-031).
- [ ] **10.5.6** Per-tenant retention hint correctness: 4 tier types; assert correct hint emit (EVT-002).
- [ ] **10.5.7** CloudEvents 1.0 schema validation: events pass `cloudevents.io` validator (EVT-002).
- [ ] **10.5.8** Cargo-audit + cargo-deny clean.
- [ ] **10.5.9** Outbox atomicity: chaos test (D1 fail) → handler 503 + no audit gap (EVT-023).
- [ ] **10.5.10** Cost regression: per-event emit cost ≤ $0.0000003 (Lote 9.4 §14.10).

## 11. DoD

- [ ] `corelink-audit` crate compila + integration tests green.
- [ ] All 33 event types (Lote 10.3bis P0 expansion: +6 denied granular + 3 lifecycle + new_device_used) defined + serde + AuthEventData payloads.
- [ ] CloudEvents 1.0 envelope validated.
- [ ] PII redaction macros em CI lint.
- [ ] Chain hash deterministic + tested.
- [ ] OutboxEmitter + DirectSiemEmitter + MultiplexEmitter impls.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em CI.
- [ ] Métricas (5 listadas §6.1.10) emitted.
- [ ] Integration hooks deployed em WI-S03-001..006 (downstream consumers).
- [ ] rustdoc + 4 examples + threat model README.
- [ ] ADR-0033 published.
- [ ] Compliance + Privacy + Crypto SME + Architect reviews.
- [ ] PRR Architect sign-off.
- [ ] Event taxonomy doc `docs/internal/auth-event-taxonomy.md` published.

## 12. Invariants Validated

- **INV-AUDIT-NO-RAW-PII** (CRITICAL): zero raw PII em chain; CI lint enforces; static check.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL): audit outbox INSERT em mesma transação que handler commit; D1 batch atomic.
- **INV-AUDIT-CHAIN-HASH-DETERMINISTIC** (HIGH): content_hash deterministic dado event payload; canonical JSON.
- **INV-AUDIT-EVENT-TYPE-EXHAUSTIVE** (HIGH): all AuthEventType variants têm AuthEventData impl; compile-time.
- **INV-AUDIT-RETENTION-HINT-ACCURATE** (HIGH): retention_hint matches tenant.tier; verified em property test.

TLA+ alignment: planned `audit_chain.tla` (S-09 forward); modela emit → outbox → drain → chain integrity.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| `corelink-audit` crate | `crates/corelink-audit/` | Rust workspace member |
| AuthEvent + types | `crates/corelink-audit/src/events.rs` | Rust |
| Redaction macros | `crates/corelink-audit/src/redact.rs` | Rust |
| Chain hash compute | `crates/corelink-audit/src/chain.rs` | Rust |
| Emitter trait + impls | `crates/corelink-audit/src/emitter.rs` | Rust |
| PII redaction CI lint | `tools/audit_pii_lint/` | Rust binary |
| Property tests | `crates/corelink-audit/tests/prop_audit.rs` | Rust |
| Integration tests | `tests/integration_audit_emit.rs` | Rust |
| Auth event taxonomy doc | `docs/internal/auth-event-taxonomy.md` | Markdown |
| ADR-0033 | `specs/02_governance/decisions/ADR-0033-audit-events-cloudevents.md` | Markdown |
| Examples (4) | `crates/corelink-audit/examples/` | Rust |

## 14. Quality Standards SOTA

- **14.5.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.5.2** rustdoc 100% public API + 4 examples + taxonomy doc.
- **14.5.3** Test coverage ≥ 95%.
- **14.5.4** Latência: emit p99 ≤ 5ms; chain_hash compute ≤ 100µs.
- **14.5.5** SAST: cargo-audit + cargo-deny + clippy + custom PII lint.
- **14.5.6** Métricas RED + outbox lag + redact violations.
- **14.5.7** Runbook: nenhum (audit é passive infra; failures handled by outbox + downstream).
- **14.5.8** Breaking changes em AuthEventType = bump major + 1yr deprecation policy.
- **14.5.9** Memory bounded: per-event ≤ 4 KiB serialized payload typical.
- **14.5.10** Cost regression gate: per-event emit ≤ $0.0000003.

## 15. Chaos Experiments

1. **Raw PII em audit emit**: synthetic PR adds `format!("{}", email)` em data field; CI lint catches; PR red.

2. **Chain hash drift**: synthetic patch breaks deterministic JSON ordering; property test detects via byte-equal mismatch.

3. **Outbox INSERT failure mid-handler**: D1 timeout durante audit emit; handler 503; verify atomic rollback.

4. **Direct SIEM webhook outage**: simulate SIEM endpoint 503; verify outbox path continues; SEV-2 alert if SIEM lag > 5min.

5. **Event volume spike**: 100k events/sec storm; verify outbox drain handles + queue backpressure bounded.

6. **Backward compat regression**: deploy older parser against new event variant; verify graceful via #[non_exhaustive].

7. **Cross-region request_id correlation**: 100 simulated user flows across regions; verify chain query JOIN works.

8. **Anomaly emit fires correctly**: synthetic 4-region burst; verify auth.anomaly.cross_region_burst emit + alert.

9. **Per-tenant retention hint correctness**: 100 events per tier (4 tiers); verify hint matches expected.

10. **Pseudonymization preservation em DSR**: synthetic erasure; verify chain events retain principal_id_hash; raw PII deleted from user_account.

## 16. PRR

PRR HIGH_RISK 13 sign-offs gated em WI-S03-008 ship gate.

- [ ] All Gherkin green.
- [ ] Property + chaos green.
- [ ] CloudEvents schema validated.
- [ ] CI PII lint enforced.
- [ ] OWASP ASVS V8 (data protection) 100%.
- [ ] LGPD Art. 18 + Art. 38 traceability.
- [ ] ADR-0033 published.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml deps | 1h |
| ST-002 | AuthEvent + CloudEvents 1.0 envelope | 2.5h |
| ST-003 | AuthEventType enum (23 variants) + payloads | 4h |
| ST-004 | Redaction macros (redact_pat!, hash_principal_id, etc.) | 2.5h |
| ST-005 | Chain hash compute (deterministic JSON canonicalization) | 3h |
| ST-006 | Emitter trait + OutboxEmitter impl | 2h |
| ST-007 | DirectSiemEmitter impl (webhook fan-out) | 2h |
| ST-008 | MultiplexEmitter (compose for SEV-1 events) | 1.5h |
| ST-009 | Per-tenant retention hint logic | 2h |
| ST-010 | Integration emit hooks em WI-S03-001..006 | 4h |
| ST-011 | Métricas (5) + trace span | 1.5h |
| ST-012 | PII redaction CI lint binary | 4h |
| ST-013 | Property tests 10k iter | 3h |
| ST-014 | Anomaly detection emit (cross-region, replay) | 2h |
| ST-015 | rustdoc + 4 examples + taxonomy doc | 4h |
| ST-016 | ADR-0033 redação | 2h |
| ST-017 | Compliance + Privacy + Crypto SME + Architect review iteration | 3h |
| ST-018 | OWASP ASVS V8 self-checklist | 1.5h |

**Total Optimistic**: ~46h. **PERT** (O=42h, M=46h, P=70h): **~50h**.

## 18. Dependencies

### Hard blockers

- WI-S01-005 (audit_outbox table) SEALED.
- WI-S03-005 (Neon schema; tenant.tier lookup) SEALED.

### Soft blockers

- WI-S03-001..006 (emit consumers) — não bloqueante para crate; integration hooks deployed iteratively.
- S-09 chain processor (forward) — consumer of emitted events; este WI não bloqueado.

### Outbound

- WI-S03-001/002/003/004/006 (downstream consumers).
- WI-S03-008 (ship gate).
- WI-S09-001 (audit chain processor; consumes events).
- WI-S11-XXX (DSR worker; honors retention_hint).

## 19. Effort PERT

O: 42h, M: 46h, P: 70h → PERT **50h**.

## 20. Time-boxing

**56h hard limit**. Se exceder → escalation: split em sub-WI (core taxonomy vs PII lint vs anomaly emit).

## 21. Observability

5 métricas listadas §6.1.10. Trace span `audit.emit` com attributes:
- `audit.event_type` (enum value)
- `audit.tenant_tier` (string)
- `audit.payload_size_bytes` (number)
- `audit.duration_us`

Logs structured JSON; INFO em normal emit; WARN em DirectSIEM webhook fail; ERROR em outbox INSERT fail.

Dashboard widget DASH-AUDIT:
- Emit rate per type.
- Outbox lag.
- Redact violations (alert > 0).
- Anomaly emit frequency.

## 22. Cost Analysis

**Per-event cost**:
- D1 batch INSERT (atomic com handler): incremental ~$0.0000001.
- Hash compute CPU: ~50µs.
- Total: ~$0.0000003.

**TCO 12m projection** (10M req/dia × 1 audit event):
- 10M × $0.0000003 = $3/dia × 365 = ~$1.1k/yr.
- Storage growth: 10M × 200 bytes = 2 GB/dia × 365 = 700 GB/yr.
- Storage cost (Neon retention 90d for team-tier average): ~50 GB × 12 × $0.000164 = ~$0.10/yr (trivial).
- Direct SIEM fan-out (anomaly events ~0.01%): 1k events × $0.0001 webhook = $0.10/dia = $36/yr.
- **Total**: ~$1.1k/yr em 10M req/dia workload.

**Cost regression gate**: per-event ≤ $0.0000005.

## 23. API Contract

Internal Rust API; consumers em WI-S03-001..006 + S-09 chain processor.

Public types stable post v1.0:
- `AuthEvent` (`#[non_exhaustive]`).
- `AuthEventType` (`#[non_exhaustive]`).
- `AuthEventData` enum (vide §1).

Breaking changes = bump major + migration plan + 1yr deprecation.

## 24. Post-mortem Hooks

- Raw PII detected em chain (LGPD violation) → CRITICAL post-mortem + breach notification.
- Audit gap detected (event missing for handler success) → SEV-1.
- Chain hash break (tampering OR ordering bug) → SEV-1.
- Outbox lag > 60s p99 sustained > 1h → SEV-2.
- Redact CI lint bypassed via merge → SEV-2 + retroactive sweep.
- Anomaly detection false positive storm → SEV-3 + threshold tuning.

## 25. Rollback / Recovery

Hot rollback via Wrangler. Audit chain integrity preserved via S-09 (forward); rollback restores prior version.

RTO ≤ 30 min; RPO 0 (events durables em D1).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: events tagged with TenantCtx authenticated source; insider direct write requires DB role (logged).
- **Tampering**: chain integrity validation S-09 forward; tamper detection via hash break.
- **Repudiation**: append-only events; non-deletable em chain (S-09 enforces).
- **Information disclosure**: PII redacted; chain encrypted at rest (forward); SIEM fan-out sanitized.
- **DoS**: emit cost-bound; rate limit S-08 forward caps storm.
- **Elevation of privilege**: insider with chain access logged via privileged role.

**LINDDUN delta**:
- **Linkability**: principal_id_hash 64-bit pseudonymous; cross-event correlation preserved within hash space.
- **Identifiability**: raw PII encrypted (WI-S03-005 pgcrypto); chain hash one-way.
- **Non-repudiation**: append-only chain (S-09); DSR pseudonymization preserves linkability sem re-identifying.
- **Detectability**: events não distinguishable em backup; analyst log access.
- **Disclosure of information**: per-event payload size bounded; no body bytes em data field.
- **Unawareness**: SLA addendum + dashboard "data we collect" page.
- **Non-compliance**: LGPD Art. 38 (registro de operações) + GDPR Art. 30 (records of processing) + SOC 2 + ISO 27001 Annex A.12.4 satisfied.

## 27. Knowledge Transfer

- **Tech talk** (1.5h): "Audit Events EVT-047 + CloudEvents 1.0 + Chain Integrity".
- **Doc** `docs/internal/auth-event-taxonomy.md` — 33 event types (Lote 10.3bis P0 expansion: +6 denied granular + 3 lifecycle + new_device_used) + AuthEventData payloads + retention.
- **Doc** `docs/internal/audit-emit-pattern.md` — outbox + dual fan-out.
- **Doc** `docs/internal/pii-redaction.md` — macro usage + CI lint enforcement.
- **ADR-0033** — design rationale.
- **Workshop** (2h): com Compliance + Privacy + Crypto SME + handler authors — taxonomy walkthrough + emit hook placement.
- **Onboarding test** (5 questions): CloudEvents fields, redact macro usage, chain hash semantics, retention hint mapping, anomaly emit triggers.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Raw PII em chain (LGPD violation) | L | M | CRITICAL | L | LOW | redact macros + CI lint + manual review |
| R-002 | Chain hash drift (non-deterministic JSON) | L | M | HIGH | L | LOW | canonical JSON + property test deterministic |
| R-003 | Audit emission gap (outbox INSERT fails silent) | L | L | CRITICAL | L | LOW | atomic D1 batch; failure rolls back handler |
| R-004 | Event type backward compat break | L | L | MEDIUM | L | LOW | #[non_exhaustive] + 1yr deprecation policy |
| R-005 | Per-tenant retention hint drift | L | M | MEDIUM | L | LOW | property test; quarterly audit |
| R-006 | DirectSIEM webhook outage causes anomaly delay | M | M | MEDIUM | M | LOW | fan-out: outbox + direct; SEV-2 alert on delay |
| R-007 | Audit chain volume cost spike | M | L | MEDIUM | L | LOW | sampling for non-CRITICAL; cold storage forward |
| R-008 | Pseudonymization collision (sha256 prefix 16 hex) | L | L | LOW | L | LOW | 64-bit prefix; collision birthday-bound impractical |
| R-009 | Event payload size bloat | M | M | LOW | M | LOW | per-event size budget 4 KiB; metric alert |
| R-010 | CI PII lint false positive | M | H | LOW | M | LOW | lint tunable; allowlist patterns |
| R-011 | Anomaly false positive storm | M | M | LOW | M | LOW | threshold tunable; alert storm cap |
| R-012 | Cost regression per-event > $0.0000005 | M | L | MEDIUM | L | LOW | §14.10 gate + monthly bench |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Compliance review event taxonomy + retention.
2. **Privacy (D+2)**: Privacy review redaction macros + chain pseudonymization + DSR alignment.
3. **Crypto (D+3)**: Crypto SME review chain hash deterministic JSON + canonical form.
4. **Code (D+6)**: peer review.
5. **Compliance (D+8)**: Compliance review SOC 2 + LGPD Art. 38 traceability.
6. **PRR (D+10)**: Architect sign-off.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 + LGPD Art. 38 + GDPR Art. 30 traceability_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — PII redaction + DSR pseudonymization + retention_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — taxonomy + envelope + emit pattern_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; chain integrity + insider threat surface_ | _pending_ | _pending_ |
| 13 | Crypto SME | _mandatory; chain hash deterministic + redact macros + canonical JSON_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S03-007 (Lote 10.3); SOTA full (CloudEvents 1.0 + 33 event types (Lote 10.3bis P0 expansion: +6 denied granular + 3 lifecycle + new_device_used) + chain hash + redact macros + per-tenant retention + 5 INVs + 10 chaos + 12-row risk + ADR-0033). |

## 32. Anti-patterns evitados

- ❌ Raw PII em chain.
- ❌ Sync emit em hot path.
- ❌ Best-effort emit (require at-least-once).
- ❌ Custom event format (CloudEvents 1.0 only).
- ❌ Fire-and-forget sem persistência.
- ❌ Event type drift sem deprecation.
- ❌ Per-event encryption (chain hash-based; PII redacted).
- ❌ Cross-tenant event leak.
- ❌ Variable JSON ordering (canonical form mandatory).
- ❌ Manual emit em handler code (use macros + middleware hooks).
- ❌ Retention hardcoded global (per-tenant tier).
- ❌ Audit chain DELETE em DSR (pseudonymization preserves).

---

**Fim WI-S03-007.** Próximo: WI-S03-008 (PRR ship gate — property test 10k revocation + pentest engagement + DSR PAT export + RB-FM-160 + 13 sign-offs).
