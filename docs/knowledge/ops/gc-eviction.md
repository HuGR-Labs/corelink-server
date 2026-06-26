---
type: "Runbook"
title: "GC / eviction operations"
description: "How CoreLink reclaims storage safely: the GC mark→sweep→delete→reconcile phase machine, the gc-pause degrade-mode kill-switch, the per-tier eviction TTLs + 95% quota trigger, and the mandatory 10→50→100% production rollout."
source_files:
  - "crates/corelink-gc/src/run.rs"
  - "crates/corelink-gc/src/degrade.rs"
  - "crates/corelink-eviction/src/tier.rs"
  - "crates/corelink-eviction/src/reservation.rs"
  - "crates/corelink-eviction/src/trigger.rs"
  - "docs/internal/gc-prod-rollout-plan.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "gc", "eviction", "storage", "runbook", "rollout"]
timestamp: "2026-06-26T00:00:00Z"
---

# GC / eviction operations

CoreLink reclaims storage along two independent but complementary planes: **garbage collection**
(`corelink-gc`) walks the reachable set and soft-deletes orphaned blobs through a strict phase machine,
while **eviction** (`corelink-eviction`) enforces per-tier retention TTLs and a 95%-quota pressure
trigger. Both ship today as pure-logic skeletons with in-memory fakes (the production Cloudflare Cron
Durable Object wiring is the deferred PRR ship gate), so the load-bearing reality an operator must
understand is the *state machine + the kill-switch + the rollout gates* — not yet a live cron. This
runbook is the operator's map of those invariants and the mandatory gradual production rollout that
governs turning GC on. The reclaim math it protects is the same per-tenant accounting behind the
[$-ceiling](/tenancy/dollar-ceiling.md) and the [storage-quota header](/tenancy/storage-quota-header.md).

# Role
- The safe-reclaim control surface: GC decides *which* bytes are unreachable and eligible to delete,
  eviction decides *when* retained bytes age out or when quota pressure forces a reclaim.
- The emergency stop: a config-singleton degrade-mode flag the operator flips to halt new GC spawn
  during an incident (`crates/corelink-gc/src/degrade.rs:35`).
- The rollout governor: a non-waivable 10→50→100% canary sequence per the S-06 production rollout plan.

# How it works
1. GC runs a fixed phase machine `Idle → Mark → Sweep → PhysicalDelete → Reconcile → Completed`, encoded
   as the `GcPhase` enum (`crates/corelink-gc/src/run.rs:48-64`).
2. Only those forward edges (plus any phase → `Failed`) are legal transitions; `Failed`/`Completed` are
   terminal with no outbound edge (`crates/corelink-gc/src/run.rs:105-115`).
3. Degrade-mode has three kinds — `Off` / `GcPause` / `GcReadOnly` — defined as the `DegradeKind` enum
   (`crates/corelink-gc/src/degrade.rs:30-38`), each with a stable mnemonic for the metric label
   (`crates/corelink-gc/src/degrade.rs:45-50`).
4. `GcPause` is the hard stop: a running worker must abort at the next batch boundary, which the
   `requires_abort` predicate encodes as matching only `GcPause` (`crates/corelink-gc/src/degrade.rs:59`).
5. Both `GcPause` and `GcReadOnly` block *new* runs from spawning, per the `blocks_new_runs` predicate
   (`crates/corelink-gc/src/degrade.rs:65`).
6. Eviction resolves a retention TTL per the 5-tier `Tier` enum (`crates/corelink-eviction/src/tier.rs:51-61`)
   through the `ttl_for_tier` resolver (free=7d, solo=30d, team=90d, business/enterprise=365d)
   (`crates/corelink-eviction/src/tier.rs:89-93`).
7. The size-proportional reservation TTL clamps between a 60s floor and a 7d ceiling so a large multipart
   upload never expires mid-write (`crates/corelink-eviction/src/reservation.rs:77-81`).
8. The 95% quota trigger fires (boundary-inclusive) when `bytes_used / bytes_quota >= 0.95`, computed by
   `should_fire_quota_trigger` (`crates/corelink-eviction/src/trigger.rs:61-62`), and the reclaim target
   is `bytes_used - 0.90 × bytes_quota` so it drains back to ≤90% (`crates/corelink-eviction/src/trigger.rs:82-83`).
9. Production turn-on is gated: phase 1 is a 10% canary (tenant-id hash mod 10, all 5 regions), advancing
   only on a 24h SEV-0/1-free window with refcount drift <0.1% (`docs/internal/gc-prod-rollout-plan.md:34-49`).
10. Phase 2 is 50% (hash mod 2), advancing on a 48h clean window; phase 3 is 100% with a 7d clean window
    before GA can be declared (`docs/internal/gc-prod-rollout-plan.md:50-75`).

# Invariants
- The GC phase machine is monotone forward — no skipping and no backward edge; only the encoded edges or
  a `Failed` transition are legal (`crates/corelink-gc/src/run.rs:105-115`).
- Probe failure is fail-closed *by design contract*: an unprobeable degrade state MUST be treated as
  `GcPause`, never silently proceeding. This is the DESIGNED degrade contract — a caller-side MUST stated on
  the `DegradeProbe::probe` trait doc and the module header (`crates/corelink-gc/src/degrade.rs:14-19`,
  `crates/corelink-gc/src/degrade.rs:117-121`), NOT yet an in-crate enforcer (the caller that must honour it
  is the deferred cron wiring). What IS enforced in-crate is the consequence: `GcPause` forces abort within
  one batch boundary via `requires_abort` (`crates/corelink-gc/src/degrade.rs:59`).
- The enterprise TTL admin override is hard-capped at 730 days; a longer override is rejected by
  `ttl_for_tier_with_override` with `ExceedsMaxTtl` (`crates/corelink-eviction/src/tier.rs:145-148`).
- The reservation TTL never drops below the 60s floor nor exceeds the 7d cap, bounding both tiny and
  pathological uploads (`crates/corelink-eviction/src/reservation.rs:50`, `crates/corelink-eviction/src/reservation.rs:54`).
- A zero `bytes_quota` is defended: the trigger returns below-threshold rather than firing forever
  (`crates/corelink-eviction/src/trigger.rs:61-62`).
- Direct 100% rollout is forbidden — the gradual 10→50→100% sequence is mandatory and any SEV-0 trips an
  immediate `gc-pause` rollback (`docs/internal/gc-prod-rollout-plan.md:64-82`).

# Gotchas
- Activating `gc-pause` stops new spawn but does NOT abort in-flight runs mid-batch — they finalize to an
  `Aborted` audit; a hard mid-run kill could leave `gc_run` rows inconsistent, so it is deliberately
  batch-boundary scoped (`docs/internal/gc-prod-rollout-plan.md:84-88`).
- The 5 rollout regions (`sam/iad/lhr/nrt/syd`) are colo strings, distinct from the macro `Tier` vocabulary
  — do not conflate a region cohort with a tenant tier.
- Both crates are pure-logic skeletons today: the real D1 `blob_meta.deleted_at` UPDATE + cron DO binding
  land at the PRR ship gate, so "GC is wired" means the invariants are proven against fakes, not that a
  live cron is reclaiming prod bytes yet.

# Citations
1. `crates/corelink-gc/src/run.rs:48-64` — the `GcPhase` enum: Idle/Mark/Sweep/PhysicalDelete/Reconcile/Completed.
2. `crates/corelink-gc/src/run.rs:105-115` — `can_transition_to`: the only legal forward + `Failed` edges.
3. `crates/corelink-gc/src/degrade.rs:30-38` — `DegradeKind` enum (Off / GcPause / GcReadOnly).
4. `crates/corelink-gc/src/degrade.rs:45-50` — `as_str` canonical mnemonics for the metric label.
5. `crates/corelink-gc/src/degrade.rs:59` — `requires_abort` matches only `GcPause` (the hard stop).
5b. `crates/corelink-gc/src/degrade.rs:14-19`, `crates/corelink-gc/src/degrade.rs:117-121` — the fail-closed-probe DESIGN contract (module + `DegradeProbe::probe` trait doc: unprobeable ⇒ caller MUST treat as `GcPause`); a caller-side MUST, NOT an in-crate enforcer.
6. `crates/corelink-gc/src/degrade.rs:65` — `blocks_new_runs` matches `GcPause | GcReadOnly`.
7. `crates/corelink-eviction/src/tier.rs:41` — `MAX_ENTERPRISE_TTL_DAYS = 730` override cap.
7b. `crates/corelink-eviction/src/tier.rs:145-148` — `ttl_for_tier_with_override` rejects a longer override with `ExceedsMaxTtl`.
8. `crates/corelink-eviction/src/tier.rs:51-61` — the 5-value `Tier` enum.
9. `crates/corelink-eviction/src/tier.rs:89-93` — `ttl_for_tier` per-tier TTL resolution.
10. `crates/corelink-eviction/src/reservation.rs:50` — `MIN_RESERVATION_TTL_MS = 60_000` floor.
11. `crates/corelink-eviction/src/reservation.rs:54` — `MAX_RESERVATION_TTL_MS = 7 × 86_400_000` cap.
12. `crates/corelink-eviction/src/reservation.rs:77-81` — the floor/cap clamp of the proportional TTL.
13. `crates/corelink-eviction/src/trigger.rs:61-62` — `should_fire_quota_trigger` 95% boundary + zero-quota defense.
14. `crates/corelink-eviction/src/trigger.rs:82-83` — `target_bytes_to_reclaim` drains back to ≤90%.
15. `docs/internal/gc-prod-rollout-plan.md:34-49` — phase 1 10% canary scope + advance gates.
16. `docs/internal/gc-prod-rollout-plan.md:50-75` — phase 2 (50%) + phase 3 (100%) gates.
17. `docs/internal/gc-prod-rollout-plan.md:64-82` — gradual-rollout mandate + rollback triggers/table.
18. `docs/internal/gc-prod-rollout-plan.md:84-88` — `gc-pause` stops spawn but not in-flight runs (Aborted audit).
