---
id: "LIGHTHOUSE-KIT-05-SLA-ATTESTATION-INSTRUCTIONS"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "marketing", "sla", "attestation", "instructions", "30d", "signoff"]
---

# 05 — SLA Attestation Instructions

> **Use:** Hand to customer at D+40 alongside the pre-filled attestation form.
> **Form template:** `specs/_lighthouse/sla-attestation-template.md`.
> **Filed location post-signing:** `specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-attestation.md` (audit_status: ACTIVE).
> **Why this matters:** the GA Evidence Gate D+60 (per spec contract §10.s20.7) requires 3/3 signed attestations. Yours is one of them.

---

## 1. Overview — who signs what

| Section | Filled by | Reviewed by | Signed by |
|---|---|---|---|
| §1 Customer profile | CoreLink CS (drafts) | Customer signatory | n/a (factual fields, no signature required) |
| §2 SLA actuals (30d measured) | Customer SRE / ops | CoreLink Engineer S-20 lead | n/a (numbers attest themselves) |
| §2.2 Enterprise BYOK additional fields | Customer SRE + CoreLink CS | Customer security/platform lead | n/a |
| §3 Operational observations | CoreLink CS (drafts) | Customer signatory | n/a |
| §4 Aggregate attestation checkboxes | Customer signatory | Legal Counsel (both sides) | Customer signatory (§5.1) |
| §5.1 Customer signatures | Customer | Customer Legal | Customer CTO / VP Eng / Tech Lead / Maintainer + Procurement |
| §5.2 CoreLink signatures | CoreLink | CoreLink Legal | CS + Engineer S-20 lead + Privacy Officer (Enterprise) + Legal Counsel |

---

## 2. Step-by-step (chronological)

### Step 1 — D+38: CoreLink drafts the form

CoreLink CS instantiates `specs/_lighthouse/sla-attestation-template.md` for this customer. §1 and §3.3 (weekly Customer Success engagements) are pre-populated from the weekly check-in recaps. §2 values are auto-extracted from the SLO dashboard.

The draft is filed at `specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-attestation.md` with `doc_status: DRAFT`. The slot id and date stamp follow the canonical format.

The customer receives:
- The draft form (PDF + markdown).
- A read-only Grafana dashboard URL showing the same numbers in the production observability stack.
- A diff explanation: "every value in §2 is the 30d rollup as of D+40 00:00 UTC."

### Step 2 — D+40..D+41: Customer SRE / ops fills §2

The customer's SRE or operations engineer:

1. Opens the shared Grafana dashboard URL.
2. Cross-checks each row in §2 against their own observability stack (Datadog, New Relic, Prometheus federation, whatever they use).
3. Marks each "Met?" column `yes` / `no`. Discrepancies > 0.05 percentage points are reconciled with CoreLink Engineer S-20 lead before signing.
4. For Enterprise BYOK: fills §2.2 against the BYOK chaos-drill log (provided by CoreLink Engineer S-20 lead at D+38).

If any SLO was not met, the customer does NOT sign. Instead, a remediation cycle is triggered per `lighthouse-customer-program.md` §6, the form is re-drafted from a fresh 30-day window, and dates shift accordingly.

### Step 3 — D+41..D+42: Customer signatory reviews §3

The customer signatory (CTO / VP Eng / Tech Lead / OSS Maintainer) reviews:

- §3.1 incidents — confirms the list is complete; adds any incident CoreLink missed.
- §3.2 DSR — confirms or marks N/A.
- §3.3 weekly Customer Success engagements — confirms the recap matches their recollection.
- §4 aggregate attestation checkboxes — ticks all that apply.

Any disagreement is escalated to CoreLink CS within 24 hours; CS resolves or escalates to CS lead.

### Step 4 — D+42..D+43: Procurement / Legal review

The customer's procurement and / or Legal team reviews:

- §4 aggregate attestation language — is this the scope of attestation they're signing?
- §5.1 signatory authority — confirm the signatory is empowered to attest on behalf of the customer entity.
- Whether the customer authorizes CoreLink to reference the attestation in:
  - GA Evidence Gate D+60 PRR pack (EVT-018) — required.
  - Marketing case study materials — optional for Enterprise BYOK (NDA-sanitized); public for OSS Team tier (encouraged).

If procurement requires changes to the form, those are made before signing (not after) and a new revision is filed.

### Step 5 — D+43..D+44: Customer signs §5.1

The customer signatory signs §5.1 (CTO / VP Eng / Tech Lead / OSS Maintainer) via DocuSign. The DocuSign envelope id is captured in the form.

If procurement also signs (Enterprise common case), a second row is added to §5.1 with the procurement signer's details.

### Step 6 — D+44..D+45: CoreLink countersigns §5.2

CoreLink countersignatories sign in order:

1. **Customer Success engineer** — primary day-to-day contact.
2. **Engineer S-20 lead** — technical attestation that the SLO actuals are accurate.
3. **Privacy Officer** — only if the attestation includes DPA-related sub-modes (Enterprise BYOK typically yes; Team tier typically no).
4. **Legal Counsel** — final review that the attestation matches the LOI scope.

All four signatures land via the same DocuSign envelope used for §5.1, then the form is filed at `specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-attestation.md` with `doc_status: SEALED` and `audit_status: ACTIVE`.

### Step 7 — D+45: State machine transition recorded

CoreLink operator transitions the state machine in `corelink-lighthouse-tracker`:

```
Observing → Attested
  evidence_url = specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-attestation.md
  operator_id = op_<alias>
```

An audit chain row is emitted (`event_type = lighthouse_lifecycle_transition`). The customer receives a confirmation email with the signed attestation PDF and a "thank you" note from the founder.

---

## 3. Where the values come from (transparency)

| §2 row | Source metric | Window |
|---|---|---|
| `SLO-AVAIL-CAS-PUT` | `availability:cas_put_sli_30d` | 30d rolling, D+10..D+40 |
| `SLO-AVAIL-CAS-GET` | `availability:cas_get_sli_30d` | 30d rolling, D+10..D+40 |
| `SLO-LAT-CAS-GET p99` | `histogram_quantile(0.99, sum(rate(corelink_cas_get_duration_seconds_bucket[5m])) by (le))` | 30d rolling |
| `SLO-FRESH-BILLING` | `corelink_billing_reconcile_drift_ratio` p99 | 30d rolling |
| `SLO-FRESH-DSR-ERASURE` | `corelink_dsr_erasure_latency_hours_bucket` | window of any DSR triggered |
| `SLO-BYOK-KILL-SWITCH` (Ent.) | `corelink_byok_kill_switch_duration_seconds_bucket` p99 | weekly chaos drills, last 4 weeks |
| Cache hit ratio | `corelink_cache_hit_ratio` | 30d avg + p99 daily |
| GB stored at end-of-window | `corelink_cas_bytes_stored_total` snapshot | D+40 00:00 UTC |

All metrics are exported in the customer's Grafana dashboard (read-only embed) and cross-verifiable against the `lighthouse_sla_samples` D1 table (30 rows, one per day).

---

## 4. FAQ

**Q: What if our internal numbers disagree with CoreLink's?**
A: Within 0.05 percentage points or 5 ms — round to CoreLink's. Larger gap — Engineer S-20 lead investigates and the discrepancy is documented in §3.1 before signing.

**Q: What happens if an SLO was missed mid-window?**
A: Remediation cycle. The 30d window restarts on a fresh `observation_started_at` after the fix is verified. Attestation is delayed accordingly. The original miss is logged in the audit chain regardless of whether a fresh window subsequently passes.

**Q: Who keeps the signed form?**
A: Both parties. CoreLink files in `specs/_audits/`; the customer keeps their DocuSign envelope. Both are durable copies; the spec repo path is the canonical version for the GA Evidence Gate.

**Q: Can we redact §3.1 incidents from the public case study?**
A: Yes (Enterprise BYOK default; OSS Team tier on request). The full attestation remains in `specs/_audits/`; the sanitized version is what marketing uses.

**Q: What if procurement is slow?**
A: Soft deadline D+45; hard deadline D+50. Beyond D+50, founder escalates directly to customer procurement leadership. No attestation past D+60 — the cohort risks falling out of the GA Evidence Gate.

**Q: Does signing this commit us to anything beyond the lighthouse program?**
A: No. The attestation is a factual report on the 30-day window. Continued use of CoreLink is governed by the LOI / MSA / DPA, not this form.

---

## 5. Filing & audit trail

- Customer-facing PDF: DocuSign envelope, customer retains.
- CoreLink canonical: `specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-attestation.md` (markdown front-matter `doc_status: SEALED`, `audit_status: ACTIVE`).
- Audit chain row: emitted on `Observing → Attested` transition; `event_type = lighthouse_lifecycle_transition`.
- D1 evidence: `lighthouse_sla_samples` table retains the 30 daily samples used to compute §2 values.
- Cross-reference: `specs/_lighthouse/lighthouse-customer-program.md` §8 (audit trail canonical).

---

**Fim 05-SLA-ATTESTATION-INSTRUCTIONS.**
