---
id: "CUSTOMER-COMM-BREACH-AUDIT-CHAIN-INTEGRITY-EN"
type: "customer_communication_template"
template_class: "breach_notification"
incident_class: "audit_chain_integrity"
locale: "en"
jurisdiction_anchor: "EU + US"
legal_basis: "GDPR Art. 34 (communication to data subject) + Art. 32 (security of processing); CCPA §1798.82 (resident notification)"
version: "1.0.0"
created: "2026-05-15"
owner: "Privacy Officer (Gustavo Schneiter interim)"
parent_runbook: "RB-BREACH-NOTIFICATION"
parent_wi: "WI-S11-006"
wave: "wave-18"
audit_event_triggers:
  - "corelink.audit.export_verify_failed.v1"
  - "corelink.audit.export_integrity_failed.v1"
delivery_channel: "Cloudflare Email transactional + status page banner (SEV-1)"
legal_review_status: "PENDING"
native_speaker_reviewed: true
template_variables:
  - "{{breach_id}}"
  - "{{breach_detected_at}}"
  - "{{customer_name}}"
  - "{{tenant_id_hex8}}"
  - "{{affected_window_from}}"
  - "{{affected_window_to}}"
  - "{{at_sequence}}"
  - "{{classification}}"
  - "{{containment_actions}}"
  - "{{estimated_resolution_eta}}"
  - "{{incident_status_url}}"
  - "{{customer_action_required}}"
  - "{{contact_email}}"
  - "{{breach_severity}}"
---

# Customer Notification — Audit-Chain Integrity Incident

> **This is a mandatory personal-data-breach communication under GDPR Article 34 (and CCPA §1798.82 where applicable).** The template must be filled in by the Privacy Officer and reviewed by Legal before dispatch to the customer. Remove this banner before sending.

---

**Subject:** `[{{breach_severity}}] CoreLink — Audit-chain integrity incident affecting your tenant ({{breach_id}})`

**From:** privacy@hugr.dev
**To:** {{customer_name}}
**Dispatched at:** *(fill in with UTC timestamp of send)*

---

Dear {{customer_name}},

In compliance with our notification duty under **Article 34 of the General Data Protection Regulation (GDPR)** and **California Civil Code §1798.82** (where Californian residents are affected), we are formally notifying you of a security incident detected on your CoreLink tenant (`{{tenant_id_hex8}}`).

## 1. Incident Summary

| Field | Value |
|---|---|
| **Incident ID** | `{{breach_id}}` |
| **Severity** | `{{breach_severity}}` |
| **Detected at** | `{{breach_detected_at}}` (UTC) |
| **Nature** | Cryptographic verification failure on an audit-log export response (chain break or tampering) |
| **Technical classification** | `{{classification}}` |
| **Affected window** | `{{affected_window_from}}` → `{{affected_window_to}}` (UTC) |
| **Divergent sequence** | `at_sequence = {{at_sequence}}` |
| **Impacted tenant** | `{{tenant_id_hex8}}` (your tenant — direct notification) |

Our server-side verifier detected a Merkle-chain mismatch in a batch of audit events delivered to your `GET /v1/audit/export` endpoint, compared against the canonical daily-verifier checkpoint. **The bytes were delivered to your client; we cannot independently vouch for the integrity of those bytes at the moment of delivery.**

## 2. What happened

This is an audit-chain integrity failure — **at this stage there is no indication of personal-data exfiltration**. The risk this notice addresses is to **regulatory evidentiary value** (you may have relied on these logs for SOC 2 audits, supervisory-authority requests, or litigation). Root-cause analysis is actively running across three branches:

- **Real chain break in R2** (canonical storage): unlikely given R2 Object Lock; under investigation.
- **Tampering of an archived row** (probability near-zero owing to Object Lock).
- **Export-pipeline bug** (medium probability; chain replay in progress).

## 3. Action requested from you

`{{customer_action_required}}`

Typical scenarios:

- **No immediate action required** if you have not shared the affected export with a downstream auditor and have not relied on it as evidence.
- **Re-export the audit log** after we clear the freeze (we will issue an explicit "all clear" notice when the endpoint reopens — currently returning HTTP `503` for your tenant per RB-AUDIT-EXPORT-VERIFY-FAILED §3).
- **Notify your downstream auditor** if you have already shared the affected export, so they are aware that the batch is on integrity-hold.
- **Confirm receipt of this notice within 24 hours** by replying to `privacy@hugr.dev`.

## 4. What we have done

1. **Audit-export endpoint disabled** for your tenant (HTTP `503`) pending investigation.
2. **Audit-chain writes paused** for your tenant — new events are queued in a holding buffer and will be drained back into the chain after rebuild.
3. **Forensic bundle captured** of the affected R2 chunks (BLAKE3-addressed, write-once).
4. **Cryptographic replay running** against the canonical daily-verifier checkpoint.
5. **Supervisory authority notification in preparation** in parallel (GDPR Art. 33 — 72-hour clock running from confirm; we will file with the Irish DPC as our lead supervisory authority once classification is consolidated by triage).

## 5. Investigation status and resolution ETA

- **Current state:** investigation active; technical classification `{{classification}}`.
- **Resolution ETA:** `{{estimated_resolution_eta}}` (UTC). Contractual MTTR ≤ 4h end-to-end; may extend if the rebuild spans a wide R2 window.
- **Incident status page:** `{{incident_status_url}}`.
- **Public post-mortem:** within 5 business days of closure, published at `https://corelink.humangr.com/incidents/{{breach_id}}`.

## 6. Your rights and regulatory complaint channels

This notification is part of our duty under **GDPR Article 34** (communication to the data subject when a personal-data breach is likely to result in a high risk to their rights and freedoms) and, where Californian residents are affected, under **California Civil Code §1798.82**. The content follows the structure required by GDPR Art. 33(3): nature of the breach, approximate number of data subjects and records, likely consequences, and mitigation measures.

If you consider your data-protection rights to have been infringed, you may:

- Submit a request to HuGR directly via `privacy@hugr.dev`.
- Lodge a complaint with our lead supervisory authority, the **Irish Data Protection Commission**: `https://www.dataprotection.ie/en/individuals/raising-concern-we`.
- Lodge a complaint with the **Office of the Attorney General of California**: `https://oag.ca.gov/contact/consumer-complaint-against-business-or-company`.
- Exercise your data-subject rights (access, rectification, erasure, portability, restriction, objection — GDPR Art. 15–22 / CCPA §1798.100–125) via our self-service endpoint `POST /v1/privacy/dsr/{kind}` (authentication required).

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
- Technical runbook RB-AUDIT-EXPORT-VERIFY-FAILED (`specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md`)
- GDPR Art. 34 + Art. 33 + Art. 32; CCPA §1798.82; EDPB Guidelines 9/2022
- INV-AUDIT-APPEND-ONLY (§3.6 L116; R2 Object Lock 7 years)
