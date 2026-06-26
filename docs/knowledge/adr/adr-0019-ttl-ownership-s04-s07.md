---
type: "ADR"
title: "ADR-0019 — AC TTL ownership: S-07 supersedes S-04 with per-tier defaults"
description: "Resolves an ownership clash so S-07's per-tier AC TTL defaults supersede S-04's flat 90d, with migration rollout deferred to S-13 behind a TierTtlResolver boundary trait."
source_files:
  - "specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "ownership", "ac-ttl", "eviction", "tier"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0019 — AC TTL ownership: S-07 supersedes S-04 with per-tier defaults

The Action Cache TTL governs how long cached `ActionResult` entries live, and two sprints both claimed it: S-04 delivered a flat 90d default while S-07 delivered per-tier expiry (free=7d … business=365d). Without a ruling, a free tenant onboarded under S-04 would silently lose 12.8× of cache lifetime when S-07 shipped, breaking the publicly published tier SLA. This ADR makes S-07's per-tier table canonical, keeps S-04's TTL infrastructure, and defers the migration rollout to S-13 behind a swappable resolver trait. It matters as the single source of truth for AC TTL, governing the [Action Cache](/surfaces/action-cache.md) surface.

# Context

A Round-2 audit found an ownership clash: S-04's `CAP-AC-004` set a default 90d AC TTL with refresh-on-hit, while S-07's `CAP-EVICT-002` set per-tier TTL — so a free tenant created in S-04 staging with a promised 90d lifetime would drop to 7d when S-07 shipped, with no migration plan and a public SLA contract at risk (`specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:20-28`).

# Decision

S-07 supersedes S-04's TTL default semantics; the canonical truth is the per-tier table (Free 7d, Solo 30d, Team 90d, Business 365d, Enterprise configurable) while S-04 retains the TTL infrastructure (worker, refresh-on-hit, expiry detection) and S-07 overrides only the default value (`specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:32-42`). The migration EXECUTION is deferred to S-13 (config rollout + 14d notification + grace), while S-07 owns runtime TTL resolution — a separation of concerns matching the ADR-0020 email-defer pattern (`specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:44-55`). The ADR specifies the S-04↔S-07 boundary as a `TierTtlResolver` trait whose S-04 fallback impl is swapped for the per-tier impl when S-07 seals, leaving the cron worker unchanged (`specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:81-94`).

# Consequences

- A single source of truth for AC TTL (S-07 + this ADR + the S-18 docs), with customer trust preserved by a 14d notification and pricing-tier semantics now enforceable (`specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:59-63`).
- Free users lose 12.8× cache lifetime (mitigated by a clear pricing page + upgrade path) and migration adds ~2 weeks of notification/grace complexity (`specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:65-67`).
- The ADR records the delivered TTL infrastructure (refresh-on-hit gate, bounded batch eviction, per-region cron, anti-bulk-DELETE trait surface) with the per-tier resolver impl deferred to S-07 (`specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:96-111`).

# Citations

1. `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:20-28` — the S-04 90d vs S-07 per-tier ownership clash + SLA risk.
2. `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:32-42` — the decision: S-07 supersedes; per-tier table canonical.
3. `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:44-55` — migration execution deferred to S-13.
4. `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:81-94` — the TierTtlResolver boundary mechanism as specified.
5. `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:59-67` — positive and negative consequences.
6. `specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md:96-111` — delivered TTL infrastructure and deferred items.
