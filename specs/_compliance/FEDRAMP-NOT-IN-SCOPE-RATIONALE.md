---
id: "FEDRAMP-NOT-IN-SCOPE-RATIONALE"
type: "compliance_rationale"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R-PREP-FEDRAMP-INFO"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "FEDRAMP-MODERATE-CROSSWALK-2026-05-15"
  - "COMPLIANCE-MATRIX"
tags:
  - "fedramp"
  - "not-in-scope"
  - "rationale"
  - "go-no-go"
  - "informational"
---

# FedRAMP — Not-in-Scope Rationale (CoreLink)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** record the
> reasoning behind the explicit decision **not to pursue FedRAMP
> authorization** as part of the GA program, the conditions under which that
> decision would be re-opened, and the operational fallback in the
> interim.

## 1. Decision

**CoreLink does not pursue FedRAMP Rev 5 Moderate (or Low, or High) authorization in the GA-to-Type-II window (2026-Q2 through 2027-Q4).** No 3PAO is engaged; no SSP authored in the FedRAMP template; no JAB queue submission; no Agency ATO sponsorship sought.

Decision owner: Gustavo Schneiter (Owner / final approver).
Decision date: **2026-05-15**.
Documented in: this file + `specs/_compliance/FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md §1 Header` + `specs/03_architecture/compliance_matrix.md §1` (existing row already says "Sob demanda; 36+ meses se demanda").
Customer-facing posture: `apps/docs/docs/trust/fedramp-info.mdx`.

---

## 2. Why not (rationale)

### 2.1 Target market is commercial, not federal

CoreLink's go-to-market is the commercial SaaS engineering market — build-cache and content-addressable storage for software engineering teams. The ICP today is:

- **Tier 1:** mid-market SaaS engineering organizations (50–500 engineers; CI/CD spend bottleneck).
- **Tier 2:** enterprise platform engineering teams in regulated industries (FinServ, healthcare, large-tech) — these prospects may ask about FedRAMP in a procurement form **as a checkbox**, but they are not buying CoreLink to deploy inside a federal boundary.
- **Tier 3 (informational, not GTM):** US federal civilian agencies, DoD components, federal system integrators. **Not targeted.**

No revenue pipeline today routes through a federal contracting vehicle (GSA Schedule, SEWP, ITES-SW2, etc.). No active conversations with a federal sponsor agency. No GovCloud-isolated deployment requested by any current prospect.

> **Source:** quarterly pipeline review (internal) — no federal-target opportunities in the next-4-quarter rolling pipeline as of 2026-05-15.

### 2.2 Cost is incompatible with stage / runway

The all-in first-year FedRAMP Moderate authorization cost — per FedRAMP PMO public marketplace data + Coalfire 2024 cost analysis + industry blog reports — falls in a **$500k–$900k range** for a first-time SaaS vendor at our scale. Components:

| Cost line | Range | Notes |
|---|---|---|
| 3PAO engagement (Stage 1 + Stage 2) | $200-350k | Schellman, A-LIGN, Coalfire, KPMG |
| FedRAMP SSP authoring (GSA template, ~400 pages) | $80-150k | Contracted FedRAMP-specialist author; ~12 person-months |
| FIPS 140-3 validated cryptographic modules (uplift) | $30-60k | Azure HSM Premium uplift + HashiCorp Vault Enterprise FIPS license + GCP CloudHSM provisioning |
| US-citizen background investigation for privileged staff | $5-15k per employee + sponsor agency arrangement | OPM-style; minimum Tier 2 (formerly NACLC) Moderate; takes 6–12 months wall-clock |
| Continuous Monitoring (ConMon) ongoing | $50-100k / year | Monthly POA&M to FedRAMP Marketplace; monthly authenticated scans; Significant Change Request workflow |
| Agency ATO sponsorship costs OR JAB P-ATO path | $0 (in-kind) OR $200-400k (JAB) | Agency path requires identified sponsor agency willing to issue ATO |
| **Total first-year** | **~$500-900k** | Above the existing SOC 2 + ISO 27001 spend (~$95-175k combined per `ISO27001-ROADMAP.md` + `SOC2-ROADMAP.md`). |

CoreLink is pre-revenue / early-revenue at GA. Spending **$500-900k on a certification with zero federal pipeline** is not defensible against the alternative use of those funds (additional engineering hires, security tooling, commercial enterprise tenancy onboarding). The unit economics inversion is severe: at typical enterprise SaaS ACV ($30-100k per tenant), FedRAMP cost equates to **5-30 lost commercial tenants of runway** — without a corresponding federal pipeline.

### 2.3 Timeline is incompatible with GA window

The wall-clock to a FedRAMP ATO from a cold start — per FedRAMP PMO 2024 cycle-time report — averages:

| Phase | Duration |
|---|---|
| Readiness Assessment + 3PAO selection | 2-3 months |
| SSP authoring (~400 pages, GSA template) | 6-9 months (parallelizable with readiness) |
| Stage 1 (documentation review) + remediation | 3-4 months |
| Stage 2 (control assessment) + remediation | 4-6 months |
| Authorization decision (Agency ATO or JAB P-ATO queue) | 3-12 months (JAB queue depth at 2024 = ~18 months) |
| **Total (Agency ATO path)** | **12-18 months optimistic** |
| **Total (JAB P-ATO path)** | **24-36 months realistic** |

Our GA window is **Q4-2026 / Q1-2027** (SOC 2 Type I + ISO 27001 Stage 2). Adding a 12-18-month FedRAMP track on top of the existing SOC 2 + ISO 27001 + LGPD + GDPR + PCI DSS SAQ-A multi-framework portfolio would either (a) defer GA into 2028, or (b) require a parallel staffing of compliance specialists we don't have today.

### 2.4 Substance is already 85% covered (so the cost-benefit collapses)

Per `FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md §3.2`, the Moderate baseline substance is **~85% covered today** through SOC 2 + ISO 27001 + LGPD + GDPR overlap. The marginal value of formal FedRAMP authorization for a commercial-target SaaS is the certificate itself (a procurement gate for federal customers), not the underlying controls — which we already have.

For prospects in regulated commercial industries who reference FedRAMP **as a quality bar** (not as an actual procurement requirement), the crosswalk answers the question. For prospects who **actually procure** through a FedRAMP-required vehicle, sponsorship discussion is the right path (see §4 trigger conditions below).

### 2.5 Alternatives that satisfy "FedRAMP-aware" without certification

For prospects who require demonstrable alignment with the Moderate baseline without an authorization:

1. **Crosswalk document delivery** — `FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md` delivered under NDA via `trust@corelink.dev`.
2. **SOC 2 Type I report** (target Q1-2027) — covers AC / AU / CA / CM / IR / PE / RA / SA / SC / SI / SR families substantively.
3. **ISO 27001:2022 certificate** (target Q1-2027) — covers Annex A overlap at 98.9% in-scope.
4. **CSA STAR Level 1 self-assessment** (CAIQ submission) — completable on demand; ~2 person-weeks. *(Not committed; offered on request.)*
5. **Pentest report** (Schellman / Bishop Fox annual external pentest per `SOW-S20-EXTERNAL-PENTEST.md`).
6. **Sub-processor inheritance** — Cloudflare (FedRAMP Moderate-equivalent + StateRAMP), AWS GovCloud-adjacent, GCP Assured Workloads, Azure Gov-adjacent SOC 2 reports inherited per `legal/sub-processors.md`.

This stack answers "are you FedRAMP-aligned?" for ~95% of commercial-regulated prospects without the certification cost or timeline.

---

## 3. Operational fallback (what we do instead in 2026-2027)

| Need / Question | Operational answer |
|---|---|
| "Do you have a FedRAMP authorization?" | "No. Our SOC 2 + ISO 27001 covers ~85% of FedRAMP Moderate substance. Here is the crosswalk." |
| "Can you deliver a FedRAMP crosswalk?" | Yes — `FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md` under NDA via `trust@corelink.dev` within 1 business day. |
| "Can you commit to FedRAMP by date X?" | Only with sponsorship. See §4 trigger conditions. |
| "Are you on the FedRAMP Marketplace?" | No. We do not appear on `marketplace.fedramp.gov`. |
| "Do you support GovCloud / IL4 / IL5?" | No. We deploy on Cloudflare commercial + AWS / GCP / Azure commercial regions only. |
| "Do you use FIPS 140-3 validated modules?" | Partial — AWS KMS FIPS endpoint is FIPS 140-3 L3 attested (per `BYOK-FIPS-ATTESTATION-MATRIX.md`); GCP CloudHSM + Azure Key Vault HSM + HashiCorp Vault Enterprise FIPS paths are mapped; RFI in flight per SOC 2 GAP-02. |
| "Are your engineers US-citizens with background investigations?" | No. We do not implement OPM-style personnel screening. We implement SOC 2 / ISO 27001 personnel-security controls (Owner attestation + advisor pool CV review + NDAs). |

---

## 4. Trigger conditions that would re-open the FedRAMP decision

The decision is **re-evaluated** if any one of the following conditions becomes true. Re-evaluation does not auto-trigger the pursuit; it triggers a formal go / no-go decision review with documented outcome.

### 4.1 Hard triggers (any one → mandatory re-evaluation)

1. **Named US federal customer with sponsorship.** A US federal civilian agency, DoD component, or federal system integrator commits in writing (LOI or signed sponsorship letter) to sponsor a FedRAMP authorization for CoreLink with one or more of the following economic terms:
   - Sponsoring agency willing to issue Agency ATO upon successful 3PAO assessment (no JAB queue);
   - Contract value ≥ $1.5M annual recurring revenue or ≥ $5M total contract value;
   - Sponsor pays or co-funds 3PAO + SSP authoring + first-year ConMon costs (≥ $400k contribution);
   - Sponsor agrees to a 12-18-month authorization timeline (not "by next quarter").
2. **Aggregate federal pipeline ≥ $5M ARR.** Three or more active federal-target opportunities representing ≥ $5M aggregate ARR enter the pipeline within a 12-month rolling window, each conditioned explicitly on FedRAMP authorization. This converts FedRAMP from a "compliance theatre" cost into a revenue-unlock investment.
3. **Industry partner co-investment.** A strategic partner (e.g., FedRAMP-authorized PaaS / SaaS that wants to embed CoreLink as a build-cache layer) commits ≥ $500k toward 3PAO + SSP authoring as part of a partnership / OEM agreement.

### 4.2 Soft triggers (any one → revisit at next quarterly compliance review)

1. **A commercial enterprise prospect at ≥ $500k ACV** explicitly references FedRAMP as a procurement-blocking requirement (not a checkbox). Soft trigger reviews the crosswalk strength, not the cert path.
2. **Cloudflare Workers obtains FedRAMP Moderate authorization** (currently FedRAMP authorization is held for Cloudflare's Magic Network Services and some Zero Trust products; Workers/R2/D1/DO are not on the Marketplace as of 2026-05-15). If our primary sub-processor obtains Workers-tier FedRAMP authorization, our inheritance posture changes materially.
3. **FedRAMP Rev 6 baseline release** introduces materially different controls (e.g., dropping the FIPS 140-3 module requirement or relaxing US-citizen personnel screening for cloud-native vendors). Soft re-evaluate against new baseline.
4. **An adjacent certification (DoD CMMC, IRS Pub 1075, CJIS) is sought** — if business need drives one of these, FedRAMP becomes incrementally cheaper (substantial overlap).

### 4.3 Review cadence

- Quarterly compliance review (per `RB-COMPLIANCE-WEEKLY-REVIEW.md` rollup → quarterly digest) explicitly checks the trigger list.
- Annual refresh of `FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md` re-prices the §4 cost estimates against current market data and checks for FedRAMP PMO policy changes (Rev 5 amendments, OMB M-24-XX memos, etc.).

---

## 5. Open questions / known unknowns

| ID | Question | Owner | Resolution path |
|---|---|---|---|
| Q1 | Does StateRAMP (state government equivalent) make sense as a lower-cost intermediate step? | Compliance Officer | Re-evaluate at first state-government opportunity. StateRAMP cost is ~30-50% of FedRAMP; could be a credibility bridge. |
| Q2 | Should we pursue CSA STAR Level 1 (CAIQ self-assessment) as a "FedRAMP-aware" signal? | Compliance Officer | Decision pending after SOC 2 Type I report issuance Q1-2027. Low cost (~$5-10k); high signal-to-noise ratio. |
| Q3 | What is the actual federal demand for build-cache / CAS infrastructure? | GTM (Owner) | Track inbound from `.gov` / `.mil` email domains on the contact form; no formal market research planned. |
| Q4 | Could a FedRAMP Tailored / FedRAMP Low path work? | Compliance Officer | Both are no longer accepted for new authorizations (FedRAMP Tailored sunset 2023; FedRAMP Low has near-zero adoption). Not viable. |

---

## 6. Decision record

This document constitutes the **decision record** for "FedRAMP not pursued in the 2026-2027 GA window."

| Field | Value |
|---|---|
| Decision | FedRAMP not pursued; informational crosswalk + customer-facing trust page maintained. |
| Date | 2026-05-15 |
| Decided by | Gustavo Schneiter (Owner) |
| Rationale | §2 above — commercial-market focus, $500-900k cost incompatible with stage, 12-18-month timeline incompatible with GA, substance already 85% covered via SOC 2 + ISO 27001. |
| Re-evaluation triggers | §4 above. |
| Customer-facing posture | `apps/docs/docs/trust/fedramp-info.mdx` — "we do not pursue FedRAMP today; contact `enterprise@corelink.dev` for sponsorship discussion". |
| Affected docs | `compliance_matrix.md §1` (already aligned: "Sob demanda; 36+ meses se demanda"); `ROADMAP-TO-GA.md` (this document linked as informational — no roadmap row added). |

---

## 7. Cross-references

- **FedRAMP Moderate crosswalk (companion):** `specs/_compliance/FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md`
- **Customer-facing one-pager:** `apps/docs/docs/trust/fedramp-info.mdx`
- **Trust center index:** `apps/docs/docs/trust/index.mdx`
- **SOC 2 evidence rollup:** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **ISO 27001:2022 Annex A crosswalk:** `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`
- **Compliance matrix (canonical Level 3):** `specs/03_architecture/compliance_matrix.md` §1
- **BYOK FIPS attestation matrix (FIPS 140-3 thread):** `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`
- **FedRAMP PMO (authoritative source):** <https://www.fedramp.gov/>
- **FedRAMP Marketplace:** <https://marketplace.fedramp.gov/>
- **NIST SP 800-53 Rev 5:** <https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final>

---

## 8. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 R-prep FedRAMP-info agent) | Initial not-in-scope rationale: decision record + market / cost / timeline rationale + operational fallback + hard and soft trigger conditions for re-evaluation + quarterly review cadence + 4 open-question entries. Companion to FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md + fedramp-info.mdx customer page. |

---

**Fim FEDRAMP-NOT-IN-SCOPE-RATIONALE.**
