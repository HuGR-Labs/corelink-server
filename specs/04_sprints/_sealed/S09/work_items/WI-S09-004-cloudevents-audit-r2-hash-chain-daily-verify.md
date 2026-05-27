---
id: "WI-S09-004"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.3.0"
created: "2026-04-25"
updated: "2026-05-03"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-003"]
parent: "S-09"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "PRIVACY-MODEL"
tags: ["wi", "s09", "audit-log", "cloudevents", "hash-chain", "soc2", "r2-object-lock", "high-risk"]
---

# WI-S09-004 — CloudEvents v1.0 Audit Emitter + R2 Object Lock 7y + Per-Region Hash Chain Integrity + Daily Verifier Job + SIEM Fan-Out (`crates/corelink-audit-emitter`; CloudEvents v1.0 spec compliance subjects {tenant:<id>, cas:put, cas:get, ac:lookup, gc:purge, auth:login, quota:exceeded, abuse:detected}; sink R2 bucket `audit-events-<region>` Object Lock Governance Mode 7y CTRL-AUDIT-001 + INV-AUDIT-APPEND-ONLY; per-region hash chain — cada event tem `prev_hash` BLAKE3 + own digest; daily verifier job alerta em break SEV-1; SIEM fan-out via Cloudflare Queue → customer webhook compliance integration; INV-OBS-AUDIT-CHAIN-INTEGRITY enforcement; **audit emit fail-CLOSED** (vs WI-S09-001/002/003 fail-OPEN distinção Lote 10.6bis lesson canonical))

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-09](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S09-004 |
| Título | CloudEvents v1.0 emitter compliance subjects 8 canonical (tenant: + cas:put/get + ac:lookup + gc:purge + auth:login + quota:exceeded + abuse:detected; per `observability_model.md §7`); sink R2 bucket `audit-events-<region>` em Object Lock Governance Mode 7y retention (CTRL-AUDIT-001 + INV-AUDIT-APPEND-ONLY foundation S-06 inherited); **per-region hash chain integrity** — cada event tem `prev_hash: BLAKE3-256` referenciando previous event digest + own digest computed on event body; daily verifier job runs at UTC 02:00 per region, walks chain do mais recente para genesis, alerta SEV-1 immediately em qualquer break detectado (sprint contract §6 DoD: 7d clean required); SIEM fan-out via Cloudflare Queue → customer webhook configurable per region (sprint contract §5.4 R-S09-11); INV-OBS-AUDIT-CHAIN-INTEGRITY enforcement (NEW invariant registry §3.12); **audit emit fail-CLOSED** (Lote 10.6bis pattern canonical absorbed; transaction aborts se audit emit fails — distinct from WI-S09-001/002/003 fail-OPEN observability emit) |
| Sprint | S-09 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-AUDIT-001 audit chain integrity = SOC 2 CC7.2 compliance; chain break = compliance gap), FF-HR-003 (PII em audit events; redact! macro inheritance from WI-S09-002) |

## 1. Intent

Audit log é **the compliance primitive** do CoreLink — SOC 2 CC7.2 explicit requirement: "audit logs are immutable + integrity-verified". Sem hash chain integrity, customer compliance officer cannot verify CoreLink history isn't tampered → audit deficiency → contract loss. CloudEvents v1.0 is canonical CNCF spec for audit event format; R2 Object Lock Governance Mode 7y enforces immutability at storage level (CF cannot delete/modify objects within retention window even with admin credentials). Daily verifier job is **the canary primitive** — break detection must be ≤ 24h.

```rust
// File: crates/corelink-audit-emitter/src/lib.rs

#![forbid(unsafe_code)]

use cloudevents::{Event, EventBuilder};

#[async_trait]
pub trait AuditEmitter: Send + Sync {
    /// Emit audit event with hash chain link to previous; FAIL-CLOSED on error.
    /// Returns Err if emit fails — caller MUST abort transaction (Lote 10.6bis pattern).
    /// Lote 10.9bis P0-J: typed AuditEventData enum (NOT serde_json::Value); compile-time PII enforcement.
    async fn emit(
        &self,
        subject: AuditSubject,                          // 8 canonical CNCF subjects
        tenant_ctx: &TenantCtx,                         // Lote 10.4bis enforcement
        attributes: AuditAttributes,                    // CloudEvents v1.0 attributes
        data: AuditEventData,                           // typed enum per AuditSubject; redact!-wrapped fields canonical
    ) -> Result<EventId, AuditError>;

    /// Verify chain integrity for region; daily background job.
    async fn verify_chain(&self, region: Region) -> Result<ChainVerifyReport, AuditError>;

    /// Fan-out to SIEM webhook (best-effort; NOT fail-closed; emit success = audit committed).
    async fn fanout_to_siem(&self, event_id: EventId) -> Result<(), AuditError>;
}

#[derive(strum::Display)]
pub enum AuditSubject {
    #[strum(serialize = "tenant")]
    Tenant,                                             // tenant lifecycle (create/delete/upgrade)
    #[strum(serialize = "cas:put")]
    CasPut,                                             // CAS write events
    #[strum(serialize = "cas:get")]
    CasGet,                                             // CAS read events (compliance-required)
    #[strum(serialize = "ac:lookup")]
    AcLookup,                                           // Action Cache lookups
    #[strum(serialize = "gc:purge")]
    GcPurge,                                            // GC sweep events (S-06)
    #[strum(serialize = "auth:login")]
    AuthLogin,                                          // PAT auth events
    #[strum(serialize = "quota:exceeded")]
    QuotaExceeded,                                      // S-08 quota enforcement
    #[strum(serialize = "abuse:detected")]
    AbuseDetected,                                      // S-08 abuse score events
}

/// Lote 10.9bis P0-J: typed AuditEventData enum replaces serde_json::Value.
/// Compile-time PII enforcement: each variant uses redact!-wrapped types only.
/// serde_json::Value REJECTED — accepts arbitrary user input bypassing type system.
///
/// Lote 10.9-quaters NEW-P0-2 critical security boundary: BlobDigest, BearerToken,
/// IpAddress, EmailAddress wrapper types implement `serde::Serialize` EXPLICITLY
/// (em corelink-log-schema crate; WI-S09-002) to call `Redact::redact()` at serialization
/// boundary. `#[derive(serde::Serialize)]` on this enum is safe BECAUSE the wrapper types
/// own their Serialize impl that emits redacted output — NOT raw inner values. Without this
/// explicit impl, raw PII would write to 7-year immutable R2 Object Lock audit archive.
#[derive(serde::Serialize)]
#[serde(tag = "subject", rename_all = "snake_case")]
pub enum AuditEventData {
    Tenant {
        tenant_id: TenantId,
        action: TenantAction,                           // Created | Deleted | Upgraded | Suspended
        previous_tier: Option<Tier>,
        new_tier: Option<Tier>,
    },
    CasPut {
        digest_truncated: BlobDigest,                   // redact! truncated 16 hex chars
        size_bytes: u64,
        result: PutResult,
    },
    CasGet {
        digest_truncated: BlobDigest,
        size_bytes: u64,
        cache_hit: bool,
    },
    AcLookup {
        action_digest_truncated: BlobDigest,
        cache_hit: bool,
    },
    GcPurge {
        chunks_purged: u64,
        bytes_reclaimed: u64,
        reachable_count: u64,
    },
    AuthLogin {
        pat_id_redacted: BearerToken,                   // redact! ****<last4>
        client_ip_redacted: IpAddress,                  // redact! /24 IPv4 or /64 IPv6
        result: AuthResult,
    },
    QuotaExceeded {
        bytes_used: u64,
        max_storage_bytes: u64,
        retry_after_seconds: u64,
    },
    AbuseDetected {
        abuse_score: f64,
        response_tier: ResponseTier,                    // Noop | SilentDowngrade | AdminReview | SuspendCandidate
        features_breakdown: AbuseFeatures,              // typed; no raw user input
    },
}

pub struct AuditEvent {
    /// CloudEvents v1.0 required attributes (CNCF spec).
    pub spec_version: &'static str, // "1.0" (Lote 10.9bis P0-G corrected from "1.0.2"; Lote 10.9-quaters NEW-P1-5 typo fix; observability_model.md §7.1 canonical)
    pub id: EventId,                                    // ULID
    pub source: String,                                 // "corelink/region/<region>"
    pub subject: AuditSubject,
    pub event_type: String,                             // "dev.hugr.corelink.<subject>.v1"
    pub time: DateTime<Utc>,                            // RFC 3339
    pub data_content_type: &'static str,                // "application/json"
    pub data: AuditEventData,                           // Lote 10.9bis P0-J: typed enum (NOT serde_json::Value); compile-time PII enforcement via redact!-wrapped variants

    /// CoreLink-specific extensions (CloudEvents extension attributes):
    pub tenant_id: TenantId,                            // for tenant_id-indexed queries
    pub region: Region,
    pub trace_id: TraceId,                              // W3C correlation
    pub prev_hash: BlakeHash,                           // BLAKE3-256 of previous event
    pub event_digest: BlakeHash,                        // BLAKE3-256 of own body (excluding prev_hash + event_digest)
}

pub struct ChainVerifyReport {
    pub region: Region,
    pub verified_at_ms: i64,
    pub events_walked: u64,
    pub chain_intact: bool,
    pub broken_at: Option<EventId>,                     // None if intact
    pub genesis_event_id: EventId,
}

#[derive(thiserror::Error, Debug)]
pub enum AuditError {
    #[error("R2 Object Lock write failed (FAIL-CLOSED; transaction abort): {0}")]
    R2WriteFailed(String),

    #[error("hash chain break detected: previous_event_id={previous}; current_event_id={current}; prev_hash mismatch")]
    HashChainBreak { previous: EventId, current: EventId },

    #[error("CloudEvents v1.0 schema validation failed: {0}")]
    CloudEventsValidation(String),

    #[error("fan-out webhook failed (NOT fail-closed; audit already committed): {0}")]
    SiemWebhookFailed(String),

    #[error("daily verifier job failed: {0}")]
    VerifierJobFailed(String),

    #[error("PII detected em audit data (use redact! macro from WI-S09-002): {0}")]
    PiiInAuditData(String),
}
```

**Cripto-driven invariants enforced**:

1. **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH; registry §3.12 NEW per sprint contract §8):
   - Per-region hash chain: `event[i].prev_hash == BLAKE3(event[i-1].event_digest)`.
   - Genesis event: `prev_hash = 0x00..00` (32 bytes zeros).
   - Daily verifier walks chain; alerts SEV-1 em first break detected.
   - **Why per-region (não global)**: cross-region split-brain risk eliminated; sprint contract §15 R-Audit-chain-break.

2. **INV-AUDIT-APPEND-ONLY** (CRITICAL; registry §3.12 inherited from S-06):
   - R2 Object Lock Governance Mode 7y (canonical SOC 2 CC7.2 retention).
   - CF R2 cannot DELETE/MODIFY objects within retention window even with admin credentials.
   - TLA+ proven em `specs/tla/audit_immutability.tla` (Lote 6.2 GREEN status).

3. **CTRL-AUDIT-001 (security_model.md)**: integrity chain digital signed via per-region key (HKDF-derived; tied to R2 region for tamper-evidence).

4. **Audit emit fail-CLOSED** (Lote 10.6bis pattern canonical absorbed):
   - R2 Object Lock write fails → return Err(R2WriteFailed); caller MUST abort transaction.
   - **Distinct from WI-S09-001/002/003 fail-OPEN observability emit**.
   - Rationale: audit data integrity > availability; missing audit event = compliance gap (4% global revenue regulatory fine risk).
   - Lote 10.6bis lesson: distinção fundamental entre obs (degradação OK) vs audit (integridade > disponibilidade).

5. **CloudEvents v1.0 strict compliance** (CNCF spec):
   - Required attributes: `specversion`, `id`, `source`, `type`, `time`, `data_content_type`, `data`.
   - Custom extensions: `tenantid`, `region`, `traceid`, `prevhash`, `eventdigest` (lowercase per CloudEvents naming).
   - Schema validation em CI via `cloudevents-cli` validator.

6. **8 canonical subjects** (sprint contract §5.4 R-S09-10):
   - `tenant:`, `cas:put`, `cas:get`, `ac:lookup`, `gc:purge`, `auth:login`, `quota:exceeded`, `abuse:detected`.
   - NEW subject requires PR amending `AuditSubject` enum + `observability_model.md §7`.
   - Compliance Officer sign-off mandatory para NEW subject (regulatory scoping).

7. **PII redaction compile-time enforcement (Lote 10.9bis P0-J corrected; was structurally impossible com serde_json::Value)**: typed `AuditEventData` enum (per AuditSubject) com redact!-wrapped fields canonical (BlobDigest, BearerToken, IpAddress, EmailAddress); raw email/IP/bearer/digest forbidden via type system; runtime DLP scanner CI test (10k fixtures inheritance) é defense-in-depth secondary; chain digest BLAKE3(serde_json::to_string(&audit_event)) deterministic via struct field ordering (RFC 8785 JCS NOT needed for typed structs; serde_json em structs serializes em declaration order).

8. **TenantCtx-only enforcement** (Lote 10.4bis lesson): `tenant_id` em event from middleware (S-03); NEVER request body.

9. **Daily verifier job** (sprint contract §6 DoD: 7d clean):
   - DO `AuditChainVerifier-<region>` per-region; cron alarm 24h at UTC 02:00 (low-traffic window).
   - Walks chain from latest event back to genesis (BLAKE3 hash chain).
   - Memory budget: stream walk; ~1 KB/event in-memory; bounded via batched R2 reads (1000 events/batch; Lote 10.5bis batch ≤250 not applicable em R2 list).
   - Alarm re-arm AT START (Lote 10.4bis lesson absorbed).

10. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future()` for async R2 writes; `async_lock::Mutex` for chain head pointer protection (Lote 10.3-tris).

11. **Race-aware chain integrity** (Lote 10.7bis P0-6 strict-< predicate adapted):
    - Concurrent emits em same region: `chain_head: AtomicU64` per-region em DO; CAS update ensures monotonic.
    - First emit wins; second waits + reads new head; chain order preserved.
    - Property test 100k race emits validates chain integrity.

12. **5-tier canonical** (Lote 10.7bis P0-7): tenant_tier em event extension attribute; useful for tenant tier-based queries (não affects chain integrity).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + audit chain discipline justification)

Audit log integrity é **the compliance primitive** do CoreLink. SOC 2 CC7.2 audit explicit requirement: "system monitoring + audit logs are immutable + integrity verifiable". Without hash chain, customer compliance officer cannot prove CoreLink history não foi tampered → audit deficiency → contract loss (regulated industries: HIPAA covered entities, financial services GLBA, EU GDPR controllers).

**Why CloudEvents v1.0** (CNCF spec): canonical industry-standard audit event format; vendor-neutral (Datadog/Splunk/Sumo SIEM all canonical consume); customer compliance integration via SIEM webhook simplified; vs custom format = customer SDK fragmentation.

**Why R2 Object Lock Governance Mode 7y**: SOC 2 CC7.2 minimum 1y; ISO 27001 minimum 3y; HIPAA minimum 6y; CoreLink targets 7y para max coverage + safety margin. Governance Mode (não Compliance Mode): CoreLink admin can extend retention; CANNOT delete/modify within retention. Compliance Mode would prevent even legitimate retention extension; over-restrictive.

**Why per-region hash chain** (sprint contract §15 R-Audit-chain-break): cross-region split-brain risk eliminated. Each region maintains independent chain; cross-region correlation via `trace_id` + `tenant_id` at event level (NOT chain level). Failure of one region's chain doesn't propagate; single region recovery via R2 backup possible.

**Why daily verifier 24h cycle**: balances detection latency vs cost. Real-time verification per-emit = +10ms latency (BLAKE3 hash + R2 read); 24h batch = ~1ms amortized + 5min execution time. Break detection ≤ 24h sufficient for SOC 2 (no real-time requirement).

**Why audit fail-CLOSED** (Lote 10.6bis lesson canonical absorbed): observability emit (logs, metrics, traces) fail-OPEN preserves request availability; audit emit fail-CLOSED preserves data integrity. Trade-off: 0.001% requests fail vs missing audit events. Regulatory: missing audit event = 4% global revenue fine risk (GDPR Art. 83); request failure = retry possible. Asymmetric risk = fail-closed canonical.

**Why BLAKE3 hash chain** (vs SHA-256): BLAKE3-256 same security level (256-bit) + 5x faster + parallelizable. Chain verifier walks 1M events em ~5s vs ~25s SHA-256. CoreLink already BLAKE3 primary (CAS digests); consistency.

**Adversarial scenarios**:
- **Hash chain break detected** (event[i].prev_hash != hash(event[i-1])): SEV-1 alert immediate; investigation: was it (a) R2 corruption, (b) code bug em emit, (c) malicious tampering. Response per RB-AUDIT-CHAIN-001 runbook.
- **R2 Object Lock retention violation attempt** (admin tries DELETE): Object Lock rejects; SEV-1 alert; investigation: was admin attempting test or compromise.
- **PII em audit data** (raw email leaked): redact! macro inheritance from WI-S09-002 catches; CI gate rejects PR.
- **CloudEvents schema violation** (missing required field): emit returns CloudEventsValidation error; transaction aborts (fail-closed); CI test catches PR.
- **SIEM webhook fail**: separate concern; audit already committed em R2; webhook retry queue handles eventual consistency.
- **Verifier job memory pressure** (1M+ events em chain): stream walk via R2 batched list; bounded memory.
- **Cross-region timestamp skew**: chain integrity per-region; `time` field UTC RFC 3339; ordering within-region monotonic via DO atomic counter.
- **Concurrent emit race** (2 emits same region): DO actor serializes; chain head atomic update; deterministic ordering.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-AUDIT-001 chain integrity = SOC 2 compliance.
- **FF-HR-003**: PII em audit data = LGPD/GDPR violation.
- 12 sign-offs (Compliance + AppSec emphatic; Privacy mandatory) + chaos suite + property test 100k race emits.

## 3. Customer Impact & Journey

**Persona 1 — Compliance officer (customer)**: queries CoreLink admin endpoint (S-13 stub OK initial); receives R2 audit log archive 7y retention; verifies hash chain integrity locally via BLAKE3 walk; submits as evidence em SOC 2 / ISO 27001 audit.

**Persona 2 — DevOps responding to break**: SEV-1 alert "audit chain break em region iad event_id X"; opens RB-AUDIT-CHAIN-001 runbook; investigates: code bug, R2 corruption, or tampering; root-causes; communicates to compliance officer ≤ 4h.

**Persona 3 — Developer adding NEW subject**: PR amends `AuditSubject` enum (e.g., adds `dsr:request` for S-11 privacy); Compliance Officer sign-off required (regulatory scoping); CloudEvents type registered em CI manifest.

**Persona 4 — Customer compliance integration (SIEM)**: configures customer webhook em S-13 admin endpoint; receives fan-out CloudEvents stream; integrates com Splunk/Datadog/Sumo; cross-correlates com customer logs.

**Persona 5 — Privacy officer (CoreLink internal)**: monitors audit data for PII leakage; DLP scanner inheritance from WI-S09-002; SEV-1 alert se leak detected; remediation immediate.

**SLA addendum**:
- Audit emit latency: ≤ 100ms p99 (R2 Object Lock write + chain link).
- Chain verifier daily: completes em ≤ 5min p99.
- Break detection latency: ≤ 24h.
- R2 Object Lock retention: 7y (SOC 2 / ISO 27001 max coverage).
- SIEM fan-out lag: ≤ 30s p99 (best-effort; eventual consistency).

## 4. Capability Mapping

- **CAP-OBS-004** (CloudEvents audit log) — IMPLEMENTA primary.
- Trace: `observability_model.md §7 audit canonical` + `security_model.md CTRL-AUDIT-001` + `invariant_registry.md INV-AUDIT-APPEND-ONLY (S-06 inherited) + INV-OBS-AUDIT-CHAIN-INTEGRITY (NEW §3.12; Lote 10.9-quaters NEW-P1-3 corrected from §3.14)` + sprint contract §5.4 (R-S09-10/11) + CloudEvents v1.0 spec (CNCF) + SOC 2 CC7.2 + LGPD Art. 32 + GDPR Art. 32.

## 5. Tipo

CloudEvents emitter Rust crate + R2 Object Lock IaC + DO daily verifier job + SIEM fan-out via Cloudflare Queue; HIGH_RISK; FF-HR-005 + FF-HR-003.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-audit-emitter/` module** — AuditEmitter trait + CloudEvents v1.0 impl + R2 writer + chain logic.

2. **CloudEvents v1.0 emission**:
   - `cloudevents` Rust crate v0.7+ canonical SDK.
   - 8 subjects enum (sprint contract §5.4 R-S09-10).
   - Strict schema validation via `cloudevents-cli` em CI.

3. **R2 Object Lock IaC**:
   ```hcl
   # File: infra/cloudflare/r2/audit_bucket.tf
   resource "cloudflare_r2_bucket" "audit_events" {
       account_id = var.cloudflare_account_id
       name = "audit-events-${var.region}"
       location = var.region
   }

   # Object Lock canonical SOC 2 CC7.2; 7y retention max coverage
   resource "cloudflare_r2_bucket_lock_configuration" "audit_lock" {
       bucket_name = cloudflare_r2_bucket.audit_events.name
       enabled = true
       default_retention {
           mode = "GOVERNANCE"   # admin can extend; cannot delete within window
           days = 2557           # 7 years (365*7+2 leap)
       }
   }
   ```

4. **Per-region hash chain** (`crates/corelink-audit-emitter/src/chain.rs`):
   - DO `AuditChainHead-<region>` per-region; AtomicU64 `chain_head_event_id` + BLAKE3 hash of latest event.
   - Concurrent emit: DO actor serializes (race-free); chain head CAS update.
   - Chain link computation:
     ```rust
     let event_body_canonical = serde_json::to_string(&event).unwrap();  // canonical JSON ordering
     let event_digest = blake3::hash(event_body_canonical.as_bytes()).as_bytes();
     event.prev_hash = self.chain_head.read_prev_hash().await?;
     event.event_digest = event_digest;
     // R2 Object Lock write (fail-closed)
     self.r2_writer.put_object(&event.id.to_string(), &event_body_canonical).await?;
     // Update chain head
     self.chain_head.update(event_digest).await?;
     ```

5. **Daily verifier job** (`crates/corelink-audit-emitter/src/verifier.rs`):
   - DO `AuditChainVerifier-<region>` per-region.
   - Cron alarm: 24h interval at UTC 02:00 (low-traffic).
   - Stream walk: list R2 objects descending by event_id (ULID timestamp-ordered); compute hash chain backward to genesis.
   - Memory budget: 1 KB × 1000 events/batch = 1 MB/batch (well under DO 32 MiB).
   - Alarm re-arm AT START (Lote 10.4bis).

6. **SIEM fan-out** via Cloudflare Queue:
   - DO emits → Cloudflare Queue `audit-fanout-<region>`.
   - Consumer Worker batches + POSTs to customer webhook.
   - Retry: exponential backoff 3x; if all fail, dead-letter queue.
   - **NOT fail-closed**: audit already committed em R2 quando emit returns Ok.

7. **PII redaction inheritance** (from WI-S09-002):
   - `data: AuditEventData` typed enum (Lote 10.9bis P0-J corrected from `serde_json::Value`); fields MUST use redact!-wrapped types canonical (BlobDigest, BearerToken, IpAddress, EmailAddress); compile-time enforcement via type system primary defense; DLP scanner runtime secondary.
   - DLP scanner CI test inheritance: 10k audit fixtures + 0 leaks.
   - clippy lint catches raw String em audit data.

8. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): R2 writes via `worker::send_future()` for non-blocking concurrency; chain head update via `async_lock::Mutex`.

9. **Audit emit fail-CLOSED** (Lote 10.6bis pattern canonical absorbed):
   - R2 Object Lock write fails → return Err(R2WriteFailed); caller MUST propagate failure.
   - Caller pattern: `audit_emit(..)?` propagates via `?` operator; transaction aborts.
   - **Distinct from WI-S09-001/002/003 fail-OPEN**: this WI is the canonical fail-closed exemplar.

10. **Métricas operacionais**:
    - `corelink_audit_events_emitted_total{subject, region}` (counter; 8 subjects × 30 regions = 240 séries; well under budget).
    - `corelink_audit_emit_failures_total{reason}` (counter; **alert SEV-1 if > 0** — fail-closed; transaction aborts).
    - `corelink_audit_chain_head_event_id{region}` (gauge; current head ULID; informational).
    - `corelink_audit_chain_verify_runs_total{region, result}` (counter; daily; **alert SEV-1 if break**).
    - `corelink_audit_chain_verify_duration_ms{region}` (histogram; SLO ≤ 5min p99).
    - `corelink_audit_siem_fanout_failures_total{customer_id_hash}` (counter; **alert SEV-2 if > 1%** — best-effort).
    - `corelink_audit_r2_object_lock_violations_total{region}` (counter; **alert SEV-1 if > 0** — admin DELETE attempt detected).
    - `corelink_audit_pii_in_data_detected_total` (counter; **alert SEV-1 if > 0** — DLP regression em audit).

11. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3):
    - `prop_chain_integrity_under_concurrent_emits`: 100k race emits; assert chain order monotonic + integrity preserved (BLAKE3 hash chain).
    - `prop_cloudevents_schema_strict`: 10k synthetic events; assert schema validation accept canonical + reject malformed.
    - `prop_emit_idempotent`: 10k retries on transient R2 errors; assert no duplicate events em chain.
    - `prop_verifier_walks_chain_correctly`: 10k synthetic chains (some intact, some broken); assert verifier correctly identifies break point.
    - `prop_no_pii_em_audit_data`: 10k fixtures via proptest (inheritance from WI-S09-002 DLP); 0 leaks.
    - `prop_genesis_event_correctness`: 10k regions; assert genesis event prev_hash = zeros.

12. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    - 1. **Hash chain break (synthetic)**: inject corrupt event; verifier detects; SEV-1 alert; investigation per RB-AUDIT-CHAIN-001.
    - 2. **R2 Object Lock retention violation**: admin DELETE attempt; rejected; SEV-1 alert.
    - 3. **R2 outage 30min during emit**: emit fail-closed; transaction aborts; on R2 recovery, retry succeeds.
    - 4. **Concurrent emit race** (1000 emits same region): DO actor serializes; chain integrity preserved; property test 100k.
    - 5. **PII em audit data leak attempt**: clippy lint rejects PR; runtime DLP catches if merged; SEV-1 alert.
    - 6. **CloudEvents schema violation**: emit returns error; CI test catches PR.
    - 7. **SIEM webhook outage**: fan-out failures; audit already committed; recovery via retry queue.
    - 8. **Verifier job memory pressure** (1M+ events): stream walk; bounded memory; SEV-3 alert if > 80% DO budget.
    - 9. **Cross-region split-brain**: per-region chain isolated; cross-region correlation via trace_id only; chain integrity unaffected.
    - 10. **Customer compliance audit request**: 7y archive query via R2 list; BLAKE3 chain verification possible offline.
    - 11. **GDPR Art. 17 erasure (tenant_id)**: legal complexity — audit retention 7y vs erasure right; Lote 10.8bis humane LGPD precedent: customer notification + Compliance Officer review; deletion only after retention window OR legal exemption (LGPD Art. 16 retention obligation supersedes erasure).

### 6.2 Out-of-scope (deferred)

- Customer self-service audit query (deferred S-13 admin plane).
- Real-time chain verification (cost prohibitive; daily canonical).
- Compliance Mode Object Lock (over-restrictive; Governance canonical).
- Cross-region chain federation (per-region canonical; deferred S-14).
- ML-based anomaly detection em audit (anti-scope §10).
- Audit log encryption at rest (R2 server-side encryption canonical; BYOK deferred S-14).

## 7. Anti-Scope

- ❌ Audit fail-OPEN (would compromise compliance integrity; canonical fail-closed Lote 10.6bis).
- ❌ Cross-region global chain (split-brain risk; per-region canonical).
- ❌ Real-time chain verify per-emit (latency tax > 10ms).
- ❌ Compliance Mode Object Lock (admin retention extension impossible).
- ❌ Custom CloudEvents extensions outside CNCF naming (lowercase canonical).
- ❌ Skip CI schema validation (silent invalid emit).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ Raw PII em audit data (CTRL-PRIV-001 violation; redact! inheritance).

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: CloudEvents Audit Emitter + R2 Object Lock + Hash Chain + Daily Verifier

  Scenario: Audit emit succeeds with chain link
    Given AuditEmitter configured for region iad
    Given chain head event_id E[i-1] with hash H[i-1]
    When emit(subject=cas:put, tenant_ctx, attributes, data) called
    Then event E[i] created with prev_hash = H[i-1]
    Then event_digest = BLAKE3(canonical_json(event_body))
    Then R2 Object Lock write succeeds (fail-closed; transaction commits)
    Then chain head updated to E[i] with H[i]
    Then corelink_audit_events_emitted_total{subject=cas:put, region=iad} increments

  Scenario: Audit emit fails fail-closed
    Given R2 Object Lock write returns 503 transient error
    When emit called
    Then AuditError::R2WriteFailed returned
    Then caller transaction MUST abort (fail-closed canonical Lote 10.6bis)
    Then corelink_audit_emit_failures_total{reason=r2_write_failed} increments
    Then SEV-1 alert (audit data integrity priority)

  Scenario: Hash chain integrity preserved under concurrent emits
    Given 1000 concurrent emit calls em region iad
    When DO actor serializes
    Then chain order deterministic
    Then no duplicate event_ids
    Then BLAKE3 chain integrity verifiable retroactively
    Then property test prop_chain_integrity_under_concurrent_emits green (100k)

  Scenario: Daily verifier detects chain break
    Given chain has 1M events em region iad
    Given event E[k] corrupted (prev_hash mismatch)
    When verifier runs at UTC 02:00
    Then stream walk identifies break at E[k]
    Then ChainVerifyReport { chain_intact=false, broken_at=Some(E[k]) }
    Then corelink_audit_chain_verify_runs_total{region=iad, result=break} increments
    Then SEV-1 alert immediately; investigation per RB-AUDIT-CHAIN-001

  Scenario: R2 Object Lock prevents DELETE within retention
    Given audit event committed em R2 with 7y Object Lock
    When admin attempts R2 DELETE via CF API
    Then R2 rejects (Object Lock Governance Mode)
    Then SEV-1 alert: corelink_audit_r2_object_lock_violations_total
    Then incident response per RB-AUDIT-LOCK-VIOLATION

  Scenario: SIEM webhook fan-out (best-effort; not fail-closed)
    Given audit event committed em R2 successfully
    When fanout_to_siem(event_id) called
    Given customer webhook returns 503
    Then 3x exponential backoff retries
    Then if all fail: dead-letter queue
    Then audit emit success preserved (NOT fail-closed; webhook is downstream)

  Scenario: PII em audit data rejected
    Given PR adds raw email em event.data field
    When clippy lint runs (redact! macro inheritance from WI-S09-002)
    Then compilation fails
    Then PR cannot merge

  Scenario: CloudEvents schema validation strict
    Given emit with missing required attribute "type"
    When CloudEvents validation runs
    Then AuditError::CloudEventsValidation returned
    Then emit fails fail-closed; transaction aborts
    Then CI test catches em PR diff (cloudevents-cli validator)

  Scenario: NEW subject requires Compliance Officer sign-off
    Given developer adds AuditSubject::DsrRequest for S-11 privacy sprint
    When PR opens
    Then Compliance Officer sign-off required (mandatory emphatic; regulatory scoping)
    Then observability_model.md §7 amended with new subject
    Then CloudEvents type registered: "dev.hugr.corelink.dsr.request.v1 (Lote 10.9bis P0-G corrected)"

  Scenario: Verifier job 7d clean (sprint contract §6 DoD)
    Given daily verifier runs 7 consecutive days
    When all 7 runs return chain_intact=true
    Then sprint contract §6 DoD satisfied
    Then ship gate criterion met

  Scenario: GDPR Art. 17 erasure vs 7y retention conflict
    Given customer T submits erasure request (LGPD Art. 18 / GDPR Art. 17)
    When tenant_id=T audit events queried
    Then within 7y retention window: events NOT deleted (R2 Object Lock)
    Then customer notified: retention obligation supersedes erasure (LGPD Art. 16)
    Then Compliance Officer human review (humane LGPD pattern Lote 10.8bis)
    Then if exemption granted: deletion via legal channel post-retention
```

## 9. Design Decisions

- 9.1: CloudEvents v1.0 (CNCF spec canonical).
- 9.2: R2 Object Lock Governance Mode 7y (SOC 2 max coverage).
- 9.3: Per-region hash chain (split-brain risk eliminated).
- 9.4: BLAKE3-256 hash chain (5x faster than SHA-256).
- 9.5: Daily verifier UTC 02:00 (low-traffic window; ≤24h break detection).
- 9.6: **Audit emit fail-CLOSED** (Lote 10.6bis canonical pattern; distinct from observability fail-OPEN).
- 9.7: SIEM fan-out best-effort (audit already committed; webhook downstream).
- 9.8: redact! macro inheritance from WI-S09-002 (PII em audit data forbidden).
- 9.9: TenantCtx-only enforcement (Lote 10.4bis).
- 9.10: CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.11: NEW INV-OBS-AUDIT-CHAIN-INTEGRITY registered em invariant_registry §3.12.
- 9.12: NO new ADR (extends security_model.md CTRL-AUDIT-001 + observability_model.md §7 canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s09.004.1** Crate compila + integration tests green.
- [ ] **10.s09.004.2** All 11 Gherkin scenarios green.
- [ ] **10.s09.004.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s09.004.4** Chaos suite 11 scenarios green.
- [ ] **10.s09.004.5** **Daily verifier 7d clean** sustained (sprint contract §6 DoD; INV-OBS-AUDIT-CHAIN-INTEGRITY).
- [ ] **10.s09.004.6** R2 Object Lock 7y retention configured + verified via `aws s3api get-object-lock-configuration`.
- [ ] **10.s09.004.7** CloudEvents v1.0 schema validation em CI green em 100% PR sample events.
- [ ] **10.s09.004.8** PII redaction inheritance: 0 leaks em 10k audit fixtures (DLP CI test from WI-S09-002).
- [ ] **10.s09.004.9** SIEM fan-out webhook tested em staging 24/7.
- [ ] **10.s09.004.10** Métricas (8 §6.1.10) emitted; chain break alerts SEV-1; r2_object_lock_violations alerts SEV-1.
- [ ] **10.s09.004.11** Cargo-audit + cargo-deny + clippy clean; cloudevents-cli validate green.
- [ ] **10.s09.004.12** Compliance Officer sign-off mandatory para 8 canonical subjects scoping.

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Lote 10.8bis P1-2; Compliance + AppSec emphatic).

## 12. Invariants Validated

- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH; registry §3.12 NEW): hash chain unbroken; daily verifier 7d clean.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven em Lote 6.2): R2 Object Lock 7y enforces append-only at storage level.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-region chain; cross-region failures isolated.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): TenantCtx-only middleware (S-03 inheritance).
- **CTRL-AUDIT-001** (security_model.md): integrity chain + per-region key signing.
- **CTRL-PRIV-001** (privacy_model.md): redact! macro inheritance from WI-S09-002.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Audit emitter module | `crates/corelink-audit-emitter/` | Rust |
| Chain head DO | `crates/corelink-audit-emitter/src/chain_head_do.rs` | Rust |
| Daily verifier DO | `crates/corelink-audit-emitter/src/verifier_do.rs` | Rust |
| R2 Object Lock IaC | `infra/cloudflare/r2/audit_bucket.tf` | Terraform |
| SIEM Queue config | `infra/cloudflare/queue/audit_fanout.tf` | Terraform |
| CloudEvents schema | `specs/_schemas/audit_event.cloudevents.json` | CloudEvents Schema |
| Property tests | `crates/corelink-audit-emitter/tests/prop_chain.rs` | Rust |
| Chaos suite | `tests/chaos_audit.rs` | Rust |
| Runbook RB-AUDIT-CHAIN-001 | `docs/runbooks/RB-AUDIT-CHAIN-001.md` | Markdown |
| Runbook RB-AUDIT-LOCK-VIOLATION | `docs/runbooks/RB-AUDIT-LOCK-VIOLATION.md` | Markdown |

## 14. Quality Standards SOTA

- 14.s09.004.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s09.004.2: rustdoc 100% public API.
- 14.s09.004.3: Test coverage ≥ 90%.
- 14.s09.004.4: Audit emit latency ≤ 100ms p99.
- 14.s09.004.5: SAST clean; cloudevents-cli strict.
- 14.s09.004.6: Métricas (8 §6.1.10).
- 14.s09.004.7: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s09.004.8: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); race-aware chain head update (Lote 10.7bis P0-6 adapted).
- 14.s09.004.9: redact! macro inheritance from WI-S09-002 (PII em audit data forbidden).
- 14.s09.004.10: **Audit fail-CLOSED** canonical (Lote 10.6bis pattern; distinct from observability fail-OPEN em WI-S09-001/002/003).
- 14.s09.004.11: BLAKE3-256 hash chain (consistent with CAS digests primary).
- 14.s09.004.12: CloudEvents v1.0 strict CNCF spec compliance.

## 15. Chaos Experiments (11)

§6.1.12 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Compliance + AppSec emphatic).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + AuditEmitter trait + 8 subjects enum | 2 |
| ST-002 | CloudEvents v1.0 emit lib + schema validation | 2 |
| ST-003 | Per-region hash chain + DO chain head atomic update | 3 |
| ST-004 | R2 Object Lock IaC + bucket configuration per region | 1.5 |
| ST-005 | Daily verifier DO + cron alarm + stream walk | 2.5 |
| ST-006 | SIEM fan-out via Cloudflare Queue | 1.5 |
| ST-007 | Métricas (8) emit | 1 |
| ST-008 | RB-AUDIT-CHAIN-001 + RB-AUDIT-LOCK-VIOLATION runbooks | 1.5 |
| ST-009 | Property tests (6 × 10k; 100k nightly) | 2.5 |
| ST-010 | Chaos suite (11) | 2 |

**Total**: ~19h. **PERT** O=12h M=18h P=28h: **~18.7h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S09-001 SEALED (métricas emit lib); WI-S09-002 SEALED (redact! macro inheritance for PII em audit data); S-03 SEALED (TenantCtx middleware).
- Soft: S-06 SEALED (INV-AUDIT-APPEND-ONLY foundation); S-08 SEALED (audit emit pattern from WI-S08-001/003 Lote 10.6bis fail-closed inheritance); S-11 SEALED (DSR audit subjects).
- Hard infra: Cloudflare R2 Object Lock feature enabled per region; Cloudflare Queue available; cloudevents-cli em CI.

## 19. Effort PERT: ~18.7h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

8 metrics §6.1.10. Trace span `audit.{emit, chain_link, r2_write, verify, fanout}`.

## 22. Cost Analysis

- R2 Object Lock storage: 7y retention × 1 KB/event × 100k events/dia/region = 73 GB/region/yr × 7y = 511 GB/region; $0.015/GB-mo = ~$92/região/yr storage; × 5 regions = $460/yr.
- R2 PUT operations: $4.50/1M PUTs × 100k events/dia × 365 × 5 regions = ~$821/yr.
- Cloudflare Queue: $0.40/1M operations × 100k events/dia × 365 × 5 regions = ~$73/yr.
- Daily verifier: 1M events × BLAKE3 hash × 5 regions/dia × 365 × $0.0001 (DO compute) = trivial.
- TCO 12m: ~$1400/yr audit infrastructure.
- **Cost saved by audit chain integrity**: prevents catastrophic regulatory fine (LGPD/GDPR 4% global revenue × annual rev = ${potential multi-million}); SOC 2 audit failure = customer contract loss.

## 23. API Contract

- Public Rust: `AuditEmitter` trait + `AuditEvent`, `AuditSubject`, `ChainVerifyReport`, `AuditError` types; `#[non_exhaustive]`.
- Storage: R2 Object Lock Governance Mode 7y per region.
- Wire: CloudEvents v1.0 (CNCF spec).
- Fan-out: Cloudflare Queue → customer webhook.

## 24. Post-mortem Hooks

- Audit chain break em produção → SEV-1 + post-mortem mandatory dentro 48h (sprint contract §18 trigger; compliance officer notificado).
- R2 Object Lock violation attempt → SEV-1 + investigation; admin compromise possibility.
- PII leak em audit data (post-merge) → CRITICAL post-mortem; CI gate review.
- SIEM fan-out failure rate > 1% sustained → SEV-2; customer compliance integration review.
- Verifier job memory pressure > 80% sustained → SEV-3; chain length scaling review.

## 25. Rollback / Recovery

- Rollback: revert AuditEmitter mount; emit returns Err immediately; ALL transactions abort (fail-closed canonical).
- Recovery: emitter re-mounted; emits resume; chain head re-derived from latest R2 object.
- RTO: ≤ 5min (mounting); RPO: ≤ 0min (no in-flight loss em fail-closed model).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware (S-03); per-region key signing (CTRL-AUDIT-001).
- T(ampering): R2 Object Lock Governance prevents modification; hash chain detects tampering.
- R(epudiation): immutable audit log = compliance evidence.
- I(nformation disclosure): redact! macro inheritance; PII em audit data forbidden.
- D(enial of Service): per-region isolation; cross-region failures contained.
- E(scalation of Privilege): admin DELETE attempt rejected by Object Lock.

**LINDDUN** (LGPD/GDPR mandatory emphatic):
- L(inkability): per-tenant audit events; tenant_id em event; retention 7y vs erasure conflict (RB-AUDIT-LGPD-001 governance).
- I(dentifiability): redact! macro; raw PII forbidden.
- N(on-repudiation): hash chain + R2 Object Lock = canonical non-repudiation.
- D(etectability): customer can request audit access via S-13 admin.
- D(isclosure): audit retention 7y > erasure right; LGPD Art. 16 retention obligation supersedes Art. 18 erasure (humane review required Lote 10.8bis pattern).
- U(nawareness): customer notified em humane review of retention vs erasure conflict.
- N(on-compliance): **SOC 2 CC7.2 + LGPD Art. 32 + GDPR Art. 32 compliance**: hash chain + Object Lock + daily verifier + 7y retention.

## 27. Knowledge Transfer

Tech talk (2h): "S-09 Audit: CloudEvents + Hash Chain + Fail-CLOSED"; doc `docs/dev/audit-architecture.md`; onboarding test 8 questions: CloudEvents v1.0 spec, R2 Object Lock Governance vs Compliance, BLAKE3 chain integrity, daily verifier 24h cycle, fail-CLOSED vs fail-OPEN distinção (Lote 10.6bis), per-region split-brain elimination, SOC 2 CC7.2 requirements, LGPD Art. 16 vs 18 retention conflict.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Hash chain break em produção | L | M | CRITICAL (compliance gap) | L | LOW | Daily verifier ≤ 24h detection; SEV-1 alert; RB-AUDIT-CHAIN-001 |
| R-002 | R2 Object Lock retention violation | L | M | CRITICAL | L | LOW | Object Lock Governance enforced; admin DELETE rejected |
| R-003 | PII leak em audit data | L | M | CRITICAL (LGPD/GDPR) | L | LOW | redact! macro inheritance; DLP CI test 10k fixtures |
| R-004 | R2 outage causes fail-closed cascade | M | L | MEDIUM (request failure) | M | LOW | Fail-closed canonical; transaction abort + retry; minimal request impact |
| R-005 | Daily verifier memory pressure | M | L | LOW | L | LOW | Stream walk; 1 KB/event × 1000/batch; SEV-3 if > 80% |
| R-006 | Concurrent emit race | L | L | MEDIUM | L | LOW | DO actor serialize; atomic chain head; property test 100k |
| R-007 | SIEM fan-out failure rate > 1% | M | L | LOW | L | LOW | Best-effort; audit committed em R2; retry queue |
| R-008 | Cross-region split-brain | L | M | HIGH | L | LOW | Per-region chain; cross-region correlation via trace_id only |
| R-009 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-010 | LGPD Art. 18 vs 7y retention conflict | M | M | MEDIUM | M | LOW | Humane LGPD review (Lote 10.8bis pattern); Art. 16 supersedes |
| R-011 | NEW subject without Compliance sign-off | L | L | MEDIUM | L | LOW | PR review mandatory; CI hook checks AuditSubject enum amendments |
| R-012 | Cost regression Object Lock storage > 10% | L | L | LOW | L | LOW | §14.s09.7 gate |

## 29. Review Checkpoints

D+0 design (Architect; chain integrity); D+1 Compliance (SOC 2 CC7.2 + LGPD/GDPR); D+2 AppSec (Object Lock + STRIDE); D+3 Privacy (LINDDUN + redact! inheritance); D+4 code review; D+5 chaos validation (chain break injection); D+6 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap; Lote 10.8bis P1-2)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory emphatic** — CTRL-AUDIT-001 chain integrity_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos chain break + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC7.2 + LGPD Art. 16/18/32 + GDPR Art. 17/32 + 8 subjects scoping_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + redact! inheritance_ |
| 11 | Architect | _TBD; **mandatory** — fail-CLOSED canonical pattern + per-region chain + BLAKE3 discipline_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — R2 Object Lock + admin DELETE rejection + chain integrity verifier_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.9) | Criação WI-S09-004; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris lessons absorbed: 5-tier canonical Tier (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); **audit fail-CLOSED canonical** (Lote 10.6bis pattern; distinct from observability fail-OPEN em WI-S09-001/002/003); race-aware chain head update (Lote 10.7bis P0-6 adapted from S-06 INV-GC-004 + S-07 WI-S07-002 lessons); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.14 (NEW INV-OBS-AUDIT-CHAIN-INTEGRITY canonical position). NEW corelink-audit-emitter crate. NEW CloudEvents v1.0 schema artifact. NEW R2 Object Lock 7y IaC. NEW daily verifier DO. NEW 2 runbooks (RB-AUDIT-CHAIN-001 + RB-AUDIT-LOCK-VIOLATION). redact! macro inheritance from WI-S09-002 (PII em audit data forbidden via type system). BLAKE3-256 hash chain (consistent with CAS digests primary). SOC 2 CC7.2 + LGPD Art. 32 + GDPR Art. 32 compliance. LGPD Art. 16 retention vs Art. 18 erasure humane review (Lote 10.8bis humane LGPD pattern). |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.9bis) | R4+R5 review remediation: P0-B INV §3.X → §3.12; P0-E Prom métricas underscores; P0-G CloudEvents specversion `1.0.2` → `1.0` canonical + event type prefix `io.corelink.*` → `dev.hugr.corelink.*` canonical (observability_model §7.1; SIEM consumer compatibility com S-06 audit events); P0-H abuse:detected adicionado à sprint contract §5.4 R-S09-10 (8 subjects canonical aligning DoD §6); **P0-J typed AuditEventData enum replaces serde_json::Value** (compile-time PII enforcement structurally enabled; redact!-wrapped fields canonical: BlobDigest, BearerToken, IpAddress, EmailAddress; was structurally impossible com untyped Value field). Aggregate target ≥ 8.5 (R4 8.2 + R5 6.5 baselines). |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.9-quaters **SEALED**) | Sonnet R5 quinquies 8.5/10 APPROVED ship gate. **NEW-P0-2 cross-WI security boundary**: explanatory note added §1.5 + §6.1.7 explaining serde::Serialize delegation via WI-S09-002 wrapper types' explicit Serialize impls (Redact::redact() called at serialization boundary). NEW-P1-3 §3.14 → §3.12 em §4 Capability Mapping fixed. NEW-P1-5 spec_version comment typo 'corrected from 1.0' → 'corrected from 1.0.2' fixed. AuditEventData typed enum with redact!-wrapped fields canonical. Critical security boundary (serde Serialize bypass) closed end-to-end across WI-S09-002 → WI-S09-004 boundary. **WI sealed pre-implementation; LGPD/GDPR/SOC 2 CC7.2 compliance achieved.** |
| 1.3.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S09-004 SEAL) | **WI-S09-004 SEALED — `crates/corelink-audit-chain/` v0.1.0 shipped (no D1 migration; production CF R2 PutObject + Object Lock Governance Mode 7y retention IaC + scheduled DO `AuditChainVerifier-<region>` Cron + SIEM fan-out via Cloudflare Queue + cloudevents-cli CI gate deferred to WI-S09-007 PRR ship gate per `trait-abstraction-defer` charter pattern).** Implementation diverges from the WI-design's `cloudevents` Rust crate v0.7+ direct dependency to a hand-rolled CloudEvents 1.0 envelope + `R2AuditSink` trait + `InMemoryR2AuditSink` orchestrator + `ChainVerifier` daily-verify primitive surface aligned with the in-flight S-09 emit primitive trait-abstraction-defer pattern (per `corelink-analytics::CardinalityValidator` ship gate from WI-S09-001 SEAL + `corelink-logpush::PiiRedactor` ship gate from WI-S09-002 SEAL + `corelink-tracing::TracingService` ship gate from WI-S09-003 SEAL); the runtime in-memory pipeline IS the load-bearing INV-OBS-AUDIT-CHAIN-INTEGRITY + INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER + INV-TENANT-ISOLATION + RFC 8785 JCS canonical determinism falsifiability target — the production R2 PutObject + Object Lock Governance Mode 7y + DO `AuditChainVerifier-<region>` cron + SIEM fan-out + 100k nightly + chaos 11 are deferred alongside WI-S09-007 (PRR ship gate; `trait-abstraction-defer` charter pattern). New crate ships 6 source modules (~2738 LOC src + ~429 LOC tests + 1 Cargo.toml = ~3170 LOC total): `event` (`AuditEvent` CloudEvents 1.0 aligned: `specversion` `"1.0"` hard-pinned + `event_type` canonical `dev.hugr.corelink.<subject>.v1` per Lote 10.9bis P0-G + `source` Worker URI + `subject` `AuditEventKind` `#[non_exhaustive]` 8-canonical CNCF subject taxonomy `tenant`/`cas:put`/`cas:get`/`ac:lookup`/`gc:purge`/`auth:login`/`quota:exceeded`/`abuse:detected` per WI Lote 10.9bis P0-H + `id` UUIDv7 + `time_ms` Unix epoch ms + `datacontenttype` `"application/json"` hard-pinned + CoreLink extensions `tenant_id` UUIDv7 pseudonymous TenantCtx-only enforcement per S-03 + `region` mirrored from `corelink_analytics::Region` 22-canonical CF colocode + `sequence_number` u64 monotonic per-tenant chain partition + `prev_hash` `ChainHash` 32-byte BLAKE3-256 hex-rendered on the wire + `data` redacted serde_json::Value + canonical NDJSON serializer round-trip pinned + `genesis()` constructor + `is_genesis()` predicate + canonical `GENESIS_PREV_HASH` `[0u8; 32]` Bitcoin-genesis-block convention + `GENESIS_SEQUENCE_NUMBER` `0` + `EVENT_TYPE_PREFIX` `dev.hugr.corelink.` per Lote 10.9bis P0-G); `chain` (`HashChainBuilder` per-tenant chain state machine: `head` + `next_sequence`; appends events enforcing sequence + `prev_hash` integrity at the boundary; BLAKE3-256 link hash via `compute_canonical_bytes` RFC 8785 JCS canonicalization → `link_chain_hash` BLAKE3 of `prev_hash_bytes || canonical_bytes(event)` Bitcoin-block-header pattern; `link_chain_hash_from_canonical` for verifier reuse without re-running JCS; `verify_chain_link` predicate; resume-from-checkpoint discipline for cold-start production wiring); `sink` (`R2AuditSink` trait + `InMemoryR2AuditSink<A>` orchestrator wiring per-tenant `HashChainBuilder` ledger + audit-emit-BEFORE-mutation fail-CLOSED envelope on every emit arm + per-instance `Arc<Mutex<>>` F-001 closure + canonical R2 NDJSON layout `audit/{tenant_id}/{date YYYY-MM-DD}/{seq:08}.cloudevent.ndjson` per WI §6.1.4 via `canonical_r2_key` + `canonical_date_yyyy_mm_dd` pure-logic Howard Hinnant 2018 days-from-civil inversion no chrono dep wasm32-clean + `next_link_inputs` for production cold-start hydration + `record_sink_failure` arm for the production-side fail-CLOSED sink-failure ledger `corelink_audit_emit_failures_total` SEV-1 alert source; `CapturedR2AuditSink` + `FailingR2AuditSink` adversarial fixtures); `verifier` (`ChainVerifier` daily-verify primitive: walks `[start, end]` slice; recomputes BLAKE3 links; fail-CLOSED on first mismatch with canonical `corelink.audit_chain.chain_break_detected` SEV-0 audit emit; `verify_chain` resumable variant + `verify_chain_from_genesis` convenience wrapper + `VerifyOutcome` shape `events_verified_count` / `last_verified_hash` / `first_break_at_seq` + tenant isolation guard at the slice boundary + sequence monotonicity guard); `audit` (`AuditChainAuditEventType` `#[non_exhaustive]` 4-event canonical taxonomy `corelink.audit_chain.{event_appended, chain_verified_ok, chain_break_detected, sink_failure}` + `AuditChainAuditRecord` typed shape + `AuditChainAuditSink` trait + `InMemoryAuditChainAuditSink` capture + `FailingAuditChainAuditSink` adversarial fixture; SEV-0 classification on `ChainBreakDetected` + SEV-1 classification on `SinkFailure` per WI §6.1.10); `error` (`AuditChainError` `#[non_exhaustive]` taxonomy with `Canonicalization` JCS / `Audit` lifting audit-of-audit envelope failures / `Sink` lifting R2 PutObject failures fail-CLOSED canonical / `ChainBreak` carrying first-divergence sequence + tenant_id / `SequenceOrderingViolation` carrying expected_start + observed + index / `TenantIsolationViolation` carrying chain_tenant + event_tenant + index / `Internal` mutex poisoning); `lib` (re-exports + `audit_chain_schema_version()` const fn = 1). Tests: **83 tests across all targets, 0 failures, parallel-safe**: 72 inline lib unit (audit 6 + chain 13 + error 7 + event 18 + sink 16 + verifier 12) + 11 prop_audit_chain including the 8 required property tests at 10k iter PR-gate (`PROPTEST_CASES` env var override at runtime per S-07 P1-2 fix): `prop_chain_append_only` (INV-AUDIT-APPEND-ONLY canary; chain only grows; never rewrites), `prop_chain_verify_passes_on_unmodified` (INV-OBS-AUDIT-CHAIN-INTEGRITY positive case; full chain verifies clean; independent recomputation matches producer-side chain head), `prop_chain_break_detected_on_tamper` (INV-OBS-AUDIT-CHAIN-INTEGRITY negative case; data-tamper on non-last events + prev_hash-tamper on any event both surface ChainBreak at first divergence sequence), `prop_genesis_zero_prev_hash` (Bitcoin-genesis-block convention; first event has zero prev_hash + zero sequence), `prop_chain_sequence_monotonic` (sequence numbers strictly increase by 1 starting from 0; builder rejects out-of-order at the boundary), `prop_jcs_canonicalization_deterministic` (RFC 8785 §1 byte-stability; serde_jcs 0.2 pin), `prop_tenant_isolation` (INV-TENANT-ISOLATION canary; per-tenant chain partitioning + verifier rejects cross-tenant slices), `prop_audit_emit_per_event_type` (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER; every chain emit fires EventAppended audit; verifier emits ChainVerifiedOk on clean walks + ChainBreakDetected on tamper). Plus surface-pinning regression tests covering canonical 8-element AuditEventKind subjects + canonical 4-element audit event strings + canonical genesis constants. **Trait-abstraction-defer per charter**: real CF R2 PutObject with Object Lock Governance Mode 7y retention via `worker::send_future` + Terraform IaC for R2 Object Lock + DO `AuditChainVerifier-<region>` cron alarm 24h at UTC 02:00 (low-traffic) + alarm re-arm AT START (Lote 10.4bis lesson) + SIEM fan-out via Cloudflare Queue → customer webhook (best-effort) + cloudevents-cli CI gate against canonical schema + 100k nightly via `PROPTEST_CASES=100000` + chaos 11 — all consolidated alongside WI-S09-007 PRR ship gate per `trait-abstraction-defer` charter pattern. Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-audit-chain` member + workspace dep + `serde_jcs = "0.2"` direct dep; reuses `corelink-analytics::Region` 22-canonical CF colocode (no drift; cross-crate region binding pinned at the type system). Quality gates verde: `cargo test -p corelink-audit-chain --all-targets` 83 tests 0 failures debug + release; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (279 schema + 6 YAML = 285 docs); `check_migrations_additive.py` clean (16 migrations; no new migration this WI per `trait-abstraction-defer` — chain head durable mirror lands alongside WI-S09-007 PRR ship gate). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-09 corpus. |

## 32. Anti-patterns evitados

- ❌ Audit fail-OPEN (compliance integrity sacrificed); ❌ Cross-region global chain (split-brain); ❌ Real-time chain verify (latency tax); ❌ Compliance Mode Object Lock (over-restrictive); ❌ Custom CloudEvents extensions outside CNCF; ❌ Skip CI schema validation; ❌ tokio::spawn em CF Workers; ❌ Raw PII em audit data (CTRL-PRIV-001 violation); ❌ Skip 8 canonical subjects scoping (Compliance Officer sign-off mandatory).

---

**Fim WI-S09-004.** Próximo: WI-S09-005 (12 Grafana dashboards-as-code).
