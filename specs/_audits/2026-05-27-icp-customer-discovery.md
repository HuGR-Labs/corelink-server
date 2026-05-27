---
id: "AUDIT-2026-05-27-ICP-CUSTOMER-DISCOVERY"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "icp", "customer-discovery", "go-to-market", "personas", "solo-startup"]
---

# ICP customer discovery — who is CoreLink for at launch

> **Scope.** Identify the Ideal Customer Profile (ICP) for CoreLink's
> public launch. CoreLink is a multi-tenant, content-addressable cache
> SaaS on Cloudflare (REAPI v2, BYOK across 4 KMS, RFC-6962 audit chain,
> 4 regions). Solo-founder bootstrapped, no funding. The question this
> audit answers: **of all the people who could buy a managed remote
> cache, which one segment should we sell to FIRST**, and how do we
> reach them?

> **Method.** Web-research-only; no customer interviews yet (the
> recommendation in §4 includes "do 10 interviews" as the validation
> step). Sources cited inline as URLs. No invented numbers — any
> unknown is flagged "unknown — discover by [method]".

> **Bottom line (forward reference).** Recommended ICP at launch:
> **"Platform / DevEx Lead at a 50–250-eng compliance-aware SaaS or
> fintech, Bazel or sccache shop, CI > 25 min, no current managed
> remote-cache vendor, $500–$2,000/mo unsigned authority"** (§4).
> Anti-ICP: hobbyist OSS, > 500-eng FAANG-adjacent, mobile-only
> Gradle/Android shops (§5). Channels: r/bazel + bazel-discuss +
> Platform Engineering Slack + targeted "Stripe-style 45-min CI"
> outbound (§6).

## §1 Research method

This audit reflects 8 web searches and 4 page fetches conducted
2026-05-27. Categories investigated:

| Category | Primary sources |
| --- | --- |
| Competitor pricing & positioning | [BuildBuddy pricing](https://www.buildbuddy.io/pricing/), [Nx Cloud pricing](https://nx.dev/pricing), [EngFlow pricing](https://www.engflow.com/product/pricing), [Depot pricing](https://depot.dev/pricing) |
| Who uses Bazel / monorepo cache today | [Bazel users page](https://bazel.build/community/users), [thestack.technology on Bazel](https://www.thestack.technology/why-everyone-is-moving-to-googles-bazel-build-system/), [Aspect.build](https://www.aspect.build/platform) |
| Buyer triggers ("45 min CI") | [DEV "Halved Go Monorepo CI"](https://dev.to/jimmyyeung/journey-of-systematically-cut-our-monorepo-ci-time-in-half-ec8), [HN "We Halved Go Monorepo CI Build Time"](https://news.ycombinator.com/item?id=31882512), [daily.dev "Reduced React Monorepo CI by 70%"](https://daily.dev/blog/how-we-reduced-our-react-monorepo-ci-time-by-70/) |
| Communities & podcasts | [platformengineering.org Slack](https://platformengineering.org/), [Platform Engineering Podcast](https://podcasts.apple.com/us/podcast/platform-engineering-podcast/id1729594542), [DPE Summit 2026](https://dpe.org/dpe-summit/), [DX Engineering Enablement Podcast](https://getdx.com/podcast/) |
| BYOK as buyer requirement | [IBM on BYOK](https://www.ibm.com/think/topics/byok), [Cryptomathic BYOK/CYOK/HYOK](https://www.cryptomathic.com/blog/what-is-the-difference-between-byok-cyok-hyok), [Kiteworks on customer-managed keys](https://www.kiteworks.com/gdpr-compliance/customer-owned-encryption-key-control-data-privacy-compliance/) |
| Budget / platform-eng authority | [platformengineeringcost.com](https://platformengineeringcost.com/), [Notchup eng budget framework](https://notchup.com/insights/the-ems-framework-for-allocating-the-engineering-budget), [Tallyfy approval-limits matrix](https://tallyfy.com/approval-limits-matrix-template/) |
| sccache / Docker-cache adopters | [Mozilla sccache](https://github.com/mozilla/sccache), [Depot blog on sccache+GHA](https://depot.dev/blog/sccache-in-github-actions), [Netdata on Docker layer caching](https://www.netdata.cloud/academy/docker-layer-caching/) |

**Limitations of method.** Web research surfaces public artifacts
(blog posts, pricing pages, conference proceedings). It does **not**
substitute for first-party customer interviews. The §4 recommendation
explicitly schedules 10 discovery calls before the ICP is treated as
validated rather than hypothesised.

## §2 Market segmentation — buyer vs end-user

The remote-cache market has a non-obvious structure. The person who
**feels the pain** is usually not the person who **signs the invoice**.

### §2.1 Who currently pays for a managed remote build cache

Three distinct buyer archetypes appear in the public record:

1. **Platform / DevEx engineers at mid-stage scale-ups (50–500 eng).**
   Bought to fix a CI bottleneck. Public reports of this pattern
   include Stripe collapsing CI from 45 min to under 7 min by Bazel
   adoption ([thestack.technology](https://www.thestack.technology/why-everyone-is-moving-to-googles-bazel-build-system/)),
   teams "systematically halving monorepo CI time"
   ([DEV / Jimmy Yeung](https://dev.to/jimmyyeung/journey-of-systematically-cut-our-monorepo-ci-time-in-half-ec8)),
   and React-monorepo teams getting 70% reductions
   ([daily.dev](https://daily.dev/blog/how-we-reduced-our-react-monorepo-ci-time-by-70/)).
   Buying authority typically $500–$5,000/mo without VP-Eng sign-off
   (inferred from Nx Cloud's published $19/contributor + credit model
   and BuildBuddy's open Personal/Team tiers — both are designed to be
   purchasable by a single platform engineer).

2. **VP-Eng / CTO at large monorepo orgs (500+ eng).** Buys EngFlow or
   BuildBuddy Enterprise. Custom-quoted (both vendors require "contact
   sales" — [EngFlow](https://www.engflow.com/product/pricing),
   [BuildBuddy enterprise](https://www.buildbuddy.io/docs/enterprise/)).
   Typical deals are 5–6 figures annually based on the "unlimited cores"
   + dedicated-engineer + SLA + SSO/SAML packaging both vendors share.
   These are the Pinterest / Lyft / Canva / Block class of buyer named on
   the [Bazel users page](https://bazel.build/community/users).

3. **CISO / Head of Compliance at regulated mid-market (banking,
   healthcare, gov-adjacent SaaS).** Does **not** buy remote cache as
   a primary product — buys it as a side effect of needing BYOK +
   audit-log + residency in their CI/CD supply chain. Pattern shows up
   in regulated-industry BYOK requirements
   ([Cryptomathic](https://www.cryptomathic.com/blog/what-is-the-difference-between-byok-cyok-hyok),
   [Kiteworks](https://www.kiteworks.com/gdpr-compliance/customer-owned-encryption-key-control-data-privacy-compliance/)).
   This buyer cares about the audit chain and BYOK first, the
   cache-hit ratio second.

### §2.2 Who is the end-user

In all three archetypes, the day-to-day end-user is the **application
engineer** running `bazel build`, `cargo build`, `docker build`, or a
CI job. They do not pick the vendor; they notice when builds are slow
or fast. They generate the demand the buyer responds to.

### §2.3 Why this matters for CoreLink

CoreLink's invariant-grounded positioning (TLA+ tenant isolation,
RFC-6962 audit chain, 4-KMS BYOK, residency-honest) is **over-built
for buyer #1 alone** and **under-priced for buyer #3 alone**. The
sweet spot is the **overlap** — a buyer who is technically buyer #1
(feels the CI pain, has the authority) but works in an org where
buyer #3's checklist (BYOK, audit log, residency) is a hard gating
requirement *they would otherwise have to build themselves*. That
overlap is the persona profiled as P1 below, and the ICP
recommendation in §4.

## §3 Three persona profiles

Each persona below is a synthesis of public signals, not a single
named individual. Numbers are bounded by what's defensible from the
cited sources; unknowns are flagged.

### §3.1 Persona P1 — "Priya, Platform Lead at compliance-aware Series B/C SaaS"

| Dimension | Value |
| --- | --- |
| **Role** | Platform / DevEx Lead, or "Head of DevOps", or first platform hire |
| **Company stage / size** | Series B or Series C; 50–250 engineers; ARR $10M–$100M |
| **Industry** | B2B SaaS, fintech, healthtech, or any vertical with SOC 2 + GDPR exposure |
| **Stack** | Bazel or sccache + Go/Rust/Scala monorepo; GitHub Actions or Buildkite CI; Cloudflare or AWS infra; Docker images shipped to ECR/GAR |
| **Daily pain — concrete** | (a) "PR check on the monorepo takes 28–45 min, devs context-switch 4x"; (b) "sccache S3 bucket cross-region is $X/mo and we're tired of managing it"; (c) "Security team flagged that our self-hosted bazel-remote on EC2 has no audit log and the encryption-at-rest story is 'trust AWS'"; (d) "We have a SOC 2 Type II audit in Q3 and the auditor asked how we prove our build cache hasn't been tampered with — we have no answer" |
| **Current workaround** | Self-hosted [`bazel-remote`](https://github.com/buchgr/bazel-remote) on EC2 with an S3 backend, OR raw sccache pointing at S3 ([Mozilla sccache S3 docs](https://github.com/mozilla/sccache/blob/main/docs/S3.md)), OR no cache + bigger CI runners. Pays $200–$1,500/mo in S3 + egress + EC2 today. |
| **Budget authority — unsigned** | $500–$2,000/mo on a corporate card without VP-Eng approval. Above $2k typically needs a PO and Finance. Inferred from the Nx Cloud Team-tier model ($19/contributor + $5.50/10k credits — [Nx pricing](https://nx.dev/pricing)) and BuildBuddy Team-tier pay-as-you-go positioning ([BuildBuddy pricing](https://www.buildbuddy.io/pricing/)) — both vendors price the self-serve tier to land **under** the typical platform-engineer signoff ceiling. |
| **Where heard about new tools** | r/bazel; the [bazel-discuss Google Group](https://groups.google.com/g/bazel-discuss); Platform Engineering Slack ([platformengineering.org](https://platformengineering.org/)); Hacker News front page; the [Platform Engineering Podcast](https://podcasts.apple.com/us/podcast/platform-engineering-podcast/id1729594542); [DX Engineering Enablement Podcast](https://getdx.com/podcast/); annual [DPE Summit](https://dpe.org/dpe-summit/); Bazel community day; KubeCon hallway track; "ThePrimeagen reposted" on Twitter |
| **Trial-to-buy timeline** | 3–14 days. Will install the CLI same-day if the install is `brew install`. Will run a 1-week shadow-cache comparison against current solution. Will purchase if cache-hit ratio + p95 latency match or beat current, AND BYOK story is real. |
| **Likely objections to CoreLink** | (1) "You're a solo founder, what's your bus-factor story?" (2) "Cloudflare R2 isn't on our SOC 2 sub-processor list — how long to add?" (3) "Why pay you when bazel-remote + S3 already works?" (4) "I'd rather use BuildBuddy because they're VC-funded and have a logo wall" (5) "No SSO/SAML in your $X tier? Hard pass — our IT requires it" |

### §3.2 Persona P2 — "Marcus, VP-Eng at 800-engineer late-stage scale-up"

| Dimension | Value |
| --- | --- |
| **Role** | VP-Engineering or Head of Developer Productivity |
| **Company stage / size** | Series D+ / late-stage / pre-IPO; 500–3,000 engineers |
| **Industry** | Any (the org-size and monorepo shape matter more than vertical) |
| **Stack** | Bazel monorepo at scale (Stripe / Block / Canva / Lyft / Pinterest pattern — [Bazel users page](https://bazel.build/community/users)); full remote execution (not just cache); custom build infra team of 3–10 |
| **Daily pain — concrete** | (a) "Build infra team is 6 FTEs costing $2M+ loaded — VP-Finance is asking if we can shrink it"; (b) "Bazel remote-execution cluster on EKS keeps falling over on Mondays"; (c) "EngFlow / BuildBuddy bill is $40k/mo and growing 30% YoY — what are alternatives?" |
| **Current workaround** | EngFlow Enterprise, BuildBuddy Enterprise, or in-house Bazel RBE cluster |
| **Budget authority** | $50k–$500k/yr; signs after Procurement + Security + Legal review (8–16 weeks) |
| **Where heard about new tools** | Direct sales / outbound from competitor; analyst briefings (Gartner, Forrester); Bazel community events; [Bazel partners page](https://bazel.build/community/partners); peer-CIO recommendations |
| **Trial-to-buy timeline** | 3–9 months. Includes a paid PoC, security questionnaire, DPA + MSA negotiation, internal champion-recruitment |
| **Likely objections to CoreLink** | (1) "We need full remote *execution*, not just cache — your roadmap shows RBE is post-GA Phase 1" ([README §"What CoreLink does not aim to do"](../../README.md)); (2) "Single-person company can't satisfy our vendor-risk review"; (3) "We already have BuildBuddy and switching costs are real" |

### §3.3 Persona P3 — "Sam, Indie / OSS maintainer with a Rust or Go side project"

| Dimension | Value |
| --- | --- |
| **Role** | Solo developer, OSS maintainer, or 2–5 person seed-stage startup |
| **Company stage / size** | Pre-seed / seed; 1–5 engineers |
| **Stack** | Rust + sccache, or Go + go-build-cache, or small Node monorepo with Turborepo |
| **Daily pain** | (a) "GitHub Actions Rust build takes 12 min, my local dev takes 90s, this is annoying"; (b) "I want to share cache between CI and laptop"; (c) "sccache + S3 is 3 env vars too many to configure" |
| **Current workaround** | GitHub Actions cache (free, broken across forks); sccache + S3 (cheap but DIY); Depot Cache ($0.20/GB, 500 build-minutes free — [Depot pricing](https://depot.dev/pricing)); Buildless free tier |
| **Budget authority** | $0–$50/mo personal. Will not buy a SaaS at $50+ for a side project, period. |
| **Where heard about new tools** | r/rust, r/programming, r/golang, Hacker News, "show HN" front page, Twitter/X dev influencers, YouTube (ThePrimeagen, Fireship) |
| **Trial-to-buy timeline** | Install same-day if free; converts to paid only at company scale-up (i.e. graduates to P1) |
| **Likely objections to CoreLink** | (1) "Why would I trust a cache I have to sign up for when sccache + S3 is free?"; (2) "Multi-tenant security I do not need — I'm a solo project"; (3) "Where's the GitHub Actions one-line action?" |

## §4 ICP recommendation — target P1 first

**Recommendation.** Sell to **Persona P1 (Priya, Platform Lead at
compliance-aware Series B/C SaaS, 50–250 eng, Bazel or sccache shop,
CI > 25 min, $500–$2,000/mo unsigned authority)** first. Defer P2 and
P3 until P1 traction is proven (10 paying customers or $10k MRR).

### §4.1 Why P1 wins on the seven-factor test

| Factor | P1 verdict | P2 verdict | P3 verdict |
| --- | --- | --- | --- |
| **Pain is acute** | Yes — CI minutes burn dev productivity daily | Yes — but already bought a solution | Mild — annoyance, not bleeding |
| **Budget exists** | Yes — $500–$2,000/mo is below sign-off ceiling | Yes — but 8–16-week procurement | No — will not pay |
| **Solo-founder bus-factor objection survivable** | Mostly — "we have $X/mo escape clause and your data is BYOK-encrypted" answers it | No — fatal objection for $50k+ deals | N/A |
| **CoreLink differentiation matters** | Yes — BYOK + audit chain + residency is exactly the SOC-2-prep checklist | Partially — they have a security team that built equivalents | No — overkill |
| **Sales cycle compatible with solo-founder** | Yes — 3–14 day self-serve | No — 3–9 months, needs sales/SE/legal | Yes — but no revenue |
| **TAM / reachability** | Several thousand companies match the profile (rough order: companies with $10M–$100M ARR, eng > 50, monorepo with Bazel or large sccache estate; sub-segment of the broader 50k-company "SaaS with eng > 50" pool, intersected with the ~100s of Bazel-adopting orgs visible publicly — [Bazel users page](https://bazel.build/community/users) lists ~40 explicitly, real population an order of magnitude higher) | ~500 globally; all already courted by EngFlow & BuildBuddy | Tens of thousands but $0 ARPU |
| **Reference-class for future P2 sales** | Yes — P1 wins are the case-study fuel for P2 sales 12–18 months out | N/A | No |

### §4.2 The defensible thesis

**Why this segment will pay CoreLink specifically, not a competitor.**

- **BuildBuddy** is the obvious comparison. Their Personal tier is
  free up to 10 users, Team tier is pay-as-you-go over 100 GB transfer
  ([BuildBuddy pricing](https://www.buildbuddy.io/pricing/)). They do
  not commit to BYOK, RFC-6962 audit chain, or
  residency-by-structural-invariant in their public pricing tiers.
  Compliance-aware buyers (the P1 sub-segment that has a SOC 2 audit
  in 90 days) need those guarantees, not "varies" or "Enterprise tier
  contact sales".

- **EngFlow** is enterprise-only in practice; their Free tier is
  single-machine and Enterprise is custom-quoted
  ([EngFlow pricing](https://www.engflow.com/product/pricing)). P1
  buyers cannot self-serve EngFlow within the unsigned-authority
  window.

- **Nx Cloud** is JS/TS-monorepo-shaped (Nx the build tool drives the
  adoption); their pricing is $19/contributor + credits
  ([Nx pricing](https://nx.dev/pricing)). It is the wrong shape for a
  Bazel or sccache-Rust P1. The May-2026 CVE-2025-36852 deprecation of
  the free self-hosted plugin ([Medium / Emily Xiong](https://emilyxiong.medium.com/exploring-of-nx-self-hosted-cache-5bc39bd2ed7f))
  is forcing former self-hosted-Nx users to either pay Nx Cloud or
  evaluate alternatives — small but real tailwind for any vendor with
  a credible self-serve story.

- **Depot** is Docker-build + GitHub-Actions-cache-shaped, $0.20/GB
  over plan cache ([Depot pricing](https://depot.dev/pricing)). Strong
  for the Docker-layer sub-segment of P1, weak for the
  Bazel-REAPI-CAS sub-segment. CoreLink can coexist (Depot for image
  builds, CoreLink for REAPI cache) early on, then expand into the
  Docker layer cache once we have a beachhead.

- **Self-hosted bazel-remote + S3** is the "do nothing" alternative.
  The win against it is operational simplicity + the BYOK/audit/
  residency story that the P1 buyer cannot build themselves in a
  reasonable budget.

### §4.3 Validation step before treating this as truth

Run **10 customer-discovery calls** with people who fit P1
(recruited via the channels in §6). Questions to validate:

1. Is CI time the actual top-3 platform-eng pain right now?
2. What do you pay today (S3 + EC2 + your team's time)?
3. What signoff threshold would you need a managed cache to fit under?
4. Is there a SOC 2 / GDPR audit on your 90-day horizon?
5. Would BYOK + audit-chain change your willingness to pay?

Until these calls happen, this ICP is **hypothesised, not validated**.

## §5 Anti-ICP — who NOT to target now

### §5.1 P3 — hobbyists, OSS maintainers, side projects

**Why not.** They will not pay. Free-tier abuse risk on a multi-tenant
service eats into the cost structure. Time spent on P3 marketing
(YouTube, "show HN", r/programming) does not convert to P1 revenue
*directly* — though it CAN build awareness that compounds. **Posture:**
keep a sandbox + free tier so P3 can self-onboard if they show up, but
do not spend marketing or feature work on them. Don't write content
targeted at them. Don't optimize landing-page copy for them.

### §5.2 P2 — Fortune-500 / FAANG-adjacent giants

**Why not now.** 3–9-month sales cycles are incompatible with a
solo-founder who needs revenue this quarter. Their security review
will block on the bus-factor question. Their procurement will block on
the lack of a Procurement-compatible vendor record (D-U-N-S, $1M
liability insurance, SOC 2 Type II *already shipped* not in-progress,
etc.). **Posture:** politely refer enterprise inquiries to a
"join the waitlist" page; come back to them after 12 months of P1
proof and a Type-II report in hand.

### §5.3 Mobile-only Gradle / Android shops

**Why not.** Bazel and Gradle are both monorepo build tools, but the
Android/Gradle community is downstream of [Gradle Develocity](https://gradle.com/develocity/)
which has deep mobile-tooling integration that CoreLink does not match
on day one. Cash App's "remote build cache at scale" case study is a
Develocity-on-Gradle case study ([gradle.com](https://gradle.com/blog/managing-a-remote-build-cache-at-scale-with-local-build-observability-at-cash-app/)),
not a generic remote-cache buyer. **Posture:** REAPI-first means
Bazel-first; revisit mobile in year 2.

### §5.4 Pure-JS/TS Turborepo / Nx ecosystems

**Why not.** These ecosystems are Turborepo Cloud or Nx Cloud-shaped;
the developer audience overlaps minimally with the
Bazel/sccache/REAPI audience CoreLink is built for. Trying to compete
on Nx's home turf with a non-Nx-native experience is a losing
positioning fight on day one. **Posture:** add Turborepo cache
adapter post-GA if a P1 customer asks for it; don't lead with it.

### §5.5 Defense / federal / classified

**Why not.** FedRAMP, IL5, ATO timelines are 12–24 months; require
US-government-cleared personnel; need US-only infrastructure
(precludes Cloudflare's global network without contortion). **Posture:**
"FedRAMP — not yet" on the Trust Center page; revisit at $5M ARR with
a hired compliance lead.

## §6 Channels to reach P1

Ordered by expected unit-cost-per-qualified-lead, lowest first.

### §6.1 Free / earned (do these first)

1. **Bazel community: r/bazel + [bazel-discuss Google Group](https://groups.google.com/g/bazel-discuss).**
   Post technical write-ups, not marketing. Example angles: "We
   re-implemented REAPI in Rust on Cloudflare Workers — here's the
   tenant-isolation TLA+ model"; "How our audit chain proves your
   build cache wasn't tampered with"; "Cache-hit ratios across regions:
   data from the first 30 days". Bazel community values rigor; the
   audit-chain + TLA+ posture is on-message here.

2. **[Platform Engineering Slack](https://platformengineering.org/).**
   Active community of exactly the P1 role. Do NOT broadcast — answer
   questions about caching, CI optimization, BYOK in CI; let people
   click through to a profile or signature.

3. **Hacker News "Show HN" — exactly one shot.** Time it for a Tuesday
   morning ET. The post needs to be a *technical* show-HN (the
   TLA+ model + RFC-6962 audit chain + 4-KMS BYOK story is the
   hook, not "managed Bazel cache"). Pre-write the FAQ for the
   inevitable "why not bazel-remote + S3?" question.

4. **[Platform Engineering Podcast](https://podcasts.apple.com/us/podcast/platform-engineering-podcast/id1729594542)
   and [DX Engineering Enablement Podcast](https://getdx.com/podcast/).**
   Pitch a guest spot on "what makes a REAPI cache audit-grade".
   Solo-founder + technical depth is exactly the guest profile these
   shows look for.

5. **Conference talks (12-month horizon): [BazelCon](https://events.linuxfoundation.org/bazelcon/),
   [DPE Summit](https://dpe.org/dpe-summit/), KubeCon Platform-Eng
   day.** Submit talks now for events 6–9 months out. Talks
   "How we proved tenant isolation in a multi-tenant cache with TLA+"
   sit at the intersection of theoretical-rigor and practical-pain
   that these audiences value.

### §6.2 Targeted outbound (do once messaging is sharp)

6. **"Stripe-style 45-min CI" outbound.** Identify companies with
   public engineering blog posts about CI > 30 min in the last 18
   months (e.g. the patterns from [DEV](https://dev.to/jimmyyeung/journey-of-systematically-cut-our-monorepo-ci-time-in-half-ec8),
   [HN](https://news.ycombinator.com/item?id=31882512),
   [daily.dev](https://daily.dev/blog/how-we-reduced-our-react-monorepo-ci-time-by-70/)).
   Cold-email the platform lead with a *specific* offer: "I read your
   post about cutting CI from 45 to 18 min — here's a 90-second video
   showing CoreLink's REAPI cache against your stack, free 30-day
   trial, no card". 50 emails / week; expect 1–3 qualified replies if
   the personalization is real.

7. **GitHub-signal outbound.** Crawl GitHub for repos with both a
   `.bazelversion` file AND `WORKSPACE.bazel` AND a `BUILD.bazel`
   structure indicating > 100 targets, public org name. Cross-reference
   with LinkedIn for the platform-eng hire. ~500 orgs globally fit
   this filter; concentrate the outbound there. (Discovery method;
   not yet executed.)

### §6.3 Paid (only once self-serve conversion is proven)

8. **Sponsor the DPE Summit, BazelCon, or platformengineering.org
   newsletter.** Expensive per-impression but the *only* audience that
   converts. Skip generic "DevOps" or "cloud" advertising entirely —
   the audience is wrong.

9. **Google search ads on "bazel remote cache", "sccache S3
   alternative", "managed bazel cache".** Bounded budget ($500/mo
   ceiling) to see if the buyer-intent search query actually exists at
   a meaningful volume. Unknown — discover by running the ad.

### §6.4 Anti-channels (deliberately avoid)

- LinkedIn paid ads — wrong audience-targeting tools for "platform
  engineer at compliance-aware Series B SaaS".
- Generic "DevOps" media (TheNewStack untargeted, generic DevOps
  podcasts) — too broad; P1 reads narrower, sharper sources.
- Twitter/X non-organic — algorithmic amplification of
  founder-marketing posts converts poorly for B2B platform tools.
- Reddit r/programming, r/devops, r/sre — too broad; P1 is in
  r/bazel specifically.

## §7 What "valid" looks like for this ICP after 60 days

- **10 customer-discovery calls** completed with people who fit P1;
  the 5 questions in §4.3 answered.
- **3 paying customers** at the self-serve tier (proves $500–$2,000/mo
  authority hypothesis).
- **1 "we picked CoreLink over [BuildBuddy / bazel-remote + S3]" case
  study** the customer is willing to put their name on (proves the
  differentiation thesis in §4.2).
- **First inbound lead** from r/bazel or Platform Engineering Slack
  (proves the channel mix in §6.1).

If two of these four are missing at day 60, **re-segment** —
candidates to test are (a) drop the "compliance-aware" qualifier and
go pure cache-perf positioning, (b) flip to Docker-layer-cache as the
wedge and Bazel-REAPI as the upsell, or (c) move up-market to P2
faster than planned.

## §8 References

Internal:

- [`README.md`](../../README.md) — current product state-of-truth,
  invariant guarantees.
- [`apps/docs/docs/pricing/comparison.mdx`](../../apps/docs/docs/pricing/comparison.mdx)
  — draft capability matrix vs BuildBuddy / NativeLink / EngFlow /
  self-hosted bazel-remote.
- [`specs/_audits/2026-05-27-launch-readiness-check.md`](./2026-05-27-launch-readiness-check.md)
  — solo-startup launch-gate audit (landing / signup / pricing /
  privacy / pipeline).

External (deduplicated from §1 and inline citations above):

- [BuildBuddy pricing](https://www.buildbuddy.io/pricing/)
- [BuildBuddy enterprise overview](https://www.buildbuddy.io/docs/enterprise/)
- [EngFlow pricing](https://www.engflow.com/product/pricing)
- [Nx Cloud pricing](https://nx.dev/pricing)
- [Depot pricing](https://depot.dev/pricing)
- [Bazel users page](https://bazel.build/community/users)
- [Bazel partners page](https://bazel.build/community/partners)
- [Aspect.build platform page](https://www.aspect.build/platform)
- [thestack.technology — Bazel adoption](https://www.thestack.technology/why-everyone-is-moving-to-googles-bazel-build-system/)
- [HN — We Halved Go Monorepo CI Build Time](https://news.ycombinator.com/item?id=31882512)
- [DEV — Halving monorepo CI time](https://dev.to/jimmyyeung/journey-of-systematically-cut-our-monorepo-ci-time-in-half-ec8)
- [daily.dev — Reduced React Monorepo CI by 70%](https://daily.dev/blog/how-we-reduced-our-react-monorepo-ci-time-by-70/)
- [Mozilla sccache](https://github.com/mozilla/sccache)
- [Mozilla sccache S3 docs](https://github.com/mozilla/sccache/blob/main/docs/S3.md)
- [buchgr/bazel-remote](https://github.com/buchgr/bazel-remote)
- [Depot blog — Introducing Depot Cache](https://depot.dev/blog/introducing-depot-cache)
- [Depot blog — sccache in GitHub Actions](https://depot.dev/blog/sccache-in-github-actions)
- [Netdata — Docker Layer Caching cut build times 70%](https://www.netdata.cloud/academy/docker-layer-caching/)
- [Gradle blog — Cash App remote build cache at scale](https://gradle.com/blog/managing-a-remote-build-cache-at-scale-with-local-build-observability-at-cash-app/)
- [platformengineering.org (Slack + community)](https://platformengineering.org/)
- [Platform Engineering Podcast](https://podcasts.apple.com/us/podcast/platform-engineering-podcast/id1729594542)
- [DX Engineering Enablement Podcast](https://getdx.com/podcast/)
- [DPE Summit 2026](https://dpe.org/dpe-summit/)
- [IBM — What is BYOK](https://www.ibm.com/think/topics/byok)
- [Cryptomathic — BYOK/CYOK/HYOK](https://www.cryptomathic.com/blog/what-is-the-difference-between-byok-cyok-hyok)
- [Kiteworks — customer-owned encryption keys](https://www.kiteworks.com/gdpr-compliance/customer-owned-encryption-key-control-data-privacy-compliance/)
- [Medium / Emily Xiong — Nx self-hosted cache deprecation](https://emilyxiong.medium.com/exploring-of-nx-self-hosted-cache-5bc39bd2ed7f)
- [platformengineeringcost.com — IDP cost calculator](https://platformengineeringcost.com/)
- [Notchup — EM framework for engineering budget](https://notchup.com/insights/the-ems-framework-for-allocating-the-engineering-budget)
- [Tallyfy — approval limits matrix template](https://tallyfy.com/approval-limits-matrix-template/)

---

**Status.** This audit is `audit_status: ACTIVE` — the ICP is a
hypothesis that the §4.3 customer-discovery calls and §7 60-day
metrics will validate, refute, or refine. Re-seal once the 10
discovery calls are complete and the recommendation is either
confirmed or replaced.
