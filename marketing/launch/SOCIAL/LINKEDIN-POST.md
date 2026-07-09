# LinkedIn — CEO Launch Post

> **DRAFT — pending CEO sign-off.**
> Trace: WI-S20-008 §2.1.4 · launch runbook T-0 09:00 PT slot.
> Voice: founder, technical-credible, no breathlessness.

---

Today is the day CoreLink, HuGR Labs' first commercial product, reaches General Availability.

For background: CoreLink is a multi-tenant, content-addressable remote cache built on Cloudflare's edge platform, designed as a drop-in for Bazel, Buck2, and Remote Build Execution workloads. It is, in 2026, the cache for teams that have outgrown the "single-tenant SaaS or self-host" dichotomy.

Some technical decisions I am proud of, in the order I am proud of them:

**1. We split GA into two gates.** An engineering gate that is binary and unappealable — PRR globally approved, external pentest clean with retest, 30 days of sustained staging, three lighthouse customers with SLA met, SOC 2 gap analysis delivered, zero active waivers in CRITICAL controls. And a launch orchestration gate — this post, the press release, the blog series — which is soft and could shift its date without affecting the engineering decision. The engineering gate gated the launch. Not the other way around.

**2. Tenant isolation is a TLA+ invariant.** We maintain four formal specifications in CI. CI fails if the safety property regresses. We did this because the difference between marketing-language multi-tenant and actually-multi-tenant is exactly the kind of gap formal verification was invented to close.

**3. BYOK is real.** AWS KMS at GA — GCP, Azure, and HashiCorp Vault on the roadmap. Customer-managed kill switch. Verifiable crypto-erasure (a replayable Ed25519-signed attestation is on the roadmap). The DEK cache is hard-capped at five minutes by code path, not configuration. The vendor cannot read your bytes unilaterally. We took the procurement-team question seriously.

**4. The audit chain is primary, not side-effect.** RFC 6962 Merkle construction over RFC 8785 JCS-canonicalized leaves. Customers re-derive the chain head from their own copy of the events. Trust the math, not the vendor.

**5. We named three lighthouse customers and held the gate against marketing pressure.** Two team-tier deployments and one enterprise BYOK deployment, each with SLA claims met across a sustained 30-day observation. None of those attestations were waived. None of the engineering criteria were waived.

To the engineering team at HuGR Labs and the engineering teams at the three lighthouse customers — thank you. To external counsel (Cooley / DLA Piper / Bird & Bird) for the DPA review, to the pentest firm for finding the things we wanted them to find, to the on-call rotation for the synthetic page exercises that nobody enjoys — thank you.

CoreLink is at `corelink.humangr.com`. Trust center is at `corelink.humangr.com/trust`. Blog series (five posts, technical deep-dives) is at `corelink.humangr.com/blog`.

If your build cache is the bottleneck in your inner loop, I would like to hear from you.

— Gustavo
Founder, HuGR Labs

#CoreLink #DevInfra #Bazel #Buck2 #RemoteCache #FormalVerification #BYOK

---

## Internal notes

- Length: ~400 words. LinkedIn allows up to 3,000 chars; this is well within.
- No specific dollar amounts. No competitor handles. All claims trace to canonical sources.
- Hashtag set kept short to avoid spam-signal.
