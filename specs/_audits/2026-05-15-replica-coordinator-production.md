---
id: "AUDIT-REPLICA-COORDINATOR-PROD-2026-05-15"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-PREP wave-15 (multi-region replication coordinator production)"
parent_wi: "R-PREP-REPL-COORD-PROD"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "replication", "coordinator", "failover", "split-brain", "tla", "dr-16", "r-prep", "wave-15"]
---

# Multi-Region Replication Coordinator — Production-grade promotion + failback

> **doc_status:** REVIEW · **scope:** ship a production-grade replication
> coordinator that consumes the DEBT-011 wave-11+13 SLI fabric
> (R2/D1/KV/Neon) + the wave-14 DR-16 failover-router, exposes a
> singleton-DO-lock-modelled promotion sequence with fail-CLOSED audit,
> a canonical 24 h hot-standby failback cool-down, and a
> `replication_status()` API for `/health` + customer dashboards.
> Closes the "coordinator is partially fake" gap left after wave-13.

---

## 1. Context

DEBT-011 wave 11+13 closed ten R-prep replication tickets (P0+P1+P2)
covering the canonical replication-lag SLI surface across the four
data domains:

- R2 hot blobs (`SLO-REPLICATION-LAG-R2`, p99 ≤ 60 s)
- D1 read replicas (`SLO-REPLICATION-LAG-D1`, p99 ≤ 60 s)
- KV cross-region propagation (`SLO-REPLICATION-LAG-KV`, p99 ≤ 60 s typical)
- Neon read replicas (`SLO-REPLICATION-LAG-NEON`, p99 ≤ 5 s soft / informational)

The DR-16 wave-14 failover router (`corelink-failover-router`) ships
the request-path side: multi-signal health detection (5xx + latency +
consecutive failures), per-region health snapshot, sibling-region read
routing, and write blocking during failover. Wave-14 also pinned the
24 h hot-standby canonical pattern in
`specs/03_architecture/resilience_patterns.md §Failback` and proved
INV-FAILOVER-NO-SPLIT-BRAIN at the *write-lease* layer in
`specs/tla/failover_no_split_brain.tla`.

The remaining gap before DR-16 production-mode dry-run is the
**coordinator** layer: a singleton orchestrator that maintains
per-region role state (PRIMARY / HOT_STANDBY / REPLICA), routes writes
to the current primary, monitors all four SLO domains under load,
makes the failover decision, executes the role flip with audit-emit-
BEFORE-mutation fail-CLOSED, blocks failback for the canonical 24 h
cool-down, and serves a `replication_status()` view to consumers.

## 2. Deliverable

**New crate**: `crates/corelink-replication-coordinator/`

`#![forbid(unsafe_code)]`, no `tokio` in `src/`, no `unwrap`/`expect`/
`panic` outside test targets, every public enum `#[non_exhaustive]`,
PROPTEST_CASES env override per S-07 P1-2 nightly gate, audit
fail-CLOSED on every region state mutation.

### 2.1 Module surface

| Module | Trait / Type | Responsibility |
|---|---|---|
| `state` | `RegionRole` (`Primary` / `HotStandby` / `Replica`) | 3-canonical role taxonomy; `HOT_STANDBY_COOLDOWN_SECONDS` = 86_400 |
| `heartbeat` | `HeartbeatRegistry` + `InMemoryHeartbeatRegistry` | Per-region heartbeat tracking; `HEARTBEAT_STALE_SECONDS` = 60 |
| `lag` | `LagBundle` + `SLO_REPLICATION_LAG_{R2,D1,KV,NEON}_SECONDS` | 4-domain frozen lag observation; `within_slo()` excludes Neon (soft) |
| `audit` | `CoordinatorAuditSink` + 4-event taxonomy | `region_promoted.v1` / `region_demoted.v1` / `failback_blocked.v1` / `failback_committed.v1` |
| `error` | `CoordinatorError` `#[non_exhaustive]` | `UnknownRegion` / `NoEligibleReplica` / `SplitBrainRejected` / `PrimaryStillEligible` / `CooldownNotElapsed` / `Audit` / `Internal` |
| `coordinator` | `ReplicationCoordinator` + `InMemoryReplicationCoordinator` | Decision tree + role-map mutation + `replication_status()` |

### 2.2 Decision tree

```text
evaluate(primary, now):
  if heartbeat(primary) fresh AND lag(primary) within SLO:
    -> KeepPrimary
  else:
    for each replica in deterministic Region::ALL order:
      if heartbeat(replica) fresh AND lag(replica) within SLO:
        -> PromoteReplica(replica)
    -> NoEligibleReplica            # escalate to runbook (DO NOT auto-promote)

promote(primary, replica, now):
  ACQUIRE singleton lock (modelled = per-instance Mutex; prod = DO singleton ID)
  if any region != primary holds Primary:
    -> SplitBrainRejected           # INV-FAILOVER-NO-SPLIT-BRAIN
  if primary still eligible:
    -> PrimaryStillEligible         # anti-flap guard
  if replica not eligible:
    -> NoEligibleReplica
  emit_audit(region_demoted.v1, primary)   # fail-CLOSED
  emit_audit(region_promoted.v1, replica)  # fail-CLOSED
  state.role[primary]  = HotStandby; cooldown_started[primary] = now
  state.role[replica]  = Primary

failback(region, now):
  if role[region] != HotStandby -> Internal (fail-CLOSED)
  if now - cooldown_started[region] < 24h:
    emit_audit(failback_blocked.v1)
    -> CooldownNotElapsed
  emit_audit(failback_committed.v1)
  demote current Primary to Replica
  state.role[region] = Primary
```

### 2.3 INV-FAILOVER-NO-SPLIT-BRAIN guarantees

Three layered defences:

1. **Mutex linearization (production = Cloudflare DO singleton)** —
   every role mutation acquires the per-instance state lock; the lock
   is held across the audit-emit + state-flip pair so concurrent
   callers cannot race a second primary into the topology.
2. **Explicit primary check** — before any mutation, `promote`
   enumerates the role map and returns `SplitBrainRejected` if any
   region other than the caller-specified `primary` arg already holds
   `RegionRole::Primary`.
3. **Audit-emit-BEFORE-mutation fail-CLOSED** — both
   `RegionDemoted` and `RegionPromoted` audits emit before the role
   map flip. If either emit fails, the state is **NEVER** mutated and
   the caller observes `CoordinatorError::Audit(detail)`.

Property-test `tests/prop_coordinator.rs::at_most_one_primary` drives
2 000 (`PROPTEST_CASES` overridable) randomized
promote/failback/clock-tick sequences and asserts the invariant after
EVERY step. Verified.

### 2.4 24 h hot-standby cool-down (canonical failback)

Source: `specs/03_architecture/resilience_patterns.md §Failback`.
The demoted primary stays in `HotStandby` for
`HOT_STANDBY_COOLDOWN_SECONDS = 86_400` seconds before
`ReplicationCoordinator::failback` may re-promote it. Premature
attempts return `CoordinatorError::CooldownNotElapsed { elapsed_seconds,
remaining_seconds }` (forensic detail for the runbook + dashboard) AND
emit a `failback_blocked.v1` audit record.

Property-test `tests/prop_coordinator.rs::cooldown_always_blocks_under_24h`
fuzzes `elapsed_s ∈ [0, 86_399]` and asserts every failback attempt is
blocked. Verified.

## 3. TLA+ spec — `specs/tla/replica_failover.tla`

Models the COORDINATOR-level 3-state role machine (PRIMARY /
HOT_STANDBY / REPLICA), the `Promote` atomic step (single TLA+
transition models the singleton DO lock holding the demote +
promote pair atomic), the `Failback` cool-down guard, and the
`MarkHealthy`/`MarkUnhealthy` adversary.

**Invariants verified**:

- `InvAtMostOnePrimary` — INV-FAILOVER-NO-SPLIT-BRAIN (CRITICAL),
  strictly generalizes the wave-12 lease-handoff guarantee.
- `InvCooldownTrackedForHotStandby` — every HOT_STANDBY region has
  a recorded `cooldown_started` (the 24h gate is well-defined).
- `InvCooldownNotInFuture` — `cooldown_started <= clock` (monotonic
  clock + Promote is the only writer).
- `InvRoleCanonical` — every region role is in the 3-canonical set.
- `InvHealthyWellFormed` — health set ⊆ Regions.

**Lanes**:

- PR lane: `replica_failover.cfg` (2 regions, `CooldownTicks = 2`,
  `MaxOps = 8`; finishes well under 2 min on a 4-core laptop).
- Nightly lane: `replica_failover_nightly.cfg` (3 regions,
  `CooldownTicks = 3`, `MaxOps = 14`).

The spec is independent of and complementary to
`failover_no_split_brain.tla` (wave-12), which models the *write-lease
side* of split-brain. This new spec models the *coordinator role
side*; together they cover both layers of the GA failover stack.

## 4. End-to-end harness — `tests/e2e-replication-failover/`

Six scenarios (one test fn each), all `cargo test`-driven, byte-for-
byte reproducible under a deterministic logical clock (no `tokio`):

| # | Scenario | Validates |
|---|---|---|
| 1 | `scenario_1_primary_fail_then_failback_after_24h` | Full lifecycle: stale heartbeat → evaluate → promote → cool-down blocks → failback after 24h. Asserts 4-record audit trail. |
| 2a | `scenario_2a_split_brain_at_registration_rejected` | Two regions claim Primary at registration → `SplitBrainRejected`. |
| 2b | `scenario_2b_split_brain_at_promote_rejected` | Caller asserts wrong demotion target while another region holds Primary → `SplitBrainRejected`. |
| 3 | `scenario_3_replica_lag_slo_breach_triggers_reroute` | Primary heartbeat fresh BUT R2 lag breaches SLO → write refused + `PromoteReplica` decision. |
| 4 | `scenario_status_dashboard_view` | `replication_status()` reflects exactly one Primary in healthy mode, flips to `unhealthy` when primary heartbeat stales. |
| 5 | `scenario_no_eligible_replica_escalates_to_runbook` | All regions breach SLO → `NoEligibleReplica` (do NOT auto-promote). |

All 6 pass deterministically.

## 5. Quality gates

| Gate | Status |
|---|---|
| `cargo build -p corelink-replication-coordinator` | green |
| `cargo build -p e2e-replication-failover` | green |
| `cargo clippy -p corelink-replication-coordinator --tests -- -D warnings` | green (no warnings) |
| `cargo clippy -p e2e-replication-failover --tests -- -D warnings` | green |
| `cargo test -p corelink-replication-coordinator` | 29 unit + 2 proptest + 6 integration + 3 doctest = 40 / 40 |
| `cargo test -p e2e-replication-failover` | 6 / 6 |
| `python3 scripts/validate_specs.py` | green |
| `python3 scripts/validate_canonical_consistency.py` | green (tla_verified count not regressed) |

Charter constraints upheld:

- `#![forbid(unsafe_code)]` on both new crates.
- No `unwrap` / `expect` / `panic` in `src/` (only in test targets,
  gated by per-block `#[allow]`).
- No `tokio` import in `src/`.
- All public enums `#[non_exhaustive]`.
- PROPTEST_CASES env override implemented (default 2 000).
- Audit emit fail-CLOSED — both `promote` and `failback` emit BEFORE
  any state mutation; `FailingCoordinatorAuditSink` integration test
  verifies state UNCHANGED on emit failure.
- Singleton lock held across promotion sequence (Mutex in tests,
  Cloudflare DO singleton in production).

## 6. Production wiring (deferred per autonomous-execution charter
   `trait-abstraction-defer` clause)

The following production bindings remain `trait-abstraction-defer`
and are tracked as separate WI candidates for the staging dry-run
follow-on:

- Heartbeat registry binding: Cloudflare Durable Object that
  records replica-worker tick metadata in D1 (15 s P0).
- Audit sink binding: append `corelink.failover.region_promoted.v1`
  + `region_demoted.v1` + `failback_committed.v1` to the
  CloudEvents bus → `audit_outbox` D1 table (consumed by the
  audit-chain crate per S-06).
- Singleton lock binding: Cloudflare Durable Object singleton ID
  (replaces the per-instance `Mutex`).
- `replication_status()` HTTP binding: `/v1/health/replication`
  endpoint surfacing the snapshot to customer dashboards.

Until the DO singleton wiring lands, the `InMemoryReplicationCoordinator`
is **production-shipped** behind a feature flag for staging dry-run +
DR-16 active-failover drill rehearsal. Real production traffic still
goes through the wave-14 `corelink-failover-router` for request-path
failover; the coordinator drives the back-plane orchestration.

### 6.1 Future work — DR-16 drill rehearsal closure (wave-16)

The four `trait-abstraction-defer` bindings above are **exercised
end-to-end and validated by the DR-16 quarterly drill rehearsal**
spec'd at:

- **Operator runbook**: `specs/_runbooks/RB-REPLICA-FAILOVER.md`
  (wave-16; 8 sections: Detect → Pre-checks → Promote → Verify →
  Failback after 24 h → Communication → Rollback → Post-incident
  review). Companion to `RB-ACTIVE-FAILOVER.md`; both runbooks fire
  together in real DR-16 incidents (ordering pinned in §1.4 of the
  replica-failover runbook: coordinator-layer flip BEFORE
  request-path flip — the reverse would create INV-FAILOVER-NO-SPLIT-BRAIN
  violation at the coordinator layer).
- **Drill spec WI**: `specs/04_sprints/S17/work_items/WI-S17-008-active-failover-drill.md`
  (wave-16; quarterly cadence, ≥ 1 promote+failback round/quarter,
  MTTA ≤ 5 min, MTTR ≤ 30 min, zero SEV-1 from drill, INV-FAILOVER-NO-SPLIT-BRAIN
  + INV-REGION-NO-CROSS-LEAK assertions at every phase boundary,
  full `BCP-DR-DRILL-CADENCE.md §DR-16` alignment). Tests: TLA+
  `replica_failover.tla` nightly + `prop_coordinator::at_most_one_primary`
  2000 cases + new E2E scenario 7 `scenario_7_quarterly_drill_dry_run`.

Closure of the `trait-abstraction-defer` items above lands as part
of WI-S17-008 execution (the drill rehearsal IS the integration test
for those production wirings — see WI-S17-008 §21 Soft blockers).

## 7. Cross-links

- Source audit (DEBT-011): `specs/_audits/2026-05-15-replication-audit.md`
- Follow-up ticket dispatch: `specs/_audits/replication-followup-tickets.md`
- Resilience patterns: `specs/03_architecture/resilience_patterns.md §Failback`
- Failover write-lease TLA spec (wave-12): `specs/tla/failover_no_split_brain.tla`
- New coordinator TLA spec (this audit): `specs/tla/replica_failover.tla`
- Wave-14 failover router: `crates/corelink-failover-router/`
- ROADMAP entry: `ROADMAP-TO-GA.md §6` (Wave R-6 BCP/DR cadence)

## 8. Branch / commit

- Branch: `wt/r-prep-replica-coordinator-prod`
- Worktree: `.claude/worktrees/agent-abfbf16ad645ff29a/`
- Commit: see git log on this branch.
