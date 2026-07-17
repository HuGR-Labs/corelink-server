---
id: "ADR-0030"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.1"
created: "2026-04-30"
updated: "2026-07-12"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "revocation", "durable-object", "cf-queue", "propagation", "auth", "s03"]
---

# ADR-0030 — Revocation Propagation: Durable Object + CF Queue + ≤ 60 s p99 single SLA

## Status

FROZEN (S-03 WI-S03-004 ratificada em Lote 10.3 → SEALED 2026-04-30).

## Context

S-03 introduces personal access tokens with revocation as a first-class lifecycle event. The canonical SLO is `SLO-FRESH-PAT-REVOKE ≤ 60 s p99` — a compromised PAT must stop being accepted globally within one minute of the user / admin clicking "revoke." Three orthogonal axes contribute to "stale window" in the deployed system:

1. **Hot path** — verify hits a regional session cache populated before the revoke. Bound: KV TTL ≤ 60 s.
2. **Cold path** — verify misses the session cache and consults Neon `pat.revoked_at IS NULL`. Bound: regional Postgres commit visibility ≤ 100 ms.
3. **Cross-region propagation** — the revoke originates in region A and must reach the verify path in regions B, C, D, E. Bound: ≤ 60 s p99.

A canonical surface needs to coordinate three Cloudflare primitives (Durable Object for region-pinned revocation list; D1/Neon for the transactional source-of-truth; CF Queue for cross-region broadcast) plus the WI-S02-005 KV adapter for the session-cache invalidate hook.

## Decision

### D1 — Neon `pat.revoked_at` is the canonical SoT

`auth_model.md §5` + `data_model.md §4.1` declare Neon `pat.revoked_at IS NULL` as the authoritative filter that verify-path implementations consult. The Durable Object storage and the KV session cache are **hot-path optimisations**, never sources of truth (INV-AUTH-NEON-IS-SOT). Concrete consequences:

- The verify cold path queries Neon directly; a DO-storage outage does NOT compromise correctness.
- A revoke MUST atomically commit the `pat` UPDATE + the `audit_outbox` INSERT in a single Postgres transaction (re-using the WI-S01-005 atomic-batch pattern); the canonical idempotent shape is `UPDATE pat SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL RETURNING revoked_at` — an empty `RETURNING` indicates the row was already revoked and downstream side effects MUST be skipped (preserves INV-AUTH-REVOCATION-IDEMPOTENT).
- The DO `is_revoked()` query exists for the admin / audit query plane (S-13 forward), not the verify hot path.

### D2 — Durable Object as region-pinned broadcast cache + idempotent ingest

Each region runs one `RevocationDo` with persistent storage holding `(pat_id) → RevokedEntry`. The DO surface (RPC) exposes:

- `revoke(req)` — runs the orchestrator's local-region steps (Neon UPDATE + DO upsert + KV invalidate + Queue enqueue).
- `ingest_remote(entry)` — called by the CF Queue consumer in peer regions; idempotent upsert keyed by `(pat_id, revoked_at)` to satisfy at-least-once delivery (INV-AUTH-PROPAGATION-AT-LEAST-ONCE).
- `is_revoked(pat_id)` — admin query path only.

Why DO and not D1 alone: DO offers single-writer-per-region semantics + push-based broadcast capability + persistent WAL across migration; D1 alone would require multi-writer race resolution + polling. Combined: D1 = transactional truth, DO = real-time orchestration.

### D3 — CF Queue for cross-region fan-out (not synchronous DO RPC)

Synchronous DO RPC fan-out (region A → 4 RPC calls → `await` all 4 acks) is fragile under partition + costs ~4 × region-RTT at p99 before retries. CF Queue with at-least-once delivery + consumer-side dedup absorbs partition + producer outages gracefully:

- Queue producer at region A enqueues a single message after the local revoke commit; the orchestrator returns to the caller within ~100 ms p99 (Phase 1 SLA).
- Queue consumer drains async, calling `DO_peer.ingest_remote(entry)` for each peer region.
- Idempotent ingest tolerates duplicate delivery (at-least-once).
- DLQ + 5× retry policy + SEV-2 alert at sustained `dlq_size > 0`.

The trade-off (queue latency adds 5–30 s vs sync RPC's ~200 ms p99) is acceptable given the 60 s SLO.

### D4 — Single SLA 60 s p99 — orthogonal axes, NOT additive

WI-S03-004 v1.0.0 shipped with prose claiming "combined 120 s" stale window, which the Lote 10.3bis P0 fix corrected: the three axes (hot TTL, cold D1 visibility, cross-region propagation) are independent — a given verify-path attempt is on exactly one axis, not all three. The customer-facing SLA is `max(60 s hot, 100 ms cold, 60 s cross-region) = 60 s p99 single SLA`.

### D5 — Mass-revoke two-phase

Per INV-AUTH-MASS-REVOKE-ATOMIC: Phase 1 is a single atomic Postgres UPDATE bounded by tenant_id (10 000 rows commit ≤ 5 s); Phase 2 is chunked audit_outbox INSERT (`MASS_REVOKE_OUTBOX_CHUNK_SIZE = 1000`) + chunked Queue broadcast (`MASS_REVOKE_BROADCAST_BATCH_SIZE = 100`). Phase 2 may partially commit; eventual completeness ≤ 5 min via at-least-once + consumer dedup. Per-tenant rate limit `MASS_REVOKE_RATE_PER_TENANT_PER_SEC = 100` caps queue backpressure.

## Consequences

**Positive**:
- Fail-closed verify path (Neon SoT); a DO/KV outage never lets a revoked token through.
- Bounded propagation latency with explicit alert tiers (SEV-2 > 30 s, SEV-1 > 60 s).
- Atomic mass-revoke with auditable per-row events.
- Trait-abstraction defer pattern keeps the host-side property tests realistic + the CF binding shims small.

**Negative**:
- Three storage planes (Neon + DO + KV) need a daily reconciliation Cron to detect drift > 5 min.
- Cross-region propagation lag can exceed 60 s during CF Queue outages — graceful degradation is documented but customer-visible.

**Mitigations**:
- Reconciliation Cron + RB-FM-REVOKE-DRIFT runbook.
- Tiered alerts + RB-FM-REVOKE-LAG runbook.
- Local revocation effective immediately (D1 commit ≤ 100 ms regional) — cross-region lag never compromises the origin region.

**Consequences addendum (2026-07-17, WP-D M26, non-normative):** on the LIVE D1-SoT path (see Reconciliation below), the container's `NativePatGate` in-memory verify cache (`crates/corelink-container/src/native_pat_gate.rs`, `VERIFY_CACHE_TTL = 5s`) is an additional, in-region consumer of this ADR's propagation budget: a cache hit skips the D1 `revoked_at_ms IS NULL` check entirely for up to 5s. It is not one of the three axes above and is not counted toward the deferred Neon/DO/Queue design — it is bounded well within the single 60s p99 SLA regardless.

## Reconciliation vs shipped code (2026-07-12)

> ⚠️ **The Decision above is DESIGN-INTENT (DEFERRED / roadmap), not the deployed path.** The Neon-Postgres `pat.revoked_at` SoT + `RevocationDo` broadcast + CF-Queue cross-region fan-out backbone is NOT wired in production. The Rust surface (`crates/corelink-worker/src/auth/revocation.rs`) is feature-gated behind `tower-middleware` and is not in the verify hot path; its only callers today are the in-memory fakes (see the Implementation-seam note below).
>
> **The LIVE PAT-revocation source-of-truth is D1.** The container verifier checks `revoked_at_ms IS NULL` against the D1 `pat` table (migration `0063_pat_customer_keys`) on every verify — see `crates/corelink-container/src/adapter_pat.rs` — enforced under **INV-PAT-REVOKE-PROPAGATION** (`invariant_registry.md §3.28`) and TLA-verified by `specs/tla/auth_pat_revoke.tla` (which models the D1 `revoked_at IS NULL` verify path, no edge-cache lookahead). Revocation IS enforced in production today (via D1); everything in the Decision/Consequences above — the Neon SoT, the DO broadcast, the CF-Queue fan-out, the three-axis ≤ 60 s p99 SLA, and `INV-AUTH-NEON-IS-SOT` — describes the target architecture, not the running system. The deferred Neon queue-side propagation design is modelled (spec-only) by `specs/tla/auth_revocation.tla`.

## Cross-references

- `auth_model.md §5` — revocation lifecycle canonical narrative.
- `data_model.md §4.1` — Neon `pat.revoked_at` as SoT.
- `resilience_patterns.md PAT-INVALIDATE-001` — propagation SLO contract.
- `slo_catalog.md SLO-FRESH-PAT-REVOKE` — 60 s p99 single SLA.
- `invariant_registry.md §3.14` — INV-AUTH-REVOCATION-IDEMPOTENT, INV-AUTH-REVOCATION-SLO-60S, INV-AUTH-NEON-IS-SOT, INV-AUTH-MASS-REVOKE-ATOMIC, INV-AUTH-PROPAGATION-AT-LEAST-ONCE.
- `WI-S03-004` — implementation contract.

## Implementation seam (post-SEAL)

The Rust surface lives in `crates/corelink-worker/src/auth/revocation.rs` behind the `tower-middleware` feature flag. Four canonical traits — `RevocationStore`, `MetaRevocationSink`, `SessionCacheInvalidator`, `RevocationBroadcast` — abstract the four Cloudflare primitives so host-side property tests exercise the same orchestration the production binding shims will. The shims (real `worker::DurableObjectStorage`, `worker::Queue`, Neon connection pool wrapper) land alongside the host-server wiring WI; until then the in-memory fakes (`InMemoryRevocationStore`, `InMemoryMetaRevocationSink`, `KvSessionCacheInvalidator<InMemoryKv>`, `InMemoryBroadcast`) are the only callers.

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-30 | Gustavo (via Claude Opus 4.7) | First publication alongside WI-S03-004 SEAL. |
| 1.0.1 | 2026-07-12 | Gustavo (via Claude Opus 4.8) | Reconciliation note (non-normative): Neon-SoT / DO / CF-Queue backbone marked DEFERRED (feature-gated, not deployed); LIVE revocation SoT is D1 (`adapter_pat.rs`, migration 0063) under INV-PAT-REVOKE-PROPAGATION. Decision body unchanged (design-intent record). |
