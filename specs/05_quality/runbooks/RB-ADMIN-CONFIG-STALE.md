---
id: "RB-ADMIN-CONFIG-STALE"
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
tags: ["runbook", "slo", "admin", "config", "propagation", "s13", "r6-2"]
---

# RB-ADMIN-CONFIG-STALE — Admin Config Propagation Latency Breach

> **SLO covered:** SLO-ADMIN-CONFIG-PROPAGATION (§4.14) — target ≤ 5 min p99 sustained 30d (admin singleton update visible in all workers).
> **Referenced from:** `slo_catalog.md §4.14 Notas` — "Breach → SEV-2 + RB-ADMIN-CONFIG-STALE".
> **Invariant:** INV-ADMIN-CONFIG-CAS (S-13 §3.X).
> **WI:** WI-S13-001.

## Symptom

- PagerDuty SEV-2 page `corelink-slo-admin-config-propagation-breach` fires.
- Operator dashboard shows config version mismatch: `admin_config_published_version=N` but workers still serving `N-1`.
- Symptom downstream: a config change (e.g., rate limit raise, feature flag flip) made via admin API was acknowledged but NOT taking effect in production after expected 5 min window.

## Detection

```promql
# p99 propagation latency over 30m window
histogram_quantile(0.99,
  sum by (le) (rate(corelink_admin_config_propagation_ms_bucket[30m])))
  > 300000  # 5 min in ms

# Specific stale workers (version lag)
max(corelink_admin_config_version{component="published"})
- min(corelink_admin_config_version{component="worker_running"})
  > 0
```

Audit events:

```sql
-- D1
SELECT * FROM admin_op_log
 WHERE op = 'config.publish'
   AND time > datetime('now', '-30 minutes')
   AND propagated_at IS NULL;
```

## Immediate mitigation (≤ 5 min)

1. **Confirm version skew**: query each region's `worker_running_version`:
   ```bash
   for region in wnam enam weur apac; do
     curl -s "https://${region}.corelink.humangr.com/_admin/config/version" \
       -H "Authorization: Bearer $ADMIN_TOKEN"
   done
   ```
   All should return the latest published version.
2. **Identify stuck region(s)**: regions reporting older version are the culprits.
3. **Force config reload** on stuck region(s):
   ```bash
   # Admin endpoint forces re-fetch from CAS singleton
   curl -X POST "https://${REGION}.corelink.humangr.com/_admin/config/reload" \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -d '{"version_target": "<latest>"}'
   ```
4. **If reload fails**: degrade — pin all workers to the previously known-good version via env override:
   ```bash
   wrangler secret put ADMIN_CONFIG_VERSION_OVERRIDE --env prod  # value: "<last-good>"
   ```

## Root-cause investigation (15–30 min)

1. **CAS singleton fetch path**:
   - Check `corelink_admin_config_fetch_duration_seconds` p99 — is the CAS get itself slow?
   - Check R2 status for the admin config bucket.
2. **Dual-approval signature verification** (CTRL-ADMIN-002):
   - Did the new version pass signature verify on all workers? Query `corelink_admin_config_signature_verify_total{outcome="fail"}`.
   - If failing: rotate signing key OR re-publish with valid signature → see `RB-ADMIN-DUAL-APPROVAL-BREACH`.
3. **Clock skew check** (FM-350):
   - `corelink_clock_skew_seconds` for affected region > 5s?
4. **DO admin singleton health**:
   - Is the DO actor responsive? Check `worker-cp.admin_singleton` span.
   - If unresponsive: trigger DO restart via `_admin/restart-singleton`.
5. **D1 admin_op_log integrity**:
   - Last successful `propagated_at` timestamp per region.
   - Any rows with `op='config.publish' AND status='applied'` but `propagated_at IS NULL` for > 5 min?

## Rollback / recovery

| Cause                                      | Recovery                                                                       |
|--------------------------------------------|--------------------------------------------------------------------------------|
| CAS singleton fetch slow (R2 slow)         | Wait + monitor; if > 30 min, switch admin config source to backup R2 region    |
| Signature verify fail                      | Re-sign config with current key set; re-publish via admin API                  |
| DO singleton unresponsive                  | Restart via `_admin/restart-singleton`; falls back to last cached config       |
| Bad config content (validation regression) | Revert via admin API: `POST /_admin/config/revert?to=<version>`                |
| Network partition (worker can't fetch)     | Fall back to env-pinned ADMIN_CONFIG_VERSION_OVERRIDE                          |

Verify recovery:

```promql
# All workers serving latest version within 5 min
max(corelink_admin_config_version{component="published"})
== min(corelink_admin_config_version{component="worker_running"})

# Propagation histogram returns to baseline
histogram_quantile(0.99,
  sum by (le) (rate(corelink_admin_config_propagation_ms_bucket[5m]))) < 300000
```

## Escalation path

| Time elapsed | Who                              | Criteria                                  |
|--------------|----------------------------------|-------------------------------------------|
| 0            | Primary SRE on-call (PagerDuty)  | SEV-2 fires                               |
| 30 min       | Secondary SRE + Admin plane owner| MTTA breach or unresolved                 |
| 1h           | Architect (INV-ADMIN-CONFIG-CAS) | Singleton invariant suspected at risk     |
| 2h           | VP Engineering                   | Multi-region propagation broken            |
| 4h           | Comms Lead                       | Customer-impacting (config-gated features stuck) |

**Comms template (internal):**

```
SEV-2 — Admin config propagation breach
Published: v{N} at {time}; observed running: v{N-1} in {region(s)}.
Customer impact: {feature flag X / rate limit Y} change not yet effective.
Owner: @{handle}. Mitigation in progress. Next update: +30 min.
```

## Post-incident

Capture:

- Timeline: `published_at`, `propagated_at` per region, mitigation start/end.
- Was propagation a one-off (network blip) or systemic (signing/CAS path bug)?
- INV-ADMIN-CONFIG-CAS: was the invariant violated? (Two regions running different versions for sustained period = INV breach.)
- Audit chain entry sealed for the incident.
- If signing key issue surfaced: trigger `RB-ADMIN-ROTATION-GAP`.
- If broader admin signing question: `RB-KEY-COMPROMISE` if compromise suspected.

## Related

- **SLO:** SLO-ADMIN-CONFIG-PROPAGATION (`slo_catalog.md §4.14`).
- **Invariant:** INV-ADMIN-CONFIG-CAS (S-13 §3.X).
- **CTRLs:** CTRL-ADMIN-002 (dual-approval), CTRL-ADMIN-006 (propagation), CTRL-ADMIN-007 (rollback).
- **FMs:** FM-201 (config change ratelimit drop), FM-205 (admin mistake), FM-350 (clock skew).
- **Patterns:** PAT-DUAL-APPROVAL-001, PAT-CIRCUIT-001.
- **Sister runbooks:** `RB-FM-201`, `RB-FM-205`, `RB-ADMIN-DUAL-APPROVAL-BREACH`, `RB-ADMIN-ROTATION-GAP`, `RB-ROLLOUT-STUCK`.
- **WI:** WI-S13-001 (admin plane config CAS singleton).
- **ADR:** ADR-S11-001 (MFA step-up destructive admin ops).
- **Drill cadence:** quarterly (synthetic config publish + propagation timing measurement).

---

**Fim RB-ADMIN-CONFIG-STALE.**
