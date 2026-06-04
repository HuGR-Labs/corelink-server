# Expansion Campaign #2 — CoreLink Workspaces / Sandboxes

> **Status:** strategy / campaign brief — **post-launch (phase 3+)**. NOT a change
> to the launch route, and NOT a change to Expansion Campaign #1 — it *generalizes*
> it. CoreLink launches as the cache + storage-governance product; campaign #1
> (CI / build acceleration) fires first; this brief names the primitive both are
> built on, so the work compounds instead of forking.
>
> **Owner:** Gustavo Schneiter · **Drafted:** 2026-06-04

---

## The one-sentence thesis

**The workspace — source + deps + toolchain + build state — becomes a
content-addressed object in CoreLink; compute becomes a disposable cursor over it.
Any machine (a CI runner, an AI agent's sandbox, your laptop) hydrates a workspace
in seconds from a cache that gets warmer with every customer.**

Today a machine is where a project *lives* — so the machine carries everything:
gigabytes of build artifacts, runner fleets, a dozen agent processes. In the
Workspaces model the durable object is the workspace in the CAS; machines are
born, attach, work, write back a delta, and die.

---

## One primitive, four products

| Product surface | What it actually is |
|---|---|
| Ephemeral CI runner (campaign #1) | **workspace + a command** |
| AI-agent sandbox | **workspace + an agent** |
| Cloud dev box ("I just want to see the chat") | **workspace + a human attached** |
| Infinite drive / SSD tiering | **lazy materialization of a workspace** |

Campaign #1 is not a sibling product — it is the **first instantiation of the
workspace primitive**. Everything it builds (Firecracker-class compute shell on
Hetzner-class infra, the cache warm-up loop, the pooled-ephemeral economics)
is reused verbatim by the other three rows.

Two user stories the primitive answers directly:

- **Resume:** "every time the environment is reborn I reinstall everything" →
  pull the snapshot, continue where you left off. No dependency resolution, no
  install scripts, no compilation — a manifest download.
- **Incremental rebuild:** "I bump one dep and pay a full rebuild" → a build is
  hundreds of cacheable compiler invocations; rebuild only the real delta, pull
  the rest. ~90%+ of compilation units in a typical tree are dependencies the
  world has already compiled.

---

## Why CoreLink wins this market

The 2026 agent-sandbox wave (e2b, Daytona, Modal, Codespaces/Gitpod) competes on
**cold start**, and their technology is VM snapshots. CoreLink's answer is
structurally better:

- **Content-addressed hydration with cross-tenant shared public deps** — a new
  customer's workspace is already ~40–70% warm because their public dependencies
  are the same ones the previous 499 customers cached. The campaign-#1 network
  effect, applied to the *birth of machines*.
- **R2 free egress** — hydration is the egress-heaviest operation a sandbox
  platform performs; competitors on S3 pay for every byte of it.
- **AI agents multiply demand** — CI is ~1 pipeline per repo; agents are N
  sandboxes per developer *per day* (the founder runs 5+ concurrent agent
  sessions on one laptop today). Same bursty/pooled economics as campaign #1,
  larger and faster-growing market.

## The poetic part (continued from campaign #1)

Campaign #1's origin story was the founder's Mac crashing under recompilation.
This brief's origin story is the same Mac one chapter later: 16 GB RAM with
69 MB free, load average 30, disk at 96%, ten CI runners and twenty-one agent
processes resident — **because the machine was the place where every project
lived.** The founder is customer #0 of Workspaces.

---

## What already exists vs. what's missing

**Already in the codebase (the hard part):**

- Chunking — SplitBlob/SpliceBlob (Wave-5), 5 MiB blobs + multipart.
- **Merkle manifests** — root + ordered chunk list (`corelink-manifest-*` buckets):
  ~80% of a file/workspace representation.
- Multi-region CAS (global) + region-scoped AC/chunk/manifest buckets.
- AC — memoized command results (the `corelink-run` wrapper concept rides on it).
- PAT auth, tenant isolation (HMAC-derived prefixes), fail-CLOSED audit.

**Missing (and mostly shared with campaign #1):**

1. Snapshot client — `corelink-cli snapshot` / `hydrate` (eager or FUSE-lazy).
2. Compute shell — Firecracker-class microVMs on cheap bare metal (campaign #1's
   runner work, verbatim).
3. **Mutable namespace over immutable CAS** — the path→manifest tree with
   git-like semantics. This is the genuinely new engineering in this brief.

---

## Monetization — pinning & the three tiers of "warm"

A user can pay to keep content **permanently warm, independent of traffic**.
"Warm" decomposes into three sellable layers:

| Tier | Guarantee | Mechanism |
|---|---|---|
| **Pin (existence)** | never evicted | pin-set per tenant in D1; eviction job skips pinned digests (refcounted) |
| **Pin regional (latency)** | warm in all 5 regions + edge | replicate pinned blobs to regional buckets; edge cache kept hot |
| **Warm workspace (compute)** | pre-hydrated, attach-and-go | standby slot in the pool; billed as reserved concurrency (consistent with campaign #1 pricing philosophy) |

**Pinning requires eviction — they are two sides of one coin.** Today nothing
evicts (storage COGS grows monotonically); eviction must ship regardless, to
protect margin. Ship best-effort eviction + paid pinning in the same change:
the eviction protects COGS, the pin sells the exception.

### Pricing decision (recorded 2026-06-04)

**Pin 100 GB — $5/mo** ($0.05/GB-month, sold as a package, not per-GB).

| | Value |
|---|---|
| COGS (R2 @ $0.015/GB-mo) | $1.50 / 100 GB |
| Nominal gross margin | ~70% |
| **Effective margin with dedupe** | **~85–90%** |
| vs. field | Depot $0.20/GB → 4× cheaper; ⅙ of the Solo plan → impulse add-on |

**The margin is insane via dedupe, not via markup.** The CAS stores a physical
byte once; N tenants pinning the same public deps are billed N times for one
stored copy — the same GB sold many times (public deps dedupe *perfectly*:
same hash, guaranteed). Markup stays low ($0.05 vs the $0.015 public R2 price a
developer can read) so the price never insults the buyer or invites DIY; the
margin comes from physics.

Pleasant externality: one tenant pinning `tokio 1.x` keeps it warm for every
tenant — pinners are paid heaters of the collective cache. And pinning resolves
campaign #1's honest caveat #1 ("savings land on subsequent builds, not the
first cold one"): a pinned customer has an **SLA on warmth** — there is no
first cold build.

---

## Honest caveats (go in eyes-open)

1. **Latency physics:** local SSD ~100 µs, R2 ~50–150 ms. Workspaces accelerate
   the *birth and resume* of environments; the hot working set still runs on
   local disk. This is tiered storage + memoization, not a network SSD.
2. **Untrusted compute** is a heavier ops discipline than a cache (inherited
   verbatim from campaign #1's caveat #2 — microVM isolation, abuse, SLAs).
3. **Workspaces are private by definition.** Only public, deterministic
   dependencies cross tenants. "Share the public, isolate the private" stays
   sacred; one breach turns the moat into a liability.
4. **Pinning has value only once eviction exists.** Until then everything is
   accidentally perma-warm and the SKU is unsellable. Sequence them together.
5. **The mutable-namespace layer is real new engineering** (FS semantics over
   immutable manifests). Scope it honestly; don't let it leak into the launch
   route or campaign #1's critical path.
6. **Cache-hit economics are per-unit, not magic:** a hit is a download — worth
   it when compile ≫ transfer (true for heavy deps, neutral for tiny crates);
   final linking is always local; changes to root crates still cascade.

---

## Decision recorded

- **Shape:** the **unifying primitive** behind campaign #1 and its successors —
  one product family (runner / sandbox / dev box / drive are SKUs of one
  workspace object), not four products.
- **Timing:** **post-launch.** Campaign #1 fires first and builds the compute
  shell; Workspaces names where it converges so nothing is built twice.
- **Pricing recorded:** Pin 100 GB at $5/mo (package), regional pin as premium
  tier above it, warm-workspace slots billed as reserved concurrency.
- **Sequence:** launch the cache product → dogfood sccache → CoreLink on our own
  runners → campaign #1 (runners) → snapshot/hydrate client → eviction + pinning
  → agent sandboxes → dev workspaces.
