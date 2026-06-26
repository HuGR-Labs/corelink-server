---
type: "ADR"
title: "ADR-0040 — Multipart D1 sharding strategy (per-region, 80% trigger)"
description: "Shards the chunks table per-region (5 shards) with an 80%-of-10GB sharding trigger, because projected row counts exceed the D1 per-database hard limit."
source_files:
  - "specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "multipart", "d1", "sharding", "scaling", "s05"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0040 — Multipart D1 sharding strategy (per-region, 80% trigger)

The `chunks` table is the densest control-plane table CoreLink has, and at GA-target workload its
projected size blows past Cloudflare D1's per-database hard limit — so the question is not whether to
shard but how. This ADR is the design record (originally flagged as a rubber-stamp risk and then
authored properly) that picks the shard key, the trigger threshold, and the migration choreography,
which is essential context for the [chunk/manifest buckets](/storage/chunk-manifest-buckets.md) and
the [CAS/AC core crate cluster](/crates/cas-ac-core.md).

# Context

The WI-S05-004 `chunks` schema projects ~250M rows × 150 bytes ≈ 37 GB at the S-05 GA target, while
the D1 hard limit per database is 10 GB; sharding is therefore mandatory before the threshold, and
the ADR must define the shard key, trigger, migration plan, and cross-shard query semantics.

# Decision

The shard key is **per-region (5 shards: sam, iad, lhr, nrt, syd)** — aligned with R2 bucket region
attribution, fixed in count (no row migration on tier upgrades), and matching the per-region/per-tenant
shape of most analytics queries; per-tenant_tier, per-tenant, and hash-based sharding were all
rejected. Sharding is mandatory before any single shard reaches 80% of the 10 GB limit (≈ 8 GB,
≈ 30M rows), driven by `corelink.d1.chunks.size_bytes{region}` alerts (PD-WARNING at 50%, PD-CRITICAL
at 80%, hard-stop at 95%). The first-shard-split migration is a staged dual-write + reconcile + cutover
plan, and a v1.1.0 addendum explicitly includes the LIVE `multipart_sessions` table in the reconcile
scope so the orphan-detection invariant holds across split transitions.

# Consequences

D1 storage scales to 5 × 10 GB and the sharding trigger is explicit and alert-driven; the costs are
that cross-region queries are pushed to the S-09 OLAP pipeline (extra infrastructure) and a rare
multi-region tenant has chunks distributed across shards.

# Citations

1. `specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md:24-31` — Context: 250M-row/37 GB projection vs the 10 GB D1 hard limit; sharding mandatory.
2. `specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md:34-46` — Decision: per-region (5 shards) shard key + the rejected alternatives.
3. `specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md:50-56` — the 80%-of-10GB sharding trigger + the alert ladder.
4. `specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md:72-87` — the v1.1.0 addendum folding `multipart_sessions` into the reconcile scope.
5. `specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md:97-104` — Consequences: 5×10 GB scale + explicit trigger vs cross-region OLAP forwarding cost.
