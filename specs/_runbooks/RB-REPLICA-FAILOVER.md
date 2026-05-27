---
id: "RB-REPLICA-FAILOVER"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "dr", "replica-failover", "replication-coordinator", "split-brain", "soc2-a1-2", "soc2-cc7-5", "rto-15m", "rpo-60s", "dr-16", "wave-15", "wave-16"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §6, §8.4.

# RB-REPLICA-FAILOVER — Coordinator-driven Primary/Replica Role Flip

> **Status:** ACTIVE. Companion to `crates/corelink-replication-coordinator/` (wave-15 `InMemoryReplicationCoordinator`), `specs/tla/replica_failover.tla` (✅ GREEN — `InvAtMostOnePrimary` proves INV-FAILOVER-NO-SPLIT-BRAIN at the coordinator layer), `specs/04_sprints/_sealed/S17/work_items/WI-S17-008-active-failover-drill.md` (quarterly drill spec), `specs/_compliance/BCP-DR-DRILL-CADENCE.md` §DR-16.
>
> **Purpose:** procedural runbook for the **back-plane coordinator-level** failover — the four-domain replication SLI (R2 hot blobs, D1, KV, Neon) shows the current `Primary` region is no longer eligible (heartbeat stale OR lag breaches `SLO-REPLICATION-LAG-{R2,D1,KV}` p99 budget) and the singleton coordinator must flip role state (Primary → HotStandby, eligible Replica → Primary) **with fail-CLOSED audit + 24 h failback cool-down**. Complements `RB-ACTIVE-FAILOVER.md` (which drives the **request-path / write-lease** flip via `corelink-failover-router`); both runbooks fire together for a real DR-16 incident — see §1.4.
>
> **RTO target:** ≤ 15 min (singleton-lock acquire + audit emit + role flip + cross-tenant probe, steps 1–6).
> **RPO target:** ≤ 60 s (per `SLO-REPLICATION-LAG-{R2,D1,KV}` p99 budget; pre-check gate refuses to proceed if any of the three breaches budget at T+0).
> **Failback (§5):** held by `HOT_STANDBY_COOLDOWN_SECONDS = 86_400` (24 h) hard floor in `corelink-replication-coordinator::state` — premature attempts return `CoordinatorError::CooldownNotElapsed` AND emit `failback_blocked.v1` audit.
>
> **DO NOT** invoke any state-changing command without `CONFIRM=I_UNDERSTAND_REPLICA_FAILOVER_PROD` env if `CORELINK_ENV=production`.
>
> **NOT the same as `RB-ACTIVE-FAILOVER.md`.** That runbook flips the write-lease in the request-path router. This runbook flips the **canonical role label** in the coordinator state machine. In an incident, this runbook drives Detect→Promote, then `RB-ACTIVE-FAILOVER.md` consumes the new role to redirect traffic. In the wave-15→wave-16 build-out, the coordinator singleton DO + audit-bus + `/v1/health/replication` HTTP binding remain `trait-abstraction-defer` — those are the production wirings DR-16 active-failover drill rehearsal exercises (see WI-S17-008).

## 1. Trigger conditions

This runbook is invoked when **any one** of the following holds for ≥ 5 min:

1. **Heartbeat stale on `Primary`** — `HeartbeatRegistry::is_fresh(primary, now)` returns `false` (age ≥ `HEARTBEAT_STALE_SECONDS = 60`); OR
2. **Replication-lag SLO breach on `Primary`** — any of `SLO-REPLICATION-LAG-R2`, `SLO-REPLICATION-LAG-D1`, `SLO-REPLICATION-LAG-KV` p99 exceeds 60 s sustained 5 min (Neon is soft / informational per coordinator `LagBundle::within_slo()`); OR
3. **Manual override** — IC declares early-drain after observed customer-visible degradation but before the 5-min sustain window closes (audit `override_reason` required, see §2.3).

### 1.1 Required alert signals

| Alert | Source | Threshold |
|---|---|---|
| `ReplicaHeartbeatStale-{region}` | `corelink-replication-coordinator::heartbeat` gauge | `corelink_replica_heartbeat_age_seconds{role="primary",region=<P>} > 60` for ≥ 5 min |
| `ReplicationLagR2HotSli` | `corelink_replication_lag_seconds{domain="r2_hot",primary_region,replica_region}` | `histogram_quantile(0.99, rate({metric}_bucket[1h])) > 60` (DEBT-011 P0-001 SLI; `slo_catalog.md §4.23`) |
| `ReplicationLagD1Sli` | `corelink_d1_replica_lag_seconds{primary_region,replica_region}` | `quantile_over_time(0.99, metric[1h]) > 60` (DEBT-011 P0-002 SLI; `slo_catalog.md §4.24`) |
| `KvPropagationLagSli` | `corelink_kv_propagation_lag_seconds{write_region,read_region}` | `histogram_quantile(0.99, rate({metric}_bucket[1h])) > 60` in ≥ 5% of region-pair samples (DEBT-011 P1-001 SLI; `slo_catalog.md §4.25`) |
| `ReplicaEligibleSiblingHealthy` | `replication_status()` API (gate, not alert) | `≥ 1` region in `ReplicationStatus::regions` reports `role=Replica`, `heartbeat_fresh=true`, `lag_within_slo=true` |
| `CoordinatorFailbackBlocked` | `corelink_replica_failback_blocked_total{reason="cooldown_not_elapsed"}` counter | `increase(metric[1h]) == 0` during stable ops; non-zero in a failback window = 24 h floor not elapsed, refer §5 |

### 1.2 Multi-burn-rate alert thresholds

Per `slo_catalog.md` multi-burn-rate canonical (1h fast-burn + 6h slow-burn):

| Burn lane | Window | Threshold for trigger |
|---|---|---|
| Fast | 1 h | error-budget consumption rate > 14.4× (2 % budget in 1 h) — page L1 immediately |
| Slow | 6 h | error-budget consumption rate > 6× (5 % budget in 6 h) — ticket L1; auto-escalate to L2 at 12 h |

A fast-burn alert alone is **not** sufficient to trigger this runbook — the heartbeat-stale OR SLO-breach gate in §1 MUST also hold. (Avoids paging on a transient burn that the coordinator's anti-flap guard would itself reject in `PromotionDecision::PrimaryStillEligible`.)

### 1.3 Manual override

L2 (Eng Manager) or L3 (CTO) may invoke this runbook before the 5-min sustain window closes IF:

- A SEV-1 customer-impact incident is open AND
- `replication_status()` shows the eligible Replica is genuinely healthy (per §3 pre-checks) AND
- `override_reason` field is recorded in the `region_promoted.v1` audit event payload.

The override MUST be co-authorized by a second L2+ in `#incidents-major` (Slack `/approve-replica-failover` slash) per the manual-lane pattern in `RB-ACTIVE-FAILOVER.md §2.1`.

### 1.4 Relationship to `RB-ACTIVE-FAILOVER.md`

In a real DR-16 incident BOTH runbooks fire. Ordering is **always**:

```
1. RB-REPLICA-FAILOVER  (this doc)  →  flips coordinator role state (back-plane)
                                       Primary→HotStandby + Replica→Primary
                                       audit emit BEFORE mutation (fail-CLOSED)

2. RB-ACTIVE-FAILOVER              →  consumes the new role label and flips
                                       the request-path write-lease + DNS + CF Worker
                                       routing so client traffic hits the new Primary
```

The reverse ordering would create split-brain: the request-path router would point traffic at a region the coordinator still labels `Replica`, causing INV-FAILOVER-NO-SPLIT-BRAIN violation at the coordinator layer (and rejected promotion via `CoordinatorError::SplitBrainRejected`). The coordinator promotion is the source of truth; the request-path flip follows.

For **drill mode** (WI-S17-008 quarterly cadence): both runbooks execute in this ordering against the staging environment with the chaos-mesh-injected stale heartbeat / SLO breach. The drill orchestrator is `scripts/active-failover-drill.sh --staging` (already 3-mode per wave-14); a wave-16 extension adds a `--replica-coordinator-flip` flag that wires the coordinator-side flip before the request-path flip.

## 2. Pre-checks (T+0:00 → T+0:03)

Before invoking `coordinator.promote(...)`, gate-check **all** of:

### 2.1 Replica lag SLO

```bash
# 1. Snapshot lag for the eligible-replica candidate across the 3 hard SLO domains.
#    Neon is informational only; if it's breached we still allow promotion.
curl -sS https://api.corelink.io/v1/health/replication \
    | jq '.regions[] | select(.role == "Replica") | {region, lag: .lag_seconds, fresh: .heartbeat_fresh}'

# Required values for the chosen replica candidate (deterministic Region::ALL order):
#   lag.r2_hot   <= 60
#   lag.d1       <= 60
#   lag.kv       <= 60
#   heartbeat_fresh == true
#   role == "Replica"
```

If the candidate replica's R2 / D1 / KV p99 lag exceeds 60 s OR heartbeat is stale, escalate to the next replica in deterministic Region order. If **no** replica is eligible, the coordinator returns `CoordinatorError::NoEligibleReplica` — see §7 rollback (do NOT auto-promote; escalate to L3 + cold-restore consideration).

### 2.2 Heartbeat freshness

```bash
# 2. Confirm primary heartbeat IS stale (or lag genuinely breached) and replica
#    heartbeat is fresh. This is the inverse-symmetric check that prevents
#    PrimaryStillEligible anti-flap rejection.
bash scripts/active-failover-drill.sh --staging --check-heartbeat \
    --primary "${PRIMARY}" --expect-stale \
    --replica "${REPLICA}" --expect-fresh
```

### 2.3 No pending writes in primary's WAL / outbox

Cross-domain drain gate — must be empty before the role flip so a writer doesn't observe split-brain mid-flight:

```bash
# 3. Audit-outbox drain check (DEBT-011 P1-002 gate; mirror of RB-ACTIVE-FAILOVER §7.3).
wrangler d1 execute corelink_audit --remote \
    --command "SELECT COUNT(*) AS pending FROM audit_outbox WHERE primary_region = '${PRIMARY}' AND drained_at IS NULL;"
# Required: pending == 0  OR  pending > 0 AND IC accepts forensic gap (audit reason)

# 4. R2 hot-blob WAL drain — no pending replication writes.
curl -sS "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT_ID}/r2/buckets/corelink-hot-${PRIMARY}/replication_status" \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    | jq '.result.pending_object_count'
# Required: <= 100  (within RPO budget; >100 means we'd lose > RPO worth of writes)

# 5. D1 inflight writes — same shape as RB-ACTIVE-FAILOVER §3.1.
wrangler d1 execute corelink_core --remote \
    --command "SELECT COUNT(*) AS in_flight FROM write_lease_state WHERE region = '${PRIMARY}' AND state = 'in_flight';"
# Required: in_flight == 0  OR  queue-TTL exhausted (90 s budget)
```

### 2.4 Exit gate

- [ ] Eligible replica identified (`role == "Replica"`, `lag <= 60s` on R2+D1+KV, `heartbeat_fresh == true`).
- [ ] Primary genuinely ineligible (`heartbeat_stale == true` OR `lag > 60s` on ≥ 1 hard domain).
- [ ] Audit-outbox + R2 WAL + D1 inflight all drained (or override with audited gap acknowledgement).
- [ ] `replication_status()` snapshot saved to `drill-replica-failover-${TS}-prestate.json`.

> **If §2.3 drain fails**: do NOT call `coordinator.promote(...)`. Either (a) wait up to queue-TTL (90 s) for natural drain, (b) execute reverse-replication catch-up via the DEBT-011 P1-002 path, or (c) declare aborted-failover per §7. **NEVER bypass the drain gate** — the coordinator's audit-BEFORE-mutate fail-CLOSED guards split-brain at promotion time but cannot retroactively recover lost writes.

## 3. Promote (T+0:03 → T+0:08)

The eligible replica takes the `Primary` role. **This is the only step that mutates the coordinator role map.**

### 3.1 Singleton DO lock acquire (production)

In wave-15, the singleton lock is modelled as a per-instance `Mutex` (in-memory). The production binding (wave-16 `trait-abstraction-defer` — exercised by WI-S17-008 drill rehearsal) is a single Cloudflare Durable Object whose `idFromName("replication-coordinator")` returns one identity across all colos. The lock is held across the entire promote sequence (demote-audit-emit → promote-audit-emit → role-map flip) so concurrent callers see `SplitBrainRejected` not race conditions.

```bash
# 1. Acquire the singleton DO lock + run a no-op probe to confirm singleton identity.
#    The probe writes a forensics breadcrumb in the DO storage with the IC's session ID.
curl -sS -X POST "https://coordinator.corelink.io/v1/admin/lock/acquire" \
    -H "Authorization: Bearer ${COORDINATOR_ADMIN_TOKEN}" \
    -H "X-Incident-Id: ${PD_INCIDENT_ID}" \
    -H "X-IC-Session: ${IC_SESSION_ID}" \
    --data-raw '{"reason":"replica_failover","primary":"'"${PRIMARY}"'","replica":"'"${REPLICA}"'"}'
# Required response: 200 + {"lock_held_by":"<IC_SESSION_ID>","do_singleton_id":"<canonical>"}.
# If 409 Conflict: another IC holds the lock — coordinate via #incidents-major BEFORE retrying.
```

### 3.2 Audit emit BEFORE mutation (fail-CLOSED)

Two events MUST land in the audit chain **before** the role map flips. Order: `region_demoted.v1` (about the old primary) then `region_promoted.v1` (about the new primary). If either emit fails, `coordinator.promote(...)` returns `CoordinatorError::Audit(detail)` and the role map is **NEVER** mutated.

```bash
# 2. Emit region_demoted.v1 + region_promoted.v1 via the coordinator's audit-bus binding.
#    In wave-15 modelled mode this is the InMemoryReplicationCoordinator; in
#    production wiring (wave-16) this writes CloudEvents to the audit-bus →
#    audit_outbox D1 table (consumed by audit-chain per S-06).
curl -sS -X POST "https://coordinator.corelink.io/v1/admin/promote" \
    -H "Authorization: Bearer ${COORDINATOR_ADMIN_TOKEN}" \
    -H "X-Lock-Token: ${LOCK_TOKEN_FROM_3_1}" \
    -H "Idempotency-Key: replica-failover-${PD_INCIDENT_ID}" \
    --data-raw '{
      "primary":"'"${PRIMARY}"'",
      "replica":"'"${REPLICA}"'",
      "now_ms": '"$(date +%s%3N)"',
      "override_reason": null
    }'
# Required response: 200 + {"decision":"Promoted","audit_chain":["region_demoted.v1@<seq>","region_promoted.v1@<seq+1>"]}
# Possible failure modes (treat as rollback triggers per §7):
#   - 409 + SplitBrainRejected      => another region already holds Primary
#   - 409 + PrimaryStillEligible    => coordinator considers primary healthy; re-check §1
#   - 409 + NoEligibleReplica       => candidate is not eligible; pick next or escalate
#   - 500 + Audit                   => audit emit failed BEFORE mutation; state UNCHANGED; retry after audit-bus heal
#   - 500 + Internal                => coordinator-side bug; SEV-1 escalation
```

### 3.3 Role flip + cool-down record

If the audit emits succeed, the coordinator atomically (under the singleton lock):

1. Sets `role[<old_primary>] = HotStandby`.
2. Records `cooldown_started[<old_primary>] = now`.
3. Sets `role[<new_primary>] = Primary`.

This is **one TLA+ transition** in `specs/tla/replica_failover.tla::Promote` — the singleton DO lock models the atomicity guarantee; the spec proves `InvAtMostOnePrimary` over all reachable states.

### 3.4 Validate via `replication_status()` API

```bash
# 3. Snapshot the post-promote state. Expect exactly ONE Primary, the demoted region in
#    HotStandby with cooldown_started_at_ms set, others in Replica.
curl -sS "https://api.corelink.io/v1/health/replication" \
    | jq '.regions[] | {region, role, cooldown_started_at_ms, lag: .lag_seconds}'
```

### 3.5 Exit gate

- [ ] Singleton DO lock acquired AND held throughout the promote.
- [ ] Both audit events (`region_demoted.v1` + `region_promoted.v1`) appended to chain in correct order.
- [ ] `replication_status()` shows exactly one `Primary`, exactly one `HotStandby`, others `Replica`.
- [ ] `cooldown_started_at_ms` set for the new HotStandby region.
- [ ] `corelink_replica_role_label{region=<NEW_PRIMARY>}` Prometheus gauge updated.
- [ ] Singleton DO lock released.

## 4. Verify (T+0:08 → T+0:12)

End-to-end probes confirm the role flip is consistent across the four data domains AND customer-visible surfaces.

### 4.1 Cross-tenant smoke probe

```bash
# 1. 3-tenant smoke — write + read + audit-emit, against canonical drill tenants pinned
#    to different residency tiers (EU/US/BR) — mirror of RB-ACTIVE-FAILOVER §6.1 step 2
#    but assertive on the coordinator's role label (not the request-path region label).
for tenant in drill-eu drill-us drill-br; do
    bash scripts/active-failover-drill.sh --staging --smoke-write \
        --tenant "${tenant}" \
        --expect-coordinator-primary "${REPLICA}"
done
```

### 4.2 Customer-visible health endpoint

```bash
# 2. The /v1/health/replication endpoint (wave-16 production wiring — exercised by
#    drill rehearsal per WI-S17-008) is the customer-dashboard surface. It MUST reflect
#    the new Primary within 30 s of the role flip (KV propagation budget per
#    SLO-REPLICATION-LAG-KV).
for i in $(seq 1 10); do
    curl -sS "https://api.corelink.io/v1/health/replication" \
        | jq -r '.regions[] | select(.role == "Primary") | .region'
    sleep 3
done
# Required: every sample returns ${REPLICA}. Any sample returning ${PRIMARY} is a
# KV-staleness alert (and a customer-dashboard inconsistency tracked under
# SLO-REPLICATION-LAG-KV).
```

### 4.3 Grafana dashboard probe

```bash
# 3. Confirm the failover dashboard reads the correct primary. Render PNG for evidence.
curl -sS -H "Authorization: Bearer ${GRAFANA_API_TOKEN}" \
    "https://grafana.corelink.io/render/d/replication-coordinator/dashboard?from=now-15m&to=now" \
    > "drill-replica-failover-${TS}-postpromote.png"
```

### 4.4 Split-brain check

```bash
# 4. Confirm zero writes were accepted by the demoted region after T+0:03 (the
#    audit-emit timestamp). This is INV-FAILOVER-NO-SPLIT-BRAIN audit-time evidence;
#    complements the TLA+ proof in replica_failover.tla::InvAtMostOnePrimary.
bash scripts/active-failover-drill.sh --staging --split-brain-check \
    --primary "${PRIMARY}" --new-primary "${REPLICA}" \
    --window-start "${PROMOTE_T0_TS}"
# Required: zero overlapping writes. Any non-zero result is SEV-1 (real) / SEV-2 (drill).
```

### 4.5 Audit-chain Merkle walk

```bash
# 5. Walk the audit chain from pre-state marker to post-promote; verify continuous
#    Merkle root + both region_demoted.v1 + region_promoted.v1 entries present.
bash scripts/active-failover-drill.sh --staging --audit-walk \
    --from "replica-failover-pre-${TS}" --to "now" \
    --expect-events "region_demoted.v1,region_promoted.v1"
```

### 4.6 Exit gate

- [ ] All 3 tenant smoke writes succeed against the new Primary + read-back matches.
- [ ] `/v1/health/replication` reflects new Primary in 100% of 10 samples (≤ 30 s settle).
- [ ] Grafana dashboard PNG saved as post-promote evidence.
- [ ] Split-brain check returns zero overlapping writes.
- [ ] Audit-chain walk returns continuous Merkle root with both required events.

> **If split-brain check fails**: declare SEV-2 (drill) or SEV-1 (real). Do NOT proceed to failback. Root-cause within 14 d per `RB-POSTMORTEM-PROCESS.md`. This would indicate a coordinator-layer bug — the TLA+ spec proves it cannot happen under the modelled assumptions, so a real occurrence means the production wiring violates one of those assumptions (likely the singleton DO identity guarantee).

## 5. Failback after 24 h cool-down

The demoted region stays in `HotStandby` for `HOT_STANDBY_COOLDOWN_SECONDS = 86_400` seconds (24 h hard floor). Premature attempts return `CoordinatorError::CooldownNotElapsed { elapsed_seconds, remaining_seconds }` AND emit a `failback_blocked.v1` audit record. **DO NOT bypass the cool-down**.

### 5.1 Failback prerequisites

- [ ] `cooldown_remaining_seconds` from `replication_status()` for the HotStandby region == 0.
- [ ] Original-primary heartbeat fresh AND R2 + D1 + KV lag within SLO sustained ≥ 1 h.
- [ ] No SEV-1/SEV-2 incident currently open against either region.
- [ ] Reverse-replication catch-up complete: writes that landed on the new Primary during the failover window MUST be replicated back to the demoted region (audit-outbox drain gate per `RB-ACTIVE-FAILOVER.md §7.3` mirror).

### 5.2 Failback procedure

```bash
# 1. Confirm cool-down elapsed via replication_status().
curl -sS "https://api.corelink.io/v1/health/replication" \
    | jq '.regions[] | select(.role == "HotStandby") | {region, cooldown_remaining_seconds}'
# Required: cooldown_remaining_seconds == 0

# 2. Invoke coordinator.failback(<region>, now). This emits failback_committed.v1 audit
#    BEFORE the role flip (fail-CLOSED), demotes the current Primary to Replica, and
#    promotes the HotStandby region back to Primary.
curl -sS -X POST "https://coordinator.corelink.io/v1/admin/failback" \
    -H "Authorization: Bearer ${COORDINATOR_ADMIN_TOKEN}" \
    -H "Idempotency-Key: replica-failback-${PD_INCIDENT_ID}" \
    --data-raw '{"region":"'"${PRIMARY}"'","now_ms":'"$(date +%s%3N)"'}'
# Required response: 200 + {"decision":"FailbackCommitted","audit_chain":["failback_committed.v1@<seq>"]}
# Possible failure modes:
#   - 409 + CooldownNotElapsed     => 24h floor not yet passed; check cooldown_remaining_seconds
#   - 409 + PrimaryStillEligible   => current new-Primary is healthy; coordinator refuses anti-flap
#   - 500 + Audit                  => audit emit failed; state UNCHANGED; retry after audit-bus heal

# 3. Repeat §4 verification (cross-tenant probe + health endpoint + Grafana + split-brain
#    check + audit-chain walk) with primary/replica labels swapped back.
```

### 5.3 Failback exit gate

- [ ] `failback_committed.v1` audit event written.
- [ ] `replication_status()` reflects original Primary == `Primary` again; new-Primary back to `Replica`.
- [ ] Cross-tenant smoke + health endpoint + Grafana all consistent.
- [ ] Split-brain check returns zero overlapping writes in failback window.
- [ ] Audit-chain Merkle walk continuous from `region_demoted.v1` through `failback_committed.v1`.

## 6. Communication

### 6.1 Statuspage

- **Detect step (real-incident only):** SRE on-call posts "We're investigating replication lag in our `<region>` region" within 5 min of alert fire. Component: `Replication Coordinator`. Status: `degraded_performance`.
- **Promote step (real-incident only):** Customer Comms updates: "Replication primary has been moved to our `<sibling>` region. No customer action required; reads from your region continue normally." Status: `partial_outage` until §4 verify exit-gate clears, then `monitoring`.
- **Verify exit (real-incident only):** Status: `operational` after split-brain check clears.
- **Failback (24h+ later):** posted as scheduled maintenance window with 1 h advance notice.

### 6.2 Customer email template

Sent at Promote-exit if the failover lasted ≥ 30 min OR a customer-visible read was rejected during the window:

```
Subject: CoreLink replication primary moved — no action required

We performed a replication primary move from <PRIMARY> to <REPLICA> at
<TIMESTAMP UTC> to maintain our replication-lag SLO (≤ 60 s p99).

Customer impact: writes during the <DURATION_MINUTES>-minute flip window
may have observed elevated latency. No data was lost; all writes accepted
by either region are durable.

Residency: your data continues to be stored in your contracted regions
per your Data Processing Agreement.

We will move the primary back to <PRIMARY> after our 24 h post-incident
hot-standby cool-down completes. You do not need to take any action.

Incident ID: <PD_INCIDENT_ID>
Status page: https://status.corelink.io
Postmortem: posted to https://docs.corelink.io/postmortems within 14 days.

— CoreLink SRE
```

### 6.3 Internal Slack

- `#incidents-major` (real-incident SEV-1) OR `#drill-replica-failover-YYYY-MM-DD` (drill).
- IC posts timeline updates every 5 min through Verify exit-gate; every 30 min through Failback.
- Scribe attaches `drill-replica-failover-${TS}-prestate.json` + `-postpromote.png` + audit-chain walk output.

## 7. Rollback (aborted promotion)

If §3 Promote fails mid-flight, rollback rules:

| Failure mode | Coordinator state | Rollback action |
|---|---|---|
| `CoordinatorError::SplitBrainRejected` | Unchanged (audit not emitted) | Investigate why another region holds Primary; reconcile coordinator state via L3 escalation; **do NOT retry blindly** — the rejection is the safety guarantee fired. |
| `CoordinatorError::PrimaryStillEligible` | Unchanged | Re-check §1 trigger conditions; either trigger was stale or anti-flap fired. Wait 5 min and re-evaluate. |
| `CoordinatorError::NoEligibleReplica` | Unchanged | All replicas breach SLO. Escalate to L3 (CTO) + consider `RB-COLD-RESTORE-FROM-ZERO.md` if primary is genuinely unrecoverable. |
| `CoordinatorError::Audit(detail)` | Unchanged (audit emit failed BEFORE mutation) | Audit-bus is degraded. Diagnose audit-bus health; **DO NOT** mutate coordinator state until the bus is restored. Emit `failover.aborted` synthetic record once the bus recovers (forensic continuity). |
| `CoordinatorError::Internal` | Possibly inconsistent | SEV-1 escalation immediately. Acquire singleton DO lock manually + inspect state; reconcile via `replication_status()` cross-check against the four-domain SLI feed. |
| Network timeout on §3.2 HTTP call | Unknown (idempotency-key recovers) | Re-call §3.2 with same `Idempotency-Key`. The coordinator deduplicates; either the original call landed (200 returned) or never did (clean retry). |

In **all** rollback cases:

1. Release the singleton DO lock (`POST /v1/admin/lock/release` with the lock token from §3.1).
2. Emit a `failover.aborted` audit event (separate audit-bus call) with `abort_reason` field.
3. Notify IC; reschedule the failover within 7 d (drill) or escalate to L3 (real).

> **NEVER** bypass `audit-emit BEFORE mutation`. Bypassing the fail-CLOSED ordering would silently violate INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER AND INV-FAILOVER-NO-SPLIT-BRAIN (the audit chain is one of the three layered defences per `replica-coordinator-production` audit §2.3).

## 8. Post-incident review

For both drills and real incidents, the IC + Scribe MUST complete the following within 14 d (real) or 7 d (drill):

- [ ] Postmortem written using `RB-POSTMORTEM-PROCESS.md` template.
- [ ] Evidence doc archived at `specs/_compliance/drill-evidence/YYYY-MM-replica-failover-<simulate|staging|prod>.md` per `BCP-DR-DRILL-CADENCE.md §6`.
- [ ] Audit-chain Merkle walk output attached.
- [ ] `replication_status()` pre-state + post-promote + post-failback JSONs attached.
- [ ] MTTA / MTTR / split-brain count / failback-cool-down compliance metrics submitted to `corelink_dr_drill_*` Prometheus gauges per WI-S17-008 §19.
- [ ] Lessons-learned section feeds FM-202 mitigation cycle + runbook updates.
- [ ] 5-Why analysis if the drill failed any §4 exit-gate item OR a real incident hit any SEV-1 customer impact.
- [ ] Runbook deltas committed (this file + companions) within the 14 d / 7 d window.

## 9. References

- `crates/corelink-replication-coordinator/` — wave-15 coordinator (state / heartbeat / lag / audit / coordinator modules; `InMemoryReplicationCoordinator`)
- `specs/tla/replica_failover.tla` — ✅ GREEN; `InvAtMostOnePrimary` proves INV-FAILOVER-NO-SPLIT-BRAIN at the coordinator layer
- `specs/_audits/sealed/2026-05-15-replica-coordinator-production.md` — wave-15 audit (singleton DO lock, audit-bus, `/v1/health/replication` deferred)
- `specs/04_sprints/_sealed/S17/work_items/WI-S17-008-active-failover-drill.md` — quarterly drill spec (this runbook's drill rehearsal cadence)
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md §DR-16` — drill cadence (weekly `--simulate` + monthly `--staging` + quarterly real-paired with active-failover)
- `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` — DR-16 drill spec parent
- `specs/_runbooks/RB-ACTIVE-FAILOVER.md` — request-path / write-lease runbook (this doc's companion; ordering: replica-failover BEFORE active-failover, see §1.4)
- `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` — escalation path if `NoEligibleReplica` AND primary unrecoverable ≥ 4 h
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — post-incident review template
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — 3-tier escalation
- `specs/03_architecture/invariant_registry.md §3` — INV-FAILOVER-NO-SPLIT-BRAIN + INV-REGION-NO-CROSS-LEAK canonical entries
- `specs/03_architecture/resilience_patterns.md §Failback` — 24 h hot-standby canonical pattern
