---
id: "RB-REGION-FAILOVER"
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
tags: ["runbook", "slo", "dr", "rto", "rpo", "region", "failover", "s17", "r6-2"]
---

# RB-REGION-FAILOVER — Region Failover Operational Procedure (RTO + RPO SLOs)

> **SLOs covered:**
> - SLO-RTO-REGION-FAILOVER (§4.18) — ≤ 30 min (1800s) p99.
> - SLO-RPO-REGION (§4.19) — ≤ 60s p99.
> **Drill source:** WI-S17-002 + `RB-DR-DRILL` (semestral drill); this RB is the *operational* (real-incident) procedure.
> **Canonical constants:** `corelink-dr-drill::outage::RTO_CEIL_SECONDS = 1800`, `RPO_CEIL_SECONDS = 60`.

## Symptom

- PagerDuty SEV-1 page `corelink-region-outage-declared` fires when:
  - `corelink_region_health_score{region=$R}` drops below 0.3 sustained 2 min.
  - OR manual declaration via `_admin/declare-outage`.
- Customer reports: regional connectivity dead (e.g., enam users 5xx everywhere).
- Status page: CF region status indicates degradation.

## Detection

```promql
# Region health composite (availability + latency + saturation)
corelink_region_health_score{region=~"wnam|enam|weur|apac"}

# Region-specific 5xx burst
sum by (region) (rate(corelink_cas_get_requests_total{outcome="error"}[2m]))
  / sum by (region) (rate(corelink_cas_get_requests_total[2m])) > 0.5
```

Outage declaration audit:

```sql
SELECT * FROM dr_outage_log ORDER BY declared_at DESC LIMIT 5;
```

## Immediate mitigation (≤ 5 min — RTO clock starts here)

1. **Declare outage** (starts canonical RTO measurement):
   ```bash
   curl -X POST "https://corelink.dev/_admin/declare-outage" \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"region":"'$REGION'","reason":"<short>"}'
   ```
   This records `t_outage_declared` for RTO/RPO metrics (WI-S17-002).
2. **Activate failover region**:
   ```bash
   curl -X POST "https://corelink.dev/_admin/region-failover" \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"failed":"'$REGION'","target":"'$TARGET_REGION'"}'
   ```
   This:
   - Promotes Neon replica in `$TARGET_REGION` to primary (if D1 multi-region scope).
   - Updates DNS GeoSteering rules to route `$REGION` traffic to `$TARGET_REGION`.
   - Re-routes R2 reads via cross-region replication endpoint (eventual consistency window: <60s = within RPO SLO).
3. **Verify first request served by failover region**:
   ```promql
   sum(rate(corelink_cas_get_requests_total{region=$TARGET_REGION,
                                            served_for_region=$FAILED_REGION}[1m]))
     > 0
   ```
   This timestamp = `t_first_request_served_by_failover_region`. RTO = this − `t_outage_declared`. Target ≤ 30 min.

## Root-cause investigation (parallel — does NOT block failover)

1. **Why did the region fail?**
   - CF edge outage (FM-101) → see `RB-FM-101`. Failover is correct response.
   - R2 region degradation (FM-050) → see also `RB-FM-051`. Failover OK; investigate scrub status.
   - Neon primary failure (FM-057) → failover is the recovery (`RB-FM-057`).
   - Network/BGP issue (FM-102) → may resolve before failover completes; continue failover anyway.
2. **Was the outage warning gradual** (saturation building up) **or sudden** (instant 0)?
   - Gradual: backpressure mechanisms (PAT-DEGRADE-001) should have triggered; check why they didn't.
   - Sudden: external dependency hard failure — coordinate with provider.
3. **RPO assessment**: how stale is the data in `$TARGET_REGION` at time of failover?
   ```promql
   max(corelink_replication_lag_seconds{from=$FAILED_REGION,to=$TARGET_REGION})
   ```
   If > 60s, RPO SLO breached — flag for post-incident.

## Rollback / recovery (failback once original region healthy)

| Phase                       | Action                                                                            |
|-----------------------------|-----------------------------------------------------------------------------------|
| Original region healthy ≥30m| Verify health: `corelink_region_health_score{region=$ORIG} > 0.9` for 30 min      |
| Drain failover region        | `_admin/drain-region?region=$TARGET&direction=$ORIG` — gradual 10%/min            |
| Reverse Neon promotion       | Promote `$ORIG` Neon back to primary (if scope allows; coordinate w/ DBA)         |
| DNS revert                   | Restore default GeoSteering rules                                                  |
| Verify normal operation      | Synthetic test passing in both regions; SLO burn returning to baseline             |
| Declare incident closed      | `_admin/close-outage` records `t_recovered`; updates `dr_outage_log`               |

**Do NOT failback** until:
- Original region healthy for ≥ 30 min sustained.
- Root cause known + patched (not just "it stopped failing").
- Replication lag returning to baseline.

## Escalation path

| Time elapsed | Who                                  | Criteria                                       |
|--------------|--------------------------------------|------------------------------------------------|
| 0            | Primary SRE + SRE Lead (auto-paged)  | Outage declared OR SEV-1 region alert fires    |
| 5 min        | Architect + Secondary SRE            | Failover not yet activated                     |
| 15 min       | VP Engineering + Comms Lead          | Status page update needed; customer comms      |
| 25 min       | CEO + Enterprise CSM team            | RTO at risk (5 min before 30-min target)       |
| 30 min       | Compliance / Privacy (DPO)           | RTO SLO breached + extended outage (regulator notification eval) |

**Comms template (status page):**

```
[Investigating] We are experiencing a service disruption in the {REGION}
region of CoreLink. Traffic is being failed over to {TARGET_REGION}; some
customers in {REGION} may see brief latency increase or transient errors
during the transition. ETA to full recovery: ~30 min. Next update: +10 min.
```

**Comms template (status page after failover complete):**

```
[Identified] Traffic from {REGION} has been routed to {TARGET_REGION}.
Service is operational. We are investigating root cause and will provide
a post-incident report within 14 days at status.corelink.dev/incidents.
```

## Post-incident

Capture (canonical fields for `dr_drill_runs` table even for real incidents):

- `outage_declared_at` (start of RTO clock).
- `first_request_served_by_failover_region_at`.
- `last_committed_write_replicated_at` (for RPO).
- Computed `rto_seconds` and `rpo_seconds`.
- Pass/fail vs SLO target.
- Root cause FM-ID.
- Customer-facing comms: status page entries with timestamps.
- Whether failback was completed (yes/no, when).

**Post-mortem required** within 14 days. If RTO > 1800s OR RPO > 60s: GA gate at risk — escalate to S-17 sprint owner + Architect.

## Related

- **SLOs:** SLO-RTO-REGION-FAILOVER (§4.18), SLO-RPO-REGION (§4.19).
- **Alerts:** `corelink-region-outage-declared`, `corelink-region-health-score-low`.
- **FMs:** FM-050 (R2 region), FM-057 (Neon failover), FM-100/101/102 (network/DNS/BGP), FM-105 (inter-region latency).
- **Patterns:** PAT-REGION-FAILOVER-001, PAT-DEGRADE-001, PAT-SESSION-CONSISTENCY-001.
- **Sister runbooks:** `RB-DR-DRILL` (drill-only), `RB-FM-057`, `RB-FM-100`, `RB-FM-101`, `RB-FM-105`, `RB-SLO-AVAIL-DATA-PLANE`.
- **WI:** WI-S17-002 (DR drill scheduler).
- **ADR:** ADR-0031 (Neon schema), ADR-0036 (D1 schema migration).
- **Invariants:** INV-AUDIT-APPEND-ONLY (must hold across failover — no audit gap).
- **Drill cadence:** **semestral** (Jan 1 + Jul 1 mandated); GA gate dependency.

---

**Fim RB-REGION-FAILOVER.**
