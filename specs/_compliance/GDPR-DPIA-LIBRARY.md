---
id: "GDPR-DPIA-LIBRARY"
type: "compliance_dpia_index"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-22-FOLLOWUP-FULL-AUDIT"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "GDPR-FULL-AUDIT-2026-05-15"
  - "LGPD-FULL-AUDIT-2026-05-15"
  - "PRIVACY-MODEL"
tags: ["gdpr", "gdpr-art-35", "gdpr-art-36", "dpia", "ripd", "lighthouse", "high-risk-processing"]
---

# GDPR DPIA Library — Index of Data Protection Impact Assessments (2026-05-15)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:** index of Article 35 DPIAs for high-risk processing at CoreLink, with Lighthouse-customer onboarding trigger list, completed DPIA register, and per-tenant DPIA template pointer.
>
> **Framework version:** GDPR Art. 35 + Art. 36 (prior consultation); EDPB Guidelines WP248 rev.01 endorsed by EDPB; LGPD Art. 38 (RIPD — parallel obligation; same DPIA records satisfy both regimes).
>
> **Companion canonical docs:**
> - `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` §1.12 (Art. 35 article-level)
> - `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` §1.11 (LGPD RIPD parallel)
> - `legal/dpia/template.md` (canonical DPIA template — already exists; derived from ANPD 2024 RIPD guidance + EDPB WP248)
> - `legal/dpia/lighthouse-onboarding-template-2026-05-15.md` (per-EU-tenant DPIA template — this delivery)
> - `legal/dpia/` (per-flow / per-tenant DPIA records)
> - `specs/03_architecture/privacy_model.md` §4 (LINDDUN threat model) + §5.6 (purpose enum + LIA gates)

---

## 1. Why this index exists

GDPR Art. 35(1) triggers DPIA when processing is "**likely to result in a high risk to the rights and freedoms of natural persons**". EDPB Guidelines WP248 rev.01 (endorsed) lists **nine criteria**; if **two or more** apply, DPIA is mandatory. National SAs publish their own "likely high risk" lists under Art. 35(4) (CNIL France, BfDI Germany, AEPD Spain, Garante Italy, DPC Ireland).

CoreLink's DPIA practice is to:

1. Maintain a **trigger catalogue** that maps WI metadata + Lighthouse-customer characteristics to DPIA requirement.
2. Pre-stage DPIAs for **high-velocity flows** (BYOK key custody, audit chain retention, lighthouse onboarding) so customer onboarding is not gated on a 30-day DPIA scramble.
3. Index all DPIAs in this library for **discoverability + cross-reference** during audit (SOC 2, ISO 27001, LGPD ANPD inquiry, GDPR SA inquiry).

---

## 2. Trigger criteria (when DPIA is required)

### 2.1 Automatic triggers (WI metadata)

A WI in CoreLink's sprint cycle automatically triggers DPIA if its frontmatter declares ANY of the following:

| WI declaration | Why DPIA |
|---|---|
| `privacy_risk_tag: HIGH_RISK` | Self-declared trigger |
| New `purpose_tag` in `privacy_model.md §5.6.1` | Material change to processing |
| New sub-processor with cross-border data flow | Art. 30 + Art. 44 + Art. 35 nexus |
| Change in `legal_basis` for existing purpose | Re-DPIA (basis swap is material) |
| New ML model training on user-generated data | Art. 22 nexus; profiling concern |
| Sensitive-data exception invoked (Art. 9(2)) | High-risk by construction |
| BYOK key custody changes (new region, new vendor) | Custodial obligation change |
| Audit chain retention changes (e.g., from 7y to 10y) | Storage limitation re-assessment |

### 2.2 EDPB WP248 nine-criteria cross-check (≥2 = DPIA mandatory)

| WP248 criterion | CoreLink instance |
|---|---|
| 1. Evaluation or scoring | `analytics_personalized` purpose, opt-in only |
| 2. Automated-decision with legal effect | **NONE** — declared in `gdpr.mdx` §7 |
| 3. Systematic monitoring | Tenant-driven; HuGR is processor only |
| 4. Sensitive data / data of highly personal nature | **NONE** intentionally (Art. 9 anti-pattern); WebAuthn classification covered in LGPD-FULL-AUDIT §1.6 |
| 5. Data processed on large scale | EU subject count grows with tenants; per-tenant DPIA required when tenant > 10k EU subjects |
| 6. Matching / combining datasets | Cross-tenant benchmarks opt-in only |
| 7. Data concerning vulnerable subjects | **NONE** by design (B2B ≥ 18) |
| 8. Innovative use / technological solutions | BYOK key custody design = innovative (DPIA in §4.3) |
| 9. Transfer outside EU + Art. 49 derogation | Stripe + Clerk (covered in `GDPR-SCC-EXECUTION-2026-05-15.md` TIA register) |

### 2.3 Lighthouse customer trigger list

CoreLink's Lighthouse customer onboarding (R5-A5 / RB-LIGHTHOUSE-PHASE-MANAGEMENT.md) requires a **per-tenant DPIA** when ANY of:

| Trigger | Why |
|---|---|
| Tenant is EU-established with > 10k EU subjects in their planned uploads | WP248 criterion 5 (large scale) |
| Tenant operates in regulated vertical (healthcare / financial / public-sector / education with minors / telecom) | Vertical-specific high-risk presumption per national SA lists |
| Tenant intends to use `training_ml_models` purpose | WP248 criterion 8 (innovative) + Art. 22 proximity |
| Tenant intends BYOK for EU subject data | WP248 criterion 8 + custodial obligation change |
| Tenant intends to opt-in to `cross_tenant_benchmarks` | WP248 criterion 6 (matching / combining) |
| Tenant requests `analytics_personalized` for their internal use of CoreLink data | WP248 criterion 1 (evaluation) + criterion 6 |
| Tenant requests joint controllership (Art. 26) arrangement | Special control architecture |
| Tenant deploys CoreLink under their own DPO's oversight (DPO-to-DPO engagement) | Process complexity |
| Tenant is from a non-adequate country, controlling EU subject data | Art. 44 + Art. 49 derogation analysis |

**Default posture:** every Lighthouse EU customer triggers at least one of the above. The **per-tenant DPIA template** (`legal/dpia/lighthouse-onboarding-template-2026-05-15.md`) is the operational anchor.

---

## 3. DPIA process

### 3.1 Steps (canonical, per EDPB WP248)

1. **Identify the need** — WI metadata or Lighthouse intake form.
2. **Describe the processing** — flows + purposes + bases + retention + recipients (cross-reference RoPA).
3. **Consult stakeholders** — internal DPO + Security Lead + (where impactful) data subjects/representatives via tenant.
4. **Assess necessity and proportionality** — minimisation; alternatives considered.
5. **Identify and assess risks** — LINDDUN threat model + concrete harms (financial / discriminatory / reputational / psychological / freedom-impacting).
6. **Identify mitigations** — technical (encryption, pseudonymisation, fragmentation) + organisational (access control, training) + contractual (DPA clauses).
7. **Sign-off** — DPO + Security Lead + final approver.
8. **Re-assess** — minimum annual; sooner on material change.

### 3.2 Sign-off matrix per DPIA

| Role | Responsibility |
|---|---|
| DPO (interim Privacy Officer until formal appointment) | Methodology rigor; Art. 35(2) consultation; Art. 36(1) prior-consultation determination |
| Security Lead | Threat modelling; mitigation feasibility |
| Final approver (compliance) | Cross-framework alignment (SOC 2, ISO 27001, LGPD parallel) |
| Tenant DPO (per-tenant DPIA only) | Tenant-side risk inputs; tenant warrants legal basis |

### 3.3 Art. 36 prior consultation trigger

If a DPIA concludes the processing would result in **high residual risk** absent measures the controller can take, Art. 36(1) requires **prior consultation with the SA** before processing begins. Threshold:

- BYOK key custody for tenant with regulated-vertical EU subjects = LOW residual risk (envelope + customer-held key + audit + LIA + SCC where applicable) — no prior consultation.
- Cross-tenant ML model training without k-anon floor = HIGH residual risk — would require prior consultation; **CoreLink's design forbids this** (CTRL-PRIV-010 k≥50 floor; opt-in only).
- New observability vendor processing EU subject telemetry in a non-adequate country without SCC/DPF = HIGH residual risk — would require prior consultation; **disabled by default**.

**Posture:** by design, no production DPIA at CoreLink concludes HIGH residual risk; if a future DPIA does, Art. 36 prior consultation is triggered before deployment.

---

## 4. DPIA register (completed and pending)

### 4.1 Completed DPIAs

| # | DPIA | File | Date | Status | Re-assessment due |
|---|---|---|---|---|---|
| DPIA-001 | Security monitoring (legitimate interest LIA + balancing) | `legal/dpia/security_monitoring-2026-04-15.md` | 2026-04-15 | SIGNED | 2027-04-15 |
| DPIA-002 | Cross-region deduplication | `legal/dpia/cross_region_dedup-2026-04-22.md` | 2026-04-22 | CLOSED (decision: tenant-local default; opt-in only) | 2027-04-22 |
| DPIA-003 | BYOK key custody design | `legal/dpia/byok-key-custody-2026-05-15.md` | 2026-05-15 (this delivery) | DRAFT | 2027-05-15 |
| DPIA-004 | Audit chain retention design (7y Object Lock) | `legal/dpia/audit-chain-retention-2026-05-15.md` | 2026-05-15 (this delivery) | DRAFT | 2027-05-15 |
| DPIA-005 (template) | Lighthouse customer onboarding — per-tenant template | `legal/dpia/lighthouse-onboarding-template-2026-05-15.md` | 2026-05-15 (this delivery) | TEMPLATE | (each instance: 12 months) |

### 4.2 Pending DPIAs (gated)

| Trigger | DPIA target file | Owner | ETA | Gate |
|---|---|---|---|---|
| Lighthouse EU customer #1 onboarding | `legal/dpia/lighthouse-<tenant_id>-<date>.md` | DPO | At intake | Gating gate on EU enterprise onboarding |
| Sentry/PostHog/LogRocket activation for EU subjects (GAP-14 follow-up) | `legal/dpia/observability-vendor-<vendor>-<date>.md` | DPO | T+60 | Gating gate before vendor activation |
| Any new ML feature touching subject data | (auto-trigger via WI metadata) | DPO | Per WI | Gating gate before merge |

### 4.3 Pre-staged DPIA summaries

**DPIA-003 — BYOK key custody (summary):**

- **Risk:** EU tenant's envelope key URI sits in HuGR's account_management store; vendor (AWS/GCP/Azure/Vault) holds the key material. Risk: vendor compromise → key material exposure; HuGR compromise → URI exposure but not key material.
- **Necessity:** EU regulated tenants (financial, healthcare) require customer-held keys for separation-of-duties. BYOK is the canonical mitigation.
- **Mitigations:** (a) key material never traverses HuGR (CTRL-CRYPTO-002 envelope at access time only); (b) URI is a reference, not a secret; (c) audit on every key-use event; (d) dual-approval for key rotation; (e) BYOK ADR `specs/_adrs/ADR-S07-*-byok.md`; (f) HuGR has no encrypt-at-rest data ungated by customer-held envelope key.
- **Residual risk:** LOW. Customer retains operational control; HuGR's compromise does not yield plaintext.
- **Art. 36 prior consultation:** NOT triggered.

**DPIA-004 — Audit chain retention (summary):**

- **Risk:** 7-year Object Lock retention of audit events containing pseudonymized subject identifiers. Risk: re-identification of pseudonyms (HKDF-derived) over long retention windows; storage limitation tension (Art. 5(1)(e)).
- **Necessity:** SOC 2 + ISO 27001 + LGPD Art. 37 + national fiscal/accounting law (BR 5y, DE/FR ~10y depending on sector) require retention. Art. 5(1)(e) recognises retention "where necessary for compliance with a legal obligation" (Art. 6(1)(c) base).
- **Mitigations:** (a) **pseudonymisation on erasure** (HKDF info `corelink/v1/audit-pseudonym`; one-way; salt rotation per `crates/corelink-privacy-pseudonymize/`); (b) audit minimization (CTRL-PRIV-014; only canonical event fields, no payload PII); (c) Object Lock in governance mode (immutability + DPO break-glass for explicit legal-hold release); (d) 7y retention ceiling (not 10y+).
- **Residual risk:** LOW. Pseudonymization on erasure + minimization + immutability + ceiling balance retention obligation vs storage limitation.
- **Art. 36 prior consultation:** NOT triggered.

**DPIA-005 (template) — Lighthouse customer onboarding:**

- Template fields: tenant identity; EU subjects population estimate; intended purposes; intended legal bases; cross-border flows; intended sub-processors; intended retention; intended sensitive-data handling (anti-pattern affirmation); intended ML use; intended BYOK; intended joint controllership; per-trigger risk assessment from §2.3 above; mitigations per trigger; DPO sign-off; tenant DPO countersign.

---

## 5. Per-DPIA evidence retention

| Evidence | Location | Retention |
|---|---|---|
| Signed DPIA document | `legal/dpia/*.md` | Active + 7y |
| LIA balancing record (when DPIA references legitimate interest) | `legal/lia/*.md` | Active + 7y |
| Risk assessment worksheets + LINDDUN matrices | DPIA appendix | Active + 7y |
| Tenant DPO countersign (per-tenant DPIA only) | DPIA appendix | Active + 5y post-tenant-termination |
| Re-assessment history | DPIA file `## History` section | Forever |
| Art. 36 prior consultation correspondence (if triggered) | `legal/sa-consultations/` | Forever |

---

## 6. Library health metrics (tracked in DPO monthly checklist)

| Metric | Target | Source |
|---|---|---|
| DPIA backlog | 0 overdue | This library §4 |
| Re-assessment compliance rate | ≥ 95% on schedule | DPO checklist item 4 |
| Per-Lighthouse-tenant DPIA on-time rate | 100% (gating gate) | RB-LIGHTHOUSE-PHASE-MANAGEMENT |
| WP248 criteria audit (sanity check) | Quarterly | DPO checklist item 16 (cross-ref) |

---

## 7. Cross-references

- **GDPR full audit (Art. 35 detail):** `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` §1.12
- **LGPD full audit (Art. 38 RIPD parallel):** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` §1.11
- **SCC execution (Art. 44–50 transfer DPIA nexus):** `specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md`
- **RoPA (Art. 30):** `specs/_compliance/LGPD-ROPA-2026-05-15.md`
- **Privacy model (LINDDUN threat model):** `specs/03_architecture/privacy_model.md` §4
- **Compliance matrix (DPIA cross-framework):** `specs/03_architecture/compliance_matrix.md` §5
- **DPO monthly checklist (re-assessment cadence):** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- **Lighthouse phase management runbook:** `specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md`
- **DPIA records folder:** `legal/dpia/`
- **LIA records folder:** `legal/lia/`
- **DSR runbook (where DPIA + DSR intersect for §V portability across DPIA-controlled flows):** `specs/_runbooks/RB-DSR-GDPR.md`

---

**Fim de GDPR-DPIA-LIBRARY.** Living index — every new DPIA registered here; re-assessment triggers tracked in DPO monthly checklist.
