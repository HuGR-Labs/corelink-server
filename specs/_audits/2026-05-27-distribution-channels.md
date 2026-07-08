---
id: "AUDIT-2026-05-27-DISTRIBUTION-CHANNELS"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "distribution", "marketing", "launch", "solo-founder", "go-to-market"]
---

# Distribution channels research — solo-founder dev-tool launch (CoreLink)

> **Scope.** SOTA research on distribution channels for a solo-founder developer-tool launch (CoreLink = content-addressable remote cache for Bazel / sccache / Docker / Turborepo / Nx). Output: a rated channel inventory, a 30-day launch calendar, and re-usable post templates. No paid spend; effort budgeted in solo-founder hours, not headcount.
> 
> **Method.** Web research (May 2026) on Hacker News, Product Hunt, dev.to, Hashnode, Indie Hackers, Reddit, X/Twitter, Slack/Discord, podcasts, and SEO opportunity. Cross-referenced against published case studies of comparable dev-infra launches (BuildBuddy, Tinybird, Cal.com, Lago, Depot, Nx Cloud, Turborepo, Plausible).
> 
> **Audience.** ICP = engineers at 20-500-eng companies feeling Bazel / Turborepo / Nx / sccache / Docker CI pain. Decision-maker = staff/principal eng or platform/devprod lead. Buyer = same person or their VP-Eng.
> 
> **Anti-scope.** Paid ads (waste pre-PMF), LinkedIn lead-gen (wrong audience), conferences as primary channel (cost > ROI pre-revenue), influencer paid placements, PR firms.

---

## §1 — Channel research table

Ratings on 1-5 scale: **F** = solo-founder fit (can do alone in <1 day?), **A** = audience fit for dev-infra, **C** = conversion quality (signups → activation), **E** = effort to execute well, **O** = expected first-week signup range.

| # | Channel | URL | Audience fit (A) | Solo fit (F) | Conv quality (C) | Effort (E) | First-week signups (O) | Notes / rules |
|---|---------|-----|------|------|------|------|------|------|
| 1 | **Show HN** | https://news.ycombinator.com/show | 5 | 5 | 4 | 2 (1-day prep) | 50-500 if front page; 0-20 if not | Title format: `Show HN: <Name> – <one-line value prop>`. No marketing speak. Must have a working demo URL, no signup wall. Author replies to every comment in first 4h. Best window: Tue-Thu 7-10am PT. ([HN guidelines](https://news.ycombinator.com/showhn.html)) |
| 2 | **Launch HN** | https://news.ycombinator.com/launches | 5 | 3 | 5 | 4 (gated by YC) | 100-2,000 | YC-only. Not applicable unless we go through YC. ([Launch HN context](https://news.ycombinator.com/item?id=16589820)) |
| 3 | **Product Hunt** | https://www.producthunt.com | 3 | 4 | 2 | 3 (1-week prep) | 20-200 trial signups (1.4% conv) | Still works for dev tools (Tinybird Forward Mar-2025 got 198 upvotes; Cal.com remains a flagship). Hunter not required since 2024. Launch Tue/Wed 00:01 PT. Weak for high-intent infra buyers. ([Tinybird PH](https://www.producthunt.com/products/tinybird), [Cal.com PH](https://www.producthunt.com/products/cal), [2025 PH guide](https://www.wowtechub.com/blog/the-2025-guide-to-product-hunt-launches/)) |
| 4 | **dev.to** | https://dev.to | 4 | 5 | 3 | 2 | 5-50 per post | Cross-post with `canonical_url` to corelink.humangr.com. Best tags: `#bazel`, `#devops`, `#performance`, `#monorepo`, `#cicd`. Long-form tutorials > announcements. ([Syndication guide](https://draft.dev/learn/syndicating-developer-content)) |
| 5 | **Hashnode** | https://hashnode.com | 4 | 5 | 3 | 2 | 5-30 per post | Personal domain support is the win — content sits on your subdomain, builds your SEO. Use `originalArticleURL` for canonical. ([Hashnode canonical](https://townhall.hashnode.com/why-you-should-republish-your-devblog-posts-and-how-to-do-it)) |
| 6 | **Indie Hackers** | https://www.indiehackers.com | 2 | 5 | 4 | 2 | 5-30 over 4-6 wks | Audience = other founders, not enterprise infra buyers. Conversion quality 17x PH for *founder-focused* products but our ICP is platform-engineers. Still worth building-in-public posts. ([IH 2025 strategy](https://awesome-directories.com/blog/indie-hackers-launch-strategy-guide-2025/)) |
| 7 | **r/bazel** | https://reddit.com/r/bazel | 5 | 5 | 5 | 1 | 5-30 | Small (~5k subs) but ICP-pure. Read rules first; one self-promo post per author per month is the unwritten norm. Lead with technical content, mention CoreLink in body, not title. |
| 8 | **r/devops** | https://reddit.com/r/devops | 4 | 5 | 3 | 1 | 10-100 | ~700k subs. Strict no-promo rule outside the monthly thread. Frame as "we built X to solve Y, here's the postmortem". |
| 9 | **r/programming** | https://reddit.com/r/programming | 3 | 5 | 2 | 2 | 0-200 (lottery) | ~6M subs. Pure technical writeup only; product mentions get removed by mods. Best as a long-form architecture post on our blog that someone else submits. |
| 10 | **r/golang / r/rust** | https://reddit.com/r/golang, https://reddit.com/r/rust | 3 | 5 | 4 | 1 | 5-40 | Only if we publish a Rust/Go-specific deep-dive (we use Rust → r/rust is a fit). Audience values craft, not pitch. |
| 11 | **r/sre** | https://reddit.com/r/sre | 4 | 5 | 4 | 1 | 5-30 | ~80k subs. Quiet but high-signal. SRE leads are exactly the buyer for cache infra. |
| 12 | **r/buildtools** | (verify) | n/a | n/a | n/a | n/a | n/a | **Does not exist as an active sub.** Skip. (r/buildapc dominates the namespace.) |
| 13 | **X / Twitter — dev-infra accounts** | https://x.com/bazelbuild and others | 4 | 5 | 3 | 2 (ongoing) | 5-50 per viral thread | See §1.1 for handles. Strategy: reply to existing pain threads, then post own thread. |
| 14 | **BazelBuild Slack** | https://slack.bazel.build | 5 | 5 | 5 | 1 | 5-20 | ~4k members. `#general`, `#remote-execution`, `#remote-cache` are direct ICP. No spam — answer questions for 2 weeks before mentioning CoreLink. |
| 15 | **CNCF Slack** | https://communityinviter.com/apps/cloud-native/cncf | 3 | 5 | 3 | 1 | 5-20 | Massive but diffuse. Find `#build-tools`, `#sig-developer-experience` style channels. |
| 16 | **Buildkite / Earthly / Depot Discord** | (per-product) | 4 | 4 | 4 | 1 | 5-20 | Adjacent-tool communities — users already feel build-cache pain. Same lurk-then-help-then-mention rule. |
| 17 | **The Changelog podcast** | https://changelog.com/podcast | 5 | 2 | 5 | 5 (pitch + 4-6wk lead) | 50-300 | High-bar pitch; need a story angle beyond "we built X". Better target after first 10 paying customers. ([Pitch contact](https://changelog.com/podcast)) |
| 18 | **DevTools FM / Software Engineering Daily / CoRecursive** | various | 5 | 2 | 5 | 5 | 20-200 | Same as above — long lead, high signal. |
| 19 | **SEO — comparison pages** | own site | 5 | 4 | 5 | 3 (per page) | 0 wk1 → 50/mo at mo-6 | "CoreLink vs BuildBuddy", "vs Nx Cloud", "vs Turborepo Remote Cache", "vs Depot", "self-hosted Bazel remote cache" — bottom-funnel intent. Compounds. |
| 20 | **SEO — keyword content** | own site | 5 | 4 | 4 | 3 (per page) | 0 wk1 → 100/mo at mo-6 | Targets: "remote build cache", "bazel cache hosting", "sccache S3 setup", "turborepo remote cache self-hosted", "monorepo CI speedup". Compounds. |
| 21 | **HN comments (not posts)** | https://news.ycombinator.com | 4 | 5 | 4 | 1 (ongoing) | 5-30/wk steady | Reply substantively on any build-cache / CI / monorepo / Bazel thread. Link only when contextually relevant. Cumulative effect > single post. |
| 22 | **Conferences (KubeCon, OSSummit, BazelCon)** | various | 5 | 1 | 5 | 5 (months) | 10-50 in-person | **AVOID pre-revenue.** Costs >$5k all-in (travel + booth or even just attendance). Re-evaluate after 50 paying customers. |

### §1.1 — X / Twitter handles to engage (sample 10)

Hand-picked for build-system / CI / monorepo / infra-engineer relevance. Follower counts are approximate (May 2026).

| Handle | URL | Approx followers | Why |
|---|---|---|---|
| @bazelbuild | https://x.com/bazelbuild | ~15k | Official Bazel — retweets community wins |
| @aspect_build | https://x.com/aspect_build | ~3k | Bazel consultancy, blog posts get traction |
| @nxdevtools | https://x.com/nxdevtools | ~10k | Nx team — competitor-adjacent but engages |
| @turborepo | https://x.com/turborepo | ~30k | Vercel-owned; broad monorepo reach |
| @depotdev | https://x.com/depotdev | ~5k | Closest analogue (remote Docker build cache) |
| @earthly_dev | https://x.com/earthly_dev | ~6k | Build-tool community |
| @kelseyhightower | https://x.com/kelseyhightower | ~190k | Infra OG — one retweet = thousands of impressions |
| @copyconstruct (Cindy Sridharan) | https://x.com/copyconstruct | ~80k | Distributed-systems voice |
| @mitchellh (Mitchell Hashimoto) | https://x.com/mitchellh | ~110k | DX-tool builder; loves polished CLIs |
| @b0rk (Julia Evans) | https://x.com/b0rk | ~200k | Explains infra concepts; if she writes about cache, huge lift |

Verify each before engaging — these accounts shift. Strategy: reply substantively to their *build/CI* tweets first; never DM pitch.

---

## §2 — Top-3 recommended channels for launch

### #1 — Show HN (D-day primary)

**Why.** Highest signal-to-noise for ICP. Free. One-shot. Comparable launches (Depot, BuildBuddy, Earthly) all originated here. Audience expects technical product → CoreLink fits exactly. Self-promotion is the *point* of Show HN, so no guideline friction.

**Rationale specifics.**
- HN traffic skews to senior engineers, exactly our buyer.
- Open-source friendly community: lead with `Apache-2.0` license badge in repo.
- Even a non-front-page Show HN gets ~10-30 visits/upvote and persists on the `show` tab for days — long tail.
- Costs zero. Re-runnable in 6 months with a major feature.

**Pre-reqs.**
- Working demo at a stable URL (no auth wall on landing).
- README with one-paragraph "what is this" + 60-second quickstart.
- Pricing page exists (HN comments will demand it within minutes).
- Author online to reply to *every* comment for first 4 hours.

### #2 — Reddit r/bazel + r/devops + r/sre (high-intent, low-effort)

**Why.** Small absolute reach, but every reader is ICP. Conversion quality dwarfs PH. Zero prep beyond a well-written technical post. Compounds because Reddit posts rank in Google for years.

**Rationale specifics.**
- r/bazel is small (~5k) but every user has felt remote-cache pain.
- r/devops + r/sre have ~750k combined; even 0.1% engagement = 750 viewers.
- Avoids the "promotional post" tag by leading with a technical writeup + repo link, with CoreLink as the substrate (not the headline).

**Pre-reqs.**
- 2-week "lurk + help" warm-up (build comment karma in target subs).
- Technical post (architecture deep-dive, benchmark, postmortem) — not a launch announcement.
- Read each sub's rules + posting schedule (most have a weekly self-promo thread as fallback).

### #3 — Content + SEO comparison pages (compounding, low-immediate)

**Why.** Every other channel is a pulse. SEO is a heartbeat. Comparison-page intent ("X vs Y") is bottom-funnel — the visitor is already in evaluation. Ten well-crafted pages will produce 100+ qualified visits/month by month 6 with no ongoing effort.

**Rationale specifics.**
- Long-tail keywords like `"self-hosted bazel remote cache"` have low volume but ~100% buyer intent.
- Comparison pages ("CoreLink vs BuildBuddy", "vs Nx Cloud", "vs Depot", "vs Turborepo Remote Cache") rank fast because few competitors create them about themselves.
- Compounds while you sleep. Pairs with Hashnode canonical strategy to leech authority from cross-posts.

**Pre-reqs.**
- Honest comparisons (HN/Reddit will eviscerate exaggerated ones).
- One page per competitor (5-8 pages MVP). 1,500-2,500 words each.
- Schema markup for `ComparisonPage` (rich-snippet eligible).

---

## §3 — 30-day launch calendar

Solo-founder time-budget assumption: 30-50 productive hours/week (so 4-7h/day).

### Day 0-3 — Foundation (no public moves)

| Day | Task | Channel | Effort | Output |
|---|---|---|---|---|
| 0 | Lock landing page copy (HN-grade: tech-honest, no superlatives) | own site | 4h | corelink.humangr.com tells the story in 5 seconds |
| 0 | Pricing page live with at least Free + Paid tier | own site | 2h | /pricing |
| 1 | 60-second demo video (Loom screen-recording, no editing) | own site | 2h | embed on landing |
| 1 | README polish: quickstart in <10 lines | GitHub | 2h | README.md |
| 2 | Write architecture deep-dive post #1 ("How we built a content-addressable cache on Cloudflare Workers") | own blog | 6h | 2,000-word post |
| 2 | Set up basic analytics (Plausible or PostHog) + signup tracking | own site | 2h | per-channel UTM dashboard |
| 3 | Draft Show HN title + first comment (the "why we built this" reply) | local | 2h | doc ready |
| 3 | Smoke-test signup flow end-to-end (login, key issuance, first cache hit) | own site | 2h | green |

**Gate to Day 4.** Demo must work for a stranger in <60s with no support.

### Day 4-7 — Warm launch (private network)

| Day | Task | Channel | Effort | Output |
|---|---|---|---|---|
| 4 | DM 10 ex-colleagues + 10 Twitter dev-infra mutuals: "would value 5 min of your time" | DM / email | 3h | 10 first-look invites |
| 4 | Post #1 on personal X: "Been building X for 6 months. Here's why." (no launch link yet) | X | 1h | seed thread |
| 5 | Publish architecture post on own blog | own blog | 1h | live |
| 5 | Cross-post to Hashnode w/ canonical → own domain | Hashnode | 1h | mirror |
| 6 | Cross-post to dev.to w/ canonical (wait 48h after own-domain publish for SEO) | dev.to | 1h | mirror |
| 6 | Collect 5-10 testimonial quotes / screenshots from warm-launch users | email | 2h | social proof asset |
| 7 | Fix top-3 issues that warm users hit | code | 6h | smoother onboarding |
| 7 | Final dry-run of Show HN launch with 2 trusted readers reviewing title + first comment | local | 2h | reviewed |

### Day 8-14 — First wide launch (Show HN + Reddit r/bazel)

| Day | Task | Channel | Effort | Output |
|---|---|---|---|---|
| 8 (Tue) | **Show HN, 7:00 PT** — title format `Show HN: CoreLink – <value prop in 6 words>` | HN | 6h *that day* (replies) | post live; reply to every comment for 6h |
| 8 | Tweet thread tied to Show HN (template in §5) | X | 1h | thread |
| 8 | Post in BazelBuild Slack `#general` — "Just shipped, would love feedback" (not a pitch) | Slack | 0.5h | seed conversation |
| 9 | Reply to all overnight HN comments | HN | 3h | continued engagement |
| 9 | r/bazel post: tech writeup, link to repo + cache architecture; mention CoreLink as the substrate | Reddit | 2h | post live |
| 10 | Indie Hackers milestone post: "Launched on HN — here's what happened" | IH | 1h | post |
| 11 | r/devops post: angle = "we built this to fix X" (not a launch) | Reddit | 2h | post |
| 12 | dev.to post #2: tutorial ("Set up CoreLink with Bazel in 5 minutes") | dev.to | 3h | post |
| 13 | First retro: which channel converted best? Per-channel UTM table | analytics | 2h | data |
| 14 | Buffer day — fix whatever broke under load | code | 6h | stable |

### Day 15-21 — Targeted communities + cold outreach

| Day | Task | Channel | Effort | Output |
|---|---|---|---|---|
| 15 | r/sre post: SRE-angle (cache failures, multi-region, observability) | Reddit | 2h | post |
| 16 | r/rust post: implementation deep-dive (we wrote it in Rust) | Reddit | 2h | post |
| 17 | Publish first 2 comparison pages: "CoreLink vs BuildBuddy", "vs Depot" | own site | 5h | 2 pages live |
| 18 | Cold-email batch #1: 20 platform/devprod leads at 100-300-eng companies using Bazel (GitHub repo signal: `WORKSPACE` file + `.bazelrc`) — template §6 | email | 3h | 20 emails |
| 19 | HN comment campaign: reply substantively on 5 build/CI/monorepo threads | HN | 1h/day | comments |
| 20 | Engage on X with 5 dev-infra influencer threads (§1.1) — substantive replies only | X | 1h | replies |
| 21 | Publish comparison pages 3-4: "vs Nx Cloud", "vs Turborepo Remote Cache" | own site | 5h | 2 pages live |

### Day 22-30 — Iteration + Product Hunt + second wave

| Day | Task | Channel | Effort | Output |
|---|---|---|---|---|
| 22 | Analyze first-30-day funnel: visits → signups → activation → second-session retention | analytics | 3h | retro doc |
| 23 | Fix top friction point identified in retro | code | 6h | iteration |
| 24 | Product Hunt launch (Tue/Wed 00:01 PT) — only if landing converts >2% | PH | 1d prep | launch |
| 25 | Cold-email batch #2: 20 more leads, with refined pitch | email | 3h | 20 emails |
| 26 | Hashnode post #3: customer story / case study (if we have a paying user) | Hashnode | 3h | post |
| 27 | Pitch The Changelog with a story angle (not "we launched") — e.g. "what we learned building on Cloudflare Workers" | email | 2h | pitch |
| 28 | Reddit follow-up: post benchmarks vs first-week numbers | Reddit | 2h | post |
| 29 | First "30-day retro" public blog post — builds in public | own blog | 3h | post |
| 30 | Calendar v2 for days 31-60 based on what converted | planning | 4h | next-plan |

**Hard pause triggers** (replan, don't push through):
- Signup → activation <10% after 100 signups → fix product before more distribution.
- Show HN dies before front page (<20 upvotes in 2h) → don't burn other channels; debug and re-launch in 8 weeks with a better angle.
- Production incident lasting >2h during a launch window → pull all amplification, focus on stability.

---

## §4 — Show HN post template (skeleton)

Not the actual post — the framework.

```
TITLE (60-80 chars max)
  Format: "Show HN: <Name> – <value prop in 6-10 plain words>"
  Bad: "Show HN: The future of build caching"
  Good: "Show HN: CoreLink – self-hosted remote cache for Bazel/sccache/Docker"

URL field
  Direct link to working demo (NOT a landing page with a "sign up" wall).
  Prefer a /try or /playground route that works without auth.

TEXT field (optional; ~150-300 words)
  ¶1: What it is — one sentence, no superlatives.
  ¶2: Why we built it — concrete pain in concrete terms ("our CI took 22 min;
       sccache S3 setup was 400 lines of Terraform; we wanted 3 lines of YAML").
  ¶3: How it works in one paragraph — content-addressable, CDN-backed, BYOK encryption.
  ¶4: What's free / what's paid / what's open source (license).
  ¶5: Explicit ask: "Would love feedback on X, Y, Z."

FIRST COMMENT (post immediately after submission, from same account)
  - Tech architecture in 3-5 bullets (the "HN tax" — they want internals)
  - Pricing (don't make them dig)
  - Roadmap items you'd want feedback on (gives commenters a hook)
  - Link to GitHub repo with stars badge

REPLIES (first 4 hours)
  - Reply to EVERY comment, including criticism.
  - When criticized: agree with the valid part, then add nuance. Never defensive.
  - When asked "how is this different from X?": one-paragraph comparison,
    not a list of features. Be honest about what X does better.
  - Never use "we're the first/best/fastest". HN punishes superlatives.

TIMING
  - Submit Tue/Wed/Thu 07:00-09:00 PT (10:00-12:00 ET, 15:00-17:00 UTC).
  - Avoid Mondays (weekend backlog), Fridays (early checkout), holidays.
  - Be at keyboard for the next 6 hours minimum.

ANTI-PATTERNS
  - Don't ask friends to upvote (HN detects ring-voting).
  - Don't post from a brand-new account.
  - Don't link to a paywalled or auth-walled page.
  - Don't write marketing copy ("revolutionize", "next-generation", "AI-powered").
```

---

## §5 — X / Twitter thread template (skeleton)

```
TWEET 1 — hook (1/n)
  Lead with concrete pain or concrete number, not the product name.
  Bad:  "Excited to launch CoreLink today!"
  Good: "Our CI cache miss rate was 64%. Setting up sccache+S3 was 400 lines of
        Terraform. So we built CoreLink. (thread)"

TWEET 2 — context (2/n)
  Who it's for in one sentence. ("If you run Bazel/sccache/Docker on >5 build
  agents, this thread is for you.")

TWEET 3-5 — the 3 key technical decisions
  One tweet per decision. Each with the tradeoff: what we chose, what we
  gave up. Engineers reward honesty about tradeoffs.

TWEET 6 — a screenshot or terminal recording (asciinema/gif)
  Visual beats text. Show the speedup, not the marketing claim.

TWEET 7 — pricing in one line
  "Free up to 10GB/mo. $0.02/GB after. BYOK encryption included." (concrete)

TWEET 8 — open-source posture
  Repo link + license. State explicitly which parts are OSS vs commercial.

TWEET 9 — explicit ask + link
  "Trying it takes 90 seconds: corelink.humangr.com/try.
   Reply with your stack — I'll tell you honestly if it's a fit."

TWEET 10 — credit
  Name 3-5 prior-art projects (Bazel, BuildBuddy, sccache, Depot) with @mentions.
  This is good karma AND gets you on their notifications.
```

Post the thread the same hour you submit Show HN — synergy. Pin the thread for the day.

---

## §6 — Cold outreach email template (skeleton)

Target: platform-eng / devprod / build-eng leads at companies showing Bazel/Turborepo signals on GitHub.

```
SUBJECT (under 50 chars, lowercase, no marketing speak)
  Format: "<thing they did> — quick question"
  Examples:
    "saw your bazel migration post — quick q"
    "your monorepo CI talk at <conf> — quick q"
  AVOID: "Speed up your Bazel builds 10x!" (filtered to spam).

PREVIEW LINE (first 80 chars must hook)
  "Hi <name>, read your post on <specific thing> last week and had a question
  about your remote-cache setup —"

BODY (max 90 words; plain text only)

  Hi <name>,

  Read your <specific post / talk / GitHub commit> on <specific thing>.
  Your point about <specific quote/idea> matched what I see at most teams
  running Bazel at scale.

  Quick context: I built CoreLink — content-addressable remote cache,
  3 lines of YAML, BYOK encryption, works with Bazel/sccache/Docker.
  Built it because <one-sentence pain story>.

  Not pitching — would love 15 min of your time to learn what your team's
  cache stack looks like today and where it falls over. In return I'll
  share what I've seen across the ~20 teams I've talked to.

  Worth a chat?

  — Gustavo
  corelink.humangr.com  |  github.com/HumanGuardrail/corelink

SIGNATURE
  Real name. Real URL. No corporate logo. No "Sent from my iPhone".

VOLUME / CADENCE
  Cap 20 emails/day from a warmed inbox. Wait 5 business days before
  one follow-up (one only). Reply rate target: 5%+. Below 3% means the
  list or copy is wrong — fix before sending more.

ANTI-PATTERNS
  - "Hope this finds you well" → delete.
  - "I'd love to schedule a quick demo" → no, you want a conversation.
  - HTML signatures with logos → filters to spam.
  - Sending to info@ / contact@ → never.
  - Mass-merge without per-recipient first sentence → never.
```

Benchmarks (B2B 2026): 27.7% open / 3.4% reply average; advanced personalization can lift reply to 10-18%.

---

## §7 — What to AVOID

| Anti-channel | Why avoid (pre-PMF / solo) |
|---|---|
| **Paid ads (Google, Meta, LinkedIn)** | Burns runway, teaches nothing. Skip until you can answer: "what is my LTV?" with real data. |
| **LinkedIn lead-gen / Sales Navigator** | Wrong audience. Platform-eng buyers are on HN/Reddit/Slack, not LinkedIn. The few times it works, it's because the buyer already knew you. |
| **In-person conferences (KubeCon, BazelCon, OSSummit) as primary channel** | All-in cost >$5k (travel + lodging + opportunity cost). Re-evaluate after $5k MRR. Exception: free local meetups. |
| **PR firms / press releases** | Tech press doesn't cover infra startups under $1M ARR. PR firms charge $5-15k/mo for nothing useful at this stage. |
| **Paid influencer placements on X / YouTube** | Audience is sensitive to `#ad` disclosure; trust collapses. Build relationships instead (free, slower, real). |
| **Hiring a marketer / agency before $10k MRR** | You don't know what works yet. An agency will spend money on what worked for *other* products. |
| **Generic "post on LinkedIn" advice** | Wrong audience for build-cache infra. Skip. |
| **Reddit drive-by spam** | Will get you shadow-banned across multiple subs simultaneously. Build comment karma in target subs for 2 weeks before any link. |
| **Email-list buying** | Burn your sender reputation, kill cold-email channel for months. Build the list yourself via GitHub signals + manual research. |
| **Show HN-ing twice in 4 weeks** | The community remembers. Save re-launches for genuine new chapters (GA, major feature, OSS milestone). |
| **Paying for Product Hunt "upvote rings"** | Detected, dehydrates rank, public shaming risk. |
| **Conferences as a *speaker* pre-revenue without a polished talk** | A weak talk in front of your ICP is anti-marketing. Wait until you have customer stories. |

---

## §8 — Effort summary (solo-founder hour budget)

| Phase | Wall-clock | Solo-founder hours | Output |
|---|---|---|---|
| Day 0-3 (foundation) | 4 days | ~22h | Landing, demo, pricing, first deep-dive post, Show HN draft |
| Day 4-7 (warm) | 4 days | ~17h | 10 warm-launch users, 2 cross-posts, top-3 fixes |
| Day 8-14 (Show HN + Reddit) | 7 days | ~28h | HN launch, 3 Reddit posts, IH post, dev.to tutorial |
| Day 15-21 (targeted + cold) | 7 days | ~20h | 2 Reddit posts, 4 comparison pages, 20 cold emails, HN/X engagement |
| Day 22-30 (iterate + PH + ChangeLog pitch) | 9 days | ~30h | PH launch, 20 more emails, podcast pitch, retro post |
| **Total** | **30 days** | **~117h** | **30-day distribution coverage with no paid spend** |

That's ~4h/day average — realistic for a solo founder who's also coding.

---

## §9 — Per-channel expected outcomes (honest)

| Channel | Best-case wk-1 signups | Realistic wk-1 | Worst-case wk-1 |
|---|---|---|---|
| Show HN (front page) | 500 | 80 | 5 |
| Show HN (no front page) | 30 | 8 | 0 |
| Product Hunt | 200 | 40 | 10 |
| r/bazel | 30 | 12 | 3 |
| r/devops | 100 | 25 | 5 |
| r/sre | 30 | 10 | 2 |
| r/rust (deep-dive) | 40 | 15 | 3 |
| dev.to tutorial | 50 | 12 | 2 |
| Hashnode | 30 | 8 | 1 |
| Indie Hackers | 30 | 8 | 1 |
| BazelBuild Slack | 20 | 8 | 2 |
| Cold email (20 sent) | 4 conversations | 1-2 | 0 |
| The Changelog (4-6wk lag) | 300 | 100 | 30 |
| Comparison-page SEO (mo-6) | 100/mo | 30/mo | 5/mo |

Aggregate realistic first-30-day signups: **150-400** if Show HN gets traction and Reddit posts land; **50-100** if Show HN dies. Plan for the lower end.

---

## §10 — Open questions / re-evaluate at day-30 retro

1. Did Show HN deliver? If not, what's the better narrative for re-launch in 8 weeks?
2. Which Reddit sub had the best conversion? Double down there.
3. Are SEO pages indexing? (Check Google Search Console for impressions on `"vs BuildBuddy"` etc.)
4. Did any cold email reply turn into a real conversation? At what reply rate?
5. Should we open-source the cache server itself (currently only client SDK)? OSS shifts the narrative permanently — make the call deliberately, not by drift.

---

## §11 — Sources

- HN launch best-practices: <https://www.lucasfcosta.com/blog/hn-launch>, <https://www.markepear.dev/blog/dev-tool-hacker-news-launch>, <https://dev.to/dfarrell/how-to-crush-your-hacker-news-launch-10jk>
- HN guidelines (Show HN): <https://news.ycombinator.com/showhn.html>
- Show HN vs Launch HN: <https://news.ycombinator.com/item?id=16589820>, <https://news.ycombinator.com/item?id=43109280>
- Product Hunt 2025 guide: <https://www.wowtechub.com/blog/the-2025-guide-to-product-hunt-launches/>
- Tinybird PH: <https://www.producthunt.com/products/tinybird>
- Cal.com PH: <https://www.producthunt.com/products/cal>
- Indie Hackers conversion data: <https://awesome-directories.com/blog/indie-hackers-launch-strategy-guide-2025/>
- IH 100-users case studies: <https://www.indiehackers.com/post/how-i-got-my-first-100-users-2fc9d71c34>
- BuildBuddy GitHub (comparable launch): <https://github.com/buildbuddy-io/buildbuddy>
- Depot remote build caching post: <https://depot.dev/blog/remote-build-caching-secret-to-software-builds>
- Bazel Slack: <https://slack.bazel.build>, <https://bazelbuild.slack.com>
- Bazel community page: <https://bazel.build/community>
- CNCF Slack: <https://communityinviter.com/apps/cloud-native/cncf>
- Cross-posting / canonical strategy: <https://draft.dev/learn/syndicating-developer-content>, <https://townhall.hashnode.com/why-you-should-republish-your-devblog-posts-and-how-to-do-it>
- B2B cold-email benchmarks 2026: <https://martal.ca/b2b-cold-email-statistics-lb/>, <https://instantly.ai/blog/cold-email/>
- The Changelog podcast: <https://changelog.com/podcast>
- Turborepo remote cache landscape: <https://turborepo.dev/docs/core-concepts/remote-caching>, <https://github.com/ducktors/turborepo-remote-cache>
- Monorepo tool comparison: <https://monorepo.tools/compare>, <https://www.aviator.co/blog/monorepo-tools/>

---

**End of audit. Next action: commit and bring proposal to Day-0 foundation work (landing/demo/pricing freeze).**
