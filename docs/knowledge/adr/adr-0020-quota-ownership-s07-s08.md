---
type: "ADR"
title: "ADR-0020 — Quota ownership: S-07 storage soft-pressure, S-08 hard-block + bandwidth"
description: "Splits quota ownership by category so S-07 owns storage soft-pressure eviction (≤95%) and S-08 owns the 100% hard-block plus bandwidth and behavioral rate-limits."
source_files:
  - "specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md"
  - "crates/corelink-container/src/tenant_quota.rs"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "ownership", "quota", "rate-limit", "eviction"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0020 — Quota ownership: S-07 storage soft-pressure, S-08 hard-block + bandwidth

CoreLink enforces several distinct quota categories, and two sprints both claimed "quota," leaving it unclear who detects a breach, who enforces it, and who alerts. This ADR decomposes ownership by category: S-07 owns storage soft-pressure (eviction below 100%) and S-08 owns the 100% hard-block plus bandwidth and behavioral rate-limits. It matters because a clean owner per category turns quota from a cliff into a gradient (silent eviction, then a 429) and prevents blind eviction. It governs the same tenant-quota machinery surfaced via the [storage-quota header](/tenancy/storage-quota-header.md) and the [request quota](/tenancy/request-quota.md).

# Context

A Round-2 audit found an ownership clash: S-07 delivered `CAP-EVICT-003` "Quota enforcement (storage)" via eviction while S-08 delivered `CAP-QUOTA-001/002` for per-tenant storage and ingress/egress quota — so both sprints claimed "quota" with no clear decomposition of who detects, enforces, and alerts (`specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:21-26`).

# Decision

Decompose ownership by quota category: S-07 owns storage soft-pressure eviction — triggered when `bytes_used > 0.8 × storage_limit`, using LRU eviction within the tier to reduce usage before any hard block; S-08 owns bandwidth + behavioral quota — per-tenant/IP/PAT rate-limits, the 100% storage hard-block (429 + `Retry-After`), monthly ingress/egress counters, and abuse response (`specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:32-44`). The boundary protocol: soft state (80–95% storage) is S-07's; hard state (100% storage, bandwidth, rate-limit, or abuse) is S-08's (`specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:46-49`). "S-08 owns all quotas" and "S-07 owns all storage quotas" were both rejected (`specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:70-71`).

# Consequences

- One owner per quota category gives clear redirect/block decisions, separate storage/bandwidth/behavioral metrics, and a customer-facing gradient rather than a cliff (`specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:53-57`).
- The two sprints must still coordinate config via the S-13 DO config-singleton, and S-08's quota check reads the `bytes_used` that S-07 maintains — an explicit cross-reference coupling (`specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:59-61`).

# Status vs shipped code

One nuance vs the shipped container: the ADR describes the 100% hard-block as a `429 + Retry-After`,
but the live per-tenant **$-ceiling** over-limit response is a **`402 PAYMENT_REQUIRED`** ("monthly
$-ceiling exceeded; raise the cap or wait for the cycle to reset"), on both the existing-row and the
brand-new-tenant seed paths (`crates/corelink-container/src/tenant_quota.rs:799,825`) — matching the
402 surfaced by [ADR-0068's dollar-ceiling](/adr/adr-0068-per-tenant-monthly-dollar-ceiling.md). The
429 + Retry-After framing applies to the bandwidth/behavioral rate-limit arm, not the cost-ceiling
arm. The ownership decomposition itself is unchanged.

# Citations

1. `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:21-26` — the S-07/S-08 quota ownership clash.
2. `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:32-44` — the decision: S-07 storage soft-pressure, S-08 bandwidth/behavioral/hard-block.
3. `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:46-49` — the soft-vs-hard boundary protocol.
4. `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:70-71` — rejected single-owner alternatives.
5. `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:53-57` — positive consequences (clear ownership, gradient).
6. `specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md:59-61` — the explicit S-07↔S-08 coupling cost.
7. `crates/corelink-container/src/tenant_quota.rs:799` — the live over-limit response is a `402 PAYMENT_REQUIRED` ($-ceiling), not the ADR's `429 + Retry-After` framing (also on the new-tenant seed path at :825).
