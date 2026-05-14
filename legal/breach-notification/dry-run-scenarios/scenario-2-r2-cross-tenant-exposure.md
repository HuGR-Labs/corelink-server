---
id: "DRY-RUN-SCENARIO-002"
type: "dry_run_scenario"
version: "1.0.0"
created: "2026-05-13"
owner: "Privacy Officer (Gustavo Schneiter interim)"
scenario_name: "R2 Cross-Tenant Data Exposure"
expected_severity: "SEV-1"
expected_jurisdictions: ["BR", "EU", "US-CA"]
time_to_decision_target_hours: 4
---

# Dry-Run Scenario 2: R2 Cross-Tenant Data Exposure

> **Cadence**: Use in Q3 semestral dry-run (July–August)
> **Participants**: Privacy Officer + Legal + Security Lead + Compliance + CEO interim

## 1. Scenario Description

A bug in the CAS (Content-Addressable Storage) refcount-aware erasure path caused
build artifacts from Tenant A to be returned in response to API requests from Tenant B.
The affected window is 6 hours. During this window, approximately 1,200 distinct
build artifacts were cross-exposed.

The artifacts may contain compiled code, source file paths, environment variable names
embedded in build logs, and potentially developer email addresses from git commit
metadata embedded in build artifacts. The vulnerability was triggered by an edge case
in the `R2CasBackend` adapter where `subject_unaffiliated` vs `subject_dedicated`
classification failed under concurrent refcount updates.

INV-TENANT-ISOLATION is violated (CRITICAL invariant breach).

## 2. Simulation Timeline

| T | Event | Participant |
|---|---|---|
| T+0 | Automated alert: INV-TENANT-ISOLATION canary fires; cross-tenant AC cache hit | Monitoring |
| T+10m | SRE on-call receives PagerDuty SEV-1 | SRE |
| T+20m | SRE escalates: data potentially crossed tenant boundary | SRE |
| T+30m | Privacy Officer paged via `corelink-breach-sev1`; CEO interim paged | PagerDuty |
| T+45m | All participants convened (Privacy Officer + Legal + Security Lead + CEO) | All |
| T+1h | Scope investigation initiated: identify affected tenant pairs + artifact count | Security Lead |
| T+2h | Scope confirmed: 1,200 artifacts; ~1,500 affected principals across tenants | Security Lead |
| T+2h30m | Decision tree consultation begins | All |
| T+4h | Target: decision finalized + all 3 templates filled (mock) | Privacy Officer |

## 3. Decision Tree Exercise

**Step 1 — Severity Classification** (RB-BREACH-NOTIF §2):
- PII affected? POTENTIALLY YES (dev emails in git metadata; source paths).
- Count: estimated 1,200+ principals across cross-tenant exposure.
- Sensitive data? NO (build artifacts, not health/financial/biometric).
- Cross-tenant exposure? YES (INV-TENANT-ISOLATION violated).
- → **Expected result: SEV-1** (> 1000 titulares + cross-tenant = SEV-1 per decision tree).

**Step 2 — Jurisdiction determination**:
- Brazilian users? YES → ANPD.
- EU users? YES → Irish DPC.
- California users? YES (> 1000 threshold met) → California AG.
- → **Expected: ALL 3 jurisdictions**.

**Step 3 — Template selection**:
- ALL 3 jurisdictional templates required (SEV-1).
- ALL 3 customer notification locales required.
- Status page banner MUST be activated (SEV-1 per decision tree).
- CEO interim must be kept informed.

**Step 4 — Fill-in mock variables**:

```yaml
breach_id: "DRY-RUN-2026-Q3-002"               # MOCK — NOT a real incident
breach_detected_at: "2026-07-22T09:15:00Z"      # MOCK
data_categories_affected: |
  - Build artifact metadata (source file paths, environment variable names)
  - Developer email addresses embedded in git commit metadata within artifacts
  - Compiled code (may contain proprietary algorithms)
  Note: NO passwords, financial data, or LGPD Art. 5 II sensitive data confirmed.
count_subjects_affected: 1500  # approximate across affected tenant pairs
containment_actions: |
  - Affected R2 cross-tenant ACLs immediately revoked
  - CAS refcount race condition patched (PR #XXXX deployed within 2h)
  - Affected artifact objects quarantined pending forensic review
  - INV-TENANT-ISOLATION canary monitoring enhanced
mitigation_offered: |
  - Forensic report to each affected tenant within 48h
  - Option for affected tenants to rotate all build keys and re-run affected pipelines
  - 12-month enhanced monitoring for affected accounts
  - Root cause analysis published in public post-mortem within 14 days
contact_email: "privacy@hugr.dev"
breach_severity: "SEV-1"
```

**Step 5 — Status page activation** (RB-BREACH-NOTIF §7):
- Participants MUST walk through the status page banner activation procedure.
- Mock: `curl -X POST https://api.cloudflare.com/...` (do not execute against production).
- Verify banner.json update procedure.

**Step 6 — PagerDuty escalation validation**:
- Confirm `corelink-breach-sev1` escalation policy is correct: Privacy Officer (15m) → Legal (30m) → Security Lead (45m) → CEO interim (60m).
- Walk through escalation timeline from T+0 above.

## 4. Measurement

Same as Scenario 1. Target: T_decision - T0 ≤ 4h.

## 5. Evidence Collection (EVT-017)

Archive at: `r2://evidence-runbooks/<YYYY-MM-DD>-rb-breach-notif-dry-run-scenario-2.cast`

## 6. Pass Criteria

| Criterion | Target | Actual (fill) |
|---|---|---|
| Decision tree completed | ≤ 4h | *(fill)* |
| Correct severity classified | SEV-1 | *(fill)* |
| Correct jurisdictions identified | ANPD + Irish DPC + California AG | *(fill)* |
| All 3 templates filled-in (mock) | 8 vars each | *(fill)* |
| Status page banner procedure walked through | YES | *(fill)* |
| CEO interim briefed per escalation matrix | YES | *(fill)* |
| EVT-017 evidence archived | R2 cast file | *(fill)* |

## 7. Known Complexities (for facilitator)

- INV-TENANT-ISOLATION is CRITICAL — participants should understand the regulatory
  and contractual implications of cross-tenant exposure for enterprise customers.
- Legal privilege: discussion of internal root cause should note that Legal involvement
  may trigger attorney-client privilege considerations in some jurisdictions.
- California AG threshold: confirm ≥ 500 CA resident threshold is met; if uncertain,
  consult Legal (conservative approach = notify).
- Audit event ordering: participants should note the `breach.notification_dispatched.v1`
  event must be emitted AFTER dispatch (regulatory SLA priority; audit retry if emit fails).
