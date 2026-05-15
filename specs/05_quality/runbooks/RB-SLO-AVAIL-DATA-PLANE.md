---
id: "RB-SLO-AVAIL-DATA-PLANE"
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
tags: ["runbook", "slo", "availability", "cas", "ac", "data-plane", "r6-2"]
---

# RB-SLO-AVAIL-DATA-PLANE — Data Plane Availability SLO Burn (CAS / AC / Execute)

> **SLOs covered:** SLO-AVAIL-CAS-GET (§4.2), SLO-AVAIL-CAS-PUT (§4.3), SLO-AVAIL-AC (§4.4), SLO-AVAIL-EXEC (§4.5).
> **Alert aliases:** `RB-SLO-AVAIL-CAS-GET-FAST-BURN` → §Fast-burn (≤5 min); `RB-SLO-AVAIL-CAS-GET-SLOW-BURN` → §Slow-burn (≤30 min) — referenced in `WI-S09-006`.
> **Related FMs:** FM-050, FM-052, FM-057, FM-058, FM-101, FM-250, FM-400.
> **Related ADRs:** ADR-0028 (miss reason uniform 404 freeze), ADR-0042 (GC worker scheduler).

## Symptom

What on-call sees:

- PagerDuty page from `corelink-slo-availability-data-plane` alert (`-fast`, `-slow`, `-medium`, or `-ticket` suffix per obs §9.3).
- Customer reports: `cargo build`, `bazel build`, `npm ci` with CoreLink REAPI endpoint returning 5xx or hanging.
- `DASH-GLOBAL-HEALTH` shows error budget burn rate spike on CAS or AC line.
- `DASH-CAS` or `DASH-AC`: p99 latency normal but error rate elevated, OR availability < SLO target.

## Detection

Source signals (all queryable from Grafana):

```promql
# Burn-rate fast (5m): SEV-1 if > 14.4 for SLO 99.9% target
(1 - (rate(corelink_cas_get_requests_total{outcome="ok"}[5m])
      / rate(corelink_cas_get_requests_total[5m])))
  / (1 - 0.999) > 14.4

# Slow-burn (1h): SEV-1 if > 6.0
(1 - (rate(corelink_cas_get_requests_total{outcome="ok"}[1h])
      / rate(corelink_cas_get_requests_total[1h])))
  / (1 - 0.999) > 6.0

# Per-op breakdown to isolate component
sum by (op, outcome, error_code) (
  rate(corelink_cas_get_requests_total[5m])
)
```

Log query (Loki) for context:

```logql
{service="worker-cp", env="prod"} |= `op="cas.get"` | json | level="ERROR"
  | line_format `{{.error_code}} {{.region}} {{.tenant_id}}`
```

## Immediate mitigation (≤ 5 min)

### Fast-burn (≤ 5 min) — SEV-1

1. **Confirm signal**: open `DASH-GLOBAL-HEALTH` + `DASH-CAS` simultaneously. Multi-region or single? Multi-tenant or single?
2. **Identify component** via `error_code` top-3:
   - `UPSTREAM_R2` → FM-050 / FM-051 path → `RB-FM-051`.
   - `UPSTREAM_NEON` → FM-057 / FM-058 → `RB-FM-057`.
   - `UPSTREAM_CF_EDGE` → FM-101 → `RB-FM-101`.
   - `RATE_LIMIT_GLOBAL` → FM-250 → `RB-FM-250` (DDoS volumetric).
   - `INTERNAL_ASSERTION` → recent deploy regression → §Rollback below.
3. **If recent deploy** (< 2h): trigger auto-rollback NOW:
   ```bash
   wrangler rollback --env prod --service worker-cp
   ```
   `PAT-PROGRESSIVE-ROLLOUT-001` *should* have auto-rollback'd; if it didn't, file FM-201 / FM-200 incident.
4. **If no deploy**: apply degrade mode preventively:
   ```bash
   # Read-only mode for control plane (still serves cached CAS reads)
   wrangler secret put DEGRADE_MODE --env prod  # value: "cache-only"
   ```

### Slow-burn (≤ 30 min) — SEV-2

1. Error budget burn sustained but not catastrophic. Investigate without panic.
2. Check `corelink_cf_workers_cpu_ms` p99 — saturation?
3. Check `corelink_resource_saturation_ratio{resource="r2_pps"}` — R2 PPS quota near limit?
4. Check tenant top-talker: `topk(3, sum by (tenant_id) (rate(corelink_cas_get_requests_total[10m])))` — one tenant flooding?
5. If single tenant: enforce per-tenant rate limit via DO config update (CTRL-RATE-001).

## Root-cause investigation (15–30 min)

1. **Timeline reconstruction** (Tempo):
   - Pick a failing trace (`status_class="5xx"`); follow span `worker-cp.cas.get` → `worker-cp.storage.r2.get`.
   - Identify where time is spent + where error originated.
2. **Cardinality drill-down**:
   - Per-region: `sum by (region) (rate(corelink_cas_get_errors_total[5m]))`.
   - Per-plan: `sum by (plan) (rate(corelink_cas_get_errors_total[5m]))` — enterprise tenants disproportionately affected?
3. **Saturation check**:
   - `corelink_resource_saturation_ratio` for all backends; threshold 0.8.
   - `corelink_do_storage` near 32 MiB cap → FM-059.
4. **Dependency check** (parallel): CF Status, R2 dashboard, Neon dashboard, Stripe (if billing path involved).
5. **TLA+ invariants**: if `outcome="error" AND error_code IN (INTERNAL_*)`, run quick local re-check of `INV-TENANT-ISOLATION` + `INV-GC-001` — silent corruption masquerading as 5xx?

## Rollback / recovery

| Scenario                              | Command                                                                       |
|---------------------------------------|-------------------------------------------------------------------------------|
| Recent deploy regression              | `wrangler rollback --env prod --service worker-cp`                            |
| Config change (rate-limit drop FM-201)| `git revert <sha>` in `infra/cf-config` repo + `terraform apply`              |
| R2 region failure                     | Follow `RB-REGION-FAILOVER` + degrade to cache-only                           |
| Neon failover triggered               | `RB-FM-057` — wait 30–90s, monitor connection pool                            |
| Edge outage (CF global)               | `RB-FM-101` — status page + customer comms; no internal mitigation possible   |
| DDoS volumetric                       | `RB-FM-250` — engage CF DDoS managed + WAF rule tighten                       |

After rollback, verify recovery with:

```promql
# Should return to baseline within 2–5 min
1 - (rate(corelink_cas_get_requests_total{outcome="ok"}[5m])
     / rate(corelink_cas_get_requests_total[5m])) < 0.001
```

## Escalation path

| Time elapsed         | Who to page                                                | Trigger criteria                                |
|----------------------|------------------------------------------------------------|-------------------------------------------------|
| 0 min                | Primary SRE on-call (PagerDuty)                            | Alert fires                                     |
| 5 min (no ack)       | Secondary SRE + SRE Lead                                   | MTTA SLO-ONCALL-MTTA-SEV1 breach                |
| 15 min (no mitigation)| Architect + product on-call                                | MTTR risk; cross-component coordination         |
| 30 min               | VP Engineering + Comms Lead                                | Customer-visible outage; status page `major`     |
| 60 min               | CEO + enterprise tenant CSM                                | SLA credit territory; enterprise comms          |

**Comms template (status page):**

```
[Investigating] We are seeing elevated error rates on CoreLink CAS/AC read paths
in {region(s)}. Engineering is investigating. Cached client-side reads continue
to work. Next update in 15 min.
```

## Post-incident

Capture for retro (`specs/_postmortems/<date>-data-plane-outage.md`):

- Timeline (PagerDuty + Slack + git log of changes in window).
- Failing trace_ids (≥ 3 representative).
- D1 snapshot pre/post-mitigation if storage component involved.
- Tenant impact list (top-10 by error count, anonymized for public report).
- Was multi-burn-rate alert correct? (No flapping, no missed burn).
- Was degrade mode effective? (Did latency for surviving requests stay within SLO-LAT-*?).
- Did auto-rollback fire? If not, FM-201 follow-up issue.

Map root cause to FM-ID (`failure_modes.md §3`). If no FM applies, propose new FM in same PR.

## Related

- **SLOs:** SLO-AVAIL-CAS-GET, SLO-AVAIL-CAS-PUT, SLO-AVAIL-AC, SLO-AVAIL-EXEC (`slo_catalog.md §4.2–4.5`).
- **Alerts:** `corelink-slo-availability-*-fast`, `-slow`, `-medium`, `-ticket` (`observability_model.md §9.3`).
- **FMs:** FM-050, FM-052, FM-057, FM-058, FM-101, FM-250, FM-400.
- **Patterns:** PAT-DEGRADE-001, PAT-PROGRESSIVE-ROLLOUT-001, PAT-CIRCUIT-001.
- **ADRs:** ADR-0028 (404 miss freeze), ADR-0042 (GC scheduler).
- **Dashboards:** `DASH-GLOBAL-HEALTH`, `DASH-CAS`, `DASH-AC`, `DASH-EXEC`.
- **Drill cadence:** quarterly chaos test injects latency/5xx in staging (`observability_model.md §12` EVT-023).

---

**Fim RB-SLO-AVAIL-DATA-PLANE.**
