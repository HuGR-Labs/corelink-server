---
id: "ADR-S14-007"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
wi: "WI-S14-008"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags:
  - "adr"
  - "dpa"
  - "schrems-ii"
  - "tia"
  - "edpb-01-2020"
  - "legal-externo"
  - "gdpr-art-46"
  - "lgpd-art-33"
  - "12th-sign-off"
  - "lighthouse-customer"
  - "s14"
supersedes: null
superseded_by: null
---

# ADR-S14-007 — DPA Amendment + Schrems II TIA (EDPB Recommendations 01/2020) + Legal Externo Review Path + Lighthouse Customer + 12th Sign-off Legal Counsel Exception

## Status

**ACTIVE** — Ratified 2026-05-14, WI-S14-008.

---

## Context

CoreLink S-14 delivers data residency, BYOK encryption, and erasure attestation for 4 enumerated regions (WNAM/ENAM/WEUR/SAM). Enterprise customers — particularly in the EU/EEA, Brazil, and US financial/regulated sectors — require:

1. A **Data Processing Agreement (DPA)** per GDPR Art. 28 covering residency commitment per region.
2. A **Schrems II Transfer Impact Assessment (TIA)** per EDPB Recommendations 01/2020, documenting supplementary measures for US sub-processor (Cloudflare) jurisdiction risk.
3. Evidence of **Legal externo review** (no in-house counsel at solo-founder tier; per ADR-0034).
4. **1 lighthouse enterprise customer** with signed DPA demonstrating real-world contract closure.
5. **12th sign-off Legal Counsel exception** per sprint.md §14 (legal-touching WI; precedent S-12 Compliance Officer).

Without these artifacts, enterprise contract closure stalls, revenue is blocked, and GDPR Art. 46 / LGPD Art. 33 §1 transfer obligations are unmet.

---

## Decision

### D1 — DPA Amendment Template (Legal Externo Reviewed)

**Chosen**: Author a comprehensive DPA Amendment template (`legal/dpa-residency-amendment.md`) covering 15 sections + 3 appendices, structured for external Legal review, not authored as final legal text.

**Rationale**:
- CoreLink is Processor; Customer is Controller. GDPR Art. 28 requires a DPA.
- Residency commitment per region must be explicit (4 regions enumerated) — generic "multi-region" language is rejected by customer Legal teams.
- Template structure enables Legal externo firm to redline efficiently within 6-week lead time.
- DPA references S-14 implementation evidence (BYOK FIPS, erasure attestation, region pinning) ensuring legal commitments match technical capabilities.

**Rejected alternatives**:
- Author in-house without Legal review: Anti-scope (legal exposure permanent; no in-house counsel).
- Use off-the-shelf SaaS DPA template: Does not cover BYOK FIPS evidence, erasure attestation, or 4-region specificity required by enterprise Legal.

### D2 — Schrems II TIA per EDPB Recommendations 01/2020 (Not Custom Framework)

**Chosen**: Structure TIA per EDPB Recommendations 01/2020 framework (Steps 1-6; technical/organisational/contractual supplementary measures), with BYOK customer-controlled CMK as primary effectiveness argument.

**Rationale**:
- EDPB Recommendations 01/2020 is the EU standard framework; customer Legal expects it.
- BYOK Use Case 6 (encryption at rest with customer-held key) is the strongest available supplementary measure for US sub-processor risk: Cloudflare holds only ciphertext; government compelled-disclosure yields only encrypted blobs unreadable without Customer CMK.
- CMK revocation ≤ 5 min globally (INV-BYOK-CRYPTO-SOVEREIGNTY) provides unilateral Customer data sovereignty.
- References: GDPR Art. 46 · LGPD Art. 33 §1 · C-311/18 (Schrems II CJEU) · EDPB Recommendations 01/2020 §80-91 · FISA 702 · CLOUD Act.

**Rejected alternatives**:
- Custom framework: Customer Legal rejects non-standard TIA frameworks.
- Rely solely on EU-US Data Privacy Framework (DPF): DPF subject to Schrems III challenge; defence-in-depth with supplementary measures required regardless.

### D3 — Legal Externo Review (Not In-House Counsel Solo-Tier)

**Chosen**: Engage external GDPR-experienced law firm (~$15-30k, 6-week lead) from candidate list (Schellman Legal / Cooley / DLA Piper / Bird & Bird / Fenwick & West / Latham & Watkins).

**Rationale**:
- No in-house counsel at solo-founder tier (per ADR-0034).
- External GDPR + Schrems II specialist = higher quality redaction than solo-founder drafting.
- Deliverable: redlined DPA + TIA + sign-off letter = Legal Counsel 12th sign-off evidence.
- Budget approved: $15-30k per WI-S14-008 §22.

**Rejected alternatives**:
- Skip Legal review: Anti-scope. Legal exposure permanent for customer-facing contract.
- In-house review only: No in-house counsel; legally insufficient for enterprise enterprise procurement requirements.

### D4 — 1 Lighthouse Enterprise Customer Beta DPA Signed

**Chosen**: Engage 1 enterprise customer beta (parallel to Legal externo review) to sign DPA template, demonstrating contract closure capability.

**Rationale**:
- Sales evidence: lighthouse reference enables future enterprise deals.
- Real-world validation of DPA template negotiation cycle.
- GA gate requires 1 signed customer DPA (S-20).

**Rejected alternatives**:
- 0 customers: No contract closure evidence; GA gate fails.
- 3+ customers: Scope exceeds sprint window; S-19/S-20 handles scale.

### D5 — 12th Sign-off Legal Counsel Exception

**Chosen**: WI-S14-008 requires 12 sign-offs (HIGH_RISK standard 11 + Legal Counsel as 12th exception), per sprint.md §14.

**Rationale**:
- Legal-touching WI: DPA + TIA = customer-facing legal contracts.
- Legal Counsel sign-off = Legal externo firm sign-off letter committed as formal evidence.
- Precedent: S-12 added Compliance Officer as 12th sign-off for SOC 2 attestation.

### D6 — WAIVER-S14-001 if Legal Externo Review Misses Sprint Timeline

**Chosen**: If Legal externo review not completed by D+30 (sprint window), issue WAIVER-S14-001 with 90-day expiry and D+60 GA Evidence Gate hard deadline.

**Rationale**:
- 6-week legal review lead may exceed sprint window.
- WAIVER documents accepted residual risk explicitly; avoids silent compliance gap.
- D+60 aligns with GA Evidence Gate; no customer DPA signed while waiver active.

---

## Consequences

### Positive

- Enterprise contract closure unblocked: DPA + TIA enables procurement approval in GDPR/LGPD regulated sectors.
- BYOK effectiveness argument satisfies EDPB §83 Use Case 6; strongest available Schrems II supplementary measure.
- 4 regions enumerated with failover restrictions: customer Legal accepts explicit residency commitment.
- Legal externo review provides professional indemnity for customer-facing legal text.
- Lighthouse customer DPA signed: real-world evidence for future prospects.
- Quarterly Legal review cycle ensures TIA remains current as EDPB guidance evolves.

### Negative / Trade-offs

- Legal externo cost: $15-30k initial + $5k/quarter ongoing. Budget approved.
- 6-week Legal review lead: May miss sprint window → WAIVER-S14-001 path.
- Per-customer custom clauses require separate Legal externo engagement (+2-4 weeks each).
- Single DPO (Gustavo dual-hat): must contract external DPO per ADR-0034 Option C post-Series A.

### Invariants Preserved

- **INV-DATA-RESIDENCY**: DPA Section 7 commits to data localization in enumerated region.
- **INV-REGION-NO-CROSS-LEAK**: DPA references WI-S14-002 enforcement; failover restrictions explicit.
- **INV-BYOK-CRYPTO-SOVEREIGNTY**: DPA Section 10 + TIA Section 5.1 reference ≤ 5 min CMK revocation.
- **INV-ERASURE-ATTESTATION-SIGNED**: DPA Section 10 + Section 14 reference Ed25519 attestation.

---

## References

| Reference | Type | Purpose |
|---|---|---|
| GDPR Art. 28 | Regulation | DPA requirement |
| GDPR Art. 33 | Regulation | Breach notification 72h |
| GDPR Art. 37 | Regulation | DPO designation |
| GDPR Art. 46(2)(c) | Regulation | SCCs transfer mechanism |
| LGPD Art. 33 §1 | Regulation | Brazil international transfers |
| LGPD Art. 46 | Regulation | Security measures |
| LGPD Art. 48 | Regulation | Breach notification |
| C-311/18 (Schrems II) | CJEU ruling | Transfer impact assessment requirement |
| EDPB Recommendations 01/2020 | EDPB guidance | TIA framework (Steps 1-6, §80-91) |
| NIST SP 800-88 Rev.1 §2.4 | Standard | Crypto-erase erasure method |
| WI-S14-001..007 | Sprint WIs | Technical evidence pack (BYOK + erasure + 4-region) |
| `legal/dpa-residency-amendment.md` | Artifact | DPA template |
| `legal/tia-template.md` | Artifact | TIA template |
| `legal/legal-externo-engagement-contract.md` | Artifact | Engagement plan |
| `legal/quarterly-legal-review-template.md` | Artifact | Quarterly review |
| `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md` | ADR | Waiver if timeline missed |
| ADR-0034 | ADR | No in-house counsel constraint |

---

*ADR-S14-007 · Version 1.0.0 · 2026-05-14 · WI-S14-008.*
