---
id: "DRY-RUN-SCENARIO-001"
type: "dry_run_scenario"
version: "1.0.0"
created: "2026-05-13"
owner: "Privacy Officer (Gustavo Schneiter interim)"
scenario_name: "PII Leak via Application Log"
expected_severity: "SEV-2"
expected_jurisdictions: ["BR", "EU"]
time_to_decision_target_hours: 4
---

# Dry-Run Scenario 1: PII Leak via Application Log

> **Cadence**: Use in Q1 semestral dry-run (January–February)
> **Participants**: Privacy Officer + Legal + Security Lead + Compliance

## 1. Scenario Description

A stack trace in the Loki/Grafana log system contains the email addresses of
approximately 50 CoreLink users. The stack trace was generated during an
exception in the DSR request processing pipeline and inadvertently logged
with the full request payload — including the `subject_email` field that
should have been redacted by the CTRL-PRIV-014 minimization middleware.

The log entries are visible to any user with Loki viewer access (internal SRE team).
No external party has been confirmed to have accessed the logs.

## 2. Simulation Timeline

| T | Event | Participant |
|---|---|---|
| T+0 | Security Lead notices anomalous Loki query pattern in audit log | Security Lead |
| T+15m | Security Lead escalates to Privacy Officer via PagerDuty `corelink-breach-sev3` | Security Lead |
| T+30m | Privacy Officer reviews log samples; confirms PII (email addresses) present | Privacy Officer |
| T+45m | Privacy Officer upgrades to `corelink-breach-sev2` (50 titulares < 100 threshold for SEV-2 lower bound — borderline; Privacy Officer judgment) | Privacy Officer |
| T+1h | Legal engaged; triage team assembled | Privacy Officer + Legal |
| T+2h | Scope confirmed: 50 users affected; emails only; no passwords/financial data | Security Lead |
| T+3h | Decision tree consultation (participants execute §2 of RB-BREACH-NOTIF) | All |
| T+4h | Target: decision finalized + templates filled-in (mock data) | Privacy Officer |

## 3. Decision Tree Exercise

Participants MUST work through the following decision points:

**Step 1 — Severity Classification** (RB-BREACH-NOTIF §2):
- PII affected? YES (email addresses).
- Count: ~50 titulares.
- Sensitive data (Art. 5 II LGPD)? NO (emails are ordinary PII, not sensitive).
- Cross-tenant? NO (single-tenant log access, internal SRE only).
- → **Expected result: SEV-2** (100–1000 range is borderline; Privacy Officer judgment = SEV-2 given internal access only; alternatively SEV-3 if < 100 threshold applied — document decision rationale).

**Step 2 — Jurisdiction determination**:
- Brazilian users affected? YES → ANPD notification required.
- EU users affected? *(check user locale data)* — assume YES for this exercise → Irish DPC notification required.
- California users? *(check)* — assume NO for this exercise (SEV-2 CCPA evaluated case-by-case per decision tree).
- → **Expected: ANPD + Irish DPC**.

**Step 3 — Template selection**:
- `lgpd-anpd-template.pt-br.md` for ANPD.
- `gdpr-irish-dpc-template.en.md` for Irish DPC.
- Customer notifications: `customer-breach-notification.{pt-BR,en-US,es-MX}.mjml` (all 3 locales per mandatory GA requirement).

**Step 4 — Fill-in mock variables**:

```yaml
breach_id: "DRY-RUN-2026-Q1-001"          # MOCK — NOT a real incident
breach_detected_at: "2026-01-15T14:23:00Z"  # MOCK
data_categories_affected: "Email addresses (50 users)"
count_subjects_affected: 50
containment_actions: |
  - Loki log purge initiated via /loki/api/v1/delete API (RB-DSR-ERASURE-INCOMPLETE cross-ref)
  - CTRL-PRIV-014 minimization middleware patched to redact subject_email from error context
  - SRE Loki viewer access audit: confirmed no external access
mitigation_offered: |
  - CTRL-PRIV-014 patch deployed within 4h of detection
  - All affected users notified by email
  - 6-month monitoring period for anomalous login activity
contact_email: "privacy@hugr.dev"
breach_severity: "SEV-2"
```

**Step 5 — Mock notification (DO NOT SEND)**:
- Fill templates with mock data above.
- Review templates for correctness and completeness.
- Note any gaps or ambiguities in templates (→ action items).
- Simulate email dispatch (DO NOT ACTUALLY SEND to ANPD/DPC/customers).

## 4. Measurement

- **T0**: when Privacy Officer calls "START" at Step 3 entry.
- **T_decision**: when all decision points resolved + templates filled (mock).
- **Target**: T_decision - T0 ≤ 4h.
- **Record**: `corelink_breach_time_to_decision_seconds` histogram (mock increment only).

## 5. Evidence Collection (EVT-017)

- Record asciinema or screen recording of decision tree walkthrough.
- Archive at: `r2://evidence-runbooks/<YYYY-MM-DD>-rb-breach-notif-dry-run-scenario-1.cast`
- Document outcome: `passed` (≤ 4h) or `passed_with_concerns` (> 4h + action items).
- File post-dry-run review within 7 days.

## 6. Pass Criteria

| Criterion | Target | Actual (fill) |
|---|---|---|
| Decision tree completed | ≤ 4h | *(fill)* |
| Correct severity classified | SEV-2 | *(fill)* |
| Correct jurisdictions identified | ANPD + Irish DPC | *(fill)* |
| Templates filled-in (mock) | All 8 canonical vars | *(fill)* |
| All 3 customer locales covered | YES | *(fill)* |
| EVT-017 evidence archived | R2 cast file | *(fill)* |

## 7. Known Complexities (for facilitator)

- SEV-2 vs SEV-3 borderline (50 users < 100): Privacy Officer judgment exercise. Document reasoning.
- CCPA SEV-2 case-by-case: participants should discuss whether California notification is warranted.
- Loki deletion SLA: cross-reference RB-DSR-ERASURE-INCOMPLETE (WI-S11-002) for backend timing.
