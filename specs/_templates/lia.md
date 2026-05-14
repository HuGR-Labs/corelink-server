---
id: "LIA-TEMPLATE"
type: "template"
doc_status: "FROZEN"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Privacy Officer"
tags: ["lia", "legitimate-interest", "lgpd-art-10", "gdpr-art-6-1-f", "template"]
evidence_event: "EVT-046"
evidence_retain: "3y"
---

# Legitimate Interest Assessment (LIA) Template
### LGPD Art. 10 / GDPR Art. 6(1)(f) — ICO Three-Part Test

> **When to use:** Complete this LIA when processing relies on legitimate interest
> as the legal basis (LGPD Art. 10 / GDPR Art. 6(1)(f)). Reference:
> WP29 Opinion 06/2014 on legitimate interests endorsed by EDPB (8 balancing criteria);
> ICO three-part test (purpose, necessity, balance).
>
> **How to use:** Copy this file to `legal/lia/<feature-slug>.md`. Fill every
> section marked `[FILL]`. Privacy Officer reviews annually and on any change to
> the underlying processing.

---

## Metadata

| Field | Value |
|---|---|
| LIA ID | `[FILL: e.g. LIA-S09-001]` |
| Feature / Sprint | `[FILL: e.g. S-09 WI-S09-006 — telemetry aggregation]` |
| Data Controller | HuGR Labs Ltda (LGPD) / HuGR Labs Ltd (GDPR) |
| Encarregado / DPO | `[FILL: name + contact]` |
| Privacy Officer | `[FILL: name]` |
| Legitimate interest claimed | `[FILL: e.g. operational telemetry for service reliability and security]` |
| Processing basis | legitimate_interest (LGPD Art. 10 / GDPR Art. 6(1)(f)) |
| Date initiated | `[FILL: YYYY-MM-DD]` |
| Next review date | `[FILL: annual minimum — YYYY-MM-DD]` |
| Legal frameworks | GDPR Art. 6(1)(f) + LGPD Art. 10 + WP29 Opinion 06/2014 endorsed by EDPB + ICO LIA guidance |
| Related DPIA | `[FILL: legal/dpia/<slug>.md or "N/A"]` |
| EVT evidence path | `evidence-legal/lia-<slug>.md` (R2 retain 3y per EVT-046) |

---

## Section 1 — Purpose Test

> *"The purpose must be legitimate — lawful, clearly articulated, and not contrary to law."*
> (WP29 Opinion 06/2014 §3.1)

### 1.1 Statement of legitimate interest
`[FILL: Describe the specific, fully articulated legitimate interest. Vague descriptions (e.g., "business purposes") are insufficient. Be precise about what operational or business need is served.]`

### 1.2 Lawfulness of the interest
`[FILL: Confirm the interest is lawful under applicable law. Is it recognized by GDPR recitals (recitals 47-49), LGPD Art. 10, or other legislation? Are there any conflicting legal obligations?]`

### 1.3 Real and present interest
`[FILL: Is the interest current and genuine — not hypothetical? Document evidence that the interest is actually being pursued (e.g., operational metrics, security incidents prevented, etc.).]`

### 1.4 LGPD Art. 10 specific requirements
`[FILL: LGPD Art. 10 requires that legitimate interest processing: (a) is for legitimate purposes; (b) is necessary for the controller's purposes; (c) does not prevail over the fundamental rights of the data subject. Address each requirement.]`

---

## Section 2 — Necessity Test

> *"The processing must be necessary — the least privacy-invasive means to achieve the purpose."*
> (ICO three-part test, part 2; WP29 Opinion 06/2014 §3.2)

### 2.1 Link between processing and purpose
`[FILL: Is there a direct link between the data processed and the legitimate interest? Demonstrate that the processing actually serves the stated purpose.]`

### 2.2 Data minimization analysis
`[FILL: Could the purpose be achieved with less data, or without processing personal data at all? Document alternatives considered and why they are insufficient.]`

### 2.3 Less intrusive alternatives considered

| Alternative | Considered | Why insufficient |
|---|---|---|
| Aggregate-only (no per-user) | `[YES/NO]` | `[FILL]` |
| Synthetic/anonymized data | `[YES/NO]` | `[FILL]` |
| Consent-based processing | `[YES/NO]` | `[FILL]` |
| Contractual necessity basis | `[YES/NO]` | `[FILL]` |
| `[Other alternative]` | `[YES/NO]` | `[FILL]` |

### 2.4 Storage limitation justification
`[FILL: How long is personal data retained for this processing? Why is this period necessary? What retention controls are in place?]`

---

## Section 3 — Balance Test

> *"The legitimate interest must not be overridden by the data subject's interests or fundamental rights."*
> (GDPR Art. 6(1)(f); LGPD Art. 10 caput; WP29 Opinion 06/2014 §3.3 — 8 balancing criteria)

### 3.1 WP29 Opinion 06/2014 endorsed by EDPB — 8 balancing criteria

| Criterion | Assessment | Detail |
|---|---|---|
| 1. Nature of the legitimate interest (how compelling) | `[FILL]` | `[FILL]` |
| 2. Impact on data subject — nature of the data | `[FILL]` | `[FILL: ordinary vs sensitive, special category]` |
| 3. Impact on data subject — nature of processing | `[FILL]` | `[FILL: profiling, monitoring, disclosure risk]` |
| 4. Status of the data subject (vulnerable?) | `[FILL]` | `[FILL: consumers, employees, children, etc.]` |
| 5. Reasonable expectations of data subject | `[FILL]` | `[FILL: would subjects expect this processing?]` |
| 6. Consequences of processing (potential harm) | `[FILL]` | `[FILL: financial, reputational, physical, social harm]` |
| 7. Safeguards applied to reduce impact | `[FILL]` | `[FILL: pseudonymization, access controls, retention limits, opt-out]` |
| 8. Overriding interests / fundamental rights | `[FILL]` | `[FILL: privacy, data protection, freedom of expression, etc.]` |

### 3.2 Balance verdict
`[FILL: On balance, does the legitimate interest override the data subject's rights and interests? Justify. If the balance is close, what additional safeguards tip it in favor of legitimate interest?]`

### 3.3 Opt-out / objection mechanism (GDPR Art. 21 / LGPD Art. 18 II)
`[FILL: Describe the opt-out mechanism available to data subjects. How is the right to object honored? What is the response SLA?]`

### 3.4 Privacy notice transparency
`[FILL: Is the legitimate interest processing disclosed in the privacy notice (WI-S11-004)? Reference the relevant section of the privacy notice.]`

---

## Section 4 — Privacy Officer Review

### 4.1 Review conclusion
`[FILL: Summarize whether legitimate interest is a valid legal basis for this processing, based on the three-part test. If YES, processing may proceed. If NO, an alternative legal basis must be identified (e.g., consent).]`

### 4.2 Conditions / mitigations required
`[FILL: List any conditions that must be met for the legitimate interest processing to remain valid (e.g., annual review, specific opt-out mechanism in place, data minimization implemented).]`

### 4.3 Sign-off

| Role | Name | Date | Status |
|---|---|---|---|
| Privacy Officer | `[FILL]` | `[FILL]` | `[ ] Approved / [ ] Pending` |
| DPO / Encarregado | `[FILL]` | `[FILL]` | `[ ] Approved / [ ] Pending` |
| Legal (external) | `[FILL]` | `[FILL]` | `[ ] Approved / [ ] Pending (if required)` |

> **EVT-046 evidence:** Once signed, upload this document to R2 `evidence-legal/lia-<slug>.md` with 3-year retention. Record event `EVT-046` in the audit log.

> **Annual review:** LIA must be reviewed annually AND whenever the underlying processing changes materially. Set a calendar reminder.

### 4.4 LIA changelog

| Version | Date | Author | Change summary |
|---|---|---|---|
| 1.0 | `[FILL]` | `[FILL]` | Initial LIA |

---

*Template version 1.0.0 — WI-S11-008 — LGPD Art. 10 / GDPR Art. 6(1)(f) LIA.*
*Legal citations: WP29 Opinion 06/2014 endorsed by EDPB; ICO LIA guidance; LGPD Art. 10 + 18; GDPR Art. 6(1)(f) + 21.*
