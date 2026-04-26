---
id: "WI-S10-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-26"
updated: "2026-04-26"
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
tags: ["wi", "s10", "counter-aggregator", "billing", "durable-object", "hash-chain", "late-arrival", "high-risk"]
---

# WI-S10-002 — Counter Aggregator Cron Durable Object + Hash Chain Tamper Detection + Late-Arrival Split (`BillingAggregatorCron-<region>` per-region DO; Cloudflare Workers cron-trigger 1h interval drains usage_event_staging from WI-S10-001 → R2 hour bucket replay → aggregates por `(tenant_id, region, sku, hour)` → D1 `usage_counter` table; hash chain BLAKE3 per-region [`prev_hash`+`own_digest` formato canonical inheritance from WI-S09-004]; `usage_counter_late` separate D1 table for events com `ts < now - 6h` per sprint contract §5.2 R-S10-5 + alert SEV-3; INV-BILLING-NO-LOSS counter-layer enforcement via reconciliation Layer 1 contract com WI-S10-004; **fail-CLOSED at counter aggregation** [vs WI-S10-001 hot path fail-OPEN — Lote 10.6bis split-tier canonical: counter integrity > availability]; CRITICAL atomic write D1 transaction for counter + hash chain pair; corelink_time::next_month_first_utc_midnight() canonical [Lote 10.8bis P0-D]; cron retry idempotent via watermark-based replay)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-10](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S10-002 |
| Título | Counter aggregator cron Durable Object per-region (`BillingAggregatorCron-<region>`; Cloudflare Workers cron-trigger 1h interval per region; routing via `tenant.primary_region` per Lote 10.7bis P0-9 DO routing canonical); aggregation logic (drains usage_event_staging from WI-S10-001 + replays R2 hour bucket; aggregates por `(tenant_id, region, sku, hour)` SUM(bytes) + SUM(ops); 5 SKUs canonical inheritance from WI-S10-001); D1 schema `usage_counter` (PRIMARY KEY (tenant_id, region, sku, hour) UNIQUE; CHECK constraints inline per Lote 10.5bis); hash chain BLAKE3-256 per-region (prev_hash + canonical_json(counter_record) digest = own_digest = next prev_hash; tamper detection inheritance from WI-S09-004 audit hash chain pattern); `usage_counter_late` separate D1 table for events com `ts < now - 6h` (sprint contract §5.2 R-S10-5; SEV-3 alert + Finance review trigger); SLO-FRESH-BILLING ≤ 15min event-to-counter (sprint contract §5.7 R-S10-14 enforcement layer); fail-CLOSED canonical (Lote 10.6bis split-tier: counter aggregation integrity > availability — vs WI-S10-001 hot path fail-OPEN); corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D inheritance); typed payload no serde_json::Value (Lote 10.9-quinquies NEW-P0-2); 12 sign-offs HIGH_RISK (sprint contract §6 + framework §33.5.4.3 cap; Finance + Legal + Privacy emphatic) |
| Sprint | S-10 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-BILLING-001 financial integrity counter layer; bypass = miscount silent), FF-HR-009 (counter feeds Stripe invoice; counter drift = customer dispute) |

## 1. Intent

Counter aggregator é **the integrity primitive** entre raw events (WI-S10-001) e Stripe invoice (WI-S10-003) — sem aggregation discipline, INV-BILLING-NO-LOSS é violated via lost events between R2 raw e usage_counter D1, e INV-BILLING-RECONCILE-3-LAYER (sprint contract §8 NEW INV) é unenforceable porque Layer 1 (R2 events ↔ counter D1) not measurable. Cron Durable Object hourly drains staging (WI-S10-001) + replays R2 hour bucket → aggregates por `(tenant_id, region, sku, hour)` → D1 `usage_counter` com hash chain BLAKE3 per-region. Late-arriving events (`ts < now - 6h`) routed to `usage_counter_late` separate table per sprint contract §5.2 R-S10-5 (silent backfill manipulation prevention).

```rust
// File: crates/corelink-billing-counter/src/aggregator.rs

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use blake3::Hasher;

#[async_trait]
pub trait CounterAggregator: Send + Sync {
    /// Cron trigger entry: drain WI-S10-001 staging + replay R2 hour bucket.
    /// FAIL-CLOSED: any error halts aggregation; SEV-1 alert.
    /// Atomic: counter row + hash chain advance em mesma D1 transaction.
    async fn aggregate_hour(
        &self,
        region: Region,                                 // DO routing per Lote 10.7bis P0-9
        hour_window: HourWindow,                        // start_ts (UTC hour-aligned) + end_ts
    ) -> Result<AggregateReport, CounterError>;

    /// Watermark-based idempotent retry: cron may fire 2× same hour;
    /// counter rows UPSERT-safe via PRIMARY KEY (tenant_id, region, sku, hour) UNIQUE.
    async fn replay_from_watermark(
        &self,
        region: Region,
        watermark: AggregateWatermark,                  // last successful (region, hour)
    ) -> Result<ReplayReport, CounterError>;

    /// Late-arrival routing: events with ts < now() - 6h go to usage_counter_late.
    /// Returns count routed for SEV-3 alert telemetry.
    async fn route_late_arrivals(
        &self,
        region: Region,
        late_events: Vec<UsageEvent>,                   // typed (NOT serde_json::Value); Lote 10.9-quinquies NEW-P0-2
    ) -> Result<LateRoutingReport, CounterError>;

    /// Hash chain advance: previous chain head + new counter digest = next chain head.
    /// Inheritance from WI-S09-004 audit hash chain pattern.
    async fn advance_hash_chain(
        &self,
        region: Region,
        new_counter_record: &CounterRecord,             // canonical_json source
    ) -> Result<HashChainHead, CounterError>;
}

#[derive(serde::Serialize)]
pub struct CounterRecord {
    pub tenant_id: TenantId,
    pub region: Region,                                 // 5 canonical (iad/fra/nrt/syd/gru); R5 P0-A — region em PK + canonical_json digest input para tamper detection per-region
    pub sku: Sku,                                       // 5 canonical inheritance from WI-S10-001
    pub hour: DateTime<Utc>,                            // UTC hour-aligned canonical
    pub qty_bytes: u64,                                 // sum bytes per (tenant, region, sku, hour); 0 for op-count SKUs
    pub qty_ops: u64,                                   // sum operations
    pub event_count: u32,                               // raw event count contributing
    pub aggregated_at: DateTime<Utc>,                   // when DO produced this counter
    pub prev_hash: HashChainHead,                       // chain link (BLAKE3-256 hex)
    pub own_digest: HashChainHead,                      // BLAKE3-256(canonical_json(self - own_digest)); region INCLUDED in canonical_json so cross-region records produce distinct digests
}

#[derive(serde::Serialize)]
pub struct LateCounterRecord {
    pub tenant_id: TenantId,
    pub region: Region,                                 // 5 canonical; PK component + digest input
    pub sku: Sku,
    pub original_event_ts: DateTime<Utc>,               // when event SHOULD have been
    pub aggregated_at: DateTime<Utc>,                   // when DO actually saw it
    pub age_hours: u32,                                 // (now - original_event_ts).hours()
    pub qty_bytes: u64,
    pub qty_ops: u64,
    pub event_count: u32,
}

#[derive(thiserror::Error, Debug)]
pub enum CounterError {
    #[error("D1 transaction failed (fail-CLOSED; aggregation halted): {0}")]
    D1TransactionFailed(String),

    #[error("R2 hour bucket read failed (fail-CLOSED; SEV-1 alert): {0}")]
    R2ReadFailed(String),

    #[error("hash chain integrity violation detected (CRITICAL; tampering signal): {0}")]
    HashChainViolation(String),

    #[error("schema version mismatch (incompatible event v{received} for aggregator v{expected})")]
    SchemaVersionMismatch { received: String, expected: String },

    #[error("watermark regression detected (cron concurrent fire suspected): {0}")]
    WatermarkRegression(String),

    #[error("counter row already exists with different digest (replay corruption): {0}")]
    CounterDigestMismatch(String),
}

pub struct HourWindow {
    pub start_ts: DateTime<Utc>,                        // hour-aligned UTC
    pub end_ts: DateTime<Utc>,                          // start_ts + 1h exact
}

impl HourWindow {
    /// Canonical UTC hour-aligned constructor; rejects non-aligned input.
    pub fn from_aligned(start: DateTime<Utc>) -> Result<Self, CounterError> {
        if start.minute() != 0 || start.second() != 0 || start.nanosecond() != 0 {
            return Err(CounterError::WatermarkRegression(
                format!("HourWindow start_ts must be UTC hour-aligned; got {start}")
            ));
        }
        Ok(Self {
            start_ts: start,
            end_ts: start + chrono::Duration::hours(1),
        })
    }
}
```

**Cripto-driven invariants enforced**:

1. **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136):
   - Σ(events R2) per hour = Σ(usage_counter qty) + Σ(usage_counter_late qty) per hour for each (tenant, sku).
   - Counter aggregator is Layer 1 enforcement of 3-layer reconciliation (sprint contract §5.4 R-S10-8; WI-S10-004 concern).
   - Drift > 0.1% = SEV-2 alert + Finance review; aggregator halts subsequent hours até resolution (fail-CLOSED downstream). HIGH severity → SEV-2 canonical (registry §2).

2. **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137):
   - PRIMARY KEY (tenant_id, region, sku, hour) UNIQUE em usage_counter.
   - UPSERT semantics: re-aggregation of same hour produces idempotent same digest.
   - Watermark-based replay safe.

3. **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance from WI-S09-004):
   - Counter records append-only via D1 INSERT-only pattern; UPDATE rejected (only DELETE-and-reINSERT via admin emergency procedure with hash chain re-link).
   - Hash chain BLAKE3-256 per-region detects tampering.

4. **INV-BILLING-RECONCILE-3-LAYER** (HIGH; sprint contract §8 NEW; registry pending):
   - Layer 1 enforcement: Σ(R2 events) ↔ Σ(usage_counter) per (tenant, region, sku, hour).
   - Counter aggregator is the producer; reconciliation worker (WI-S10-004) is the consumer.

5. **CTRL-BILLING-001** (security_model.md): financial integrity; counter records + hash chain.

6. **Lote 10.9-quinquies NEW-P0-2 lesson absorbed**: typed CounterRecord + LateCounterRecord (NOT serde_json::Value); inherits WI-S09-002 wrapper Serialize impls if PII fields present (none in counter — counters store aggregates only, no per-event PII).

7. **Fail-CLOSED at counter aggregation** (vs WI-S10-001 hot path fail-OPEN — Lote 10.6bis split-tier canonical):
   - Counter integrity > availability; partial counter row = financial drift = inadmissible.
   - D1 transaction atomic: counter INSERT + hash_chain_head UPDATE em mesma transaction; on failure, both rollback.
   - SEV-1 alert if aggregation halted; pages Finance + SRE.

8. **TenantCtx propagation** (Lote 10.4bis): events from WI-S10-001 carry tenant_id; aggregator never extracts tenant_id from request body (no request body — cron trigger).

9. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): DO state for hash chain head; D1 prepared statements for batch UPSERT; NEVER `tokio::spawn` ou `std::thread`.

10. **DO routing per tenant.primary_region** (Lote 10.7bis P0-9 canonical):
    - One DO instance per region: `BillingAggregatorCron-iad`, `-fra`, `-nrt`, `-syd`, `-gru`.
    - 5 regions canonical (sprint contract §17 inheritance from S-08 PagerDuty 3 services × regions).
    - Tenant.primary_region routing prevents cross-region race; counter rows scoped per (region, tenant, sku, hour).

11. **5 SKUs canonical** (sprint contract §10 anti-scope; inheritance WI-S10-001):
    - cas_storage_gb_month, cas_egress_gb, cas_put_op_count, cas_get_op_count, ac_lookup_op_count.
    - Counter table cardinality: 5 SKUs × 1000 tenants × 720 hours/month = 3.6M rows/month/region; manageable em D1 (D1 limit ~10GB; 3.6M rows × ~200B = 720MB/month/region ✓).

12. **Schema versioning** (sprint contract §5.1 R-S10-3 inheritance):
    - Aggregator validates `event_schema_version` em event payload; rejects unknown major versions.
    - 2-version backward-compat: aggregator v1.0.x processes events v1.0.0 + v1.0.1.
    - Schema version mismatch = SchemaVersionMismatch error (fail-CLOSED).

13. **Late-arriving events policy** (sprint contract §5.2 R-S10-5):
    - Threshold: `ts < now - 6h` → `usage_counter_late` table; SEV-3 alert + Finance review trigger.
    - Protege contra silent backfill manipulation (FM-302 billing drift).
    - Late counter records still subject to hash chain (separate `usage_counter_late_hash_chain` per-region).

14. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D inheritance):
    - Monthly boundary calculation NEVER ad-hoc `now() + Duration::days(30)`.
    - Used em GAAP cutoff time (sprint contract §14.s10.7) + monthly aggregate rollup downstream.

15. **Hash chain BLAKE3-256 per-region** (inheritance WI-S09-004 pattern):
    - Genesis hash: BLAKE3-256("corelink-billing-counter-genesis-${region}-${YYYY-MM}").
    - Each counter record digest = BLAKE3-256(canonical_json(record without own_digest field)).
    - Chain head stored em DO state + persisted em D1 `hash_chain_head(region, current_head, last_hour)`.
    - Tamper detection: WI-S10-004 reconciliation Layer 1 verifies chain integrity daily.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + financial integrity discipline justification)

Counter aggregator é **the financial integrity layer** entre raw events (R2 immutable archive) e Stripe invoice generation (WI-S10-003). Stripe Billing Architecture Guide (sprint contract §17): "exact metering requires hourly or finer aggregation com tamper-evident audit chain; loss between event-emit e counter-table = silent revenue leak inadmissível for SOC 2 CC1.4 audit." Counter layer must be **deterministic** (replay produces identical digest), **idempotent** (cron retry safe), e **tamper-evident** (hash chain detects any post-hoc modification).

**Why fail-CLOSED at aggregation** (vs WI-S10-001 hot path fail-OPEN): Lote 10.6bis split-tier canonical lesson absorbed — counter integrity is non-negotiable. Hot path (CAS PUT/GET) fail-OPEN protects customer SLA p99 ≤ 3ms; aggregator é background cron — NO customer SLA at risk. Partial counter row (e.g., D1 transaction failed mid-write) = drift > 0.1% Layer 1 reconciliation = customer dispute. Therefore aggregation halts on any error; SEV-1 alert; manual replay via `replay_from_watermark()` after root-cause fix.

**Why hash chain BLAKE3-256 per-region** (inheritance WI-S09-004): tamper detection is SOC 2 CC1.4 + GAAP ASC 606 audit-grade requirement. Linear chain (prev_hash + own_digest) detects single-point tampering immediately; daily reconciliation worker (WI-S10-004) verifies head matches expected via WI-S10-001 R2 raw event replay. Per-region chain (vs single global chain) prevents cross-region single-point-of-failure: region A chain corruption does NOT invalidate region B; isolation discipline.

**Why `usage_counter_late` separate table** (sprint contract §5.2 R-S10-5): silent backfill manipulation is FM-302 billing drift attack vector — adversary (or buggy system) emits events 7h+ delayed claiming retroactive usage. Mixing late events into `usage_counter` causes hourly aggregate rewrite = hash chain break = audit chaos. Separation isolates late events em distinct table com SEV-3 alert + Finance review; legitimate late events processed manually after triage.

**Why DO per-region routing** (Lote 10.7bis P0-9): one DO instance per region eliminates concurrent aggregation races. Cron trigger fires hourly per region independently; tenant.primary_region routing ensures (tenant, region, hour) unique compute. 5 regions canonical (iad, fra, nrt, syd, gru); cardinality bounded.

**Adversarial scenarios**:
- **D1 transaction half-fails mid-write**: rollback complete; SEV-1 alert; manual replay via watermark; INV-BILLING-NO-LOSS preserved.
- **Cron fires 2× same hour (CF Workers retry)**: PRIMARY KEY (tenant_id, region, sku, hour) UNIQUE = UPSERT idempotent; second invocation produces same digest (deterministic); no duplicate.
- **Hash chain tampering attempt**: admin INSERT directly to usage_counter without hash chain advance → WI-S10-004 reconciliation detects mismatch; SEV-1 alert.
- **Late-arriving event 7h+**: routed to `usage_counter_late`; SEV-3; Finance triage.
- **Schema version v1.0.0 + v1.0.1 mixed em hour bucket**: aggregator processes both (2-version backward-compat); v1.0.2+ rejected → SchemaVersionMismatch SEV-2.
- **R2 hour bucket missing (deletion attack within 7y)**: R2 Object Lock prevents (sprint contract §6 DoD + WI-S10-001 enforcement); aggregator detects via WI-S09-004 hash chain re-verification; SEV-1.
- **Aggregator overrun > 1h** (event volume spike): DO continues; next cron fires anyway; watermark prevents double-aggregate; SLO-FRESH-BILLING SEV-2 if > 15min sustained.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: counter integrity é financial-grade requirement; bypass = revenue leak.
- **FF-HR-009**: counter feeds Stripe invoice direct; bug = customer dispute legal exposure.
- 12 sign-offs (Finance + Legal + Privacy emphatic) + chaos suite + property test 100k race aggregate + TLA+ no-loss/no-dup (WI-S10-007 concern).

## 3. Customer Impact & Journey

**Persona 1 — Bazel client**: unaffected directly (background cron); usage events emitted by hot path (WI-S10-001) aggregated into counters; SLA p99 ≤ 3ms preserved.

**Persona 2 — DevOps reviewing usage**: opens DASH-COST (WI-S09-005); per-tenant `usage_counter` aggregates plotted; identifies cost trends.

**Persona 3 — Finance auditor (SOC 2)**: queries `usage_counter` D1 + verifies hash chain integrity via WI-S10-004 reconciliation; reconstructs invoice from R2 raw events (WI-S10-006); 3-layer match.

**Persona 4 — Customer disputing invoice**: claims overcharge for Sept 2026; Finance triggers WI-S10-006 replay; events R2 → re-aggregated → matches `usage_counter` byte-for-byte → matches Stripe invoice; dispute resolved.

**Persona 5 — SRE responding to SLO-FRESH-BILLING SEV-2**: aggregator > 15min lag detected; investigates DO state; root-causes D1 transaction contention; mitigation; backfill via watermark replay.

**Persona 6 — Compliance officer reviewing late-arriving event SEV-3**: 7h+ late event detected; investigates source (clock skew, replay attack, buggy backfill); decides triage path (route to legitimate billing OR reject per RB-BILLING-002).

**SLA addendum**:
- Aggregation latency: ≤ 5min p99 per hour bucket (SLO-FRESH-BILLING ≤ 15min event-to-counter sustained).
- INV-BILLING-NO-LOSS: 0 events lost between R2 raw e usage_counter em chaos test 30d.
- INV-BILLING-NO-DUP: 0 duplicate counter rows em property test 100k.
- Hash chain integrity: 100% verifiable em WI-S10-004 daily reconciliation.
- Late-arriving events: < 0.1% mensal sustained (alert threshold; > 5% triggers post-mortem).

## 4. Capability Mapping

- **CAP-BILLING-002** (Counter aggregation hourly) — IMPLEMENTA primary.
- Trace: `data_model.md` (usage_counter schema) + `security_model.md CTRL-BILLING-001` + `invariant_registry.md INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP + INV-AUDIT-APPEND-ONLY + INV-BILLING-RECONCILE-3-LAYER` + sprint contract §5.2 (R-S10-4/5) + Stripe Billing Architecture Guide + Lago aggregation reference.

## 5. Tipo

Counter aggregator Rust crate + Cloudflare Durable Object cron-triggered worker + D1 schema migration `usage_counter` + `usage_counter_late` + hash chain head table; HIGH_RISK; FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-billing-counter/` module** — CounterAggregator trait + DO impl + tests.

2. **DO `BillingAggregatorCron-<region>`** (Cloudflare Workers Durable Object):
   - Cron-trigger config: `0 * * * *` (every hour at :00 UTC) per region.
   - State em DO durable: hash chain head, watermark, last_aggregated_hour.
   - 5 regions canonical: iad, fra, nrt, syd, gru.
   - Routing: `tenant.primary_region` (Lote 10.7bis P0-9).

3. **Aggregation logic** (sprint contract §5.2 R-S10-4):
   - Step 1: drain WI-S10-001 staging table — events with `drained_to_r2_at IS NULL` AND `emitted_at >= hour_window.start_ts AND emitted_at < hour_window.end_ts`.
   - Step 2: replay R2 hour bucket `billing-events-<region>/YYYY/MM/DD/HH/` for completeness (catches retry queue late drains).
   - Step 3: deduplicate by (tenant_id, request_id) UNIQUE — events seen em both staging + R2 counted once.
   - Step 4: aggregate by (tenant_id, region, sku, hour) → SUM(bytes), SUM(ops), COUNT(events).
   - Step 5: separate late-arriving events (`event.ts < cron_now - 6h` where `cron_now` é fixed-snapshot invocation timestamp; canonical per R5 P1-QUIN-1) → `usage_counter_late`.
   - Step 6: D1 transaction: UPSERT usage_counter + INSERT hash_chain_head update.

4. **D1 schema `usage_counter`** (CHECK constraints inline per Lote 10.5bis; PRIMARY KEY inclui `region` para evitar colisão multi-region — R5 P0-A fix):
   ```sql
   CREATE TABLE usage_counter (
       tenant_id TEXT NOT NULL,
       region TEXT NOT NULL,                           -- canonical 5; PK component (R5 P0-A: omitir region causa multi-region overwrite)
       sku TEXT NOT NULL,
       hour INTEGER NOT NULL,                          -- unix epoch SECONDS hour-aligned; canonical primitive
       qty_bytes INTEGER NOT NULL DEFAULT 0,
       qty_ops INTEGER NOT NULL DEFAULT 0,
       event_count INTEGER NOT NULL DEFAULT 0,
       aggregated_at INTEGER NOT NULL,                 -- unix epoch MILLISECONDS (column name no _ms suffix per Lote 10.7bis P0-3; convention via narrative)
       prev_hash TEXT NOT NULL,                        -- BLAKE3-256 hex (64 chars)
       own_digest TEXT NOT NULL,                       -- BLAKE3-256 hex (64 chars)
       schema_version TEXT NOT NULL DEFAULT '1.0.0',
       PRIMARY KEY (tenant_id, region, sku, hour),     -- 4-tuple per (R5 P0-A); region MUST be in PK
       CHECK (sku IN ('cas_storage_gb_month', 'cas_egress_gb', 'cas_put_op_count', 'cas_get_op_count', 'ac_lookup_op_count')),
       CHECK (region IN ('iad', 'fra', 'nrt', 'syd', 'gru')),
       CHECK (qty_bytes >= 0),
       CHECK (qty_ops >= 0),
       CHECK (event_count >= 0),
       CHECK (length(prev_hash) = 64),
       CHECK (length(own_digest) = 64),
       CHECK (hour % 3600 = 0)                         -- enforces UTC hour-alignment (hour em unix seconds)
   );

   CREATE INDEX idx_usage_counter_tenant_hour ON usage_counter(tenant_id, hour);
   CREATE INDEX idx_usage_counter_region_hour ON usage_counter(region, hour);
   CREATE INDEX idx_usage_counter_sku_hour ON usage_counter(sku, hour);
   ```

5. **D1 schema `usage_counter_late`** (separate table per sprint contract §5.2 R-S10-5; PK include `region` por consistência com `usage_counter` — R5 P0-A inheritance):
   ```sql
   CREATE TABLE usage_counter_late (
       tenant_id TEXT NOT NULL,
       region TEXT NOT NULL,                           -- canonical 5; PK component
       sku TEXT NOT NULL,
       original_event_ts INTEGER NOT NULL,             -- when event SHOULD have been (unix seconds)
       aggregated_at INTEGER NOT NULL,                 -- when DO actually saw it (unix milliseconds)
       age_hours INTEGER NOT NULL,                     -- (now - original_event_ts).hours()
       qty_bytes INTEGER NOT NULL DEFAULT 0,
       qty_ops INTEGER NOT NULL DEFAULT 0,
       event_count INTEGER NOT NULL DEFAULT 0,
       prev_hash TEXT NOT NULL,                        -- separate hash chain for late events
       own_digest TEXT NOT NULL,
       triaged_at INTEGER,                             -- NULL until Finance review (unix milliseconds when set)
       triaged_decision TEXT,                          -- 'accept' | 'reject' | 'manual_invoice'
       PRIMARY KEY (tenant_id, region, sku, original_event_ts, aggregated_at),
       CHECK (sku IN ('cas_storage_gb_month', 'cas_egress_gb', 'cas_put_op_count', 'cas_get_op_count', 'ac_lookup_op_count')),
       CHECK (region IN ('iad', 'fra', 'nrt', 'syd', 'gru')),
       CHECK (age_hours >= 6),                         -- R5 P1-D fix: was `> 6` off-by-one (boundary 6h era silently dropped); inclusive `>= 6` é canonical (sprint contract §5.2 R-S10-5)
       CHECK (qty_bytes >= 0),
       CHECK (qty_ops >= 0),
       CHECK (triaged_decision IS NULL OR triaged_decision IN ('accept', 'reject', 'manual_invoice'))
   );

   CREATE INDEX idx_usage_counter_late_pending ON usage_counter_late(aggregated_at)
       WHERE triaged_at IS NULL;
   ```

6. **D1 schema `hash_chain_head`** (per-region chain head; inheritance WI-S09-004 pattern):
   ```sql
   CREATE TABLE hash_chain_head (
       region TEXT NOT NULL,
       chain_kind TEXT NOT NULL,                       -- 'usage_counter' | 'usage_counter_late'
       current_head TEXT NOT NULL,                     -- BLAKE3-256 hex (64 chars)
       last_aggregated_hour INTEGER,                   -- unix epoch hour-aligned (NULL = genesis state)
       updated_at INTEGER NOT NULL,
       PRIMARY KEY (region, chain_kind),
       CHECK (region IN ('iad', 'fra', 'nrt', 'syd', 'gru')),
       CHECK (chain_kind IN ('usage_counter', 'usage_counter_late')),
       CHECK (length(current_head) = 64),
       CHECK (last_aggregated_hour IS NULL OR last_aggregated_hour % 3600 = 0)
   );
   ```

7. **Hash chain BLAKE3-256 algorithm** (inheritance WI-S09-004; R4 P1-5 fix: digest input EXPLICITLY inclui region):
   ```rust
   pub fn compute_counter_digest(record: &CounterRecord) -> HashChainHead {
       // Canonical JSON serialization (sorted keys, no whitespace) sans own_digest field.
       // CRITICAL: `region` field IS included no canonical_json input — garante que cross-region records
       // (mesmo tenant_id+sku+hour) produzam digests DISTINTOS (R5 P0-A + R4 NEW-P0-2 cascade fix).
       // Sem region em canonical_json, multi-region tenants causariam silent tamper detection failure.
       let canonical = serde_canonical_json::to_string_canonical(&record_sans_digest(record))
           .expect("typed CounterRecord serializes deterministically; region included em payload");
       let mut hasher = Hasher::new();
       hasher.update(canonical.as_bytes());
       hasher.update(record.prev_hash.as_bytes());     // chain link
       HashChainHead(hex::encode(hasher.finalize().as_bytes()))
   }

   pub fn genesis_head(region: Region, chain_kind: ChainKind, year_month: &str) -> HashChainHead {
       let mut hasher = Hasher::new();
       hasher.update(format!("corelink-billing-{}-{}-genesis-{}", chain_kind, region, year_month).as_bytes());
       HashChainHead(hex::encode(hasher.finalize().as_bytes()))
   }
   ```

8. **Atomic D1 transaction** (CRITICAL — fail-CLOSED):
   ```rust
   async fn write_counter_atomic(
       db: &D1Database,
       record: &CounterRecord,
       new_chain_head: &HashChainHead,
   ) -> Result<(), CounterError> {
       // R5 P0-A fix: PK includes region. R4 P0-5 fix: removed self-defeating WHERE clause that prevented late-arrival re-aggregation.
       // Idempotency garantida via PK (tenant_id, region, sku, hour) UNIQUE; digest mismatch detection é explícita ANTES da UPSERT (CounterDigestMismatch error path)
       // — não silenciosa via WHERE no UPSERT.
       let stmt_counter = db.prepare(
           "INSERT INTO usage_counter (tenant_id, region, sku, hour, qty_bytes, qty_ops, event_count, aggregated_at, prev_hash, own_digest, schema_version)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (tenant_id, region, sku, hour) DO UPDATE SET
              qty_bytes = excluded.qty_bytes,
              qty_ops = excluded.qty_ops,
              event_count = excluded.event_count,
              aggregated_at = excluded.aggregated_at,
              prev_hash = excluded.prev_hash,
              own_digest = excluded.own_digest"  // re-aggregation overwrites com latest digest (idempotent for stable input; corrige quando late-arrivals mudam payload)
       );
       let stmt_chain = db.prepare(
           "UPDATE hash_chain_head SET current_head = ?, last_aggregated_hour = ?, updated_at = ?
            WHERE region = ? AND chain_kind = 'usage_counter'"
       );
       db.batch(vec![stmt_counter.bind(...)?, stmt_chain.bind(...)?]).await
           .map_err(|e| CounterError::D1TransactionFailed(e.to_string()))?;
       Ok(())
   }
   ```

9. **Watermark-based replay idempotency**:
   - DO state: `last_aggregated_hour` per region per chain_kind.
   - Cron retry: aggregator queries hash_chain_head; if (region, hour) already aggregated AND digest matches re-computed = idempotent skip.
   - If digest differs = CounterDigestMismatch error (replay corruption); fail-CLOSED.

10. **Schema versioning enforcement** (sprint contract §5.1 R-S10-3):
    - Reject events with `event_schema_version` major mismatch.
    - 2-version backward-compat: aggregator v1.x.y processes events v1.0.0 + v1.0.1 (additive fields ignored gracefully).
    - SchemaVersionMismatch error fail-CLOSED; SEV-2 alert.

11. **Late-arriving event detection + routing** (sprint contract §5.2 R-S10-5):
    - **Reference time canonical** (R5 NEW-P1-1 fix): cron invocation timestamp `now()` (fixed at start of aggregation run); aligned com sprint contract §5.2 R-S10-5 wording "ts < now - 6h" + WI-001 §6.1.8 + `LateCounterRecord.age_hours = (now - original_event_ts).hours()`. Antes a narrative usava `hour_window.start_ts - 6h` que produzia classificação non-determinística entre cron runs.
    - Threshold: `event.ts < cron_now - chrono::Duration::hours(6)` (cron_now é fixed-snapshot do timestamp de início da aggregation run).
    - Routed to `usage_counter_late` separate table.
    - SEV-3 alert: `corelink_billing_late_event_total{tenant_id, sku, region}` > 0.
    - Finance review trigger via PagerDuty `corelink-finance` service (S-09 inheritance).

12. **chrono primitives canonical** (Lote 10.8bis P0-D inheritance):
    - `corelink_time::next_month_first_utc_midnight()` for monthly boundary (used em downstream WI-S10-003 invoice generation).
    - `Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap()` rejected — use canonical helper.
    - `HourWindow::from_aligned()` enforces UTC hour-alignment.

13. **TenantCtx propagation** (Lote 10.4bis): tenant_id from event payload (already authenticated by WI-S10-001 middleware S-03); aggregator does NOT re-authenticate.

14. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): DO state via `state.storage().get/put`; D1 prepared statements; NEVER `tokio::spawn`.

15. **DO routing per tenant.primary_region** (Lote 10.7bis P0-9 canonical): one DO per region; cron-trigger fires per region.

16. **5-tier canonical Plan reference** (Lote 10.7bis P0-7): tier_id em counter rows derived from tenant.plan; cardinality bounded; per-tier aggregate views available downstream.

17. **Métricas operacionais** (via WI-S09-001 emit lib; cardinality budget; underscored canonical Lote 10.9bis P0-E):
    - `corelink_billing_counter_aggregations_total{region, status}` (counter; status ∈ success/error; 5×2 = 10 séries).
    - `corelink_billing_counter_aggregation_duration_seconds{region}` (histogram; 5 regions × 11 buckets = 55 + sum/count).
    - `corelink_billing_counter_events_processed_total{region, sku}` (counter; 5×5 = 25 séries).
    - `corelink_billing_counter_late_event_total{region, sku, age_bucket}` (counter; **alert SEV-3 if > 0**; 5×5×4 = 100 séries; age_bucket ∈ 6-12h, 12-24h, 24-72h, >72h).
    - `corelink_billing_counter_hash_chain_head_age_seconds{region}` (gauge; **alert SEV-2 if > 5400s** = 1.5h staleness).
    - `corelink_billing_counter_schema_version_mismatch_total{region, received_version}` (counter; **alert SEV-2 if > 0**).
    - `corelink_billing_counter_d1_transaction_failures_total{region, reason}` (counter; **alert SEV-1 if > 0**).
    - `corelink_billing_counter_digest_mismatch_total{region}` (counter; **alert SEV-1 if > 0** — replay corruption).
    - `corelink_billing_counter_drift_pct{region, sku}` (gauge; Layer 1 reconciliation drift; **alert SEV-1 if > 0.1%**).

18. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_aggregate_idempotent`: 100k random hour buckets aggregated 2× each; assert identical digest both times.
    - `prop_no_loss_aggregate`: 100k random event sets; assert Σ(events bytes) = Σ(counter qty_bytes) per (tenant, region, sku, hour).
    - `prop_no_dup_aggregate`: 100k random events with duplicate (tenant_id, request_id); assert dedup; counter reflects unique only.
    - `prop_late_routing`: 1k events com synthetic ts em random offset; assert events with `ts < now - 6h` routed to usage_counter_late.
    - `prop_hash_chain_integrity`: 1k counter writes; assert chain head advances; assert recomputed digest matches stored.
    - `prop_schema_version_backward_compat`: 1k events v1.0.0 + 1k v1.0.1 mixed; assert both processed; v1.1.0 rejected.

19. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    1. **D1 transaction failure mid-aggregate**: rollback complete; SEV-1 alert; manual replay via watermark; INV-BILLING-NO-LOSS preserved.
    2. **Cron fires 2× same hour (CF Workers retry)**: PRIMARY KEY UNIQUE; UPSERT idempotent; identical digest; INV-BILLING-NO-DUP preserved.
    3. **Hash chain tampering** (admin INSERT bypass): WI-S10-004 reconciliation detects via head verification; SEV-1.
    4. **Late-arriving event 7h+**: routed to usage_counter_late; SEV-3 alert; Finance triage trigger.
    5. **Schema version v1.1.0 event** (incompatible major): SchemaVersionMismatch error; SEV-2; aggregation halts for that event but continues for compatible.
    6. **R2 hour bucket missing** (Object Lock retention violation attempt): aggregator detects via WI-S10-001 staging completeness check; SEV-1.
    7. **Aggregator overrun > 1h** (event volume spike 10× baseline): DO continues; next cron fires; watermark prevents double-aggregate; SLO-FRESH-BILLING SEV-2 if > 15min.
    8. **DO state corruption** (rare CF infrastructure event): genesis hash recomputed for current month; partial chain re-derived from R2 raw; SEV-1.
    9. **Cross-region tenant migration mid-hour**: tenant.primary_region change between cron fires; events split across DO instances; reconciliation Layer 1 catches via tenant SUM check.
    10. **Counter UPSERT digest mismatch** (replay corruption): CounterDigestMismatch error; fail-CLOSED; SEV-1 manual investigation.
    11. **TLA+ no-loss/no-dup verification** (WI-S10-007 concern): billing_atomicity.tla model checked em CI.

### 6.2 Out-of-scope (deferred)

- Stripe integration (delegate WI-S10-003).
- Reconciliation worker 3-layer (delegate WI-S10-004 — counter is producer, not consumer).
- Quota state machine (delegate WI-S10-005).
- Replay forensic endpoint (delegate WI-S10-006).
- TLA+ billing_atomicity (delegate WI-S10-007).
- Customer-facing usage dashboard data (delegate S-13).
- Multi-currency conversion (anti-scope sprint contract §10).
- Real-time per-second aggregation (anti-scope §10 — hourly canonical).

## 7. Anti-Scope

- ❌ serde_json::Value em counter records (Lote 10.9-quinquies NEW-P0-2; typed canonical).
- ❌ Fail-OPEN at counter aggregation (would cause Layer 1 reconciliation drift; fail-CLOSED canonical Lote 10.6bis).
- ❌ Single global hash chain (per-region canonical for isolation).
- ❌ ad-hoc `now() + Duration::days(30)` (use canonical chrono primitives Lote 10.8bis P0-D).
- ❌ Mixing late events into usage_counter (use usage_counter_late separate; sprint contract §5.2 R-S10-5).
- ❌ Re-aggregating with DELETE+INSERT on usage_counter (UPSERT only; idempotent semantics).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ TenantCtx re-authentication (already authenticated upstream WI-S10-001).
- ❌ Skipping schema version validation (sprint contract §5.1 R-S10-3 mandatory).
- ❌ Per-feature SKU expansion beyond 5 canonical (sprint contract §10).

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: Counter Aggregator Cron DO + Hash Chain + Late-Arrival Split

  Scenario: Hourly aggregation produces counter row + hash chain advance
    Given hour bucket 2026-09-01T03:00:00Z em region iad
    Given 1000 events em WI-S10-001 staging table for tenant T (sku cas_put_op_count)
    When BillingAggregatorCron-iad cron-trigger fires
    Then aggregator drains staging + replays R2 hour bucket
    Then deduplicates by (tenant_id, request_id) UNIQUE
    Then INSERT INTO usage_counter (tenant_id=T, sku=cas_put_op_count, hour=epoch(2026-09-01T03), qty_ops=1000, ...)
    Then UPDATE hash_chain_head (region=iad, chain_kind=usage_counter, current_head=BLAKE3(...), last_aggregated_hour=epoch(2026-09-01T03))
    Then both writes em D1 transaction atomic

  Scenario: Aggregation idempotent on cron retry
    Given cron fires for hour bucket 2026-09-01T03:00:00Z; counter row inserted with digest D1
    When CF Workers retry fires same hour bucket within 5min
    Then aggregator detects PRIMARY KEY (tenant_id, region, sku, hour) conflict
    Then re-computes digest from same input events (deterministic; identical events → identical digest)
    Then ON CONFLICT DO UPDATE overwrites com same digest (idempotent re-aggregation)
    Then NO duplicate counter row
    Then INV-BILLING-NO-DUP preserved

  Scenario: Late-arriving event routed to usage_counter_late
    Given event with ts=2026-09-01T03:00:00Z arrives at 2026-09-01T10:30:00Z (7.5h late)
    When BillingAggregatorCron-iad processes event during 10:00 cron
    Then event filtered as late (ts < cron_now - 6h; cron_now = invocation timestamp fixed at start of aggregation run; canonical per R5 P1-QUIN-1)
    Then INSERT INTO usage_counter_late (tenant_id=T, sku=..., original_event_ts=epoch(03:00), aggregated_at=epoch(10:30), age_hours=7, ...)
    Then NOT inserted into usage_counter
    Then SEV-3 alert: corelink_billing_counter_late_event_total{age_bucket=6-12h} increments
    Then PagerDuty corelink-finance service paged

  Scenario: Hash chain integrity preserved across hours
    Given last hash chain head for region iad usage_counter = H_n
    When BillingAggregatorCron-iad aggregates hour bucket 2026-09-01T04:00:00Z
    Then prev_hash em new counter records = H_n
    Then own_digest = BLAKE3(canonical_json(record sans own_digest) || H_n)
    Then UPDATE hash_chain_head SET current_head = H_n+1
    Then chain auditable via WI-S10-004 daily reconciliation

  Scenario: D1 transaction failure halts aggregation fail-CLOSED
    Given D1 transient unavailability during aggregator execution
    When write_counter_atomic() invoked
    Then D1TransactionFailed error returned
    Then NEITHER usage_counter NOR hash_chain_head updated (atomic rollback)
    Then SEV-1 alert: corelink_billing_counter_d1_transaction_failures_total increments
    Then aggregator halts subsequent hours até resolution
    Then manual replay via replay_from_watermark() after recovery

  Scenario: Schema version mismatch rejected
    Given event with event_schema_version="2.0.0" em hour bucket
    When aggregator processes event
    Then SchemaVersionMismatch error
    Then event NOT included em counter aggregate
    Then SEV-2 alert: corelink_billing_counter_schema_version_mismatch_total{received_version=2.0.0} increments
    Then Compliance Officer notified for schema migration review

  Scenario: 2-version backward compatibility v1.0.0 + v1.0.1
    Given hour bucket has 500 events v1.0.0 + 500 events v1.0.1 (additive new_metadata field)
    When aggregator v1.0.x processes
    Then both versions processed; new_metadata ignored gracefully
    Then counter row reflects 1000 total events
    Then INV-BILLING-NO-LOSS preserved

  Scenario: Watermark prevents double-aggregate cross-cron
    Given aggregator successfully processed hour 03:00 (last_aggregated_hour = epoch(03))
    When cron fires at 04:00 with replay_from_watermark
    Then aggregator queries hash_chain_head; sees last_aggregated_hour = epoch(03)
    Then proceeds to hour 04:00 (epoch(04))
    Then 03:00 NOT re-aggregated
    Then cron-fires-2x case differs (PRIMARY KEY UPSERT idempotent vs watermark skip)

  Scenario: 5 SKUs canonical enforcement
    Given developer attempts INSERT counter row with sku="cas_storage_tb_year"
    When CHECK constraint validates
    Then INSERT rejected (CHECK violation)
    Then 5 SKUs canonical preserved (sprint contract §10 anti-scope)

  Scenario: DO routing per tenant.primary_region (Lote 10.7bis P0-9)
    Given tenant T has primary_region=fra
    When events for tenant T emitted from edge worker (any region)
    Then events routed to BillingAggregatorCron-fra DO (NOT iad/nrt/syd/gru)
    Then counter rows scoped per (region=fra, tenant=T)
    Then cross-region race eliminated

  Scenario: Hash chain tamper detection (WI-S10-004 cooperation)
    Given admin INSERT directly to usage_counter bypassing hash chain advance
    When WI-S10-004 daily reconciliation runs hash chain re-verification
    Then expected head (re-computed from chain) ≠ stored head
    Then HashChainViolation detected
    Then SEV-1 alert: corelink_billing_counter_hash_chain_violations_total
    Then CRITICAL post-mortem; CTRL-BILLING-001 violation audit
```

## 9. Design Decisions

- 9.1: Cloudflare Durable Object cron-triggered (vs standalone Worker — DO state for hash chain head canonical + tenant.primary_region routing per Lote 10.7bis P0-9).
- 9.2: Per-region DO instances (5 canonical: iad, fra, nrt, syd, gru) — eliminates cross-region race; isolation discipline.
- 9.3: Hash chain BLAKE3-256 per-region (NOT global; isolation discipline; inheritance WI-S09-004).
- 9.4: Atomic D1 transaction: usage_counter INSERT + hash_chain_head UPDATE em mesma transaction (CRITICAL fail-CLOSED).
- 9.5: Watermark-based replay (last_aggregated_hour em DO state + persistent em D1 hash_chain_head).
- 9.6: PRIMARY KEY (tenant_id, region, sku, hour) UNIQUE → UPSERT idempotent on cron retry.
- 9.7: usage_counter_late SEPARATE table (NOT mixed; sprint contract §5.2 R-S10-5).
- 9.8: 6h late threshold canonical (sprint contract §5.2 R-S10-5).
- 9.9: Fail-CLOSED at counter aggregation (vs WI-S10-001 hot path fail-OPEN; Lote 10.6bis split-tier canonical).
- 9.10: Typed CounterRecord + LateCounterRecord (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance).
- 9.11: Schema version validation strict; SchemaVersionMismatch fail-CLOSED.
- 9.12: 2-version backward-compat policy (v1.0.0 + v1.0.1).
- 9.13: corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D inheritance).
- 9.14: 5 SKUs canonical enum closed (sprint contract §10 anti-scope; CHECK constraint enforces).
- 9.15: HourWindow::from_aligned() enforces UTC hour-alignment (rejects non-aligned input).
- 9.16: CF Workers Rust API DO state + worker::send_future (Lote 10.7bis R5 P0-3); NEVER tokio::spawn.
- 9.17: NEW INVs INV-BILLING-NO-LOSS Layer 1 + INV-BILLING-NO-DUP em invariant_registry — verify canonical position before commit (Lote 10.8bis P1-13).

## 10. Completeness Criteria SOTA

- [ ] **10.s10.002.1** Crate compila + integration tests green.
- [ ] **10.s10.002.2** All 11 Gherkin scenarios green.
- [ ] **10.s10.002.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s10.002.4** Chaos suite 11 scenarios green.
- [ ] **10.s10.002.5** **INV-BILLING-NO-LOSS Layer 1** chaos test 30d sustained zero drift > 0.1% (sprint contract §6 DoD).
- [ ] **10.s10.002.6** **INV-BILLING-NO-DUP** property test 100k retries zero duplicate counter rows.
- [ ] **10.s10.002.7** D1 schemas (usage_counter + usage_counter_late + hash_chain_head) migrations applied.
- [ ] **10.s10.002.8** Hash chain integrity verifiable via WI-S10-004 daily reconciliation.
- [ ] **10.s10.002.9** SLO-FRESH-BILLING ≤ 15min event-to-counter p99 sustained.
- [ ] **10.s10.002.10** Schema version v1.0.0 + 2-version backward-compat tested.
- [ ] **10.s10.002.11** Late-arriving 6h policy enforced; usage_counter_late routing tested.
- [ ] **10.s10.002.12** corelink_time::next_month_first_utc_midnight() canonical primitive used (Lote 10.8bis P0-D).
- [ ] **10.s10.002.13** Métricas (9) emitted via WI-S09-001 emit lib; cardinality budget respected (~250 séries baseline).
- [ ] **10.s10.002.14** Cargo-audit + cargo-deny + clippy clean.

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Finance + Legal + Privacy emphatic).

## 12. Invariants Validated

- **INV-BILLING-NO-LOSS Layer 1** (HIGH; registry §3.9 line 136; verify via grep before commit per Lote 10.8bis P1-13): Σ(R2 events) = Σ(usage_counter qty) + Σ(usage_counter_late qty) per (tenant, region, sku, hour); chaos test 30d zero drift > 0.1%.
- **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137): PRIMARY KEY (tenant_id, region, sku, hour) UNIQUE; UPSERT idempotent; property test 100k retries zero duplicates.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance): D1 INSERT-only counter rows; UPDATE rejected outside UPSERT idempotent semantics.
- **INV-BILLING-RECONCILE-3-LAYER** (HIGH; sprint contract §8 NEW): Layer 1 producer; reconciliation worker WI-S10-004 consumer.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-region DO routing; cross-region race eliminated.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): tenant_id em events authenticated upstream WI-S10-001; aggregator preserves.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Counter aggregator module | `crates/corelink-billing-counter/` | Rust |
| Durable Object impl | `crates/corelink-billing-counter/src/aggregator_do.rs` | Rust |
| D1 schema migrations | `migrations/00X_usage_counter.sql`, `00Y_usage_counter_late.sql`, `00Z_hash_chain_head.sql` | SQL |
| Cron-trigger config | `infra/cloudflare/workers/billing_aggregator_cron.toml` | Wrangler |
| DO routing config | `infra/cloudflare/durable_objects/billing_aggregator.tf` | Terraform |
| Property tests | `crates/corelink-billing-counter/tests/prop_aggregator.rs` | Rust |
| Chaos suite | `tests/chaos_billing_counter.rs` | Rust |
| Hash chain helpers | `crates/corelink-billing-counter/src/hash_chain.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s10.002.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s10.002.2: rustdoc 100% public API.
- 14.s10.002.3: Test coverage ≥ 90%.
- 14.s10.002.4: Aggregation latency ≤ 5min p99 per hour bucket.
- 14.s10.002.5: SAST clean; sqlfluff strict for D1 migrations.
- 14.s10.002.6: Métricas (9 §6.1.17).
- 14.s10.002.7: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s10.002.8: TenantCtx propagation (Lote 10.4bis); CF Workers Rust API DO state + worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical Plan ref (Lote 10.7bis P0-7); DO routing per tenant.primary_region (Lote 10.7bis P0-9); column drift no `_ms` suffix (Lote 10.7bis P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV positions §3.9 L136/137 (NO-LOSS/NO-DUP) + §3.12 L166/167 (RECONCILE-3-LAYER/REPLAYABLE) verified Lote 10.10bis (Lote 10.8bis P1-13 lesson absorbed).
- 14.s10.002.9: corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D); HourWindow::from_aligned() rejects non-aligned input.
- 14.s10.002.10: D1 batch ≤ 250 (Lote 10.5bis); CHECK constraints inline (Lote 10.5bis).
- 14.s10.002.11: Counter aggregation fail-CLOSED canonical (Lote 10.6bis split-tier; vs WI-S10-001 hot path fail-OPEN).
- 14.s10.002.12: BLAKE3-256 hash chain consistency com WI-S09-004 audit chain pattern + WI-S10-001 idempotency staging.
- 14.s10.002.13: Typed CounterRecord (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance).
- 14.s10.002.14: Atomic D1 transaction (counter + hash_chain_head em mesma transaction).
- 14.s10.002.15: Prom metric names underscored canonical (Lote 10.9bis P0-E inheritance).
- 14.s10.002.16: `corelink_time::next_month_first_utc_midnight()` é canonical helper interno em `crates/corelink-time/src/canonical.rs` (R4 round-1 P1-9 / round-2 P1-9 fix); signature `pub fn next_month_first_utc_midnight() -> chrono::DateTime<chrono::Utc>`; UTC-anchored via `Utc::now().date_naive().with_day(1).unwrap().checked_add_months(Months::new(1)).unwrap().and_hms_opt(0,0,0).unwrap().and_utc()`; property tests `prop_idempotent_across_dst` + `prop_handles_leap_year`.

## 15. Chaos Experiments (11)

§6.1.19 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Finance + Legal + Privacy emphatic).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + CounterAggregator trait + CounterRecord/LateCounterRecord typed structs | 1.5 |
| ST-002 | DO impl + cron-trigger config + 5 region instances | 2 |
| ST-003 | D1 schema migrations (usage_counter + usage_counter_late + hash_chain_head) | 1.5 |
| ST-004 | Aggregation logic (drain staging + R2 replay + dedup + SUM aggregation) | 2.5 |
| ST-005 | Hash chain BLAKE3-256 helpers + canonical_json + atomic D1 transaction | 2 |
| ST-006 | Late-arriving event detection + usage_counter_late routing | 1 |
| ST-007 | Watermark-based idempotent replay logic | 1.5 |
| ST-008 | Schema version validation + 2-version backward-compat | 1 |
| ST-009 | Métricas (9) emit | 1 |
| ST-010 | Property tests (6 × 10k; 100k nightly) | 2.5 |
| ST-011 | Chaos suite (11) | 2 |
| ST-012 | Compliance review + Finance walkthrough hash chain integrity | 0.7 |

**Total**: ~19.2h. **PERT** O=12h M=18h P=28h: **18.7h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S10-001 SEALED (staging table + R2 events feed); WI-S09-001 SEALED (cardinality emit lib + métricas underscores); WI-S09-004 SEALED (hash chain pattern + R2 Object Lock); S-03 SEALED (TenantCtx + tenant.primary_region routing).
- Soft: WI-S10-004 (reconciliation Layer 1 consumer); WI-S10-007 (TLA+ billing_atomicity).
- Hard infra: Cloudflare Durable Objects available per region; D1 per region; cron-trigger Workers config.

## 19. Effort PERT: 18.7h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

9 metrics §6.1.17. Trace span `billing.counter.{drain_staging, replay_r2, dedup, aggregate, hash_chain_advance, write_atomic, route_late}`.

## 22. Cost Analysis

- Cloudflare Durable Objects: $0.15/1M requests + $12.50/GB-mo storage + duration. 5 regions × 720 hourly cron invocations/mo × ~5s each = ~5 GB-s/region/mo = negligible.
- D1 storage: usage_counter ~720MB/mo/region × 5 regions × 12mo retention = ~43 GB; $0.75/GB-mo × 43 = ~$32/mo = ~$385/yr.
- D1 reads: ~10M reads/mo (aggregation) × 5 regions × 12mo = 600M; $0.001/1k = ~$600/yr.
- D1 writes: ~3.6M writes/mo × 5 regions × 12mo = 216M; $1.00/1M writes = ~$216/yr.
- TCO 12m: ~$1200/yr counter aggregation infrastructure (well within budget).
- **Cost saved by INV-BILLING-NO-LOSS Layer 1 enforcement**: prevents silent counter-layer revenue leak; 3-layer reconciliation enabled (WI-S10-004 cooperation).

## 23. API Contract

- Public Rust: `CounterAggregator` trait + `CounterRecord`, `LateCounterRecord`, `HourWindow`, `HashChainHead`, `CounterError` types; `#[non_exhaustive]`.
- Wire: D1 `usage_counter` + `usage_counter_late` + `hash_chain_head` schemas (migrations canonical).
- Storage: D1 per region; Durable Object state for hash chain head + watermark.
- Cron-trigger: `0 * * * *` per region (CF Workers cron config).

## 24. Post-mortem Hooks

- INV-BILLING-NO-LOSS Layer 1 violation detected (Σ(events) ≠ Σ(counter)) → HIGH-severity post-mortem (revenue leak).
- INV-BILLING-NO-DUP violation (duplicate counter row) → HIGH-severity (double-charge customer downstream).
- Hash chain violation detected (HashChainViolation error) → CRITICAL post-mortem; CTRL-BILLING-001 audit.
- D1 transaction failure > 1/day → SEV-2 + capacity review.
- Late-arriving events > 5% mensal → post-mortem (system delay root-cause).
- SLO-FRESH-BILLING > 15min sustained 30min → SEV-2; aggregation backpressure investigation.
- Schema version mismatch sustained (events not processable) → SEV-2 + Compliance review.

## 25. Rollback / Recovery

- Rollback: revert DO + cron-trigger config; events accumulate em WI-S10-001 staging untouched; INV-BILLING-NO-LOSS preserved (events retained); manual aggregation possible from R2 raw via WI-S10-006 replay.
- Recovery: DO re-deployed; watermark-based replay catches up missed hours; reconciliation Layer 1 (WI-S10-004) verifies catch-up correctness.
- RTO ≤ 30min (DO cold-start + cron resume); RPO ≤ 0min (events retained em R2 + staging).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx propagation upstream WI-S10-001; aggregator preserves authenticated tenant_id.
- T(ampering): Hash chain BLAKE3-256 per-region detects post-hoc modification; WI-S10-004 daily verify.
- R(epudiation): Counter rows append-only via UPSERT idempotent; chain head auditable.
- I(nformation disclosure): Counter aggregates have no per-event PII (digest_truncated em events redacted upstream); counter row contains only (tenant_id, sku, qty) — no PII.
- D(enial of Service): Fail-CLOSED at aggregation halts on D1 failure; events retained upstream; recovery via watermark replay.
- E(scalation of Privilege): D1 access scoped per region; admin INSERT bypass detected via hash chain mismatch.

**LINDDUN** (LGPD/GDPR):
- L(inkability): Counter rows linkable per tenant_id; expected (financial-grade per-tenant billing).
- I(dentifiability): Tenant_id pseudonymous (UUID); no direct PII em counter table.
- N(on-repudiation): Hash chain immutable evidence trail.
- D(etectability): Customer access via WI-S10-006 replay endpoint.
- D(isclosure): Counter retention 12mo (vs R2 events 7y); customer DSR erasure (S-11) → tenant_id pseudonymized via S-11 procedure; aggregate counters retained but unlinkable.
- U(nawareness): Customer notified via S-13 admin plane.
- N(on-compliance): **CTRL-BILLING-001 + SOC 2 CC1.4 + GAAP ASC 606 + LGPD Art. 32 + GDPR Art. 32** via append-only counter + hash chain + 3-layer reconciliation.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-10 Counter Aggregator: Cron DO + Hash Chain + Late-Arrival Split"; doc `docs/dev/billing-counter-architecture.md`; onboarding test 8 questions: hash chain BLAKE3-256 per-region (NOT global), atomic D1 transaction (counter + chain head), watermark-based replay idempotency, fail-CLOSED at aggregation vs hot path fail-OPEN (Lote 10.6bis split-tier), late-arriving 6h threshold + usage_counter_late, DO routing per tenant.primary_region (Lote 10.7bis P0-9), schema versioning 2-version backward-compat, corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D).

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-BILLING-NO-LOSS Layer 1 violation (counter drift) | L | M | HIGH | M | LOW | 3-layer reconciliation WI-S10-004 + chaos 30d + SEV-2 alert + Finance review (HIGH severity → SEV-2 canonical per registry §2; > 1% drift escalates to SEV-1) |
| R-002 | INV-BILLING-NO-DUP violation (duplicate counter row) | L | L | HIGH | L | LOW | PRIMARY KEY UNIQUE + UPSERT idempotent + property 100k |
| R-003 | Hash chain tampering admin INSERT bypass | L | M | CRITICAL | M | LOW | Daily reconciliation WI-S10-004 detects head mismatch; SEV-1 |
| R-004 | D1 transaction failure mid-aggregate | M | L | HIGH | M | LOW | Atomic transaction; rollback complete; manual replay watermark |
| R-005 | Aggregator overrun > 1h (event spike) | M | L | MEDIUM | L | LOW | Watermark-based catch-up; SLO-FRESH-BILLING SEV-2 if > 15min |
| R-006 | Cron fires 2× same hour (CF Workers retry) | M | L | LOW | L | LOW | UPSERT idempotent (PRIMARY KEY UNIQUE); property test |
| R-007 | Late-arriving events > 5% mensal | L | M | HIGH | M | LOW | usage_counter_late split + SEV-3 alert + Finance triage |
| R-008 | Schema version v1.1.0 incompatible flood | L | L | MEDIUM | L | LOW | SchemaVersionMismatch fail-CLOSED + Compliance review trigger |
| R-009 | DO state corruption (rare CF infra) | L | L | HIGH | L | LOW | Genesis hash recomputed for current month; partial chain re-derived from R2 raw |
| R-010 | Cross-region tenant migration race | L | L | MEDIUM | L | LOW | Tenant.primary_region routing canonical Lote 10.7bis P0-9; reconciliation Layer 1 catches |
| R-011 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.9 (NO-LOSS L136 / NO-DUP L137) + §3.12 (RECONCILE-3-LAYER L166 / REPLAYABLE L167) verified Lote 10.10bis; ongoing maintenance discipline via grep CI gate |
| R-012 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; DO state + worker::send_future |

## 29. Review Checkpoints

D+0 design (Architect; fail-CLOSED split-tier + DO routing); D+1 Finance (counter integrity discipline + 3-layer reconciliation prep); D+2 AppSec (hash chain integrity + atomic transaction); D+3 Privacy (LINDDUN + counter aggregate non-PII); D+4 Compliance (schema versioning + audit trail + GAAP cutoff); D+5 SRE (DO routing + cron-trigger + watermark recovery); D+6 code review; D+7 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — CTRL-BILLING-001 + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 30d + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC1.4 + GAAP ASC 606 + schema versioning + audit chain_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + counter non-PII + DSR cooperation_ |
| 11 | Architect | _TBD; **mandatory emphatic** — fail-CLOSED split-tier (Lote 10.6bis) + DO routing (Lote 10.7bis P0-9) + chrono primitives canonical (Lote 10.8bis P0-D) + INV §3.X verification (Lote 10.8bis P1-13)_ |
| 12 | Finance | _TBD; **mandatory emphatic** — counter integrity foundation + 3-layer reconciliation Layer 1 producer_ |

(Legal sign-off via DPA reference at sprint level; not per-WI.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.10) | Criação WI-S10-002; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris + Lote 10.9bis/tris/quaters/quinquies lessons absorbed: 5-tier canonical Plan (Lote 10.7bis P0-7); DO routing per tenant.primary_region (Lote 10.7bis P0-9); CF Workers Rust API DO state + worker::send_future (R5 P0-3); 100k nightly property test (Lote 10.7bis P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X canonical position TBD verification before commit (Lote 10.8bis P1-13); corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D); calibration n=50+50 + 95% CI bounded (Lote 10.8bis P0-E); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis). **Lote 10.9-quinquies NEW-P0-2 critical absorption**: typed CounterRecord + LateCounterRecord (NOT serde_json::Value); inheritance from WI-S09-002 wrapper Serialize impls if PII fields. **Lote 10.6bis split-tier lesson**: fail-CLOSED at counter aggregation canonical (vs WI-S10-001 hot path fail-OPEN). **Lote 10.9bis P0-E inheritance**: Prom metric names underscored canonical. NEW corelink-billing-counter crate + Cloudflare DO BillingAggregatorCron-<region> per-region cron-trigger 1h + hash chain BLAKE3-256 per-region (inheritance WI-S09-004 audit pattern) + atomic D1 transaction (counter + hash_chain_head) + usage_counter_late separate table for events ts < now-6h + watermark-based idempotent replay. INV-BILLING-NO-LOSS Layer 1 + INV-BILLING-NO-DUP CRITICAL invariants enforcement layer (registry position TBD). 5 regions canonical (iad, fra, nrt, syd, gru). 5 SKUs canonical inheritance from WI-S10-001 (sprint contract §10 anti-scope). Schema versioning 2-version backward-compat. Late-arriving events 6h policy. CTRL-BILLING-001 + SOC 2 CC1.4 + GAAP ASC 606 compliance counter-layer. |
| 1.1.0 | 2026-04-26 | Gustavo (Lote 10.10bis) | P0+P1 remediation pós-R4 Opus + Sonnet R5 adversarial reviews. **8 P0s fixed**: (1) PlanTier 5-tuple inventado `free/team/enterprise/custom/trial` → canonical `free/solo/team/business/enterprise` per `data_model.md §1` line 68 — corrige CHECK constraints em WIs 003+005 + sprint contract §5.3 R-S10-7; (2) INV registry positions estampadas `§3.X TBD` → reais §3.9 line 136 (NO-LOSS HIGH), §3.9 line 137 (NO-DUP HIGH), §3.12 line 166 (RECONCILE-3-LAYER HIGH), §3.12 line 167 (REPLAYABLE HIGH); severity drift CRITICAL→HIGH cascateia em SEV-2 alert wording (vs SEV-1); Lote 10.8bis P1-13 lesson finalmente absorvida; (3) WI-007 TLA+ math 5× errado (15k claim → real 3,000; 150k → real 30,000) + caveat sobre TLC reachable state graph ≠ Cartesian product CONSTANTS; MaxConcurrentEvents bound NEW; (4) CTRL-AUTHZ-005 alucinado (security_model só define -001/-002) → CTRL-AUTHZ-001 + CTRL-AUTHZ-002 explicit; sprint contract §5.6 R-S10-13 também atualizado; (5) WI-002 UPSERT WHERE clause auto-anulante removido (`WHERE own_digest = excluded.own_digest` rejeitava updates legítimos); (6) Cloudflare R2 Terraform schema marcada com TODO pre-merge verification (era hallucinated AWS S3 pattern `cloudflare_r2_bucket_lock_configuration`); (7) **R5 P0-A**: WI-002 PRIMARY KEY `(tenant_id, region, sku, hour)` → `(tenant_id, region, sku, hour)` — multi-region tenants antes corrompiam dados a cada UPSERT; (8) **R5 P0-B**: WI-001 D1 CHECK enum `('cas_put',...)` rejeitava 100% dos inserts (strum serializa para long form `dev.hugr.corelink.cas.put.v1`); aligned com Lote 10.9bis P0-G CloudEvents prefix canonical. **P1s fixed**: (a) staging table region column added (R5 P1-A); (b) WI-003 narrative 62-char idempotency key contradiziando §6.1 35-char (R5 P1-B); (c) idempotency comment WI-001 (P1-C); (d) `CHECK (age_hours > 6)` off-by-one → `>= 6` (R5 P1-D); (e) cardinality 30 regions → 5 canonical (P1-G); (f) PercentValue type/DB CHECK alignment 0..=200 (R5 P1-H); (g) chrono::next_month_first_utc_midnight() (não existe em chrono crate) → corelink_time::next_month_first_utc_midnight() canonical helper internal; (h) CTRL-PRIV-002 mis-mapped to "pseudonymization" → privacy_model.md L209 "data classification tags" + S-11 cooperation para DSR pseudonymization separada; (i) sprint contract §5.7 R-S10-15 metric names dots → underscores (Prom canonical); (j) §5.1 R-S10-1 CloudEvents prefix `corelink.usage.*` → `dev.hugr.corelink.<op>.v1`; (k) WI-005 4-state vs 5-state — GraceActive removido como state, virou flag boolean preserving sprint contract §5.5 R-S10-10 canonical; (l) failure_modes.md adicionado RB-FM-151 mapping (P1-5). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.10-quaters) | P0+P1 remediation pós-R4 + R5 round-2 audits (R4 6.9/10, R5 7.8/10). Same S-08/S-09 cascade-incomplete pattern: bis ~60% complete; quaters fecha residuais. **8 P0s addressed**: (1) NEW-P0-1 spec contract §8 invariants L149-151 CRITICAL→HIGH (era source-of-truth não atualizada; severity contradicting WIs); (2) NEW-P0-2 WI-002 PK 4-tuple cascade ~30%→100% — `CounterRecord` + `LateCounterRecord` Rust structs add `region: Region` field (digest input para tamper detection per-region); 15 narrative/Gherkin/design-decision references swept (tenant_id, sku, hour) → (tenant_id, region, sku, hour); (3) NEW-P0-3 TLA+ `MaxConcurrentEvents` CONSTANT NEW added to body L58-65 + bounded em EmitEvent; `Hours` time abstraction comment clarified (R4 round-1 caveat absorbed); `quota_grace_active` separate VARIABLE (R5 NEW-P0-1 fix: grace é flag, não 5º state value); `ReconcileLayer1` action NEW with drift computation (R4 round-1 P2-10 fix); (4) WI-003 PlanTier residuals §1 item 11 L247-250 + §6.1.13 L548 swept canonical 5-tuple; (5) WI-002 Gherkin L530 UPSERT WHERE clause text fixed (R4 P0-5 partial); (6) WI-001 risk register R-001/R-002 + post-mortem hooks severity CRITICAL→HIGH; SEV-1 → SEV-2 (R4 NEW-P1-2). **P1s addressed**: (a) WI-005 PercentValue cluster L74/L178/L328 align com 0..=200 DB CHECK (R4 NEW-P1-1 + R5 NEW-P1-3); (b) WI-006 sign-off `Finance + Legal + Architect` → `Finance + Compliance Officer + Architect` (Legal sprint-level only; R4 NEW-P1-4 / round-1 P1-6 finalmente fix); (c) WI-003 título L29 + WI-005 L727 + spec contract L244 CTRL-PRIV-002 mis-mapping → "data classification tags @classification=pii" + S-11 separated DSR; (d) WI-002 §6.1.11 late event reference time → cron_now (R5 NEW-P1-1); (e) WI-002 §14 corelink_time crate path canonical reference added (`crates/corelink-time/src/canonical.rs`); (f) WI-S10-003 Idempotency-Key 64-char internal cap rationale ADR documented em line 367; (g) WI-S10-007 ADR-S10-007-tla-substitutes-proptest justifying property test exemption + lightweight sanity props NEW; (h) WI-S10-006 "Mfa" → "MFA" capitalization (30 occurrences); (i) WI-004 Layer 2/3 narrative aggregation semantics clarified (collapsed across regions for Stripe scope; per-region drift preservada via Layer 1). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.3.0 | 2026-04-26 | Gustavo (Lote 10.10-sextus) | P0+P1 remediation pós-R4 + R5 round-3 audits (R4 8.0/10, R5 9.1/10 CONDITIONAL SEAL). 2 P0s + 8 P1s residuais addressed. **2 P0s**: (1) R4 NEW-P0-1 severity cascade incompleto — quaters só varreu WI-001; sextus completou WI-002 (L745-746 post-mortem + L786 R-001) + WI-003 (L869-870) + WI-007 (L743 + L785 R-005) — todos `INV-BILLING-NO-LOSS/NO-DUP` violation references → HIGH-severity post-mortem + SEV-2 alert (não SEV-1). (2) R4 NEW-P0-2 WI-S10-004 Layer 1 ainda 3-tuple `(tenant, sku, hour)` → 4-tuple `(tenant, region, sku, hour)` em título L41 + trait doc L67 + narrative L217 + Gherkin L521 + sprint contract §5.2 R-S10-4 line 86 + CAP-BILLING-002 line 65 — mascarava multi-region drift via cross-region offsets (financial-grade integrity restored). **8 P1s**: (a) R5 P1-QUIN-1 WI-002 L538 + step 5 Gherkin/narrative `hour_window.start_ts` → `cron_now`; (b) R5 P1-QUIN-2 WI-001 L210 §1 item 9 idempotency key long-form → 35-char canonical; (c) R4 P1-1 WI-003 título L41 + ST-010 sub-task CTRL-PRIV-002 pseudonymization → "data classification tags @classification=pii" + S-11 separated DSR; (d) R4 P1-2 WI-006 §29 D+3 review checkpoint Legal sign-off → sprint-level only (NÃO 3-of-3 per-replay; matched §6.1.6 fix); (e) R4 P1-3 spec contract metadata bumped v1.1.0/sota-v1.1/2026-04-24 → v1.3.0/sota-v1.3/2026-04-26; (f) R4 P1-4 + R5 P1-QUIN-3 TLA+ block: `BillingPeriods` + `RequestIds` agora declared CONSTANTS; helper operators (`Abs`, `SumCounters`, `SumLineItems`, `ComputeStripeInvoiceId`, `StripeIdParts`, `EventAccountedInInvoice`) documented as separate INSTANCE module; (g) R5 P1-QUIN-4 INV_BILLING_NO_LOSS retry_queue Sequence membership bug fixed via `RetrySet == {retry_queue[i] : i \in DOMAIN retry_queue}` Range conversion (semantic correctness); (h) R4 P1-5 `compute_counter_digest()` doc explicitly confirms region IS in canonical_json input (cross-region tamper detection guaranteed); (i) R4 P1-6 §14.s10.x.8 + R-010/R-011 risk register stale "INV §3.X TBD pre-merge" → concrete §3.9 L136/137 + §3.12 L166/167 verified Lote 10.10bis (residual após mitigação LOW). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |

## 32. Anti-patterns evitados

- ❌ serde_json::Value em counter records (Lote 10.9-quinquies NEW-P0-2; typed canonical); ❌ Fail-OPEN at counter aggregation (Layer 1 reconciliation drift; fail-CLOSED canonical Lote 10.6bis); ❌ Single global hash chain (per-region canonical for isolation); ❌ ad-hoc `now() + Duration::days(30)` (Lote 10.8bis P0-D canonical primitives); ❌ Mixing late events into usage_counter (separate table sprint contract §5.2 R-S10-5); ❌ DELETE+INSERT on usage_counter (UPSERT idempotent only); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ TenantCtx re-authentication (already authenticated upstream); ❌ Schema version validation skip; ❌ Per-feature SKU expansion beyond 5 canonical (sprint contract §10); ❌ Non-atomic counter + chain head writes (CRITICAL atomic D1 transaction).

---

**Fim WI-S10-002.** Próximo: WI-S10-003 (Crate corelink-billing Stripe adapter + idempotency-key + webhook handler).
