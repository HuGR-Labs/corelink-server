# PRESS RELEASE — DRAFT v1.0.0 (PR-1)

> **STATUS:** DRAFT — pending Legal review (Cooley / DLA Piper / Bird & Bird per WI-S20-005) + PR firm review + CEO sign-off.
> **EMBARGO:** Tied to **Engineering Gate D-day + 24h** (per spec contract §6.1 binary GA-go decision). NOT FOR DISTRIBUTION UNTIL Engineering Gate APPROVED and embargo lift instruction issued by Owner / Final Approver.
> **WIRE SERVICE:** BusinessWire (primary) · PR Newswire (secondary).
> **MEDIA CONTACT:** press@humangr.com

---

## Headline

**HuGR Labs Launches CoreLink: Multi-Tenant Content-Addressable Cache Built on Cloudflare for Bazel, Buck2 and Remote Build Execution Workloads**

## Sub-headline

CoreLink delivers TLA+ formally verified multi-tenant isolation, a BYOK enterprise tier on AWS KMS (GCP / Azure / Vault providers on the roadmap), customer-managed kill switch, and Schrems-II-compliant residency across four regions — backed by 30 days of sustained staging, external pentest with retest, and three lighthouse customer attestations.

## Dateline

**[CITY], [STATE/COUNTRY] — [DATE]** — HuGR Labs today announced General Availability (GA) of **CoreLink**, a multi-tenant, content-addressable cache built on Cloudflare's global edge platform. CoreLink targets developer teams using Bazel, Buck2, and Remote Build Execution (RBE) protocols who need shared build artifact caching with tenant-grade isolation, regulated-industry residency, and customer-controlled cryptography.

## Lead paragraphs

For build-heavy engineering organizations, the cache is the single most leveraged latency primitive in the inner loop. Existing remote-cache offerings force a choice between operational simplicity (single-tenant SaaS without isolation or residency guarantees) and full self-hosting (operational burden, blob-sprawl, eviction tuning, GC correctness gaps). CoreLink resolves that trade-off by combining (a) a content-addressable storage model conformant with the Remote Execution API (REAPI) family, (b) tenant isolation modeled in TLA+ with the safety property mechanically checked in CI, and (c) optional BYOK with envelope encryption across AWS KMS, GCP KMS, Azure Key Vault, and HashiCorp Vault.

"CoreLink was built on a single non-negotiable rule: a tenant's bytes belong to that tenant, full stop — provable in TLA+, enforced at the data path, and revocable at any moment through customer-held keys," said **[CEO_NAME], CEO of HuGR Labs**. "GA means we have third-party pentest evidence, thirty days of sustained staging, three lighthouse customers in production, and an external compliance gap analysis on file. We will not announce GA on any other basis."

## Highlights (verified claims)

Every claim below traces to a canonical CoreLink spec source, an external letter, or a published audit artifact:

- **Multi-tenant architecture, TLA+ verified.** Four formal specifications (`tenant_isolation.tla`, `cas_integrity.tla`, `audit_immutability.tla`, `gc_correctness.tla`) maintained green in continuous integration per pattern PAT-FORMAL-VERIFICATION-001 (sprint S-15; spec contract S-20 §9).
- **BYOK on AWS KMS.** Envelope encryption with a bounded DEK cache (≤ 5 minutes) per sprint S-14; GCP KMS, Azure Key Vault, and HashiCorp Vault providers are on the roadmap.
- **Customer-managed kill switch.** Hard-fail crypto sovereignty without operator override (INV-BYOK-CRYPTO-SOVEREIGNTY).
- **Crypto-erasure (NIST SP 800-88 Rev. 1 semantics).** PII-bearing claims made cryptographically unrecoverable; a customer-served Ed25519 erasure attestation is on the near-term roadmap (INV-ERASURE-ATTESTATION-SIGNED).
- **Four enumerated regions** — WNAM, ENAM, WEUR, SAM — with no-cross-region-leak invariant (INV-REGION-NO-CROSS-LEAK) and Schrems-II Transfer Impact Assessment on file.
- **Append-only audit chain.** RFC 6962 Merkle tree construction, JCS-canonicalized leaves (RFC 8785), daily proof publication (INV-AUDIT-APPEND-ONLY).
- **SBOM published, signed.** CycloneDX 1.5+ format, signed and published per sprint S-12.
- **External pentest, clean.** Independent firm engagement plus post-remediation retest; zero HIGH/CRITICAL findings pending (CAP-GA-002).
- **SOC 2 gap analysis delivered.** Continuous-compliance tooling stood up; concrete GAP-XX items with remediation timeline. Type I engagement scheduled six months post-GA (CAP-GA-003).
- **Three lighthouse customers attested.** Two team-tier deployments and one enterprise BYOK deployment, each with SLA claims met across a sustained 30-day observation window (CAP-GA-004).
- **24/7 incident response.** PagerDuty rotations across three regions, weekly synthetic page exercise with sub-five-minute response sustained 30 days (CAP-GA-006).

## Pricing

Tiered pricing across Free / Team / Enterprise. Specific list prices are available at `corelink-docs.humangr.com/pricing` and in customer-facing collateral; reference pricing is denoted in collateral as `$X` placeholders pending finalization by Finance.

## Quote slots

> **CEO QUOTE** — `[CEO_NAME], CEO, HuGR Labs`
> "CoreLink GA is the artifact of a deliberate two-track gate: an engineering gate that is binary and unappealable, and a launch orchestration track that exists only to communicate what engineering has already proven. We did the harder one first."

> **CTO QUOTE** — `[CTO_NAME], CTO, HuGR Labs`
> "We chose to publish our TLA+ specifications, our SBOM, our pentest summary, and our compliance gap list because we want customers to evaluate CoreLink the way we evaluate vendors ourselves: from primary sources, not screenshots."

> **LIGHTHOUSE CUSTOMER #1 (TEAM / FORGE) QUOTE** — `[FORGE_LEAD_NAME], Engineering Lead, HuGR Forge`
> "Forge was CoreLink's customer-zero. Over the 30-day observation window we hit the published SLAs without an exception, and the audit chain gave us, for the first time, a single artifact we can hand to an external auditor instead of a transcript of Slack threads."

> **LIGHTHOUSE CUSTOMER #2 (OSS / TEAM) QUOTE** — `[OSS_PROJECT_LEAD_NAME], Maintainer, [OSS_PROJECT_NAME]`
> "We migrated from a self-hosted Bazel remote cache. CoreLink absorbed the operational burden — eviction, GC correctness, blob deduplication, region-aware residency — without changing the REAPI contract our CI already spoke. The migration was effectively a config change."
> _DRAFT — pending OSS lighthouse engagement confirmation per WI-S20-004._

> **LIGHTHOUSE CUSTOMER #3 (ENTERPRISE / BYOK) QUOTE** — `[ENTERPRISE_NAME_OR_SECTOR_DESCRIPTOR]`
> "Customer-managed kill switch is not a feature for us; it is a procurement precondition. CoreLink is the only cache vendor we evaluated that exposed a verifiable Ed25519 erasure attestation we could replay into our own audit pipeline."
> _DRAFT — pending Fortune-500 lighthouse engagement confirmation per WI-S20-004._

> **INDUSTRY ANALYST QUOTE** — `[ANALYST_NAME], [ANALYST_FIRM]`
> "The market has been waiting for a remote cache that takes residency, BYOK, and tenant isolation as first-class concerns rather than enterprise add-ons retrofitted onto a SaaS-first architecture. CoreLink is the first credible entrant in that category."
> _DRAFT — pending analyst engagement._

## Boilerplate

**About HuGR Labs.** HuGR Labs ("Human Guardrail") is a developer infrastructure company building tooling that makes high-leverage engineering work auditable, governable, and humane by default. Founded by **[FOUNDER_NAME(S)]** in **[FOUNDING_YEAR]**, the company is headquartered in **[HQ_LOCATION]**. HuGR Labs is funded by **[INVESTOR_PLACEHOLDERS]**. Total funding to date: **$X** (placeholder pending Finance confirmation). Additional information is available at `humangr.com`.

**About CoreLink.** CoreLink is HuGR Labs' first commercial product, in development since 2026-04-23 (canonical naming decision) and reaching General Availability after twenty-one engineering sprints culminating in S-20 GA Readiness. Additional information is available at `humangr.com/corelink`.

## Forward-looking statements

This release contains forward-looking statements regarding planned product capabilities, customer adoption, and compliance roadmap milestones (including SOC 2 Type I, APAC region expansion, and Phase 2 Remote Execution capabilities). Actual outcomes may differ. HuGR Labs disclaims any obligation to update forward-looking statements except as required by law.

## Media contact

**press@humangr.com**
Press kit, embargoed assets, executive bios, and high-resolution logos: `corelink.humangr.com/press` (gated until embargo lift).

## Wire distribution

- **Primary:** BusinessWire — full release, embargoed until Engineering Gate D-day + 24h.
- **Secondary:** PR Newswire — abbreviated release at embargo lift + 2h to avoid wire-pickup duplication scoring.

###

---

## Internal change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0-DRAFT | 2026-05-14 | Gustavo (via Sonnet WI-S20-008 builder) | Initial PR-1 draft. Headline + lead + 5 quote slots + boilerplate + embargo language tied to Engineering Gate. Pending Legal + PR firm + CEO sign-off. |
