---
id: "RB-DPO-ESCALATION"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "specs/_runbooks/RB-ONCALL-POLICY.md"
inherits_from:
  - "DPO-APPOINTMENT-2026-05-15"
  - "DPO-RESPONSIBILITIES-MATRIX"
  - "LGPD-FULL-AUDIT-2026-05-15"
  - "PRIVACY-MODEL"
tags:
  - "runbook"
  - "dpo"
  - "escalation"
  - "lgpd"
  - "lgpd-art-41"
  - "gdpr-art-39"
  - "privacy"
  - "breach"
  - "anpd"
  - "sla"
  - "gap-01"
---

# RB-DPO-ESCALATION — DPO Consultation + Escalation

> **Purpose:** define **when** internal CoreLink teams must consult or escalate to the interim DPO (Gustavo Schneiter), with **response SLAs** and **escalation tree** including external advisors, Legal, and ANPD as terminal endpoint.
> **Audience:** Engineering, SRE, Support, Security, Sales — every team that may touch a privacy-impacting decision.
> **Parent:** `specs/_runbooks/RB-ONCALL-POLICY.md` (general on-call) + `specs/_runbooks/RB-DSR-TICKET-TRIAGE.md` (DSR-specific intake).
> **Companion docs:** `DPO-APPOINTMENT-2026-05-15.md`; `DPO-RESPONSIBILITIES-MATRIX.md`; `LGPD-DPO-MONTHLY-CHECKLIST.md`.
> **Hard rule:** when in doubt, **escalate**. False-positive escalations are cheap; missed escalations are LGPD Art. 43 sanction triggers.

---

## 1. Scope

### 1.1 In-scope (this runbook applies)

- Any decision that creates, modifies, or removes a privacy-relevant data flow.
- Any incident (security, operational, or contractual) that **may** expose personal data of a data subject.
- Any inbound communication from a regulator (ANPD, EDPB, FTC, state AG, etc.) or peer DPA.
- Any sub-processor or vendor change with PII exposure.
- Any consent-state or `legal_basis` change.

### 1.2 Out-of-scope (use a different runbook)

- Routine DSR intake → `RB-DSR-TICKET-TRIAGE.md` (DPO consulted only when triage flags escalation).
- Security-only vulnerabilities (no PII path) → `RB-SECURITY-VULNERABILITY-INTAKE.md`.
- Pure operational incidents (no privacy delta) → `RB-ONCALL-POLICY.md`.
- BCP/DR drills (no PII data) → `RB-COLD-RESTORE-FROM-ZERO.md`.
- Tabletop exercises → `RB-TABLETOP-TEMPLATE.md`.

---

## 2. Roles

| Role | Person (interim) | Contact (primary) | Contact (fallback) |
|---|---|---|---|
| **DPO / Privacy Officer** | Gustavo Schneiter | `privacy@hugr.com` (public) / `gustavo@humangr.com` (internal) | PagerDuty `dpo-oncall` schedule |
| **External Privacy Advisor (R5-8)** | _pending advisor pool onboarding (target H-15)_ | TBD | TBD |
| **Legal (H-11 law firm)** | _pending H-11 engagement (target Q2-2026)_ | TBD | external advisor pool as bridge |
| **Security Lead** | Gustavo Schneiter (interim) | same as DPO | PagerDuty `security-oncall` |
| **SRE Lead** | Gustavo Schneiter (interim) | same as DPO | PagerDuty `sre-oncall` |
| **Support Lead** | Gustavo Schneiter (interim) | same as DPO | PagerDuty `support-oncall` |
| **Board (for §6 sunset cases)** | Gustavo Schneiter (sole director, interim) | same as DPO | n/a |
| **ANPD (terminal endpoint)** | external regulator | per `legal/breach-notification/anpd-contacts.md` | ANPD online forms |

---

## 3. Escalation triggers — when teams MUST consult DPO

> **Row count:** 12 trigger families. Each row has a **mandatory trigger** (escalation is non-negotiable), **mandatory SLA** (time to DPO acknowledgement), and **resolution path** (when DPO either decides or further escalates).

### 3.1 Trigger: Breach or suspected breach involving PII

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| **Any** PII exposure suspected, regardless of subject count | **≤ 1h** (page DPO via PagerDuty `dpo-oncall`) | DPO declares breach severity per `legal/breach-notification/templates/anpd-incident-form-v2024.md`; downstream per `RB-BREACH-NOTIF.md` |
| Breach affecting **> 100 subjects** OR sensitive-data category (per LGPD Art. 5 §II) | **≤ 1h** + immediate Advisor + Legal countersign on declaration | ANPD notification within 48h internal SLA (Resolução 15/2024 baseline = 3 BD; CoreLink commits 48h); subject notification within 72h after ANPD |
| Breach affecting **a single tenant only** (no cross-tenant impact) | **≤ 4h** | DPO + Support Lead jointly draft tenant-only DPA notification |
| Suspected ransomware / extortion (regulatory + tenant + comms angles converge) | **≤ 30min** SEV-1 page | DPO + Security Lead + Legal + Advisor war room within 1h |
| Lost / stolen device with prod-credential exposure | **≤ 1h** | DPO determines blast radius via `crates/corelink-audit/`; rotates credentials per `RB-SYSTEM-CMK-ROTATION.md` |

### 3.2 Trigger: DSR escalation from triage

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| DSR Erasure scope crosses tenant boundary (cross-tenant subject linkage) | **≤ 2 BD** | DPO authors decision letter; documents in `EVT-042` |
| DSR Erasure refusal grounded in LGPD Art. 18 §4º exception | **≤ 2 BD** | DPO + Legal countersign refusal; documented exception type |
| DSR ambiguity (e.g., subject claims rights on behalf of another, deceased subject, minor as data subject) | **≤ 5 BD** | DPO + Legal + Advisor decision; subject-facing letter from DPO |
| DSR from non-customer (random Brazilian asking for our customers' content) | **≤ 5 BD** | Triage by Support; DPO supervises rejection letter (tenant is the controller; HuGR is operator under DPA) |
| Bulk-erasure batch (≥ 2 subjects in single request, e.g., tenant offboarding) | **≤ 1 BD** to approve | DPO + Advisor countersign per conflict-of-interest rule (`DPO-RESPONSIBILITIES-MATRIX.md §3`) |

### 3.3 Trigger: New sub-processor or vendor change

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| **New sub-processor with any PII exposure** | **≤ 5 BD** before contract signature | DPO veto possible; if approved, vendor added to `legal/sub-processors.md`; DPA + SCCs signed; `VENDOR-RISK-REGISTER.md` updated |
| **Sub-processor region change** affecting `sam`-tenant data | **≤ 5 BD** | DPO checks `crates/corelink-privacy-residency-enforcement/` config impact; may veto |
| **Sub-processor risk-tier reclassification** (Critical → Important or vice versa) | **≤ 10 BD** | DPO + Security Lead joint sign-off; documented in `VENDOR-RISK-REGISTER.md` change log |
| **Sub-processor DPA renewal** with material clause change | **≤ 10 BD** | DPO + Legal review per `RB-DPA-CHANGE.md` |
| **Sub-processor breach notification received** | **≤ 4h** | Cascades to §3.1 trigger family; downstream notification per DPA terms |

### 3.4 Trigger: `purpose_tag` enum or `legal_basis` change

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| New `purpose_tag` proposed in any WI | **≤ 10 BD** before merge | DPO veto possible; if approved, `legal_basis` paired immutably per `LGPD-FULL-AUDIT-2026-05-15.md §1.3` |
| `legal_basis` change for existing `purpose_tag` | **Always blocking** — DPO veto required to merge | If approved, forces `notice_version` major bump + force re-consent campaign |
| LIA (legitimate-interest assessment) authoring for new flow | **≤ 10 BD** | DPO + Advisor + Legal countersign; LIA filed in `legal/lia/` |
| Consent ledger schema migration | **≤ 10 BD** | DPO + Advisor sign-off; backward-compat required (no destructive migration on consent records) |

### 3.5 Trigger: HIGH_RISK WI → DPIA / RIPD required

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| WI declares `privacy_tag: HIGH_RISK` in sprint contract `§16` | **At WI kickoff** | DPO authors or supervises authoring of DPIA from `legal/dpia/template.md` |
| WI declares new ML model training on user data | **At WI kickoff** | DPIA + LIA + opt-in consent design; DPO veto if k-anon < 50 |
| WI declares cross-tenant feature (e.g., benchmarks, dedup opt-in) | **At WI kickoff** | DPIA + tenant-isolation review (`CTRL-ISO-001..005`) |

### 3.6 Trigger: Cross-border data-flow change (LGPD Art. 27 + Art. 33 §1º)

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| Any change to `corelink-privacy-residency-enforcement` rules | **≤ 10 BD** before merge | DPO veto possible; if approved, residency attestation must be refreshed |
| New region added to `primary_region` enum | **≤ 10 BD** | DPO + SRE Lead joint review; residency attestation refresh required |
| Bug or incident causing cross-region write rejection (`write_rejected_cross_region.v1` CloudEvent) spike > 10/day per tenant | **≤ 4h** | Per `LGPD-DPO-MONTHLY-CHECKLIST.md` item 6; tenant-side investigation |

### 3.7 Trigger: Inbound regulatory or peer-DPA communication

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| Inbound from ANPD (any channel) | **≤ 4h** acknowledgement; **≤ 5 BD** substantive response | See §5 below |
| Inbound from EDPB / state AG / FTC / equivalent peer | **≤ 1 BD** acknowledgement | DPO + Legal joint response |
| Inbound subpoena or court order touching PII | **≤ 4h** | DPO + Legal mandatory; refer to `RB-DSR-LGPD-FULL.md §5` (special cases) |
| Inbound from tenant requesting DPA-clause-specific compliance evidence | **≤ 5 BD** | DPO + Support Lead joint response; uses `AUDITOR-WALKTHROUGH-SCRIPT.md` if SOC 2 walkthrough |

### 3.8 Trigger: Privacy-impacting code or infrastructure change

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| PR touching `crates/corelink-privacy-*` or `crates/corelink-dsr/` | **At PR open** — DPO must be tagged reviewer | DPO approval required; conflict-of-interest rules per `DPO-RESPONSIBILITIES-MATRIX.md §3` |
| PR adding new audit event family (`EVT-*`) with PII content | **At PR open** | DPO confirms schema + retention + tenant-isolation |
| Infrastructure change touching `audit-sam` or `audit-*` R2 buckets (Object Lock) | **≤ 5 BD** | DPO + SRE Lead joint review; Object Lock immutability invariant non-negotiable |

### 3.9 Trigger: Consent revocation propagation SLA breach (≤ 5min)

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| `CTRL-PRIV-CONSENT-002` SLO breach (revocation > 5min) | **≤ 4h** | DPO + SRE Lead joint incident review; corrective action documented |
| Consent ledger immutability invariant violation (any mutation to historic consent record) | **≤ 30min** SEV-1 | DPO + Security Lead war room; restore from R2 Object Lock |

### 3.10 Trigger: Retention boundary breach

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| GC job fails to purge data beyond retention window | **≤ 1 BD** | DPO + SRE Lead joint review; manual purge per `crates/corelink-privacy-erasure-worker/` |
| Manual override event on R2 Object Lock retention (audit-* bucket) | **≤ 4h** | DPO + Security Lead investigation; documented justification or remediation |

### 3.11 Trigger: Customer-facing transparency change

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| Privacy notice text change (any locale) | **≤ 10 BD** | DPO + Legal countersign before publish; `notice_version` bump per change-class rules |
| Sub-processor list change (publish-visible) | **≤ 5 BD** | DPO sign-off; site rebuild within 24h of approval |
| Customer-facing DPIA / RIPD publication | **≤ 10 BD** | DPO drafts; Legal countersigns; published under `apps/docs/docs/explanation/privacy/` |

### 3.12 Trigger: Onboarding of new prod-access engineer or contractor

| Sub-trigger | DPO consultation SLA | Resolution path |
|---|---|---|
| New engineer with prod access (any tier) | **≤ 5 BD** before access grant | DPO confirms LGPD training enrolled per `LGPD-DPO-MONTHLY-CHECKLIST.md` item 23 |
| New contractor with PII path access | **≤ 5 BD** | DPO + Legal countersign NDA + LGPD addendum |
| Offboarding of any prod-access individual | **≤ 1 BD** | DPO confirms credential revocation per `RB-SYSTEM-CMK-ROTATION.md` |

---

## 4. Response SLA matrix (summary)

| Severity class | DPO acknowledgement SLA | Resolution / decision SLA | Examples |
|---|---|---|---|
| **SEV-1 (privacy)** | 30 min | 4h to incident commander | Ransomware with PII, consent ledger mutation, prod-credential leak |
| **SEV-2 (privacy)** | 1h | 1 BD | Single-tenant breach < 100 subjects, single DSR escalation |
| **SEV-3 (privacy)** | 4h | 5 BD | Sub-processor onboarding, DPIA authoring, privacy-notice text change |
| **SEV-4 (privacy)** | 1 BD | 10 BD | New `purpose_tag` proposal, LIA authoring, sub-processor risk reclassification |
| **Routine (monthly)** | per checklist | per checklist | `LGPD-DPO-MONTHLY-CHECKLIST.md` items |

> **DPO unavailability:** if the interim DPO is unreachable within the acknowledgement SLA, the **External Privacy Advisor (R5-8)** is the first fallback, then **Legal (H-11)** as second fallback. Advisor onboarding closure is gated on H-15 in `ROADMAP-TO-GA.md §9`. Until advisor pool is staffed, fallback is sole DPO + Board (interim: same individual). This gap is documented as residual risk in `LGPD-FULL-AUDIT-2026-05-15.md §4`.

---

## 5. Escalation tree — DPO → Advisor → Legal → ANPD

```
Detection (any team)
    │
    ▼
Page DPO via PagerDuty `dpo-oncall` (or email privacy@hugr.com for non-urgent)
    │
    ├── [Acknowledged within SLA per §4]
    │       │
    │       ▼
    │   DPO decides per `DPO-RESPONSIBILITIES-MATRIX.md` §2 (unilateral / veto / countersigned)
    │       │
    │       ├── [Solo decision] → log in `EVT-049` or relevant event family → close
    │       │
    │       └── [Requires countersign]
    │               │
    │               ▼
    │           External Privacy Advisor (R5-8)
    │               │
    │               ├── [Advisor countersign sufficient] → close
    │               │
    │               └── [Material legal exposure]
    │                       │
    │                       ▼
    │                   Legal (H-11 law firm)
    │                       │
    │                       ├── [Internal resolution sufficient] → close
    │                       │
    │                       └── [Regulator notification required]
    │                               │
    │                               ▼
    │                           ANPD notification per Art. 48 + Resolução 15/2024
    │                               │
    │                               ├── 48h internal SLA (CoreLink commitment)
    │                               ├── 3 BD external SLA (ANPD baseline)
    │                               └── Subject notification 72h after ANPD
    │
    └── [NOT acknowledged within SLA]
            │
            ▼
        Fallback chain (Advisor → Legal → Board)
            │
            ▼
        Document SLA breach in `specs/_audits/dpo-sla-breach-YYYY-MM-DD.md` for monthly review
```

### 5.1 ANPD inbound communication handling

When ANPD initiates contact (audit notice, inquiry, complaint forwarded from subject, RIPD demand under Art. 38):

1. **Acknowledge to ANPD within 4h** (auto-reply from `privacy@hugr.com` + manual confirmation).
2. **Open internal ticket** in `legal/anpd-comms-log.md` (append-only).
3. **Convene response team** within 1 BD: DPO + Advisor + Legal.
4. **Draft response** within 5 BD (or per ANPD-specified deadline, whichever shorter).
5. **DPO signs response** before transmission; Advisor + Legal countersign for any material commitment or evidence handover.
6. **Archive** outbound and ANPD reply in `legal/anpd-comms-log.md` with retention 10 years.

### 5.2 Subject-initiated ANPD complaint (cascaded from regulator)

If ANPD forwards a subject complaint:

1. Acknowledge cascade per §5.1 above.
2. **Identify the DSR record** (if any) via `crates/corelink-dsr/` lookup — was this complaint already handled internally?
3. If yes: ANPD response includes audit-trail evidence (`EVT-048` + `EVT-042`).
4. If no: open expedited DSR ticket; treat as `SEV-2` per §4.
5. ANPD response with corrective action within 5 BD of ANPD's cascade.

---

## 6. Contact reference

> **Hard rule:** these contacts are stable but their **persons** (interim accumulation) will change on DPO appointment. Refresh this section on every `DPO-APPOINTMENT-{YYYY-MM-DD}.md` cycle.

| Contact | Public surface | Internal surface | PagerDuty schedule |
|---|---|---|---|
| DPO inbox | `privacy@hugr.com` | `gustavo@humangr.com` | `dpo-oncall` |
| Security inbox | `security@hugr.com` | same as DPO (interim) | `security-oncall` |
| Support inbox | `support@hugr.com` | same as DPO (interim) | `support-oncall` |
| SRE oncall | n/a (not public) | n/a | `sre-oncall` |
| Breach hotline | `breach@hugr.com` (auto-forwards to DPO + Security) | n/a | `incident-oncall` |
| ANPD (external) | https://www.gov.br/anpd/pt-br/canais_atendimento | per `legal/breach-notification/anpd-contacts.md` | n/a |
| External Advisor pool (R5-8) | n/a | pending H-15 onboarding | n/a |
| Legal (H-11) | n/a | pending H-11 engagement | n/a |

---

## 7. Cross-references

- **DPO appointment:** `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md`
- **DPO responsibilities + RACI:** `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md`
- **DPO handoff plan:** `specs/_compliance/DPO-HANDOFF-PLAN.md`
- **DPO monthly checklist:** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- **LGPD full audit:** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md`
- **Breach notification runbook:** `legal/breach-notification/` + `RB-BREACH-NOTIF.md`
- **DSR triage runbook:** `specs/_runbooks/RB-DSR-TICKET-TRIAGE.md`
- **DSR full runbook:** `specs/_runbooks/RB-DSR-LGPD-FULL.md`
- **Oncall policy:** `specs/_runbooks/RB-ONCALL-POLICY.md`
- **DPA change runbook:** `specs/_runbooks/RB-DPA-CHANGE.md`
- **Vendor quarterly review:** `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md`
- **Compliance matrix:** `specs/03_architecture/compliance_matrix.md`
- **Privacy model:** `specs/03_architecture/privacy_model.md`
- **ANPD contacts:** `legal/breach-notification/anpd-contacts.md`

---

## 8. Sign-off

| Role | Name | Signature | Date |
|---|---|---|---|
| Interim DPO | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| SRE Lead | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Security Lead | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Support Lead | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| External Privacy Advisor (R5-8) | _pending advisor onboarding_ | `__________________` | _2026-__-__ |

---

**Fim de RB-DPO-ESCALATION.** Refresh annually OR on permanent DPO appointment OR on any change to escalation tree (whichever earliest). Dry-run quarterly per `IR-TABLETOP-SCHEDULE-2026.md` (privacy track).
