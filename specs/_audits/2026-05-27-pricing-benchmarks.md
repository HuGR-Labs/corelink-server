---
id: "AUDIT-2026-05-27-PRICING-BENCHMARKS"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "0.1.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "pricing", "benchmarks", "strategy", "go-to-market", "solo-startup"]
---

# Pricing benchmarks + CoreLink pricing v0.1 recommendation

> **Scope.** Research SOTA dev-tool SaaS pricing models (direct comparables + solo-founder dev-infra patterns), quantify CoreLink's actual unit economics on Cloudflare, frame the value proposition in CI-minutes-saved, and recommend a v0.1 pricing structure that is solo-founder-realistic (one pricing page, three tiers max). All prices were fetched live on 2026-05-27; URLs cited. Cost figures from Cloudflare public docs (R2, Workers/KV/D1) on the same date.

> **Method.** WebFetch on each vendor's `/pricing` page; cross-reference with secondary sources where the page was 404 or content-thin (EngFlow, Garnix). Cloudflare costs from `developers.cloudflare.com/r2/pricing/` and `/workers/platform/pricing/`.

> **What this is not.** Not a financial model (no LTV/CAC, no cohort math). Not a packaging spec (no SKU IDs, no Stripe price-IDs). Output is decision-grade input for the actual pricing page (`apps/docs/docs/pricing.mdx`) and a future `docs(strategy): pricing-v0.1` PR that wires Stripe.

---

## §1 — Comparable pricing table (direct: build/CI cache SaaS)

All fetched 2026-05-27.

| Vendor | Free tier | Paid entry | Pricing axis | Enterprise | URL |
|---|---|---|---|---|---|
| **BuildBuddy** | 100 GB cache transfer/mo, 10 users, 80 Linux cores RE, community support | "Team" pay-as-you-go: $X/GB over 100 GB (price obfuscated on page), 800 Linux cores, Mac at $45/core, unlimited users | Per-GB cache transfer + per-core RE (Mac separate SKU) | "Request a Quote", SSO/SAML, isolated infra, 99.9% SLA | https://www.buildbuddy.io/pricing |
| **EngFlow** | Single-node, 1 machine, 32 cores, Linux only, community | No public paid tier between Free and Enterprise | (gap) | "Contact sales", hundreds–thousands of machines, Linux/macOS/Windows, S3/GCS, autoscaling, 99.9% SLA | https://www.engflow.com/product/pricing |
| **Nx Cloud** | "Hobby" forever-free: 50k credits/mo, 5 contributors, 10 concurrent CI connections | "Team": starts at $0 + **$19/contributor** + **$5.50/10k credits** + **$2.25/concurrent connection** | Hybrid: per-seat + per-credit + per-connection | Custom; conformance, cross-repo, SSO, isolated/self-host | https://nx.dev/pricing |
| **Vercel Remote Cache (Turborepo)** | Hobby: 100 GB upload/mo, 100 art-req/min | Pro: 1 TB upload/mo, 10k art-req/min (bundled into Vercel Pro at $20/seat) | Bundled with Vercel seat ($20/mo Pro) under "fair use"; not separately metered | 4 TB upload/mo, bundled with Vercel Enterprise | https://vercel.com/docs/monorepos/remote-caching |
| **Garnix CI** | 1,500 CI min/mo, 500 PR-deploy min, 2-mo trial of server-deploy, public+private cache | "Individual": **$25/mo**, 10,000 CI min, unlimited PR-deploy (10k incl.), 1 server-deploy slot; overage $0.006/min | Per-minute (CI/deploy) + per-server-instance ($15–$120/mo per machine, 20 TB traffic each) | "Contact sales", unlimited min, on-prem, custom CI domain | https://garnix.io/pricing/ |

**Patterns observed.**

1. **Free tier is universal.** Every comparable has one — none gate the on-ramp behind a credit card. Floor is "1 user/1 machine/small cap"; ceiling is BuildBuddy's 100 GB + 10 users which is generous.
2. **Pricing axes split into two camps.** (a) **Resource-metered** (BuildBuddy per-GB, Garnix per-minute, Nx per-credit) — aligns vendor cost to revenue but is unpredictable for the buyer. (b) **Bundled flat** (Vercel "fair use" inside the Pro seat) — predictable but only works if the cache is a feature of a larger product. CoreLink is the cache, so option (b) is unavailable.
3. **"No public mid-tier" is real.** EngFlow has free → enterprise with nothing between. BuildBuddy obfuscates the $/GB on the public page. This is a deliberate filter: small teams self-serve on free; sales conversation gates everything else. Risk: kills bottom-up adoption.
4. **Per-seat is the minority axis.** Only Nx Cloud charges per-contributor ($19) and even there it's a small line item next to credits. BuildBuddy, EngFlow, Garnix do not charge per-seat at all. **Important signal for CoreLink: per-seat in a cache product is friction without proportionality to value.**
5. **Enterprise = SSO/SAML + dedicated infra + SLA.** Across all five, the enterprise tier upsell is the same three levers. No vendor exposes BYOK as an enterprise lever yet — opportunity for CoreLink given the BYOK µK architecture from Wave 33.

---

## §2 — Solo-founder dev-infra pricing patterns

All fetched 2026-05-27.

| Vendor | Free tier | Entry paid | Top public tier | Pricing axis | Annual discount | URL |
|---|---|---|---|---|---|---|
| **Plausible** | 30-day trial, no CC | Starter $9/mo (10k pageviews, 1 site) | Business $19/mo (10 sites, 10 members, funnels, API) | Per-pageview (stepped) + per-site/seat caps | "2 months free" (~17%) | https://plausible.io/#pricing |
| **Tinybird** | 0.25 vCPU, 1k req/day, 10 GB | Developer $49/mo (0.5 vCPU, 25 GB) | SaaS custom (32 vCPU, 500 GB) | Compute-second ($0.0002/s) + storage ($0.058/GB) overage | Not advertised | https://www.tinybird.co/pricing |
| **Lago** | Self-host (OSS, free) | Cloud "Business": contact-us | "Enterprise": contact-us, self-host option, 24/7 | Opaque (sales-led); OSS escape valve | n/a | https://www.getlago.com/pricing |
| **PlanetScale** | (Free tier removed 2024) | Postgres EBS non-HA from $5/mo (PS-5 single node) | Up to $94k/mo (Metal 3-node) | Cluster size (vCPU/RAM) + storage + backups + egress | Not advertised on table | https://planetscale.com/pricing |
| **Resend** | 100 emails/day, 3k/mo | Pro $20/mo (50k emails); $35 (100k) | Scale $90–$1,150/mo (100k–2.5M); $0.90→$0.46 per 1k overage | Per-email (stepped tiers) | Not advertised | https://resend.com/pricing |
| **Sentry** | Dev: 5k errors, 50 replays, 5 GB logs, 5M spans, 1 user | Team $26/mo (annual) | Business $80/mo (annual) | Per-event (errors/spans/replays); PAYG overages ($0.50/GB logs) | Annual billing tier shown | https://sentry.io/pricing/ |
| **BetterStack Uptime** | 10 monitors, 1 status page | $25/mo monthly / $21/mo annual (+50 monitors) | Higher tiers add monitors | Per-monitor pack | ~16% (annual) | https://betterstack.com/uptime/pricing |

**Patterns observed.**

1. **The $20/mo psychological anchor is real.** Plausible ($9 → $19), Resend ($20), BetterStack ($21 annual), Sentry Team ($26 annual). Every successful solo-founder dev-infra tool prices its entry paid tier between **$9 and $30/mo**. Anything above $49 (Tinybird) demands either consumption flexibility or a different buyer (team lead vs. individual dev).
2. **Three tiers is the dominant shape.** Free → one mid → "contact us" (Sentry, Resend, Plausible, Garnix, BuildBuddy, EngFlow). Plausible has four because pageview metering forces it; that is the exception. **For CoreLink, three tiers is correct.**
3. **Annual discount = "2 months free" / ~17%.** Plausible, BetterStack converge here. Sentry uses annual as the *only* way to hit advertised price (a sharper lever).
4. **Consumption-only pricing (Tinybird) demands a high-ARPU buyer.** $49 entry + per-second compute is for funded teams. **Bad fit for CoreLink at v0.1**: solo-founder credibility requires a price below the trial-budget threshold.
5. **Removed-free-tier is rare and noticed.** PlanetScale killed its free tier in 2024 and lost developer goodwill (and ate a competitor wave). **Do not start without a free tier.**
6. **Sales-led-only (Lago, EngFlow) does not work for bottom-up products.** It works for Lago because the buyer is a billing-platform implementer; it works marginally for EngFlow because Bazel users are concentrated at the top. CoreLink's buyer is a CI-pain-feeling dev → must self-serve.

---

## §3 — CoreLink cost structure on Cloudflare (per-tenant)

All prices from CF docs fetched 2026-05-27. Egress is free on R2 + Workers — this is the structural advantage that lets CoreLink price aggressively against AWS-hosted competitors.

### R2 (cache object storage)
- Storage: **$0.015 / GB-month** (Standard).
- Class A (writes/PUT): **$4.50 / M** ops.
- Class B (reads/GET): **$0.36 / M** ops.
- Egress: **free** (Workers/S3 API/r2.dev).
- Free tier (counts against the platform free, not per-tenant): 10 GB-mo storage, 1M class-A, 10M class-B.

### Workers (CAS gateway, manifest API, signed-URL minter)
- Base: **$5/mo flat** for the paid Workers plan.
- Requests: **10M included / mo**, then **$0.30 / M**.
- CPU: 30M CPU-ms included, then **$0.02 / M ms**.
- No egress, no bandwidth charge.

### KV (hot manifest metadata)
- Reads: 10M included, then **$0.50 / M**.
- Writes/deletes: 1M included, then **$5.00 / M**.

### D1 (relational: tenants, scopes, audit log)
- Rows read: 25B included, then **$0.001 / M**.
- Rows written: 50M included, then **$1.00 / M**.

### Per-tenant variable cost — worked example

Assume a paying tenant cache footprint of **50 GB storage, 1M reads/mo, 100k writes/mo**:
- R2 storage: 50 × $0.015 = **$0.75/mo**.
- R2 class A: 100k × ($4.50/M) = **$0.45/mo**.
- R2 class B: 1M × ($0.36/M) = **$0.36/mo**.
- Worker requests (≈1.1M total, all within 10M shared free): **$0.00 marginal**.
- KV reads (manifest lookups ≈ 1 per cache GET, 1M, within 10M shared): **$0.00 marginal**.
- D1 (audit log row per write, 100k, within 50M shared): **$0.00 marginal**.
- **Subtotal: ~$1.56 / month per paying tenant** at this size.

A 10× tenant (500 GB, 10M reads, 1M writes) lands around **$15.10/mo direct cost**, still well under any pricing point that includes a paid Worker base ($5/mo amortized).

**Implication.** Variable cost per paying tenant at v0.1 footprint is **single-digit dollars/mo** even with generous limits. The constraint on pricing is **not cost recovery** — it is value capture + ability to fund support + ability to fund the next platform investment (BYOK, dedicated tenants).

---

## §4 — Value framing (CI-minutes-saved → $)

The buyer doesn't pay for storage. They pay for the CI minutes that CoreLink eliminates.

### Anchor: GitHub Actions Linux runner pricing
- $0.008/min for `ubuntu-latest` (per GitHub-hosted runner billing page; standard public number).
- A medium build pipeline: 20 min/run × 30 runs/day × 22 working days = **13,200 min/mo per repo** = **$105.60/mo** in raw runner cost (ignoring matrix multipliers).
- A team of 5 devs sharing one heavy repo with the same cadence: same $105.60 (it's per-repo, not per-dev).
- Mac/Windows runners are 10×/2× respectively — for any team using `macos-latest`, the per-repo number jumps to **$1,056/mo** quickly.

### CoreLink cache-hit math
- Realistic warm-cache hit rate on a healthy Bazel/Turborepo monorepo: **40–70%** of work units (Bazel papers + Turborepo case studies cluster here).
- Conservative 40% hit reduces 13,200 min → 7,920 min → **$42/mo saved** per Linux repo at standard runner cost.
- Aggressive 70% hit on a Mac-heavy repo: **$739/mo saved**.

### Plus: developer-time savings
- 30-min CI feedback collapsed to 8 min × 30 runs/day = 11 hours/day of context-switch latency removed across a team.
- At $100/hr fully-loaded engineer cost, that's **~$11k/mo** of attention-debt eliminated per active dev team.
- This number is the marketing headline, not the pricing anchor (too soft to defend in a buyer conversation), but it's what makes a $20–$50/mo price feel free.

### Pricing-anchor takeaway
The honest defensible value floor for an active small-team user is **~$40/mo of GH Actions cost displaced** on Linux, scaling to **$100s/mo** on Mac/Windows. Any CoreLink price below ~$30/mo for Pro is unambiguously a no-brainer on TCO. The Free tier exists to prove the cache-hit math; Pro exists to capture the team-scale customer once they've measured the win.

---

## §5 — RECOMMENDED CoreLink pricing v0.1

Solo-founder constraint: **one pricing page, three tiers**, no usage rate-card, no per-seat. Designed to be implementable in one Stripe checkout + one webhook + one quota-meter Worker.

### Free — "Starter"
- **$0/mo.** No credit card.
- **10 GB cache storage** (matches CF R2 free tier exactly; pass-through cost ceiling).
- **500k cache requests/mo** (≈ generous personal-project ceiling).
- **1 workspace**, unlimited collaborators on that workspace.
- All core APIs: CAS upload/download, manifests, scopes, presigned URLs.
- Public + private artifacts.
- Community support (GitHub Discussions).
- **Hard limit at 10 GB / 500k req**: writes return `429 Quota Exceeded` with upgrade hint. No silent overage charge.

### Pro — "Team"
- **$25/mo** (or **$250/yr — 2 months free**, ~17% off, matching Plausible/BetterStack convention).
- **500 GB cache storage** included.
- **20M cache requests/mo** included.
- **Unlimited workspaces**, unlimited collaborators.
- Email support (target 1 business-day response).
- Overage policy at v0.1: **soft cap with email warning at 80%, hard cap at 100%** (no overage billing until v0.2). Forces upgrade conversation rather than surprise bills — the Sentry/Plausible playbook.
- 99.5% uptime target (informal — no SLA credit at this tier).

### Enterprise — "contact us"
- Custom pricing. Gates that justify the conversation:
  - **BYOK** (µK-driven, per Wave 33 spec) — bring-your-own-R2 + bring-your-own-KMS.
  - **Dedicated tenant** (isolated Worker namespace, custom subdomain).
  - **99.9% SLA with credits**.
  - **SSO/SAML** (Clerk org-mode).
  - **Audit log export** (S3/GCS sink, beyond the 90-day in-app retention).
  - **DPA + custom MSA**.
- Pricing range to anchor the first 3 sales calls: **$500–$5,000/mo** depending on storage + dedicated infra. Below $500/mo, redirect to Pro.

### Why this specific shape
- Free tier matches the **CF R2 free tier exactly** — zero marginal cost on the free user, so the free tier is sustainable indefinitely and can be marketed as "actually free forever," not a hidden trial.
- $25/mo Pro lands inside the **$20–$30 solo-founder anchor window** (Resend $20, Garnix $25, BetterStack $25, Sentry $26). This is buyer pattern recognition.
- 500 GB / 20M req at $25 = a gross margin of ~**75–90%** at the per-tenant cost from §3 (worst case 500 GB + 10M reads ≈ $15 direct cost vs. $25 revenue = $10 gross). Healthy enough to fund support + reinvest without venture-scale pressure.
- No per-seat, no per-build, no rate card. **One number** ($25). **Two limits** (GB, req). A buyer can decide in 30 seconds.
- Enterprise gates intentionally align to **Wave 33's µK/BYOK architecture** — the platform investment is also the enterprise upsell.

---

## §6 — A/B test ideas to find PMF pricing

Cheap experiments that don't require rebuilding the billing stack. Each is binary and reversible.

1. **Pro price point: $19 vs. $25 vs. $29.** Split landing-page traffic three ways for 4 weeks. Measure conversion-to-paid. $19 catches the Plausible Business anchor; $29 catches the "premium" anchor; $25 is the middle. Likely answer: $25 wins because it signals "team tool" not "indie tool." But measure.
2. **Free-tier ceiling: 10 GB vs. 25 GB vs. 50 GB.** Hypothesis: larger free tier → faster word-of-mouth + bigger funnel, at no marginal cost (CF R2 free is 10 GB, but the marginal $0.015/GB at 50 GB is $0.60/mo per free user — still rounding error). Measure: paid conversion at 90 days from each cohort.
3. **Annual discount: "2 months free" vs. "20% off".** Plausible's "2 months free" is the dominant phrasing; "20% off" is more legible. Probably converts the same; the test costs nothing.
4. **Pro overage policy: hard cap vs. $0.05/GB overage.** Hypothesis: hard cap drives more upgrades to Enterprise conversations (good); overage billing is more buyer-friendly but kills upgrade velocity. Test in cohorts of 50.
5. **Tier name: "Pro" vs. "Team" vs. "Build".** Surfacing test — does naming change conversion? Sentry uses "Team," Resend uses "Pro," Garnix uses "Individual." For CoreLink, "Team" maps to the buyer (a team) and is recommended; "Pro" maps to the user. Easy 2-week test.
6. **Show vs. hide enterprise price floor.** Some buyers self-disqualify when they see "$500/mo starting." Others self-qualify and skip the discovery call. Test in the second quarter once 3 enterprise deals exist for a ground truth.

**Discipline:** run **one** test at a time, 4-week windows, minimum n=200 trial signups before deciding. Without that volume, every "test" is noise.

---

## §7 — What to avoid (anti-patterns)

1. **Per-seat pricing pre-PMF.** Per-seat is administratively expensive (who counts as a seat? what about CI service accounts?) and creates an adoption tax exactly when the buyer is trying to spread the tool. Nx Cloud is the only direct comparable that charges per-seat and it's a tiny line item. **CoreLink should not introduce per-seat before $1M ARR.**
2. **Complex usage rate-card before billing infra exists.** Tinybird-style $0.0002/sec works for funded teams with finance buyers; it is the wrong shape for the first 50 customers of a solo-founder product. A buyer who needs a spreadsheet to estimate next month's bill **will not buy**.
3. **No-free-tier launch.** PlanetScale killed its free tier and gifted the developer mindshare to Neon/Supabase. The free tier is the funnel; without it there is no funnel.
4. **Hidden overage billing.** Don't let the bill silently grow 5×. Hard cap + email warning + upgrade CTA is the right v0.1 stance. Add metered overage in v0.2 only after the buyer trusts the meter.
5. **"Contact sales" as the only path to a real product.** EngFlow and Lago both gate everything behind sales. That works only if the buyer is concentrated (Bazel mega-monorepos / billing-platform engineers). CoreLink's buyer is diffuse — gating kills the funnel.
6. **More than three tiers.** Plausible has four because pageview metering forces it; for CoreLink there is no such forcing function. Each extra tier doubles the cognitive cost of the pricing page and halves the conversion rate.
7. **Founder-set prices that nobody validates.** This document recommends $25/Pro because it's defensible against six comparables, but the right next step is the §6 test #1, not "ship $25 and call it done."
8. **Free tier so generous it cannibalizes Pro.** 10 GB / 500k req is the right size: enough to prove the cache-hit math on a real personal project, too small to run a team's CI on. Resist pressure (from oneself) to push the free tier higher to "drive adoption" — that's how you build a free product, not a business.

---

## §8 — Next actions

1. **Wire `apps/docs/docs/pricing.mdx`** with the §5 structure (currently the page is a stub per the Launch Readiness audit `2026-05-27-launch-readiness-check.md`).
2. **Stand up Stripe** with three products: `corelink-free`, `corelink-pro-monthly` ($25), `corelink-pro-annual` ($250). Defer Enterprise to manual invoice for the first 3 deals.
3. **Wire the quota-meter Worker** to enforce the 10 GB / 500k-req free cap and 500 GB / 20M-req Pro cap. Storage check daily from D1; request count from KV counter.
4. **Email template** for the 80% warning + the 100% hard-cap notification.
5. **Schedule the §6 test #1 ($19 vs. $25 vs. $29)** for the first 4 weeks post-launch; commit to picking by week 5.

---

## §9 — Sources (URLs fetched 2026-05-27)

- BuildBuddy — https://www.buildbuddy.io/pricing
- EngFlow — https://www.engflow.com/product/pricing
- Nx Cloud — https://nx.dev/pricing
- Vercel Remote Cache — https://vercel.com/docs/monorepos/remote-caching
- Garnix — https://garnix.io/pricing/
- Plausible — https://plausible.io/#pricing
- Tinybird — https://www.tinybird.co/pricing
- Lago — https://www.getlago.com/pricing
- PlanetScale — https://planetscale.com/pricing
- Resend — https://resend.com/pricing
- Sentry — https://sentry.io/pricing/
- BetterStack Uptime — https://betterstack.com/uptime/pricing
- Cloudflare R2 pricing — https://developers.cloudflare.com/r2/pricing/
- Cloudflare Workers pricing — https://developers.cloudflare.com/workers/platform/pricing/
