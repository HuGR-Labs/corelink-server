---
type: "ADR"
title: "ADR-S13-001 — DO config-singleton per-region with CAS atomic update"
description: "Why CoreLink's runtime config (flags, rate-limit tunables, retention) lives in a per-region Durable Object with compare-and-swap versioning rather than D1 or KV."
source_files:
  - "specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s13", "config", "durable-object", "cas", "admin-plane"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S13-001 — DO config-singleton per-region with CAS atomic update

CoreLink needs a single runtime source-of-truth for feature flags, rate-limit tunables, and retention policies that is strongly consistent (concurrent admin writes must never lose data — the blast radius is global multi-tenant), low-latency on the read hot path, audited for 90 days, and propagated to the edge within 5s p99. This ADR records the decision to host that config in a per-region Durable Object guarded by compare-and-swap versioning, and why KV and D1 were rejected.

# Context

The config source must guarantee strong consistency (a lost-write race is catastrophic across all tenants), low read latency (Workers read config on the rate-limit middleware hot path), 90d audit history for SOC 2 CC8.1 + rollback, and ≤5s p99 edge propagation since stale config has global blast radius (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:28-41`).

# Decision

Use a `ConfigSingletonDO` Durable Object as the per-region source-of-truth (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:44-45`):

- **Single instance per region** via `idFromName("corelink-config-{region}")` — strong consistency in the DO model with per-region blast radius (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:47-50`).
- **CAS atomic update**: the caller supplies `expected_version`; a mismatch returns 409, a match increments the version and writes — making concurrent lost-write races impossible (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:51-56`).
- **Propagation** via a Cloudflare Queue with a 60s idempotent poll safety-net, SLO ≤5s p99 (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:57-62`).
- **90d audit** in D1 `config_change_log` and a versioned **rollback API** with pre-write re-validation (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:63-69`).
- **Alternatives rejected:** D1 single-row (weaker row-locking for high-concurrency CAS, no native cross-region), and Cloudflare KV (eventual-consistency — *unsafe* for CAS, lost writes possible) (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:73-82`).

# Consequences

- Positive: lost-write race impossible; 90d history for SOC 2 CC8.1; rollback ≤5 min p99; fresh edge config ≤5s p99 (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:99-105`).
- Negative / mitigated: DO single-instance is a per-region SPOF (Cloudflare DO HA + regional isolation), and a CAS retry storm under high concurrency (client backoff + circuit breaker + alert) (`specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:107-114`).

# Citations

1. `specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:28-41` — the consistency / latency / audit / propagation requirements.
2. `specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:44-69` — the DO singleton + CAS + queue + audit + rollback decision.
3. `specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:73-82` — D1 and KV alternatives rejected (KV unsafe for CAS).
4. `specs/03_architecture/adrs/ADR-S13-001-do-config-singleton-cas.md:99-114` — positive consequences and mitigated negatives.
