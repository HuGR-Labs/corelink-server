# Decision record — CAS cold-start / warm-container (WP-3): accept for v1, do NOT always-warm

> 2026-06-19 · CoreLink Server TL · CAS hot-path latency wave (WP-3).
> Companion to `docs/perf/2026-06-19-cas-hot-path-latency.md` (the measured root cause) and
> ADR-0069 (WP-4). **This is a decided position, not deferred debt.**

## Context

The CAS latency measurement showed two cost components: the per-request D1-over-HTTP hops
(fixed in WP-2a/WP-2b → single-object GET ≈ sub-100ms warm) and a **container cold-start of
~2.5s**. The container (Worker → per-tenant Durable Object → native container) is stopped by
the DO after `IDLE_TIMEOUT_MS = 5 min` of inactivity (`worker/src/durable_object.ts`); the
next request after idle pays the cold-start.

## Decision

**Accept the cold-start for v1. Do NOT keep containers always-warm.**

Rationale:
- The cold-start is **once per 5-min idle period**, not per-request. For the real workloads —
  bulk git ingest (continuous) and the WP-1 batch endpoints (few large requests) — it
  amortizes to ~zero across the session.
- CoreLink is **self-serve SMB at ~80% target margin** (CLAUDE.md). Keeping a container warm
  per-tenant 24/7 is a direct, recurring COGS hit against that margin for an idle tenant who
  may make no requests for hours. Always-warm trades a one-time 2.5s for continuous spend —
  the wrong trade for this product economics.
- There is no free "warm": cold-start is inherent to on-demand containers; eliminating it
  means paying to hold compute.

## What would change this (revisit triggers)

- If hugit's **serve-boot eager-prefetch** (or another latency-sensitive read path) sets a
  per-request SLO that the cold-start violates *in practice* (not in theory), revisit with a
  **scoped** warm strategy: keep warm only for tenants in a recently-active session (a sliding
  activity window), never globally — preserving the margin while killing the cold-start for
  the tenants who are actually using it.
- If measurement shows the cold-start (not the D1 hops) is the binding constraint on a real
  customer workload.

## Status

CLOSED — v1 accepts cold-start; WP-3 implements **no** always-warm. The DO's existing 5-min
idle + 30s health alarm are unchanged. Revisit only on the triggers above.
