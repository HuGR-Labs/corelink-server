# LinkedIn — CEO Launch Post

> **DRAFT — pending CEO sign-off.**
> Trace: WI-S20-008 §2.1.4 · launch runbook T-0 09:00 PT slot.
> Voice: founder, technical-credible, no breathlessness.

---

Today is the day CoreLink, HuGR Labs' first commercial product, reaches General Availability.

For background: CoreLink is a multi-tenant, content-addressable remote cache built on Cloudflare's edge platform, designed as a drop-in for Bazel and other REAPI-over-HTTP workloads. There is no gRPC ingress, so gRPC-only clients (Buck2, Pants, NativeLink) cannot connect today. It is, in 2026, the cache for teams that have outgrown the "single-tenant SaaS or self-host" dichotomy.

Some technical decisions I am proud of, in the order I am proud of them:

**1. We split GA into two gates.** An engineering gate that is binary and unappealable — PRR globally approved, 30 days of sustained staging, external lighthouse evidence, SOC 2 gap analysis delivered, zero active waivers in CRITICAL controls, and `CAP-GA-002` external report plus retest with no outstanding HIGH/CRITICAL findings. The external customer and pentest gates are unmet, so this draft does not authorize a GA claim or imply customer adoption. And a launch orchestration gate — this post, the press release, the blog series — which is soft and could shift its date without affecting the engineering decision. The engineering gate gates the launch. Not the other way around.

**2. Tenant isolation is a TLA+ invariant.** We maintain four formal specifications in CI. CI fails if the safety property regresses. We did this because the difference between marketing-language multi-tenant and actually-multi-tenant is exactly the kind of gap formal verification was invented to close.

**3. BYOK is real.** AWS KMS at GA — GCP, Azure, and HashiCorp Vault on the roadmap. Customer-managed kill switch. Verifiable crypto-erasure (a replayable Ed25519-signed attestation is on the roadmap). The DEK cache is hard-capped at five minutes by code path, not configuration. The vendor cannot read your bytes unilaterally. We took the procurement-team question seriously.

**4. The audit chain is primary, not side-effect.** An append-only, tamper-evident hash chain — each event linked to the previous one via `BLAKE3(prev || event)`. Customers re-derive the chain head from their own copy of the events. Trust the math, not the vendor.

**5. No external lighthouse customer attestations exist yet.** The
Enterprise/BYOK slot remains unpopulated: no external deployment, SLA
observation, or customer evidence is available for circulation. The customer
engagement and approval gates remain in force.

To the engineering team at HuGR Labs and the internal Forge team that helped
exercise the product — thank you. To external counsel (Cooley / DLA Piper /
Bird & Bird) for the DPA review, to the internal red-team reviewers for
finding the things we wanted them to find, to the on-call rotation for the
synthetic page exercises that nobody enjoys — thank you.

CoreLink is at `humangr.com/corelink`. Trust center is at `corelink-docs.humangr.com/trust`. Blog series (five posts, technical deep-dives) is at `corelink-docs.humangr.com/blog`.

If your build cache is the bottleneck in your inner loop, I would like to hear from you.

— Gustavo
Founder, HuGR Labs

#CoreLink #DevInfra #Bazel #RemoteCache #FormalVerification #BYOK

---

## Internal notes

- Length: ~400 words. LinkedIn allows up to 3,000 chars; this is well within.
- No specific dollar amounts. No competitor handles. All claims trace to canonical sources.
- Hashtag set kept short to avoid spam-signal.
