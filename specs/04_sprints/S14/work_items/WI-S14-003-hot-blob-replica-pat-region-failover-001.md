---
id: "WI-S14-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-003"]
parent: "S-14"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "STORAGE-SEMANTICS-MATRIX"
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
  - "PRIVACY-MODEL"
tags: ["wi", "s14", "region", "failover", "replica", "hot-blob", "offline-aggregation", "cardinality-budget", "high-risk"]
---

# WI-S14-003 — Hot Blob Replica Worker (PAT-REGION-FAILOVER-001) com **Offline Batch Aggregation** sobre S-09 Audit Log R2 (Top 1% per Tenant — **NÃO** via Métrica Labeled por `tenant_id` Proibido por INV-OBS-CARDINALITY-BUDGET; Live Métrica é `corelink_cas_get_bytes_total{tenant_tier, region}` Tier-Labeled NÃO Tenant-Labeled per Lote 9.4 Opus H-02 Fix em Spec Contract R-S14-3) + Replication Worker Async + Read Failover Routing Transparent + Replication Lag p99 ≤ 60s SLO Sustained 7d + SLO-LAT-CAS-GET p99 < 300ms Preserved During Region Outage Chaos Test

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-003 |
| Título | Hot blob replica worker implementando PAT-REGION-FAILOVER-001 com **OFFLINE batch aggregation** sobre S-09 audit log R2 (daily job computa top 1% per tenant via batch query — escapa cardinality budget porque é offline NÃO live métrica labeled por tenant_id; live métrica é `corelink_cas_get_bytes_total{tenant_tier, region}` budget-safe tier-labeled per Lote 9.4 Opus H-02 fix); replication worker async copia top-1% para sibling region; replication lag p99 ≤ 60s SLO sustained 7d staging; read failover routing transparent durante region outage chaos test (SLO-LAT-CAS-GET p99 < 300ms preserved); failover overhead ≤ 50ms p99 |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cross-region replica preserva tenant isolation), FF-HR-003 (residency em failover scenarios) |

## 1. Intent

Hot blob replica + read failover via PAT-REGION-FAILOVER-001: top 1% blobs por tenant replicated cross-region async; read failover transparent quando primary region indisponível (5xx/504/timeout); SLO-LAT-CAS-GET p99 < 300ms preserved chaos test. **Decisão crítica de cardinality budget** (Lote 9.4 Opus H-02 fix em spec contract R-S14-3): top-1% per tenant detection é **OFFLINE batch aggregation** sobre S-09 audit log R2 daily job — **NÃO** uma live Prometheus métrica labeled por `tenant_id` (proibido por INV-OBS-CARDINALITY-BUDGET S-09; com 10k+ tenants × 100 metrics each = 1M+ séries únicas explode cardinality budget). Live live métrica is `corelink_cas_get_bytes_total{tenant_tier, region}` (tier label only — Solo/Team/Business/Enterprise = 4 values; region = 4 values; total cardinality 16 séries baseline; budget-safe). Offline aggregation: cron daily job query S-09 audit log R2 (audit events `corelink.cas.get.ok` contains `tenant_id + blob_hash + bytes`); Spark/DataFusion-style aggregation computa top-1% blob_hash per tenant; results stored em D1 `hot_blobs` table; replication worker reads D1 + copies blobs cross-region. Failover routing: middleware checks primary region health (5xx threshold + latency p99 SLO violated); if degraded → reads from sibling region replica; transparent ao customer; failover overhead ≤ 50ms p99.

```rust
// File: crates/corelink-replica-worker/src/lib.rs

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Region {
    Wnam,
    Enam,
    Weur,
    Sam,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotBlob {
    pub tenant_id: String,
    pub blob_hash: String,
    pub primary_region: Region,
    pub replica_region: Region,
    pub bytes: u64,
    pub access_count_30d: u64,
    pub last_access_ms: u64,
}

#[derive(Debug, Error)]
pub enum ReplicaError {
    #[error("offline aggregation failed: {0}")]
    Aggregation(String),
    #[error("R2 copy failed: {0}")]
    R2Copy(String),
    #[error("hash mismatch post-replica: expected={expected} actual={actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("residency violation: tenant.primary_region={primary:?} replica_region={replica:?} not allowed")]
    ResidencyViolation { primary: Region, replica: Region },
    #[error("D1 storage error: {0}")]
    Storage(String),
}

pub struct ReplicaWorker {
    // ...
}

impl ReplicaWorker {
    /// Daily offline aggregation job: query S-09 audit log R2 events
    /// `corelink.cas.get.ok` for last 30d window; compute top 1% blob_hash per tenant
    /// by bytes_total. Insert results em D1 `hot_blobs` table.
    /// CRITICAL: This is OFFLINE (cron job), NOT a live Prometheus métrica
    /// labeled por tenant_id (proibido por INV-OBS-CARDINALITY-BUDGET).
    pub async fn run_offline_aggregation(&self, window_days: u32) -> Result<u32, ReplicaError> {
        unimplemented!()
    }

    /// Replication worker: reads D1 `hot_blobs`; copies blob R2 → sibling region R2
    /// async; verifies hash post-copy (INV-CAS-INTEGRITY); respects residency
    /// (WEUR tenant cannot replicate to ENAM; SAM tenant cannot replicate to non-SAM).
    pub async fn replicate_hot_blobs(&self) -> Result<u32, ReplicaError> {
        unimplemented!()
    }
}

pub struct FailoverRouter {
    // ...
}

impl FailoverRouter {
    /// Read failover: if primary region 5xx/504/timeout sustained > N requests,
    /// degrade to sibling region replica reads. Transparent ao customer.
    /// Overhead ≤ 50ms p99.
    pub async fn route_read(
        &self,
        tenant_id: &str,
        blob_hash: &str,
        primary: Region,
    ) -> Result<Vec<u8>, ReplicaError> {
        unimplemented!()
    }
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Hot blob replica + read failover é critical pra SLO sustainability durante region outages: sem failover, region outage WEUR = WEUR tenants completamente offline (catastrophic). PAT-REGION-FAILOVER-001 mitigates via top 1% blobs (CAS GET dominant traffic) replicated cross-region; failover transparent preserves SLO-LAT-CAS-GET p99 < 300ms.

**Decisão crítica de cardinality budget** (Lote 9.4 Opus H-02 fix em spec contract R-S14-3 — repetido aqui em §5 detailed design):

Hot blob detection NÃO pode ser live Prometheus métrica labeled por `tenant_id`. Razão: `corelink_cas_get_bytes_total{tenant_id="T1", blob_hash="abc..."}` com 10k tenants × 100 blobs each = 1M+ séries únicas; INV-OBS-CARDINALITY-BUDGET (S-09) bound 20k séries por métrica + 100k total. Cardinality explode = Grafana Mimir tenant limit hit + alert tenant-limit-exceeded SEV-2 + métricas dropped silently.

**Solução**: top-1% per tenant detection é **OFFLINE** via:
- Cron daily 02:00 UTC job em Cloudflare Worker.
- Query S-09 audit log R2 (audit chain events `corelink.cas.get.ok` payload contains `{tenant_id, blob_hash, bytes, ts}`).
- DataFusion-style aggregation: GROUP BY tenant_id, blob_hash; SUM(bytes) over last 30d window.
- Compute top 1% per tenant by bytes_total.
- Insert results into D1 `hot_blobs` table (`tenant_id, blob_hash, primary_region, replica_region, bytes, access_count_30d, last_access_ms`).
- Replication worker reads D1 + copies blobs.

Live Prometheus métrica é `corelink_cas_get_bytes_total{tenant_tier, region}` (tier ∈ {solo, team, business, enterprise} = 4 values; region = 4 values; cardinality 16 séries baseline) — budget-safe.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Cardinality budget violation via tenant_id label**: dev adds tenant_id label to hot blob métrica = 1M+ séries explode. Mitigação: cardinality validator CI gate (S-09 herdada) detecta + bloqueia PR; explicit comment em métrica documentation `// DO NOT add tenant_id label — use offline aggregation`.

2. **Residency violation em replication**: WEUR tenant blob replicated para ENAM = Schrems II violation. Mitigação: replication worker checks `tenant.primary_region == primary_region`; replica_region restricted (e.g., WEUR → only SAM-EU sibling, not US-NAM); INV-REGION-NO-CROSS-LEAK enforced.

3. **Replication lag > 60s**: top-1% blobs not replicated em time; failover serves stale OR fails. Mitigação: lag p99 ≤ 60s SLO sustained 7d; alert SEV-2 if > 60s; replication worker scale-out.

4. **Hash mismatch post-replica (INV-CAS-INTEGRITY)**: replicated blob bytes corrupted; primary hash != replica hash. Mitigação: verify hash post-copy (R2 returns ETag = MD5; compare); mismatch = retry; persistent mismatch = alert SEV-2 + audit emit.

5. **Failover false-positive (slowness misdetected as outage)**: transient slowness triggers failover; cost regression. Mitigação: multi-signal detection (5xx rate > threshold + latency p99 > SLO + 3 consecutive failures); failover only after sustained signal (5s); runbook documents.

6. **Failover false-negative (real outage not detected)**: primary serves 200 OK with corrupted body. Mitigação: synthetic probe per region (5min cadence); body integrity check; multi-signal triangulation.

7. **Replication storm (top 1% becomes 30%)**: cardinality explodes; replication overhead grows. Mitigação: daily cardinality detector + budget alert; manual override possible (admin API).

8. **Failover routing loop**: WEUR fails over to SAM; SAM fails over to ENAM; ENAM fails over to WEUR (loop). Mitigação: failover graph acyclic per-region (WEUR → SAM only; SAM → WEUR only); enforced em static config.

9. **Stale data served from replica post-write**: tenant writes to primary; reads to replica during failover serve stale. Mitigação: failover read-only mode; writes blocked during failover; alert customer; SLA addendum documented.

**Atacante adversarial scenarios**:

- **Force replication storm**: attacker triggers synthetic high-traffic blobs to inflate top-1%. Mitigação: cardinality detector + manual override + rate limit per-tenant.

- **Tamper replicated blob**: attacker compromises sibling region R2; modifies replica. Mitigação: hash verify post-copy + audit emit + alert; INV-CAS-INTEGRITY enforced.

- **Trigger false failover**: attacker DDoSs primary region to force failover. Mitigação: WAF + rate limit; failover preserves SLO mas not unconditional service.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: cross-region replica preserves tenant isolation (replica_region restricted by residency).
- **FF-HR-003**: residency em failover scenarios (replica_region constraints).
- **Reversibility**: replication errors detected + reconciliation; SLO violations measurable + remediation.

11 sign-offs canonical incl. Architect (offline aggregation design + cardinality budget + failover routing) + SRE Lead (chaos test region outage + replication lag SLO + RB-FM-105) + Compliance Officer (residency em replica scenarios) + Privacy Officer (LGPD + Schrems II em failover paths).

## 3. Customer Impact & Journey

**Persona 1 — Customer EU experiencing region outage**:
- WEUR primary outage detected; failover routes reads to SAM-EU sibling replica.
- SLO-LAT-CAS-GET p99 < 300ms preserved (overhead ≤ 50ms p99).
- Customer notified via dashboard + email if outage > 30 min sustained.
- Writes blocked during failover (read-only mode); SLA addendum documented.

**Persona 2 — Auditor SOC 2 + ISO 27001 + GDPR**:
- PAT-REGION-FAILOVER-001 evidence: chaos test region outage cada região verde; SLO-LAT-CAS-GET p99 preserved.
- Replication lag SLO sustained 7d staging; runbook RB-FM-105 dry-run.
- Residency preserved em failover (replica_region restricted).

**Persona 3 — Internal SRE on-call**:
- DASH-REGION shows replication lag p99 + hot blob count + failover events.
- RB-FM-105 (region replication diverge) committed + dry-run.
- Chaos test region outage cada região runs em staging weekly.

**SLA addendum**:
- Region failover transparent ao cliente: SLO-LAT-CAS-GET p99 < 300ms preserved.
- Failover overhead ≤ 50ms p99.
- Replication lag p99 ≤ 60s sustained 7d.
- Hot blob detection: offline daily aggregation (NOT live cardinality-violating métrica).
- Failover read-only mode during outage; writes blocked + customer notified.
- Replica_region restricted by residency (WEUR → SAM-EU only; SAM → WEUR only; WNAM → ENAM only; ENAM → WNAM only).

## 4. Capability Mapping

- **CAP-REGION-003** (read failover hot blobs) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1 R-S14-3 + R-S14-4` + `resilience_patterns.md` (PAT-REGION-FAILOVER-001) + `slo_catalog.md` (SLO-LAT-CAS-GET p99 < 300ms preserved) + `observability_model.md §3.1` (cardinality budget + offline aggregation pattern) + `failure_modes.md` (FM-105 region replication diverge if exists; create stub if not).

## 5. Tipo

Background worker + middleware + chaos test; HIGH_RISK; FF-HR-002 + FF-HR-003.

## 6. Escopo

### 6.1 In-scope

1. **Offline aggregation job** `scripts/hot_blob_offline_aggregator.rs` (Rust binary, **OFFLINE NÃO live métrica**):
   - Cron daily 02:00 UTC.
   - Query S-09 audit log R2 events `corelink.cas.get.ok` for last 30d window.
   - DataFusion-style aggregation: `GROUP BY tenant_id, blob_hash; SUM(bytes) AS bytes_total; COUNT(*) AS access_count_30d`.
   - Compute top 1% per tenant by bytes_total.
   - INSERT into D1 `hot_blobs` table; UPDATE if exists; DELETE if not in current top 1%.
   - Métrica `corelink_hot_blob_aggregation_duration_seconds_bucket` (operation latency).
   - Métrica `corelink_hot_blob_count_per_tenant_tier{tenant_tier}` (gauge per tier — NÃO per tenant_id).
   - Cardinality validator CI gate ensures NO `tenant_id` label em live métricas.

2. **D1 schema migration `migrations/0XX_hot_blobs.sql`**:
   ```sql
   CREATE TABLE hot_blobs (
       tenant_id TEXT NOT NULL,
       blob_hash TEXT NOT NULL,
       primary_region TEXT NOT NULL CHECK (primary_region IN ('wnam','enam','weur','sam')),
       replica_region TEXT NOT NULL CHECK (replica_region IN ('wnam','enam','weur','sam')),
       bytes BIGINT NOT NULL,
       access_count_30d BIGINT NOT NULL,
       last_access_ms BIGINT NOT NULL,
       replicated_at_ms BIGINT,
       replication_status TEXT NOT NULL CHECK (replication_status IN ('pending', 'in_progress', 'replicated', 'failed', 'evicted')),
       PRIMARY KEY (tenant_id, blob_hash)
   );
   CREATE INDEX idx_hot_blobs_replication_status ON hot_blobs(replication_status);
   CREATE INDEX idx_hot_blobs_primary_region ON hot_blobs(primary_region);
   ```

3. **Replication worker `crates/corelink-replica-worker/`**:
   - Cloudflare Worker cron (every 10 min).
   - Reads D1 `hot_blobs WHERE replication_status = 'pending'`.
   - Per-blob: R2 GET primary region → R2 PUT replica region → verify hash post-copy → UPDATE D1 `replication_status = 'replicated'`.
   - Residency check: replica_region restricted (WEUR ↔ SAM-EU; WNAM ↔ ENAM); enforced em insert.
   - Métrica `corelink_region_replication_lag_seconds{primary_region, replica_region, plan}` (p99 ≤ 60s SLO).
   - Failure → retry exponential backoff (5 attempts max); persistent failure → SEV-2 alert + audit emit `corelink.region.replication.failed`.

4. **Failover router `crates/corelink-failover-router/`**:
   - Middleware Tower layer (after region_check, before R2 fetch).
   - Health probe per-region 5min cadence (synthetic GET request).
   - 5xx rate threshold > 1% + latency p99 > SLO + 3 consecutive failures within 5s = degraded.
   - If primary degraded: route read to replica region (D1 lookup `hot_blobs.replica_region`).
   - Failover overhead ≤ 50ms p99.
   - Read-only mode during failover; writes blocked + return 503 + audit emit + customer notification.

5. **Failover graph static config** (acyclic per-region):
   - WNAM ↔ ENAM (US sibling pairs).
   - WEUR ↔ SAM-EU (EU sibling; SAM has EU sub-region OR fallback to WEUR with residency check; **decision deferred**: if SAM = sa-east real Brazil region, then WEUR fails over to itself read-replica only, NOT cross-jurisdiction; documented em ADR).
   - **Strict**: failover graph acyclic; no loops; enforced em static config + integration test.

6. **Métricas underscored Prometheus** (per `observability_model.md §3.1`; label `plan` aplicável; **NUNCA per-tenant labels**):
   - `corelink_cas_get_bytes_total{tenant_tier, region}` (counter; tier label NOT tenant_id).
   - `corelink_hot_blob_count_per_tenant_tier{tenant_tier}` (gauge).
   - `corelink_hot_blob_aggregation_duration_seconds_bucket` (histogram; daily job latency).
   - `corelink_region_replication_lag_seconds{primary_region, replica_region, plan}` (histogram p99 ≤ 60s SLO).
   - `corelink_region_replication_total{primary_region, replica_region, outcome, plan}` (outcome ∈ ok|hash_mismatch|retry|failed).
   - `corelink_region_failover_total{from_region, to_region, plan}` (counter).
   - `corelink_region_failover_duration_seconds_bucket{from_region, to_region}` (histogram; failover overhead ≤ 50ms p99).
   - `corelink_region_health_probe_duration_seconds_bucket{region}` (histogram).
   - `corelink_region_health_status{region}` (gauge; 0=down/1=degraded/2=healthy).

7. **Observability** — trace spans `replica.{aggregate, replicate, copy, verify_hash, evict}` + `failover.{detect, route, fallback}` com attributes:
   - `region.primary` (enum).
   - `region.replica` (enum).
   - `replica.tenant_id_hash` (hashed).
   - `replica.blob_hash`.
   - `failover.trigger` (enum: 5xx_rate | latency_slo | consecutive_failures).
   - `result` (enum).

8. **Audit emission**:
   - `corelink.region.replication.{started, completed, failed}` per blob.
   - `corelink.region.failover.{detected, resolved}` per region health change.
   - `corelink.hot_blob.aggregation.{started, completed, failed}` per daily run.

9. **Chaos test region outage cada região**:
   - Simulate primary region 503 sustained 5 min (CF region partial outage simulation).
   - Verify: failover detected within 5s; routes engaged; SLO-LAT-CAS-GET p99 < 300ms preserved; customer notified after 30 min.
   - Per-region: 4 chaos scenarios (WNAM/ENAM/WEUR/SAM each).

10. **Property tests** (10k iter PR + 100k iter nightly):
    - `prop_replication_lag_within_slo`: simulate 10k blobs replication; assert p99 ≤ 60s.
    - `prop_failover_overhead_within_slo`: simulate 10k failover decisions; assert overhead ≤ 50ms p99.
    - `prop_residency_in_replication`: simulate 10k random tenant + region combinations; assert replica_region respects residency restrictions.
    - `prop_hash_integrity_post_replica`: simulate 10k replications; assert 0 hash mismatches.
    - `prop_failover_acyclic`: simulate 10k failover graph traversals; assert no cycles.

11. **Integration test E2E**:
    - Provision 4 tenant types per region em staging; warm up cache (top 1% blob writes).
    - Run offline aggregation; verify D1 `hot_blobs` populated.
    - Run replication worker; verify R2 sibling region has replicas.
    - Verify hash integrity per-replica.
    - Inject region outage; verify failover engages + SLO preserved.

12. **Runbook RB-FM-105 (region replication diverge)**:
    - Detection: hash_mismatch counter > 0 OR replication_lag p99 > 60s sustained 1h.
    - Investigation: identify divergent blob_hash + primary_region + replica_region.
    - Reconciliation: re-replicate from primary; verify hash; update D1 status.
    - Persistent divergence: manual investigation + Architect + Crypto SME (CAS integrity).
    - Customer notification if data corruption confirmed.

13. **Adversarial regression tests**:
    - Force replication storm: synthetic top 1% explosion; verify cardinality detector alert + manual override.
    - Tamper replica blob: inject corruption em sibling R2; verify hash mismatch + retry + alert.
    - Force failover loop: inject failover graph cycle attempt; verify static config rejects.
    - Stale read during failover: write to primary during failover; verify replica returns stale + customer alert.

### 6.2 Out-of-scope (deferred)

- **Active-active multi-region writes**: Fase 2.
- **Per-tenant region migration self-service**: manual ticket only at GA.
- **APAC/AFR regions**: pós-GA demand-driven.
- **Cross-region replication for non-hot blobs (top 1% only)**: cost-prohibitive at scale.
- **Multi-cloud replication (AWS S3 + R2)**: pós-GA enterprise.
- **Customer-controlled replication policy (opt-out, custom top-N%)**: pós-GA.
- **Federated KMS for replicated BYOK blobs**: BYOK adapters em WI-S14-004+; replication composed.

## 7. Anti-Scope

- Live Prometheus métrica labeled por `tenant_id` (CARDINALITY BUDGET VIOLATION; offline aggregation only).
- Skip residency restriction em replica_region (Schrems II violation).
- Skip hash verify post-copy (INV-CAS-INTEGRITY violation).
- Skip failover graph acyclic enforcement (loop possibility).
- Auto-failover sem multi-signal triangulation (false-positive cost regression).
- Failover with writes allowed (stale read post-write inconsistency).
- Skip chaos test region outage cada região (operational unreadiness).
- Skip RB-FM-105 dry-run.
- Replicate non-top-1% blobs (cost regression).
- Cross-jurisdiction replication (WEUR → ENAM = Schrems II violation).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-003 — Hot blob replica + read failover PAT-REGION-FAILOVER-001 + offline aggregation

  Background:
    Given 4 regions provisioned (WI-S14-001)
    Given region pinning enforced (WI-S14-002)
    Given S-09 audit log R2 operational
    Given D1 hot_blobs table created

  Scenario: Offline aggregation daily job
    Given S-09 audit log R2 contains 30d of corelink.cas.get.ok events
    When daily cron 02:00 UTC runs offline_aggregator
    Then GROUP BY tenant_id, blob_hash; SUM(bytes) computed
    And top 1% per tenant identified
    And D1 hot_blobs INSERT/UPDATE rows
    And NO live Prometheus métrica labeled por tenant_id (cardinality budget preserved)
    And aggregation duration métrica emit

  Scenario: Live métrica tier-labeled NOT tenant-labeled
    Given live emission corelink_cas_get_bytes_total
    When métrica emitted
    Then label set is {tenant_tier, region} ONLY (4 × 4 = 16 séries)
    And NO tenant_id label
    And cardinality budget preserved (INV-OBS-CARDINALITY-BUDGET S-09)

  Scenario: Replication worker cross-region
    Given D1 hot_blobs row (tenant=T1, blob_hash=H1, primary=weur, replica=sam, status=pending)
    When replication worker cron runs
    Then R2 GET corelink-cas-weur/H1 → fetch bytes
    And R2 PUT corelink-cas-sam/H1 → store
    And hash verified post-copy (INV-CAS-INTEGRITY)
    And D1 status updated to 'replicated' + replicated_at_ms set
    And lag métrica emit p99 ≤ 60s SLO

  Scenario: Residency restriction em replication (WEUR → SAM only)
    Given tenant T1 primary_region = 'weur'
    When replication worker assigns replica_region
    Then replica_region = 'sam' (EU sibling pair OR self read-replica per ADR)
    And NOT replica_region = 'enam' or 'wnam' (Schrems II violation)
    And INV-REGION-NO-CROSS-LEAK preserved

  Scenario: Hash mismatch post-replica detected
    Given replication completes
    When hash compare primary_etag vs replica_etag
    Then mismatch detected
    And retry exponential backoff
    And persistent mismatch = SEV-2 alert + audit emit

  Scenario: Failover detection multi-signal
    Given primary region WEUR latency p99 > 300ms sustained 5s
    Given 5xx rate > 1% sustained 5s
    Given 3 consecutive failures within 5s
    When failover router evaluates
    Then degraded status set
    And reads route to SAM replica
    And failover_total métrica increment
    And audit emit corelink.region.failover.detected

  Scenario: Failover overhead ≤ 50ms p99
    Given chaos test region outage WEUR
    When 10000 read requests during failover
    Then failover overhead p99 ≤ 50ms
    And SLO-LAT-CAS-GET p99 < 300ms preserved

  Scenario: Failover read-only mode (writes blocked)
    Given failover engaged WEUR → SAM
    When client writes to weur.api.corelink.dev
    Then 503 returned
    And customer notified via dashboard alert
    And write blocked (no stale read post-write inconsistency)

  Scenario: Failover graph acyclic
    Given failover graph WNAM↔ENAM, WEUR↔SAM
    When failover triggered
    Then no cycles possible
    And static config integrity verified em integration test

  Scenario: Chaos test region outage cada região verde
    Given 4 chaos scenarios (WNAM/ENAM/WEUR/SAM each)
    When all 4 scenarios run sequentially
    Then 4/4 scenarios green
    And SLO preserved per-region
    And customer notification if outage > 30 min

  Scenario: Property test 10k iter green
    Given prop_replication_lag_within_slo + prop_failover_overhead_within_slo + prop_residency + prop_hash_integrity + prop_failover_acyclic
    When 10k iter run em PR
    Then 0 SLO violations
    And 0 residency violations
    And 0 hash mismatches
    And 0 failover loops

  Scenario: RB-FM-105 dry-run
    Given simulated region replication divergence
    When dry-run executes
    Then runbook commands executable
    And reconciliation procedure validates
    And drift findings + runbook updates committed
```

## 9. Design Decisions

### 9.1 Why offline aggregation NÃO live métrica (Lote 9.4 Opus H-02 fix)

- Live métrica labeled por `tenant_id` = 1M+ séries (cardinality explode); INV-OBS-CARDINALITY-BUDGET violated.
- Offline aggregation = batch job; no cardinality budget impact.
- Aggregation runs daily; results cached em D1 (low latency reads).
- Live métrica é tier-labeled (4 × 4 = 16 séries baseline; budget-safe).
- **CRITICAL**: NO `tenant_id` label em ANY live Prometheus métrica em CoreLink S-14; cardinality validator CI enforces.

### 9.2 Why top 1% (NÃO top 5% / 10%)

- Cost regression bound: replication egress + storage = ~2× primary cost per replicated blob.
- Top 1% covers ~80% of read traffic (Pareto distribution observed).
- Top 5% = 5× cost; top 10% = 10× cost; trade-off cost vs SLO preservation.
- 1% is configurable via admin API (waiver); default GA = 1%.

### 9.3 Why replication async (NÃO sync)

- Sync replication = write latency 2× (primary + replica); SLO-LAT-CAS-PUT regression.
- Async = write latency primary only; replication background job.
- Eventual consistency acceptable for top 1% (CAS GET dominant; writes rare).
- Replication lag SLO p99 ≤ 60s sustained 7d.

### 9.4 Why failover read-only (NÃO active-active writes)

- Active-active = data divergence risk (split-brain); CAS integrity at risk.
- Read-only = simple semantics; writes blocked + customer notified.
- Active-active = Fase 2 (post-GA enterprise).

### 9.5 Why residency restriction em replica_region

- Schrems II + LGPD Art. 33: EU data must not transit non-EU; same logic for cross-jurisdiction.
- WEUR → SAM (EU sibling region; sa-east-1 has EU sub-region OR self read-replica) per ADR.
- WNAM ↔ ENAM (US sibling pair).
- Cross-jurisdiction (WEUR → ENAM) = forbidden by static config.

### 9.6 Why hash verify post-copy (INV-CAS-INTEGRITY)

- R2 ETag = MD5; primary vs replica ETag compare detects bit corruption.
- Mismatch = retry; persistent = audit emit + SEV-2.
- Standard pattern from S-01 CAS layer.

### 9.7 Why ADR potencial?

- Sim — **ADR-XXXX**: "Hot blob replica via offline aggregation (NOT live cardinality-violating métrica) + PAT-REGION-FAILOVER-001 + residency restriction em replica_region S-14". Decisão arquitetural cardinality budget critical; reuse pattern em APAC/AFR forward.

## 10. Completeness Criteria SOTA

- [ ] **10.s14.003.1** Offline aggregation daily job operational; D1 hot_blobs populated (EVT-013).
- [ ] **10.s14.003.2** Live métrica tier-labeled NOT tenant-labeled; cardinality validator CI gate green (EVT-002).
- [ ] **10.s14.003.3** Replication worker async; lag p99 ≤ 60s SLO sustained 7d staging (EVT-021) *(GA Evidence Gate D+60)*.
- [ ] **10.s14.003.4** Hash integrity post-replica verified; INV-CAS-INTEGRITY preserved (EVT-002).
- [ ] **10.s14.003.5** Residency restriction em replica_region (WEUR ↔ SAM-EU; WNAM ↔ ENAM); INV-REGION-NO-CROSS-LEAK preserved (EVT-002).
- [ ] **10.s14.003.6** Failover routing transparent; overhead ≤ 50ms p99; SLO-LAT-CAS-GET p99 < 300ms preserved chaos test (EVT-021).
- [ ] **10.s14.003.7** Failover read-only mode during outage; writes blocked + customer notified (EVT-024).
- [ ] **10.s14.003.8** Failover graph acyclic enforced (static config + integration test) (EVT-002).
- [ ] **10.s14.003.9** Chaos test region outage cada região (4/4 scenarios) verde (EVT-023).
- [ ] **10.s14.003.10** Property test 10k iter PR + 100k nightly green (EVT-022).
- [ ] **10.s14.003.11** Runbook RB-FM-105 (region replication diverge) committed + dry-run (EVT-017).
- [ ] **10.s14.003.12** Cost regression gate: replication overhead < 2× primary cost per top-1% blob; matched em benchmark CI.

## 11. DoD

- [ ] Crate `corelink-replica-worker` + `corelink-failover-router` compilam.
- [ ] Offline aggregation script `hot_blob_offline_aggregator.rs` operational.
- [ ] D1 schema migration applied.
- [ ] Replication worker cron deployed.
- [ ] Failover router middleware wired.
- [ ] Failover graph static config + integration test.
- [ ] All 12 Gherkin scenarios green em integration test.
- [ ] Property tests 5 props × 10k iter green.
- [ ] Adversarial regression tests 4+ scenarios green.
- [ ] Chaos test 4/4 region outage scenarios green.
- [ ] Métricas + audit emission operational.
- [ ] Runbook RB-FM-105 dry-run.
- [ ] ADR-XXXX (replica + failover) escrito + ratificado.
- [ ] Cardinality validator CI gate green.
- [ ] Cost regression gate green.
- [ ] Code review (Architect + SRE Lead + Compliance + Privacy).

## 12. Invariants Validated

### Mantidas

- **INV-OBS-CARDINALITY-BUDGET** (HIGH — registry §3.12 herdada S-09): este WI critical preserves; offline aggregation NOT live métrica labeled por tenant_id.
- **INV-CAS-INTEGRITY** (CRITICAL — registry §3.11 herdada S-01): hash verify post-replica.
- **INV-REGION-NO-CROSS-LEAK** (CRITICAL — registry §3.12 nova WI-S14-002): residency restriction em replica_region.
- **INV-DATA-RESIDENCY** (CRITICAL — registry §3.11 herdada): residency em failover paths.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada): replication audit emit em D1 atomic batch.

### Novas

Nenhuma direta neste WI; reforça INVs herdadas.

TLA+ alignment: registry §4.2 indica `region_residency.tla` PLANNED S-14 WI-S14-009; este WI provê failover implementation.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Offline aggregator | `scripts/hot_blob_offline_aggregator.rs` | Rust binary |
| Replication worker | `crates/corelink-replica-worker/src/lib.rs` | Rust |
| Failover router | `crates/corelink-failover-router/src/lib.rs` | Rust |
| D1 migration hot_blobs | `migrations/0XX_hot_blobs.sql` | SQL |
| Failover graph static config | `crates/corelink-failover-router/src/failover_graph.rs` | Rust |
| Property tests | `crates/corelink-replica-worker/tests/prop_replica.rs` | Rust |
| Adversarial tests | `crates/corelink-replica-worker/tests/adversarial.rs` | Rust |
| Integration tests E2E | `tests/e2e_hot_blob_replica.rs` | Rust |
| Chaos tests region outage | `tests/chaos_region_outage_{wnam,enam,weur,sam}.rs` | Rust |
| Runbook RB-FM-105 | `specs/05_runbooks/RB-FM-105.md` | Markdown |
| ADR-XXXX (replica + failover) | `specs/03_architecture/adrs/ADR-XXXX-hot-blob-replica-offline-aggregation.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s14.003.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s14.003.2** rustdoc 100% public API + 4 examples (4 regions).
- **14.s14.003.3** Test coverage ≥ 90% (`cargo tarpaulin`); property tests 10k+100k.
- **14.s14.003.4** Latência: replication lag p99 ≤ 60s; failover overhead ≤ 50ms p99.
- **14.s14.003.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s14.003.6** Cardinality validator CI gate: NO tenant_id label em live métricas.
- **14.s14.003.7** Runbook RB-FM-105 committed + dry-run.
- **14.s14.003.8** Memory bounded em offline aggregator (DataFusion-style streaming).
- **14.s14.003.9** Cost regression gate: < 2× primary cost per top-1% blob.
- **14.s14.003.10** SOC 2 + ISO 27001 + Schrems II attestation.

## 15. Chaos Experiments

1. **Region outage WNAM**: simulate 5 min outage; verify failover to ENAM; SLO preserved.

2. **Region outage ENAM**: same pattern.

3. **Region outage WEUR**: same pattern; verify EU residency preserved (SAM-EU sibling OR self read-replica).

4. **Region outage SAM**: same pattern.

5. **Hash mismatch synthesis**: inject corruption em replica; verify retry + alert.

6. **Replication storm**: synthetic top 1% explosion; verify cardinality detector + manual override.

7. **Failover loop attempt**: inject cycle in failover graph attempt; verify static config rejects.

8. **Stale read during failover**: write to primary; read from replica; verify customer notified.

9. **Cardinality budget violation attempt**: dev tries add tenant_id label; CI gate rejects.

10. **RB-FM-105 dry-run**: full incident response simulation; reconciliation + drift findings.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Architect + SRE Lead + Compliance + Privacy review):

- [ ] All 12 Gherkin scenarios green.
- [ ] Property tests 5 props × 10k iter green.
- [ ] Adversarial regression tests 4+ scenarios green.
- [ ] Chaos test 4/4 region outage scenarios green.
- [ ] Cost regression gate green.
- [ ] Cardinality validator CI gate green.
- [ ] Métricas + dashboards configurados em DASH-REGION.
- [ ] ADR-XXXX (replica + failover) published.
- [ ] Compliance Officer review (residency em replica + failover).
- [ ] Privacy Officer review (LGPD + Schrems II em failover paths).
- [ ] Architect approval (offline aggregation + cardinality budget + failover graph).

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Offline aggregator scaffold + DataFusion query | 3h |
| ST-002 | D1 hot_blobs migration + indexes | 1.5h |
| ST-003 | Replication worker scaffold + cron + R2 copy + hash verify | 3h |
| ST-004 | Residency restriction em replica_region (static config) | 1.5h |
| ST-005 | Failover router scaffold + multi-signal detection | 3h |
| ST-006 | Failover graph acyclic config + integration test | 1.5h |
| ST-007 | Read-only mode + writes blocked + customer notification | 2h |
| ST-008 | Métricas emit (9 metrics) + trace spans + audit emission | 2.5h |
| ST-009 | Property tests 5 props × 10k iter | 4h |
| ST-010 | Adversarial regression tests 4+ scenarios | 2h |
| ST-011 | Integration tests E2E 4 regions | 2.5h |
| ST-012 | Chaos test region outage cada região (4 scenarios) | 4h |
| ST-013 | Runbook RB-FM-105 escrita + dry-run | 2h |
| ST-014 | ADR-XXXX redação | 2h |
| ST-015 | Code review (Architect + SRE + Compliance + Privacy) | 2.5h |

**Total Optimistic**: ~37h. **PERT** (O=16h, M=24h, P=36h, per spec contract §12): **24.7h**.

## 18. Dependencies

### Hard blockers

- **WI-S14-001 SEALED** (4 regions infra; per-region R2 + D1).
- **WI-S14-002 SEALED** (region pinning enforcement; INV-REGION-NO-CROSS-LEAK).
- **S-09 SEALED** (audit log R2 + cardinality budget validator).
- **S-01 SEALED** (CAS layer; INV-CAS-INTEGRITY foundation).

### Soft blockers

- DataFusion crate (Arrow-Rust ecosystem) for offline aggregation.

### Outbound

- **WI-S14-004..007** (BYOK + erasure attestation respect failover paths).
- **WI-S14-009** (TLA+ region_residency formalizes failover; pentest validates).

## 19. Effort PERT

O: 16h, M: 24h, P: 36h → PERT **24.7h** (per spec contract §12).

## 20. Time-boxing

**32h hard limit owner**. If exceeded → escalation: split em sub-WI (offline aggregator vs replication worker vs failover router).

## 21. Observability

9 métricas listadas §6.1.6. Trace spans em §6.1.7. Logs structured JSON.

Dashboard widget DASH-REGION:
- Replication lag p99 per replica pair (heatmap).
- Hot blob count per tier (gauge).
- Failover events timeline.
- Region health status per-region.
- Hash mismatch alert counter (alert > 0).

## 22. Cost Analysis

- Offline aggregation: 1× daily run × ~10min compute = ~$10/mês.
- D1 `hot_blobs` storage: bounded top 1% = ~$5/mês.
- Replication worker: cron every 10min × R2 GET+PUT egress = ~$30/mês top 1%.
- Failover router: middleware overhead negligible.
- Health probes: 5min × 4 regions × $0.001 = ~$5/mês.
- **Total custo direto WI-S14-003**: ~$50/mês baseline + workload-dependent ~$200/mês.

## 23. API Contract

Failover transparent ao customer (no API changes). Replication worker é internal. Offline aggregator is admin-triggered (no public API).

API semver stable post v1.0; breaking changes em failover behavior = bump major + ADR + customer SLA addendum.

## 24. Post-mortem Hooks

- Replication lag p99 > 60s sustained 1h → SEV-2 + post-mortem.
- Hash mismatch detected → CRITICAL + Architect + Crypto SME review.
- Failover loop detected production → CRITICAL + static config review.
- Cardinality budget violation detected → CRITICAL + observability review.
- Region outage > 30 min → SEV-2 + customer notification + post-mortem.
- Failover false-positive > 1× mês → review thresholds.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- Replication rollback: D1 reset hot_blobs status; re-aggregate.
- Failover rollback: disable failover routing via admin API; manual cutover.
- RTO ≤ 30 min (Worker rollback).
- RPO 0 (replication eventual; primary preserves source-of-truth).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: replica region authenticated via Worker binding scope.
- **Tampering**: hash verify post-copy detects bit corruption.
- **Repudiation**: replication + failover events em audit chain.
- **Information disclosure**: residency restriction em replica_region; cross-jurisdiction forbidden.
- **DoS**: failover preserves SLO; chaos test verde.
- **Elevation of privilege**: admin API for manual override (waiver) requires admin role + dual-approval.

**LINDDUN delta**:
- **Linkability**: tenant_id em audit (compliance); NO tenant_id em métricas (cardinality).
- **Identifiability**: region em audit (intentional Schrems II evidence).
- **Non-repudiation**: replication + failover audit chain.
- **Detectability**: hash mismatch + replication lag alert.
- **Disclosure**: residency commitment em replica_region documented em DPA (WI-S14-008).
- **Unawareness**: customer notified on failover sustained > 30 min.
- **Non-compliance**: SOC 2 + Schrems II + LGPD Art. 33 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-replica-worker/README.md` — overview + offline aggregation pattern.
- ADR-XXXX — replica + failover ratification.
- Doc `docs/internal/multi-region-byok.md` (failover section) — sequence diagram + cardinality budget rationale.
- Workshop interno (1.5h) com Architect + SRE + Observability lead pós-merge.
- Onboarding test (5 questions): offline aggregation rationale, top 1% choice, residency em replica, failover graph acyclic, RB-FM-105.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cardinality budget violation via tenant_id label | L | H | CRITICAL | M | LOW | Offline aggregation + cardinality validator CI |
| R-002 | Residency violation em replication | L | H | CRITICAL | M | LOW | Static config restriction + property test + INV-REGION-NO-CROSS-LEAK |
| R-003 | Replication lag > 60s sustained | M | M | HIGH | M | LOW | Worker scale-out + alert SEV-2 + RB-FM-105 |
| R-004 | Hash mismatch post-replica | L | M | CRITICAL | L | LOW | Verify post-copy + retry + audit emit |
| R-005 | Failover false-positive | M | L | MEDIUM | M | LOW | Multi-signal triangulation + 5s sustained threshold |
| R-006 | Failover false-negative (real outage missed) | L | H | HIGH | M | LOW | Synthetic probe + body integrity check |
| R-007 | Replication storm (top 1% becomes 30%) | L | M | MEDIUM | L | LOW | Cardinality detector + manual override |
| R-008 | Failover routing loop | L | H | HIGH | M | LOW | Static config acyclic + integration test |
| R-009 | Stale data served during failover | M | M | MEDIUM | M | LOW | Read-only mode + customer notification |
| R-010 | Replica blob tampered (sibling region compromise) | L | H | CRITICAL | M | LOW | Hash verify + audit + alert |
| R-011 | Cost regression em replication | M | L | MEDIUM | L | LOW | Cost regression gate + top 1% bounded |
| R-012 | Chaos test impacta production | L | H | HIGH | L | LOW | Staging-only + production isolated |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Observability lead review offline aggregation + cardinality budget rationale.
2. **D1 + worker (D+1)**: SRE + Engineer pair-program replication worker.
3. **Failover router (D+2)**: SRE + Architect review multi-signal detection + acyclic graph.
4. **Code (D+3)**: peer review + adversarial test scenarios.
5. **Compliance (D+3)**: Compliance Officer review residency em replica + failover.
6. **Privacy (D+3)**: Privacy Officer review LGPD + Schrems II em failover paths.
7. **Property test (D+4)**: 10k iter green em PR; 100k nightly green.
8. **Chaos test (D+4)**: SRE + on-call run 4 region outage scenarios em staging.
9. **PRR mini (D+5)**: Architect + SRE + Compliance + Privacy sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — offline aggregation design + cardinality budget + failover graph acyclic_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — replica integrity + failover loop defense + adversarial scenarios_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — chaos test region outage + replication lag SLO + RB-FM-105_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD; emphatic — property test 10k + 100k nightly + chaos coverage_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — residency em replica + Schrems II em failover paths_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; emphatic — LGPD + GDPR em failover; customer notification on outage_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — replica tampering + failover loop attempts + storm scenarios_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (cripto-touching WIs S-14-004+; este WI é replication+failover; INV-CAS-INTEGRITY hash-related). Peer reviewers contribuem em PR review sem sign-off canonical separado.

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-003 (cycle 12.S14.0); Lote 9.4 Opus H-02 fix internalized: offline aggregation NOT live cardinality-violating métrica. |

## 32. Anti-patterns evitados

- Live Prometheus métrica labeled por `tenant_id` (CARDINALITY BUDGET VIOLATION).
- Skip residency restriction em replica_region.
- Skip hash verify post-copy.
- Skip failover graph acyclic enforcement.
- Auto-failover sem multi-signal triangulation.
- Failover with writes allowed (stale read inconsistency).
- Skip chaos test region outage cada região.
- Skip RB-FM-105 dry-run.
- Replicate non-top-1% blobs (cost regression).
- Cross-jurisdiction replication.
- Sync replication (write latency 2×).
- Active-active multi-region writes (split-brain risk; Fase 2).

---

**Fim WI-S14-003.** Próximo: WI-S14-004 (BYOK trait + AWS KMS adapter + envelope encryption + matrix framework).
