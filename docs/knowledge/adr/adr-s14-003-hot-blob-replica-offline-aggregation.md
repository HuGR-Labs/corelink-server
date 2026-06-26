---
type: "ADR"
title: "ADR-S14-003 — Hot-blob replica via offline aggregation + residency-restricted failover"
description: "Why hot-blob detection uses daily offline batch aggregation (not a tenant-id-labeled live metric) and why replication is restricted to acyclic same-jurisdiction sibling pairs."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s14", "region", "failover", "replica", "cardinality"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-003 — Hot-blob replica via offline aggregation + residency-restricted failover

To survive a regional outage, CoreLink proactively replicates the hottest CAS blobs to sibling regions. The naive design — a live `corelink_cas_get_bytes_total{tenant_id, blob_hash}` metric to find the top 1% — was rejected during review as a cardinality-budget violation (1M+ series vs a 100k budget). This ADR (DRAFT) records the decision to detect hot blobs via daily offline batch aggregation instead, to restrict replication to acyclic same-jurisdiction sibling pairs, and to gate failover on a 3-signal acyclic router.

# Context

CAP-REGION-003 (read failover for hot blobs) initially used a live per-blob metric; with 10k+ tenants × 100 blobs that is 1M+ series, 10× the INV-OBS-CARDINALITY-BUDGET total — causing Mimir tenant-limit breaches and silent metric drops (`specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:24-32`). Naive replication also risks cross-jurisdiction transfer (GDPR Art. 44 / Schrems II / LGPD Art. 33) and, without an acyclic graph, failover routing loops (`specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:33-36`).

# Decision

- **Offline batch aggregation, not a live metric**: a daily 02:00 UTC cron groups S-09 audit-log `corelink.cas.get.ok` events by `(tenant_id, blob_hash)`, takes the top 1% per tenant by 30d bytes into a D1 `hot_blobs` table; the only live metric is tier-labeled `{tenant_tier, region}` (16 series, budget-safe), with a CI gate enforcing `NO_TENANT_ID_LABEL` (`specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:39-50`).
- **Residency-restricted replication**: static acyclic sibling pairs WNAM↔ENAM (US) and WEUR↔SAM (EU/LGPD); cross-jurisdiction transfers are FORBIDDEN by `ResidencyGraph::is_allowed()` with no runtime override, verified by a 10k property test (`specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:52-58`).
- **PAT-REGION-FAILOVER-001**: failover requires all 3 signals simultaneously (5xx > 1%, p99 > 300ms, ≥3 consecutive failures, each sustained 5s); the failover graph is the same acyclic sibling map; writes are blocked (503) during failover to prevent stale-read inconsistency (`specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:60-73`).

# Consequences

- Positive: cardinality budget preserved (16 vs 1M+ series); zero cross-jurisdiction replication; failover loops eliminated by construction; SLO-LAT-CAS-GET held during chaos via sibling reads (`specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:77-83`).
- Negative / trade-offs: hot-blob detection lags ≤24h (30d window smooths spikes); writes are blocked during failover (read-heavy workload + SLA addendum); WEUR→SAM is cross-hemisphere latency, accepted as the only Schrems II-safe sibling with async replication (`specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:85-89`).

# Citations

1. `specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:24-36` — the cardinality-budget violation, cross-jurisdiction risk, and failover-loop risk.
2. `specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:39-50` — offline batch aggregation and the tier-only live metric.
3. `specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:52-58` — acyclic same-jurisdiction sibling pairs and the forbidden cross-jurisdiction transfers.
4. `specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:60-73` — the 3-signal failover router and read-only mode.
5. `specs/03_architecture/adrs/ADR-S14-003-hot-blob-replica-offline-aggregation.md:77-89` — positive consequences and trade-offs.
