---
id: "RB-BREACH-NOTIF"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Privacy Officer (Gustavo Schneiter interim)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p0", "privacy", "legal", "breach", "lgpd-art-48", "gdpr-art-33", "ccpa-1798-82", "high-risk", "s11"]
parent: "WI-S11-006"
---

# RB-BREACH-NOTIF — Breach Notification Runbook

> **Status**: Production-grade (WI-S11-006 S-11 expansion of Lote 5.12 stub)
> **Owner**: Privacy Officer (Gustavo Schneiter interim)
> **Reviewer**: Legal + Compliance Officer + Security Lead
> **Cadence**: Triggered by SEV-1/2/3 breach incidents; dry-run **semestrally** (Q1 + Q3)
> **SLA legal**: ≤ 72h DPA notification (GDPR Art. 33 + LGPD Art. 48 + CCPA §1798.82)
> **SLO interno**: time-to-decision-tree-completion ≤ 4h (ensures 72h SLA achievable)
>
> **CTRL-PRIV-032**: This runbook satisfies CTRL-PRIV-032 (breach notification runbook executable).
>
> **Canonical decision tree**: `legal/breach-notification/rb-breach-notif-decision-tree.yaml`
> **Invariant**: INV-AUDIT-APPEND-ONLY (§3.6 L116) — breach notification audit event R2 7y Object Lock

---

## Section 1: Decision Tree Summary

The canonical decision tree is located at:
`legal/breach-notification/rb-breach-notif-decision-tree.yaml`

**Severity × Jurisdiction → Notify-Whom-When matrix**:

| Severity | Criteria (summary) | ANPD (BR) | Irish DPC (EU) | CA AG (US-CA) | Customer | Status Page |
|---|---|---|---|---|---|---|
| **SEV-1** | > 1000 titulares OR sensitive data OR cross-tenant | ≤72h | ≤72h | ≤72h | ≤72h | YES |
| **SEV-2** | 100–1000 titulares OR availability loss > 24h | ≤72h | ≤72h | Case-by-case | ≤72h | NO |
| **SEV-3** | < 100 titulares OR pseudonymized data OR near-miss | None | None | None | ≤168h (case-by-case) | NO |

**Clock semantics**: The 72h window starts at `breach_detected_at` (ISO 8601 UTC), NOT at incident declaration or containment. Document `breach_detected_at` immediately.

**ANPD specifics**: ANPD Res. CD/ANPD nº 15/2024 requires notification "preferencialmente em até 2 dias úteis" (≈ 48h business days); conservative interpret = ≤72h calendar (same as GDPR Art. 33).

**CCPA specifics**: "In the most expedient time possible and without unreasonable delay" — ≤72h in majority of California court interpretations.

---

## Section 2: Severity Classification Criteria

### SEV-1 (Critical — ALL 3 jurisdictions)

Declare SEV-1 if ANY of the following:
1. PII of **> 1,000** data subjects affected.
2. **Sensitive data** (LGPD Art. 5 II — racial/ethnic origin, religion, political opinions, trade union membership, health, sex life, biometric data, criminal records; also WebAuthn public keys, financial data).
3. **Cross-tenant data exposure** (INV-TENANT-ISOLATION violated — CRITICAL invariant).
4. **Cryptographic key material** (DEKs, master keys, signing keys) exposed.
5. Breach involves **special categories** under GDPR Art. 9.

**PagerDuty**: `corelink-breach-sev1`
**Escalation**: Privacy Officer (15m) → Legal (30m) → Security Lead (45m) → CEO interim (60m)

### SEV-2 (High — BR + EU; CCPA case-by-case)

Declare SEV-2 if ANY of the following (and NOT SEV-1):
1. PII of **100–1,000** data subjects affected.
2. **Availability loss > 24h** (GDPR Art. 33 includes loss of availability of personal data).
3. **Integrity compromise** affecting a data subset (not confirmed no-PII).
4. Single-tenant exposure (internal access path only; no cross-tenant).

**PagerDuty**: `corelink-breach-sev2`
**Escalation**: Privacy Officer (15m) → Legal (30m) → Security Lead (45m)

### SEV-3 (Low — Internal post-mortem only)

Declare SEV-3 if:
1. PII of **< 100** data subjects affected.
2. **Pseudonymized data only** (no direct identifier; GDPR Recital 26 escape valve).
3. **Confirmed-no-PII near-miss** (system anomaly; no data exfiltration confirmed).
4. Infrastructure incident (audit chain gap, hash mismatch) with no PII disclosure.

**Regulatory notification**: NONE by default. Privacy Officer judgment for escalation.
**PagerDuty**: `corelink-breach-sev3`
**Escalation**: Privacy Officer only

---

## Section 3: Notification Flow Per Jurisdiction

### 3.1 ANPD (BR) — LGPD Art. 48 + Res. CD/ANPD nº 15/2024

**Trigger**: SEV-1 or SEV-2 with Brazilian data subjects.
**Deadline**: ≤ 72h from `breach_detected_at`.
**Template**: `legal/breach-notification/lgpd-anpd-template.pt-br.md`

**Steps**:
1. Open template. Fill all 8 canonical variables (§4).
2. Legal externo review (≤ 2h).
3. Privacy Officer final approval.
4. Send to: `comunicacao@anpd.gov.br`
5. CC: `privacy@hugr.dev` (internal retention copy).
6. Submit via portal (if ANPD portal active): https://www.gov.br/anpd/pt-br/assuntos/comunicacao-de-incidente-de-seguranca
7. Archive sent email with timestamp in R2 `evidence-legal/<breach_id>/anpd-notification-sent.eml`.
8. Emit `dev.hugr.corelink.breach.notification_dispatched.v1` audit event (§4.1).

**Content required** (LGPD Art. 48 §1º):
- I. Nature of affected data and data subjects.
- II. Technical measures in use.
- III. Risks related to the incident.
- IV. Reasons for delay (if > 72h — with documented justification).
- V. Mitigation measures adopted or proposed.

### 3.2 Irish DPC (EU) — GDPR Art. 33 + EDPB Guidelines 9/2022

**Trigger**: SEV-1 or SEV-2 with EU data subjects.
**Deadline**: ≤ 72h from `breach_detected_at` (GDPR Art. 33.1).
**Template**: `legal/breach-notification/gdpr-irish-dpc-template.en.md`

**Steps**:
1. Open template. Fill all 8 canonical variables (§4).
2. Legal externo review (≤ 2h; GDPR Art. 33 compliance focus).
3. Privacy Officer final approval.
4. Submit via Irish DPC breach reporting portal: https://www.dataprotection.ie/en/organisations/data-security/personal-data-breaches
5. Email copy to: `commissioners@dataprotection.ie` (source: dataprotection.ie).
6. Archive submitted notification in R2 `evidence-legal/<breach_id>/dpc-notification-submitted.pdf`.
7. Emit `dev.hugr.corelink.breach.notification_dispatched.v1` audit event.

**Content required** (GDPR Art. 33.3):
- (a) Nature of breach, categories/approximate number of subjects + records.
- (b) DPO contact details.
- (c) Likely consequences.
- (d) Measures taken or proposed.

**Late notification** (> 72h): GDPR Art. 33.1 "accompanied by reasons for the delay". Document delay justification in template.

### 3.3 California AG (US-CA) — CCPA §1798.82

**Trigger**: SEV-1 (≥ 1000 subjects) or SEV-2 case-by-case (≥ 500 California residents per §1798.82(f)).
**Deadline**: "most expedient time possible" — ≤ 72h standard.
**Template**: `legal/breach-notification/ccpa-state-ag-template.en.md`

**Steps**:
1. Open template. Fill all 8 canonical variables (§4).
2. Legal externo review (California privacy law focus; ≤ 2h).
3. Privacy Officer final approval.
4. Submit via AG portal: https://oag.ca.gov/privacy/databreach/reporting
5. Provide SIMULTANEOUS notice to affected California residents (Cal. Civ. Code §1798.82(d)).
6. Archive in R2 `evidence-legal/<breach_id>/ccpa-ag-notification-submitted.pdf`.
7. Emit `dev.hugr.corelink.breach.notification_dispatched.v1` audit event.

**Threshold**: AG notification required when **≥ 500 California residents** are affected (§1798.82(f)). Below this: direct resident notification only.

### 3.4 Customers

**Trigger**: SEV-1 (mandatory) + SEV-2 (mandatory) + SEV-3 (case-by-case).
**Deadline**: ≤ 72h for SEV-1/2; ≤ 168h (7d) for SEV-3 individual awareness.
**Templates**:
- `legal/breach-notification/customer-breach-notification.pt-BR.mjml`
- `legal/breach-notification/customer-breach-notification.en-US.mjml`
- `legal/breach-notification/customer-breach-notification.es-MX.mjml`

**Delivery**:
- SEV-1: Cloudflare Email transactional (3 locales) + status page banner activation.
- SEV-2: Cloudflare Email transactional (3 locales).
- SEV-3: Case-by-case direct email.

**3-locale requirement**: All 3 locales (PT-BR, EN-US, ES-MX) MUST be sent. This is a mandatory GA requirement per sprint contract §10.s11.7 and LGPD Art. 9 (direito à informação acessível). Do NOT send single locale.

---

## Section 4: Templates Fill-in Instructions

**Variables** — consistently used across ALL templates (8 canonical):

| Variable | Description | Example |
|---|---|---|
| `{{breach_id}}` | ULID assigned at incident declaration | `01JTVHXX-...` |
| `{{breach_detected_at}}` | ISO 8601 UTC timestamp of detection | `2026-05-13T14:23:00Z` |
| `{{data_categories_affected}}` | Enum list from privacy_model.md §2 | `email addresses, usage metadata` |
| `{{count_subjects_affected}}` | Approximate integer (NOT exact — GDPR Art. 33 allows "approximate") | `1500` |
| `{{containment_actions}}` | Free-text bullet list of actions taken | `- Credentials rotated\n- Patch deployed` |
| `{{mitigation_offered}}` | Free-text bullet list of remediation + monitoring | `- 12mo identity monitoring` |
| `{{contact_email}}` | DPO/Privacy Officer contact | `privacy@hugr.dev` |
| `{{breach_severity}}` | Severity classification | `SEV-1` |

**Fill-in protocol**:
1. Open the relevant template(s) for the severity + jurisdiction determined in §3.
2. Replace EVERY `{{...}}` placeholder with actual incident data.
3. Remove all "COMPLETION INSTRUCTIONS" sections.
4. Do NOT leave any placeholder unfilled — a missing variable is a compliance gap.
5. Legal review the filled template before dispatch (≤ 2h buffer from decision to dispatch).

### 4.1 Audit Event (INV-AUDIT-APPEND-ONLY)

After EACH regulatory or customer notification dispatch, emit:

```json
{
  "specversion": "1.0",
  "type": "dev.hugr.corelink.breach.notification_dispatched.v1",
  "source": "corelink/breach-notification-runbook",
  "id": "<uuid-v7>",
  "time": "<ISO-8601-UTC>",
  "datacontenttype": "application/json",
  "data": {
    "breach_id": "{{breach_id}}",
    "severity": "{{breach_severity}}",
    "jurisdictions_notified": ["ANPD", "Irish DPC", "California AG"],
    "customer_notifications_sent": true,
    "customer_locales": ["pt-BR", "en-US", "es-MX"],
    "ts": "<ISO-8601-UTC>",
    "retry_attempt": 0
  }
}
```

**Audit fail behavior**: If emit to R2 audit-`<region>` fails:
1. Do NOT block or rollback the notification dispatch (regulatory SLA priority).
2. Fire SEV-1 alert to Security Lead + Compliance.
3. Queue manual re-emit: `corelink-privacy-breach-emit` crate retry with `retry_attempt` incremented.
4. Use `breach_id` as idempotency anchor — same payload, `retry_attempt++`.
5. This is documented-priority behavior per DD audit failure trade-off (WI-S11-006 §9.3), NOT silent-skip.

---

## Section 5: Dry-Run Procedure (Semestral)

**Cadence**: Q1 (January–February) + Q3 (July–August). Ad-hoc tabletop encouraged for new team member onboarding.

**Step-by-step**:

1. **Schedule** (≥ 2 weeks ahead): Privacy Officer schedules 4h tabletop block.
   - Required: Privacy Officer + Legal + Security Lead + Compliance.
   - Optional (for SEV-1 scenario): CEO interim.

2. **Select scenario**: Choose from `legal/breach-notification/dry-run-scenarios/`:
   - Q1: Scenario 1 (PII leak via log — SEV-2)
   - Q3: Scenario 2 (R2 cross-tenant exposure — SEV-1)
   - Ad-hoc / new hire: Scenario 3 (audit chain integrity break — SEV-3 near-miss)

3. **Execute tabletop**:
   - Facilitator (Privacy Officer) reads scenario aloud. Participants MUST NOT have read it in advance.
   - Team works through decision tree from `rb-breach-notif-decision-tree.yaml`.
   - Classify severity + jurisdictions + templates.
   - Fill-in templates with mock data from scenario (§4).
   - Walk through notification flow (§3) WITHOUT actually sending to regulators or customers.
   - Walk through status page banner activation (§7) for SEV-1 scenarios.

4. **Measure time**: `T_decision = time from start of decision tree consultation to all templates filled`.
   - Target: ≤ 4h.
   - If > 4h: `outcome = 'passed_with_concerns'` + document specific blockers as action items.

5. **Record evidence (EVT-017)**:
   - Record asciinema session OR screen recording of the tabletop (with participant consent).
   - Archive at: `r2://evidence-runbooks/<YYYY-MM-DD>-rb-breach-notif-dry-run-<scenario-number>.cast`
   - Increment Prometheus counter: `corelink_breach_dry_run_completion_total{outcome='passed'}` or `{outcome='passed_with_concerns'}`.

6. **Post-dry-run review** (≤ 7 days):
   - Document gaps identified (missing contact info, unclear template variables, process ambiguity).
   - File action items with owners + deadlines.
   - Update this runbook or templates if process changes warranted.
   - If significant process change: file ADR minor.

---

## Section 6: Post-Mortem Hooks (Real Incident)

**Window**: ≤ 14 days from breach declaration (privacy_model.md §11.2).

**Lead**: Privacy Officer + Security Lead.
**Public**: SEV-1 incidents → public post-mortem (status page + blog).
**Internal**: SEV-2/3 → internal post-mortem only.

**Post-mortem content** (EVT-019):
1. Timeline of events (detection → containment → notification → resolution).
2. Root cause analysis (5-why or similar).
3. Impact assessment (data subjects affected, jurisdictions, data categories).
4. Regulatory compliance status (notifications sent + timestamps; any late notification with justification).
5. Action items with owners + deadlines (patch, process change, monitoring update).
6. Lessons learned for next dry-run scenario refresh.

**Cross-references**:
- `specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md` — if erasure failure triggered breach (FM-450).
- `failure_modes.md L283` — FM-061 (audit Object Lock vs DSR legal hold path).
- `failure_modes.md §3.10` — FM-453 (sub-processor breach upstream).
- `privacy_model.md §11` — full breach notification model.

---

## Section 7: Status Page Banner Activation

**Trigger**: SEV-1 only (per decision tree). Privacy Officer judgment for SEV-2.

**Steps**:
1. Update `banner.json` (Cloudflare Pages static asset):

```json
{
  "active": true,
  "severity": "SEV-1",
  "message": "We are investigating a security incident. We have notified relevant data protection authorities. For questions: privacy@hugr.dev",
  "message_pt_BR": "Estamos investigando um incidente de segurança. Notificamos as autoridades competentes. Dúvidas: privacy@hugr.dev",
  "message_es_MX": "Estamos investigando un incidente de seguridad. Hemos notificado a las autoridades competentes. Consultas: privacy@hugr.dev",
  "incident_id": "{{breach_id}}",
  "updated_at": "{{breach_detected_at}}"
}
```

2. Deploy via Cloudflare Pages API (S-09 inheritance pattern):

```sh
# Deploy updated banner.json to Cloudflare Pages
# Substitute PROJECT_NAME and ACCOUNT_ID before executing
curl -X POST \
  "https://api.cloudflare.com/client/v4/accounts/${ACCOUNT_ID}/pages/projects/${PROJECT_NAME}/deployments" \
  -H "Authorization: Bearer ${CF_API_TOKEN}" \
  -H "Content-Type: multipart/form-data" \
  -F "manifest={\"banner.json\":\"/banner.json\"}" \
  -F "banner.json=@./banner.json"
```

3. Verify banner visible at: https://status.hugr.dev

4. **Clear banner** (post-resolution):

```json
{
  "active": false,
  "cleared_at": "<ISO-8601-UTC>",
  "incident_id": "{{breach_id}}"
}
```

---

## Section 8: PagerDuty Escalation Matrix

**3 services configured** (S-09 inheritance):

### corelink-breach-sev1

| Step | Who | Timeout | Channel |
|---|---|---|---|
| 1 | Privacy Officer (Gustavo interim) | 15 min | PagerDuty app + SMS + email |
| 2 | Legal | 30 min | PagerDuty app + SMS |
| 3 | Security Lead | 45 min | PagerDuty app + SMS |
| 4 | CEO interim (Gustavo) | 60 min | PagerDuty app + phone call |

### corelink-breach-sev2

| Step | Who | Timeout | Channel |
|---|---|---|---|
| 1 | Privacy Officer | 15 min | PagerDuty app + SMS |
| 2 | Legal | 30 min | PagerDuty app + SMS |
| 3 | Security Lead | 45 min | PagerDuty app |

### corelink-breach-sev3

| Step | Who | Timeout | Channel |
|---|---|---|---|
| 1 | Privacy Officer | 60 min | PagerDuty app |

**Fallback** (PagerDuty offline): Send email to privacy@hugr.dev + security@hugr.dev + activate status page banner manually.

---

## Appendix A: Key Contacts

| Role | Person | Email | Availability |
|---|---|---|---|
| Privacy Officer (DPO interim) | Gustavo Schneiter | privacy@hugr.dev | 24/7 for incidents |
| Legal (external TBD) | TBD (compliance_matrix.md §9 GAP-01) | legal@humangr.com (internal route) | Business hours + on-call for SEV-1 |
| Security Lead | TBD | security@hugr.dev | 24/7 for SEV-1 |
| ANPD (Brazil) | — | comunicacao@anpd.gov.br | Business hours BR |
| Irish DPC (EU) | — | commissioners@dataprotection.ie | Business hours IE |
| California AG (US-CA) | — | Portal: oag.ca.gov/privacy/databreach/reporting | Business hours CA |

## Appendix B: Regulatory References

| Regulation | Article | Requirement | Deadline |
|---|---|---|---|
| GDPR | Art. 33 | DPA notification | ≤ 72h |
| GDPR | Art. 34 | Data subject notification (high risk) | "Without undue delay" |
| LGPD | Art. 48 | ANPD + subject notification | ≤ 72h (ANPD Res. 15/2024 interpretation) |
| CCPA | §1798.82 | AG + resident notification | "Most expedient time possible" |
| SOC 2 | CC7.4..7.5 | Incident response documentation | Per incident |
| EDPB | Guidelines 9/2022 | Breach notification best practices | Per GDPR Art. 33 |

## Appendix C: Metrics + Observability

Canonical Prometheus metrics (4) — `corelink-privacy-breach-emit` crate:

| Metric | Type | Labels | Description |
|---|---|---|---|
| `corelink_breach_time_to_notification_seconds` | Histogram | `{jurisdiction, severity}` | Time from `breach_detected_at` to dispatch per jurisdiction |
| `corelink_breach_time_to_decision_seconds` | Histogram | `{severity, scenario}` | Time from triage start to decision tree completion |
| `corelink_breach_dry_run_completion_total` | Counter | `{outcome, scenario}` | Dry-run completions — `passed` or `passed_with_concerns` |
| `corelink_breach_customer_notification_delivery_total` | Counter | `{outcome, locale, severity}` | Customer notification delivery outcomes |
