---
id: "RB-SLO-CORRECT-VIOLATION"
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
tags: ["runbook", "slo", "correctness", "sev-1", "cas", "isolation", "r6-2"]
---

# RB-SLO-CORRECT-VIOLATION — Correctness SLO Violation (CAS integrity / Tenant isolation)

> **SLOs covered:** SLO-CORRECT-CAS (§4.9, target 100% — zero budget) + SLO-CORRECT-ISO (§4.10, target 100% — zero budget).
> **Severity:** any non-zero violation = **SEV-1 immediately**. No grace period, no budget.
> **Related FMs:** FM-051 (R2 bit rot), FM-062 (hash collision), FM-254 (cache poisoning), FM-253 (cross-tenant read), FM-303 (AC cross-tenant).

## Symptom

- PagerDuty SEV-1 page `corelink-slo-correct-violation` fires when ANY of:
  - `corelink_cas_client_verify_total{outcome="mismatch"} > 0` (client-side hash mismatch).
  - `corelink_isolation_assertion_total{outcome="fail"} > 0` (tenant_id assertion fail).
  - `corelink_cross_tenant_blob_resolve_total > 0` (AC entry pointing to wrong tenant).
- Customer report: "wrong file content returned" / "I see another tenant's data" — **treat as confirmed SEV-1 until disproven**.

## Detection

```promql
# CAS integrity violation (last 1m)
increase(corelink_cas_client_verify_total{outcome="mismatch"}[1m]) > 0

# Tenant isolation assertion fail
increase(corelink_isolation_assertion_total{outcome="fail"}[1m]) > 0

# AC cross-tenant resolution (FM-303 signature)
increase(corelink_ac_cross_tenant_resolve_total[1m]) > 0
```

Logs (Loki):

```logql
{service=~"worker-cp"} | json
  | error_code=~"CAS_HASH_MISMATCH|TENANT_ISO_FAIL|AC_CROSS_TENANT"
  | line_format `{{.tenant_id}} {{.digest_hex8}} {{.request_id}}`
```

Audit events:

```sql
-- D1
SELECT * FROM audit_events
 WHERE type IN ('cas.verify.mismatch.v1', 'isolation.violation.v1',
                'ac.cross_tenant.v1')
   AND time > datetime('now', '-1 hour');
```

## Immediate mitigation (≤ 5 min)

**CRITICAL: act before investigating. Correctness violations destroy trust.**

1. **Freeze the affected component**:
   - CAS integrity → `wrangler secret put DEGRADE_MODE --env prod` value `frozen-cas-write` (block new writes; reads remain).
   - Tenant isolation → `wrangler secret put DEGRADE_MODE --env prod` value `frozen-tenant-resolve` (block authz path; service returns 503 until clear).
   - AC cross-tenant → invalidate ALL AC entries for affected tenant_ids (or globally if scope unclear):
     ```sql
     UPDATE ac_entries SET status='invalidated', reason='RB-SLO-CORRECT-VIOLATION'
      WHERE tenant_id IN ('<affected>') OR ... ;
     ```
2. **Preserve evidence** (snapshot before anything changes):
   - D1 backup: `wrangler d1 export corelink-prod --output=/tmp/incident-<ts>.sql`.
   - R2 object versions for affected digests: list + lock versions.
   - Audit chain: snapshot `audit_chain` table state (hash + sequence).
3. **Open SEV-1 incident** in PagerDuty with title `CORRECTNESS VIOLATION — <SLO-ID>`.
4. **Status page**: `partial outage` (data integrity investigation in progress).

## Root-cause investigation (15–30 min)

### Branch A — CAS hash mismatch (SLO-CORRECT-CAS)

1. Cross-reference with FM:
   - 1 occurrence, single digest → FM-051 (R2 bit rot likely) → see `RB-FM-051`.
   - Multiple occurrences, same digest → FM-254 (cache poisoning) → see `RB-FM-254`.
   - Two distinct contents same hash → FM-062 (hash collision) → see `RB-FM-062`.
2. **Verify with scrub**: run scrub job for the affected R2 keyspace; check `corelink_scrub_mismatch_total`.
3. **Re-compute hash** server-side: download blob → compute BLAKE3 → compare with stored digest.
4. **Check ingest path**: was this blob recently uploaded? Trace the PUT request — verify hash was computed pre-storage (CTRL-CAS-001).

### Branch B — Tenant isolation fail (SLO-CORRECT-ISO)

1. **TLA+ replay**: run `corelink-tla` with seed = trace_id hash; verify INV-TENANT-ISOLATION (the model should fail too if real bug).
2. **Audit log query**: which authz path triggered? `worker-cp.authz` span attributes `tenant_id, scope, decision`.
3. **Cross-reference FMs**:
   - Direct CAS read leak → FM-253 → `RB-FM-253`.
   - AC entry points cross-tenant → FM-303 → `RB-FM-303`.
   - Backup/restore bug → FM-AC-MIGRATION-BUG → `RB-FM-AC-MIGRATION-BUG`.
4. **Scope assessment**: how many requests / tenants / blobs affected? Use audit chain to enumerate.

## Rollback / recovery

| Scenario                             | Recovery action                                                            |
|--------------------------------------|----------------------------------------------------------------------------|
| Single bit-rot blob (FM-051)         | Restore from R2 prior version if available; else mark blob `corrupt`, force tenant re-upload |
| Hash collision (FM-062)              | Tabletop scenario; engage Architect + Security Lead; document in CVE-style |
| Cache poisoning (FM-254)             | Invalidate all CAS reads for affected window; rotate any compromised PAT   |
| Cross-tenant read (FM-253)           | Disable affected endpoint via feature flag; deploy patch; TLA+ re-verify   |
| AC cross-tenant (FM-303)             | Invalidate AC namespace; re-derive entries from CAS source-of-truth        |
| Recent deploy regression             | `wrangler rollback --env prod --service worker-cp` IMMEDIATELY             |

Recovery verification (must be 100% before un-freezing):

```promql
# All three must be exactly 0 over the last 1h
increase(corelink_cas_client_verify_total{outcome="mismatch"}[1h]) == 0
increase(corelink_isolation_assertion_total{outcome="fail"}[1h]) == 0
increase(corelink_ac_cross_tenant_resolve_total[1h]) == 0
```

## Escalation path

| Time elapsed | Who to page                                          | Criteria                                  |
|--------------|------------------------------------------------------|-------------------------------------------|
| 0 min        | Primary SRE on-call + Security Lead (auto-paged)     | SEV-1 alert fires                         |
| 5 min        | Architect (TLA+ owner) + secondary SRE               | MTTA breach OR scope unclear              |
| 15 min       | VP Engineering + Comms Lead + Legal counsel          | Confirmed cross-tenant exposure (FM-253/303) |
| 30 min       | CEO + Privacy Officer (DPO)                          | Potential GDPR/LGPD breach disclosure required |
| 60 min       | Affected enterprise tenant CSM                       | Customer-specific notification needed     |

**Comms template (status page):**

```
[Investigating] We have detected a possible data integrity / isolation anomaly
on the CoreLink CAS / AC path. We have paused affected operations to investigate.
Customer data may be temporarily inaccessible. Next update in 15 min.
```

**Customer notification template** (if cross-tenant confirmed):

```
Subject: CoreLink incident notification — possible data exposure

Dear {tenant_name},

On {date} at {time UTC}, our integrity monitoring detected an anomaly affecting
{N} request(s) involving your tenant. Specifically: {one-line description}.

What we have done:
- Paused affected pathway at {time + Xm}.
- Preserved full audit trail (request_id, trace_id, digest list available).
- Engaged security + privacy review per CoreLink incident response policy.

What we know so far:
- {Scope: blobs, tenants, time window}
- {Root cause status: investigating / contained / patched}

We will publish a full post-mortem within 14 days at status.corelink.humangr.com.
For questions: incidents@humangr.com.

— CoreLink Security & Engineering
```

## Post-incident

**Mandatory** for any correctness violation (zero-budget SLO):

- Public post-mortem in ≤ 14 days (`specs/_postmortems/<date>-correct-violation.md`).
- TLA+ specification updated to cover the violated scenario.
- Property test added covering the exact data shape.
- Audit chain entry for the incident sealed with timestamp + final scope.
- Customer notifications sent + receipt logged (if cross-tenant exposure).
- Privacy/Legal review: GDPR Art. 33 (72h) / LGPD Art. 48 notification clock — assess whether required.
- Insurance / cyber-policy notification (Architect + CFO call).
- If FM-253 / FM-303 confirmed: trigger `RB-BREACH-NOTIF`.

## Related

- **SLOs:** SLO-CORRECT-CAS (§4.9), SLO-CORRECT-ISO (§4.10).
- **Alerts:** `corelink-slo-correct-cas-violation`, `corelink-slo-correct-iso-violation` (1× threshold = SEV-1, no burn-rate).
- **FMs:** FM-051, FM-062, FM-253, FM-254, FM-303.
- **Patterns:** PAT-AUTHZ-001, CTRL-CAS-001/002, CTRL-ISO-001..005.
- **Invariants:** INV-TENANT-ISOLATION, INV-CAS-INTEGRITY, INV-CAS-IMMUTABILITY, INV-AC-DUAL-SIDE-VERIFY.
- **ADRs:** ADR-0021 (HKDF AC signing), ADR-0035 (AC handler invariants).
- **Sister runbooks:** `RB-FM-051`, `RB-FM-062`, `RB-FM-253`, `RB-FM-254`, `RB-FM-303`, `RB-BREACH-NOTIF`.
- **Dashboards:** `DASH-SECURITY`, `DASH-PRIVACY`, `DASH-CAS`, `DASH-AC`.
- **Drill cadence:** **quarterly** (dry-run injecting fake mismatch in staging).

---

**Fim RB-SLO-CORRECT-VIOLATION.**
