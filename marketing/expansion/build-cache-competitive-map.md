# Build-cache competitive map + sourced user pain

> **What this is:** reference material, not a proposal. Assembled 2026-08-23 from
> sourced research while evaluating whether to add a Gradle cache surface and
> whether to enter the game-studio vertical. It is filed because CoreLink had **no
> written competitive picture at all** for the market its cache product already
> competes in.
>
> **Confidence:** every figure below is labelled. Competitor pricing and
> infrastructure claims were sourced from public pages; anything not sourced is
> marked **UNVERIFIED**. Re-check before quoting externally — pricing moves.

---

## The headline finding

**CoreLink is already in a contested category and did not know it.** At least four
vendors sell a hosted, multi-protocol build cache — the exact positioning
("one cache that speaks every build tool's protocol") that looked like an original
idea. One of them runs on the same substrate we do.

| | Cache protocols | Cloud | Egress to customer | Cross-tenant dedup |
|---|---|---|---|---|
| **Depot** | GH Actions, Bazel, Go, Turborepo, sccache, Pants, Gradle | **AWS** (us-east-1, eu-central-1) | absorbed, not billed | **explicitly NO** |
| **Cachely** | Nx, Lerna, Turbo, Gradle, Bazel | **Cloudflare R2** | n/a (R2) | not mentioned |
| **Buildless** | Gradle, Maven, Bazel, ccache | Cloudflare edge + Dragonfly backend | not published | not mentioned |
| **BuildFetch** | Gradle, Bazel, Sbt, Ccache, Go, Nx | undetermined | **billed separately** | not mentioned |
| **Develocity** (Gradle Inc.) | Gradle, Maven | customer BYO cloud | customer pays own | n/a |

### The two findings that damaged our assumed advantages

**1. Zero egress is not unique.** Cachely runs on Cloudflare R2 — *"Artifacts are
stored on managed Cloudflare R2"* — the same zero-egress substrate we do, at
$29.99/month for 100 GiB + 20M requests, no per-seat charge. The claim "nobody else
is zero-egress" is false.

**2. Zero egress is largely invisible to the buyer.** Depot runs on AWS and
**absorbs** egress rather than billing it: *"Depot doesn't charge for data transfer
in or out of the cache."* Our cost advantage is therefore a **margin** advantage
against them, not a price the customer can see and compare. This is the same trap
as compression: improving absolute cost does not move relative position.

BuildFetch is the exception that proves it — they itemise network at
$0.10–0.15/GB, which is directly beatable.

### Competitor scale

Depot: **$10M Series A** (Mar 2026), *"3,000 users across 1,800 organizations…
1M builds/month"*, ~52 employees. It is also the closest in **shape** to CoreLink —
cache + remote builds + registry. The others read as small or early; no funding or
customer data was found for Cachely, Buildless, or BuildFetch.

### What remains genuinely unclaimed

**Cross-tenant deduplication.** Nobody advertises it, and Depot explicitly refuses
it: *"builders are never shared across organizations… cache entries can only be read
by the same repository that saved them."*

**⚠️ But it does not apply to build caches.** Verified in our own code: `_public`
namespace usage is brew 7, pip 7, npm 9, OCI 10 — and turbo 0, cargo 0, cas 0, ac 0.
`bazel_v2`'s three references are **rejections** (it refuses `_public` as a tenant to
prevent poisoning). This is correct architecture, not an oversight: build-cache
artifacts are the tenant's own compiled output and must stay private.

**So cross-tenant dedup is a real moat on the package-mirror surface and is not
available on the build-cache battlefield at all.**

---

## Sourced user pain — what the market actually complains about

Ranked by frequency across GitHub issues, Gradle forums, HN, and studio blogs. All
quotes are from real users, not vendors.

### 1. Correctness and trust — the dominant complaint

- Bazel disk cache poisoning, **closed as not planned**: *"it's really important to
  see how we can mitigate such poisonous entries since they will cause a serious
  lack of trust for end users ('bazel clean' / 'rm -rf cache folder' here we come)"*
  — github.com/bazelbuild/bazel/issues/6126
- Bazel remote cache poisoning, unresolved: *"If the wrong action proto gets
  uploaded to the cache, the cache will be poisoned. We've seen this in
  production."* — issues/4276
- HN: *"you'll see in a lot of projects random commits like 'blow away corrupted
  cache'"*
- **AndroidX (Google) disables remote caching on release CI**: *"the time saved is
  not worth the cost of potential failure."*

### 2. Silent failure — the tool reports success when nothing was cached

- Turborepo: *"no log output to indicate that the HTTP request failed… I'm unsure
  whether the failure occurs on PUT or GET."* Client reported success; the server
  never received the PUT — github.com/vercel/turborepo/issues/487
- Bazel: cache reports a hit, the reassembled output is truncated, **the build still
  reports success** — issues/29544
- sccache treats HTTP 429 as a cache miss, so rate limiting silently destroys the
  hit rate — corroborated across issues #484, #357, #278. **UNVERIFIED** as a single
  authoritative thread.

### 3. No graceful degradation — a cache outage fails the whole build

- Bazel issue #2964, filed **2017**, still unresolved: *"if the cache is inaccessible
  for any reason… then my builds fail."*
- **Note: this half is not ours to fix.** Fallback on cache outage is client
  behaviour (`--remote_local_fallback`), not server behaviour.

### 4. Vendor lock-in anger

- Nx removing free self-hosted caching: *"Why would you think it is acceptable to
  make us pay for storage that we provide and administer ourselves?"* /
  *"This is Enshitification at its finest."* — github.com/nrwl/nx/discussions/28332

### 5. Cache-key instability causes false misses

- Gradle: keys change when unrelated tasks change; module metadata changes every
  build; keys not stable across platforms — gradle issues #11103, #12854, #7750

### 6. Operational overhead

- Gradle Build Cache Node defaults are too small and eviction tuning is manual:
  *"Remote build cache node keeps evicting cache entries"* — discuss.gradle.org/t/44696

---

## The strategic reading

The market's **biggest unmet need is a cache that can be trusted without manual
verification** — one that fails loud and never silently serves a stale, corrupt, or
truncated hit. Bazel, the ecosystem's most-used tool, has left this unresolved for
years and closed the flagship issue as *not planned*.

**CoreLink is unusually positioned for this**, and it is not a coincidence — silent
success is this repository's own documented dominant defect class, so the
organisation has spent months building the instinct the market is asking for.

The machinery partly exists. `crates/corelink-container/src/adapter_cache.rs` does
re-hash-on-read with self-heal on the `_public` path:

> *"mismatch means CAS corruption, a poisoned map row, or a write [failure]… treat
> it as a miss so the caller re-fetches the authentic upstream and re-stores,
> **self-healing** the entry"* — and *"refusing to serve (treating as miss for
> self-heal)"*

Because the key **is** the content hash, a poisoned entry cannot be served.

**⚠️ It is not uniform.** Integrity-check counts per surface: `cas` 7, `bazel_v2` 3,
`turbo_v8` 3, `cargo` 2, **`ac` 0**. And `turbo_v8`'s own module doc states the hash
is *"stored verbatim as the KV key without any hash-integrity verification."*

**Therefore "the cache you can trust" is not a claim CoreLink can make today** — but
the gap is bounded, knowable, and cheap to close, and closing it is correct
engineering regardless of positioning.

**Honest caveat on the strategy:** trust is how you win an evaluation, not how you
get into one. Nobody shops for "a cache that doesn't corrupt" — they assume it,
discover otherwise, and then disable caching (as AndroidX did). Treat it as a
differentiator to state in the sales conversation, not as an acquisition wedge.

---

## Measured cost of adding a cache surface, for future planning

Two precedents from this repository's own history:

| Surface | Commits | Elapsed |
|---|---|---|
| Turborepo | 35 | 2026-05-29 → 2026-07-12 |
| sccache / cargo | 23 | 2026-05-26 → 2026-07-20 |

**Read these carefully.** Commit count is not effort, elapsed calendar time is not
focused work, and both precedents included building a bridge crate from scratch and
a 457-line PAT resolver **that now exists and is reusable**. The marginal cost of the
next surface is genuinely lower. What does **not** amortise is the per-surface gate
cost: an OKF concept (anti-drift gated, blocking in CI), an e2e journey, and a
conformance test.

An insertion map for a fifth surface (Gradle) was produced and is recorded in
`docs/` planning notes: 6 files, 3 of them non-obvious — the `allowBasicAuth` opt-in
at `worker/src/index.ts:3003`, the `RouteKind` union at `:458`, and the OKF concept
file. No new bridge crate is required; `scope.rs` needs no change at all.
