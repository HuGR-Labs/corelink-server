# Product Hunt — Opening Maker Comment

> **DRAFT — pending CEO/Founder Maker sign-off.**
> Trace: WI-S20-008 §2.1.4 · Product Hunt convention (Maker opens the comment thread).
> Post timing: **T-0 + 5 minutes** per `PH-LAUNCH-PLAN.md`.

---

Hi Product Hunt — Gustavo here, founder of HuGR Labs and Maker of CoreLink.

Quick context on what we're shipping and why we think it matters.

**What CoreLink is.** A multi-tenant, content-addressable remote cache built on Cloudflare's edge, designed as a drop-in for Bazel, Buck2, and Remote Build Execution workloads. We are conformant with the REAPI family, which means most existing Bazel / Buck2 configurations work with a `--remote_cache` URL change.

**What's actually different.** Three things, in order of how much we agonized over them:

1. **Tenant isolation is a TLA+ invariant.** We maintain four formal specifications in CI and the build fails if the safety property regresses. Most "multi-tenant" caches are single-tenant SaaS with namespacing; this one is structurally different.
2. **BYOK is real on AWS KMS** (GCP KMS, Azure Key Vault, and HashiCorp Vault are on the roadmap). Customer-managed kill switch. Verifiable crypto-erasure (a customer-served Ed25519 attestation is on the roadmap). The vendor cannot read your bytes unilaterally — that's the property, not the marketing.
3. **Engineering gate separated from launch.** We split GA into a binary engineering gate (PRR + pentest + 30d staging + 3 lighthouse customers attested) and a soft-gate launch orchestration (this post, the press release, the blog series). The engineering gate is binary, unappealable, and gated the launch — not the other way around.

**Who it's for.** Build-heavy engineering teams running Bazel or Buck2 at scale, particularly teams with residency, BYOK, or audit requirements that existing remote caches paper over.

**What we'd love from you.** Honest technical questions. We'll answer in this thread. If you're a Bazel / Buck2 / RBE practitioner and CoreLink intersects something you actually do, we want the rough-edge feedback.

**Links.**

- Site: humangr.com/corelink
- Docs: corelink-docs.humangr.com
- Trust center: corelink-docs.humangr.com/trust
- Blog series (launch day): corelink-docs.humangr.com/blog
- TLA+ specs: corelink.humangr.com/trust/formal-verification

I'll be here through the launch day responding. Thank you to **[HUNTER_NAME]** for the hunt, and to the engineers — both inside HuGR and at our three lighthouse customers — who got this to a place where we could ship it without flinching.

— Gustavo

---

## Internal notes

- Length: ~330 words. Slightly long for PH conventions; consider trimming list items if first-hour engagement suggests TL;DR.
- Hunter name slot filled at T-1d post hunter confirmation.
- No competitor names. No specific dollar amounts. All claims traceable.
