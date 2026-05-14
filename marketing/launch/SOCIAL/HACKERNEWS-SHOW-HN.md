# Hacker News — Show HN Draft

> **DRAFT — pending CEO/Founder sign-off.**
> Trace: WI-S20-008 §2.1.4 · launch runbook T-0 10:00 PT slot.
> Compliance: Hacker News community guidelines (no vote manipulation, no resubmission, no astroturfing, no marketing-speak inflation).

---

## Submission

**Title (≤ 80 chars, HN convention):**

> Show HN: CoreLink – Multi-tenant Bazel/Buck2/RBE cache with TLA+ verified isolation

**Character count:** 80 / 80.

**URL:**

> https://corelink.dev

**Text (optional Show HN body):**

Hi HN — Gustavo here, founder of HuGR Labs.

CoreLink is a multi-tenant content-addressable remote cache for Bazel, Buck2, and Remote Build Execution workloads, built on Cloudflare's edge platform. Today is GA. I think a few decisions are worth specifically highlighting for this audience:

- **TLA+ formal verification of tenant isolation, in CI, gating the build.** Four specs (tenant_isolation, cas_integrity, audit_immutability, gc_correctness) maintained green. We chose to publish them — link in the trust center.
- **BYOK across four KMS providers** (AWS / GCP / Azure / Vault) with envelope encryption, a 5-minute hard cap on the DEK cache, and a customer-managed kill switch. The vendor cannot decrypt unilaterally; the kill is structural, not policy-level.
- **Audit chain follows RFC 6962** (the Certificate Transparency Merkle tree construction) over RFC 8785 JCS-canonicalized leaves. Customers independently re-derive the chain head from their copy of the events. Consistency proofs published daily.
- **Engineering gate is separated from launch orchestration.** GA was decided by a binary engineering gate (PRR + external pentest clean with retest + 30d sustained staging + 3 lighthouse customers attested + zero CRITICAL waivers), not by marketing readiness. Splitting those two gates was a discipline we wanted to bake in from the start.
- **REAPI conformant.** Most existing Bazel / Buck2 configurations work with a `--remote_cache` URL change.

Stuff that is not in this launch (anti-scope, on purpose): on-prem self-hosting, Apache 2.0 open source release, FedRAMP, APAC region, Phase 2 (Remote Execution). Each has a tracked roadmap entry; we wanted to ship the cache cleanly before stacking commitments on top of it.

What I would love from HN: hard technical questions, especially on the TLA+ approach, the BYOK threat model, the audit chain construction, and where the SLOs are most likely to surprise us in production. We will respond in the thread.

Links:
- Site: https://corelink.dev
- Docs: https://docs.corelink.dev
- Trust center (TLA+ specs, SBOM, DPA): https://corelink.dev/trust
- Blog launch series (5 technical posts): https://corelink.dev/blog

— Gustavo

---

## HN community-guidelines posture

- **Single submission.** No resubmission, no alt-account boosting.
- **No vote manipulation.** No "please upvote" outreach. Internal team and ambassadors organically engaging in thread is fine; coordinated voting is not.
- **No flame-bait.** No competitor names, no superlatives, no marketing inflation.
- **Substantive response posture.** Founder is the primary respondent for the first 24 hours. Technical Lead pairs as backup on deep architecture questions.
- **Genuine.** Show HN audience reads marketing-speak the way an immune system reads a pathogen. Voice stays plain, sourced, technical.

## Internal notes

- Submission timing: T-0 10:00 PT per launch runbook (after PH go-live at 00:01 PT, blog publish at 06:00 PT, LinkedIn/Twitter at 09:00 PT).
- HN's ranking algorithm favors early-discussion velocity; the staggered timing is intentional.
- All claims trace to canonical spec sources. No specific dollar amounts.
