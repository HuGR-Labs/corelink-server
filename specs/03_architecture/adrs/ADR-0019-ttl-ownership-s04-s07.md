---
id: "ADR-0019"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "ownership", "ac-ttl", "eviction", "tier"]
---

# ADR-0019 — AC TTL ownership: S-07 supersedes S-04 (per-tier TTL)

## Context

Round 2 audits (Opus C-03 + Codex CF-05) detectaram ownership clash entre S-04 e S-07:

- `S-04` entrega `CAP-AC-004` "AC TTL management" com **default 90d, refresh on hit** (R-S04-5).
- `S-07` entrega `CAP-EVICT-002` "TTL-based AC entry expiry **per-tier**" (free=7d, solo=30d, team=90d, business=365d, enterprise=customer-configurable).

Customer free criado em S-04 staging → AC entries com prometida vida útil de 90d. Quando S-07 ship, esse mesmo customer free passa a ter TTL 7d (12.8× redução). **Sem migration plan; sem ADR; sem override declaration**.

Pior, sprint contracts publicados em `docs.corelink.humangr.com/sla` (S-18) prometem tier semantics — divergência entre S-04 e S-07 cria contract violation público.

## Decision

**S-07 supersedes S-04 CAP-AC-004 TTL semantics.** A canonical truth pós-S-07 SEALED é a tabela per-tier:

| Tier | AC TTL default |
|---|---|
| Free | 7d |
| Solo | 30d |
| Team | 90d |
| Business | 365d |
| Enterprise | Customer-configurable (default 365d, max 730d) |

S-04 mantém entrega de **CAP-AC-004 infrastructure** (TTL worker, refresh on hit, expiry detection); S-07 sobrescreve apenas o **default value**.

## Migration plan

**Execution scope amendment** (Lote 10.7-tris cycle 4 alignment): migration EXECUTION deferred to S-13 admin plane (config rollout) + S-13 notification surface (email infra), matching ADR-0020 email defer pattern. S-07 owns RUNTIME TTL resolution (`ttl_for_tier` + per-tier defaults wired into eviction worker); S-13 owns ROLLOUT mechanics (DO config-singleton push + 14d notification + grace handling). Audit event `corelink.ac.tier_migration` is emitted by S-13 migration worker (not S-07 eviction worker) — separation of concerns.

Phased execution (S-13 owned):
1. Pre-deploy: existing tenants têm TTL 90d default (S-04 setup).
2. S-07 deploy (S-13 config-singleton push): rollout per-tier defaults via DO config (S-13 admin plane).
3. Free tenants com entries ativas > 7d: 14d notification email + grace period (S-13 notification worker per ADR-0020 email infra defer pattern).
4. Pós-grace: TTL aplicado per tier; entries expiradas seguem normal eviction flow (S-07 eviction worker consumes config).
5. Audit trail: S-13 migration worker emits `corelink.ac.tier_migration` CloudEvent per migration event.

S-07 deliverable scope: TTL config consumption + eviction flow respects new tier defaults (already covered by WI-S07-002 §6.1.3 `ttl_for_tier`). Migration orchestration is S-13 deliverable scope (ADR-0019 cycle 4 alignment).

## Consequences

**Positive:**
- Single source of truth para AC TTL (S-07 + ADR + S-18 docs).
- Customer trust preserved via 14d notification.
- INV-AC-OUTPUTS-VALID mantido (entries expired via flow normal).
- Pricing tier semantics enforce-able.

**Negative:**
- Customer free users perdem 12.8× cache lifetime — **mitigado** com clear pricing page + upgrade path.
- Migration adiciona ~2 semanas de complexity (notification + grace).

## Alternatives considered

- **Manter S-04 default 90d, S-07 overrides apenas per opt-in tier**: rejected — customer free com 90d em produção é cost overhead (R2 storage); pricing tier semantics dictam shorter TTL para free.
- **S-04 elimina CAP-AC-004; S-07 entrega tudo**: rejected — S-04 já SEALED ou em iminent implementation; rework excessivo.
- **Per-customer override toda hora**: rejected — operational complexity sem benefício; tier-based é simpler + aligned com pricing model.

## Spec contract patches required

- `S-04 _spec_contract.md`: adicionar `local_deltas: ["CAP-AC-004 TTL default semantics overridden by S-07 CAP-EVICT-002 (ADR-0019)"]`.
- `S-07 _spec_contract.md`: já reflete; adicionar reference explícita a ADR-0019.
- `S-18 _spec_contract.md`: pricing page renderiza tabela canonical; CI gate verifica match com ADR.

## Boundary mechanism (WI-S04-005 implementation)

The boundary mechanism between S-04 (infrastructure) and S-07 (per-tier defaults) is the `TierTtlResolver` trait shipped at `crates/corelink-worker/src/reapi/ac/ttl/resolver.rs`:

```rust
pub trait TierTtlResolver: Send + Sync + fmt::Debug {
    /// Resolve the TTL delta (ms) for a tenant tier.
    fn resolve_ttl_ms(&self, tier: TenantTier) -> u64;
}
```

S-04 GA wires `EnvConfigTierTtlResolver` (single global default; `CORELINK_AC_TTL_DEFAULT_MS` env override). S-07 SEALED swaps in `S07PerTierTtlResolver` (config-singleton table lookup keyed off `tenant_quota.tier`). The trait stays Send+Sync+Debug so the AC handler holds it as `Arc<dyn TierTtlResolver>`; swap-out is a binding-time decision.

The 5 canonical tiers (`Free`, `Solo`, `Team`, `Business`, `Enterprise`) are pinned in the `TenantTier` enum + canonical `as_str()` labels (`free`, `solo`, `team`, `business`, `enterprise`) matching the `tenant_quota.tier` SQL enum. `MockTierTtlResolver::s07_canonical()` returns the post-S-07 value table for property tests verifying the boundary swap leaves cron worker behavior unchanged.

## TTL infrastructure delivery (WI-S04-005)

**S-04 deliverables** (this WI):

1. **Refresh-on-hit threshold gate** — `refresh_if_needed(now_ms, last_hit_at_ms, threshold)` pure-logic predicate; default 60s threshold (`DEFAULT_REFRESH_THRESHOLD_MS`); reduces D1 UPDATE storm under high-rate workloads (1k req/s on hot digest = 1k UPDATE/s without gate; ≤ 1 UPDATE/min per digest with gate).
2. **Tenant-scoped batched eviction** — `EvictBatch::run_one_batch(tenant_id, now_ms, limit, request_id)` drives R2-DELETE → D1-DELETE → audit-emit → KV-invalidate per row in canonical order; bounded at `MAX_BATCH_SIZE = 250` (D1 100KB batch ceiling).
3. **Per-region cron orchestrator** — `TtlWorker` trait + `InMemoryTtlWorker` fake; round-robin sweep across tenants with expired rows; `TtlWorkerTickOutcome::hit_row_ceiling` storm signal; per-region pinning enforced at SELECT + DELETE seams.
4. **Audit emission** — 3 new `AcEventType` variants (`EvictTtlExpired` / `EvictR2Failed` / `EvictD1Failed`) emitted per row; chains into `corelink-audit` outbox in production wiring (WI-S04-006).
5. **Trait-surface anti-DELETE-bulk** — `AcMetaStore::delete_tenant_scoped(tenant_id, action_digest, region)` is the only DELETE method; no "DELETE WHERE expires_at < ?" bulk method exists. Cross-tenant DELETE is structurally unreachable per `INV-AC-EVICT-TENANT-SCOPED`.

**Deferred (WI-S04-006)**:

- Real Cloudflare Cron Durable Object binding shim (alarm-based cron firing).
- `S07PerTierTtlResolver` impl (lands when S-07 SEALED).
- Migration worker emitting `corelink.ac.tier_migration` audit events (S-13 owned per ADR-0019 §Migration plan cycle 4).
- Real R2 envelope `delete` adapter (envelope_store trait surface lands alongside conformance suite).

## References

- `specs/04_sprints/S04/_spec_contract.md` (CAP-AC-004).
- `specs/04_sprints/S04/work_items/WI-S04-005-ttl-worker-cron-do-adr-0019.md` (TTL infrastructure delivery).
- `specs/04_sprints/S07/_spec_contract.md` (CAP-EVICT-002).
- Opus Round 2 C-03 (`specs/_audits/2026-04-24-opus-independent-sota-review-r2.md:63-71`).
- Codex Round 2 CF-05.
- `crates/corelink-worker/src/reapi/ac/ttl/resolver.rs` — `TierTtlResolver` trait + S-04 GA fallback impl.

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (Lote 9.1) | Criação ADR-0019 (TTL ownership boundary S-04 → S-07 supersedes per-tier defaults). |
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.7bis P0-5 fix) | **DRAFT → FROZEN promotion** (Agent R4 + Sonnet R5 caught: WI-S07-002/005 cited "ADR-0019 FROZEN" mas era DRAFT). Content audit by Owner: Decision §32 tier vocabulary (free/solo/team/business/enterprise — 5 tiers) + per-tier TTL semantics (7d/30d/90d/365d/730d max) + S-07 supersedes S-04 default 90d via this ADR. Architect + Crypto SME independent re-review optional (advisory; substantive content audit complete pre-FROZEN). |
| 1.1.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | **WI-S04-005 SEALED — TTL infrastructure delivered.** ADR ratified — boundary mechanism implemented at `crates/corelink-worker/src/reapi/ac/ttl/`: `TierTtlResolver` trait + `EnvConfigTierTtlResolver` S-04 GA fallback + `MockTierTtlResolver::s07_canonical()` for property-test boundary swap verification. `S07PerTierTtlResolver` real impl deferred to S-07 (lands without rebuilding S-04 cron worker per ADR §Boundary mechanism). New §Boundary mechanism + §TTL infrastructure delivery sections added documenting the runtime artifacts (refresh-on-hit gate; bounded batch helper; per-region cron orchestrator; trait-surface anti-DELETE-bulk; 3 new audit event types). |
