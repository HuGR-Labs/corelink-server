# Case Study — [ENTERPRISE_CUSTOMER_NAME or SECTOR_DESCRIPTOR]: Enterprise BYOK on CoreLink

> **DRAFT — pending Fortune-500 lighthouse engagement confirmation. NOT FOR PUBLICATION until customer-approved + Legal-cleared for external distribution.**
> Trace: spec contract S-20 §5.2 R-S20-4 · WI-S20-004 lighthouse customer attestations (1 enterprise BYOK slot) · WI-S20-008 §2.1.3 (case study #3) · CAP-GA-004 · WI-S20-005 DPA signature gate.
> Status: **DRAFT — pending Fortune-500 enterprise engagement (sector candidates: financial services / FedRAMP-ready / EU regulated). Sanitized variant under NDA available for sales.**

---

## Customer

**[ENTERPRISE_CUSTOMER_NAME]** — **[SECTOR]** enterprise headquartered in **[HQ_REGION]** with engineering presence across **[ENG_REGIONS]**. **[ENG_HEADCOUNT]** engineers. Regulated under **[REGULATORY_FRAMEWORK]** (e.g. PCI DSS, HIPAA, FedRAMP Moderate readiness, EU-regulated financial services, or equivalent).

> Candidate enterprise lighthouse profiles under engagement:
>
> - **Financial services** — EU or US headquartered, multi-region engineering, BYOK procurement requirement.
> - **FedRAMP-ready ISV** — building government-facing product, residency mandatory.
> - **EU regulated** — DSA / DORA / NIS2 scope, Schrems II top-of-mind.
>
> Final candidate selection per Sales engagement Q3.

## The problem

The customer's procurement evaluation for any developer-infrastructure vendor includes three hard requirements that most remote-cache vendors fail at:

1. **Customer-managed keys.** The vendor must not be able to read the customer's bytes unilaterally. Vendor-managed encryption, even FIPS-validated, does not satisfy this.
2. **Verifiable erasure.** A right-to-erasure or contractual-erasure request must produce an artifact the customer's auditor can independently verify against the customer's own root of trust.
3. **Residency.** Data must demonstrably not leave the customer-selected region, and that property must be structural rather than configurable.

The customer's existing build infrastructure was a hybrid of self-hosted caches and a SaaS vendor that met requirement (3) partially and requirements (1) and (2) only at the marketing-language level.

## Why CoreLink

No customer has selected CoreLink for this lighthouse yet. The following are
the proposed evaluation criteria, not customer results or shipped capabilities:

- Whether a future, separately enabled customer-managed key and kill-switch
  path satisfies `INV-BYOK-CRYPTO-SOVEREIGNTY`.
- Whether a future erasure-attestation implementation satisfies
  `INV-ERASURE-ATTESTATION-SIGNED` with current evidence.
- Whether an external pentest is commissioned; no such assessment exists for
  this draft.
- The residency and incident-response evidence a candidate requires.
- The DPA/SCC review and signature path required by external counsel.

## The migration

The customer's migration was a phased rollout across **[ROLLOUT_PHASES]** CI environments, beginning with a low-risk pre-production CI tier and expanding to the regulated production-equivalent CI estate over **[MIGRATION_DURATION]**.

If the future capability is enabled, BYOK provisioning would use
**[CUSTOMER_KMS_PROVIDER]** (one of AWS KMS / GCP KMS / Azure Key Vault /
HashiCorp Vault). No customer KMS audit or CoreLink BYOK provisioning exists
for this draft.

No DPA or SCC signature exists for this draft. Counsel and the customer would
need to execute the applicable instrument before any migration.

## 30-day observation window

- No observation window has started; no SLA result is available.
- **[INCIDENT_SUMMARY]** — placeholder pending observation window completion.
- No BYOK kill-switch drill has been executed and no customer-side artifact
  exists.
- No erasure-attestation replay or independent auditor verification exists.

## Enterprise lighthouse testimonial (not yet collected)

> **No customer testimonial exists yet.** The former paragraph was illustrative
> copy, not a statement made by an identified customer. Do not attribute it,
> circulate it as a quote, or use it in a sales conversation until a real
> enterprise customer has completed the BYOK drill, supplied attributable
> evidence, and approved the exact wording for the intended audience.

## Sales availability

No customer-specific or sanitized variant is available for sales. A future
variant may be prepared only after the engagement, evidence, customer approval,
and Legal clearance described above. Contact: `sales@humangr.com`.

---

## Internal notes (strip before publish)

- Engagement target: per WI-S20-004 timeline (D+10..D+25) via Sales engagement.
- Status DRAFT pending engagement + customer approval + Legal clearance per WI-S20-005 cumulative.
- Sanitized variant maintained for NDA-gated sales distribution.
- All metrics are placeholders. No specific dollar amounts. No unverified compliance claims.
- The kill-switch and erasure-attestation bullets now state that no execution or
  evidence exists; they are not a methodology claim or a customer result.
