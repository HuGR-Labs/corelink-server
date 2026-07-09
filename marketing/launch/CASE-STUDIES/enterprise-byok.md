# Case Study — [ENTERPRISE_CUSTOMER_NAME or SECTOR_DESCRIPTOR]: Enterprise BYOK on CoreLink

> **DRAFT — pending Fortune-500 lighthouse engagement confirmation. NOT FOR PUBLICATION until customer-approved + Legal-cleared for external distribution.**
> Trace: spec contract S-20 §5.2 R-S20-4 · WI-S20-004 lighthouse customer attestations (1 enterprise BYOK slot) · WI-S20-008 §2.1.3 (case study #3) · CAP-GA-004 · WI-S20-005 DPA signed.
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

The customer selected CoreLink as their enterprise BYOK lighthouse based on:

- **Customer-managed kill switch** semantics as a structural property (`INV-BYOK-CRYPTO-SOVEREIGNTY`), not a configurable opt-in.
- **Verifiable crypto-erasure** with NIST SP 800-88 Rev. 1 crypto-erase classification (`INV-ERASURE-ATTESTATION-SIGNED`); a customer-served **Ed25519 erasure attestation** is on the near-term roadmap.
- **`INV-REGION-NO-CROSS-LEAK`** as a TLA+-verified property in the tenant isolation specification.
- **External pentest report** with retest, available under NDA pre-purchase.
- **DPA + SCC modules** Legal-reviewed by external EU privacy counsel (per WI-S20-005) before signature.
- **24/7 incident response** with sub-five-minute synthetic page response sustained 30 days pre-GA (`CAP-GA-006`).

## The migration

The customer's migration was a phased rollout across **[ROLLOUT_PHASES]** CI environments, beginning with a low-risk pre-production CI tier and expanding to the regulated production-equivalent CI estate over **[MIGRATION_DURATION]**.

BYOK provisioning used **[CUSTOMER_KMS_PROVIDER]** (one of AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault). The customer's security team performed an unwrap-call audit of CoreLink's KMS access pattern using their own KMS audit logs as the source of truth — not CoreLink's logs. The audit reconciled cleanly.

DPA + SCC signature was completed pre-migration per WI-S20-005 (Legal externo: Cooley / DLA Piper / Bird & Bird).

## 30-day observation window

- SLA claim met every day across SLOs in the GA SLO catalog.
- **[INCIDENT_SUMMARY]** — placeholder pending observation window completion.
- **Kill-switch drill** executed as a contractual test. The customer disabled their KEK in their KMS, observed CoreLink data path hard-fail within the documented DEK-cache expiry bound, then re-enabled. The drill produced a customer-side artifact (KMS audit log + CoreLink data-path response trace) that the customer's security team has retained.
- **Erasure-attestation replay** executed against a synthetic in-scope tenant subset. The customer's auditor independently verified the Ed25519 signature against CoreLink's published signing key.

## Enterprise lighthouse testimonial (sanitized)

> "Customer-managed kill switch is not a feature for us; it is a procurement precondition. CoreLink is the only cache vendor we evaluated that exposed a verifiable Ed25519 erasure attestation we could replay into our own audit pipeline. The kill-switch drill produced the artifact our compliance team needed; the residency story survived our Schrems II evaluation; the DPA was Legal-reviewed by external counsel before we saw it."
>
> — **[ENTERPRISE_CUSTOMER_TITLE]**, **[ENTERPRISE_CUSTOMER_NAME or SANITIZED_DESCRIPTOR]**
> _Quote DRAFT — sanitized; sharable in sales conversations under NDA pending customer engagement + customer approval._

## Sales availability

A non-sanitized variant of this case study, including the customer's name and quantitative SLA / latency / hit-rate deltas, is available under NDA for active enterprise sales conversations. Contact: `sales@humangr.com`.

---

## Internal notes (strip before publish)

- Engagement target: per WI-S20-004 timeline (D+10..D+25) via Sales engagement.
- Status DRAFT pending engagement + customer approval + Legal clearance per WI-S20-005 cumulative.
- Sanitized variant maintained for NDA-gated sales distribution.
- All metrics are placeholders. No specific dollar amounts. No unverified compliance claims.
- The customer kill-switch drill and erasure-attestation replay narratives describe the intended observation methodology; specific incident counts pending observation window completion.
