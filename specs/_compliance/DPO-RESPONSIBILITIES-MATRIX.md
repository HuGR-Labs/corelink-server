---
id: "DPO-RESPONSIBILITIES-MATRIX"
type: "compliance_raci"
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
inherits_from:
  - "DPO-APPOINTMENT-2026-05-15"
  - "LGPD-FULL-AUDIT-2026-05-15"
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
tags:
  - "lgpd"
  - "lgpd-art-41"
  - "gdpr-art-39"
  - "dpo"
  - "raci"
  - "decision-rights"
  - "responsibilities"
  - "gap-01"
  - "governance"
---

# DPO Responsibilities Matrix — RACI + Decision Rights

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** canonical RACI for every DPO-touched activity at CoreLink, with decision-rights matrix, conflict-of-interest decision rules, and cross-link to operational evidence.
>
> **Companion docs:**
> - `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md` — interim designation, term, independence safeguards.
> - `specs/_runbooks/RB-DPO-ESCALATION.md` — when internal teams must consult DPO + SLAs.
> - `specs/_compliance/DPO-HANDOFF-PLAN.md` — handoff to permanent DPO.
> - `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` — monthly operational cadence (25 items).
>
> **Legend (RACI):**
> - **R** — Responsible (does the work)
> - **A** — Accountable (signs off; only one per row)
> - **C** — Consulted (input before decision)
> - **I** — Informed (notified after decision)
> - **V** — Veto (may unilaterally block; DPO-only)
>
> **Roles legend:**
> - **DPO** — interim DPO / Privacy Officer (Gustavo Schneiter; see appointment doc)
> - **SecLead** — Security Lead (interim: Gustavo Schneiter)
> - **SRELead** — SRE Lead (interim: Gustavo Schneiter)
> - **SupLead** — Support Lead (interim: Gustavo Schneiter)
> - **Eng** — Engineering author of the changeset
> - **Legal** — H-11 law firm (engaged Q2-2026; until then, external advisor pool R5-8)
> - **Advisor** — R5-8 external privacy advisor (countersign during interim window)
> - **Board** — HuGR Labs Board (currently Gustavo Schneiter as sole director)
> - **ANPD** — Autoridade Nacional de Proteção de Dados (external regulator; for "I" cells, indicates ANPD must be informed within the SLA defined in `RB-DPO-ESCALATION.md`)

---

## 1. RACI matrix — DPO-touched activities

> **Row count:** 30 activities (RACI rows). Each row maps to a canonical evidence source + escalation trigger in `RB-DPO-ESCALATION.md` where applicable.

| # | Activity | DPO | SecLead | SRELead | SupLead | Eng | Legal | Advisor | Board | ANPD | Evidence |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | **DSR intake + identity verification** | A | I | — | R | — | — | — | — | — | `RB-DSR-TICKET-TRIAGE.md`; `crates/corelink-dsr/` |
| 2 | **DSR Erasure approval (Art. 18 §IV — destructive)** | A,V | C | I | R | C | — | — | — | — | `RB-DSR-LGPD-FULL.md §5`; `EVT-042` |
| 3 | **DSR Rectification approval (Art. 18 §III — destructive, MFA)** | A | — | — | R | C | — | — | — | — | `INV-DSR-MFA-DESTRUCTIVE`; `crates/corelink-dsr/src/endpoint.rs` |
| 4 | **DSR Access / Portability / Confirmation (non-destructive)** | I | — | — | R | — | — | — | — | — | `EVT-048`; `crates/corelink-dsr/` |
| 5 | **DSR Objection (Art. 18 §II amendment, legitimate-interest pushback)** | A,V | I | — | R | — | C | — | — | — | `legal/lia/security-monitoring.md`; `EVT-046` |
| 6 | **Consent ledger schema evolution** | A,V | C | — | — | R | C | C | — | — | `CTRL-PRIV-CONSENT-001..005`; `crates/corelink-privacy-consent-ledger/` |
| 7 | **Consent revocation propagation SLA monitoring (≤ 5min)** | A | — | R | — | — | — | — | — | — | `CTRL-PRIV-CONSENT-002`; `EVT-024` |
| 8 | **`purpose_tag` enum addition (new processing purpose)** | A,V | C | — | — | R | C | C | I | — | `privacy_model.md §5.6.1`; `LGPD-FULL-AUDIT-2026-05-15.md §1.3` |
| 9 | **`legal_basis` change for existing `purpose_tag`** | A,V | C | — | — | R | C | C | I | I (post-decision; 5 BD) | `LGPD-FULL-AUDIT-2026-05-15.md §1.3` basis-fixed invariant |
| 10 | **`notice_version` major bump + force re-consent** | A | — | — | — | R | C | C | I | — | `legal/privacy-notice/v*.md` |
| 11 | **DPIA / RIPD authoring or supervision (Art. 38)** | A,R | C | — | — | C | C | C | — | I (on request) | `legal/dpia/template.md`; `LGPD-FULL-AUDIT-2026-05-15.md §1.11` |
| 12 | **Sub-processor onboarding (new vendor with PII exposure)** | A,V | C | C | C | R | C | C | I | — | `legal/sub-processors.md`; `VENDOR-RISK-METHODOLOGY.md`; `RB-VENDOR-RISK-QUARTERLY-REVIEW.md` |
| 13 | **Sub-processor risk-tier reclassification** | A | C | — | — | R | — | C | — | — | `VENDOR-RISK-REGISTER.md` |
| 14 | **Sub-processor DPA review / refresh** | A | C | — | — | — | R | C | — | — | `legal/dpa/`; `RB-DPA-CHANGE.md` |
| 15 | **Cross-border data-flow change (Art. 27 + Art. 33 §1º)** | A,V | C | C | — | R | C | C | I | I (on material change; 30 BD) | `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`; `crates/corelink-privacy-residency-enforcement/` |
| 16 | **Residency attestation refresh (90-day cycle)** | A,R | C | — | — | — | — | C | — | — | `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`; `LGPD-DPO-MONTHLY-CHECKLIST.md` item 22 |
| 17 | **Breach declaration (P-BREACH / SEV-1 with PII exposure)** | A,V | R | C | C | C | C | C | I | I (≤ 48h internal SLA) | `RB-BREACH-NOTIF.md`; `LGPD-FULL-AUDIT-2026-05-15.md §1.14`; `EVT-019` |
| 18 | **Breach notification to data subjects (Art. 48)** | A,R | C | — | C | — | C | C | I | — | `legal/breach-notification/templates/anpd-incident-form-v2024.md` |
| 19 | **ANPD inbound inquiry response** | A,R | C | — | — | — | C | C | I | — | `legal/breach-notification/anpd-contacts.md`; `RB-DPO-ESCALATION.md §5` |
| 20 | **Tenant DPA negotiation / amendment** | A | C | — | — | — | R | C | I | — | `legal/dpa/`; `RB-DPA-CHANGE.md` |
| 21 | **Privacy notice version drafting (legal text)** | A | C | — | — | — | R | C | — | — | `legal/privacy-notice/v*.{pt-BR,en-US,es-MX}.md` |
| 22 | **Annual LGPD/GDPR training program — content authoring** | A,R | C | — | — | — | C | C | — | — | `LGPD-DPO-MONTHLY-CHECKLIST.md` item 23 |
| 23 | **Annual LGPD/GDPR training program — delivery + tracking** | A | C | C | C | C | — | — | — | — | HR system of record (post-H-19 HRIS) |
| 24 | **Quarterly compliance briefing to Board** | A,R | C | C | C | — | — | C | I | — | `DPO-APPOINTMENT-2026-05-15.md §2`; `DPO-HANDOFF-PLAN.md §3` |
| 25 | **Monthly DPO checklist execution (25 items)** | A,R | C | C | C | — | — | — | — | — | `LGPD-DPO-MONTHLY-CHECKLIST.md` |
| 26 | **Audit-trail spot-check (sample 50 records / month)** | A,R | C | C | — | — | — | — | — | — | `LGPD-DPO-MONTHLY-CHECKLIST.md §C`; `EVT-047` |
| 27 | **Residual-risk register refresh (quarterly)** | A,R | C | — | — | — | — | C | I | — | `LGPD-FULL-AUDIT-2026-05-15.md §4` |
| 28 | **ROPA / Record of Processing refresh (quarterly)** | A,R | C | — | — | C | — | C | — | — | `LGPD-ROPA-2026-05-15.md`; `LGPD-FULL-AUDIT-2026-05-15.md §1.10` |
| 29 | **SOC 2 / ISO 27001 cross-framework privacy controls sign-off** | A | C | — | — | — | — | C | I | — | `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` |
| 30 | **DPO succession / permanent hire approval** | C | — | — | — | — | C | C | A,R | — | `DPO-HANDOFF-PLAN.md §5` |

---

## 2. Decision-rights matrix

This section enumerates **decisions that the DPO either makes alone, vetoes, or delegates**, with the rationale anchored in LGPD/GDPR.

### 2.1 DPO unilateral decisions (DPO-alone, no countersign required)

| Decision | LGPD/GDPR anchor | Notes |
|---|---|---|
| Accept or reject inbound DSR Access / Confirmation / Portability requests (non-destructive) | LGPD Art. 18 §I, II, V; GDPR Art. 15, 20 | DPO may delegate intake to Support Lead but retains accountability |
| Audit-trail spot-check sample selection | LGPD Art. 37 | DPO chooses sample size and tenants |
| Monthly checklist filing | LGPD Art. 41 §2º IV | DPO signs the monthly checklist file |
| Quarterly compliance briefing content | LGPD Art. 41 §2º I; GDPR Art. 39(1)(a) | DPO chooses agenda; Board hears |
| ANPD inbound inquiry response drafting | LGPD Art. 41 §2º II | DPO drafts, may submit without engineering pre-clearance |

### 2.2 DPO veto rights (block-only; cannot unilaterally enable)

| Veto subject | LGPD/GDPR anchor | Mechanism |
|---|---|---|
| New `purpose_tag` enum addition | LGPD Art. 6 §I (finalidade) + Art. 7 (legal-basis pairing) | DPO-veto recorded as `dpo_veto.v1` CloudEvent; PR-block in code review |
| `legal_basis` change for existing `purpose_tag` | LGPD Art. 7 + immutability invariant (`LGPD-FULL-AUDIT-2026-05-15.md §1.3`) | Same as above |
| New sub-processor with PII exposure | LGPD Art. 27 + Art. 39 (compartilhamento) | Veto blocks vendor onboarding ticket; documented in `VENDOR-RISK-REGISTER.md` |
| Cross-border data-flow change | LGPD Art. 33 §1º + Art. 27 | Veto blocks `corelink-privacy-residency-enforcement` config change |
| Consent ledger schema breaking change | LGPD Art. 8 + consent immutability | Veto blocks `crates/corelink-privacy-consent-ledger/` migration |
| Breach declaration scope reduction | LGPD Art. 48 + ANPD Resolução 15/2024 | DPO may upgrade severity unilaterally; downgrade requires Advisor + Legal countersign |
| DSR Erasure scope reduction or refusal | LGPD Art. 18 §IV + 30-day SLA | DPO may approve refusal only with documented Art. 18 §4º exception |
| ANPD-facing communication content | LGPD Art. 41 §2º II | No communication to ANPD may go out without DPO sign-off |

### 2.3 DPO-with-countersign decisions (requires DPO + Advisor or DPO + Legal)

| Decision | Countersign required from | Rationale |
|---|---|---|
| DPIA / RIPD final approval (HIGH_RISK WI) | Advisor + Legal | LGPD Art. 38 expects multi-perspective review |
| Privacy-adverse processing waiver (e.g., LIA balancing test passes on new legitimate-interest base) | Advisor + Legal | LGPD Art. 7 §IX (LIA) requires documented balancing |
| Tenant DPA bespoke amendment (deviation from canonical template) | Legal | Legal contractual review |
| Breach declaration downgrade | Advisor + Legal | Prevents under-disclosure |
| Permanent DPO candidate approval | Board (A) + DPO (C) + Advisor (C) | Successor approval is Board's accountability |

### 2.4 Decisions delegated away from DPO (with DPO as I)

| Decision | Delegated to | Rationale |
|---|---|---|
| Day-to-day customer support ticket triage | Support Lead | DPO consulted only on DSR-flagged or privacy-flagged tickets (`RB-DPO-ESCALATION.md §3.1`) |
| Operational incident SEV-2/3 declarations (no PII exposure) | SRE Lead | DPO informed via post-incident review |
| Security-only vulnerabilities (no PII path) | Security Lead | DPO informed via monthly briefing |
| Code authoring in `crates/corelink-privacy-*` | Engineering | DPO reviews PR but does not author (independence safeguard, `DPO-APPOINTMENT-2026-05-15.md §3`) |

---

## 3. Conflict-of-interest decision rules

Per **ANPD Resolução 18/2024 §4** and the interim accumulation of roles documented in `DPO-APPOINTMENT-2026-05-15.md §4`, the following decision rules apply during the interim window to prevent conflict-of-interest deficits:

| Situation | Rule | Effect |
|---|---|---|
| DPO is also the author of a `crates/corelink-privacy-*` PR | DPO MAY NOT self-approve their own privacy-related PR | PR-block in CI; requires Advisor countersign |
| DPO is also the SRE Lead authoring an observability change | DPO MAY NOT approve telemetry-purpose-tag additions in their own SRE work | Veto held by Advisor for this class of PRs during interim |
| DPO is also Support Lead handling a DSR | DPO MAY handle DSR Access / Portability / Confirmation but MUST NOT solo-approve DSR Erasure of more than 1 subject in a single batch | Bulk erasure (≥ 2 subjects) requires Advisor countersign |
| Breach involves a system owned by DPO-the-Security-Lead | Breach declaration drafted by SRE on-call (rotational); DPO may not draft own-system breach summary unsupervised | Advisor + Legal countersign on declaration |
| Same individual signs on attestations as DPO, Security Lead, Compliance Officer | Documented as residual risk in `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md §7` | Re-sign on permanent DPO appointment |

**Conflict-of-interest evidence:** every PR or decision that triggers a rule above must carry an `external-advisor-countersign: <advisor-name>` field in the PR description or decision log. CI validator (future, post-H-19) enforces presence; until then, monthly DPO checklist item 23 spot-checks compliance.

---

## 4. Escalation thresholds (link to RB-DPO-ESCALATION.md)

For each row in the RACI above where DPO is `A` or has `V`, the trigger conditions for escalation are enumerated in `specs/_runbooks/RB-DPO-ESCALATION.md`. Key thresholds (full list in runbook §3):

| Activity (this matrix row) | Escalation trigger | Runbook section |
|---|---|---|
| Row 17 (Breach declaration) | Any PII exposure suspected, OR > 100 subjects affected, OR ANPD-reportable per Resolução 15/2024 | `RB-DPO-ESCALATION.md §3.1` |
| Row 12 (Sub-processor onboarding) | New vendor with PII exposure, OR new region, OR cross-border flow change | `RB-DPO-ESCALATION.md §3.3` |
| Row 8/9 (Purpose / basis change) | Any new `purpose_tag` or `legal_basis` change | `RB-DPO-ESCALATION.md §3.4` |
| Row 11 (DPIA) | Any WI tagged HIGH_RISK in privacy delta | `RB-DPO-ESCALATION.md §3.5` |
| Row 15 (Cross-border flow) | Any change to residency-enforcement rules | `RB-DPO-ESCALATION.md §3.6` |
| Row 19 (ANPD inbound) | Any inbound communication from ANPD or DPA peer | `RB-DPO-ESCALATION.md §5` |

---

## 5. Cross-references

- **DPO appointment:** `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md`
- **DPO escalation runbook:** `specs/_runbooks/RB-DPO-ESCALATION.md`
- **DPO handoff plan:** `specs/_compliance/DPO-HANDOFF-PLAN.md`
- **DPO monthly checklist:** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- **LGPD full audit (Art. 41 row):** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md §1.12`
- **LGPD ROPA:** `specs/_compliance/LGPD-ROPA-2026-05-15.md`
- **Residency attestation:** `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`
- **Privacy model:** `specs/03_architecture/privacy_model.md`
- **Compliance matrix:** `specs/03_architecture/compliance_matrix.md`
- **Vendor risk register:** `specs/_compliance/VENDOR-RISK-REGISTER.md`
- **DSR pipeline:** `crates/corelink-dsr/`
- **Consent ledger:** `crates/corelink-privacy-consent-ledger/`
- **Erasure worker:** `crates/corelink-privacy-erasure-worker/`
- **Residency enforcement:** `crates/corelink-privacy-residency-enforcement/`
- **Breach notification:** `legal/breach-notification/`

---

## 6. Sign-off

| Role | Name | Signature | Date |
|---|---|---|---|
| Interim DPO | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| External Privacy Advisor (R5-8 countersign) | _pending advisor onboarding_ | `__________________` | _2026-__-__ |
| Compliance / Final approver | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |

---

**Fim de DPO-RESPONSIBILITIES-MATRIX.** Refresh quarterly with each LGPD-RESIDENCY-ATTESTATION refresh cycle, OR on any material change to roles or sub-processor landscape, OR on permanent DPO appointment (whichever earliest).
