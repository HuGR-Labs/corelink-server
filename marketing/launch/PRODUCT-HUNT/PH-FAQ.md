# Product Hunt — Anticipated Q&A

> **DRAFT — pending Marketing + Engineering + Compliance review.**
> Trace: WI-S20-008 §2.1.4 · used by Maker for comment-tree response prep.
> Format: question + 2-sentence answer + canonical-source pointer.

---

## General

**Q: How is CoreLink different from `[existing remote cache]`?**
A: CoreLink is multi-tenant with TLA+-verified tenant isolation, BYOK across four KMS providers with customer-managed kill switch, and four enumerated regions with a structural no-cross-region-leak invariant. The relevant differentiators are listed in the launch post; we will not characterize competitors directly.
Source: `corelink.dev/blog/01-introducing-corelink`.

**Q: Is CoreLink open source?**
A: CoreLink is a managed service at GA. Apache 2.0 release is anti-scope for GA and a post-GA decision per the spec contract; the verification toolkit for the audit chain is planned for an open-source release.
Source: spec contract S-20 §10.

**Q: What does GA mean for CoreLink?**
A: GA means seven engineering work items sealed across S-20, PRR globally approved with thirteen canonical sign-offs, external pentest clean with retest, 30 days of sustained staging, three lighthouse customers with SLA met, SOC 2 gap analysis delivered, and zero active waivers in CRITICAL controls.
Source: spec contract S-20 §6.1.

## Technical

**Q: Is CoreLink REAPI-conformant?**
A: Yes. CoreLink is conformant with the Remote Execution API family (CAS + AC), which means existing Bazel / Buck2 configurations work with a `--remote_cache` URL change.
Source: REMOTE-CACHE-PRODUCT-PROFILE.

**Q: What hash algorithm does CoreLink use for content addressing?**
A: BLAKE3 where the protocol permits; SHA-256 for REAPI compatibility. The audit chain layer pins SHA-256 per RFC 6962.
Source: `corelink.dev/blog/03-audit-chain-merkle-proofs`.

**Q: How does CoreLink handle build-without-the-bytes?**
A: REAPI semantics are honored: clients that opt into build-without-the-bytes receive directory and action digests without the full blob download, with CoreLink retrieving blobs on demand.
Source: REMOTE-CACHE-PRODUCT-PROFILE.

**Q: What about cache poisoning?**
A: Content-addressed storage makes cache poisoning structurally hard: an attacker would need to produce a collision on the address, not just an alteration of the content. We further enforce tenant isolation, audit chain append-only semantics, and client-side digest verification.
Source: INV-CAS-INTEGRITY + REMOTE-CACHE-PRODUCT-PROFILE.

**Q: Can I run CoreLink on-prem?**
A: Not at GA. CoreLink GA is the managed service. Self-hosted / on-prem variants are not in scope at GA.
Source: spec contract S-20 §10.

## Security & compliance

**Q: Do you have a SOC 2 report?**
A: SOC 2 gap analysis is delivered pre-GA via Drata or Vanta tooling; Type I engagement is scheduled six months post-GA. The gap analysis with concrete GAP-XX items and a fix timeline is part of the GA evidence package.
Source: spec contract S-20 §5.1 R-S20-3.

**Q: What's the pentest situation?**
A: External pentest is engaged with one of Schellman, A-LIGN, or Trail of Bits, with a two-week test plus one-week retest. GA requires zero HIGH/CRITICAL findings pending. A summary letter is available under NDA.
Source: spec contract S-20 §5.1 R-S20-2 + CAP-GA-002.

**Q: How does BYOK work?**
A: Envelope encryption. Customer holds the KEK in their KMS (AWS / GCP / Azure / Vault). CoreLink wraps per-object DEKs under the customer's KEK. Bounded DEK cache (5-minute hard cap). Customer disables KEK → CoreLink hard-fails the data path → kill switch enforced cryptographically, not by policy.
Source: `corelink.dev/blog/02-byok-deep-dive` + INV-BYOK-CRYPTO-SOVEREIGNTY.

**Q: What regions do you support?**
A: Four regions at GA: WNAM, ENAM, WEUR, SAM. APAC is anti-scope for GA, planned post-GA Q1 demand-driven.
Source: spec contract S-20 §10 + INV-DATA-RESIDENCY.

**Q: Is data ever moved across regions?**
A: No. Cross-region replication of customer data is structurally not permitted (`INV-REGION-NO-CROSS-LEAK`). The invariant is part of the TLA+ tenant isolation spec.
Source: `corelink.dev/blog/04-multi-region-residency`.

**Q: How does erasure work?**
A: NIST SP 800-88 Rev. 1 crypto-erase semantics. Erasure produces an Ed25519-signed attestation the customer can replay against CoreLink's published signing key. 7-year attestation retention.
Source: INV-ERASURE-ATTESTATION-SIGNED + INV-DATA-ERASURE-COMPLETE.

## Pricing

**Q: How much does CoreLink cost?**
A: CoreLink ships with Free, Team, and Enterprise tiers. Specific list prices are published at `corelink.dev/pricing`. We don't quote pricing in PH comments.
Source: `corelink.dev/pricing`.

**Q: Is there a Free tier?**
A: Yes. Tier details and limits at `corelink.dev/pricing`.
Source: `corelink.dev/pricing`.

## Roadmap

**Q: When does Remote Execution ship?**
A: Phase 2 (Remote Execution — executor identity, sandboxed action execution, execution attestation) opens in the next sprint cycle post-GA. We are not pre-committing a date in this thread.
Source: spec contract S-20 §10.

**Q: When does APAC ship?**
A: Post-GA Q1, demand-driven. We will not announce APAC until we can ship it with the same operational coverage as the existing four regions.
Source: spec contract S-20 §10 + `corelink.dev/blog/04-multi-region-residency`.

**Q: What about SOC 2 Type II?**
A: Type I engagement six months post-GA; Type II is the follow-on. We are not pre-committing a Type II date in this thread.
Source: spec contract S-20 §5.1 R-S20-3.

## Trust posture

**Q: Can I see your TLA+ specs?**
A: Yes. Published at `corelink.dev/trust/formal-verification`.

**Q: Can I see your SBOM?**
A: Yes. CycloneDX 1.5+, signed, published. `corelink.dev/trust/sbom`.

**Q: Can I see your DPA?**
A: Yes. `legal.corelink.dev/dpa`. The DPA package was Legal-reviewed by external EU privacy counsel before any lighthouse customer signed.

---

## Internal notes

- Update with audience-actual questions during the first hours of the launch.
- Maker should bookmark canonical-source links for fast in-thread reference.
