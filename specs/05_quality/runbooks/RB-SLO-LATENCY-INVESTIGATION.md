---
id: "RB-SLO-LATENCY-INVESTIGATION"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "slo", "latency", "cas", "ac", "performance", "r6-2"]
---

# RB-SLO-LATENCY-INVESTIGATION — Latency SLO Burn (CAS GET / CAS PUT / AC Hit)

> **SLOs covered:** SLO-LAT-CAS-GET (§4.6, 99% < 300ms team / 200ms enterprise), SLO-LAT-CAS-PUT (§4.7, 99% < 1s team / 600ms enterprise), SLO-LAT-AC-HIT (§4.8, 99% < 150ms).
> **Related FMs:** FM-005 (DO rebalance), FM-052 (R2 eventual consistency), FM-055 (D1 cold spike), FM-057 (Neon failover), FM-105 (inter-region latency), FM-106 (HOL blocking).

## Symptom

- PagerDuty alert `corelink-slo-latency-cas-get-fast-burn` / `-slow` / variants.
- Customer reports: "builds slow", "CI taking 3× normal time".
- `DASH-CAS` p99 histogram crosses red line for sustained 5 min+.
- Hit ratio stable but latency degraded (rules out cache miss as cause).

## Detection

```promql
# Fast-burn: percent of requests > threshold in 5m vs SLO budget
1 - (sum(rate(corelink_cas_get_duration_seconds_bucket{le="0.3"}[5m]))
     / sum(rate(corelink_cas_get_duration_seconds_count[5m])))
  > (1 - 0.99) * 14.4

# Latency by region (find regional anomaly)
histogram_quantile(0.99,
  sum by (region, le) (rate(corelink_cas_get_duration_seconds_bucket[5m])))

# Latency by blob_size_bucket (find pathological size class)
histogram_quantile(0.99,
  sum by (blob_size_bucket, le)
    (rate(corelink_cas_get_duration_seconds_bucket[5m])))
```

Trace query (Tempo): filter `service=worker-cp op=cas.get duration>500ms` last 5 min.

## Immediate mitigation (≤ 5 min)

1. **Confirm signal**: is it p99 only (tail latency) or p50 also (systemic)?
   - p99 only → contention, head-of-line blocking, single noisy neighbor.
   - p50 too → systemic degradation, likely upstream (R2/D1/Neon) slowness.
2. **Check upstream dependency latency**:
   - R2: `histogram_quantile(0.99, rate(corelink_storage_r2_get_duration_seconds_bucket[5m]))`.
   - D1: `corelink_storage_d1_q_duration_seconds`.
   - Neon: `corelink_storage_neon_q_duration_seconds`.
3. **If upstream is the cause**:
   - R2 region slow → trigger `RB-REGION-FAILOVER` if sustained > 10 min.
   - Neon slow → check `RB-FM-057` (failover in progress?).
   - D1 cold → wait ~60s (warmup) OR force warm via synthetic ping.
4. **If upstream OK but worker slow**:
   - CF Workers CPU near 50ms limit per request? Check `corelink_handler_cpu_ms`.
   - DO actor rebalance (FM-005)? Check `corelink_do_state_migration` counter.
   - Single tenant flooding? Apply per-tenant rate limit (CTRL-RATE-001) to that tenant_id.

## Root-cause investigation (15–30 min)

1. **Cardinality drill-down** to find the slow slice:
   ```promql
   # Worst region × plan combination
   topk(5, histogram_quantile(0.99,
     sum by (region, plan, le)
       (rate(corelink_cas_get_duration_seconds_bucket[10m]))))
   ```
2. **Trace exemplar**: pull 3 slow traces from Tempo; identify span with bulk of time (storage? authz? rate-limit check?).
3. **Recent change correlation**: any deploy / config change in last 24h? `corelink_deploys_total[24h]` × diff vs latency baseline.
4. **Saturation breakdown**:
   - `corelink_resource_saturation_ratio` per resource — any > 0.7?
   - `corelink_resource_utilization_ratio` per backend — close to PPS cap?
5. **Blob-size correlation**: is latency degradation concentrated in `blob_size_bucket >= 16MiB`? If yes, multipart path likely culprit (CTRL-MULTIPART-002).
6. **Inter-region latency** (FM-105): `corelink_inter_region_latency_ms` for cross-region dedup path?

## Rollback / recovery

| Cause                         | Action                                                                  |
|-------------------------------|-------------------------------------------------------------------------|
| Recent deploy regression      | `wrangler rollback --env prod --service worker-cp`                      |
| Config change (e.g. timeout shrunk) | `git revert <sha>` in `infra/cf-config` + apply                   |
| R2 region degradation         | Apply `degrade_mode=cache-only` for affected region; failover via `RB-REGION-FAILOVER` |
| Single-tenant flooding        | DO config update: `rate_limit_override.<tenant_id>` reduced 2×          |
| HOL blocking (FM-106)         | Enable HTTP/3 path if disabled: `wrangler tail-logs --env prod`; verify `protocol=h3` rising |

Verify recovery:

```promql
histogram_quantile(0.99,
  sum by (le) (rate(corelink_cas_get_duration_seconds_bucket[5m]))) < 0.3
```

## Escalation path

| Time | Who                                | Criteria                                       |
|------|------------------------------------|------------------------------------------------|
| 0    | Primary SRE                        | Latency burn-rate alert fires                  |
| 10m  | Secondary SRE + Architect          | p99 sustained > 2× SLO target                  |
| 20m  | Owner of affected backend (R2/D1/Neon) | Upstream identified as cause                  |
| 30m  | Comms Lead                         | Customer-visible (status page `degraded`)      |
| 60m  | Enterprise CSM                     | Enterprise tier SLO budget burn > 50%          |

**Comms template:**

```
[Investigating] We are seeing elevated latency on CoreLink CAS reads
(p99 ~ {observed_ms}ms vs target {target_ms}ms). Functionality is intact;
some operations may take longer. Investigating root cause. Next update in 15 min.
```

## Post-incident

Capture:

- Latency heatmap before/during/after (Grafana snapshot).
- Top-3 slow traces with span breakdown.
- Whether SLO budget was breached (calculate consumption: `(error_budget_used / monthly_budget) × 100%`).
- Was the alert correct (no false positive)?
- Did mitigation restore < SLO target within MTTR SLO (30 min)?
- Did the slowness correlate with a known FM? If new pattern, propose FM in PR.

## Related

- **SLOs:** SLO-LAT-CAS-GET (§4.6), SLO-LAT-CAS-PUT (§4.7), SLO-LAT-AC-HIT (§4.8).
- **Alerts:** `corelink-slo-latency-cas-get-*`, `-cas-put-*`, `-ac-hit-*`.
- **FMs:** FM-005, FM-052, FM-055, FM-057, FM-105, FM-106.
- **Patterns:** PAT-TIMEOUT-001, PAT-DEGRADE-001, PAT-CIRCUIT-001, PAT-SESSION-CONSISTENCY-001.
- **Dashboards:** `DASH-CAS`, `DASH-AC`, `DASH-GLOBAL-HEALTH`.
- **Drill cadence:** quarterly chaos latency injection in staging (obs §12 EVT-023).

---

**Fim RB-SLO-LATENCY-INVESTIGATION.**
