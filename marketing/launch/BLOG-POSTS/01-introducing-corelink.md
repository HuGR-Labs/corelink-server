<!-- DRAFT — pending Legal + Marketing + CEO sign-off. Do not publish. -->

# Introducing CoreLink

> **DRAFT — pending Marketing + Legal + CEO sign-off. Embargoed until Engineering Gate D-day.**
> Style reference: Stripe "Introducing …" launch posts.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 · REMOTE-CACHE-PRODUCT-PROFILE.

---

If you have ever shipped a non-trivial software product on top of Bazel, Buck2, or Remote Build Execution, you have at some point opened a dashboard, watched your cache hit rate slip below the comfort threshold, and started thinking about all the operational work you did not want to do: blob lifecycle, deduplication heuristics, eviction policy, garbage-collection correctness, residency rules, customer-managed keys, audit-grade isolation. The remote cache is the most-leveraged latency primitive in the developer inner loop, and yet — until today — every offering on the market has asked teams to choose between operational simplicity and the controls a regulated, multi-tenant business actually needs.

We built **CoreLink** to make that choice obsolete.

## The build cache landscape

The remote-cache market is small, technical, and surprisingly mature. We did not build CoreLink in a vacuum. Four reference points shaped our design:

**BuildBuddy.** A polished, hosted Remote Build Execution and remote cache offering that has done more than anyone to make REAPI-conformant caching accessible to non-Google teams. BuildBuddy's hosted plane runs in a single primary region per customer; multi-region residency, customer-managed keys at the per-blob envelope-encryption layer, and tenant-isolation invariants modeled in TLA+ are not part of its current shipped scope. We owe the BuildBuddy team a debt for normalizing the REAPI surface for the rest of us.

**BuildBarn.** A high-quality self-hosted implementation, modular and operationally serious. BuildBarn is what you reach for when you want the cache to live entirely inside your own infrastructure. The trade-off is the one every self-hosted choice carries: the customer absorbs the operational tail (eviction tuning, GC correctness, deduplication ratios, blob sprawl, region replication) that a managed service would absorb instead.

**NativeLink.** A more recent entrant, Rust-implemented, with strong performance characteristics and a permissive license. NativeLink has done excellent work on the data-path side. Like BuildBarn, it sits on the self-hosted side of the trade-off; the operator is responsible for tenancy, residency, and key management.

**Bazel Remote.** The reference cache implementation. It is what most teams start with. It is also single-tenant by construction.

Each of these is a serious project, and we recommend any of them to teams whose constraints align with what they ship. The gap we kept hitting was the same one our prospective customers kept describing: a remote cache that was simultaneously managed, multi-tenant, regulated-industry-ready, REAPI-conformant, and built around invariants the customer could verify rather than infer. That product did not exist.

## What CoreLink is

CoreLink is a multi-tenant, content-addressable cache built on Cloudflare's global edge platform, designed to be a drop-in remote cache for Bazel, Buck2, and Remote Build Execution (REAPI-conformant) workloads. It ships, today, at General Availability after twenty-one engineering sprints, an external pentest with post-remediation retest, thirty days of sustained staging, and three lighthouse customer attestations.

CoreLink is opinionated in three places where the existing market is not.

First, **tenant isolation is a TLA+ invariant, not a marketing word.** We maintain four formal specifications — `tenant_isolation.tla`, `cas_integrity.tla`, `audit_immutability.tla`, and `gc_correctness.tla` — in continuous integration. CI fails if the safety property regresses. The specifications are publishable artifacts; customers can read them, their auditors can read them, and the property we claim is the property the model checks.

Second, **the audit chain is a primary artifact, not a logging side-effect.** Every state-changing operation lands in an append-only chain (RFC 6962 Merkle tree construction, RFC 8785 JCS-canonicalized leaves), with daily proof publication. Customers and their auditors can independently re-derive the chain head from raw events. The trust is in the math, not in our word.

Third, **BYOK is real.** AWS KMS is available at GA (GCP Cloud KMS, Azure Key Vault, and HashiCorp Vault Transit are on the roadmap): customers hold the keys, customers wield the kill switch, and erasure makes the tenant's data cryptographically unrecoverable — a customer-served signed Ed25519 erasure attestation is on the near-term roadmap. The 5-minute hard cap on the DEK cache is bounded by code path, not by configuration. When a customer disables their KEK, every in-flight DEK expires within that window, and after that, CoreLink simply cannot read the tenant's data.

## Who CoreLink is for

CoreLink targets four overlapping audiences.

- **Build-heavy engineering teams** running Bazel or Buck2 at scale, particularly those whose CI is bottlenecked on download bandwidth rather than compute.
- **Regulated-industry developer infrastructure groups** that need residency, BYOK, and an auditable substrate for any caching layer they introduce into the SDLC.
- **OSS maintainers** of large polyglot monorepos who want the operational simplicity of a managed cache without surrendering control over residency or erasure.
- **Procurement teams** evaluating remote-cache vendors against modern compliance baselines: GDPR, Schrems II, LGPD, SOC 2, and the crypto-sovereignty requirements that increasingly follow them.

## Why this is hard

Most existing remote caches descend from one of two genealogies. The first is the single-tenant SaaS lineage: simple operationally, but with shared-fate failure modes and no real isolation story. The second is the self-hosted, single-binary lineage: complete control, but with the long tail of operational toil rediscovered by every team that adopts it.

The mixed lineage — multi-tenant, regulated, BYOK-capable, operationally managed, REAPI-conformant — has not previously existed because each of its requirements compounds the others. Multi-tenant means you cannot use single-tenant operational shortcuts. Regulated means residency must be enforced at the data path, not the configuration layer. BYOK means the operator cannot read its own customers' bytes. REAPI conformance means none of this can break the spec your customers' CI already speaks.

CoreLink's design treats each of those as non-negotiable. The product is what falls out of taking them seriously simultaneously.

## A short tour

**Content-addressable storage.** Blobs are keyed by their BLAKE3 (or SHA-256, for REAPI compatibility) digest. Deduplication is a property of the address, not a heuristic. Eviction is content-aware. GC correctness is formally verified.

**Action Cache.** Action results are recorded against a canonical-form action key, with TLA+-verified safety properties on the read/write path. Negative caching is bounded and explicit.

**Tenant isolation.** Every CAS read, every AC write, every audit append carries a verified tenant binding. Cross-tenant access is structurally impossible — and we wrote the structural proof in TLA+ rather than the assertion in Markdown.

**Audit chain.** Every operation lands in an append-only, Merkle-rooted audit log. Daily proofs are published. Customers re-derive the root from raw events as a routine integrity exercise.

**BYOK.** AWS KMS at GA (GCP / Azure / Vault providers on the roadmap). Envelope encryption with bounded DEK cache. Customer-managed kill switch. Verifiable crypto-erasure (customer-served Ed25519 attestation on the roadmap).

**Residency.** Four enumerated regions: WNAM, ENAM, WEUR, SAM. No cross-region leak is a structural invariant.

**24/7 incident response.** Three on-call regions, weekly synthetic page exercise, sub-five-minute response sustained for thirty days as a precondition of GA.

## Customer zero: HuGR Forge

CoreLink was not built in a customer vacuum. HuGR Forge — our own engineering tooling product — has been running on a pre-GA CoreLink deployment since well before GA-day. Forge is a polyglot monorepo with Rust, TypeScript, and Python workloads; its CI graph is the kind of input we wanted to validate against before we asked anyone else to.

Over the thirty-day sustained staging observation window (`DRAFT — final numbers pending CAP-GA-004 attestation`):

- Cache hit rate stabilized in the high range that REMOTE-CACHE-PRODUCT-PROFILE targets for steady-state workloads.
- Per-hit CAS GET latency stayed within the SLO catalog budget (`SLO-LAT-CAS-GET`).
- Zero CRITICAL invariant violations recorded in audit-chain consistency proofs over the window.
- Zero unscheduled operator-initiated decrypt attempts (BYOK is operational; the kill-switch path was exercised in synthetic drill, not in incident response).

Forge's engineering lead has been candid in the lighthouse attestation: the migration to CoreLink absorbed operational work Forge no longer wanted to do, and the audit chain became, for the first time, an artifact Forge could hand to its own auditors instead of a transcript of Slack threads. The full attestation is in the GA case-study bundle.

## What "GA" means at CoreLink

We have, deliberately, two separate tracks at launch.

The first is the **engineering gate** — binary, unappealable, enforced by the founder. It is the gate that decides whether CoreLink can be sold. It is `APPROVED` or it is not, and `APPROVED` requires all seven engineering work items in sprint S-20 sealed, the Production Readiness Review globally approved across thirteen canonical sign-offs, external pentest clean with retest, thirty days of sustained staging, three lighthouse customers with SLA met, SOC 2 gap analysis delivered, on-call runbooks rehearsed, and zero active waivers in CRITICAL controls.

The second is the **launch orchestration** — the press release, the blog posts you are reading, the Product Hunt launch, the social posts. It exists to communicate, in clear language, what the engineering gate has already proven. It does not gate the engineering decision, and it can shift its date without affecting engineering readiness. That separation is deliberate, documented in the spec contract, and enforced by the founder.

We mention it here because the customers we want to attract are customers who care about that separation.

## Pricing

CoreLink ships with Free, Team, and Enterprise tiers. Reference pricing lives at `corelink.humangr.com/pricing`. Where placeholders appear in this and other launch documents, they reflect Finance's final approval cycle — not undecided product economics.

## What is next

Phase 2 — Remote Execution — opens in the next sprint cycle and brings executor identity, sandboxed action execution, and a verifiable execution attestation chain into the same trust model. SOC 2 Type I engagement begins six months post-GA. APAC region expansion follows post-GA based on customer demand. The Phase 2 roadmap is anti-scoped from GA and explicitly out of this launch — we want to ship the cache cleanly before we ship the executor.

## Get started in five minutes

If you have a working Bazel or Buck2 toolchain, the on-ramp is short. Sign up at `corelink.humangr.com/signup`, generate a project token, drop two lines into your `.bazelrc` (or the Buck2 equivalent), and re-run your build. The first run populates the cache; the second tells you whether the latency story we are claiming is the latency story you measure.

- **Sign up:** `corelink.humangr.com/signup`
- **Docs:** `docs.corelink.humangr.com`
- **Press kit:** `corelink.humangr.com/press`
- **Trust center:** `corelink.humangr.com/trust`

We are deeply proud of this launch and of the customers who came along for the ride before there was a launch to come along to. If you have a build cache that is, today, the bottleneck in your inner loop — we would like to hear from you.

— The CoreLink team at HuGR Labs

---

## Internal notes (strip before publish)

- Target length: 1,800–2,200 words.
- Outstanding edits: CEO/Founder pull-quote, lighthouse customer names (pending engagement per WI-S20-004), pricing finalization (per Finance sign-off), Forge case-study numbers (CAP-GA-004 attestation).
- Trace links to canonical spec sources verified at draft time.
- No specific dollar amounts. Competitor coverage is factual and limitation-named only.
