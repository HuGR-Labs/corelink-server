---
id: "WI-S10-001"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.4.0"
created: "2026-04-26"
updated: "2026-05-03"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-009"]
parent: "S-10"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s10", "usage-events", "billing", "r2-object-lock", "idempotency", "cloudevents", "high-risk"]
---

# WI-S10-001 — Usage Event Emitter no CAS Hot Paths + R2 Append-Only Object Lock 7y + Idempotency Staging Table (`crates/corelink-billing-events`; CloudEvents v1.0 emitter integrated em CAS PUT/GET + AC lookup + GC purge hot paths; sink R2 bucket `billing-events-<region>` Object Lock Governance Mode 7y per CTRL-BILLING-001 + INV-AUDIT-APPEND-ONLY S-09 inheritance; idempotency via `(tenant_id, request_id)` UNIQUE em D1 staging table dedup; INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP foundation; emit fail-OPEN at hot path com staging table buffer + retry queue — distinct from WI-S09-004 audit fail-CLOSED — billing pipeline tolerates hot-path emit lag via idempotent re-emission)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-10](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S10-001 |
| Título | Usage event emitter no CAS hot paths CloudEvents v1.0 (sprint contract §5.1 R-S10-1; canonical observability_model §7 alignment): subjects `tenant:<uuid>`; types `dev.hugr.corelink.cas.put.v1` + `dev.hugr.corelink.cas.get.v1` + `dev.hugr.corelink.ac.lookup.v1` + `dev.hugr.corelink.gc.purge.v1` (4 canonical types per Lote 10.9bis P0-G prefix); data payload `{tenant_id, region, bytes, sku, ts, request_id, idempotency_key}` typed (NOT serde_json::Value — Lote 10.9-quinquies NEW-P0-2 lesson absorbed); event_schema_version "1.0.0" canonical com 2-version backward-compat policy (sprint contract §5.1 R-S10-3); sink R2 bucket `billing-events-<region>` Object Lock Governance Mode 7y retention (CTRL-BILLING-001 + INV-AUDIT-APPEND-ONLY foundation S-09 inheritance from WI-S09-004); idempotency staging table D1 `usage_event_staging(tenant_id, region, request_id, event_payload_hash)` UNIQUE (tenant_id, request_id) constraint dedup; replay-safe via idempotency_key derivation `corelink-{tenant_id_short(8)}-{operation}-{event_hash_short(8)}` 35 chars; **emit fail-OPEN no hot path** com staging table buffer + retry queue (distinct from WI-S09-004 audit fail-CLOSED — billing pipeline tolerates hot-path emit lag via idempotent re-emission; SLO-FRESH-BILLING ≤ 15min); cardinality budget inheritance from WI-S09-001 (event types canonical enum NOT string-typed; tenant_tier aggregation; trace_id em exemplar) |
| Sprint | S-10 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-BILLING-001 financial integrity; bypass = revenue leak silent), FF-HR-009 (events feed Stripe contract billing; lost event = customer dispute) |

## 1. Intent

Usage event emitter é **the foundation primitive** do billing pipeline — sem isso, INV-BILLING-NO-LOSS é violated por design. Cada CAS PUT/GET + AC lookup + GC purge no hot path emit CloudEvents v1.0 typed payload to R2 Object Lock 7y + D1 staging table for idempotent dedup. Emit fail-OPEN no hot path (não block customer request) mas staging table + retry queue garantem eventual delivery — billing tolerates ≤ 15min lag (SLO-FRESH-BILLING) but NÃO tolerates loss.

```rust
// File: crates/corelink-billing-events/src/lib.rs

#![forbid(unsafe_code)]

use cloudevents::{Event, EventBuilder};

#[async_trait]
pub trait UsageEventEmitter: Send + Sync {
    /// Emit usage event from CAS hot path; FAIL-OPEN (does NOT block request).
    /// Idempotency: (tenant_id, request_id) UNIQUE em staging table dedup.
    /// Retry queue handles eventual delivery to R2 + counter aggregation.
    async fn emit_usage(
        &self,
        tenant_ctx: &TenantCtx,                         // Lote 10.4bis enforcement
        usage_type: UsageType,                          // 4 canonical CloudEvents types
        usage_data: UsageEventData,                     // typed enum (Lote 10.9-quinquies P0-J + NEW-P0-2 lessons)
    ) -> Result<EventId, BillingEventError>;

    /// Replay-safe: idempotent re-emission via (tenant_id, request_id) UNIQUE.
    async fn emit_with_idempotency(
        &self,
        tenant_ctx: &TenantCtx,
        request_id: RequestId,                          // from W3C trace context (Lote 10.9 inheritance)
        usage_type: UsageType,
        usage_data: UsageEventData,
    ) -> Result<EmitOutcome, BillingEventError>;

    /// Drain staging table → R2 + advance idempotency watermark.
    async fn drain_staging_to_r2(&self, region: Region) -> Result<DrainReport, BillingEventError>;
}

// CloudEvents `type` field; canonical prefix `dev.hugr.corelink.<op>.v1` per Lote 10.9bis P0-G.
// R5 P0-E fix: was `corelink.usage.*` violating canonical prefix; aligned com WI-S10-005/006.
#[derive(strum::Display, strum::EnumIter, serde::Serialize)]
pub enum UsageType {
    #[strum(serialize = "dev.hugr.corelink.cas.put.v1")]
    CasPut,
    #[strum(serialize = "dev.hugr.corelink.cas.get.v1")]
    CasGet,
    #[strum(serialize = "dev.hugr.corelink.ac.lookup.v1")]
    AcLookup,
    #[strum(serialize = "dev.hugr.corelink.gc.purge.v1")]
    GcPurge,
}

/// Lote 10.9-quinquies NEW-P0-2 lesson absorbed: typed payload (NOT serde_json::Value);
/// Redact-wrapped fields where PII potential (digest_truncated via WI-S09-002 inheritance);
/// Serialize impl explicit on wrappers prevents raw value leak to immutable archive.
#[derive(serde::Serialize)]
#[serde(tag = "usage_type", rename_all = "snake_case")]
pub enum UsageEventData {
    CasPut {
        tenant_id: TenantId,
        region: Region,
        bytes: u64,                                     // size em bytes; SKU calculation input
        sku: Sku,                                       // canonical billable unit (5 SKUs canonical)
        ts: DateTime<Utc>,                              // event timestamp; UTC RFC 3339
        request_id: RequestId,                          // dedup key; from W3C tracecontext
        idempotency_key: IdempotencyKey,                // formato `corelink-{tenant_id_short(8)}-{operation}-{event_hash_short(8)}` = 35 chars; ver WI-S10-003 §6.1 derivação
        digest_truncated: BlobDigest,                   // redact!-wrapped from WI-S09-002 (16 hex chars)
    },
    CasGet {
        tenant_id: TenantId,
        region: Region,
        bytes: u64,                                     // egress size; bandwidth SKU
        sku: Sku,
        ts: DateTime<Utc>,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        digest_truncated: BlobDigest,
        cache_hit: bool,                                // affects pricing (S-07 dedup inheritance)
    },
    AcLookup {
        tenant_id: TenantId,
        region: Region,
        sku: Sku,                                       // ac_lookup_count SKU
        ts: DateTime<Utc>,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        cache_hit: bool,
    },
    GcPurge {
        tenant_id: TenantId,
        region: Region,
        bytes_reclaimed: u64,                           // negative billable (storage credit)
        ts: DateTime<Utc>,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        chunks_purged: u64,
    },
}

#[derive(strum::Display, strum::EnumIter)]
pub enum Sku {
    #[strum(serialize = "cas_storage_gb_month")]
    CasStorageGbMonth,                                  // primary GB-month storage SKU
    #[strum(serialize = "cas_egress_gb")]
    CasEgressGb,                                        // egress bandwidth SKU
    #[strum(serialize = "cas_put_op_count")]
    CasPutOpCount,                                      // operation count SKU (PUT)
    #[strum(serialize = "cas_get_op_count")]
    CasGetOpCount,                                      // operation count SKU (GET)
    #[strum(serialize = "ac_lookup_op_count")]
    AcLookupOpCount,                                    // AC lookup count SKU
}

pub enum EmitOutcome {
    NewlyEmitted(EventId),                              // first time (tenant_id, request_id) seen
    AlreadyEmitted(EventId),                            // idempotent dedup hit
    Queued(EventId),                                    // staging table buffered (retry queue handles)
}

#[derive(thiserror::Error, Debug)]
pub enum BillingEventError {
    #[error("staging table write failed (fail-OPEN; emit queued for retry): {0}")]
    StagingWriteFailed(String),

    #[error("R2 Object Lock write failed (fail-OPEN; staging retains; SLO-FRESH-BILLING risk): {0}")]
    R2WriteFailed(String),

    #[error("event schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("idempotency duplicate (already emitted; safe replay): event_id={existing_event_id}")]
    IdempotencyDuplicate { existing_event_id: String },

    #[error("PII detected em event data (CTRL-PRIV-001 violation; redact!-wrapper required): {0}")]
    PiiInEventData(String),
}
```

**Cripto-driven invariants enforced**:

1. **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136):
   - Σ(events emitidos no hot path) = Σ(events em R2 Object Lock) + Σ(events em staging pending retry) + Σ(events tombstoned via idempotency dedup).
   - Drift > 0.1% = SEV-2 alert + Finance review (HIGH severity → SEV-2 not SEV-1; SEV-1 reserved for CRITICAL invariants per registry §2).
   - Foundation invariant: enforced via staging table + retry queue (events nunca perdidos; only delayed).

2. **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137):
   - `(tenant_id, request_id)` UNIQUE em D1 staging table prevents duplicate.
   - Replay-safe: re-emission of same (tenant_id, request_id) returns `EmitOutcome::AlreadyEmitted`.

3. **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven em Lote 6.2; S-09 inheritance):
   - R2 Object Lock Governance Mode 7y enforces append-only.
   - Hash chain inheritance from WI-S09-004 pattern (per-region chain BLAKE3).

4. **CTRL-BILLING-001** (security_model.md): financial integrity; append-only events + 3-layer reconciliation.

5. **Lote 10.9-quinquies NEW-P0-2 lesson absorbed**: typed UsageEventData enum (NOT serde_json::Value); BlobDigest field uses WI-S09-002 wrapper com explicit `serde::Serialize` impl calling Redact::redact() — prevents raw blob digest writing to 7y immutable R2 billing archive.

6. **Emit fail-OPEN at hot path** (vs WI-S09-004 audit fail-CLOSED — distinct):
   - Hot path (CAS PUT/GET/AC) MUST NOT block on event emit failure (SLA p99 ≤ 3ms protected).
   - Staging table buffers; retry queue handles eventual R2 write.
   - SLO-FRESH-BILLING ≤ 15min event-to-counter (sprint contract §5.7 R-S10-14).
   - Distinção canonical Lote 10.6bis lesson absorbed: **billing event emit fail-OPEN at hot path** + **billing reconciliation fail-CLOSED at daily reconcile** (split-tier model).

7. **TenantCtx-only enforcement** (Lote 10.4bis): tenant_id from middleware S-03; NEVER request body.

8. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future()` for staging table + R2 writes; NEVER `tokio::spawn`.

9. **Idempotency key derivation** (sprint contract §5.3 R-S10-6 inheritance; canonical 35-char form per WI-S10-003 §6.1.3): `corelink-{tenant_id_short(8)}-{operation}-{event_hash_short(8)}` = 9+8+1+8+1+8 = 35 chars; `tenant_id_short` = first 8 hex chars of tenant UUID hyphens-stripped (UUID v4 entropy 122-bit); canonical Stripe Idempotency-Key compatible (Stripe limit 255 chars; CoreLink internal cap 64).

10. **5 SKUs canonical** (sprint contract §5.1 + §10 anti-scope single SKU initial):
    - `cas_storage_gb_month`, `cas_egress_gb`, `cas_put_op_count`, `cas_get_op_count`, `ac_lookup_op_count`.
    - GA scope; per-feature pricing expansion deferred (sprint contract §10).
    - 5 SKUs × 5 tier × 5 region = 125 séries baseline em métricas billing (R5 P1-G fix: era 30 regiões; canonical são 5 — iad/fra/nrt/syd/gru); well under WI-S09-001 100k budget.

11. **Schema versioning** (sprint contract §5.1 R-S10-3):
    - `event_schema_version: "1.0.0"` em CloudEvents extension attribute.
    - 2-version backward-compat policy: emitter at v1.0.1 produces events readable by v1.0.0 consumer (additive fields only).
    - Schema change requires Compliance Officer sign-off (SOC 2 CC1.4 audit trail).

12. **Late-arriving events policy** (sprint contract §5.2 R-S10-5):
    - Events com `ts < now - 6h` accepted but routed to `usage_counter_late` separate D1 table (WI-S10-002 concern).
    - Alert imediato (corelink_billing_late_event_total > threshold).
    - Protege contra silent backfill manipulation (FM-302 billing drift risk).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + financial integrity discipline justification)

Usage event emitter é **the foundation primitive** do CoreLink billing pipeline — sem disciplina rigorosa de event emit, INV-BILLING-NO-LOSS é violated por design e revenue leak silent é inevitável. Stripe Billing Architecture Guide (cited sprint contract §17): "exact metering requires every billable operation produces exactly one event; loss = revenue leak; duplication = customer trust loss + chargebacks".

**Why fail-OPEN no hot path** (vs WI-S09-004 audit fail-CLOSED): customer-facing CAS PUT/GET hot path SLA p99 ≤ 3ms (S-08 inheritance). Synchronous block on event emit failure (e.g., R2 outage) cascades to customer-facing 5xx. Trade-off: hot path SLA preserved (fail-OPEN) + staging table retains event (no loss) + retry queue handles eventual R2 write within SLO-FRESH-BILLING ≤ 15min. Distinct from audit (fail-CLOSED — compliance integrity > availability). Lote 10.6bis split-tier lesson canonical.

**Why typed UsageEventData (NOT serde_json::Value)** — Lote 10.9-quinquies NEW-P0-2 lesson critical absorbed: untyped JSON containers structurally defeat compile-time PII enforcement. WI-S09-004 P0-J fix introduced typed AuditEventData; quinquies validation revealed `#[derive(serde::Serialize)]` bypasses Redact::redact() unless wrapper types implement Serialize explicitly. WI-S10-001 inherits this lesson: typed enum + WI-S09-002 wrapper types (BlobDigest with explicit Serialize impl) ensure raw blob digest never writes to 7y immutable R2 billing archive.

**Why R2 Object Lock Governance Mode 7y**: SOC 2 CC1.4 (financial integrity) + GAAP ASC 606 (revenue recognition) require audit-grade event log. Governance Mode permite admin retention extension if needed (NOT Compliance Mode which forbids even legitimate extension). 7y = max coverage SOC 2 + ISO 27001 + HIPAA + safety margin (CoreLink target).

**Why (tenant_id, request_id) UNIQUE idempotency dedup**: replay safety canonical. Worker retry on transient failure → re-emission with same (tenant_id, request_id) returns `AlreadyEmitted` (no duplicate). Stripe Idempotency-Key pattern (sprint contract §17 reference) inherited at internal layer too.

**Why 5 SKUs canonical (NOT per-feature granularity)** — sprint contract §10 anti-scope: single SKU `cas_storage_gb_month` + 4 ops counts cover GA monetization. Per-feature pricing engine deferred S-19+ (custom pricing). Cardinality budget bounded.

**Adversarial scenarios**:
- **R2 outage 30min**: hot path emit fails fail-OPEN; staging table retains; retry queue drains on R2 recovery; ≤ 15min SLO-FRESH-BILLING usually preserved; SEV-2 alert if > 15min sustained.
- **Customer flooding spam events**: dedup via (tenant_id, request_id) UNIQUE — same request_id N times = 1 event; cardinality budget unaffected.
- **Replay attack via crafted request_id**: TenantCtx middleware (S-03) authenticates tenant_id; attacker cannot forge cross-tenant; intra-tenant replay benign (idempotent dedup).
- **Late-arriving event > 6h**: routed to `usage_counter_late` (WI-S10-002 concern); alert SEV-3; Finance review.
- **Schema migration v1.0.0 → v1.0.1**: 2-version backward-compat policy; consumer at v1.0.0 ignores new optional fields; emitter at v1.0.1 produces compatible payload.
- **PII em event data**: typed enum + redact!-wrapped BlobDigest prevents raw user input em payload.
- **Worker process kill mid-emit**: staging table write é DO durable (atomic); retry queue picks up on next worker invocation.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-BILLING-001 financial integrity; bypass = revenue leak silent.
- **FF-HR-009**: events feed Stripe contract billing; lost event = customer dispute legal exposure.
- 12 sign-offs (Finance + Legal + Privacy mandatory emphatic) + chaos suite + property test 100k race emit + TLA+ billing_atomicity (WI-S10-007 concern).

## 3. Customer Impact & Journey

**Persona 1 — Bazel client**: each CAS PUT/GET produces 1 usage event; client unaware (transparent); SLA p99 ≤ 3ms preserved (fail-OPEN at hot path); event delivered to R2 within 15min for billing pipeline.

**Persona 2 — DevOps reviewing**: opens DASH-COST (WI-S09-005 inheritance); per-tenant usage breakdown; identifies unusual spikes; correlates with corelink.billing.events_emitted_total métrica.

**Persona 3 — Finance auditor**: queries R2 `billing-events-<region>` archive 7y; reconstructs invoice from raw events (WI-S10-006 replay endpoint inheritance); SOC 2 CC1.4 evidence.

**Persona 4 — Customer disputing invoice**: customer claims overcharge; Finance triggers WI-S10-006 replay forensic API; events R2 raw match Stripe invoice byte-for-byte; dispute resolved with audit trail.

**Persona 5 — DevOps responding to event lag**: SLO-FRESH-BILLING SEV-2 alert (> 15min lag); investigates retry queue; root-causes R2 transient outage; recovery; backfill to counter.

**SLA addendum**:
- Event emit overhead: ≤ 50µs p99 hot path (fire-and-forget via worker::send_future).
- SLO-FRESH-BILLING: 99.9% events processed (chegam em counter D1) ≤ 15min após emissão.
- INV-BILLING-NO-LOSS: 0 events lost em chaos test 30d.
- INV-BILLING-NO-DUP: 0 duplicate charges em property test 100k retries.
- R2 Object Lock: 7y retention enforced; admin DELETE rejected.

## 4. Capability Mapping

- **CAP-BILLING-001** (Usage events append-only) — IMPLEMENTA primary.
- Trace: `data_model.md` (usage_event schema) + `security_model.md CTRL-BILLING-001` + `invariant_registry.md INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP + INV-AUDIT-APPEND-ONLY` + sprint contract §5.1 (R-S10-1/2/3) + CloudEvents v1.0 (CNCF) + Stripe Billing Architecture Guide.

## 5. Tipo

CloudEvents v1.0 emitter Rust crate + R2 Object Lock IaC + D1 staging table migration + retry queue via Cloudflare Queue; HIGH_RISK; FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-billing-events/` module** — UsageEventEmitter trait + impls + tests.

2. **CloudEvents v1.0 emission** (sprint contract §5.1 R-S10-1):
   - 4 canonical types: cas.put, cas.get, ac.lookup, gc.purge.
   - Strict schema validation via `cloudevents-cli` em CI.
   - Schema versioning v1.0.0 com 2-version backward-compat policy.

3. **R2 Object Lock IaC** (Terraform — R4 P0-6 fix: schema CF provider verificado pre-merge):
   ```hcl
   # NOTA pre-merge: validar resource type names contra `cloudflare/cloudflare` Terraform provider docs
   # versão pinned em `infra/cloudflare/versions.tf`. R2 Object Lock GA em Feb/2025; provider expõe
   # configuration via aninhamento dentro do `cloudflare_r2_bucket` (Object Lock + Lifecycle). NÃO copiar
   # AWS S3 patterns (`aws_s3_bucket_object_lock_configuration`) — schemas divergem. Storage class
   # transitions (Standard/IA/Archive) NÃO existem em R2 (only Expiration + AbortIncompleteMultipartUpload).
   resource "cloudflare_r2_bucket" "billing_events" {
       account_id = var.cloudflare_account_id
       name       = "billing-events-${var.region}"
       location   = var.region

       # Schema exato a validar contra provider docs antes de merge (placeholder canonical):
       object_lock_configuration {
           enabled                  = true
           default_retention_mode   = "GOVERNANCE"  # admin pode estender; não pode deletar dentro da window
           default_retention_days   = 2557          # 7 anos (365*7 + 2 leap)
       }

       lifecycle_rule {
           id      = "billing-events-7y-retention"
           enabled = true
           expiration {
               days = 2557  # 7 anos após Object Lock window
           }
       }
   }
   ```

4. **D1 idempotency staging table** (sprint contract §5.1 R-S10-2; CHECK constraints inline per Lote 10.5bis):
   ```sql
   -- R5 P0-B fix: CHECK values match `UsageType` strum serialization (long form com `dev.hugr.corelink.<op>.v1` prefix per Lote 10.9bis P0-G).
   -- R5 P1-A fix: `region` column added — drain consumer routes to correct per-region R2 bucket sem deserializar payload hash.
   CREATE TABLE usage_event_staging (
       tenant_id TEXT NOT NULL,
       region TEXT NOT NULL,                           -- canonical 5; required for drain routing (R5 P1-A)
       request_id TEXT NOT NULL,                       -- W3C trace context inherited
       event_type TEXT NOT NULL,                       -- canonical 4 types (long form com prefix `dev.hugr.corelink.*.v1`)
       event_payload_hash TEXT NOT NULL,               -- BLAKE3-256 of canonical_json(payload); collision detection
       event_id TEXT NOT NULL,                         -- ULID assigned on first emission
       emitted_at INTEGER NOT NULL,                    -- unix epoch milliseconds (column name no _ms suffix per Lote 10.7bis P0-3 convention)
       drained_to_r2_at INTEGER,                       -- NULL until retry queue drains; unix milliseconds when set
       PRIMARY KEY (tenant_id, request_id),
       CHECK (region IN ('iad', 'fra', 'nrt', 'syd', 'gru')),
       CHECK (event_type IN ('dev.hugr.corelink.cas.put.v1', 'dev.hugr.corelink.cas.get.v1', 'dev.hugr.corelink.ac.lookup.v1', 'dev.hugr.corelink.gc.purge.v1')),
       CHECK (drained_to_r2_at IS NULL OR drained_to_r2_at >= emitted_at)
   );

   CREATE INDEX idx_staging_pending_drain ON usage_event_staging(region, emitted_at)
       WHERE drained_to_r2_at IS NULL;                 -- region prefix permite drain consumer per-region

   CREATE INDEX idx_staging_per_tenant ON usage_event_staging(tenant_id, region, emitted_at);
   ```

5. **Retry queue via Cloudflare Queue** (drain staging → R2):
   - Queue `billing-events-drain-<region>` per region.
   - Consumer Worker batches staging entries (D1 batch ≤ 250 per Lote 10.5bis); writes R2 Object Lock; updates `drained_to_r2_at` watermark.
   - SEV-2 alert if `staging_pending_count` > 10000 sustained 30min (drain backpressure signal).

6. **Hot path integration** em CAS WI-S01-* + WI-S02-* + AC WI-S04-* + GC WI-S06-*:
   ```rust
   // Pseudo em CAS PUT handler:
   async fn cas_put_handler(req: Request, ctx: TenantCtx) -> Result<Response, Error> {
       // ... CAS PUT logic ...
       let blob_size = result.size_bytes;
       let digest = result.digest;

       // Lote 10.6bis split-tier emit: fail-OPEN at hot path
       worker::send_future(async move {
           let usage_data = UsageEventData::CasPut {
               tenant_id: ctx.tenant_id(),
               region: ctx.region(),
               bytes: blob_size,
               sku: Sku::CasStorageGbMonth,
               ts: Utc::now(),
               request_id: ctx.request_id(),
               idempotency_key: derive_idempotency_key(&ctx, &digest),
               digest_truncated: BlobDigest(digest.to_string()),  // WI-S09-002 wrapper; auto-redacted via Serialize impl
           };
           if let Err(e) = emitter.emit_with_idempotency(&ctx, ctx.request_id(), UsageType::CasPut, usage_data).await {
               // Fail-OPEN: log + counter; do NOT propagate
               worker::console_warn!("billing event emit failed (queued for retry): {}", e);
               emit_metric("corelink.billing.emit_failures_total", 1.0);
           }
       });

       // Customer response unaffected
       Ok(response)
   }
   ```

7. **Schema versioning** (sprint contract §5.1 R-S10-3):
   - CloudEvents extension `event_schema_version: "1.0.0"` em payload.
   - 2-version backward-compat policy: v1.0.0 + v1.0.1 both processable by counter aggregator (WI-S10-002).
   - Compliance Officer sign-off mandatory para schema major version change.

8. **Late-arriving events policy** (sprint contract §5.2 R-S10-5):
   - Hot path emits with `ts = now()`.
   - If somehow `ts < now - 6h` reaches emitter (clock skew, replay), event accepted but flagged `late_arriving=true` em payload.
   - WI-S10-002 routes flagged events to `usage_counter_late` separate D1 table.

9. **TenantCtx-only enforcement** (Lote 10.4bis): tenant_id from S-03 middleware; NEVER request body.

10. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future()`; `async_lock::Mutex` for staging table dedup if needed.

11. **PII redaction inheritance** (Lote 10.9-quinquies NEW-P0-2 lesson absorbed): `digest_truncated: BlobDigest` field inherits explicit `serde::Serialize` impl from WI-S09-002 calling `BlobDigest::redact()` → 16-char truncated digest written to R2 (NOT raw 64-char digest); raw PII never reaches immutable archive.

12. **Métricas operacionais** (via WI-S09-001 emit lib; cardinality budget):
    - `corelink_billing_events_emitted_total{type, region}` (counter; 4 types × 5 regions = 20 séries).
    - `corelink_billing_emit_failures_total{reason}` (counter; **alert SEV-3 if > 1%**).
    - `corelink_billing_staging_pending_count{region}` (gauge; **alert SEV-2 if > 10000 sustained 30min**).
    - `corelink_billing_late_event_total{type, region}` (counter; **alert SEV-3 if > 0** — late-arriving signal).
    - `corelink_billing_idempotency_dedup_total{type}` (counter; informational; replay-safety canary).
    - `corelink_billing_schema_validation_failures_total` (counter; **alert SEV-2 if > 0**).
    - `corelink_billing_pii_in_event_detected_total` (counter; **alert SEV-1 if > 0** — DLP regression critical).

13. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_idempotency_no_duplicate`: 100k retries same (tenant_id, request_id); assert dedup; 0 duplicate events em R2.
    - `prop_no_loss_under_failure`: simulate R2 outage random; assert staging table retains; eventual delivery via retry queue.
    - `prop_typed_payload_no_serde_json_value`: 10k random payloads; assert UsageEventData typed enum; serde_json::Value rejected at compile time.
    - `prop_blob_digest_redacted_em_serialization`: 10k events with raw digest input; assert serialized output contains only 16-char truncation NOT full 64-char (Lote 10.9-quinquies NEW-P0-2 inheritance).
    - `prop_schema_versioning_backward_compat`: 1k events v1.0.0 + 1k v1.0.1; assert both processable.
    - `prop_late_event_routing`: 1k events with `ts < now - 6h`; assert flagged late_arriving=true.

14. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    - 1. **R2 outage 30min**: hot path emit fail-OPEN; staging retains; retry queue drains on recovery; SLO-FRESH-BILLING preserved if < 15min.
    - 2. **D1 staging outage**: hot path emit returns StagingWriteFailed (logs + counter); does NOT block customer; on recovery, in-flight events lost (acceptable rare).
    - 3. **Idempotency duplicate**: 100k retries same (tenant_id, request_id); 0 duplicate events R2; property test 100k.
    - 4. **Cardinality discipline**: emit 1M events with valid SKU enum; assert ≤ 125 séries em métricas (5 SKUs × 5 tier × 5 region; well under WI-S09-001 budget).
    - 5. **Schema version v1.0.0 + v1.0.1 mixed**: emit both; counter aggregator (WI-S10-002) processes both; backward-compat preserved.
    - 6. **Late-arriving event** (synthetic ts < now - 6h): routed to usage_counter_late table (WI-S10-002 concern); alert SEV-3.
    - 7. **PII em event data leak attempt**: BlobDigest field with raw 64-char input; serialized output 16-char truncated; raw never reaches R2 (Lote 10.9-quinquies NEW-P0-2 verification).
    - 8. **Cross-tenant injection**: TenantCtx middleware (S-03 inheritance Lote 10.4bis); attacker JWT tenant T1; payload claims T2; T1 used; T2 events untouched.
    - 9. **Worker kill mid-emit**: staging table write atomic via DO durable; retry queue picks up; no event loss.
    - 10. **R2 Object Lock retention violation**: admin DELETE attempt within 7y window; rejected; SEV-1 alert.
    - 11. **TLA+ no-loss/no-dup verification**: WI-S10-007 billing_atomicity.tla model checked em CI.

### 6.2 Out-of-scope (deferred)

- Counter aggregation (delegate WI-S10-002).
- Stripe integration (delegate WI-S10-003).
- Reconciliation worker (delegate WI-S10-004).
- Quota state machine (delegate WI-S10-005).
- Replay forensic endpoint (delegate WI-S10-006).
- TLA+ billing_atomicity (delegate WI-S10-007).
- Customer-facing usage dashboard (deferred S-13).
- Multi-currency support (anti-scope sprint contract §10).

## 7. Anti-Scope

- ❌ serde_json::Value em event payload (Lote 10.9-quinquies NEW-P0-2 lesson; typed enum canonical).
- ❌ Fail-CLOSED at hot path (would cascade to customer 5xx; fail-OPEN canonical with staging buffer).
- ❌ Blocking R2 write on CAS request response path (latency tax).
- ❌ Raw blob digest em event payload (use BlobDigest wrapper; auto-redacted Serialize).
- ❌ Schema version-less events (sprint contract §5.1 R-S10-3 mandatory).
- ❌ Per-feature SKU granularity beyond 5 canonical (sprint contract §10 anti-scope).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ TenantCtx bypass.

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: Usage Event Emitter Hot Path + R2 Append-Only + Idempotency

  Scenario: CAS PUT emits usage event fail-OPEN
    Given tenant T (team plan) executes CAS PUT 1 KB blob
    When request handler completes CAS write
    Then worker::send_future dispatches usage event emit (fire-and-forget)
    Then customer response NOT blocked on event emit (SLA p99 ≤ 3ms preserved)
    Then UsageEventData::CasPut typed payload
    Then digest_truncated: BlobDigest serialized as 16-char truncation (Lote 10.9-quinquies NEW-P0-2)
    Then event_id ULID generated; staging table row inserted

  Scenario: Idempotency dedup
    Given tenant T emits CAS PUT with request_id R em T0
    When same (tenant_id=T, request_id=R) re-emitted T0+1s (Worker retry)
    Then EmitOutcome::AlreadyEmitted returned; existing event_id reused
    Then NO duplicate event em R2
    Then INV-BILLING-NO-DUP preserved
    Then property test prop_idempotency_no_duplicate green (100k iter)

  Scenario: R2 outage fail-OPEN
    Given R2 region IAD unavailable 30min
    When CAS PUT triggers usage event emit
    Then emit returns BillingEventError::R2WriteFailed (fail-OPEN; staging retains)
    Then customer request NOT impacted
    Then corelink_billing_emit_failures_total{reason=r2_unavailable} increments
    Then on R2 recovery: retry queue drains staging; events delivered ≤ 15min SLO-FRESH-BILLING

  Scenario: Schema versioning backward compatibility
    Given emitter at v1.0.1 (additive field new_metadata: Option<String>)
    Given consumer at v1.0.0 (no new_metadata field)
    When emitter produces event with new_metadata="extra"
    Then consumer parses successfully (ignores new_metadata)
    Then 2-version backward-compat policy preserved

  Scenario: Late-arriving event routed
    Given event with ts < now() - 6h emitted (synthetic clock skew)
    When emitter processes event
    Then late_arriving=true flagged em payload
    Then WI-S10-002 routes to usage_counter_late separate table
    Then SEV-3 alert: corelink_billing_late_event_total increments
    Then Finance review trigger

  Scenario: PII em event data prevented (Lote 10.9-quinquies NEW-P0-2)
    Given developer attempts emit with raw email em BillingEventError variant
    When clippy lint runs (redact! macro inheritance from WI-S09-002)
    Then compilation fails: "trait Redact not implemented for String"
    Then PR cannot merge

  Scenario: BlobDigest auto-redacted at serialization
    Given UsageEventData::CasPut com digest_truncated: BlobDigest("BLAKE3:abcdef0123456789abcdef...64-char-full")
    When serde_json::to_string(&event) called
    Then output contains "digest_truncated":"BLAKE3:abcdef0123456789..." (16-char truncated)
    Then output does NOT contain raw 64-char hex
    Then PII never written to R2 immutable 7y archive

  Scenario: 5 SKUs canonical enforcement
    Given developer attempts emit with custom SKU "cas_storage_tb_year"
    When CanonicalSku enum match runs
    Then compilation fails (no variant)
    Then 5 SKUs canonical preserved (sprint contract §10 anti-scope)

  Scenario: R2 Object Lock 7y retention
    Given event written to R2 billing-events-iad bucket at T0
    When 7y elapse
    Then R2 Lifecycle Expiration deletes event
    Then SOC 2 CC1.4 + GAAP retention satisfied
    Given admin attempts DELETE within 7y window
    Then R2 Object Lock Governance rejects
    Then SEV-1 alert; investigation

  Scenario: TenantCtx-only enforcement
    Given attacker JWT for tenant T1
    Given request body claims tenant_id=T2
    When emitter extracts TenantCtx (T1; Lote 10.4bis)
    Then event payload tenant_id=T1 (NOT T2)
    Then T2 billing untouched
    Then audit emit corelink.billing.cross_tenant_attempt; SEV-1 alert

  Scenario: 100k retry property test (INV-BILLING-NO-LOSS + NO-DUP)
    Given proptest 100k random (tenant_id, request_id) emit calls
    When chaos injects R2 outage 5% probability + retry
    Then assert Σ(events em R2) + Σ(staging pending) = Σ(unique tenant_id+request_id emitted)
    Then assert Σ(R2 events) <= Σ(unique) (no duplicates)
    Then INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP preserved
```

## 9. Design Decisions

- 9.1: CloudEvents v1.0 (CNCF spec canonical; aligns com WI-S09-004 audit pattern).
- 9.2: Typed UsageEventData enum (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 lesson absorbed).
- 9.3: 4 canonical types (cas.put, cas.get, ac.lookup, gc.purge).
- 9.4: 5 SKUs canonical (anti-scope sprint contract §10 single SKU initial expanded; per-feature deferred).
- 9.5: Fail-OPEN at hot path (vs WI-S09-004 audit fail-CLOSED; Lote 10.6bis split-tier lesson).
- 9.6: (tenant_id, request_id) UNIQUE idempotency dedup (replay-safe canonical).
- 9.7: R2 Object Lock Governance Mode 7y (SOC 2 CC1.4 + GAAP).
- 9.8: BlobDigest wrapper with explicit serde::Serialize impl (Lote 10.9-quinquies NEW-P0-2 inheritance).
- 9.9: Cloudflare Queue retry queue (drain staging → R2; eventual delivery).
- 9.10: D1 staging table com PRIMARY KEY (tenant_id, request_id); UNIQUE enforces idempotency.
- 9.11: Late-arriving event policy 6h threshold (sprint contract §5.2 R-S10-5).
- 9.12: Schema versioning v1.0.0 + 2-version backward-compat (sprint contract §5.1 R-S10-3).
- 9.13: TenantCtx-only enforcement (Lote 10.4bis).
- 9.14: CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.15: NEW INVs INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP em invariant_registry — verify canonical position before commit (Lote 10.8bis P1-13 lesson).

## 10. Completeness Criteria SOTA

- [ ] **10.s10.001.1** Crate compila + integration tests green.
- [ ] **10.s10.001.2** All 11 Gherkin scenarios green.
- [ ] **10.s10.001.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s10.001.4** Chaos suite 11 scenarios green.
- [ ] **10.s10.001.5** **INV-BILLING-NO-LOSS** chaos test 30d sustained zero losses (sprint contract §6 DoD).
- [ ] **10.s10.001.6** **INV-BILLING-NO-DUP** property test 100k retries zero duplicates.
- [ ] **10.s10.001.7** R2 Object Lock 7y configured + verified via Cloudflare API; admin DELETE rejected.
- [ ] **10.s10.001.8** D1 staging table migration applied; PRIMARY KEY (tenant_id, request_id) UNIQUE enforced.
- [ ] **10.s10.001.9** Cloudflare Queue retry drain operational; SLO-FRESH-BILLING ≤ 15min.
- [ ] **10.s10.001.10** Schema version v1.0.0 + 2-version backward-compat tested.
- [ ] **10.s10.001.11** PII redaction inheritance verified: BlobDigest serializes 16-char truncated NOT raw 64-char (Lote 10.9-quinquies NEW-P0-2).
- [ ] **10.s10.001.12** Métricas (7) emitted via WI-S09-001 emit lib; cardinality budget respected (~120 séries baseline).
- [ ] **10.s10.001.13** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s10.001.14** Cost regression gate per sprint contract §14.s10.8 (R2 7y storage projection).

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Finance + Legal + Privacy emphatic).

## 12. Invariants Validated

- **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136; verify via grep before commit per Lote 10.8bis P1-13): Σ(events emitted) = Σ(R2) + Σ(staging) + Σ(dedup); chaos test 30d zero losses.
- **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137): (tenant_id, request_id) UNIQUE; property test 100k retries zero duplicates.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance from WI-S09-004): R2 Object Lock 7y enforces.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant TenantCtx enforcement; cross-tenant impossible.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): TenantCtx-only middleware S-03 inheritance.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Usage event emitter module | `crates/corelink-billing-events/` | Rust |
| CloudEvents schema | `specs/_schemas/usage_event.cloudevents.json` | JSON |
| R2 Object Lock IaC | `infra/cloudflare/r2/billing_events_bucket.tf` | Terraform |
| Cloudflare Queue IaC | `infra/cloudflare/queue/billing_events_drain.tf` | Terraform |
| D1 staging migration | `migrations/00X_usage_event_staging.sql` | SQL |
| Hot path integration | `crates/corelink-worker/src/hot_path_billing.rs` | Rust |
| Property tests | `crates/corelink-billing-events/tests/prop_billing.rs` | Rust |
| Chaos suite | `tests/chaos_billing_events.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s10.001.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s10.001.2: rustdoc 100% public API.
- 14.s10.001.3: Test coverage ≥ 90%.
- 14.s10.001.4: Hot path emit overhead ≤ 50µs p99.
- 14.s10.001.5: SAST clean; cloudevents-cli strict.
- 14.s10.001.6: Métricas (7 §6.1.12).
- 14.s10.001.7: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s10.001.8: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV positions §3.9 L136/137 (NO-LOSS/NO-DUP) + §3.12 L166/167 (RECONCILE-3-LAYER/REPLAYABLE) verified Lote 10.10bis (Lote 10.8bis P1-13 lesson absorbed).
- 14.s10.001.9: **Lote 10.9-quinquies NEW-P0-2 inheritance**: typed UsageEventData (NOT serde_json::Value); BlobDigest wrapper explicit serde::Serialize impl prevents raw PII writing to immutable archive.
- 14.s10.001.10: D1 batch ≤ 250 (Lote 10.5bis); CHECK inline (Lote 10.5bis).
- 14.s10.001.11: Audit fail-OPEN at hot path canonical (Lote 10.6bis split-tier; vs audit fail-CLOSED em S-09 + reconcile fail-CLOSED em S-10 WI-004).
- 14.s10.001.12: BLAKE3-256 hash chain consistency com S-09 WI-S09-004 + CAS digests primary.

## 15. Chaos Experiments (11)

§6.1.14 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Finance + Legal + Privacy emphatic).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + UsageEventEmitter trait + 4 UsageType + 5 SKU enums | 2 |
| ST-002 | CloudEvents v1.0 emit lib + schema validation + version 1.0.0 | 2 |
| ST-003 | UsageEventData typed enum (Lote 10.9-quinquies NEW-P0-2 inheritance) | 2 |
| ST-004 | Hot path integration CAS PUT/GET + AC + GC | 3 |
| ST-005 | D1 staging table migration + idempotency dedup logic | 2 |
| ST-006 | R2 Object Lock IaC + Cloudflare Queue retry drain | 2 |
| ST-007 | Late-arriving events policy + flagging logic | 1 |
| ST-008 | Schema versioning + 2-version backward-compat | 1.5 |
| ST-009 | Métricas (7) emit | 1 |
| ST-010 | Property tests (6 × 10k; 100k nightly) | 3 |
| ST-011 | Chaos suite (11) | 2.5 |
| ST-012 | Privacy review + Compliance schema versioning sign-off | 1 |

**Total**: ~23h. **PERT** O=14h M=22h P=36h: **~23h** (matches sprint contract §12 estimate exactly).

## 18. Dependencies

- Hard: S-01 + S-02 SEALED (CAS hot paths emit points); S-04 SEALED (AC); S-06 SEALED (GC); S-03 SEALED (TenantCtx + request_id from W3C trace context); WI-S09-001 SEALED (cardinality emit lib); WI-S09-002 SEALED (BlobDigest wrapper + serde::Serialize impl); WI-S09-004 SEALED (CloudEvents pattern + R2 Object Lock IaC reference).
- Soft: WI-S10-002 (counter aggregator consumes staging); WI-S10-007 (TLA+ billing_atomicity).
- Hard infra: Cloudflare R2 Object Lock per region; Cloudflare Queue available; D1 per region.

## 19. Effort PERT: ~23h. ## 20. Time-boxing: 36h hard limit.

## 21. Observability

7 metrics §6.1.12. Trace span `billing.{emit, idempotency_check, staging_write, drain_to_r2, schema_validate}`.

## 22. Cost Analysis

- R2 Object Lock storage: 7y × 1 KB/event × 1B events/yr = 7 TB/region; $0.015/GB-mo × 7000 GB × 12 = ~$1260/região/yr storage; × 5 regions = ~$6300/yr.
- R2 PUT operations: $4.50/1M × 1B/yr × 5 regions = ~$22500/yr.
- D1 staging table: ~10 GB rolling; $0.75/GB-mo × 10 × 12 = ~$90/yr.
- Cloudflare Queue: $0.40/1M ops × 1B/yr × 5 regions = ~$2000/yr.
- TCO 12m: ~$31000/yr billing infrastructure.
- **Cost saved by INV-BILLING-NO-LOSS**: prevents revenue leak (potential millions undetected); replay-from-events SOC 2 audit value indispensable.

## 23. API Contract

- Public Rust: `UsageEventEmitter` trait + `UsageType`, `UsageEventData`, `Sku`, `EmitOutcome`, `BillingEventError` types; `#[non_exhaustive]`.
- Wire: CloudEvents v1.0 (CNCF spec).
- Storage: R2 Object Lock Governance Mode 7y per region.
- Idempotency: D1 staging table (tenant_id, request_id) UNIQUE.

## 24. Post-mortem Hooks

- INV-BILLING-NO-LOSS violation detected → HIGH-severity post-mortem (revenue leak).
- INV-BILLING-NO-DUP violation → HIGH-severity (double-charge customer).
- SLO-FRESH-BILLING > 15min sustained → SEV-2; staging drain investigation.
- PII em event data leak → CRITICAL post-mortem; CTRL-PRIV-001 + CTRL-BILLING-001 violations.
- R2 Object Lock retention violation attempt → SEV-1 + investigation.
- Schema version mismatch processing failure → SEV-2 + Compliance review.

## 25. Rollback / Recovery

- Rollback: revert hot path integration; events not emitted; INV-BILLING-NO-LOSS would fail (avoid rollback once shipped; instead disable + fix forward).
- Recovery: hot path re-mounted; staging retains in-flight; eventual drain to R2; reconciliation (WI-S10-004) catches any drift.
- RTO ≤ 5min; RPO ≤ 0min (staging table durable).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware S-03; cross-tenant injection blocked.
- T(ampering): R2 Object Lock + hash chain S-09 inheritance.
- R(epudiation): immutable event log = audit evidence.
- I(nformation disclosure): BlobDigest wrapper redacts PII; explicit Serialize impl prevents raw leak.
- D(enial of Service): hot path fail-OPEN; staging buffer absorbs.
- E(scalation of Privilege): R2 Object Lock Governance prevents admin DELETE.

**LINDDUN** (LGPD/GDPR mandatory):
- L(inkability): per-tenant events; tenant_id em payload.
- I(dentifiability): BlobDigest wrapper redacts (16-char truncated NOT raw); audit chain integrity preserved.
- N(on-repudiation): R2 Object Lock 7y + hash chain.
- D(etectability): customer access via WI-S10-006 replay endpoint.
- D(isclosure): 7y retention vs erasure right (LGPD Art. 16 vs Art. 18); humane review pattern (Lote 10.8bis precedent).
- U(nawareness): customer notified via S-13 admin plane.
- N(on-compliance): **CTRL-BILLING-001 + SOC 2 CC1.4 + GAAP ASC 606 + LGPD Art. 32 + GDPR Art. 32 compliance** via append-only events + hash chain + 7y retention.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-10 Usage Events: CloudEvents + Idempotency + Fail-OPEN Split-Tier"; doc `docs/dev/billing-events-architecture.md`; onboarding test 8 questions: 4 canonical types + 5 SKUs canonical, fail-OPEN at hot path vs fail-CLOSED audit (Lote 10.6bis split-tier), idempotency dedup pattern, BlobDigest serde::Serialize boundary (Lote 10.9-quinquies NEW-P0-2), schema versioning 2-version backward-compat, late-arriving 6h policy, R2 Object Lock Governance vs Compliance mode, TenantCtx-only enforcement.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-BILLING-NO-LOSS violation | L | M | HIGH | M | LOW | Staging buffer + retry queue + chaos 30d; SEV-2 alert (HIGH severity → SEV-2 canonical) |
| R-002 | INV-BILLING-NO-DUP violation (double-charge) | L | M | HIGH | M | LOW | (tenant_id, request_id) UNIQUE + property test 100k |
| R-003 | PII em event data (Lote 10.9-quinquies NEW-P0-2) | L | M | CRITICAL | L | LOW | BlobDigest wrapper explicit Serialize impl from WI-S09-002 inheritance |
| R-004 | R2 outage causes SLO-FRESH-BILLING breach | M | L | MEDIUM | L | LOW | Staging buffer + retry queue; ≤ 15min eventual; SEV-2 alert |
| R-005 | Schema version regression | L | L | MEDIUM | L | LOW | 2-version backward-compat policy; tests both versions |
| R-006 | Late-arriving events silent revenue leak | M | H | HIGH | H | LOW | 6h policy + alert + separate usage_counter_late |
| R-007 | Cardinality drift via SKU expansion | L | L | LOW | L | LOW | 5 SKUs canonical enum closed (sprint contract §10) |
| R-008 | Hot path latency tax > 50µs | L | L | LOW | L | LOW | worker::send_future fire-and-forget; criterion benchmark |
| R-009 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-010 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.9 (NO-LOSS L136 / NO-DUP L137) + §3.12 (RECONCILE-3-LAYER L166 / REPLAYABLE L167) verified Lote 10.10bis; ongoing maintenance discipline via grep CI gate |
| R-011 | Cost regression > 10% storage | L | L | LOW | L | LOW | §14.s10.8 gate; ADR required |
| R-012 | Cross-tenant injection via tenant_id field | L | L | CRITICAL | L | LOW | TenantCtx-only S-03 inheritance; payload tenant_id ignored if mismatch |

## 29. Review Checkpoints

D+0 design (Architect; fail-OPEN split-tier); D+1 Finance (financial integrity discipline); D+2 Legal (DPA reference); D+3 AppSec (TenantCtx + R2 Object Lock); D+4 Privacy (LINDDUN + redact! inheritance); D+5 Compliance (schema versioning + audit trail); D+6 code review; D+7 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — CTRL-BILLING-001 + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 30d + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC1.4 + GAAP ASC 606 + schema versioning_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + BlobDigest wrapper inheritance from WI-S09-002_ |
| 11 | Architect | _TBD; **mandatory emphatic** — fail-OPEN split-tier (Lote 10.6bis) + Lote 10.9-quinquies NEW-P0-2 absorption + INV §3.X verification_ |
| 12 | Finance | _TBD; **mandatory emphatic** — financial integrity audit-grade + replay-from-events foundation_ |

(Legal sign-off via DPA reference at sprint level; not per-WI.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.10) | Criação WI-S10-001; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris + Lote 10.9bis/tris/quaters/quinquies lessons absorbed: 5-tier canonical Tier (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X canonical position TBD verification before commit (Lote 10.8bis P1-13); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis). **Lote 10.9-quinquies NEW-P0-2 critical absorption**: typed UsageEventData enum (NOT serde_json::Value); BlobDigest wrapper inherits explicit serde::Serialize impl from WI-S09-002 calling Redact::redact() — prevents raw PII writing to 7y immutable R2 billing archive. **Lote 10.6bis split-tier lesson**: fail-OPEN at hot path canonical (vs WI-S09-004 audit fail-CLOSED + reconcile fail-CLOSED em S-10 WI-004). NEW corelink-billing-events crate + CloudEvents v1.0 schema + R2 Object Lock 7y IaC + D1 staging table + Cloudflare Queue retry drain. INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP CRITICAL invariants new (registry position TBD). 4 canonical types + 5 canonical SKUs (sprint contract §10 anti-scope). Schema versioning 2-version backward-compat. Late-arriving events 6h policy. CTRL-BILLING-001 + SOC 2 CC1.4 + GAAP ASC 606 compliance foundation. |
| 1.1.0 | 2026-04-26 | Gustavo (Lote 10.10bis) | P0+P1 remediation pós-R4 Opus + Sonnet R5 adversarial reviews. **8 P0s fixed**: (1) PlanTier 5-tuple inventado `free/team/enterprise/custom/trial` → canonical `free/solo/team/business/enterprise` per `data_model.md §1` line 68 — corrige CHECK constraints em WIs 003+005 + sprint contract §5.3 R-S10-7; (2) INV registry positions estampadas `§3.X TBD` → reais §3.9 line 136 (NO-LOSS HIGH), §3.9 line 137 (NO-DUP HIGH), §3.12 line 166 (RECONCILE-3-LAYER HIGH), §3.12 line 167 (REPLAYABLE HIGH); severity drift CRITICAL→HIGH cascateia em SEV-2 alert wording (vs SEV-1); Lote 10.8bis P1-13 lesson finalmente absorvida; (3) WI-007 TLA+ math 5× errado (15k claim → real 3,000; 150k → real 30,000) + caveat sobre TLC reachable state graph ≠ Cartesian product CONSTANTS; MaxConcurrentEvents bound NEW; (4) CTRL-AUTHZ-005 alucinado (security_model só define -001/-002) → CTRL-AUTHZ-001 + CTRL-AUTHZ-002 explicit; sprint contract §5.6 R-S10-13 também atualizado; (5) WI-002 UPSERT WHERE clause auto-anulante removido (`WHERE own_digest = excluded.own_digest` rejeitava updates legítimos); (6) Cloudflare R2 Terraform schema marcada com TODO pre-merge verification (era hallucinated AWS S3 pattern `cloudflare_r2_bucket_lock_configuration`); (7) **R5 P0-A**: WI-002 PRIMARY KEY `(tenant_id, sku, hour)` → `(tenant_id, region, sku, hour)` — multi-region tenants antes corrompiam dados a cada UPSERT; (8) **R5 P0-B**: WI-001 D1 CHECK enum `('cas_put',...)` rejeitava 100% dos inserts (strum serializa para long form `dev.hugr.corelink.cas.put.v1`); aligned com Lote 10.9bis P0-G CloudEvents prefix canonical. **P1s fixed**: (a) staging table region column added (R5 P1-A); (b) WI-003 narrative 62-char idempotency key contradiziando §6.1 35-char (R5 P1-B); (c) idempotency comment WI-001 (P1-C); (d) `CHECK (age_hours > 6)` off-by-one → `>= 6` (R5 P1-D); (e) cardinality 30 regions → 5 canonical (P1-G); (f) PercentValue type/DB CHECK alignment 0..=200 (R5 P1-H); (g) chrono::next_month_first_utc_midnight() (não existe em chrono crate) → corelink_time::next_month_first_utc_midnight() canonical helper internal; (h) CTRL-PRIV-002 mis-mapped to "pseudonymization" → privacy_model.md L209 "data classification tags" + S-11 cooperation para DSR pseudonymization separada; (i) sprint contract §5.7 R-S10-15 metric names dots → underscores (Prom canonical); (j) §5.1 R-S10-1 CloudEvents prefix `corelink.usage.*` → `dev.hugr.corelink.<op>.v1`; (k) WI-005 4-state vs 5-state — GraceActive removido como state, virou flag boolean preserving sprint contract §5.5 R-S10-10 canonical; (l) failure_modes.md adicionado RB-FM-151 mapping (P1-5). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.10-quaters) | P0+P1 remediation pós-R4 + R5 round-2 audits (R4 6.9/10, R5 7.8/10). Same S-08/S-09 cascade-incomplete pattern: bis ~60% complete; quaters fecha residuais. **8 P0s addressed**: (1) NEW-P0-1 spec contract §8 invariants L149-151 CRITICAL→HIGH (era source-of-truth não atualizada; severity contradicting WIs); (2) NEW-P0-2 WI-002 PK 4-tuple cascade ~30%→100% — `CounterRecord` + `LateCounterRecord` Rust structs add `region: Region` field (digest input para tamper detection per-region); 15 narrative/Gherkin/design-decision references swept (tenant_id, sku, hour) → (tenant_id, region, sku, hour); (3) NEW-P0-3 TLA+ `MaxConcurrentEvents` CONSTANT NEW added to body L58-65 + bounded em EmitEvent; `Hours` time abstraction comment clarified (R4 round-1 caveat absorbed); `quota_grace_active` separate VARIABLE (R5 NEW-P0-1 fix: grace é flag, não 5º state value); `ReconcileLayer1` action NEW with drift computation (R4 round-1 P2-10 fix); (4) WI-003 PlanTier residuals §1 item 11 L247-250 + §6.1.13 L548 swept canonical 5-tuple; (5) WI-002 Gherkin L530 UPSERT WHERE clause text fixed (R4 P0-5 partial); (6) WI-001 risk register R-001/R-002 + post-mortem hooks severity CRITICAL→HIGH; SEV-1 → SEV-2 (R4 NEW-P1-2). **P1s addressed**: (a) WI-005 PercentValue cluster L74/L178/L328 align com 0..=200 DB CHECK (R4 NEW-P1-1 + R5 NEW-P1-3); (b) WI-006 sign-off `Finance + Legal + Architect` → `Finance + Compliance Officer + Architect` (Legal sprint-level only; R4 NEW-P1-4 / round-1 P1-6 finalmente fix); (c) WI-003 título L29 + WI-005 L727 + spec contract L244 CTRL-PRIV-002 mis-mapping → "data classification tags @classification=pii" + S-11 separated DSR; (d) WI-002 §6.1.11 late event reference time → cron_now (R5 NEW-P1-1); (e) WI-002 §14 corelink_time crate path canonical reference added (`crates/corelink-time/src/canonical.rs`); (f) WI-S10-003 Idempotency-Key 64-char internal cap rationale ADR documented em line 367; (g) WI-S10-007 ADR-S10-007-tla-substitutes-proptest justifying property test exemption + lightweight sanity props NEW; (h) WI-S10-006 "Mfa" → "MFA" capitalization (30 occurrences); (i) WI-004 Layer 2/3 narrative aggregation semantics clarified (collapsed across regions for Stripe scope; per-region drift preservada via Layer 1). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.4.0 | 2026-05-03 | Gustavo (impl SEAL) | **WI-S10-001 IMPL SEALED** — `crates/corelink-billing-emit/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern. **Modules**: `event` (UsageEvent CloudEvents 1.0 envelope + 6-element UsageEventKind taxonomy + IdemKey BLAKE3-256 32-byte newtype + UsageUnit canonical bytes/op_count + validate_billing_period YYYY-MM guard); `idempotency` (derive_idem_key BLAKE3-256-of-JCS-with-slot-zeroed canonical formula + IdempotencyTracker trait + InMemoryIdempotencyTracker per-tenant set-membership + IdempotencyDecision Accepted/DuplicateRejected + IdempotencyCollision SEV-1 surface); `sink` (R2UsageSink trait + InMemoryR2UsageSink append-only NDJSON layout `usage/{tenant_id}/{billing_period YYYY-MM}/{seq:08}.usage.ndjson` + INV-BILLING-APPEND-ONLY enforcement at trait surface + per-(tenant, billing_period) sequence ledger); `emitter` (UsageEventEmitter trait + InMemoryUsageEventEmitter orchestrator: canonicalize → derive idem_key → idempotency check → audit envelope BEFORE state mutation → R2 PutObject); `audit` (BillingAuditEventType `#[non_exhaustive]` 4-event taxonomy `corelink.billing.{usage_emitted, duplicate_rejected, sink_failure, idempotency_collision}` + BillingAuditSink trait + InMemoryBillingAuditSink + FailingBillingAuditSink fail-CLOSED envelope); `error` (BillingEmitError + BillingAuditSinkError + R2UsageSinkError canonical `#[non_exhaustive]` taxonomies). **Migration**: `migrations/d1/0017_usage_event_idem.sql` ships canonical `usage_event_staging(tenant_id, region, request_id, event_type, event_payload_hash, event_id, emitted_at, drained_to_r2_at)` PRIMARY KEY (tenant_id, request_id) + 2 indexes (idx_usage_event_staging_pending_drain partial WHERE drained_to_r2_at IS NULL, idx_usage_event_staging_per_tenant) + 7 CHECK inline constraints (Lote 10.4bis discipline). **Tests**: 63 inline unit tests + 10 integration property tests (`prop_idem_key_deterministic`, `prop_idem_key_unique_per_event`, `prop_duplicate_rejected_on_replay`, `prop_append_only_no_overwrite`, `prop_tenant_isolation`, `prop_audit_emit_per_decision_arm`, `prop_jcs_canonicalization_byte_stable`, `prop_billing_period_format_yyyy_mm`, `canonical_usage_event_kinds_pinned`, `canonical_billing_audit_event_strings_pinned`) at 10k iter PR-gate via PROPTEST_CASES env var (S-07 P1-2 pattern); nightly 100k via env override. **Production wiring deferred to WI-S10-007**: R2 PutObject + Object Lock Governance Mode 7y retention + Cloudflare Queue retry drain + (tenant_id, request_id) UNIQUE D1 staging table production binding + Terraform IaC. **Quality gates green**: cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings (clean), cargo test -p corelink-billing-emit --all-targets (73/73 pass), validate_specs.py (280/286 schema; 6 YAML-only), check_migrations_additive.py (17 files OK). **Charter compliance**: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance Arc<Mutex<>> F-001 closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope on every decision arm BEFORE state mutation; PROPTEST_CASES env-var read at runtime; ChaCha20Rng PRNG pinned; BLAKE3-256 + JCS RFC 8785 canonical hash chain mirroring S-09 audit-chain discipline. |
| 1.3.0 | 2026-04-26 | Gustavo (Lote 10.10-sextus) | P0+P1 remediation pós-R4 + R5 round-3 audits (R4 8.0/10, R5 9.1/10 CONDITIONAL SEAL). 2 P0s + 8 P1s residuais addressed. **2 P0s**: (1) R4 NEW-P0-1 severity cascade incompleto — quaters só varreu WI-001; sextus completou WI-002 (L745-746 post-mortem + L786 R-001) + WI-003 (L869-870) + WI-007 (L743 + L785 R-005) — todos `INV-BILLING-NO-LOSS/NO-DUP` violation references → HIGH-severity post-mortem + SEV-2 alert (não SEV-1). (2) R4 NEW-P0-2 WI-S10-004 Layer 1 ainda 3-tuple `(tenant, sku, hour)` → 4-tuple `(tenant, region, sku, hour)` em título L41 + trait doc L67 + narrative L217 + Gherkin L521 + sprint contract §5.2 R-S10-4 line 86 + CAP-BILLING-002 line 65 — mascarava multi-region drift via cross-region offsets (financial-grade integrity restored). **8 P1s**: (a) R5 P1-QUIN-1 WI-002 L538 + step 5 Gherkin/narrative `hour_window.start_ts` → `cron_now`; (b) R5 P1-QUIN-2 WI-001 L210 §1 item 9 idempotency key long-form → 35-char canonical; (c) R4 P1-1 WI-003 título L41 + ST-010 sub-task CTRL-PRIV-002 pseudonymization → "data classification tags @classification=pii" + S-11 separated DSR; (d) R4 P1-2 WI-006 §29 D+3 review checkpoint Legal sign-off → sprint-level only (NÃO 3-of-3 per-replay; matched §6.1.6 fix); (e) R4 P1-3 spec contract metadata bumped v1.1.0/sota-v1.1/2026-04-24 → v1.3.0/sota-v1.3/2026-04-26; (f) R4 P1-4 + R5 P1-QUIN-3 TLA+ block: `BillingPeriods` + `RequestIds` agora declared CONSTANTS; helper operators (`Abs`, `SumCounters`, `SumLineItems`, `ComputeStripeInvoiceId`, `StripeIdParts`, `EventAccountedInInvoice`) documented as separate INSTANCE module; (g) R5 P1-QUIN-4 INV_BILLING_NO_LOSS retry_queue Sequence membership bug fixed via `RetrySet == {retry_queue[i] : i \in DOMAIN retry_queue}` Range conversion (semantic correctness); (h) R4 P1-5 `compute_counter_digest()` doc explicitly confirms region IS in canonical_json input (cross-region tamper detection guaranteed); (i) R4 P1-6 §14.s10.x.8 + R-010/R-011 risk register stale "INV §3.X TBD pre-merge" → concrete §3.9 L136/137 + §3.12 L166/167 verified Lote 10.10bis (residual após mitigação LOW). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |

## 32. Anti-patterns evitados

- ❌ serde_json::Value em event payload (Lote 10.9-quinquies NEW-P0-2; typed canonical); ❌ Fail-CLOSED at hot path (cascade to customer 5xx); ❌ Blocking R2 write on response path; ❌ Raw blob digest em payload; ❌ Schema version-less events; ❌ Per-feature SKU granularity (anti-scope §10); ❌ tokio::spawn em CF Workers; ❌ TenantCtx bypass; ❌ Skip idempotency dedup (replay-unsafe); ❌ Skip late-arriving events policy (silent backfill manipulation).

---

**Fim WI-S10-001.** Próximo: WI-S10-002 (Counter aggregator cron DO + hash chain).
