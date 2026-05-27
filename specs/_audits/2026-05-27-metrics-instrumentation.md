---
id: "AUDIT-2026-05-27-METRICS-INSTRUMENTATION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "metrics", "kpi", "instrumentation", "solo-startup", "launch", "analytics"]
---

# Metrics & Instrumentation Strategy (solo-founder launch)

> **Scope.** Define the KPI hierarchy, instrumentation plan, dashboard layout, review cadence, and pre-committed decision rules for the CoreLink public launch. Solo-operator constraint: the entire analytics stack must cost less than US$50/month and demand less than 30 minutes of human attention per week.
>
> **Method.** (a) Web-research benchmark scan for B2B-SaaS / dev-tool AARRR metrics and analytics-tool pricing (May 2026); (b) cross-reference of CoreLink production surface (Cloudflare Workers + D1 + Stripe + Clerk + Sentry, per `2026-05-27-launch-readiness-check.md`); (c) derivation of a three-tier KPI tree calibrated to a single-operator review budget.
>
> **Non-goals.** This audit does **not** install instrumentation, does **not** create dashboards, and does **not** modify product code. It produces the spec from which a future WI executes.

---

## §1 — Research summary

All benchmarks below are public as of May 2026 and were retrieved via WebSearch. Where two sources disagreed, the more conservative number was kept.

### §1.1 — Framework: AARRR (pirate metrics)
- Dave McClure (2007) — Acquisition / Activation / Retention / Referral / Revenue. Still the default lens for early-stage SaaS in 2026 because it maps 1:1 to the customer-journey funnel and yields exactly one number per stage. Source: [Purchasely — AARRR Complete Guide 2025](https://www.purchasely.com/blog/aarrr-framework-pirate-metrics-complete-guide-for-2025); [Statsig — Pirate Metrics](https://statsig.com/perspectives/pirate-metrics-startup-growth).
- North-Star variant: PostHog's founder guide recommends collapsing AARRR into one **North Star Metric** that "captures the value the product delivers" — for dev tools this is usually a usage-volume number, not revenue, because revenue lags weeks behind product behaviour. Source: [PostHog — Finding your North Star Metric](https://posthog.com/founders/north-star-metrics).

### §1.2 — Activation benchmarks (B2B SaaS, 2025)
- All-SaaS average activation rate (signup → first meaningful action) ≈ **37.5%**, median ≈ **30%**. Product-led ≈ 34.6%; sales-led ≈ 41.6%. Source: [AgileGrowthLabs — User Activation Rate Benchmarks 2025](https://www.agilegrowthlabs.com/blog/user-activation-rate-benchmarks-2025/).
- Industry spread is enormous: FinTech 5%, AI/ML 54.8%. Dev-infrastructure is not isolated in the sample but tracks closer to AI/ML because the evaluator IS the user.
- **Time-to-first-value benchmark:** industry average 36 h; elite products <5 min. Userpilot 547-company report: expected TTFV ≈ "1 day 12 h 23 min". Sources: [ProductQuant — 5-Minute Aha Rule](https://productquant.dev/blog/5-minute-aha-rule-optimize-ttv/); [Userpilot — Time to Value](https://userpilot.com/blog/time-to-value/).
- Users who hit aha-moment within 48 h are **3.4×** more likely to convert to paid; 69% correlation between strong 7-day activation and strong 3-month retention. Source: [Amplitude — Time to Value drives retention](https://amplitude.com/blog/time-to-value-drives-user-retention).

### §1.3 — Conversion benchmarks (free → paid)
- Median B2B SaaS trial-to-paid: **18.5%**. Top quartile 35-45%, elite 60%+. Source: [Pulseahead — Trial-to-Paid Benchmarks](https://www.pulseahead.com/blog/trial-to-paid-conversion-benchmarks-in-saas).
- Self-serve PLG **freemium**: typical 2-5%, freemium-visitor median ~12%, free-to-paid average ~9%. Opt-in (no CC) free trials: 8.9-18.2%. Opt-out (CC required) free trials: 31.4-48.8%. Source: [ProductLed — PLG Benchmarks](https://productled.com/blog/product-led-growth-benchmarks); [1Capture — Free Trial Benchmarks 2025](https://www.1capture.io/blog/free-trial-conversion-benchmarks-2025).
- Implication for CoreLink: a no-CC free tier with a usage-cap upsell will sit in the 5-10% F2P band by default. Hitting the top quartile requires deliberate activation engineering, not more traffic.

### §1.4 — Retention & churn benchmarks (B2B SaaS, 2025)
- Average monthly logo churn ≈ **3.5%**; best-in-class <1%; SMB 3-5%, mid-market 1.5-3%, enterprise 0.5-2%. Source: [Vena — 2025 SaaS Churn](https://www.venasolutions.com/blog/saas-churn-rate); [HubiFi — 2025 Churn Benchmarks](https://www.hubifi.com/blog/calculate-saas-churn-rate); [Optifai — B2B SaaS Churn](https://optif.ai/learn/questions/b2b-saas-churn-rate-benchmark/).
- Infrastructure / integrated products skew low because of switching cost. CoreLink (CI-cache integration with build/Docker/ML) is structurally on the sticky end.
- Cohort retention for dev infra typically lands at M1 ≈ 50-70% (active accounts), M3 ≈ 35-50%, M6 ≈ 25-40% for self-serve; closer to 60-80% at M6 for design-partner deals.

### §1.5 — PMF measurement (Sean Ellis)
- One question: *"How would you feel if you could no longer use [product]?"* — choices "Very disappointed / Somewhat disappointed / Not disappointed / N/A — no longer use it".
- **40% "very disappointed" = empirical PMF threshold** across hundreds of startups. Below 40% → fix product first, do not scale acquisition.
- Required panel: users who have used the core product **at least twice** in the **last two weeks**. Source: [Learning Loop — Sean Ellis Score](https://learningloop.io/glossary/sean-ellis-score); [GoPractice — PMF Survey](https://pmfsurvey.com/).

### §1.6 — Solo-founder revenue benchmarks
- 30% of micro-SaaS never reach $1k MRR; 50% plateau $1-10k MRR; 15% reach $10-100k; 5% exceed $100k. Median solo journey to $1M ARR ≈ 24 months. Source: [SoftwareSeni — Solo Founder SaaS Metrics](https://www.softwareseni.com/solo-founder-saas-metrics-from-0-to-10k-mrr-in-6-months-with-realistic-timelines/).
- Reading: $1k MRR by month 3 is an optimistic gate, not a baseline. Decision-rule §6 treats it as a *signal*, not a kill-switch.

### §1.7 — Analytics-tool pricing (May 2026)
- **PostHog**: free tier covers 1M events + 5K session recordings + 1M feature-flag requests + 100K error-tracking exceptions + 1.5K survey responses + 1M data-warehouse rows per month. Pay-as-you-go above that. Source: [PostHog Pricing](https://posthog.com/pricing).
- **Plausible**: from $9/mo (10k pageviews); no free tier. Source: [Plausible Pricing 2026](https://comparetiers.com/tools/plausible-analytics).
- **Fathom**: from $14/mo (100k pageviews).
- **Stripe Sigma**: $0.02/charge analysed (effectively free at our volume); built-in dashboards cover MRR/churn natively in 2026.
- **Sentry**: free tier 5k errors + 10k performance units / mo (sufficient for launch).
- The community-built "$50/mo solopreneur stack" pattern (PostHog free + Plausible $9 + Stripe built-in + Sentry free) is the de-facto reference. Source: [F³ Fund It — Solopreneur Analytics Stack 2026](https://f3fundit.com/the-solopreneur-analytics-stack-2026-posthog-vs-plausible-vs-fathom-analytics-and-why-you-should-ditch-google-analytics/).

---

## §2 — CoreLink KPI tree

### §2.1 — Tier 1: North Star Metric (one number)

**NSM = Weekly Cache-Hit-Producing Accounts (W-CHPA)**
*Defined as: distinct paying-or-free tenants whose API key produced ≥1 successful cache HIT (not miss, not put) in the trailing 7 days.*

| Candidate                                    | Pros                                                                 | Cons                                                                                                   | Verdict |
| -------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ------- |
| Total cache hits / day                       | Trivial to count; large number motivates                             | Pumps with a single noisy CI loop; doesn't reflect customers, reflects traffic                          | NO      |
| WAU (any auth'd request)                     | Standard                                                             | A `GET /health` from a dashboard counts; doesn't reflect product VALUE delivered                        | NO      |
| Active paying customers                      | Direct revenue link                                                  | Lags weeks; near-zero variance for first 90 days; doesn't tell Gustavo what to fix                      | NO      |
| Successful CI runs that hit cache            | Closest to the buyer's outcome                                       | Requires CI-side telemetry we don't own; would need SDK reporter                                        | LATER   |
| **W-CHPA**                                   | **Captures value delivered + multi-tenant fan-out + leads revenue**  | Slightly more complex SQL than raw count                                                               | **YES** |

**Why W-CHPA wins.** It is a *paid-action equivalent* (a cache HIT is the moment CoreLink earned its existence for that tenant that week), it filters out tyre-kickers (signup without ever wiring up a real CI), and it scales by tenants not by individual users (correct unit for a B2B-team product). It is also a single D1 query, no external dependency.

**Definition of "successful cache HIT":** HTTP 200 on `GET /v1/cache/{key}` or `HEAD /v1/cache/{key}` where the response served bytes from the content store (i.e. hit, not miss/304/404). Excludes admin-UI and dashboard traffic via API-key class flag.

**Year-1 NSM targets (solo, conservative):**
- Month 1 post-launch: 10 W-CHPA
- Month 3: 30 W-CHPA
- Month 6: 100 W-CHPA
- Month 12: 300 W-CHPA

These are deliberately *small* — they assume design-partner-grade onboarding, not paid acquisition. Beating them = re-plan.

---

### §2.2 — Tier 2: Input metrics (the 6 that drive Tier 1)

Numbered T2-01 through T2-06. Each one has a direct causal arrow into W-CHPA.

| # | Metric | Definition | Cadence | Healthy band | Source |
| - | ------ | ---------- | ------- | ------------ | ------ |
| T2-01 | Signups / week | Distinct new tenants created via Clerk sign-up | Weekly | Trending up; absolute target M3 = 10/wk | Clerk + D1 `tenants` table |
| T2-02 | Activation rate | % of new tenants that produce ≥1 cache HIT within 7 days of signup | Weekly cohort | ≥30% (median), aim 50%+ (dev-tool advantage) | D1 join: `tenants.created_at` × `cache_events.first_hit_at` |
| T2-03 | Time-to-first-HIT (TTFH) | Median minutes from signup to first successful cache HIT | Weekly cohort | <60 min good, <10 min elite | D1 query as T2-02 |
| T2-04 | Cache-hit rate per active tenant | Mean of (hits / (hits+misses)) per tenant per week, weighted by request volume | Weekly | ≥70% = product working; <50% sustained = bug or mis-config | D1 `cache_events` aggregation |
| T2-05 | D7 retention | % of new tenants from week N that produce ≥1 HIT in week N+1 | Weekly cohort | ≥40% acceptable, ≥60% strong | D1 cohort SQL |
| T2-06 | MRR & free→paid % | Stripe MRR + % of activated free tenants that became paid within 30 days | Monthly | F2P ≥5% baseline, ≥10% strong; MRR — see §6 decision rules | Stripe Sigma / built-in dashboard |

Two notable *omissions*:
- **Acquisition channel breakdown** is *not* a Tier-2 metric. With <50 signups/week the per-channel sample size is statistical noise. Promote to Tier 2 only when total signups >100/wk.
- **NPS** is not Tier 2. See §6.5 — Sean Ellis PMF score is run quarterly instead; NPS is deferred until ≥50 active tenants.

---

### §2.3 — Tier 3: Diagnostic metrics (only when something is wrong)

These are **not on the dashboard**. They live in Sentry / Cloudflare Analytics and are consulted *reactively* when a Tier-1 or Tier-2 number breaks.

| # | Metric | Tool | Triggered by |
| - | ------ | ---- | ------------ |
| T3-01 | Per-endpoint p50/p95/p99 latency | Cloudflare Workers Analytics + Sentry performance | T2-04 cache-hit rate drops OR support ticket about "slow" |
| T3-02 | Per-tenant 5xx rate | Cloudflare Analytics + structured log query | T2-04 drop, or weekly alert if >0.1% sustained |
| T3-03 | Per-route 4xx counts (esp 401/403/429) | Cloudflare Analytics | T2-02 activation drop (likely auth/quota friction) |
| T3-04 | Stripe webhook failures | Stripe dashboard → email | Real-time alert (already wired) |
| T3-05 | Clerk auth error rate | Clerk dashboard | T2-01 signups drop |
| T3-06 | D1 query latency / R2 PUT errors | Cloudflare logs | T2-04 drop or T3-01 spike |
| T3-07 | Drata control-failure count | Drata dashboard | Daily auto-alert (already configured per `drata-fail-closed-fix.md`) |
| T3-08 | Background-job lag (audit, billing aggregator) | D1 `job_queue` table | T2-06 MRR mismatch with raw usage |

**Rule:** if a Tier-1 or Tier-2 number breaks the §6 bands, the first move is to inspect the relevant Tier-3 metric. Tier-3 numbers should never become weekly review items — they are tools, not targets.

---

## §3 — Instrumentation plan (per KPI)

Legend: **Source** = where the underlying event/row lives; **Tool** = where Gustavo sees the aggregated number; **Cost** = incremental US$/mo; **Setup** = one-shot human-hours.

### Tier 1
| KPI | Source | Tool | Cost | Setup |
| --- | ------ | ---- | ---- | ----- |
| W-CHPA | D1 `cache_events(tenant_id, ts, outcome)` table (need column `outcome ∈ {hit, miss, put, error}`) | D1 SQL dashboard (Cloudflare native) + weekly cron→email digest | $0 | 4 h (add `outcome` column if missing + index on `(tenant_id, ts)` + write the SQL view + cron worker that emails) |

### Tier 2
| KPI | Source | Tool | Cost | Setup |
| --- | ------ | ---- | ---- | ----- |
| T2-01 Signups/wk | Clerk webhook → D1 `tenants` | D1 SQL view in weekly digest | $0 | 1 h (webhook already exists; query only) |
| T2-02 Activation rate | D1 join `tenants` × `cache_events` | D1 SQL view | $0 | 2 h |
| T2-03 TTFH median | Same D1 join | D1 SQL view | $0 | 1 h |
| T2-04 Cache-hit rate | D1 `cache_events` agg | D1 SQL view | $0 | 1 h |
| T2-05 D7 retention | D1 cohort SQL | D1 SQL view (parameterised by week) | $0 | 3 h (cohort SQL is fiddly; do once, save) |
| T2-06 MRR & F2P% | Stripe (native) + Stripe ↔ D1 tenant join for F2P% | Stripe dashboard (MRR) + D1 view (F2P%) | $0 | 1 h |

### Tier 3
| KPI | Source | Tool | Cost | Setup |
| --- | ------ | ---- | ---- | ----- |
| T3-01..03, T3-06 | Cloudflare Workers Analytics + Logpush | Cloudflare dashboard | $0 (within free tier) | 0 h — already on |
| T3-04 | Stripe | Stripe email alert | $0 | already on |
| T3-05 | Clerk | Clerk dashboard | $0 | already on |
| T3-07 | Drata | Drata dashboard + email | (existing) | already on |
| T3-08 | D1 `job_queue` | D1 SQL ad-hoc | $0 | 1 h to write the query |

**Marketing-site analytics (Plausible)** — separately needed for landing page conversion funnel (`/` → `/pricing` → `/sign-up`). Recommended once landing page exists (currently RED per launch-readiness audit).

| KPI | Source | Tool | Cost | Setup |
| --- | ------ | ---- | ---- | ----- |
| Landing visits, /pricing CTR, /sign-up CTR | `corelink-docs.humangr.com` (Docusaurus) | Plausible | **$9/mo** | 1 h (script tag + 3 goals) |

**Optional later: PostHog** (free tier) — only if Tier-2 metrics show an activation-funnel problem we cannot diagnose from D1 alone (e.g. "where in the onboarding wizard do they drop off?"). Adds session-replay + funnel UI. Stays free up to 1M events/mo. Defer until needed; setup ≈ 3 h.

**Sean Ellis PMF survey** (quarterly) — Tally.so free tier OR Plausible + Google Form. $0. Setup 1 h. Trigger: see §5.3.

### Total recurring cost
| Item | $/mo |
| ---- | ---- |
| D1 + Workers Analytics + Cloudflare logs | 0 (Cloudflare paid plan we already have) |
| Stripe Sigma | ~0 at our volume |
| Sentry free tier | 0 |
| Clerk dashboard | 0 (covered by Clerk plan) |
| Drata | (existing compliance line, not analytics) |
| **Plausible** | **9** |
| PostHog (deferred) | 0 (free tier) |
| **Total addressable** | **$9/mo** |

Headroom against the $50/mo ceiling: **$41/mo** — reserve for PostHog overage if/when we exceed 1M events.

---

## §4 — Dashboard layout

The dashboard is a **single weekly email** (generated by a Cloudflare Cron Worker every Monday 09:00 BRT, sent to Gustavo) plus a **bookmarked Stripe + D1 SQL Console + Plausible** trio for ad-hoc drill-down. No Grafana, no Metabase, no Looker — those are second-employee tools.

### §4.1 — Weekly digest email (THE dashboard)

```
Subject: CoreLink weekly — W22 (2026-05-25 → 2026-05-31)

=== NORTH STAR =========================================
W-CHPA:               17  (Δ +3 vs W21, +21%)
                      [target M3=30, on track]

=== TIER 2 =============================================
Signups (T2-01):       8  (Δ -2 vs W21)
Activation 7d (T2-02): 38%  (3/8 new tenants hit cache)
TTFH median (T2-03):   23 min
Cache-hit rate (T2-04):72%  (healthy)
D7 retention (T2-05):  44%  (cohort W21)
MRR / F2P% (T2-06):    $312 / 7.1%

=== ALERTS =============================================
[ ] T2-02 BELOW 30% for 0/4 weeks
[ ] T2-04 BELOW 50% for 0/2 weeks
[ ] T2-05 BELOW 20% for 0/4 weeks
[ ] MRR < $1k by 2026-08-27 (in 13 weeks)
```

### §4.2 — Daily glance (3 min, mobile)

Two numbers, no chart, viewed on phone:
1. **Yesterday's HITs** (single Cloudflare Analytics widget pinned to phone bookmark)
2. **Stripe today** (Stripe mobile app push)

If both >0, day is fine. No further action.

### §4.3 — Monthly deep-dive (1 h, first Monday of month)

Open D1 SQL Console and run the saved queries:
- Cohort retention matrix (signup week × week-N retention) — render as table in terminal
- Per-tenant cache-hit distribution (histogram of cache-hit rate by tenant)
- Churn list (tenants active week N-4 but not week N)
- Stripe revenue by plan
- Then ask: *"What's the ONE thing I'd change next month to move W-CHPA?"* Write that in the monthly note. Commit it.

---

## §5 — Cadence

### §5.1 — Daily (3 min)
Glance at §4.2 numbers from phone. **No decisions** allowed at this cadence — pure smoke test. If a number is zero when it shouldn't be, file a ticket; do not fix at 7am.

### §5.2 — Weekly (15 min, every Monday)
Read §4.1 digest. For each Tier-2 metric:
- Inside its band → do nothing.
- Outside its band for the trigger-window (see §6) → write one-line hypothesis + the one experiment to run this week.

Time-box: 15 min hard. If a metric demands more than 15 min of thought, defer to the monthly deep-dive.

### §5.3 — Monthly (60 min, first Monday)
§4.3 deep-dive + one of:
- **Sean Ellis PMF survey (quarterly)**: send to all tenants with ≥2 cache HITs in the last 14 days. Three Mondays of the quarter skip; one Monday runs. (Months 3, 6, 9, 12 post-launch.)
- **Channel attribution review**: only once T2-01 ≥ 25/wk. Below that, sample size makes attribution noise.

### §5.4 — Cadence summary
| Cadence | Time-box | Output |
| ------- | -------- | ------ |
| Daily   | 3 min    | Pulse check, no decisions |
| Weekly  | 15 min   | Read digest, hypothesise on out-of-band metrics |
| Monthly | 60 min   | Cohort matrix, churn list, monthly note ("one thing to change") |
| Quarterly | + 30 min | PMF survey |

Total review budget: **≈ 90 min/month**. Anything beyond is analysis-paralysis.

---

## §6 — Decision rules (pre-committed)

Rules below are **pre-decided** so that Monday-morning emotion cannot vote. Each rule states the trigger, the window, and the *action* (not a feeling).

### §6.1 — Activation rule
**Trigger:** T2-02 activation rate <30% for **4 consecutive weeks**.
**Action:** PAUSE all marketing/outbound. Spend the next two weeks on onboarding-friction work (instrumented funnel via PostHog, fix the highest-drop step). No new acquisition until T2-02 ≥30% for 2 weeks.
*Rationale:* below-median activation means more traffic worsens unit economics. Fix the leak first. ([source](https://www.agilegrowthlabs.com/blog/user-activation-rate-benchmarks-2025/))

### §6.2 — Retention rule
**Trigger:** T2-05 D7 retention <20% for **4 consecutive weeks**.
**Action:** Stop the underperforming acquisition channel (whichever brought most of the failing cohort). If only one channel exists, treat as PMF-survey trigger (§6.5) and consider product pivot.
*Rationale:* sub-20% D7 retention for an infrastructure product means the product is not solving a real problem for the cohort attracted. ([source](https://amplitude.com/blog/time-to-value-drives-user-retention))

### §6.3 — Churn rule
**Trigger:** Monthly logo churn >5% for **2 consecutive months** (worst-of-SMB-band).
**Action:** Personally call (Loom / 30-min video) the last 5 churned tenants. Write churn-reason synthesis as a sealed audit. If reasons cluster on a single missing capability, prioritise it in the next sprint.

### §6.4 — Revenue rule
**Trigger:** MRR <$1k by **month 3** post-launch.
**Action:** This is a **signal, not a kill-switch.** Run the §6.5 PMF survey if not already run this quarter. If PMF score ≥40%, continue and re-evaluate at month 6. If PMF score <40%, formal pivot decision at month 4.
*Rationale:* median solo SaaS reaches $1k MRR at month 6, not month 3. Hard-killing at month 3 ignores base rates. ([source](https://www.softwareseni.com/solo-founder-saas-metrics-from-0-to-10k-mrr-in-6-months-with-realistic-timelines/))

### §6.5 — PMF rule (Sean Ellis)
**Trigger:** Quarterly survey of users with ≥2 HITs in last 14 days (run at months 3, 6, 9, 12).
**Action:**
- ≥40% "very disappointed" → PMF achieved. Shift focus from product features to acquisition + scaling.
- 25-40% → near-PMF. Read the "very disappointed" segment's open answers; double-down on the use-case they describe; re-survey in 90 days.
- <25% → no PMF. Triage: (a) wrong ICP, (b) wrong wedge, or (c) wrong product. Formal go/no-go decision required.
([source — methodology](https://learningloop.io/glossary/sean-ellis-score))

### §6.6 — Quality-of-service rule
**Trigger:** T2-04 cache-hit rate <50% for any tenant for **2 consecutive weeks** AND that tenant has >100 requests/week (i.e. real workload, not a probe).
**Action:** Personally reach out to that tenant (email from the digest). High miss rate on a real workload = either a CoreLink bug or a tenant-config issue; either way it's a churn-leading indicator.

### §6.7 — Cost-creep rule
**Trigger:** Analytics-stack monthly bill exceeds $50.
**Action:** Cut the most expensive *tool*, not metrics. The metric tree (§2) survives any single tool removal because the source-of-truth is D1, not a vendor.

---

## §7 — What NOT to track (analytics-paralysis avoidance)

These metrics are **intentionally excluded** from the dashboard and the weekly review. Re-adding any of them requires a documented decision (audit-grade), not a Monday-morning impulse.

| Excluded metric | Why excluded |
| --------------- | ------------ |
| DAU | A daily granularity at 10-100 W-CHPA is statistical noise. WAU is the smallest meaningful window. |
| Page-view counts of individual docs | Vanity. Plausible aggregate is enough; per-page rankings tempt over-optimisation of a low-leverage surface. |
| Per-feature click-through rate inside admin-UI | Premature. With <100 tenants we do not have N for feature-level statistics. |
| NPS | Replace with Sean Ellis PMF survey. NPS noise floor needs hundreds of respondents. |
| Twitter/social-media follower counts | Not a leading indicator of W-CHPA. Personal-brand vanity. |
| GitHub stars on the SDK repo | Same as above. Track only if SDK-stars correlate with signups; check at month 6. |
| Mailing-list size | Track only the *open rate* and *click-to-signup* of the weekly newsletter, not the raw subscriber count. |
| Hourly real-time dashboards | Real-time displays trigger interruption-driven work. The §4.1 weekly email is the contract. |
| Per-employee productivity metrics | N=1. |
| Burn-rate / runway *as a KPI* | Track in personal accounting, not in the product dashboard. Mixing them causes panic-driven product decisions. |
| Competitor benchmarks (their MRR, their hires) | Outside our control; net-zero information for our decisions. |
| LTV / CAC | Defer until ≥6 months of cohort data exist. Calculating LTV/CAC with N<50 churned cohorts is fitting noise. |
| Cohort retention at M3, M6, M12 | Track once cohorts exist; **do not** track at launch (no data). Auto-promote to Tier 2 once first M3 cohort closes. |

---

## §8 — Recommended starter analytics stack (cost-bounded)

### §8.1 — Day-0 stack (launch week)
| Tool | Purpose | $/mo |
| ---- | ------- | ---- |
| D1 (existing) | Source of truth: signups, tenants, cache_events, billing_events | 0 |
| Cloudflare Workers Analytics (existing) | Per-route latency, error rate, traffic | 0 |
| Sentry (free tier) | Error tracking + perf sampling | 0 |
| Stripe (existing) | MRR, churn, F2P% (built-in dashboards) | 0 |
| Clerk (existing) | Auth funnel diagnostics | 0 |
| Plausible | Marketing-site funnel (`/` → `/pricing` → `/sign-up`) | 9 |
| Cron Worker (new, 1 h to build) | Weekly digest email generator | 0 |
| **Total** | | **$9/mo** |

### §8.2 — Trigger-promoted additions
| Add when… | Tool | Marginal $/mo |
| --------- | ---- | ------------- |
| Activation problem cannot be diagnosed from D1 alone (e.g. need to see *which onboarding step* drops users) | **PostHog (free tier)** + session-replay on admin-UI only | 0 until 1M events |
| Inbound channel mix matters (signups >25/wk and ad spend exists) | **Plausible Goals + UTM** (no extra cost) or Attributer.io | 0 to 19 |
| Quarterly PMF survey | **Tally.so free** (200 responses/mo free) | 0 |
| Customer interview ops once >25 tenants | **Loom + Notion (existing)** | 0 |
| Status-page communication | **Cloudflare Status Page** (existing in Workers Paid) | 0 |

### §8.3 — Ceiling
Day-0 stack: **$9/mo**. Even full PostHog scale-out at 5M events/mo ≈ $20-25/mo on the metered tier. Comfortably under the $50/mo cap with the cost-creep rule (§6.7) as the safety valve.

### §8.4 — Concrete first-week build order (8 hours of work)
1. **(2 h)** Add `outcome` column + index to `cache_events` (D1 migration); backfill where derivable.
2. **(2 h)** Write the 7 saved D1 SQL views: nsm_wcpha, signups_weekly, activation_rate, ttfh_median, cache_hit_rate, d7_retention, churn_list.
3. **(2 h)** Build Cron Worker (`apps/api` or a new `workers/weekly-digest`) that runs Monday 09:00 BRT, queries the 7 views, formats the §4.1 email, sends via existing email transport.
4. **(1 h)** Install Plausible script tag on `apps/docs`; add 3 goals (`/pricing` view, `/sign-up` click, `/pilot/apply` submit).
5. **(1 h)** Smoke-test: send the digest to self with fake-data fixture; confirm Monday cron fires.

Output: §4.1 digest in inbox the following Monday. **Stop building dashboards after this.**

---

## §9 — Open questions

1. Does the existing `cache_events` table carry `outcome` granular enough for HIT vs MISS? (Spec assumes either yes, or a 2 h migration.) Confirm in implementation WI.
2. Is the existing email transport (used by Drata fail-closed alerts per `drata-fail-closed-fix.md`) reusable by the cron-worker digest? If yes, $0 incremental; if not, add Resend ≈ $0 (free tier).
3. Should `corelink-signup.humangr.com` events (pilot intake) also fire a Plausible goal? Likely yes — adds funnel visibility into design-partner pipeline.
4. Re-evaluate the NSM at month 6: if cohort data shows that "cache HITs per tenant per week" predicts retention better than the binary "any HIT", upgrade the NSM definition to a weighted version. Pre-commit: the upgrade is OK; arbitrary swaps are not.

---

## §10 — Sources

- [Purchasely — AARRR Framework: Pirate Metrics Complete Guide for 2025](https://www.purchasely.com/blog/aarrr-framework-pirate-metrics-complete-guide-for-2025)
- [Statsig — Pirate metrics (AARRR): startup growth hacking framework](https://statsig.com/perspectives/pirate-metrics-startup-growth)
- [PostHog — Finding your North Star metric and why it matters](https://posthog.com/founders/north-star-metrics)
- [Agile Growth Labs — User Activation Rate Benchmarks 2025](https://www.agilegrowthlabs.com/blog/user-activation-rate-benchmarks-2025/)
- [ProductQuant — The 5-Minute Aha Rule](https://productquant.dev/blog/5-minute-aha-rule-optimize-ttv/)
- [Userpilot — What is Time to Value (TTV)?](https://userpilot.com/blog/time-to-value/)
- [Amplitude — Time to Value: The Key to Driving User Retention](https://amplitude.com/blog/time-to-value-drives-user-retention)
- [Pulseahead — Trial-to-Paid Conversion Benchmarks in SaaS](https://www.pulseahead.com/blog/trial-to-paid-conversion-benchmarks-in-saas)
- [ProductLed — Product-Led Growth Benchmarks](https://productled.com/blog/product-led-growth-benchmarks)
- [1Capture — Free Trial Conversion Benchmarks 2025](https://www.1capture.io/blog/free-trial-conversion-benchmarks-2025)
- [Vena — 2025 SaaS Churn Rate Benchmarks](https://www.venasolutions.com/blog/saas-churn-rate)
- [HubiFi — SaaS Churn Rate Benchmarks 2025](https://www.hubifi.com/blog/calculate-saas-churn-rate)
- [Optifai — B2B SaaS Churn Rate Benchmarks](https://optif.ai/learn/questions/b2b-saas-churn-rate-benchmark/)
- [Learning Loop — Sean Ellis Score](https://learningloop.io/glossary/sean-ellis-score)
- [GoPractice — Product/Market fit survey by Sean Ellis](https://pmfsurvey.com/)
- [SoftwareSeni — Solo Founder SaaS Metrics: From $0 to $10K MRR](https://www.softwareseni.com/solo-founder-saas-metrics-from-0-to-10k-mrr-in-6-months-with-realistic-timelines/)
- [F³ Fund It — The Solopreneur Analytics Stack 2026](https://f3fundit.com/the-solopreneur-analytics-stack-2026-posthog-vs-plausible-vs-fathom-analytics-and-why-you-should-ditch-google-analytics/)
- [PostHog — Pricing](https://posthog.com/pricing)
- [Plausible — Pricing Plans 2026 (CompareTiers)](https://comparetiers.com/tools/plausible-analytics)

---

## §11 — Cross-references

- [`2026-05-27-launch-readiness-check.md`](2026-05-27-launch-readiness-check.md) — five-area launch gate; this audit feeds the L2-signup-flow remediation list with instrumentation requirements.
- [`2026-05-27-drata-fail-closed-fix.md`](2026-05-27-drata-fail-closed-fix.md) — existing alert-email transport reusable by the §8.4 cron worker.

---
