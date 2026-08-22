---
id: "CUSTOMER-COMM-BREACH-DSR-PIPELINE-DEGRADATION-EN"
type: "customer_communication_template"
template_class: "breach_notification"
incident_class: "dsr_pipeline_temporary_degradation"
locale: "en"
jurisdiction_anchor: "EU + US"
legal_basis: "GDPR Art. 34 (communication to data subject) c/c Art. 33(2)(c) (loss of availability is a notifiable breach); CCPA §1798.82 where applicable"
version: "1.0.0"
created: "2026-05-15"
owner: "Privacy Officer (Gustavo Schneiter interim)"
parent_runbook: "RB-BREACH-NOTIFICATION"
parent_wi: "WI-S11-006"
wave: "wave-18"
audit_event_triggers:
  - "corelink.privacy.statuspage_publish_failed.v1"
  - "corelink.privacy.dsr_sla_breach.v1"
delivery_channel: "Cloudflare Email transactional + status page banner"
legal_review_status: "PENDING"
native_speaker_reviewed: true
template_variables:
  - "{{breach_id}}"
  - "{{breach_detected_at}}"
  - "{{customer_name}}"
  - "{{tenant_id_hex8}}"
  - "{{outage_window_from}}"
  - "{{outage_window_to}}"
  - "{{outage_duration_hours}}"
  - "{{affected_dsr_kinds}}"
  - "{{affected_request_count}}"
  - "{{containment_actions}}"
  - "{{estimated_resolution_eta}}"
  - "{{incident_status_url}}"
  - "{{customer_action_required}}"
  - "{{contact_email}}"
  - "{{breach_severity}}"
---

# Customer Notification — Temporary Degradation of the Data-Subject Rights (DSR) Pipeline

> **This is a mandatory personal-data-breach communication under GDPR Article 34 read together with Art. 33(2)(c) — which recognises loss of availability of personal data as a notifiable breach.** The template must be filled in by the Privacy Officer and reviewed by Legal before dispatch. Remove this banner before sending.

---

**Subject:** `[{{breach_severity}}] CoreLink — Temporary degradation of the Data-Subject Rights pipeline affecting your tenant ({{breach_id}})`

**From:** privacy@hugr.dev
**To:** {{customer_name}}
**Dispatched at:** *(fill in with UTC timestamp of send)*

---

Dear {{customer_name}},

In compliance with our notification duty under **GDPR Article 34** (communication of a personal-data breach to the data subject) read together with **Article 33(2)(c)** (loss of availability of personal data is a notifiable breach), and, where Californian residents are affected, **California Civil Code §1798.82**, we are formally notifying you of an extended unavailability of our **Data-Subject Rights (DSR) pipeline** detected on your CoreLink tenant (`{{tenant_id_hex8}}`).

> **Important:** GDPR Art. 32(1)(b) requires controllers to ensure "the ability to restore the availability and access to personal data in a timely manner" — and Art. 33(2)(c) expressly includes loss of availability among notifiable breaches. A DSR-pipeline outage exceeding 24 hours, by impairing the ability to exercise rights under GDPR Art. 15–22, falls within that scope.

## 1. Incident Summary

| Field | Value |
|---|---|
| **Incident ID** | `{{breach_id}}` |
| **Severity** | `{{breach_severity}}` |
| **Detected at** | `{{breach_detected_at}}` (UTC) |
| **Nature** | Extended unavailability of the DSR pipeline (loss of availability — GDPR Art. 33(2)(c)) |
| **Outage window** | `{{outage_window_from}}` → `{{outage_window_to}}` (UTC) |
| **Duration** | `{{outage_duration_hours}}` hours (above the 24-hour threshold) |
| **Affected DSR kinds** | `{{affected_dsr_kinds}}` (subset of the 7 canonical rights) |
| **Pending requests** | `{{affected_request_count}}` |
| **Impacted tenant** | `{{tenant_id_hex8}}` (your tenant — direct notification) |

## 2. What happened

The DSR pipeline — which deterministically executes the 7 canonical data-subject right kinds (access, rectification, erasure, portability, restriction, objection, consent withdrawal) — was unavailable for **{{outage_duration_hours}} consecutive hours** for your tenant. **There is no indication of personal-data exfiltration or confidentiality compromise.** What failed was the ability to **serve active requests**, which affects the data subject's ability to exercise their GDPR Art. 15–22 / CCPA §1798.100–125 rights.

Root-cause analysis is actively running across the following branches:

- **Statuspage publisher failure** orchestrating DSR-job state in the control plane.
- **Erasure-worker queue back-pressure** (cross-backend; 12 canonical backends).
- **External dependency degradation** (Cloudflare D1 / R2 in the affected region).

## 3. Action requested from you

`{{customer_action_required}}`

Typical scenarios:

- **Re-submit pending DSR requests** after we issue the "all clear" notice — no immediate action required.
- **Notify your data subjects** whose DSR request had been forwarded to us during the affected window.
- **Confirm receipt of this notice within 24 hours** by replying to `privacy@hugr.dev`.

> **Guarantee:** every DSR request submitted during the window is **preserved in durable queues** (Durable Object + KV) — zero requests were lost. The outage affected service time (latency), not intake.

## 4. What we have done

1. **Statuspage updated** with the SEV-1 banner (`{{incident_status_url}}`).
2. **DSR queue preserved** in durable storage — zero request loss.
3. **DSR workers scaled horizontally** per the capacity playbook.
4. **Supervisory authority assessment in flight** (GDPR Art. 33 — loss-of-availability ≥ 24h triggers the 72-hour clock; filing with the Irish DPC as our lead supervisory authority subject to triage classification).
5. **Retroactive drain job staged**: upon service resumption, all queued requests will be processed in chronological order with automatic re-notification to the data subject.

## 5. Investigation status and resolution ETA

- **Current state:** partial mitigation in production; pipeline gradually returning to nominal.
- **Full-resolution ETA:** `{{estimated_resolution_eta}}` (UTC).
- **Incident status page:** `{{incident_status_url}}`.
- **Public post-mortem:** within 5 business days of closure, at `https://corelink-docs.humangr.com/trust/incident-history` (reference `{{breach_id}}`).

## 6. Your rights and regulatory complaint channels

We reaffirm that the rights set out in **GDPR Art. 15–22** and **CCPA §1798.100–125** remain **fully enforceable** notwithstanding the transient degradation of the technical channel. The statutory 30-day response window under GDPR Art. 12(3) (or 45-day window under CCPA §1798.130) continues to run from effective receipt of the request — the pipeline outage does NOT suspend the statutory deadline.

If you consider your data-protection rights to have been infringed, you may:

- Submit a request to HuGR directly via `privacy@hugr.dev`.
- Lodge a complaint with our lead supervisory authority, the **Irish Data Protection Commission**: `https://www.dataprotection.ie/en/individuals/raising-concern-we`.
- Lodge a complaint with the **Office of the Attorney General of California**: `https://oag.ca.gov/contact/consumer-complaint-against-business-or-company`.

## 7. Contact

- **DPO (Interim Data Protection Officer):** Gustavo Schneiter
- **DPO email:** `gustavo@humangr.com`
- **Privacy mailbox (canonical channel):** `{{contact_email}}` *(default: `privacy@hugr.dev`)*
- **Dedicated incident channel:** `{{incident_status_url}}`

We are available 24/7 for any questions throughout the incident window.

Sincerely,
**Gustavo Schneiter**
Interim DPO — HuGR Labs Ltda.
`gustavo@humangr.com`

---

**Cross-references:**
- HuGR CoreLink Privacy Notice v1.0.0 (`legal/privacy-notice/v1.0.0/en-US.md`)
- Canonical runbook RB-BREACH-NOTIFICATION (`specs/_runbooks/RB-BREACH-NOTIFICATION.md`)
- GDPR Art. 34 + Art. 33(2)(c) + Art. 32(1)(b); CCPA §1798.82; EDPB Guidelines 9/2022
- WI-S11-001 (DSR API 7 endpoints) + WI-S11-002 (Erasure worker)
