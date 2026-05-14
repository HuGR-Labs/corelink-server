---
id: "RB-region-leak"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p0", "region", "pinning", "residency", "schrems-ii", "lgpd", "gdpr", "wi-s14-002"]
---

# RB-region-leak — Cross-Region Tenant Data Leak: Detection → Containment → Forensic → Notification

> **INV:** INV-REGION-NO-CROSS-LEAK CRITICAL (WI-S14-002 §12) + INV-DATA-RESIDENCY CRITICAL (§3.11)
> **CTRL:** CTRL-PRIV-031 (residency) + CTRL-AUDIT-005 (7y retention)
> **FM:** FM-054 (KV global leak) + FM-105 (region replication diverge)
> **SLA:** detect ≤ 1h | mitigate ≤ 6h | customer notification ≤ 72h (GDPR Art. 33 / LGPD Art. 48)
> **Severity:** SEV-1 if confirmed data exposure; SEV-2 on counter > 0 (blocked, no exposure yet)

## Pre-conditions

- Region pinning enforcement active (WI-S14-002 SEALED).
- D1 migration 0027 applied (`tenants.primary_region NOT NULL` + immutable trigger).
- `corelink_region_cross_region_read_blocked_total` Prometheus counter at zero baseline.
- DO `region_enforcer` per-region instance running (5min TTL cache + D1 fallback).
- KV namespace per-region scope enforced (`corelink-session-{region}`).
- INV-REGION-NO-CROSS-LEAK 30k property test green in CI.
- Audit chain per-region (S-09 herdada) operational.

---

## Step 1 — Detection

**Trigger:** `corelink_region_cross_region_read_blocked_total > 0` fires SEV-2 alert.

**Verify counter spike:**
```bash
# Prometheus query (adapt to your metrics backend)
curl -s 'https://metrics.corelink.dev/api/v1/query' \
  --data-urlencode 'query=corelink_region_cross_region_read_blocked_total > 0' \
  | jq '.data.result'
```

**Determine if leak was blocked or passed through:**
- Counter increment = enforcement fired = data NOT exposed (blocked).
- If audit chain has `write_rejected_cross_region.v1` events = write was blocked.
- If R2/D1/KV shows cross-region objects = CRITICAL SEV-1 (escalate immediately).

**Dashboard:** DASH-REGION — cross-region attempts blocked counter panel.

**Escalation:**
- Counter > 0, blocked: SEV-2 on-call SRE.
- Counter > 0 + confirmed write: SEV-1 + Privacy Officer + DPO.
- Sustained > 10/min: SEV-1 + WAF kill switch.

---

## Step 2 — Immediate Containment

**If SEV-1 (confirmed cross-region write):**

```bash
# 1. WAF kill: block offending tenant_id (Cloudflare WAF custom rule)
# cf_waf_block_tenant <tenant_id>
# (requires Cloudflare API token with WAF:Edit scope)

# 2. Revoke offending PAT/session (S-03 PAT revoke API)
curl -X POST https://api.corelink.dev/v1/admin/pat/revoke \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"tenant_id": "<TENANT_ID>", "reason": "cross-region leak containment"}'

# 3. Invalidate DO region_enforcer cache for affected tenant
# (Forces fresh D1 lookup on next request — prevents stale cache re-exploitation)
# POST /internal/region-enforcer/invalidate?tenant_id=<TENANT_ID>
```

**If SEV-2 (blocked, no confirmed write):**
- Monitor rate. If counter > 10 in 5min → escalate to SEV-1.
- Identify source tenant_id from audit chain (Step 3).

---

## Step 3 — Forensic Investigation

**Query per-region audit chain:**
```bash
# Query audit chain for cross-region events (adapt to your audit backend)
# S-09 audit chain: per-region R2 bucket `audit-<region>`, Object Lock 7y.
# CloudEvent type: dev.hugr.corelink.residency.request_routed.v1
# CloudEvent type: dev.hugr.corelink.residency.write_rejected_cross_region.v1

# Example: list cross-region events in WEUR audit bucket last 1h
aws s3api list-objects-v2 \
  --bucket audit-weur \
  --prefix "residency/write_rejected_cross_region/" \
  --query 'Contents[?LastModified>=`'"$(date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%SZ)"'`]'
```

**Impact assessment checklist:**
- [ ] Identify `tenant_id` + `primary_region` + `request_region` + `endpoint`.
- [ ] Determine time window (first event → last event).
- [ ] Identify operation type: read or write? Which backends?
- [ ] Were any R2/D1/KV objects written cross-region? (SEV-1 if yes.)
- [ ] Identify source IP/ASN (Cloudflare access logs).
- [ ] Determine if attacker or accidental bug.

**D1 primary_region verification:**
```sql
-- Verify tenant primary_region was NOT mutated (trigger should have prevented this)
SELECT tenant_id, primary_region, updated_at_ms
FROM tenant
WHERE tenant_id = '<AFFECTED_TENANT_ID>';
```

---

## Step 4 — Schrems II / GDPR / LGPD Breach Assessment

**Applies if:** confirmed cross-region write of EU tenant data to non-EU region.

**Questions to answer:**
1. Was any PII data (EU GDPR / BR LGPD Art. 33 §1º) written cross-region?
2. Was the transfer adequately protected (SCC / BCR / adequacy decision)?
3. Is this a notifiable breach under GDPR Art. 33 / LGPD Art. 48?

**Schrems II TIA template:** `docs/legal/schrems-ii-tia-template.md` (WI-S14-008).

**DPA notification timeline:**
- GDPR Art. 33: notify supervisory authority within 72h if high risk.
- LGPD Art. 48: notify ANPD + data subjects without undue delay.

**Privacy Officer contact:** privacy@corelink.dev
**DPO contact:** dpo@hugr.dev

---

## Step 5 — Customer Notification

**Template:**
```
Subject: CoreLink Data Residency Incident Notification

Dear [Customer],

We have detected a cross-region access event affecting your CoreLink tenant
(primary_region: [REGION]). This notification is provided per GDPR Art. 33 /
LGPD Art. 48 within the 72-hour requirement.

Incident Details:
- Detected: [TIMESTAMP UTC]
- Tenant primary region: [REGION]
- Attempted access region: [WRONG_REGION]
- Access blocked: [YES/NO]
- Data exposed: [CONFIRMED/NOT CONFIRMED/UNDER INVESTIGATION]

Actions taken: [SUMMARY]

We will provide a full post-mortem within 5 business days.

Contact: privacy@corelink.dev
```

---

## Step 6 — Recovery + Root Cause Fix

1. **Root cause analysis:** identify which enforcement layer was bypassed (middleware? DO cache? D1 trigger?).
2. **Patch:** fix the bypass + extend property test to cover the edge case.
3. **Nightly 100k iter:** verify sustained 0 leaks post-fix.
4. **D1 trigger verification:** `trg_tenant_primary_region_immutable` still active post-patch.
5. **Post-mortem:** CRITICAL post-mortem per on-call policy; owner sign-off within 5 business days.

---

## Step 7 — Dry-Run Verification

```bash
# Host-side dry-run (CI safe — no staging required)
bash scripts/rb_region_leak_dry_run.sh
```

Expected output: all steps PASS; exit code 0.

---

## Metrics Reference

| Metric | Alert threshold | Severity |
|---|---|---|
| `corelink_region_cross_region_read_blocked_total` | > 0 | SEV-2 |
| `corelink_region_cross_region_read_blocked_total` rate > 10/5min | sustained | SEV-1 |
| `corelink_region_enforcer_lookup_duration_seconds_bucket` p99 | > 1ms | WARNING |
| `corelink_region_enforcer_cache_miss_total` rate | > 20% | WARNING |

---

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Criação WI-S14-002. |
