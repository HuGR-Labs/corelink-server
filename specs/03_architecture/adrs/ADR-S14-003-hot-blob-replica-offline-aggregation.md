---
id: "ADR-S14-003"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s14", "region", "failover", "replica", "hot-blob", "offline-aggregation", "cardinality-budget", "high-risk"]
---

# ADR-S14-003 — Hot Blob Replica via Offline Aggregation (NOT Live Cardinality-Violating Metric) + PAT-REGION-FAILOVER-001 + Residency Restriction in `replica_region` S-14

## Status

DRAFT — pending Architect + SRE Lead + Compliance + Privacy Officer ratification.

## Context

WI-S14-003 implements CAP-REGION-003 (read failover hot blobs). The initial design considered using a live Prometheus metric `corelink_cas_get_bytes_total{tenant_id, blob_hash}` to identify hot blobs (top 1% by bytes transferred) for proactive replication to sibling regions. During Lote 9.4 Opus H-02 review, this was flagged as a cardinality budget violation.

**Problem**: With 10,000+ tenants × 100 unique blobs each = 1,000,000+ unique time series. INV-OBS-CARDINALITY-BUDGET (S-09) bounds per-metric series at 20,000 and total at 100,000. The proposed live metric would consume 10× the total budget, causing:
- Grafana Mimir tenant limit exceeded
- `alert tenant-limit-exceeded` SEV-2 firing
- Metrics dropped silently (no data loss visibility)
- Cardinality explosion silently degrading observability stack

**Cross-region residency risk**: Naïve replication without jurisdiction constraints would violate GDPR Art. 44 + Schrems II (CJEU C-311/18) + LGPD Art. 33 §1º. An EU tenant's blob replicated to a US region would constitute a cross-jurisdiction transfer without an adequacy decision.

**Failover loop risk**: Without a static acyclic failover graph, multi-region degradation could produce routing loops (A→B→C→A), causing infinite redirects and SLO collapse.

## Decision

### 1. Offline batch aggregation (NOT live metric)

Hot blob detection uses **offline batch aggregation**:
- Cron job: daily 02:00 UTC in Cloudflare Worker.
- Data source: S-09 audit log R2 (`corelink.cas.get.ok` events; payload `{tenant_id, blob_hash, bytes, ts}`).
- Computation: `GROUP BY tenant_id, blob_hash; SUM(bytes) AS bytes_total; COUNT(*) AS access_count_30d` over last 30d window.
- Gate: top 1% per tenant by `bytes_total` (covers ~80% of read traffic per Pareto; configurable via admin API).
- Storage: D1 `hot_blobs` table (migration `0027_hot_blobs.sql`).

**Live metric**: `corelink_cas_get_bytes_total{tenant_tier, region}` — tier-labeled ONLY (4 tiers × 4 regions = 16 series; budget-safe). Never labeled by `tenant_id`.

Cardinality validator CI gate enforces `NO_TENANT_ID_LABEL = true` at compile time (`crates/corelink-replica-worker/src/cardinality.rs`).

### 2. Residency restriction in `replica_region`

Static acyclic sibling pairs enforce jurisdiction-safe replication:
- **WNAM ↔ ENAM**: US sibling pair (CCPA/PIPEDA; same jurisdiction).
- **WEUR ↔ SAM**: EU ↔ LGPD sibling pair (Schrems II: WEUR blob may not transit to ENAM/WNAM without adequacy decision; SAM is the closest compliant sibling).

Cross-jurisdiction transfers (WEUR→ENAM, WEUR→WNAM, SAM→ENAM, SAM→WNAM) are **FORBIDDEN** by `ResidencyGraph::is_allowed()` (static config; no runtime override). Property test `prop_residency_in_replication` verifies 0 violations across 10k iterations.

### 3. PAT-REGION-FAILOVER-001 — Multi-signal detection + acyclic failover graph

Failover router (`crates/corelink-failover-router`) uses multi-signal triangulation:
- Signal 1: 5xx rate > 1% sustained 5s.
- Signal 2: Latency p99 > 300ms (SLO-LAT-CAS-GET) sustained 5s.
- Signal 3: ≥ 3 consecutive failures within 5s.

ALL 3 signals must be simultaneously active (prevents false-positive from transient slowness).

Failover graph = same `ResidencyGraph` sibling map (acyclic; `is_acyclic()` verified in property test).

**Read-only mode**: writes blocked during failover (HTTP 503); prevents stale read post-write inconsistency. Customer notified after 30 min sustained outage.

**Overhead SLO**: failover overhead ≤ 50ms p99 (`SLO_FAILOVER_OVERHEAD_MS = 50`).

## Consequences

### Positive

- **INV-OBS-CARDINALITY-BUDGET preserved**: 16 series vs 1M+ (0.016% of per-metric budget).
- **Schrems II + LGPD Art. 33 compliant**: zero cross-jurisdiction replication in static config.
- **Failover loop eliminated**: acyclic graph by construction; verified by property test.
- **SLO-LAT-CAS-GET preserved during chaos**: p99 < 300ms maintained via sibling reads.
- **Pattern reusable**: APAC/AFR phase 2 regions follow same offline aggregation + residency pair pattern.

### Negative / Trade-offs

- **Top-1% detection is daily (not real-time)**: hot blob set lags by ≤ 24h after access pattern shift. Mitigated by: 30d rolling window smooths spikes; manual re-trigger via admin API.
- **Write blocking during failover**: customer-visible impact (HTTP 503 on writes). Mitigated by: read-heavy CAS workload (writes rare during failover); SLA addendum documented; active-active writes deferred to Fase 2.
- **WEUR→SAM cross-hemisphere latency**: SAM (sa-east) is the EU sibling, which introduces inter-continental replica lag. Accepted: (a) SAM is the only Schrems II-safe sibling; (b) replication is async (lag p99 ≤ 60s SLO); (c) WEUR self read-replica is an alternative (deferred).

## Alternatives Considered

### A: Live metric `tenant_id` label (REJECTED)
Cardinality explosion: 1M+ series → INV-OBS-CARDINALITY-BUDGET violated. Silent metric drop risk. Rejected.

### B: Separate per-tenant metric store (e.g., KV)
Complex; non-standard; bypasses observability stack. Introduces KV consistency risk. Rejected for GA; may revisit post-GA as Tier-2 feature.

### C: WEUR → ENAM replication with adequacy SCCs
Standard Contractual Clauses are revocable (Schrems II risk). Cross-jurisdiction transfer remains legally uncertain. Rejected; SAM is safer.

### D: Active-active multi-region writes
Split-brain risk; CAS integrity at risk; complexity 5×. Deferred to Fase 2 (post-GA enterprise).

## References

- `specs/04_sprints/S14/_spec_contract.md` §5.1 R-S14-3 (Lote 9.4 Opus H-02 fix)
- `specs/04_sprints/S14/work_items/WI-S14-003-hot-blob-replica-pat-region-failover-001.md`
- `crates/corelink-replica-worker/src/cardinality.rs` (NO_TENANT_ID_LABEL constant)
- `crates/corelink-replica-worker/src/region.rs` (ResidencyGraph)
- `migrations/d1/0027_hot_blobs.sql`
- INV-OBS-CARDINALITY-BUDGET (invariant_registry.md §3.12, S-09)
- INV-CAS-INTEGRITY (invariant_registry.md §3.11, S-01)
- INV-REGION-NO-CROSS-LEAK (invariant_registry.md §3.12, WI-S14-002)
- GDPR Art. 44 + Schrems II (CJEU C-311/18) + LGPD Art. 33 §1º
- `specs/05_quality/runbooks/RB-FM-105-region-replication-diverge.md`

## Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Initial draft — WI-S14-003 implementation; Lote 9.4 Opus H-02 fix internalized. |
