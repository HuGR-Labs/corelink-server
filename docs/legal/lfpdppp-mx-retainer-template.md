---
id: "LFPDPPP-MX-RETAINER-TEMPLATE"
type: "engagement_template"
doc_status: "DRAFT"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
wi: "WI-S11-004"
tags:
  - "legal-externo"
  - "retainer"
  - "engagement"
  - "lfpdppp"
  - "mexico"
  - "mx-attorney"
  - "arco"
  - "s11"
  - "wave-28"
reviewers:
  - "Owner"
  - "Privacy Officer"
  - "MX Attorney (TBD — counter-signatory)"
supersedes: null
superseded_by: null
related_audit: "specs/_audits/2026-05-16-lfpdppp-mx-engagement-package-final.md"
related_letter: "docs/legal/lfpdppp-mx-engagement-letter-template.md"
related_shortlist: "docs/legal/lfpdppp-mx-attorney-shortlist.md"
related_scoping_packet: "specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md"
---

# LFPDPPP MX Attorney Engagement — Retainer Template

## CoreLink / HuGR Labs — WI-S11-004 §6.1.4

> **Status:** Engineering-side draft retainer template prepared by the wave-28 step-6 R-prep stream. **Final retainer is pinned to the attorney's own preferred contract template** (per shortlist §3.1 step-3 + engagement-email §1 requirement 7); this draft is the **negotiating baseline** ensuring CoreLink-side requirements (scope freeze, deliverables, IP, confidentiality, cap) are met regardless of which counter-party template wins.
>
> **Use:** present this draft alongside the engagement letter (`docs/legal/lfpdppp-mx-engagement-letter-template.md`) when negotiating the retainer. Owner + attorney redline jointly; final signed instrument is the **attorney-side template with these clauses merged-in** OR this template with attorney-requested amendments. Either path is acceptable provided §1-§8 below are preserved in the executed instrument.
>
> **Companion artifacts** (read before signing):
>
> 1. `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md` — scoping packet (§1-§8; the binding work statement)
> 2. `docs/legal/lfpdppp-mx-engagement-letter-template.md` — engagement letter (defines scope; this retainer operationalizes it)
> 3. `docs/legal/lfpdppp-mx-attorney-shortlist.md` — candidate shortlist (5 firms)
> 4. `legal/privacy-notice/REVIEW_PROCESS.md` — EVT-044 PDF SOP

---

## 1. Parties

| Party | Role | Entity | Address | Email |
|---|---|---|---|---|
| **Client** | Engaging party | HuGR Labs, Inc. (Delaware corporation) | [HuGR Labs Delaware registered office address] | gustavo@humangr.com |
| **Attorney** | Engaged party | [ATTORNEY NAME], cédula profesional [#####] | [Firm name + Mexican address] | [Attorney email] |

This **Retainer Agreement** (the "Agreement") is entered into on `[EFFECTIVE DATE — target 2026-06-12]` between the parties above.

---

## 2. Scope freeze

### 2.1 In-scope work statement

Attorney shall render a **written legal opinion** on §1-§7 of the scoping packet (`specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md`), covering:

1. **§1** — Verification of the LFPDPPP statutory citations inventory (11 citation sites covering Arts. 8, 10, 20, 21, 22-26, 32, 36 + Capítulo VII PPD).
2. **§2** — Sign-off on the 2 es breach-notification templates as LFPDPPP Art. 20-21 compliant.
3. **§3** — Sign-off on the es-MX privacy notice (`legal/privacy-notice/v1.0.0/es-MX.md`) as LFPDPPP-compliant, with signed EVT-044 PDF.
4. **§4** — Sign-off on the DSR erasure flow as ARCO-compliant (Arts. 22-26 + Art. 32 SLA + Art. 26 retention exception).
5. **§5** — Confirmation that no MX data residency is required (Region::Iad fallback acceptable).
6. **§6** — Confirmation that the sub-processor disclosure satisfies LFPDPPP Art. 36 Encargado disclosure obligation.
7. **§7** — Written answers to the 4 open questions (Q1: Art. 21 anchor; Q2: Art. 10 VI scope — resolves wave-17-ter audit D-8; Q3: Reglamento Art. 67 cross-border form; Q4 [optional]: Capítulo VII PPD reference in §13).

### 2.2 Effort envelope

| Activity | Hours (low — high) |
|---|---|
| Read scoping packet (§1-§7) | 1.0 — 2.0 |
| Verify citations inventory + statutory cross-check | 1.0 — 2.0 |
| Q1 (Art. 21 anchor) — review breach templates + render opinion | 1.0 — 1.5 |
| Q2 (Art. 10 VI) — review privacy notice §4 + render opinion + propose corrected text | 2.0 — 3.0 |
| Q3 (Reglamento Art. 67) — review privacy notice §6 + render opinion + propose corrected table if needed | 1.5 — 2.5 |
| Q4 (Capítulo VII reference, optional) | 0.5 — 1.0 |
| Draft + sign EVT-044 PDFs (3 total: 1 privacy notice + 2 breach templates) | 1.0 — 1.5 |
| **Total scope-freeze envelope** | **8.0 — 13.5 hours** |

### 2.3 Out-of-scope (explicit carve-outs)

- Authoring final executed DPA for any specific Mexican customer (per-customer engagement separate).
- Mexican tax / fiscal record retention opinion (covered by Código Fiscal de la Federación; separate engagement if needed).
- Litigation advice / PPD defense before INAI (separate engagement if a PPD or verificación procedure is initiated).
- Other locales — pt-BR LGPD review + en-US GDPR/CCPA review are separate engagements with BR / EU attorneys.
- Implementation review — Attorney reviews the legal artifacts; engineering implementation of ARCO endpoints is reviewed separately by the CoreLink Architect + DPO.

---

## 3. Deliverables

Attorney shall deliver, by the date set in §6 below:

### 3.1 Written legal opinion (PDF, ≥ 4 pages, Spanish, formal-legal register)

Addressing §1-§7 of the scoping packet, with explicit "compliant" / "compliant with redlines" / "non-compliant" verdict per section.

### 3.2 Three (3) EVT-044 PDFs

Per `legal/privacy-notice/REVIEW_PROCESS.md §2.3`, one PDF per reviewed artifact:

1. **EVT-044 PDF — es-MX privacy notice** — for `legal/privacy-notice/v1.0.0/es-MX.md`; uploaded by Client to R2 `evidence-legal/v1.0.0-es-MX.pdf`.
2. **EVT-044 PDF — breach template `audit-chain-integrity-incident.md`** — for `docs/customer-comm/breach-notification/v1.0.0/es/audit-chain-integrity-incident.md`; uploaded by Client to R2 `evidence-legal/v1.0.0-es-audit-chain.pdf`.
3. **EVT-044 PDF — breach template `dsr-pipeline-temporary-degradation.md`** — for `docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md`; uploaded by Client to R2 `evidence-legal/v1.0.0-es-dsr-degradation.pdf`.

Each EVT-044 PDF shall carry:

- Attorney name + cédula profesional number.
- Date of review.
- Hash (BLAKE3 or SHA-256) of the reviewed file as it stood at review time (Client supplies the hash via the engagement packet at retainer-signature time).
- Explicit attestation in Spanish: *"He revisado este documento bajo la LFPDPPP, su Reglamento, los Lineamientos de Aviso de Privacidad (INAI), y los criterios INAI vigentes. El documento es [conforme / conforme con las redlines adjuntas / no conforme — ver opinión]."*
- Attorney signature (electronic or wet) + date.

### 3.3 Redlines (if any)

Redlined source documents for any artifact where the verdict is "compliant with redlines" or "non-compliant". Delivery mode: Word .docx / markdown diff / annotated PDF — Attorney's preference, declared at retainer signature.

### 3.4 List of material issues (if any)

If material issues are surfaced, Attorney shall deliver a prioritized list (P0 must-fix-before-GA / P1 fix-pre-launch / P2 fix-within-90d-post-GA) with citation anchors per item.

### 3.5 Q&A reserve

Up to 2 hours follow-up Q&A within 30 days of opinion delivery, billed at the same hourly rate (counted within the §4 cap), for clarification questions on the opinion.

---

## 4. Fees + budget envelope

### 4.1 Fee structure

| Component | Amount (USD) | Notes |
|---|---|---|
| **Retainer (deposit at signature)** | **USD 2,000 — 5,000** | Credited against final invoice; non-refundable if Attorney begins work then engagement is terminated by Client absent Attorney breach. |
| **Hourly rate (primary review)** | **USD 250 — 450/hr** | Pinned to specific rate `$[RATE]/hr` at signature; rate covers all attorney activities within scope. |
| **Hourly rate (Q&A reserve)** | Same as primary | Q&A reserve hours billed at primary rate; capped at 2 hours within the §4.2 hard cap. |

### 4.2 Hard cap

**Total fees + expenses shall not exceed USD 8,000** without prior written Client approval for overage. This cap corresponds to ~13 attorney hours at the upper-bound rate (USD 450/hr × 13 hours ≈ USD 5,850 plus retainer credit + Q&A reserve, rounded up to USD 8,000 envelope).

Any work beyond 13 hours requires Client approval **before** the work is performed. Attorney shall notify Client when 80% of the hour budget is consumed (i.e. at 10.4 hours) so Client can authorize overage or scope-reduce.

### 4.3 Expenses

| Expense category | Reimbursement |
|---|---|
| Mexican legal database access (e.g. INAI criterios database, Reglamento commentary) | At cost, ≤ USD 200 total |
| Travel / in-person meetings | None — engagement is remote-only |
| Cédula profesional verification | None — Attorney bears their own credential maintenance |
| EVT-044 PDF generation + electronic signature | None — Attorney bears their own e-signature infrastructure cost |

Expenses are subject to the §4.2 cap (i.e. counted within the USD 8,000 envelope, not on top).

### 4.4 Payment terms

- **Deposit (50%):** due within 7 days of retainer signature; payable to Attorney's nominated bank account (wire or Stripe Treasury at Client's preference).
- **Final invoice (50%):** due within 14 days of delivery of all §3 deliverables. Late payment carries 1.5% monthly interest per Mexican commercial-code standard.
- **Currency:** USD primary; MXN at Banxico fix on invoice date acceptable if Attorney requests.

---

## 5. Intellectual property + work product

### 5.1 Work-for-hire assignment

All deliverables under §3 — including the written opinion, the 3 EVT-044 PDFs, and any redlines — are **work-for-hire** ("obra por encargo" per Mexican Ley Federal del Derecho de Autor Art. 83) for the Client. All intellectual property rights (patrimonial rights / derechos patrimoniales) in the deliverables vest in the Client upon delivery; Attorney retains moral rights (derechos morales) per Art. 21 of the same statute.

### 5.2 Attorney's pre-existing IP

Attorney's pre-existing legal templates, internal know-how, and general practice materials remain Attorney's property. Such materials may be incorporated into the deliverables under a non-exclusive license to Client for Client's internal use and regulatory disclosure.

### 5.3 Client use rights

Client may:

- Publish the written opinion + redlines in the CoreLink spec corpus (`legal/privacy-notice/v1.0.0/es-MX.md` metadata + `specs/_audits/`) and on R2 (`evidence-legal/`).
- Disclose the opinion to auditors (SOC 2, ISO 27001, INAI verificación) and to specific customers under NDA.
- Reference the Attorney's name + cédula profesional number in the privacy notice metadata.yaml + EVT-044 PDFs.

Client may NOT:

- Reuse Attorney's deliverables for a separate legal matter without prior written consent.
- Hold Attorney out as Client's "general counsel" or "MX in-house counsel" — Attorney is engaged for this specific opinion only.

---

## 6. Engagement window + timeline

| Milestone | Owner | Target date |
|---|---|---|
| Retainer signed; 50% deposit paid | Both parties | **2026-06-12** (T+0) |
| Engagement packet shared (hashes recorded) | Client | T+1d (2026-06-13) |
| Mid-engagement check-in (status update from Attorney) | Attorney | T+10d (2026-06-22) |
| Written opinion + 3 EVT-044 PDFs + redlines delivered | Attorney | T+21d (**2026-07-03**) |
| Final payment (50% balance) | Client | T+35d (2026-07-17) |
| Q&A reserve window (2 hours) | Both parties | T+21d to T+51d (2026-07-03 → 2026-08-02) |
| Opinion absorbed into spec corpus | Client | wave-26+ absorption (post-2026-07-03) |

**Engagement window:** **2026-05-20 → 2026-06-20** (per task charter); the executed retainer's specific dates anchor to the actual signature date and slide the table above by the offset (e.g. if signed 2026-06-15, opinion delivery target is 2026-07-06).

**Critical path constraint:** the written opinion is a pre-GA hard-gate; opinion delivery before 2026-09-01 is the binding outer deadline regardless of signature slippage.

---

## 7. Confidentiality + NDA

### 7.1 Confidential information

All CoreLink artifacts shared with Attorney are **"HuGR Labs Confidential — Attorney Work Product"** under:

- Mexican Código Federal de Procedimientos Civiles privileges (secreto profesional del abogado).
- Mexican Ley Federal de Protección al Consumidor + LFPDPPP-equivalent attorney-client privilege.
- US attorney-client privilege if Attorney is admitted in any US jurisdiction (cross-border preservation).

### 7.2 Three-year NDA covering all Anexos

Attorney agrees:

- **Non-disclosure period:** 3 years from delivery of the final deliverable (§3.1).
- **Scope:** all engagement-packet artifacts (the 7 anexos enumerated in `docs/legal/lfpdppp-mx-engagement-email-template.md §0`) + any source-code excerpts + any architectural diagrams shared during the engagement.
- **Carve-outs:**
  - Information already public at time of disclosure.
  - Information independently developed by Attorney without reference to Client materials.
  - Disclosure compelled by law / INAI proceeding / court order (Attorney shall notify Client within 5 business days of compelled disclosure unless prohibited by the compelling authority).
- **Return / destruction:** upon termination, Attorney destroys all Client materials within 30 days OR returns them at Client's option; Attorney retains a single archival copy in their conflicts-check system per Mexican Barra Mexicana ethical-record-retention rules.

### 7.3 Non-disclosure of engagement existence

Attorney shall not disclose the existence of this engagement, or the identity of Client as a privacy-review client, to any third party without Client's prior written consent — except as required by law / INAI proceeding, OR as a generic anonymized credential in the firm's published practice description (e.g. "a Delaware-incorporated SaaS company"), with no identifying detail.

---

## 8. General terms

### 8.1 Governing law

This Agreement is governed by **Mexican federal law**. Disputes shall be resolved by **arbitration** under the rules of the **Centro de Arbitraje de México (CAM)** in Mexico City, in Spanish, by a sole arbitrator. Judgment on the award may be entered in any court of competent jurisdiction (including the US District Court for the District of Delaware where the Client entity is incorporated).

### 8.2 Conflict of interest

Attorney warrants that:

- No current / recent (≤ 3 years) representation of Cloudflare, Inc. / Stripe, Inc. / Neon, Inc. / Grafana Labs (CoreLink's 4 sub-processors).
- No current / recent (≤ 3 years) representation of a direct CoreLink competitor (multi-tenant content-addressable cache / build-cache / package-cache SaaS).
- Standard firm conflict check has been completed and cleared at intake.

If a conflict arises mid-engagement, Attorney shall notify Client within 5 business days and either obtain a written waiver from both parties OR withdraw with prorated refund.

### 8.3 Independent contractor

Attorney is an **independent contractor**, not an employee or agent of Client. Attorney is responsible for their own taxes, social security (IMSS / INFONAVIT), and professional liability insurance.

### 8.4 Termination

Either party may terminate this Agreement:

- **For convenience** — with 7 days written notice; Client owes pro-rated fees for work performed; Attorney retains the deposit only to the extent work has been performed and billed.
- **For cause** — immediate termination on material breach (e.g. confidentiality breach, missed delivery date by ≥ 14 days without notice, scope-out-of-bounds disclosure to a third party); aggrieved party retains all remedies under §8.1.

### 8.5 No reliance by third parties

The written opinion is delivered for Client's benefit only. Third parties (customers, auditors, INAI) may review the opinion as a record of legal review performed; they may not rely on it for their own legal decisions without independent legal counsel.

### 8.6 Severability + entire agreement

If any provision is unenforceable, the rest remains in force. This Agreement + the engagement letter + the scoping packet (the 3 referenced documents at §0 above) constitute the entire agreement; oral modifications are not binding.

---

## 9. Signatures

**HuGR Labs, Inc.** (Client)

- Signature: ______________________
- Name: Gustavo Schneiter
- Title: Founder + CEO
- Date: ______________________

**[ATTORNEY NAME]** (Attorney)

- Signature: ______________________
- Name: [ATTORNEY NAME]
- Cédula profesional: [#####]
- Firm: [FIRM NAME]
- Date: ______________________

---

## 10. Anexo — engagement-packet hash manifest (filled at signature)

| Artifact | Path | Hash (BLAKE3 or SHA-256; Client supplies) |
|---|---|---|
| Scoping packet | `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md` | `[HASH]` |
| Engagement letter | `docs/legal/lfpdppp-mx-engagement-letter-template.md` | `[HASH]` |
| Privacy notice (es-MX) | `legal/privacy-notice/v1.0.0/es-MX.md` | `[HASH]` |
| Breach template — audit-chain | `docs/customer-comm/breach-notification/v1.0.0/es/audit-chain-integrity-incident.md` | `[HASH]` |
| Breach template — DSR degradation | `docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md` | `[HASH]` |
| Review-process SOP | `legal/privacy-notice/REVIEW_PROCESS.md` | `[HASH]` |

Hashes are pinned at signature; if any artifact changes mid-engagement, Client shall notify Attorney within 1 business day and re-issue the manifest. Attorney's EVT-044 PDFs reference these hashes per §3.2.

---

## 11. Cross-references

- **Engagement letter** (the binding work statement): `docs/legal/lfpdppp-mx-engagement-letter-template.md`
- **Scoping packet** (the engineering-side §1-§8 inventory): `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md`
- **Shortlist** (5 candidate firms): `docs/legal/lfpdppp-mx-attorney-shortlist.md`
- **Email template** (Mustache RFP outreach): `docs/legal/lfpdppp-mx-engagement-email-template.md`
- **Tracker** (state machine + per-firm rollup): `scripts/admin/lfpdppp-mx-tracker.py` + `reports/lfpdppp-mx-tracker.json`
- **Review process SOP** (EVT-044 PDF SOP): `legal/privacy-notice/REVIEW_PROCESS.md`
- **Wave-28 step-6 closure audit:** `specs/_audits/2026-05-16-lfpdppp-mx-engagement-package-final.md`
- **Debt-register row:** `specs/_audits/2026-05-15-debt-register.md DEBT-025`
- **Pentest engagement-contract precedent**: `docs/legal/pentest-engagement-contract-template.md` (wave-26 precedent; same retainer-template pattern applied here for LFPDPPP MX).

---

## 12. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Gustavo Schneiter (via Claude Opus 4.7, wave-28 step-6 R-prep stream) | Initial draft. 12-section template covering parties, scope freeze, deliverables (opinion + 3 EVT-044 PDFs + redlines), fee structure (USD 2k-5k retainer + USD 250-450/hr capped at 13 hrs = USD 8k max), IP work-for-hire assignment, 3-year NDA covering all Anexos, governing-law Mexican federal + CAM arbitration, conflict-of-interest warranty, engagement window 2026-05-20 → 2026-06-20. |

---

**End LFPDPPP MX retainer template** — engineering-side draft; pin to attorney's preferred template at signature.

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
