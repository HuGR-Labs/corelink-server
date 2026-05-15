---
id: "ISO27001-ROADMAP-2026-05-15"
type: "compliance_roadmap"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R5-3-ISO27001-CROSSWALK"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "ISO27001-CROSSWALK-2026-05-15"
  - "ISO27001-GAP-ANALYSIS-2026-05-15"
  - "SOC2-ROADMAP-TYPE1-2026-05-14"
tags:
  - "iso27001"
  - "iso27001-2022"
  - "roadmap"
  - "certification"
  - "stage-1"
  - "stage-2"
  - "r-prep"
---

# ISO/IEC 27001:2022 — Phased Roadmap to Q1-2027 Certification

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** sequence the deliverables required to take CoreLink from its current **98.9% in-scope Annex A coverage** (per `ISO27001-CROSSWALK-2026-05-15.md`) to a signed ISO 27001:2022 certificate by **Q1-2027**. The roadmap layers on top of the SOC 2 Type I program (`SOC2-ROADMAP.md`); the two share Drata evidence collection, audit-firm engagement, and management-review cadence.
>
> **Anchor dates:** SOC 2 Type I fieldwork **Q4-2026**, Type I report **early Q1-2027** → ISO 27001 Stage 1 audit can stack on the Type I fieldwork visit; Stage 2 audit follows in **Q1-2027** once corrective actions from Stage 1 close.

## 1. Certification overview

| Milestone | Window | Output | Audit firm action |
|---|---|---|---|
| **M0 — ISMS scope statement** | T+0 (now) .. T+1m | scope doc + Statement of Applicability v1.0 (93-row format) | — |
| **M1 — Mandatory clauses 4–10 documentation** | T+1m .. T+3m | clauses 4 (context) / 5 (leadership) / 6 (planning) / 7 (support) / 8 (operation) / 9 (performance eval) / 10 (improvement) documented | — |
| **M2 — Internal audit (first cycle)** | T+3m .. T+4m | internal audit report; corrective actions logged | — |
| **M3 — Management review (first cycle)** | T+4m | management review minutes + actions | — |
| **M4 — Stage 1 audit (documentation review)** | T+5m..T+6m **(Q4-2026)** | Stage 1 report; readiness for Stage 2 confirmed | Schellman / A-LIGN |
| **M5 — Corrective actions from Stage 1** | T+6m..T+7m | corrective action register closed | — |
| **M6 — Stage 2 audit (operational effectiveness)** | T+7m..T+8m **(Q1-2027)** | Stage 2 report; certificate decision | Schellman / A-LIGN |
| **M7 — Certificate issuance** | T+8m..T+9m **(Q1-2027)** | ISO 27001:2022 certificate (3-year cycle with annual surveillance audits) | Accredited body |

## 2. Why Q1-2027 is realistic

Three structural enablers:

1. **98.9% in-scope coverage today** (89/90 Annex A controls Implemented or Partial; 1 Gap that closes in T+1m). The control substance is in place; the gap is documentation/formalization.
2. **Shared evidence collection with SOC 2.** Drata's 6 EvidenceStream variants already feed 91% of the SOC 2 TSC criteria that crosswalk to ISO Annex A; the same auto-collected evidence (audit logs, access reviews, credential management, change management, incident response, vulnerability management) serves ISO Stage 2 fieldwork.
3. **Audit firm continuity.** Schellman (SOC 2 incumbent, ANAB-accredited for ISO 27001) can run Stage 1 + Stage 2 on a stacked engagement, reducing onboarding overhead and total cost.

The only **incremental ISO-unique work** is the 7 gaps in `ISO27001-GAP-ANALYSIS.md` (~6.25 person-weeks total).

## 3. M0 — ISMS scope statement (T+0 .. T+1m)

### Deliverables

- `specs/_compliance/ISO27001-ISMS-SCOPE.md` — formal ISMS scope statement:
  - **In-scope:** CoreLink production surface (Cloudflare Workers + Pages + R2 + D1 + DO + KV + Clerk + Stripe); BYOK via AWS KMS / GCP Cloud KMS / Azure Key Vault / HashiCorp Vault; sub-processors per `legal/sub-processors.md`.
  - **Out-of-scope:** corporate IT / SaaS HR tools / personal devices / non-production environments (justified per ISO 27001 §4.3).
  - **Interested parties** (§4.2): customers, sub-processors, regulators (ANPD, EU DPAs, FTC), auditors, advisors.
  - **Internal/external issues** (§4.1): including the **2024 amendment climate change clause** — CoreLink runs on multi-region cloud infra with carbon disclosures inherited from Cloudflare / AWS / GCP / Azure ESG reports.
- **SoA refresh:** `specs/_audits/iso27001-soa.csv` from 46-row baseline to **93-row 2022 format** (one row per Annex A control × applicability × justification × implementation reference × evidence pointer × last review date).

### Owner / effort / dependencies

- **Owner:** Compliance Officer + Architect.
- **Effort:** S (≤ 1 person-week).
- **Dependencies:** none (uses existing scope from SOC 2 rollup).

### Exit criteria

- ISMS scope statement signed by Owner.
- SoA v1.0 (93 rows) committed.
- GAP-25 (SOC 2 register row "ISO/IEC 27001:2022 Annex A SoA refresh") flips to **DONE**.

## 4. M1 — Mandatory clauses 4–10 (T+1m .. T+3m)

ISO 27001:2022 §4–§10 are the "ISMS clauses" — process documentation, not Annex A controls. Each clause is a short doc.

| Clause | Deliverable | Source / overlap | Effort |
|---|---|---|---|
| §4 Context of the organization | `ISO27001-CLAUSE-04-CONTEXT.md` — internal/external issues; interested parties; scope; ISMS itself | M0 deliverable | XS |
| §5 Leadership | `ISO27001-CLAUSE-05-LEADERSHIP.md` — policy + roles + commitment | reuses `specs/_governance/` 13-role sign-off + Owner attestation | XS |
| §6 Planning | `ISO27001-CLAUSE-06-PLANNING.md` — risk assessment + risk treatment + objectives | reuses `specs/03_architecture/failure_modes.md` + STRIDE/LINDDUN matrix + SoA | S |
| §7 Support | `ISO27001-CLAUSE-07-SUPPORT.md` — resources + competence + awareness + communication + documented information | reuses `templates/` skill matrix + AUP (GAP-ISO-02) + communication via DPA | S |
| §8 Operation | `ISO27001-CLAUSE-08-OPERATION.md` — operational planning + risk assessment results | reuses sprint contracts + ADR + preflight reviews + Annex A SoA | S |
| §9 Performance evaluation | `ISO27001-CLAUSE-09-PERFORMANCE.md` — monitoring + internal audit + management review | reuses Drata continuous + DASH-COMPLIANCE-S20 + adversarial summaries | S |
| §10 Improvement | `ISO27001-CLAUSE-10-IMPROVEMENT.md` — non-conformity + corrective action + continual improvement | reuses GAP register + post-mortem template | XS |

### Owner / effort / dependencies

- **Owner:** Compliance Officer (lead) + Owner (clause 5 leadership sign-off).
- **Effort:** **M total** (~2 person-weeks for all 7 clauses; most content already exists, this is consolidation).
- **Dependencies:** M0 complete; ISO-unique gaps GAP-ISO-01 (asset register) + GAP-ISO-02 (AUP) issued in parallel.

### Exit criteria

- All 7 clause docs committed.
- ISO-unique gaps GAP-ISO-01..GAP-ISO-06 closed (GAP-ISO-07 MDM-lite documented).
- Drata ISO 27001 framework toggle enabled in dashboard; sustained ≥ 95% green for 8 weeks.

## 5. M2 — Internal audit first cycle (T+3m .. T+4m)

### Activities

1. **Auditor selection** — internal audit can be done by Compliance Officer **if** they were not involved in implementing the ISMS controls. For CoreLink, since Compliance Officer = Gustavo (solo founder + implementer), retain a **third-party internal auditor** (e.g., a freelance ISO 27001 Lead Auditor for 5 days). Cost: $5-10k.
2. **Audit plan** — 5-day engagement covering Annex A:2022 sample testing + clauses 4–10 review + SoA walkthrough.
3. **Sample testing** — auditor pulls samples per agreed population: 25 access provisioning events, 25 change events, 5 incidents (or simulated drills if no real incidents), 19 vendor agreements.
4. **Findings** — auditor issues internal audit report with non-conformities (major / minor / opportunity-for-improvement).
5. **Corrective actions** — Compliance Officer assigns owners + due dates for each finding; tracks in GAP register.

### Owner / effort / dependencies

- **Owner:** Compliance Officer + 3rd-party internal auditor.
- **Effort:** ~40h Compliance + 5d 3rd-party auditor.
- **Dependencies:** M1 complete; SoA + clauses 4–10 ready for review.

### Exit criteria

- Internal audit report delivered.
- All major non-conformities have corrective action plans with ETAs.
- Drata ISO 27001 dashboard reflects post-audit posture.

## 6. M3 — Management review first cycle (T+4m)

### Activities

1. **Owner-led management review meeting** (4h block) covering ISO 27001:2022 §9.3 PoF:
   - Status of actions from previous management reviews (N/A — first cycle).
   - Changes in external/internal issues relevant to the ISMS.
   - Feedback on information security performance (Drata dashboard + internal audit findings + incident summary).
   - Feedback from interested parties.
   - Results of risk assessment + risk treatment.
   - Opportunities for continual improvement.
2. **Minutes + decisions** — committed em `specs/_compliance/ISO27001-MGMT-REVIEW-2026-XX.md`.
3. **Actions assigned** — added to GAP register with ETAs pre-Stage-1.

### Owner / effort / dependencies

- **Owner:** Owner + Compliance Officer.
- **Effort:** S (≤ 1 person-week including prep + meeting + minutes).
- **Dependencies:** M2 internal audit report.

### Exit criteria

- Management review minutes signed by Owner.
- All actions tracked in GAP register.

## 7. M4 — Stage 1 audit (T+5m..T+6m / Q4-2026)

### Activities

1. **Audit firm engagement** — stacked with Schellman SOC 2 Type I fieldwork (Q4-2026); separate Stage 1 SoW + engagement letter.
2. **Stage 1 scope** — documentation review:
   - ISMS scope statement.
   - SoA + 93-row Annex A applicability.
   - Clauses 4–10 docs.
   - Internal audit report + management review minutes.
   - Risk assessment + treatment plan.
   - Policies (AUP, clear-desk, offboarding, etc.).
3. **Output** — Stage 1 report identifying readiness for Stage 2; any non-conformities flagged with corrective action window (typically 30-60 days).

### Owner / effort / dependencies

- **Owner:** Compliance Officer + Owner (engagement letter sign-off).
- **Effort:** ~80h Compliance + auditor PBC support.
- **Dependencies:** M0–M3 complete.

### Budget

- Stage 1 audit fee: **$15-25k** stacked with SOC 2 Type I (incremental).
- Already covered by SOC 2 Type I budget envelope ($40-85k total per `SOC2-ROADMAP.md §M0-M1`).

### Exit criteria

- Stage 1 report received.
- Readiness for Stage 2 confirmed by auditor.

## 8. M5 — Corrective actions (T+6m..T+7m)

### Activities

- Close all major + minor non-conformities from Stage 1.
- Document each correction with evidence link.
- Re-test impacted controls via Drata continuous monitoring.

### Owner / effort / dependencies

- **Owner:** Compliance Officer + control owners.
- **Effort:** XS-M depending on Stage 1 findings (typical: 1-3 person-weeks for first cert cycle).

### Exit criteria

- All Stage 1 non-conformities closed.
- Audit firm Stage 2 plan agreed.

## 9. M6 — Stage 2 audit (T+7m..T+8m / Q1-2027)

### Activities

1. **Stage 2 scope** — operational effectiveness testing:
   - Sample testing across Annex A:2022 controls.
   - Walkthrough interviews with control owners.
   - Evidence-of-operation review (Drata 6 EvidenceStream variants).
   - Incident response evidence (IR tabletop transcripts; real-incident handling if applicable).
   - DR/BCP drill evidence (DR-15 + DR-16 first cycles complete).
2. **Output** — Stage 2 report + certification decision recommendation.

### Owner / effort / dependencies

- **Owner:** Compliance Officer + control owners.
- **Effort:** ~120h Compliance + control-owner interview time.
- **Dependencies:** M5 complete; SOC 2 Type I report published (operating-effectiveness evidence overlap).

### Budget

- Stage 2 audit fee: **$25-40k** (stand-alone; not stacked with SOC 2 Type II which targets later in 2027).
- Total ISO 27001 first-year audit cost: **$40-65k** (Stage 1 + Stage 2 combined).

### Exit criteria

- Stage 2 report delivered with **certification decision: GRANT**.
- Any conditional findings have agreed corrective action timelines.

## 10. M7 — Certificate issuance (T+8m..T+9m / Q1-2027)

### Activities

1. **Accredited certification body** (Schellman ANAB-accredited entity) issues the **ISO/IEC 27001:2022 certificate**.
2. **Certificate posted** on `apps/docs/docs/trust/iso27001.mdx` + Drata Trust Center.
3. **Surveillance audit calendar** set:
   - Year 1 surveillance: Q1-2028.
   - Year 2 surveillance: Q1-2029.
   - Year 3 recertification: Q1-2030 (full re-audit; 3-year cycle).

### Owner / effort / dependencies

- **Owner:** Owner (certificate ownership) + Compliance Officer (ongoing maintenance).
- **Effort:** XS (administrative).

### Exit criteria

- Certificate issued + posted publicly.
- Surveillance audit calendar in Drata.
- Customer-facing trust page updated.

## 11. Risk register (roadmap-specific)

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R-1 | Stage 1 audit slips Q4-2026 → Q1-2027 | medium | medium | book Stage 1 slot with Schellman at SOC 2 Type I engagement letter signature (R5-1) |
| R-2 | Internal auditor unavailable for M2 | low | medium | 3-party freelance ISO 27001 LA market is deep; engage by T+2m |
| R-3 | ISO-unique gap closure slips past M1 | low | low | gaps are minor + small effort; compensating controls documented |
| R-4 | Climate clause (2024 amendment) interpretation drift | low | low | follow ISO/IEC technical committee guidance updates; cloud-inheritance posture is robust |
| R-5 | Budget overrun > $65k total cert cost | medium | low | competitive bid Schellman vs A-LIGN; Prescient as fallback |
| R-6 | Customer demands cert pre-Q1-2027 | medium | high | preliminary crosswalk (this doc) + 98.9% coverage narrative satisfies most procurement teams until cert issuance |

## 12. Budget envelope

| Item | Cost | Notes |
|---|---|---|
| Drata ISO 27001 framework toggle | included | already in $10-15k/yr SOC 2 subscription |
| Third-party internal auditor (M2) | $5-10k | 5-day engagement |
| Stage 1 audit (M4) | $15-25k | stacked with SOC 2 Type I |
| Stage 2 audit (M6) | $25-40k | stand-alone |
| Annual surveillance (years 1+2) | $10-15k/yr | from 2028 onwards |
| Recertification (year 3) | $30-50k | 2030 |
| Internal engineering time (M0–M6) | ~200h | mostly Compliance Officer |
| **Total first-year (cert acquisition):** | **$55-90k** | aligned with SOC 2 Type I total ($55k median) |

## 13. Cumulative dependencies

- **SOC 2 Type I program** (`SOC2-ROADMAP.md`) — must be in execution; Type I fieldwork Q4-2026 stacks with ISO Stage 1.
- **Drata subscription** ($10-15k/yr) — committed; ISO 27001 framework toggle enabled at M1.
- **Audit firm engagement** — Schellman SOC 2 incumbent extends to ISO 27001 Stage 1+2.
- **WI-R5-3** crosswalk delivery (this doc + `ISO27001-CROSSWALK-2026-05-15.md` + `ISO27001-GAP-ANALYSIS.md`).
- **ISO-unique gap closure** — 7 minor gaps closed by T+3m (per `ISO27001-GAP-ANALYSIS.md §5`).

## 14. Acceptance criteria for this roadmap

- **Quantitative target:** ISO/IEC 27001:2022 certificate issued by **Q1-2027** with Schellman as audit partner.
- **Qualitative target:** ISO certification roadmap is **calendared, budgeted, and de-risked** to the extent that an enterprise prospect asking "when can we get your ISO cert?" gets a date + audit firm + scope answer, not a hand-wave.
- **Stretch target:** stack the surveillance audits with SOC 2 Type II annual cycles to keep total compliance audit cost < $150k/yr from 2028 onwards.

## 15. Cross-references

- **Master crosswalk:** `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`
- **ISO-unique gap analysis:** `specs/_compliance/ISO27001-GAP-ANALYSIS.md`
- **SOC 2 Type I roadmap (parallel track):** `specs/_compliance/SOC2-ROADMAP.md`
- **SOC 2 evidence rollup:** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **SOC 2 gap register:** `specs/_compliance/SOC2-GAP-ANALYSIS.md`
- **Compliance matrix (Level 3 framework crosswalk):** `specs/03_architecture/compliance_matrix.md` §3
- **ISO 27001 SoA baseline CSV:** `specs/_audits/iso27001-soa.csv`
- **Customer-facing trust page:** `apps/docs/docs/trust/iso27001.mdx`
- **Vendor risk register (A.5.19 evidence):** `specs/_compliance/VENDOR-RISK-REGISTER.md`
- **BCP/DR drill cadence (A.5.29 + A.5.30 evidence):** `specs/_compliance/BCP-DR-DRILL-CADENCE.md`

## 16. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 R-prep ISO 27001 agent) | Initial phased ISO 27001:2022 certification roadmap; 8 milestones M0..M7 spanning T+0..T+9m; Q1-2027 cert target; stacked with SOC 2 Type I Schellman engagement; $55-90k first-year cost; 7 ISO-unique gaps close before Stage 1; surveillance + recert calendar laid out. |

---

**Fim ISO27001-ROADMAP.**
