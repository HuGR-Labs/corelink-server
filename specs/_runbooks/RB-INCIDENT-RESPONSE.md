---
id: "RB-INCIDENT-RESPONSE"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-06-18"
updated: "2026-06-18"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "incident-response", "ir", "security", "breach", "sev1", "sev2", "sev3", "pagerduty", "compliance", "gdpr", "lgpd", "ccpa", "dpa", "pci-dss"]
inherits_from: ["SECURITY-MODEL"]
---

> **STATUS: DRAFT — pending owner + legal review. Reflects the current repo's
> IR capability; verify against actual operational process before treating as
> binding.**
>
> This runbook is cited as a compliance commitment by the DPA §9 (all 3 locales),
> the PCI-DSS SAQ-A Q19, and a sealed S20 adversarial-summary finding. It is a
> **binding-class artifact** once approved. Until the header above is removed it
> describes the *intended* incident-response flow assembled from the artifacts
> that already exist in this repo; steps that depend on an owner decision or a
> not-yet-built capability are marked **⚠️ OWNER-GATED** or **⚠️ NOT-YET-BUILT**.

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §1.2, §7, §10.

# RB-INCIDENT-RESPONSE — Master Incident-Response Runbook

> **Owned by:** Incident Commander (L1 by default; see roles). **Triggered by:**
> any PagerDuty page on a CoreLink production/staging service, any SLO
> burn-rate alert, any tenant-isolation canary, any signal of confidentiality /
> integrity / availability loss, or any human report of a suspected incident.
>
> **Scope.** This is the **general IR plan**. It owns *detection → triage →
> containment → eradication/recovery → handoffs*. It does **not** restate the
> rotation policy, the severity×tier×SLA matrix, the breach-notification
> decision tree, or the postmortem process — those are owned by the documents
> linked in §10 and this runbook **hands off** to them at the correct step.

---

## 1. Roles

Aligned with `RB-ONCALL-ESCALATION-MATRIX.md` §1 (tier definitions) — this table
names the IR-specific responsibilities, not a new rotation.

| Role | Responsibility |
|---|---|
| **Incident Commander (IC)** | L1 on-call by default; transitions to L2 when L2 joins, to L3 only if L3 explicitly takes the role. Owns the timeline, the severity call, and step sequencing through this runbook. |
| **L1 — Primary on-call engineer** | First responder. Acks the page, opens the incident channel, executes containment runbooks, posts status updates. |
| **L2 — Engineering manager on-call** | Customer-comms approval, status-page orange/red, deploy-freeze. Pre-GA: Gustavo is the permanent L2 backstop. |
| **L3 — CTO / Legal on-call** | Public-statement approval, **regulator-notification authorisation**, ToS invocation, CMK emergency revoke. Pre-GA: CTO = Gustavo; Legal-on-call rotation. |
| **Security Lead** | Forensics, blast-radius determination, drives any confidentiality/integrity sub-track; co-owns the breach decision (§5). |
| **Privacy Officer** | **Decision owner for breach notification** (`rb-breach-notif-decision-tree.yaml`). Pre-GA: Gustavo (interim). |
| **Scribe** | Designated during the bridge; keeps the UTC timeline with `correlation_id` propagated (feeds the postmortem §8). |

**Escalation path + auto-timers** (normative source: `RB-ONCALL-POLICY.md` §5,
mirrored in `RB-ONCALL-ESCALATION-MATRIX.md` §1):

- L1 → L2 after `300s` (5 min) · L2 → L3 after `600s` (10 min) · L3 = manual page-up only.
- **SEV0** (catastrophic — regulator subpoena, forensics-confirmed mass-exfil):
  bypass the matrix, page **all of L1+L2+L3+board chair** simultaneously; procedures
  in `RB-POSTMORTEM-PROCESS.md` §extreme.

---

## 2. Stage A — Detection (T+0)

An incident enters this runbook through one of the **real, wired** detection
sources below. The IC records *which* source fired and the `correlation_id` in
the timeline.

| Detection source | Where it lives | Notes |
|---|---|---|
| **PagerDuty lifecycle/SLO page** | `worker/src/durable_object.ts` `emitLifecycleEvent()` → PagerDuty Change Events API (`https://events.pagerduty.com/v2/change/enqueue`); routing via `PAGERDUTY_ROUTING_KEY`. SLO burn pages via `crates/corelink-slo/src/pagerduty.rs`. | Lifecycle events are emitted **before** the state mutation they describe; `tenant_id` is pre-hashed (`INV-NO-PII-IN-LOGS`). Emit is fire-and-forget — a missed emit is **not** itself evidence of "no incident". |
| **SLO burn-rate alert** | `crates/corelink-slo/` (`alert.rs` multi-burn-rate orchestrator, `window.rs` 4-burn-rate matrix per Google SRE Workbook Ch 5, `decision.rs`). | Burn-rate > 14× maps to SEV2 per matrix §2; sustained full-budget burn → SEV1 candidate. |
| **Tenant-isolation canary** | `INV-TENANT-ISOLATION` canary (see dry-run scenario 2). | **Auto-escalates to SEV1** — cross-tenant boundary breach. |
| **Synthetic monitor / canary** | `crates/corelink-telemetry/` (`canary.rs`, `synthetic_pager/`). | Synthetic vs production pages are routing-key-split so synthetic false-positives don't contaminate prod MTTA (matrix §4). |
| **Logs / observability** | `crates/corelink-telemetry/` (`logpush/`, `otel/`, `tracing.rs`). | Logs are PII-redacted by contract; use `correlation_id`, not raw tenant identifiers. |
| **Human report** | `security@humangr.com`, customer support, on-call discretion. | A vulnerability *report* (no live impact) routes to `RB-SECURITY-VULNERABILITY-INTAKE.md` instead; this runbook is for *incidents* (active or suspected impact). |

> **Boundary with vulnerability intake.** If the trigger is an inbound
> vulnerability report with no confirmed live impact, run
> `RB-SECURITY-VULNERABILITY-INTAKE.md`. That runbook's Stage D
> (active-exploitation) **re-enters this runbook** when a finding causes
> customer impact.

---

## 3. Stage B — Triage & severity classification (T+0 .. ack SLA)

The IC classifies severity using the **canonical** matrix; this runbook does not
define a second severity scheme.

1. **Ack the page** within the matrix SLA (`RB-ONCALL-ESCALATION-MATRIX.md` §3:
   SEV1 ≤ 5 min, SEV2 ≤ 15 min, SEV3 ≤ 30 min in-hours).
2. **Open the incident channel** `#incident-{{incident_id}}` and post the
   declaration (Slack templates in `RB-ONCALL-ESCALATION-MATRIX.md` §7.5–§7.7).
3. **Classify severity** per `RB-ONCALL-ESCALATION-MATRIX.md` §2:

   | Sev | Trigger (summary — matrix §2 is normative) | Customer-facing? |
   |---|---|---|
   | **SEV1** | Production unavailability OR data-integrity violation OR security incident with confirmed blast radius OR **cross-tenant boundary breach**. | YES |
   | **SEV2** | Significant degradation (not full outage); burn-rate > 14×; partial-region; bounded-blast-radius security finding. | Mostly NO |
   | **SEV3** | Minor degradation; burn-rate 1–14×; recoverable within shift; drift/hygiene. | NO |
   | **SEV0** | Catastrophic existential (subpoena, confirmed mass-exfil). | YES — bypass matrix. |

4. **Default UP.** If L1 and L2 disagree on whether to declare SEV1, **declare
   SEV1**; L2 may downgrade after 30 min if data does not support it, and the
   downgrade is logged in audit (matrix §6).
5. **Forced escalations** (carry from `RB-SECURITY-VULNERABILITY-INTAKE.md` §A.5):
   - `INV-TENANT-ISOLATION` violation → **SEV1, forced**.
   - Exploited-in-the-wild → **SEV1, forced**.
6. **Assign IC + Scribe.** IC is L1 until L2 joins (matrix §6 tie-breakers).
7. **Open the incident doc** `specs/_postmortems/INC-YYYY-MM-DD-NNN-<slug>.md`
   from `templates/incident.md`; begin the UTC timeline.

---

## 4. Stage C — Containment (parallel to triage)

Goal: **stop the bleeding** before root-causing. Containment actions are drawn
from the existing per-failure runbooks — this runbook routes to them, it does
not duplicate their procedures.

- [ ] **Status page.** L1 may post yellow; L2 approves orange/red. Initial SEV1
      status post within 5 min of declaration (`RB-ONCALL-ESCALATION-MATRIX.md`
      §7.1; status-page init in `STATUSPAGE-INIT.md`).
- [ ] **Pick the containment runbook** for the failure class:

  | Failure class | Containment runbook |
  |---|---|
  | Cross-tenant / audit-export isolation | `RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md`, `RB-AUDIT-EXPORT-INTEGRITY.md` |
  | Region / availability loss | `RB-ACTIVE-FAILOVER.md`, `RB-REPLICA-FAILOVER.md`, `rb-storage-fallback.md` |
  | CMK / key-material exposure | CMK rotation `RB-SYSTEM-CMK-ROTATION.md`; **⚠️ OWNER-GATED** emergency BYOK revoke (`RB-BYOK-REVOKE.md`, L3 auth) — *referenced by the matrix; confirm the file exists before relying on it.* |
  | Billing / Stripe | `RB-STRIPE-PORTAL-INCIDENT.md`, `RB-WEBHOOK-DLQ-REPLAY.md` |
  | Supply-chain / dependency | `RB-DEPENDABOT-INCIDENT.md` |
  | Backup / restore | `RB-BACKUP-VERIFICATION-FAILURE.md`, `RB-COLD-RESTORE-FROM-ZERO.md` |
  | Secrets drift | `RB-SECRETS-DRIFT.md` |
  | Data-integrity / canonical drift | `RB-CANONICAL-DRIFT.md` |

- [ ] **Preserve evidence first** for any suspected confidentiality/integrity
      incident: snapshot logs, audit-chain entries, and `correlation_id` set
      **before** mutating state (forensics: `docs/internal/FORENSICS-GUIDE.md`
      §7). Audit objects are R2 Object-Lock, 7-year retention.
- [ ] **Deploy mitigations even without a full patch** for active exploitation
      (WAF rules, feature flags, config) — accept SLO impact (carry from
      `RB-SECURITY-VULNERABILITY-INTAKE.md` §5).
- [ ] **Comms holding statement** for any SEV1 with confidentiality/integrity or
      supply-chain dimension: legal-on-call pre-drafts the hold statement within
      30 min (`RB-ONCALL-ESCALATION-MATRIX.md` §7.8). **No claims about scope or
      affected parties until forensics confirms.**

---

## 5. Stage D — Breach-notification decision-tree handoff (CONFIDENTIALITY / INTEGRITY incidents)

**This is the regulatory-critical handoff.** If the incident involves (or may
involve) personal data — exposure, loss of availability > 24h, or
integrity compromise — the **Privacy Officer** runs the breach-notification
decision tree. The IC does **not** make the regulatory call.

**Canonical decision tree (machine-readable):**
`legal/breach-notification/rb-breach-notif-decision-tree.yaml`
(schema `schemas/yaml/breach-notification-decision-tree.json`; source WI-S11-006).

1. **Classify the breach severity** per the decision tree's
   `severity_classification` (SEV-1/2/3 — distinct lens from the ops matrix §3;
   keyed on **data-subject count + data sensitivity + cross-tenant**, not
   availability alone):
   - **SEV-1** — PII of > 1000 subjects, OR sensitive data (LGPD Art. 5 II /
     WebAuthn pubkey / health), OR cross-tenant exposure
     (`INV-TENANT-ISOLATION`), OR cryptographic key material exposed.
   - **SEV-2** — PII of 100–1000 subjects, OR availability loss > 24h, OR
     integrity compromise of a subset, OR single-tenant exposure.
   - **SEV-3** — PII of < 100 subjects, pseudonymized exposure, or confirmed-no-PII
     near-miss → internal post-mortem only (Privacy Officer judgment for
     regulatory escalation).
2. **Internal escalation** — page the right breach service:
   `corelink-breach-sev1` (PO 15m → Legal 30m → Sec Lead 45m → CEO 60m),
   `corelink-breach-sev2`, or `corelink-breach-sev3`. **Time-to-decision SLO: 4h.**
   - **⚠️ NOT-YET-BUILT / verify:** the matrix references PagerDuty services
     `corelink-breach-sev{1,2,3}` and `corelink-security-intake`. Confirm these
     services + escalation policies are provisioned in PagerDuty before relying
     on auto-paging (ROADMAP H-3 PagerDuty provisioning). Until then, **page the
     Privacy Officer manually**.
3. **Regulatory notification** (SEV-1/SEV-2; deadline **72h from awareness**),
   authorised by L3 / Legal — fill the jurisdiction template the tree selects:

   | Authority | Jurisdiction | Template | Legal basis |
   |---|---|---|---|
   | ANPD | BR | `legal/breach-notification/lgpd-anpd-template.pt-br.md` | LGPD Art. 48 + ANPD Res. CD/ANPD nº 15/2024 |
   | Irish DPC | EU | `legal/breach-notification/gdpr-irish-dpc-template.en.md` | GDPR Art. 33 + EDPB Guidelines 9/2022 |
   | California AG | US-CA | `legal/breach-notification/ccpa-state-ag-template.en.md` | CCPA §1798.82 |

4. **Customer notification** (deadline **72h** for SEV-1/2; 168h/7d for SEV-3) —
   localized templates, all three locales for SEV-1:
   `customer-breach-notification.{pt-BR,en-US,es-MX}.mjml`.
   Delivery: Cloudflare Email transactional; **status-page banner for SEV-1**.
5. **PCI-adjacent compromise** (Stripe / cardholder-data outsourcing): also
   notify Stripe (`security@stripe.com`) within 24h and run the **TT-03 Stripe
   webhook compromise** scenario per the PCI-DSS SAQ-A Q19 commitment
   (`specs/_compliance/ir-scenarios/`, `IR-TABLETOP-PLAYBOOK.md`).
6. **Audit event on dispatch:** `dev.hugr.corelink.breach.notification_dispatched.v1`
   (R2 `audit-<region>`, Object-Lock, 7y, idempotency key `breach_id`).
   **Dispatch has priority over audit** — an audit-emit failure does **not**
   block the 72h regulatory notification; it raises a SEV-1 alert to Sec Lead +
   Compliance and re-emits post-recovery (decision tree `audit_event`).

> **Rehearsed scenarios** (Q3 semestral dry-run, facilitator reference):
> `legal/breach-notification/dry-run-scenarios/` — scenario-1 (PII leak via log),
> scenario-2 (R2 cross-tenant exposure), scenario-3 (audit-chain integrity break).

---

## 6. Stage E — Eradication & recovery

- [ ] **Root-cause the trigger** (not just the symptom). Hand the integrity/
      confidentiality root-cause sub-track to the **Security Lead**.
- [ ] **Patch + regression test** the defect. The regression test MUST fail
      before the fix and pass after (carry from
      `RB-SECURITY-VULNERABILITY-INTAKE.md` §B.2). If the finding has a
      Semgrep/CodeQL signature, add a rule under `tools/sast/rules/` so it never
      regresses.
- [ ] **Verify recovery** against the affected SLO and the synthetic monitor
      before declaring resolved.
- [ ] **Data repair** if integrity was affected; document the repair in the
      incident doc §5 (Resolution).
- [ ] **Lift mitigations** (WAF/flag/config) once the permanent fix is verified.
- [ ] **Status page → resolved** (`RB-ONCALL-ESCALATION-MATRIX.md` §7.3);
      total duration recorded.
- [ ] **Close the incident doc** (`status: closed`, all timeline rows merged)
      **within 24h** of resolution — this starts the postmortem clock.

---

## 7. Stage F — Postmortem handoff

Every SEV-1, SEV-2, and any user-visible incident **mandates** a blameless
postmortem. The IC (default author) hands off to the postmortem process; this
runbook does not restate it.

- **Process owner + timeline + sign-off bar:** `RB-POSTMORTEM-PROCESS.md`.
  - Incident doc finalized ≤ 24h · postmortem draft ≤ 5 calendar days ·
    `REVIEW` ≤ 10 days · `SEALED` (two-reviewer sign-off) ≤ 14 days.
- **Special reviewers** the postmortem MUST add when this incident applies
  (`RB-POSTMORTEM-PROCESS.md` §4): **Security Lead** (confidentiality/integrity/
  audit-chain), **Privacy Officer** (PII/BYOK/DSR/residency), **Cost Owner**
  (> $1k/day emergency spend).
- **Blameless pledge** is non-negotiable; the 5-Why must terminate in a system/
  process root, never a person.
- **Action items** are typed (`prevent`/`detect`/`mitigate`/`process`/
  `documentation`) and linked to the in-flight sprint (R-S17-11).

---

## 8. Regulatory & customer notification timeline (DPA commitment)

This table reproduces the timeline CoreLink **commits to in the DPA §9** (all
locales) so the binding deadlines are visible in the operator's runbook.

| Step | Deadline | Source |
|---|---|---|
| Internal escalation of suspected personal-data breach | within **24h** of detection | DPA §9.1 (cites *this* runbook) |
| Customer (Controller) notification | within **72h** of becoming aware | DPA §9.2; GDPR Art. 33; LGPD Art. 48 |
| Supervisory-authority notification (where required) | within **72h** of awareness | Decision tree §`severity_classification` (ANPD / DPC / CA-AG) |
| Notification content — nature, categories + approx. count of subjects/records, likely consequences, measures taken; phased if not all available at once | with the notification (or in phases without undue delay) | DPA §9.3 |
| Cooperation with Controller on supervisory-authority + data-subject notification (high-risk breaches) | ongoing | DPA §9.4 |
| Stripe notification (PCI-adjacent compromise) | within **24h** | PCI-DSS SAQ-A Q19 |
| SLO-breach-triggered incident review / postmortem | per `RB-POSTMORTEM-PROCESS.md` clock | RB-SECURITY-VULNERABILITY-INTAKE §8 |

> **Awareness anchor.** The 72h clock starts at **awareness** — the moment a
> reasonable degree of certainty that a breach has occurred is reached, not the
> moment detection telemetry first fires. Legal-on-call holds the canonical
> interpretation of "awareness" per jurisdiction.

---

## 9. ⚠️ Capability gaps & owner decisions (do not treat as binding until closed)

Honest accounting of what this runbook *describes* vs what is *built/decided*:

- **⚠️ NOT-YET-BUILT:** PagerDuty breach services `corelink-breach-sev{1,2,3}`
  and `corelink-security-intake` are referenced across the matrix + decision
  tree; confirm provisioning (ROADMAP H-3). Manual paging is the fallback.
- **⚠️ VERIFY:** `RB-BYOK-REVOKE.md` (L3 CMK emergency revoke) is referenced by
  the matrix but its presence in `specs/_runbooks/` is not confirmed by this
  draft — verify before an incident depends on it.
- **⚠️ OWNER-GATED:** post-go-live alerting delivery (PagerDuty mobile/email
  paging) is owner-deferred and **unverified end-to-end** — until confirmed,
  treat detection-source paging as best-effort and keep a human on-call.
- **⚠️ OWNER + LEGAL REVIEW:** the regulatory deadlines and template selection in
  §5/§8 are reproduced from the decision tree and the DPA; they must be ratified
  by Legal before this runbook is treated as the authoritative operator copy.
- **Pre-GA staffing:** L2/L3/Privacy-Officer/Security-Lead roles collapse onto the
  founder (Gustavo) pre-GA; the role table assumes single-operator reality.

---

## 10. Cross-references

- `specs/_runbooks/RB-ONCALL-ESCALATION-MATRIX.md` — severity × tier × SLA, PD routing, comms templates (§3, §4, §7 are normative; this runbook defers to it).
- `specs/_runbooks/RB-ONCALL-POLICY.md` — rotation, fatigue, handoff, auto-escalation timers.
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — blameless postmortem process (Stage F handoff).
- `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md` — inbound vulnerability reports + active-exploitation path that re-enters here.
- `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md` — pentest-finding variant.
- `legal/breach-notification/rb-breach-notif-decision-tree.yaml` — canonical breach decision tree (Stage D).
- `legal/breach-notification/` — jurisdiction templates (ANPD / DPC / CCPA), customer locales (pt-BR/en-US/es-MX), dry-run scenarios.
- `legal/dpa/v1.0.0.{en-US,pt-BR,es-419}.md` §9 — the DPA breach-notification commitment that cites this runbook.
- `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md` Q19 — IR-plan attestation citing this runbook.
- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`, `specs/_compliance/ir-scenarios/` — tabletop scenarios (incl. TT-03 Stripe).
- `specs/03_architecture/security_model.md` (SECURITY-MODEL) — STRIDE / trust boundaries / control catalog (inherited).
- `worker/src/durable_object.ts` `emitLifecycleEvent()` — PagerDuty lifecycle emission.
- `crates/corelink-slo/`, `crates/corelink-telemetry/` — SLO burn-rate alerting + synthetic/canary/logpush detection sources.
- `docs/internal/FORENSICS-GUIDE.md` — evidence preservation.
- `specs/_audits/sealed/2026-05-14-s20-adversarial-summary.md` §9 — finding requiring clear cross-team IR ownership.

---

## 11. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 0.1.0 | 2026-06-18 | Gustavo Schneiter | Initial DRAFT authored to close the compliance-doc gap: the DPA §9 (3 locales), PCI-DSS SAQ-A Q19, and the S20 sealed audit cite `specs/_runbooks/RB-INCIDENT-RESPONSE.md`, which did not exist. Grounded in the existing breach-notification artifacts, escalation matrix, postmortem process, vulnerability-intake runbook, PagerDuty wiring, and SLO/telemetry crates. Pending owner + legal review. |

---

**End RB-INCIDENT-RESPONSE (DRAFT).**
