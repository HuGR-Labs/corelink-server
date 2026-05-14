# Introducing CoreLink

> **DRAFT — pending Marketing + Legal + CEO sign-off. Embargoed until Engineering Gate D-day.**
> Target length: 1,500–3,000 words. Style reference: Stripe "Introducing …" launch posts.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 · REMOTE-CACHE-PRODUCT-PROFILE.

---

If you have ever shipped a non-trivial software product on top of Bazel, Buck2, or Remote Build Execution, you have at some point opened a dashboard, watched your cache hit rate slip below the comfort threshold, and started thinking about all the operational work you did not want to do: blob lifecycle, dedup heuristics, eviction policy, GC correctness, residency rules, customer-managed keys, audit-grade isolation. The remote cache is the most-leveraged latency primitive in the developer inner loop, and yet — until today — every offering on the market has asked teams to choose between operational simplicity and the controls a regulated, multi-tenant business actually needs.

We built **CoreLink** to make that choice obsolete.

## What CoreLink is

CoreLink is a multi-tenant, content-addressable cache built on Cloudflare's global edge platform, designed to be a drop-in remote cache for Bazel, Buck2, and Remote Build Execution (REAPI-conformant) workloads. It is shipped, today, at General Availability after twenty-one engineering sprints, an external pentest with post-remediation retest, thirty days of sustained staging, and three lighthouse customer attestations.

CoreLink is opinionated in three places where the existing market is not:

1. **Tenant isolation is a TLA+ invariant, not a marketing word.** We maintain four formal specifications (`tenant_isolation.tla`, `cas_integrity.tla`, `audit_immutability.tla`, `gc_correctness.tla`) in continuous integration. CI fails if the safety property regresses. The specs are publishable artifacts.
2. **The audit chain is a primary artifact, not a logging side-effect.** Every state-changing operation lands in an append-only chain (RFC 6962 Merkle construction, RFC 8785 JCS-canonicalized leaves), with daily proof publication. Customers and their auditors can independently re-derive the chain head from raw events.
3. **BYOK is real.** Across four KMS providers — AWS KMS, GCP KMS, Azure Key Vault, and HashiCorp Vault — customers hold the keys, customers wield the kill switch, and erasure produces a signed Ed25519 attestation. The 5-minute hard cap on DEK cache is bounded by code path, not by configuration.

## Who CoreLink is for

CoreLink targets four overlapping audiences.

- **Build-heavy engineering teams** running Bazel or Buck2 at scale, particularly those whose CI is bottlenecked on download bandwidth rather than compute.
- **Regulated-industry developer infrastructure groups** that need residency, BYOK, and an auditable substrate for any caching layer they introduce into the SDLC.
- **OSS maintainers** of large polyglot monorepos who want the operational simplicity of a managed cache without surrendering control over residency or erasure.
- **Procurement teams** evaluating remote-cache vendors against modern compliance baselines: GDPR, Schrems II, LGPD, SOC 2, and the emerging crypto-sovereignty requirements that follow them.

## Why this is hard

Most existing remote caches are descended from one of two genealogies. The first is the single-tenant SaaS lineage: simple operationally, but with shared-fate failure modes and no real isolation story. The second is the self-hosted, single-binary lineage: complete control, but with the long tail of operational toil (eviction, GC, dedup, blob sprawl, replication) re-discovered by every team that adopts it.

The mixed lineage — multi-tenant, regulated, BYOK-capable, operationally managed, REAPI-conformant — has not previously existed because each of its requirements compounds the others. Multi-tenant means you cannot use single-tenant operational shortcuts. Regulated means residency must be enforced at the data path, not the configuration layer. BYOK means the operator cannot read its own customers' bytes. REAPI conformance means none of this can break the spec your customers' CI already speaks.

CoreLink's design treats each of those as a non-negotiable, and the product is what falls out of taking them seriously simultaneously.

## A short tour

**Content-addressable storage.** Blobs are keyed by their BLAKE3 (or SHA-256, for REAPI compatibility) digest. Deduplication is a property of the address, not a heuristic. Eviction is content-aware. GC correctness is formally verified.

**Action Cache.** Action results are recorded against a canonical-form action key, with TLA+-verified safety properties on the read/write path. Negative caching is bounded and explicit.

**Tenant isolation.** Every CAS read, every AC write, every audit append carries a verified tenant binding. Cross-tenant access is structurally impossible — and we wrote the structural proof in TLA+ rather than the assertion in Markdown.

**Audit chain.** Every operation lands in an append-only, Merkle-rooted audit log. Daily proofs are published. Customers re-derive the root from raw events as a routine integrity exercise.

**BYOK.** Four KMS providers. Envelope encryption with bounded DEK cache. Customer-managed kill switch. Signed Ed25519 erasure attestation.

**Residency.** Four enumerated regions: WNAM, ENAM, WEUR, SAM. No cross-region leak is a structural invariant.

**24/7 incident response.** Three on-call regions, weekly synthetic page exercise, sub-five-minute response sustained for thirty days as a precondition of GA.

## What "GA" means at CoreLink

We have, deliberately, two separate tracks at launch.

The first is the **engineering gate** — binary, unappealable, enforced by the CEO/Founder. It is the gate that decides whether CoreLink can be sold. It is `APPROVED` or it is not, and "APPROVED" requires all seven engineering work items in sprint S-20 sealed, the PRR globally approved across thirteen canonical sign-offs, external pentest clean with retest, thirty days of sustained staging, three lighthouse customers with SLA met, SOC 2 gap analysis delivered, on-call runbooks rehearsed, and zero active waivers in CRITICAL controls.

The second is the **launch orchestration** — the press release you are reading about, the blog posts, the Product Hunt launch, the social posts. It exists to communicate, in clear language, what the engineering gate has already proven. It does not gate the engineering decision, and it can shift its date without affecting engineering readiness. That separation is deliberate, documented in the spec contract, and enforced by the founder.

We mention it here because we think the customers we want to attract are customers who care about that separation.

## Pricing

CoreLink ships with Free, Team, and Enterprise tiers. Reference pricing lives at `corelink.dev/pricing`. Where placeholders appear in this and other launch documents, they reflect Finance's final approval cycle — not undecided product economics.

## What is next

Phase 2 — Remote Execution — opens in the next sprint cycle and brings executor identity, sandboxed action execution, and a verifiable execution attestation chain into the same trust model. SOC 2 Type I engagement begins six months post-GA. APAC region expansion follows post-GA based on customer demand. The Phase 2 roadmap is anti-scoped from GA and explicitly out of this launch — we want to ship the cache cleanly before we ship the executor.

## Get started

- **Sign up:** `corelink.dev/signup`
- **Docs:** `docs.corelink.dev`
- **Press kit:** `corelink.dev/press`
- **Trust center:** `corelink.dev/trust`

We are deeply proud of this launch and of the customers who came along for the ride before there was a launch to come along to. If you have a build cache that is, today, the bottleneck in your inner loop — we would like to hear from you.

— The CoreLink team at HuGR Labs

---

## Internal notes (strip before publish)

- Word count target: ~1,500. Current draft ~1,400.
- Outstanding edits: CEO/Founder pull-quote, lighthouse customer names (pending engagement per WI-S20-004), pricing finalization (per Finance sign-off).
- Trace links to canonical spec sources verified at draft time.
