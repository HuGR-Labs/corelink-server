# Expansion Campaign #1 — CoreLink CI / Build Acceleration

> **Status:** strategy / campaign brief — **post-launch (phase 3)**. NOT a change
> to the launch route. CoreLink launches as the cache + storage-governance
> product; this is the first *expansion* campaign, planned to fire once the
> core product is live and the cache is dogfooded.
>
> **Owner:** Gustavo Schneiter · **Drafted:** 2026-06-02

---

## The one-sentence thesis

**The same content-addressed cache that is CoreLink's core product is also the
engine for a cheap, fast, off-your-machine CI offering — and because the cache
makes builds faster (which the customer loves) *and* cheaper to run (which fattens
our margin), it is a true win-win that compounds as we grow.**

This is not a separate product. It is CoreLink *maturing into its full form*:
**cache (the engine) + compute (runners) + governance = a defensible CI platform.**
Cache alone is a commodity (Turborepo's remote cache is free). Cache + compute +
the multi-tenant network effect is a moat.

---

## Why it's a win-win (the flywheel)

| | Cold build (no cache) | Cached build (~90% hit) |
|---|---|---|
| Compute per build | ~10 min | ~1–2 min |

A cached build cuts compute per build by **~5–10×**. The load-bearing insight:

> **The exact thing that delights the customer (speed) is the exact thing that
> lowers our cost (less compute).** They are not in tension — they are the same
> lever.

One lever, three wins:
1. **Faster** for the customer → they love it (and churn less).
2. **Cheaper** for us → fat gross margin (~80% — see economics below).
3. **Higher throughput** → the runner frees up sooner → more customers per box.

## The deeper moat — a multi-tenant cache has a network effect

CoreLink's cache is **content-addressed**: an identical artifact (the same
`aws-lc-sys 1.2` compiled deterministically) has the same hash, is stored **once**,
and is **shared across customers**.

> **The more customers we have, the fuller the cache, the more cache-hits everyone
> gets, the faster + cheaper everyone's CI.** Customer #500 walks in and already
> gets a hit on 90% of the public deps the previous 499 compiled.

A competitor without scale, or with a single-tenant cache, structurally cannot match
this. **The product gets better as it grows — a data moat.**

**Security boundary (non-negotiable):** share only **public, deterministic**
artifacts (crates.io / npm / public deps — identical bytes = same hash). **Private
build outputs NEVER cross tenants.** "Share the public, isolate the private."

## The poetic part

This is **literally the founder's origin pain** — recompiling the same dependency
on every runner, filling disk, crashing the Mac — turned into the product's **margin
engine and moat.** We're not selling "a cache." We're selling
**"the world's builds, compiled once, shared."**

---

## What already exists (foundation is real, not aspirational)

The cache foundation is **already in the codebase** — this expansion mostly adds the
*compute* layer on top of caches that already work:

- **Native CAS / AC** — `crates/corelink-container/src/routes/{cas,ac}.rs` (R2-backed).
- **Bazel remote cache (REAPI v2)** — `routes/bazel_v2.rs`
  (`--remote_cache=https://corelink-api.humangr.com/bazel/v2`).
- **Turborepo remote cache** — `routes/turbo_v8.rs` (`TURBO_API=…`).
- **sccache → CoreLink** (WebDAV) — the Rust/C++ path; the dogfood we already started
  (the `PAT_SIGNING_KEY` 401 fixed 2026-06-02 was exactly for this).

So CoreLink is **already multi-language, multi-toolchain** at the cache layer. The
expansion adds: ephemeral runners (the compute) + a one-line drop-in.

---

## Economics — the COGS that make it work

**Cost to serve one Solo user ($30/mo plan), per month:**

| Component | Typical | Note |
|---|---|---|
| Compute (1× 2 vCPU, pooled/bursty) | $2–4 | CI is bursty; oversubscribe a cheap bare-metal box (Hetzner) across many users |
| Cache storage (R2, ~100 GB cap) | $0.50–1.50 | CAS dedup; users rarely fill the cap |
| **Egress (cache pulls)** | **~$0** | **R2 has no egress fees** — a structural COGS edge vs anyone on S3 |
| Platform (Workers / D1 / DO) | ~$0.50 | CoreLink's existing infra; tiny per-user marginal |
| Stripe fee | ~$1.17 | 2.9% + $0.30 on $30 |
| **Total** | **≈ $4–7** | **→ ~77–87% gross margin on $30** |

Worst-case heavy user (one runner near 24/7): COGS ~$10–14 → still ~55–67% margin,
and **bounded** (1 runner = 1 machine, ~720 h/mo cap) so it never goes negative.

**Why so cheap:** the cache does double duty — it's the feature *and* it's what lets
runners be **ephemeral** (fast warm-up from cache → spin up per job → pay compute only
when building → cost scales with real usage, not reserved capacity). Combined with
R2's free egress, the per-user COGS is a number competitors on S3 + paid separate
cache cannot reach.

---

## Pricing (recommended starting point — validate before publishing)

Cache **included free**; runners billed by **concurrency, unlimited minutes** (the
sustainable model — you bill for reserved capacity, not per-minute).

| Plan | Price | Compute | Minutes | Cache |
|---|---|---|---|---|
| Free | $0 | 1× 2 vCPU | 2,000/mo (fair-use) | 10 GB |
| **Solo** | **$30/mo** | 1× 2 vCPU | **unlimited** | 100 GB |
| Team | $120/mo | 4× 2 vCPU parallel | **unlimited** | 500 GB |
| Scale | +$28 / parallel runner | + by concurrency | **unlimited** | +250 GB/runner |
| Business | custom | BYOC (~$0.002/min infra) | unlimited | unlimited |

vs. the field (Jun 2026): GitHub-hosted Linux $0.006/min (+$0.002/min for self-hosted
from Mar 2026); BuildJet / Blacksmith / WarpBuild ~$0.003–0.004/min; Depot $20–$200
plans + $0.004/min + $0.20/GB. Flat-per-concurrency unlimited is **bizarrely cheap for
any heavy user** (20k min/mo ≈ $80+ on per-minute vs $30 flat here).

---

## The campaign angle (when we fire it)

- **Tailwind to ride:** GitHub starts **charging for self-hosted runners (+$0.002/min)
  in March 2026.** The whole market is shopping for alternatives *right now*.
- **The hook:** *"CI with unlimited minutes. Pay per parallel runner, not per minute.
  Cache included free. ~60–70% cheaper than GitHub-hosted — and it never touches your
  machine."*
- **The proof story:** dogfood. "We built CoreLink because recompiling the same deps on
  every runner kept crashing our Mac. Now that pain is our cache — and our CI runs on
  it. Here's the receipt." (Honest, founder-origin, technical-credible.)
- **Land-and-expand:** existing CoreLink cache users are the warmest possible audience —
  "you already cache with us, add runners and get full CI in one line."

## Honest caveats (go in eyes-open)

1. **Cache hit-rate varies** by workload — incremental changes = high hits; clean/from-
   scratch builds = low. The savings land on *subsequent* builds, not the first cold one.
2. **Running untrusted compute is a different, heavier ops discipline** than running a
   cache — microVM isolation (Firecracker-class), capacity/availability SLAs, abuse
   handling. Commercially one product; internally two engineering disciplines.
3. **Cross-tenant sharing is public-deterministic-only** — private outputs are sacred
   and isolated. Get this wrong once and the moat becomes a liability.
4. **Margin depends on the provisioning model** — ephemeral/pooled is what makes the
   ~80% margin; always-on reserved erodes it. Architecture choice = the business model.

---

## Decision recorded

- **Shape:** a **feature / natural evolution of CoreLink**, not a separate product.
  (Revisit a spin-out only if the compute business later grows huge + operationally
  distinct — a high-class problem for much later.)
- **Timing:** **phase 3 / post-launch.** Does not change the current launch route.
- **Sequence:** launch the cache product → dogfood sccache → CoreLink on our own runners
  → open the CI runners as the premium add-on → fire this campaign.
