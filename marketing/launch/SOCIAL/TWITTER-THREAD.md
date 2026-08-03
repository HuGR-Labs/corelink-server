# Twitter / X — Launch Thread

> **DRAFT — pending Marketing + CEO sign-off.**
> Trace: WI-S20-008 §2.1.4 · launch runbook T-0 09:00 PT slot.
> Format: 10-tweet thread + 5 alt-text image briefs.

---

## Tweet 1 — Hook

> Today we're announcing GA of CoreLink — a multi-tenant content-addressable remote cache for Bazel, Buck2, and RBE, built on Cloudflare's edge.
>
> Why now, and what we did differently. A thread. 🧵
>
> humangr.com/corelink

`[image_1: hero shot — CoreLink wordmark + tagline; 1600x900]`

## Tweet 2 — The problem

> Remote caches force a choice: simple SaaS without isolation/residency, or self-hosted with the operational tail (eviction, GC, dedup, blob sprawl, regions).
>
> Most teams pay both costs. We built CoreLink to make the choice obsolete.

## Tweet 3 — Tenant isolation as a TLA+ invariant

> Multi-tenant isolation is the place where "marketing-language multi-tenant" and "actually multi-tenant" diverge.
>
> CoreLink keeps 4 TLA+ specs green in CI: tenant_isolation, cas_integrity, audit_immutability, gc_correctness.
>
> CI fails if the property regresses.

`[image_2: TLA+ spec snippet — tenant_isolation invariant; 1200x800 monospace]`

## Tweet 4 — BYOK that doesn't lie

> BYOK is over-claimed. Strong BYOK means the vendor cannot read your bytes unilaterally.
>
> CoreLink: envelope encryption across AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault. DEK cache hard-capped at 5 min. Customer KEK off → CoreLink hard-fails the data path.
>
> That's the property.

`[image_3: BYOK envelope-encryption flow diagram — KEK at customer, DEK at CoreLink, kill switch arrow; 1600x900]`

## Tweet 5 — Erasure that produces an artifact

> "Show me proof you deleted my customer's data" should be answered with an artifact.
>
> CoreLink erasure: NIST SP 800-88 Rev. 1 crypto-erase + Ed25519 signed attestation, replayable against our published signing key. 7-year retention.

## Tweet 6 — Audit chain as primary, not side-effect

> Audit chain on CoreLink:
>
> - RFC 6962 Merkle construction
> - RFC 8785 JCS canonical leaves
> - Daily proof publication
> - Inclusion + consistency proofs on request
>
> Customers re-derive the chain head from their copy of the events. Trust the math, not us.

`[image_4: Merkle tree visualization — chain with consistency proof highlighted; 1600x900]`

## Tweet 7 — Four regions, no cross-leak

> 4 regions at GA: WNAM, ENAM, WEUR, SAM.
>
> "No cross-region leak" is an invariant (`INV-REGION-NO-CROSS-LEAK`), verified in the TLA+ tenant isolation spec.
>
> Schrems II TIA on file. DPA Legal-reviewed by external EU privacy counsel before any lighthouse customer signed.

## Tweet 8 — Engineering gate separated from launch

> We split GA into two gates:
>
> 1. Engineering gate (binary, unappealable): PRR + pentest + 30d staging + 3 lighthouse customers + zero CRITICAL waivers.
> 2. Launch orchestration (soft gate): this thread.
>
> Engineering gate decided whether we ship. Not the other way around.

## Tweet 9 — Three lighthouse customers

> Three lighthouse customers attested before GA: Forge (customer-zero, internal), one OSS Bazel/Buck2 maintainer team, one enterprise BYOK deployment.
>
> Each ran 30 days under SLA observation with claims met every day.

## Tweet 10 — Close + CTA

> CoreLink is GA today.
>
> - Site: humangr.com/corelink
> - Docs: corelink-docs.humangr.com
> - Trust center: corelink-docs.humangr.com/trust
> - Blog (launch series, 5 posts): corelink-docs.humangr.com/blog
>
> If your build cache is the bottleneck — we'd like to hear from you.

`[image_5: closing card — "CoreLink GA" + humangr.com/corelink URL; 1600x900]`

---

## Image alt-text briefs

| # | Subject | Alt text (≤ 200 chars) |
|---|---|---|
| 1 | Hero | "CoreLink wordmark with tagline 'Multi-tenant Bazel cache, TLA+ verified, BYOK-ready' on dark background." |
| 2 | TLA+ snippet | "Code excerpt from tenant_isolation.tla showing the safety invariant that cross-tenant CAS reads are structurally impossible." |
| 3 | BYOK flow | "Diagram showing customer-held KEK in customer KMS, per-object DEK at CoreLink wrapped under KEK, and customer-initiated kill switch arrow." |
| 4 | Merkle tree | "Merkle tree diagram with daily chain heads highlighted and a consistency proof path between two heads marked." |
| 5 | Closing | "CoreLink GA announcement card with humangr.com/corelink URL and HuGR Labs logo." |

---

## Internal notes

- All claims trace to canonical spec sources. No competitor handles tagged. No specific dollar amounts.
- Thread posts at T-0 09:00 PT per launch runbook.
- Re-post amplification by HuGR team within 1 hour; ambassador outreach (Bazel/Buck2/RBE ecosystem) queued for organic boost.
