---
id: "RB-ACTIVE-FAILOVER"
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
tags: ["runbook", "dr", "active-failover", "warm-failover", "soc2-a1-2", "soc2-cc7-5", "rto-15m", "rpo-5m", "dr-16"]
---

# RB-ACTIVE-FAILOVER — Warm Switch Active Region To Sibling

> **Status:** ACTIVE. Companion to `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` (drill spec, DR-16) and `scripts/active-failover-drill.sh` (3-mode orchestrator).
>
> **Purpose:** procedural runbook for the warm DR scenario — the primary region is **degraded but not destroyed** (p95 > 5s, error rate > 5%) and we flip the active write lease to the sibling per `ResidencyGraph`. Used for the monthly DR-16 drill and for real warm-failover incidents (mode switches noted per step).
>
> **RTO target:** ≤ 15 min (write-lease flip, steps 1–6). Failback RTO ≤ 30 min (step 7).
> **RPO target:** ≤ 5 min (cross-region replication checkpoint stale-budget).
>
> **DO NOT** invoke any state-changing command without `CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD` env if `CORELINK_ENV=production`. See `scripts/active-failover-drill.sh` for guard implementation.
>
> **NOT the same as cold-restore.** If the primary is unreachable for ≥ 4h with no ETA, escalate to `RB-COLD-RESTORE-FROM-ZERO.md` (DR-15) instead.

## 1. Trigger conditions

This runbook is invoked when **all three** hold:

1. Primary region is reachable but the failover-router (`crates/corelink-failover-router`) is reporting `RegionHealth::Degraded` (multi-signal: 5xx > 1% + p99 > 300 ms + ≥ 3 consecutive failures, sustained 5s window) **escalated** to the SLO breach thresholds (p95 > 5s OR error rate > 5%) for ≥ 5 min, **AND**
2. Sibling region per `ResidencyGraph::sibling(primary)` is itself `RegionHealth::Healthy` in the last 15 min, **AND**
3. Cross-region replication lag is **within RPO budget** (≤ 5 min) at the moment of declaration (`replication_lag_seconds_p99` SLO from `corelink-replica-worker::REPLICATION_LAG_P99_SLO_SECS`).

## 2. Role assignments

| Role | Tier | Responsibility | Time commitment |
|---|---|---|---|
| Incident Commander (IC) | L2 (Eng Manager) | Sequences steps; owns timeline; calls go/no-go at step 4 + step 7 | Full drill window (45 min) |
| Scribe | L1 (SRE on-call) | Timestamps every action; updates `#drill-active-failover-*` channel | Full drill window |
| Failover Operator | L1 (SRE on-call) | Executes wrangler / dig / curl commands | Steps 3–6 |
| Privacy Lead | L2 | Validates residency-graph invariants (no cross-leak) | Step 4 (Promote) + Step 6 (Verify) |
| Compliance Reviewer | L2 (Compliance Officer) | Validates audit-chain continuity + split-brain check; signs evidence doc | Step 6, post-drill |
| CTO (L3) | L3 | Sole authorizer for `--prod` real-incident mode + Override lane | Step 2 (Decide) only for Override |

## 3. Pre-step bootstrap (T+0)

Before step 1, confirm:

- [ ] War-room channel `#drill-active-failover-YYYY-MM-DD` (or `#incident-XXX` for real) is live.
- [ ] PagerDuty incident opened with severity = SEV-2 (drill) or SEV-1 (real).
- [ ] Conference bridge dialled by IC + Scribe + Failover Operator.
- [ ] Drill orchestrator started in correct mode: `bash scripts/active-failover-drill.sh --staging` (or `--prod` for real with `CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD`).

The orchestrator writes a structured log to `drill-active-failover-YYYY-MM-DD-HH-MM.log` — every step below references `LOG_STEP=N_TITLE` markers.

---

## Step 1 — Detect (T+0:00 → T+0:02)

### 1.1 Trigger alerts

The following alerts MUST fire (or be synthetically fired in `--simulate`/`--staging`) before this runbook proceeds:

| Alert | Source | Threshold |
|---|---|---|
| `RegionDegraded-{region}` | `corelink-failover-router` health gauge | `corelink_region_health_status{region="<primary>"} == 1` for ≥ 5 min (Prometheus value 1 = Degraded) |
| `Slo5xxBurnRate-{region}` | gateway error-rate panel | `sum(rate(gw_request_error_rate_5xx[5m])) > 0.05` for ≥ 5 min |
| `LatencyP95Breach-{region}` | gateway latency histogram | `histogram_quantile(0.95, gw_request_latency_p95) > 5.0` for ≥ 5 min |
| `ReplicationLagWithinBudget-{sibling}` | replica-worker checkpoint | `replication_lag_seconds_p99{region="<sibling>"} <= 300` (gate, not alert) |

### 1.2 Commands

```bash
# 1. Drop a marker in the audit chain (drill-pre).
wrangler d1 execute corelink_audit --remote \
    --command "INSERT INTO audit_chain_checkpoints (epoch, marker, ts) VALUES ('drill-pre', 'active-failover-start-${PRIMARY}', $(date -u +%s));"

# 2. Snapshot baseline.
curl -sS -H "Authorization: Bearer ${GRAFANA_API_TOKEN}" \
    "https://grafana.corelink.io/render/d/failover/dashboard?from=now-1h&to=now" \
    > "drill-baseline-${TS}.png"

# 3. Confirm sibling health.
bash scripts/active-failover-drill.sh --simulate --check-sibling "${PRIMARY}"
```

### 1.3 Exit gate

- [ ] All four alerts in §1.1 either firing (real) or synthetically asserted in log (drill).
- [ ] Sibling region returns `RegionHealth::Healthy` from probe.
- [ ] Audit-chain marker `drill-pre` written.

---

## Step 2 — Decide (T+0:02 → T+0:05)

### 2.1 Trigger lane selection

| Sustained breach duration | Lane | Authorization |
|---|---|---|
| ≥ 10 min | **Automatic** | Auto-flip; no manual ack required to begin Drain. IC paged. |
| 5–10 min | **Manual** | SRE primary on-call + L2 Eng Mgr co-authorize via Slack `/approve-failover` |
| < 5 min, IC declares | **Override** | L3 (CTO) sole authorizer; requires `override_reason` in audit emit |

### 2.2 Escalation tree

```
SRE primary on-call (L1)  ──┐
                            ├── Manual lane: BOTH must approve
Eng Manager (L2)          ──┘

Eng Manager (L2) ─── alone ─── Override lane (when L3 unavailable, 30-min window)

CTO (L3) ──── alone ──── Override lane (always); REQUIRED for --prod mode
```

### 2.3 Commands

```bash
# 1. Emit failover.detected audit event BEFORE flipping anything (fail-CLOSED ordering).
bash scripts/active-failover-drill.sh --staging --emit-audit "failover.detected" \
    --primary "${PRIMARY}" --sibling "${SIBLING}" --lane "${LANE}"

# 2. (Manual / Override only) Log authorizer identity in audit chain.
wrangler d1 execute corelink_audit --remote \
    --command "INSERT INTO audit_chain_checkpoints (epoch, marker, ts) VALUES ('drill-decision', '${LANE}:${AUTHORIZER_USER_ID}', $(date -u +%s));"
```

### 2.4 Exit gate

- [ ] Lane selected + audit-emit succeeded.
- [ ] Authorizer identity recorded (Manual/Override lanes only).
- [ ] PD incident updated with lane label.

---

## Step 3 — Drain (T+0:05 → T+0:07)

Stop new writes to the primary region; in-flight writes complete or queue in the per-tenant Durable Object (`tenant_cache_actor`).

### 3.1 Commands

```bash
# 1. Flip write-mode for the primary region to Blocked.
#    Production: this is wired through corelink-failover-router::FailoverRouter::write_mode
#    returning WriteMode::Blocked when region is degraded. The drill calls the equivalent
#    feature-flag/control-plane endpoint to assert the same state without waiting for
#    natural detection.
bash scripts/active-failover-drill.sh --staging --drain \
    --primary "${PRIMARY}" --queue-ttl-secs 300

# 2. Watch the in-flight write counter drain to zero.
#    Budget: 90 seconds. If counter > 0 after 90s, escalate to Override lane and force-flush.
watch -n 5 'wrangler d1 execute corelink_core --remote \
    --command "SELECT COUNT(*) AS in_flight FROM write_lease_state WHERE region = \"'"${PRIMARY}"'\" AND state = \"in_flight\""'
```

### 3.2 Exit gate

- [ ] `WriteMode::Blocked` confirmed on `${PRIMARY}` via `corelink_failover_router` health gauge.
- [ ] In-flight write counter = 0 OR queue-TTL exhausted (whichever first).
- [ ] No new writes accepted by primary (verify via probe).

> **Safety:** if in-flight counter > 0 AND queue-TTL exhausted, do NOT proceed to step 4 — Drain MUST complete to avoid split-brain. Escalate to Override + force-flush; otherwise the runbook aborts and the primary remains active.

---

## Step 4 — Promote (T+0:07 → T+0:10)

The sibling region takes the write lease. **This is the only step that mutates the active-region label.**

### 4.1 Commands

```bash
# 1. Emit failover.promoting audit event BEFORE mutation (fail-CLOSED).
bash scripts/active-failover-drill.sh --staging --emit-audit "failover.promoting" \
    --primary "${PRIMARY}" --sibling "${SIBLING}"

# 2. Privacy-lead invariant gate.
python3 scripts/check_ac_infra.sh  # placeholder — invokes residency-graph checker
bash scripts/active-failover-drill.sh --staging --check-residency \
    --new-active "${SIBLING}"

# 3. Flip the write lease in the control-plane DO.
bash scripts/active-failover-drill.sh --staging --promote \
    --sibling "${SIBLING}"

# 4. Emit failover.promoted audit event AFTER mutation.
bash scripts/active-failover-drill.sh --staging --emit-audit "failover.promoted" \
    --primary "${PRIMARY}" --sibling "${SIBLING}"
```

### 4.2 Exit gate

- [ ] Residency-graph checker passed (zero cross-leak).
- [ ] Write lease held by `${SIBLING}` in control-plane DO state.
- [ ] Both audit events (`failover.promoting` + `failover.promoted`) appended to chain.
- [ ] `corelink_active_region_label` Prometheus gauge updated to `${SIBLING}`.

---

## Step 5 — Reroute (T+0:10 → T+0:12)

Update DNS + CF Worker routing so client traffic hits the new active region.

### 5.1 Commands

```bash
# 1. DNS flip — point the failover hostname at the sibling region.
#    TTL must already be ≤ 60s (validated in Pre-drill §2.1).
bash scripts/active-failover-drill.sh --staging --dns-flip \
    --hostname "api.staging.corelink.io" \
    --to "${SIBLING}"

# 2. CF Worker routing flip — update the route map so /api/* served from sibling colo.
wrangler routes update --pattern "api.staging.corelink.io/*" \
    --zone-id "${CF_ZONE_ID}" \
    --service "corelink-worker-${SIBLING}"

# 3. Force-purge edge caches that pin region label.
curl -X POST "https://api.cloudflare.com/client/v4/zones/${CF_ZONE_ID}/purge_cache" \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -d '{"tags":["region-label"]}'
```

### 5.2 Exit gate

- [ ] `dig api.staging.corelink.io` resolves to the sibling CF colo (within DNS TTL).
- [ ] `wrangler routes list` shows the updated service binding.
- [ ] Cache purge response = 200.

---

## Step 6 — Verify (T+0:12 → T+0:15)

Synthetic page + 3-tenant smoke test to confirm write-flip end-to-end.

### 6.1 Commands

```bash
# 1. Synthetic page probe against new active region.
bash scripts/active-failover-drill.sh --staging --synthetic-probe \
    --target "https://api.staging.corelink.io/_health" \
    --expect-region "${SIBLING}"

# 2. 3-tenant smoke — write + read + audit-emit, against canonical drill tenants.
#    The three tenants pin different residency tiers so the smoke exercises
#    EU-residency, US-residency, and BR-residency paths simultaneously.
for tenant in drill-eu drill-us drill-br; do
    bash scripts/active-failover-drill.sh --staging --smoke-write \
        --tenant "${tenant}" --expect-region "${SIBLING}"
done

# 3. Audit-chain continuity walk.
bash scripts/active-failover-drill.sh --staging --audit-walk \
    --from "drill-pre" --to "now"

# 4. Split-brain check — confirm zero writes accepted by the primary in overlap window.
bash scripts/active-failover-drill.sh --staging --split-brain-check \
    --primary "${PRIMARY}" --sibling "${SIBLING}"
```

### 6.2 Exit gate

- [ ] Synthetic probe returns 200 + `x-corelink-region: ${SIBLING}` header.
- [ ] All 3 tenant smoke writes succeed + read-back matches + audit event appended.
- [ ] Audit-chain walk returns continuous Merkle root (no gap, no re-fork).
- [ ] Split-brain check returns zero overlapping writes.

> **If split-brain check fails:** declare SEV-2 (drill) or SEV-1 (real). Do NOT proceed to step 7. Root-cause within 14d.

---

## Step 7 — Reverse (T+0:15 → T+0:45)

Failback: once the primary recovers, return the active write lease to it.

### 7.1 Trigger conditions for Reverse

- Primary region health returned to `RegionHealth::Healthy` for ≥ 15 min sustained, **OR**
- Drill window dictates Reverse regardless (the drill always exercises failback).

### 7.2 Reconciliation procedure (writes that happened on sibling during failover)

Writes accepted by the sibling during the failover window MUST replicate back to the primary before active-lease handover. Use the replica-worker's reverse-replication path:

```bash
# 1. Pause new writes briefly on sibling (≤ 30s) to bound the catch-up window.
bash scripts/active-failover-drill.sh --staging --drain \
    --primary "${SIBLING}" --queue-ttl-secs 30

# 2. Trigger reverse replication — sibling → primary catch-up.
bash scripts/active-failover-drill.sh --staging --reverse-replicate \
    --from "${SIBLING}" --to "${PRIMARY}"

# 3. Wait until replication-lag ≤ 5s sustained 60s.
bash scripts/active-failover-drill.sh --staging --wait-lag \
    --target "${PRIMARY}" --threshold-secs 5 --duration-secs 60

# 4. Flip write lease back to primary (mirror of step 4).
bash scripts/active-failover-drill.sh --staging --promote \
    --sibling "${PRIMARY}"

# 5. DNS + Worker routing flip back (mirror of step 5).
bash scripts/active-failover-drill.sh --staging --dns-flip \
    --hostname "api.staging.corelink.io" --to "${PRIMARY}"
wrangler routes update --pattern "api.staging.corelink.io/*" \
    --zone-id "${CF_ZONE_ID}" --service "corelink-worker-${PRIMARY}"

# 6. Synthetic + 3-tenant smoke (mirror of step 6).
bash scripts/active-failover-drill.sh --staging --synthetic-probe \
    --target "https://api.staging.corelink.io/_health" \
    --expect-region "${PRIMARY}"

# 7. Emit failover.resolved audit event.
bash scripts/active-failover-drill.sh --staging --emit-audit "failover.resolved" \
    --primary "${PRIMARY}" --sibling "${SIBLING}"
```

### 7.3 Data reconciliation gate

- [ ] Reverse-replication report: zero diverged rows (sibling-only writes all present in primary).
- [ ] BLAKE3 + audit-chain Merkle walk passes against primary.
- [ ] Replication-lag ≤ 5s sustained 60s.

### 7.4 Exit gate

- [ ] Write lease held by `${PRIMARY}` again.
- [ ] DNS + Worker routing back to primary.
- [ ] Synthetic + 3-tenant smoke green on primary.
- [ ] `failover.resolved` audit event written.
- [ ] Total wall-clock ≤ 45 min (15-min flip + 30-min Reverse).

---

## 4. Real-incident divergence from drill

When invoked as a real warm-failover (not a scheduled drill):

- Severity = SEV-1 (drill = SEV-2).
- Status page updated by Customer Comms at Detect step (drill skips this).
- Customer-facing comms email staged at Promote step; sent at Verify step.
- Reverse step (§7) deferred until primary genuinely recovers; if primary never recovers within 4h, escalate to cold-restore (`RB-COLD-RESTORE-FROM-ZERO.md`).
- `CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD` env required for `--prod` mode invocation.

## 5. Aborted-failover rollback

If Drain step (§3) fails to reach in-flight=0 within 90s + queue-TTL exhausted:

1. Do NOT proceed to Promote.
2. Restore `WriteMode::Allowed` on primary via `--undo-drain` flag.
3. Emit `failover.aborted` audit event with `abort_reason` field.
4. Notify IC; reschedule drill within 7d (drill) or escalate to cold-restore consideration (real).

## 6. References

- `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` — DR-16 drill spec (RTO/RPO targets, success criteria)
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` — DR-16 cadence row
- `scripts/active-failover-drill.sh` — 3-mode orchestrator
- `tests/e2e-failover-router/` — 5-scenario E2E harness
- `crates/corelink-failover-router/` — multi-signal detection + routing primitives
- `crates/corelink-region/` — region taxonomy + provisioning events
- `crates/corelink-replica-worker/` — ResidencyGraph + replication lag SLO
- `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` — escalation path if primary unreachable ≥ 4h
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — 3-tier escalation
