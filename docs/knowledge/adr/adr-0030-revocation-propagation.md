---
type: "ADR"
title: "ADR-0030 — PAT revocation propagation (DO + CF Queue, ≤ 60 s p99)"
description: "Why PAT revocation uses Neon as source-of-truth with a Durable-Object broadcast cache and CF-Queue cross-region fan-out to hit a single 60 s p99 stale-window SLA."
source_files:
  - "specs/03_architecture/adrs/ADR-0030-revocation-propagation.md"
checkpoint_sha: "0aad76e1d132cd98d35c814a5bb23008c226d08e"
provenance: "AUTHORED"
tags: ["adr", "revocation", "pat", "durable-object", "cf-queue", "auth", "s03"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0030 — PAT revocation propagation (DO + CF Queue, ≤ 60 s p99)

A compromised PAT must stop being accepted globally within one minute of revocation. This ADR records the DESIGN-INTENT for how that `SLO-FRESH-PAT-REVOKE ≤ 60 s p99` would be met by coordinating three Cloudflare primitives — Neon as transactional truth, a region-pinned Durable Object as a broadcast cache, and CF Queue for cross-region fan-out — and why the hot caches are never allowed to become sources of truth.

> **Status vs shipped code (2026-06-28):** the Neon-SoT + `RevocationDo` broadcast + CF-Queue cross-region fan-out backbone is DEFERRED, not deployed. What ships today is the in-memory `InMemoryRevocationStore` behind the `RevocationStore` trait — a single-process `HashMap` of revoked entries with a per-PAT propagation-status map (`corelink-worker/src/auth/revocation/in_memory_store.rs` around lines 30 and 69). There is no Neon source-of-truth, no Durable-Object broadcast, and no CF-Queue fan-out wired in the running system; the three-axis `≤ 60 s p99` SLA, the `INV-AUTH-NEON-IS-SOT` fail-closed cold path, and the cross-region drain below describe the target architecture, not the deployed one.

# Context

Three orthogonal axes contribute to the stale window: a hot-path session cache bounded by KV TTL, a cold path that consults Neon `pat.revoked_at IS NULL` bounded by regional commit visibility, and cross-region propagation from the origin region to the four peers; a single canonical surface must coordinate the DO, Neon, and CF Queue plus the session-cache invalidate hook (ADR-0030:24-32).

# Decision

Neon `pat.revoked_at` is the canonical source-of-truth that the verify cold path queries directly so a DO/KV outage never compromises correctness (`INV-AUTH-NEON-IS-SOT`), with the revoke committing the `pat` UPDATE and `audit_outbox` INSERT in one idempotent transaction; a per-region `RevocationDo` acts as a broadcast cache with an idempotent `ingest_remote` for at-least-once delivery; CF Queue (not synchronous DO RPC) does cross-region fan-out so the caller returns in ~100 ms while peers drain async; and the customer SLA is the *max* of the three independent axes, a single 60 s p99, not their sum (ADR-0030:35-66).

# Consequences

The verify path is fail-closed on the Neon SoT so a cache outage never lets a revoked token through, propagation latency is bounded with tiered SEV alerts, and mass-revoke is atomic, traded against running three storage planes that need a daily reconciliation cron and cross-region lag that can exceed 60 s during a CF Queue outage (graceful but customer-visible) (ADR-0030:72-87). It governs the [D1 PAT store](/auth/d1-pat-store.md) and the [PAT verification gauntlet](/flows/pat-gauntlet.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0030-revocation-propagation.md:24-32` — the 60 s p99 SLO and the three stale-window axes (Context).
2. `specs/03_architecture/adrs/ADR-0030-revocation-propagation.md:35-66` — Neon SoT + DO broadcast cache + CF-Queue fan-out + single-max-axis 60 s SLA (Decision).
3. `specs/03_architecture/adrs/ADR-0030-revocation-propagation.md:72-87` — fail-closed verify vs three-plane reconciliation and Queue-outage lag (Consequences).
