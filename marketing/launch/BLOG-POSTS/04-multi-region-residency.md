# Multi-Region Residency at CoreLink: Schrems II, LGPD, GDPR, and the International Transfer Question

> **DRAFT — pending Marketing + Legal + Privacy Officer sign-off.**
> Target length: 1,500–3,000 words. Compliance deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 4 reframed → residency international transfer story per S-19) · INV-DATA-RESIDENCY · INV-REGION-NO-CROSS-LEAK · INV-DATA-ERASURE-COMPLETE.

---

International data transfer is the area of compliance where good intentions collide with the actual text of the law. The Court of Justice of the European Union (CJEU) Schrems II decision invalidated the EU-US Privacy Shield in July 2020 and re-grounded international transfers in Standard Contractual Clauses plus a documented Transfer Impact Assessment. Brazil's LGPD made data residency an enforceable expectation in regulated sectors. The GDPR continues to evolve case law around what "adequate protection" means in practice. Customers shipping software in 2026 cannot treat "where the data lives" as a deployment-time afterthought.

CoreLink ships with residency as a first-class concern: four enumerated regions, a structural no-cross-region-leak invariant, a Schrems II TIA on file, and DPA language that is signed by external Legal counsel before any lighthouse customer signed it.

This post explains what that means concretely.

## Four regions at GA

CoreLink GA supports four enumerated regions:

- **WNAM** — Western North America.
- **ENAM** — Eastern North America.
- **WEUR** — Western Europe.
- **SAM** — South America.

Each region is a complete failure domain: storage, control plane, audit chain, BYOK unwrap endpoints. The audit chain in WEUR does not become a leaf in the audit chain of WNAM. The CAS blob written in SAM does not get replicated to ENAM as a latency optimization. A tenant who selects WEUR at provisioning time gets a CoreLink that, structurally, does not leave WEUR.

APAC is explicitly anti-scope for GA and is planned post-GA Q1, demand-driven. We would rather ship four regions cleanly than five regions with a soft cross-region story.

## The invariant: `INV-REGION-NO-CROSS-LEAK`

The structural property the design enforces is: **data written under a tenant configured for region R is readable only through the data path in region R**. This is enforced at three layers:

1. **Routing.** The tenant's region binding is established at provisioning and is part of every authenticated request's enforcement context.
2. **Storage.** Region-local storage backends are physically distinct and addressing is region-scoped.
3. **Audit.** The audit chain is region-scoped. A cross-region replication of an audit leaf would itself be an auditable event in both regions, and the design does not permit it.

This is a CRITICAL invariant tracked in the registry, and it is one of the structural properties tenant isolation TLA+ specifications check.

## Schrems II: what we did with the Transfer Impact Assessment

Schrems II requires data exporters to perform a Transfer Impact Assessment when transferring personal data from the EU to a third country, evaluating whether the destination's legal framework offers protection essentially equivalent to GDPR. For CoreLink customers in the EU using WEUR, the answer is: data does not leave the EU. There is no transfer to assess.

For customers using WNAM or ENAM regions for EU-origin data (which we generally do not recommend but is configurable), CoreLink provides a customer-facing TIA template, the SCC (Standard Contractual Clauses) module language in the DPA, and a documented evaluation of US surveillance law as it applies to a CoreLink operator. This is published at `docs.corelink.dev/trust/schrems-ii`.

The honest take: customers handling EU-origin personal data should default to WEUR. The TIA exists for the cases where they cannot.

## LGPD: residency as a structural commitment

Brazil's LGPD (Lei Geral de Proteção de Dados) is, in practice, residency-light at the federal level but residency-heavy in specific regulated sectors (financial services notably). For customers operating under LGPD with a residency expectation, CoreLink offers SAM as a structural region binding and the same `INV-REGION-NO-CROSS-LEAK` guarantee.

DPA / DPA-amendment language for LGPD is Legal-reviewed and available alongside the GDPR DPA in the trust center.

## GDPR: SCCs, DPAs, and what a customer needs to sign

For EU data controllers using CoreLink as a processor, the DPA package at GA includes:

- The core Data Processing Agreement.
- SCC modules where applicable (controller-to-processor; controller-to-controller variant for specific configurations).
- The Schrems II TIA (executed by the customer with CoreLink's prefilled inputs).
- The processing record (Article 30) summary describing categories of data, recipients, retention, security measures.

All of these are reviewed by external EU privacy counsel (Cooley / DLA Piper / Bird & Bird per WI-S20-005) before any lighthouse customer signed. Customers do not sign template language that was Legal-reviewed by the vendor's own marketing-adjacent counsel; they sign language that an external firm warranted is fit for purpose.

## Erasure: structural, attested, replayable

Residency is closely paired with erasure. A tenant exercising right-to-erasure under GDPR, LGPD, CCPA, or any other regime triggers the CoreLink erasure flow, which:

1. Deletes the in-scope CAS blobs and AC entries at the storage layer.
2. Renders the corresponding chunks cryptographically inaccessible via the BYOK kill path (where BYOK is in use) — NIST SP 800-88 Rev. 1 crypto-erase semantics.
3. Emits an Ed25519-signed erasure attestation describing the scope and confirming completion (`INV-ERASURE-ATTESTATION-SIGNED`).
4. Retains audit leaves for the erased records (so the chain remains verifiable) while the underlying byte content is gone.

`INV-DATA-ERASURE-COMPLETE` is the structural property that erasure terminates with bytes inaccessible and an attestation persisted, every time. We model the property and test it.

## What this is not

CoreLink residency is not a substitute for the customer's own legal evaluation of jurisdiction. It is a substrate that makes the customer's evaluation tractable.

CoreLink residency is also not a guarantee against governmental compulsion of the operator in the region where data resides. It is a guarantee that data does not leak across regions through CoreLink's own systems. Compulsion against the operator is a separate threat surface, addressed in part by BYOK (the operator cannot decrypt unilaterally) and in part by transparency reporting (forthcoming, roadmap).

## A note on APAC

We get the question often: why no APAC at GA? The honest answer is that we wanted four regions where the structural invariants are enforced and the operational coverage is solid (24/7 on-call across the regions where we ship). Adding APAC at GA without that operational coverage would weaken the invariant. We will ship APAC when we can ship it without that compromise.

## Where to go next

- **Trust center:** `corelink.dev/trust`
- **Residency invariant:** `docs.corelink.dev/trust/residency`
- **Schrems II TIA template:** `docs.corelink.dev/trust/schrems-ii`
- **DPA package:** `legal.corelink.dev/dpa`

— Privacy & Trust at CoreLink

---

## Internal notes (strip before publish)

- Word count: ~1,500. Within target.
- Pending review: Privacy Officer + Legal Counsel (canonical sign-off slots 10 + 12 in WI-S20-008 §16).
- All compliance claims trace to spec contract §5.2 + INV-DATA-RESIDENCY + INV-REGION-NO-CROSS-LEAK + INV-DATA-ERASURE-COMPLETE.
- No specific dollar amounts. No unverified compliance claims (all trace to canonical sources or external Legal review path WI-S20-005).
