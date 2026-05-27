---
id: "AUDIT-2026-05-27-COMPETITIVE-LANDSCAPE"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "competitive-landscape", "strategy", "positioning", "pricing", "go-to-market"]
references:
  - "README.md"
  - "ARCHITECTURE.md"
  - "RELEASE-NOTES-v1.0.0-GA.md"
  - "ROADMAP-TO-GA.md"
---

# Competitive landscape — CoreLink remote build cache (2026-05-27)

> Snapshot of remote-cache and build-acceleration competitors as of
> 2026-05-27. Prices, free-tier limits, and feature inventories are
> taken from the vendors' own published pages on that date. All
> websites change — verify before quoting in customer-facing material.

## §0 — Scope and method

In-scope: SaaS or self-host products whose primary or material value
prop is **caching the output of compile/build/test/package steps so
the same work is not re-done across machines, branches, or CI
shards**. Out of scope: language-package registries (npm/PyPI/crates.io),
container registries (Docker Hub, ECR), and CDN caches for end-user
assets (Fastly, Cloudflare Cache).

Method:

- Live `WebFetch` of each vendor's pricing/docs page on 2026-05-27.
- Where the page was unreachable (404 / redirect), the canonical URL
  was used and the result noted.
- Per-source URL is cited in §1 / §2 tables. When a fact is uncertain,
  the cell says "unclear" — we do not invent numbers.
- All claims about CoreLink itself reference `README.md` /
  `ARCHITECTURE.md` / `ROADMAP-TO-GA.md` in this repository.

CoreLink current public position (for the comparison): GA-soft, prod
data plane live behind `corelink.humangr.com`, REAPI v2 implemented
in Rust on Cloudflare's edge, BYOK across 4 KMS providers, RFC 6962
audit log, sandbox tenants free with no card. Solo founder. No public
analytics dashboard parity with BuildBuddy. No remote execution (cache
only). Source: `README.md` lines 13-99, `ROADMAP-TO-GA.md` waves
R-5..R-8.

## §1 — Direct competitors

| Competitor | Pricing (USD, 2026-05-27) | Free tier | Self-serve? | Compat | Hosting | Differentiator | Traction | Source |
|---|---|---|---|---|---|---|---|---|
| **BuildBuddy** | Personal: $0. Team: pay-as-you-go ($/GB over 100GB; Mac core $45). Enterprise: custom. | 100GB cache transfer, 80 Linux cores RBE, 10 users | Yes (Personal/Team); Enterprise = sales | REAPI v2 (Bazel-first); also Buck2 via REAPI | SaaS + self-host (MIT OSS core + commercial enterprise edition) | Bazel build-event UI + RBE + cache in one product; YC alumni; OSS core | GitHub 748 stars, 360 releases, last release 2026-05-26; funded $3.27M (YC + Addition + Scribble + Village Global, Series A Dec-2020) | <https://www.buildbuddy.io/pricing/>, <https://github.com/buildbuddy-io/buildbuddy>, <https://techcrunch.com/2020/12/01/yc-backed-buildbuddy-raises-3-15m-to-help-developers-build-software-more-quickly/> |
| **EngFlow** | Not publicly priced. Free tier and Enterprise both require contact. | Single-machine RE, 32 cores, Linux only, Bazel only | No — contact-gated on both tiers | Bazel, Goma (Chromium), Soong (AOSP), Reclient, BuildStream, Pants, Please, Recc | SaaS managed + self-host (both tiers) | Founded by Bazel lead Ulf Adams + Helen Altshuler (ex-Google Bazel onboarding lead); Series A $18M (a16z, Tiger Global, firstminute) | Bazel ecosystem authority; speaks at BazelCon | <https://www.engflow.com/product/pricing>, <https://www.engflow.com/company/team>, <https://techcrunch.com/2022/11/15/with-18m-in-new-funding-engflow-wants-to-speed-up-your-builds/> |
| **Nx Cloud** | Hobby: $0. Team: $19/contributor + $5.50 per 10k credits + $2.25/concurrent CI conn. Enterprise: custom. | 50,000 credits/mo (resets), 5 contributors, 10 concurrent CI conns | Yes (Hobby/Team); Enterprise = sales | Nx-native (JavaScript/TypeScript monorepos); plugins for some others | SaaS; self-host only on Enterprise tier (free self-host plugins **deprecated May 2026** due to CVE-2025-36852) | Nx-native cache + CI distribution + analytics; Nrwl-owned; deep JS/TS DX integration | Massive Nx community footprint; Nx Agents marketed as "4× faster, 30% cheaper than GitHub Actions" | <https://nx.dev/pricing>, <https://emilyxiong.medium.com/exploring-of-nx-self-hosted-cache-5bc39bd2ed7f> |
| **Turborepo Remote Cache (Vercel)** | Free on all Vercel plans subject to "fair use" | Hobby: 100GB upload/mo, 100 artifact req/min. Pro: 1TB/mo, 10k req/min. Enterprise: 4TB/mo, 10k req/min. **7-day artifact expiry.** | Yes (auto-on for any Vercel team with Turborepo) | Turborepo native; SDK plugins for Nx, Rush | SaaS only (Vercel); community OSS self-host servers exist but unofficial | Bundled free with Vercel; zero setup if you're already deploying to Vercel | Default cache for the JS monorepo market; Vercel acquired Turborepo 2021 | <https://vercel.com/docs/monorepos/remote-caching> |
| **bazel-remote (OSS)** | Free (Apache-2.0) | Unlimited — you pay for your own disk/S3/GCS | Self-host only (no SaaS) | REAPI v2 (Bazel, BuildStream, recc, Pants, Please, Buck2) | Self-host only | The reference open-source REAPI cache server; S3/GCS/Azure backends; zstd; htpasswd/mTLS auth | GitHub 743 stars, 35 releases, last v2.6.1 2025-09-28; community-maintained | <https://github.com/buchgr/bazel-remote> |
| **sccache + S3 (Mozilla OSS)** | Free (Apache-2.0); you pay for object storage | Unlimited — you pay for your own backend | Self-host (CLI + your own bucket) | gcc, clang, MSVC, rustc, nvcc, hipcc, assembler — **compiler wrapper, not REAPI** | Self-host (CLI); backends: S3, R2, GCS, Azure, Redis, Memcached, GHA, WebDAV, Alibaba OSS, Tencent COS | The de facto Rust/C++ compiler cache; multi-level hierarchical caching | GitHub 7.3k stars; v0.15.0 2026-04-29; Mozilla-stewarded | <https://github.com/mozilla/sccache> |
| **Garnix** | Free: $0. Individual: $25/mo. Enterprise: custom. Overage $0.006/CI-min. | 1,500 CI min/mo, 500 PR-deploy min/mo, public + private Nix binary cache | Yes (Free/Individual); Enterprise = sales | **Nix-only** (binary cache, derivations) | SaaS; on-prem only via Enterprise | Nix-native CI + binary cache + PR preview deploys; opinionated, hosted Nix world | Smaller community than the giants but devoted Nix following | <http://garnix.io/pricing/> |

Notes on §1:

- "BuildBuddy" and "EngFlow" are the two head-to-head competitors for
  the REAPI v2 + cache + RBE bundle. CoreLink ships REAPI v2 cache
  only — no RBE in GA.
- "Nx Cloud" and "Turborepo Remote Cache" do not implement REAPI;
  they are proprietary protocols owned by their respective build tools
  (Nx and Turborepo). They are competitors for the **JS/TS monorepo
  buyer** even though the wire protocol differs.
- "bazel-remote" and "sccache" are OSS — they compete on the
  "self-host with a bucket" buyer who would otherwise pay CoreLink.
  The unit economics question for that buyer: ops cost of operating
  your own deployment vs. CoreLink managed.

## §2 — Indirect competitors (build-it-yourself, generic infra)

| Competitor | What it costs | Limits | What you build on top | Source |
|---|---|---|---|---|
| **AWS S3 + custom cache layer** | $0.023/GB-mo (Standard, us-east-1) + egress ($0.05–$0.09/GB outside AWS) + requests | None on object storage; cache semantics are your problem | REAPI server (self-host bazel-remote pointing at S3) or proprietary cache client | <https://aws.amazon.com/s3/pricing/> (verify before quoting) |
| **Cloudflare R2 + custom** | $0.015/GB-mo storage; **$0 egress**; class-A/B request fees | None | Same as S3 — you wire a cache protocol; the zero-egress is the win | <https://developers.cloudflare.com/r2/pricing/> (verify) |
| **GitHub Actions Cache** | Free with GitHub | **10GB per repo default cap** (org admin can raise to 10TB on paid); 7-day inactivity eviction; 200 uploads/min, 1500 downloads/min | Built into `actions/cache@v4` — only works inside GHA jobs | <https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows> |
| **CircleCI cache** | Bundled with CircleCI plan | Per-job cache + per-workflow cache; eviction varies; no cross-org sharing | Built-in; only inside CircleCI | (CircleCI docs — fact-check before quoting) |
| **GitLab CI cache** | Bundled with GitLab plan | Distributed cache via S3-compatible store; size/eviction controlled by your runner config | Built-in; only inside GitLab CI | (GitLab docs — fact-check before quoting) |

CircleCI / GitLab pricing pages were not fetched in this audit —
both change frequently and their cache layers are bundled with the
larger CI offer. When writing a customer-facing page, re-fetch.

## §3 — CoreLink positioning matrix

### 3.1 — Strengths CoreLink genuinely has (per repo evidence)

1. **REAPI v2 compatibility, not Bazel-only.** Any REAPI client works
   (Bazel, Buck2, BuildStream, Pants, Please, recc) — same surface
   area as BuildBuddy/EngFlow/bazel-remote. Source: `README.md` line
   15, `ARCHITECTURE.md`.
2. **Package-manager adapters beyond build systems.** Cargo, npm, pip,
   brew, OCI artifact paths in the value prop. Source: `README.md`
   line 15 ("software builds, package indices, container layers, and
   ML artifacts"). This is broader than BuildBuddy/EngFlow.
3. **BYOK across 4 KMS providers** (AWS KMS, GCP KMS, Azure Key
   Vault, Vault), customer-held KEK, 5-minute DEK in-memory cap,
   Ed25519-signed erasure attestation. Source: `README.md` lines
   83-87. **No competitor in §1 advertises 4-KMS BYOK at the cache
   tier.**
4. **TLA+-verified tenant isolation invariant.** Counterexamples block
   merge in CI. Source: `README.md` lines 76-81. This is a defensible
   trust claim no §1 competitor publishes.
5. **RFC 6962 / RFC 3161 Merkle-chained audit log** with re-derivable
   root. Source: `README.md` lines 89-93. Auditor-facing.
6. **Cloudflare edge data plane.** Low-latency on every continent
   without per-region SaaS plan splitting. Source: `README.md` line
   15.
7. **Honest residency story** with 4 enumerated regions and explicit
   metadata cross-border documentation per sub-processor. Source:
   `README.md` lines 95-99.
8. **Rust SOTA quality bar** — `cargo-deny` lockdown, proptests,
   model checking. Source: `README.md` lines 54-63.

### 3.2 — Weaknesses CoreLink has (be honest)

1. **Solo founder, no brand recognition.** BuildBuddy = YC alums;
   EngFlow = Bazel-creator pedigree. CoreLink has neither. Acceptance
   in the Bazel/Buck2 community is earned, not assumed.
2. **No RBE.** Cache only. BuildBuddy and EngFlow sell remote
   execution; CoreLink does not (per Wave 32-36 scope; no RBE in GA).
   Customers who want "skip the box AND the build farm" will not
   pick CoreLink.
3. **No analytics / build-events UI parity** with BuildBuddy's UI.
   This is BuildBuddy's stickiest hook for Bazel buyers.
4. **No lighthouse customer reference.** Per `ROADMAP-TO-GA.md` waves
   R-5..R-8, this is on the calendar but not yet in hand.
5. **No SOC 2 yet.** Target: Type I, GA + 6 months (per
   `README.md` badge line 7). Regulated buyers will gate on this.
6. **No public IDE/CLI ecosystem the size of Nx or Turborepo.**
   For pure JS/TS monorepo buyers, switching to CoreLink is a
   workflow change for marginal benefit.
7. **No published pricing yet.** Every §1 competitor except EngFlow
   has a public number; absence of pricing = friction for self-serve.
8. **Cloudflare-edge is a hosting bet.** Customers whose threat model
   forbids Cloudflare (some EU public sector, some defense) will
   self-disqualify even with BYOK.

### 3.3 — Where CoreLink clearly wins (3 segments)

1. **Regulated mid-market with multi-cloud KMS estates.** Fintech,
   healthtech, gov contractors that need BYOK with AWS+GCP+Azure (not
   just one) and an audit log that survives an external auditor's
   re-derivation. BuildBuddy/EngFlow do not advertise 4-KMS BYOK
   parity at the cache tier; Nx Cloud and Turborepo are not in this
   conversation.
2. **Polyglot orgs that span "Bazel + Cargo + npm + OCI" in one
   buying decision.** Today, those orgs stitch BuildBuddy (Bazel) +
   sccache+S3 (Rust/C++) + Turborepo cache (JS) + a private registry
   (OCI). CoreLink offers one cache, one billing, one audit log.
3. **Engineering-trust buyers who read invariants.** Teams that
   actually open `specs/03_architecture/tla+/` and check the model.
   Niche, but high-signal — these buyers become design partners.

### 3.4 — Where competitors clearly win (3 segments)

1. **Bazel-monolith shops that want RBE + cache + build-events UI in
   one buy.** BuildBuddy or EngFlow. CoreLink has no RBE.
2. **JS/TS monorepos already shipping to Vercel.** Turborepo Remote
   Cache is free, auto-on, and zero-setup. CoreLink cannot beat
   "already on by default."
3. **Solo developers / OSS maintainers with $0 budget and a single
   bucket.** bazel-remote or sccache pointed at R2 is unbeatable on
   price ($0 software + ~$15-$50/mo of R2). CoreLink should not try
   to compete here.

### 3.5 — Blue-ocean niches CoreLink could own

1. **"REAPI cache with verifiable audit for the SOC 2 auditor's
   first interview."** Sell the audit log as the product (the cache
   is the carrier). No §1 competitor leads with this.
2. **"BYOK-by-default polyglot cache for the EU mittelstand."**
   Multi-KMS + honest residency + EU data-plane (Cloudflare's `weur`)
   + content-addressable provenance. Nx/Turborepo are Vercel-coupled;
   BuildBuddy/EngFlow are US-centric.

## §4 — Recommended positioning statement

> **CoreLink is the REAPI v2 + multi-package-manager cache for
> regulated polyglot engineering orgs who need BYOK, residency
> honesty, and a re-derivable audit log — without buying a build-farm
> they do not want to operate.**

Why this works:

- "REAPI v2" — disqualifies the JS-monorepo-only buyer up front
  (good, that's not us).
- "multi-package-manager" — differentiates from Bazel-only competitors.
- "regulated polyglot engineering orgs" — narrows the ICP to a
  segment where our BYOK / audit-log story actually has buying power.
- "without buying a build-farm" — pre-empts the "but BuildBuddy has
  RBE" objection by repositioning RBE as a cost, not a feature.

Alternates to A/B test on a landing page:

- "The compliance-grade build cache. REAPI v2. BYOK on four KMS.
  Audit log your auditor can verify."
- "One cache for Bazel, Cargo, npm, pip, brew, and OCI. BYOK by
  default. Residency you can defend."

## §5 — Comparison-page targets (SEO + buyer-journey)

Priority order (highest intent first):

1. **CoreLink vs BuildBuddy** — high intent; both REAPI; differentiate
   on (a) no-RBE-by-design + BYOK breadth, (b) polyglot beyond
   Bazel. Cite BuildBuddy's $0.45/Mac-core and 100GB egress cap.
2. **CoreLink vs EngFlow** — Bazel-purist segment; lead on
   "self-serve pricing with a number on the page" (EngFlow gates
   both tiers on contact). Don't try to out-Bazel the Bazel creator
   on Bazel — out-pricing-transparency them.
3. **CoreLink vs Turborepo Remote Cache** — only worth writing if
   we ship a Turborepo adapter. Lead on "7-day artifact expiry vs
   CoreLink retention" and "your cache, your KMS." Lower priority.
4. **CoreLink vs Nx Cloud** — relevant after Nx-cloud-self-host
   deprecation (May-2026, CVE-2025-36852). Position as the
   "self-hostable or managed, your call" alternative for orgs that
   refuse SaaS-only.
5. **CoreLink vs bazel-remote + S3** — TCO calculator: ops cost of
   running your own + paying egress vs CoreLink flat. Honest enough
   to say "if you have 1 cluster and patient SREs, stay on bazel-remote."
6. **CoreLink vs sccache + S3** — same TCO story; emphasize that
   sccache and CoreLink are not mutually exclusive (sccache as L1,
   CoreLink as L2 shared). Friendly comparison.
7. **(Optional) CoreLink vs Garnix** — only for the Nix segment; low
   ICP overlap.

For pages 1, 2, 5: link to the public TLA+ spec and the audit log
re-derivation walkthrough. Those are the hardest to credibly fake and
the most defensible vs each competitor.

## §6 — Pricing benchmarks summary

| Tier shape | BuildBuddy | EngFlow | Nx Cloud | Turborepo | Garnix | CoreLink (TBD) |
|---|---|---|---|---|---|---|
| Free | 100GB cache + 80 cores + 10 users | 1 machine, 32 cores, Linux only | 50k credits, 5 contributors | 100GB/mo (Hobby) | 1.5k CI min/mo | 24h sandbox, no card (per README) |
| Paid entry | Pay-as-you-go on cache GB | Not public | $19/contributor + credits + CI conns | Free w/ Vercel plan | $25/mo flat | **Not yet published** |
| Enterprise gate | Contact | Contact (both tiers) | Contact | Contact | Contact | (TBD) |
| Mac premium | $45/core | Bundled | N/A | N/A | N/A | N/A (cache only) |

Observations:

- **Three pricing archetypes**: (a) usage-based on cache GB
  (BuildBuddy), (b) seat × usage credits (Nx Cloud), (c) flat-tier
  (Garnix). Turborepo is "free with limits" — it isn't really priced.
- **CoreLink has not chosen** — pricing is a Wave-post-GA decision.
  Recommended starting shape: flat tier (Free / Team / Enterprise)
  with a published Team number; usage-based egress as overage. Avoid
  the Nx Cloud "credits" abstraction — buyers complain about
  unpredictability (see Medium articles in §1 sources).
- **Nobody undercuts "free + your own bucket"** (sccache /
  bazel-remote). Pricing must justify the managed value, not race
  to zero.

## §7 — Brand / messaging do-NOT-copy list

Phrases competitors use that CoreLink should **not** reuse (sound
like a clone, or are already owned):

- "Faster builds" — every single competitor says this; meaningless.
  Use "verifiable builds" or "trust-grade cache" instead.
- "Remote execution" / "RBE" — BuildBuddy + EngFlow own this category;
  using the words pulls us into a fight we lose (we don't have RBE).
- "Bazel" as the primary noun on the homepage — pulls EngFlow energy.
  Lead with REAPI v2 and the polyglot story; Bazel is one client.
- "Build events viewer" / "build UI" — BuildBuddy's signature
  feature; we don't have parity. Don't promise it.
- "Smart monorepo" / "smart agents" — Nx Cloud's tagline.
- "Days saved" / "time saved" dashboards — Vercel/Turborepo's framing.
- "Zero config" / "auto-on" — Turborepo's free-with-Vercel angle;
  we are not zero-config (BYOK config is the whole point).
- "Credits" as a billing unit — Nx Cloud uses this and customers
  publicly complain about unpredictability.
- "Build farm" — implies you also have to operate one. We don't.
  CoreLink's negative-feature is "no build farm to run."

Phrases CoreLink **should** own (no §1 competitor leads with these):

- "Cross-tenant safe" (already in README L3)
- "Engineered as if you were the auditor" (already in README L3)
- "BYOK on four KMS"
- "Re-derivable audit log" / "RFC 6962 audit chain"
- "Polyglot cache" (Bazel + Cargo + npm + pip + brew + OCI)
- "Residency you can defend"

## §8 — Open items (verify before going public)

1. CircleCI and GitLab cache pricing — not fetched in this pass.
   Re-fetch before publishing comparison pages.
2. EngFlow does not publish prices; before claiming "more expensive
   than us" we need a real customer quote or sales conversation.
3. The Nx Cloud self-host deprecation (May 2026, CVE-2025-36852) is
   cited from a Medium article — confirm against Nx's official
   security advisory and changelog before using in customer-facing
   copy.
4. AWS S3 / Cloudflare R2 unit prices — cells in §2 are last-known;
   re-fetch the pricing pages before any TCO calculator.

## §9 — Sources (all fetched 2026-05-27)

- BuildBuddy pricing: <https://www.buildbuddy.io/pricing/>
- BuildBuddy repo: <https://github.com/buildbuddy-io/buildbuddy>
- BuildBuddy funding: <https://techcrunch.com/2020/12/01/yc-backed-buildbuddy-raises-3-15m-to-help-developers-build-software-more-quickly/>
- EngFlow pricing: <https://www.engflow.com/product/pricing>
- EngFlow team: <https://www.engflow.com/company/team>
- EngFlow funding: <https://techcrunch.com/2022/11/15/with-18m-in-new-funding-engflow-wants-to-speed-up-your-builds/>
- Nx Cloud pricing: <https://nx.dev/pricing>
- Nx self-host deprecation context: <https://emilyxiong.medium.com/exploring-of-nx-self-hosted-cache-5bc39bd2ed7f>
- Turborepo Remote Cache: <https://vercel.com/docs/monorepos/remote-caching>
- bazel-remote: <https://github.com/buchgr/bazel-remote>
- sccache: <https://github.com/mozilla/sccache>
- Garnix pricing: <http://garnix.io/pricing/>
- GitHub Actions cache: <https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows>
- CoreLink internal: `README.md`, `ARCHITECTURE.md`, `ROADMAP-TO-GA.md`
