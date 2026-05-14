---
id: "RB-SUB-PROCESSOR-BROADCAST-MISS"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-05-13"
owner: "Privacy Officer + SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "rb", "sub-processor", "broadcast", "fm-453", "s11", "canonical"]
---

# RB-SUB-PROCESSOR-BROADCAST-MISS — Sub-Processor Change Broadcast Miss SOP

> **FM:** [FM-453](../../03_architecture/failure_modes.md) — P1 (S=4) | **WI:** WI-S11-005 | **CTRL:** CTRL-PRIV-021 | **SLA:** detect ≤ 1h, remediate ≤ 24h (preserve 30d window)

## 1. Overview

Sub-processor change broadcast miss occurs when delivery confirmation gap > 5%
after 24h window, or when any subscribed customer does not receive the 30-day
advance notice email within the 30-day window.

**Regulatory consequence**: GDPR Art. 28.2 + LGPD Art. 39 violation if the
30-day notification SLA is not met. May require GDPR Art. 33 supervisory
authority notification if breach affects rights and freedoms.

## 2. Triggers

| Trigger | Severity | Alert Channel |
|---|---|---|
| delivery_rate < 95% after 24h post-broadcast | SEV-2 | PagerDuty + Privacy Officer |
| Any customer misses T+30d window | SEV-1 | PagerDuty + Privacy Officer + Legal |
| DKIM signing failure | SEV-2 | PagerDuty + SecLead |
| Cross-tenant broadcast leak | SEV-1 (CRITICAL) | SecLead + Privacy + Compliance |

## 3. SOP Steps

### Step 1 — Classify Failure Mode (≤ 1h)

Query D1 `sub_processor_broadcast_log`:

```sql
SELECT delivery_status, delivery_error_class, COUNT(*) as count
FROM sub_processor_broadcast_log
WHERE broadcast_id = '<broadcast_id>'
GROUP BY delivery_status, delivery_error_class
ORDER BY count DESC;
```

**Classification by delivery_error_class**:

| Error Class | Root Cause | Action |
|---|---|---|
| `dkim_failure` | DKIM key issue | SecLead + DKIM key rotation (RB-KEY-COMPROMISE) |
| `bounce_hard` | Invalid email | Remediate tenant contact info |
| `bounce_soft` | Transient mailbox | Retry after 24h |
| `cf_email_outage` | Cloudflare Email outage | Wait + retry when resolved |
| `spam_filter` | Spam filter | Review email content + DKIM |
| NULL | Unknown / webhook pending | Wait + investigate CF Email webhook delivery |

### Step 2 — Build Manual Retry List (≤ 2h)

```sql
SELECT log_id, tenant_id, recipient_email_hash, locale, notification_type
FROM sub_processor_broadcast_log
WHERE broadcast_id = '<broadcast_id>'
  AND delivery_status IN ('enqueued', 'bounced', 'failed')
ORDER BY tenant_id;
```

Exclude `complained` entries (respect implicit spam complaint opt-out).

### Step 3 — Escalate if > 5% Violation (≤ 4h)

If `count(non-delivered) / total > 0.05`:
1. Privacy Officer notified immediately.
2. Legal review of DPA exposure.
3. Document incident in `docs/incidents/YYYY-MM-DD-sub-processor-broadcast-miss.md`.

### Step 4 — Execute Retry (≤ 24h)

1. Update `delivery_status = 'enqueued'` for retry candidates.
2. Trigger broadcast cron worker retry mode with filtered log_ids.
3. Monitor CF Email webhook for 24h.
4. Verify delivery rate recovery to ≥ 95%.

### Step 5 — Regulatory Disclosure Assessment

| Threshold | Action |
|---|---|
| > 1% violation rate sustained | Privacy Officer + Legal escalation |
| Any customer misses T+30d window | Consider GDPR Art. 33 supervisory notification |
| > 5% customers miss T+30d | Mandatory GDPR Art. 33 + LGPD Art. 48 review |

**GDPR Art. 33 threshold**: 72h notification to supervisory authority.
Legal + DPO determination required in all cases.

### Step 6 — 30d Window Extension

If delivery delayed > 24h:
- Privacy Officer documents decision to extend 30d window.
- Extend via new broadcast trigger timestamp (restart countdown).
- Direct customer outreach for enterprise customers.

### Step 7 — Post-Incident

1. Update `docs/incidents/<date>-sub-processor-broadcast-miss.md` with full timeline.
2. Update FM-453 incident log.
3. Review DKIM key health (rotation needed?).
4. Verify Cloudflare Email delivery rates for next 30 days.
5. Privacy Officer final sign-off on regulatory exposure.

## 4. Metrics

- `corelink_sub_processor_broadcast_delivery_total{outcome, locale}`
- `corelink_sub_processor_30d_notification_compliance_total{outcome}`
- `corelink_sub_processor_objection_resolution_seconds{outcome}`
- `corelink_sub_processor_page_freshness_lag_seconds`

## 5. Contacts

| Role | Contact |
|---|---|
| Privacy Officer | privacy@hugr.dev |
| SRE Lead | sre@hugr.dev + PagerDuty escalation |
| Security Lead | security@hugr.dev |
| Legal | legal@hugr.dev |
| DPO Interim | dpo@hugr.dev |

## 6. References

- FM-453: `specs/03_architecture/failure_modes.md`
- Sub-processor register: `legal/sub-processors.md`
- Crate: `crates/corelink-privacy-sub-processor-emit/`
- ADR-S11-008 v2: `specs/03_architecture/adrs/ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md`
- WI-S11-005: `specs/04_sprints/S11/work_items/WI-S11-005-sub-processor-register-30d-broadcast-objection-flow.md`
- GDPR Art. 28.2 + LGPD Art. 39 + EDPB 7/2020 §125
