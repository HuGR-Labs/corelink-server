<!-- DRAFT — pending Legal + Marketing + CEO sign-off. Do not publish. -->

# Multi-Region Residency at CoreLink (Roadmap): Schrems II, LGPD, GDPR, and the International Transfer Question

> ## ⚠️ ROADMAP — NOT YET GENERALLY AVAILABLE
> **This post describes a capability CoreLink is *building*, not one that ships today.**
> At launch, CoreLink operates from a **single US (ENAM) region**: all customer data — CAS blobs, action-cache entries, metadata — is physically stored in the United States. The per-region architecture described below (EU/Frankfurt-Dublin, Brazil/São Paulo, APAC pins; cross-region failover; the residency-proof API; the `INV-REGION-NO-CROSS-LEAK` enforcement) is **future, demand-driven work** offered to enterprise customers on request as we provision jurisdiction-local R2 buckets and per-region endpoints. Cross-border transfers from the US-stored data are governed today by SCCs + supplementary measures in the DPA. Read everything below in the future tense.

> **DRAFT — pending Marketing + Legal + Privacy Officer sign-off.**
> Compliance deep-dive (forward-looking / roadmap).
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 4) · INV-DATA-RESIDENCY · INV-REGION-NO-CROSS-LEAK · INV-DATA-ERASURE-COMPLETE · sprint S-19 residency.

---

International data transfer is the area of compliance where good intentions collide with the actual text of the law. The Court of Justice of the European Union (CJEU) Schrems II decision invalidated the EU–US Privacy Shield in July 2020 and re-grounded international transfers in Standard Contractual Clauses plus a documented Transfer Impact Assessment. Brazil's LGPD made data residency an enforceable expectation in regulated sectors, especially after Article 33's transfer rules took on operational weight in CVM and BCB guidance for the financial sector. The GDPR continues to evolve case law around what "adequate protection" means in practice — the recent EU–US Data Privacy Framework adequacy decision is real but contested, and a prudent data controller continues to treat transfer impact as a real exercise, not a checkbox.

Customers shipping software in 2026 cannot treat "where the data lives" as a deployment-time afterthought.

CoreLink is being designed with residency as a first-class concern: a roadmap of enumerated regions, a structural no-cross-region-leak invariant, a Schrems II Transfer Impact Assessment, and DPA language reviewed by external Legal counsel. Today the product runs from a single US (ENAM) region; the per-region capability below is what we are building toward.

This post explains what we are building and what it will mean concretely.

## The legal landscape, briefly

Three regimes anchor the design.

**Schrems II + the EU–US Data Privacy Framework.** Following Schrems II (Case C-311/18), transfers of personal data from the EU to third countries require either an adequacy decision, Standard Contractual Clauses backed by a Transfer Impact Assessment, or a derogation. The EU–US Data Privacy Framework adequacy decision (adopted July 2023) provides a basis for transfers to certified US importers, but is the subject of an ongoing legal challenge ("Schrems III"). A controller relying solely on adequacy without a TIA is taking a position that may not survive the next CJEU cycle. The conservative posture — and the one CoreLink's structure supports — is to assume EU-origin personal data does not leave the EU unless the controller has a specific, documented reason for the contrary.

**LGPD Article 33.** Brazil's Lei Geral de Proteção de Dados, Article 33, enumerates the legal bases for international transfer of personal data: adequacy decision from the ANPD, controller-provided guarantees through contractual clauses, regulator authorization, or specific derogations. Sector regulators (notably CVM for capital markets and BCB for banking) have additionally constrained data residency for regulated entities; in practice, financial-sector customers in Brazil operate under a structural expectation that production data stays in-country.

**GDPR Article 32.** Beyond cross-border specifics, GDPR Article 32 requires "appropriate technical and organisational measures" to ensure a level of security appropriate to the risk. For a remote build cache holding source-derived artifacts that may contain personal data (build outputs, log fixtures, test data), this implies measurable controls on encryption at rest and in transit, access boundaries, audit, and erasure. Residency is one of those controls.

## Today: one region. The roadmap: four.

CoreLink ships today from a **single US region — ENAM (Eastern North America)**. All customer data lives there, in the United States.

The roadmap is a set of enumerated regions, each a complete, independent failure domain:

- **ENAM** — Eastern North America (the region we run today).
- **WNAM** — Western North America (roadmap).
- **WEUR** — Western Europe (roadmap).
- **SAM** — South America (roadmap).

The design intent: each region is a complete failure domain — storage, control plane, audit chain, BYOK unwrap endpoints. The audit chain in WEUR will not become a leaf in the audit chain of WNAM. A CAS blob written in SAM will not be replicated to ENAM as a latency optimization. A tenant who selects WEUR at provisioning time will get a CoreLink that, structurally, does not leave WEUR. That property is what we are building; it is not yet live.

APAC is planned after the initial multi-region set, demand-driven. We would rather ship each region cleanly — with the structural invariants actually enforced — than announce regions whose cross-region story is soft.

## Tenant-region pinning

The tenant's region binding is established at provisioning. It is stored in the tenant-and-residency control-plane database, and it is part of the enforcement context of every authenticated request. The binding is not a configuration knob exposed to runtime traffic; changing a tenant's region is an explicit administrative operation that emits its own audit-chain leaves, requires customer-side confirmation, and triggers a documented migration path with explicit DSR handling.

Concretely, in the multi-region design the residency contract is enforced at three layers (this is the target architecture; today there is one US region).

**Routing.** The tenant's region binding is resolved at the edge and used to route every authenticated request to the correct regional cluster. A request that reaches a region different from the tenant's binding is refused at the boundary — not load-balanced, not falling back, refused with a documented error class.

**Storage.** Region-local storage backends are physically distinct. Addressing is region-scoped. There is no global namespace from which a region could fetch another region's blob.

**Audit.** The audit chain is region-scoped. A cross-region replication of an audit leaf would itself be an auditable event in both regions, and the design does not permit it.

The structural property — `INV-REGION-NO-CROSS-LEAK` — is that data written under a tenant configured for region R is readable only through the data path in region R. It is a CRITICAL invariant tracked in the registry, and it is one of the structural properties the tenant-isolation TLA+ specification checks (deferred from S-11 to S-14 per ADR-S11-010, then sealed in the S-14 model).

## Failover within a region set

Customers occasionally ask whether the residency invariant means a regional outage takes their cache offline. The honest answer is: failover happens, but only within a residency-compatible set.

A WEUR tenant's storage is replicated across availability zones inside WEUR. A regional outage at the availability-zone level fails over to another availability zone inside the same residency boundary. An entire-region failure mode (which we treat as a recoverable but rare event) falls back to read-only mode from the secondary EU footprint we maintain within WEUR; it does not fail over to ENAM, because doing so would violate the residency contract the tenant signed up for.

Customers can opt into a multi-region active configuration explicitly — for example, a tenant willing to span ENAM + WNAM for higher availability — but this is an explicit configuration choice that updates the residency contract and is reflected in the DPA. We do not perform implicit cross-region failover under any condition.

## DSR coordination across regions

A right-to-erasure (or any other Data Subject Request) is, naturally, a multi-region concern when a customer operates across regions. CoreLink coordinates DSR across regions through a few specific mechanisms.

The **consent ledger** — the record of what data subjects have authorized and what they have revoked — is itself region-scoped, with a controlled replication into each region the tenant operates in. Each replicated entry carries a residency tag that identifies the originating region. Erasure requests that span multiple regions produce per-region erasure operations, each producing its own Ed25519-signed attestation, which compose into a single tenant-facing DSR completion record.

Critically, erasure does not require cross-region read access. The erasure request for region R is executed by region R's control plane, using region R's keys. The orchestration that fans the request out is purely metadata — tenant identifier, scope, request identifier — and the cryptographic operations happen entirely inside each region.

`INV-DATA-ERASURE-COMPLETE` is the structural property that erasure terminates with bytes inaccessible (NIST SP 800-88 Rev. 1 crypto-erase via the BYOK kill path where applicable) and an attestation persisted, every time, in every region the scope covers. We model the property and test it.

## Three-locale legal review (EN / PT-BR / ES)

Customers operating across regions sign DPAs and SCC modules whose legal force depends on the locale they are executed in. CoreLink's DPA package was reviewed by external counsel in three locales before any lighthouse customer signed:

- **English (EU + UK)** — for customers contracting under EU GDPR or UK GDPR. SCC modules where applicable, controller-to-processor and controller-to-controller variants per configuration.
- **Portuguese (Brazil)** — for customers contracting under LGPD. Sector-specific addenda available for CVM- and BCB-regulated entities.
- **Spanish (LATAM)** — for customers in Spanish-speaking LATAM jurisdictions, with locale-specific privacy-law addenda.

Per WI-S20-005, external counsel (Cooley / DLA Piper / Bird & Bird) reviewed and warranted the EN package; Brazilian counsel reviewed the PT-BR package; LATAM counsel reviewed the ES package. Customers do not sign template language reviewed by the vendor's own marketing-adjacent counsel; they sign language an external firm warranted is fit for purpose in their locale.

## Schrems II: what we did with the Transfer Impact Assessment

Once WEUR ships, the TIA conclusion for EU customers using it will be straightforward: data does not leave the EU, so there is no transfer to assess. **That is the roadmap state, not today's.**

Today, all CoreLink data — including EU-origin data — is stored in the US (ENAM). For that transfer, CoreLink provides the SCC module language in the DPA, a customer-facing TIA template, and a documented evaluation of US surveillance law as it applies to a CoreLink operator (published at `docs.corelink.humangr.com/trust/schrems-ii`). Customers handling EU-origin personal data who require in-EU storage should treat WEUR as a roadmap commitment available to enterprise on request — not a capability they can select at launch.

## A customer story (placeholder)

`DRAFT — pending customer engagement per WI-S20-004 lighthouse confirmation.`

One of our lighthouse customers is an EU-headquartered software company shipping developer tooling under a procurement framework that requires explicit residency commitments and a Schrems II–era TIA. Their compliance officer's specific ask, in our first procurement conversation, was: "show me, in code, the path by which EU data could leave the EU; if there is no such path, give me the artifact that proves it." We pointed them at the TLA+ specification, the regional architecture diagram, and the consistency-proof verification toolkit for the audit chain. The procurement cycle closed faster than any of us expected, because the answer to the compliance officer's question was an artifact rather than a narrative.

The full lighthouse case study, with the customer's name and the executive's attestation, ships with the GA case-study bundle.

## What this is not

CoreLink residency is not a substitute for the customer's own legal evaluation of jurisdiction. It is a substrate that makes the customer's evaluation tractable.

CoreLink residency (once multi-region ships) is also not a guarantee against governmental compulsion of the operator in the region where data resides. It is designed to be a guarantee that data does not leak across regions through CoreLink's own systems. Compulsion against the operator is a separate threat surface, addressed in part by BYOK (the operator cannot decrypt unilaterally) and in part by transparency reporting (forthcoming, roadmap).

## Limits and roadmap

**APAC.** We get the question often: why no APAC at GA? The honest answer is that we wanted four regions where the structural invariants are enforced and the operational coverage is solid (24/7 on-call across the regions where we ship). Adding APAC at GA without that operational coverage would weaken the invariant. We will ship APAC when we can ship it without that compromise.

**Custom geo-fencing.** Some customers ask for sub-regional fencing — for example, "data must reside in Germany specifically, not WEUR generally." We do not offer this at GA. The structural argument is that adding sub-regional fencing without a credible operational story for failure modes inside that fence is a residency story that breaks under load. We are scoping a German-data-residency option post-GA based on customer demand, and the engineering decision will be the same one we made for the GA region set: ship it when the invariants are real, not when the marketing copy is.

**Transparency reporting.** Aggregate reporting of governmental data requests, structured against the framework most large vendors now publish, is on the post-GA roadmap. The current shape of CoreLink (operator cannot decrypt under BYOK; audit chain is verifiable independently) constrains what we *could* be compelled to produce; transparency reporting documents what we *have been* asked for.

## Where to go next

- **Trust center:** `corelink.humangr.com/trust`
- **Residency invariant:** `docs.corelink.humangr.com/trust/residency`
- **Schrems II TIA template:** `docs.corelink.humangr.com/trust/schrems-ii`
- **DPA package (EN / PT-BR / ES):** `legal.corelink.humangr.com/dpa`

— Privacy and Trust at CoreLink

---

## Internal notes (strip before publish)

- Target length: 1,800–2,200 words.
- Pending review: Privacy Officer + Legal Counsel (canonical sign-off slots 10 + 12 in WI-S20-008 §16).
- All compliance claims trace to spec contract §5.2 + INV-DATA-RESIDENCY + INV-REGION-NO-CROSS-LEAK + INV-DATA-ERASURE-COMPLETE.
- Customer story is placeholder pending WI-S20-004 lighthouse engagement.
- No specific dollar amounts. No unverified compliance claims (all trace to canonical sources or external Legal review path WI-S20-005).
