# Launch Metrics Dashboard

> **DRAFT — pending Marketing + Owner sign-off.**
> Trace: WI-S20-008 §2.1.4 + §12 (observability) + §2.1.6 (launch runbook T+7d retrospective).
> Purpose: define what we measure during and after launch, in which tool, at which cadence, and what each metric tells us.

---

## 1. Categories of measurement

| Category | What it tells us | Cadence at T-0 to T+7d |
|---|---|---|
| **Press pickup** | Was the press release picked up by tier-1 tech press and ecosystem outlets? Quality vs. quantity of mentions. | Daily |
| **Blog UVs (unique visitors)** | Is the launch content reaching its audience? Which post drives the most engagement? | Hourly T-0 to T+1d; daily T+1d to T+7d |
| **Product Hunt rank** | Where does CoreLink rank in PH's daily / weekly leaderboard? | Hourly T-0; daily T+1d to T+7d |
| **Hacker News rank + Show HN engagement** | Did Show HN reach front page? Comment volume and quality. | Hourly T-0; daily T+1d to T+7d |
| **Signup conversion** | What fraction of launch-driven traffic converts to signup? Attributed by referrer. | Daily |
| **Lighthouse score** | Site performance from CDN / SEO standpoint — meaningful proxy for ranking. | Daily |
| **Social share velocity** | Engagement on CEO LinkedIn + Twitter / X thread. Re-share count, reply count. | Hourly T-0 to T+1d; daily T+1d to T+7d |
| **Enterprise inbound pipeline** | Sales inbound from launch visibility. Quality of fit. | Daily |

## 2. Specific metrics + tools

### 2.1 Press pickup

| Metric | Tool | Target / baseline |
|---|---|---|
| Tier-1 press mentions (TechCrunch, The Register, Ars Technica, etc.) | ahrefs Brand Mentions + manual sweep | ≥ 3 substantive mentions in launch week. |
| Ecosystem press mentions (Bazel community, Buck2 community, build-tools newsletters) | Manual sweep | ≥ 5 substantive mentions in launch week. |
| Backlinks to `corelink.dev` from earned media | ahrefs Site Explorer | ≥ 20 new referring domains in launch week. |
| Wire pickup count | BusinessWire dashboard + PR Newswire dashboard | ≥ 50 pickups (typical for tech wire). |

### 2.2 Blog UVs

| Metric | Tool | Notes |
|---|---|---|
| Unique visitors per blog post (01..05) | Google Analytics / Plausible / Cloudflare Web Analytics | Tracked per-post; segment by referrer (PH, HN, direct, social). |
| Time-on-page per blog post | Google Analytics / Plausible | Proxy for content quality. |
| Scroll depth | Google Analytics enhanced measurement | Detects abandonment patterns. |
| Outbound clicks to docs / trust center / signup | Google Analytics outbound tracking | Conversion-funnel signal. |

### 2.3 Product Hunt

| Metric | Tool | Cadence |
|---|---|---|
| Hour-by-hour rank | Product Hunt API | Hourly T-0 |
| Final day-1 rank | Product Hunt API | T+1d close |
| Upvote count | Product Hunt API | Hourly T-0 |
| Comment count (Maker + non-Maker) | Product Hunt API | Hourly T-0 |
| Click-through to `corelink.dev` from PH | Cloudflare Web Analytics (referrer = producthunt.com) | Daily |
| Signups attributed to PH | Signup form referrer parameter | Daily |

### 2.4 Hacker News

| Metric | Tool | Cadence |
|---|---|---|
| HN rank trajectory | Hacker News API | Hourly T-0 |
| Show HN comment count | Hacker News API | Hourly T-0 |
| Comment quality sample | Manual review (Founder + Marketing) | Daily T+1d to T+7d |
| Click-through to `corelink.dev` from HN | Cloudflare Web Analytics (referrer = ycombinator) | Daily |
| Signups attributed to HN | Signup form referrer parameter | Daily |

### 2.5 Signup conversion

| Metric | Tool | Target |
|---|---|---|
| Launch-week signups | CoreLink internal signup metrics (`corelink_signup_total{plan, referrer}`) | Pending Marketing target — placeholder. |
| Free → Team upgrade rate (launch cohort, T+30d) | CoreLink internal billing metrics | Tracked over T+30d window; not a launch-week metric per se. |
| Enterprise sales-qualified leads (SQLs) from launch | Sales CRM | Pending Sales target — placeholder. |

### 2.6 Lighthouse / SEO health

| Metric | Tool | Target |
|---|---|---|
| `corelink.dev` Lighthouse score (performance / accessibility / SEO) | Lighthouse (Google) + PageSpeed Insights | ≥ 90 across the four categories at launch. |
| Core Web Vitals (LCP, INP, CLS) | Cloudflare Web Analytics + PageSpeed Insights | Within "good" thresholds at launch. |

### 2.7 Social

| Metric | Tool | Notes |
|---|---|---|
| LinkedIn post impressions, reactions, comments, reshares | LinkedIn analytics + manual capture | Captured at T+1d and T+7d. |
| Twitter / X thread impressions, retweets, replies, bookmarks | Twitter / X Analytics API | Captured at T+1d and T+7d. |
| Share velocity (shares per hour) | Manual + API | Spike pattern is the signal. |

### 2.8 Engineering / production posture during launch

| Metric | Tool | Trigger |
|---|---|---|
| SLO compliance (`SLO-LAT-CAS-GET`, `SLO-AVAIL-CAS-GET`, `SLO-AVAIL-CAS-PUT`) | CoreLink internal dashboards | Any out-of-budget → on-call escalation. |
| Signup spike vs. baseline | CoreLink signup metric | Spike > 10x baseline → SRE awareness, scale check. |
| SEV count during launch window | Incident tracker | Any SEV-1 → CTO + Owner notification. |
| Synthetic page response time | PagerDuty + synthetic monitor | < 5 min sustained per CAP-GA-006. |

## 3. Reporting cadence

- **T-0 hour-by-hour:** PH rank, HN rank, blog UVs, signup count, social engagement.
- **T+1d 12:00 PT:** day-1 summary captured.
- **T+7d 12:00 PT:** week-1 summary + retrospective input.

## 4. Tools inventory

| Tool | Use |
|---|---|
| **ahrefs** | Brand mentions, backlinks, referring domains. |
| **Google Analytics** (or Plausible / Cloudflare Web Analytics) | Blog UVs, time-on-page, scroll depth, outbound. |
| **Product Hunt API** | PH rank, upvotes, comments. |
| **Hacker News API** | HN rank, comment count. |
| **Twitter / X Analytics API** | Thread impressions, reshares, replies. |
| **LinkedIn analytics** | CEO post engagement. |
| **Lighthouse / PageSpeed Insights** | Site performance / SEO. |
| **CoreLink internal Prometheus dashboards** (e.g. `DASH-LAUNCH-ORCHESTRATION`) | Signup metrics, blog publish metrics, PH outreach phase gauge (per WI-S20-008 §12). |
| **PagerDuty** | Incident response posture during launch. |

## 5. Privacy + measurement posture

- All audience-side analytics respect customer-side privacy: we do not deploy invasive tracking. Default to first-party Cloudflare Web Analytics or Plausible (cookieless) rather than third-party trackers.
- Signup conversion attribution uses referrer parameter, not cross-site tracking pixels.
- No personally identifiable information is collected from blog / press / PH / HN visitors beyond standard server-log granularity.

## 6. What we will not measure (intentionally)

- We will not report PH or HN engagement broken down in a way that incentivizes manipulation. Vanity rank without comment quality is meaningless.
- We will not publish enterprise pipeline numbers publicly during launch. Those are internal sales metrics.
- We will not target specific revenue numbers in launch-week reporting; the launch is a customer-acquisition + trust-establishment exercise, not a revenue exercise.

---

## Internal notes

- Dashboard panel `DASH-LAUNCH-ORCHESTRATION` embedded as soft-gate component of `DASH-GA-READINESS` per WI-S20-008 §12.
- Per WI-S20-008 §12, signup attribution and blog metrics flow through Prometheus snake_case underscored metrics; cardinality budget respected (5 posts × small dimension set; 50–100 outreach phases).
- All measurement instrumentation is independent of engineering-gate decision — launch metrics inform the next launch, not the GA decision.
