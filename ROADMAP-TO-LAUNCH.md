---
id: "ROADMAP-TO-LAUNCH"
type: "roadmap"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["roadmap", "launch", "solo-founder", "post-w36", "research-grounded"]
references:
  - "ROADMAP-TO-GA.md (v1.2.0 — predecessor enterprise-framed roadmap)"
  - "specs/_audits/2026-05-27-icp-customer-discovery.md"
  - "specs/_audits/2026-05-27-competitive-landscape.md"
  - "specs/_audits/2026-05-27-distribution-channels.md"
  - "specs/_audits/2026-05-27-pricing-benchmarks.md"
  - "specs/_audits/2026-05-27-solo-saas-playbook.md"
  - "specs/_audits/2026-05-27-plg-onboarding-framework.md"
  - "specs/_audits/2026-05-27-customer-development-playbook.md"
  - "specs/_audits/2026-05-27-legal-ops-setup.md"
  - "specs/_audits/2026-05-27-metrics-instrumentation.md"
  - "specs/_audits/2026-05-27-launch-readiness-check.md"
---

# CoreLink — Solo-Founder Roadmap to Launch

> **Replaces:** `ROADMAP-TO-GA.md` as the primary launch reference.
> `ROADMAP-TO-GA.md` is retained as the "enterprise-when-customer-asks"
> playbook (Phase 3 trigger list still valid).
>
> **Scope.** Operational, 8-phase plan from current state (Wave-32 deploy
> SEALED, 0/5 launch-gates GREEN per launch-readiness audit) through
> first $10k MRR. Synthesizes 9 Wave-1 research audits dated 2026-05-27.
> Owner = Gustavo, solo. No team mentions; no enterprise theater.

---

## §0 The 30-second pitch

> **CoreLink is the REAPI v2 + multi-package-manager cache for regulated
> polyglot engineering orgs who need BYOK, residency honesty, and a
> re-derivable audit log — without buying a build-farm they do not want
> to operate.**

[Source: competitive-landscape §4.] Two A/B alternates to test on the
landing page from week 4 (per pricing §6 + plg §4):

- *"The compliance-grade build cache. REAPI v2. BYOK on four KMS.
  Audit log your auditor can verify."*
- *"One cache for Bazel, Cargo, npm, pip, brew, and OCI. BYOK by
  default. Residency you can defend."*

**Brand DO-NOT list** (each phrase pulls us into a fight we lose, per
competitive §7): "faster builds", "remote execution / RBE", "Bazel" as
primary noun, "build events viewer", "smart monorepo / smart agents",
"credits", "zero config / auto-on", "build farm".

---

## §1 Who we serve (P1 ICP — Priya, Platform Lead)

[Source: icp-customer-discovery §3.1 + §4.]

| Dimension | Value |
|---|---|
| Role | Platform / DevEx Lead, "Head of DevOps", or first platform hire |
| Company | Series B/C SaaS, fintech, healthtech; 50–250 eng; ARR $10M–$100M |
| Stack | Bazel or sccache + Go/Rust/Scala monorepo; GitHub Actions or Buildkite; ships Docker to ECR/GAR |
| Top pain | CI 28–45 min; sccache-on-S3 ops cost; SOC 2 auditor asked "how do you prove the cache wasn't tampered with" — no answer |
| Current workaround | Self-hosted `bazel-remote` on EC2 + S3, OR raw sccache + S3, OR bigger CI runners |
| Budget authority | $500–$2,000/mo on corporate card, no VP-Eng signoff needed |
| Where heard about new tools | r/bazel, bazel-discuss, Platform Engineering Slack, HN front page, Platform Eng / DX podcasts, DPE Summit |
| Trial-to-buy | 3–14 days self-serve; runs shadow-cache comparison vs. current setup |

**Why P1 first (not P2 / P3).** The seven-factor matrix (icp §4.1)
collapses to: (a) the pain is acute, (b) the budget exists under signoff
ceiling, (c) the solo-founder bus-factor objection is survivable
("BYOK + $X/mo escape clause"), (d) CoreLink's invariant-grounded
posture matches the SOC-2-prep checklist exactly, (e) 3–14-day self-serve
fits a solo founder, (f) reference-class fuel for P2 sales 12–18 months
out.

**Anti-ICP** [icp §5]:

- **P3 hobbyists / OSS / side projects** — will not pay; free-tier abuse
  risk. Posture: keep free tier so they self-onboard; no marketing or
  feature work targeted at them.
- **P2 FAANG / Fortune-500** — 3–9-month procurement cycles fatal for
  solo. Posture: "join waitlist", revisit after 12 months of P1 proof.
- **Mobile-only Gradle / Android** — Develocity owns this; revisit year 2.
- **Pure JS/TS Turborepo / Nx** — Vercel-coupled / Nx-native ecosystem
  mismatch; add adapter post-GA only on P1 customer ask.
- **Defense / federal / classified** — FedRAMP timeline 12–24mo; revisit
  at $5M ARR with hired compliance lead.

---

## §2 Why CoreLink wins (positioning)

[Source: competitive-landscape §3.3–3.5.]

### Three win segments

1. **Regulated mid-market with multi-cloud KMS estates.** Fintech /
   healthtech / gov contractors needing BYOK across AWS+GCP+Azure +
   re-derivable audit. No §1 competitor advertises 4-KMS BYOK at the
   cache tier.
2. **Polyglot orgs spanning Bazel + Cargo + npm + OCI in one buying
   decision.** Today they stitch BuildBuddy + sccache+S3 + Turborepo +
   private registry. CoreLink offers one cache, one bill, one audit log.
3. **Engineering-trust buyers who read invariants.** Teams that open
   `specs/03_architecture/tla+/` and check the model. Niche, high-signal,
   become design partners.

### Three lose segments (honest)

1. **Bazel-monolith shops wanting RBE + cache + build-events UI in one
   buy.** BuildBuddy / EngFlow. CoreLink has no RBE.
2. **JS/TS monorepos already on Vercel.** Turborepo Remote Cache is
   free + auto-on; cannot beat "already on by default".
3. **Solo devs / OSS maintainers with $0 budget.** sccache + R2 is
   unbeatable on price. Do not compete here.

### Two blue-ocean niches

1. **"REAPI cache with verifiable audit for the SOC 2 auditor's first
   interview."** Sell the audit log as the product; the cache is the
   carrier. No competitor leads with this.
2. **"BYOK-by-default polyglot cache for the EU mittelstand."**
   Multi-KMS + honest residency + Cloudflare `weur` data-plane.

---

## §3 Pricing v0.1

[Source: pricing-benchmarks §5.]

| Tier | Price | Quota | Auth | Support |
|---|---|---|---|---|
| **Free / Starter** | $0, no card | 10 GB cache / 500k req/mo / 1 workspace / unlimited collaborators | GitHub OAuth | GitHub Discussions |
| **Pro / Team** | **$25/mo** or **$250/yr** (2 months free) | 500 GB / 20M req/mo / unlimited workspaces | GitHub + email/password | Email, 1-business-day target |
| **Enterprise** | Contact (anchor $500–$5,000/mo) | BYOK (4 KMS), dedicated tenant, 99.9% SLA + credits, SSO/SAML, DPA, audit-log export | SSO/SAML | DPA + custom MSA |

**Why this shape.** Free tier matches CF R2 free tier exactly → zero
marginal cost → "actually free forever", not a hidden trial. $25 lands
inside the $20–$30 solo-founder anchor window (Resend $20, Garnix $25,
BetterStack $25, Sentry $26). Gross margin ~75–90% on Pro per pricing
§3 worked example. No per-seat (only Nx Cloud does, and it's tiny).
No usage rate card (Tinybird-style requires a finance buyer). Three
tiers — anything more doubles the cognitive cost.

**Quota policy.** Free + Pro both **hard-cap** at 100% with email
warning at 80% (Sentry / Plausible playbook). No silent overage billing
at v0.1. Forces upgrade conversation rather than surprise bills.

**A/B test plan, first 4 post-launch weeks** [pricing §6]:

- **Test #1 (run first):** Pro = $19 vs $25 vs $29. Split landing-page
  traffic three ways for 4 weeks. Decide by week 5. Minimum n=200 trial
  signups before deciding. Hypothesis: $25 wins (signals "team tool").
- Subsequent tests, one at a time, 4-week windows: free-tier ceiling
  (10/25/50 GB), tier name (Pro vs Team vs Build), annual phrasing
  ("2 months free" vs "20% off"), overage policy (hard cap vs $0.05/GB).
- **Discipline:** ONE test at a time. Never simultaneous.

**Disagreement noted.** pricing §5 names the Pro tier "Team"; plg §1.4
recommends a 5 GB / 50 GB freemium. Adopted reconciliation: name "Pro",
quotas 10 GB / 500k req (matches CF R2 free tier exactly — zero marginal
cost is the deciding factor). Revisit at month 3.

---

## §4 Onboarding (TTFV ≤ 10 min, activation = `first_cache_hit`)

[Source: plg-onboarding-framework §3–§4.]

| Metric | Target | Stretch |
|---|---|---|
| Median TTFV (signup → first cache hit) | **≤ 10 min** | ≤ 5 min |
| p75 TTFV | ≤ 20 min | ≤ 10 min |
| Median signup → authenticated `corelink ping` | ≤ 5 min | ≤ 2 min |
| % signups reaching activation same session | ≥ 35 % | ≥ 50 % |

**Canonical activation event.** `first_cache_hit` = the **second**
successful CAS read for the same content-addressed key inside a tenant,
where the first read was a miss-then-write within the previous 24 h.
Single SQL predicate, hard to fake, implies value received, reachable
in one session. Source: plg §3.2.

**New onboarding flow (replaces 6-step wizard with 2 mandatory steps):**

1. Land → single primary CTA "Start free — 10 GB free monthly"
2. Click → Clerk sign-up with **GitHub OAuth first**, email/password second
3. Auto-provision tenant + free plan + nearest region (Geo-IP) + PAT (server-side, invisible)
4. `/welcome` shows three things: one-line copy-paste install, PAT (copy button), live status pane
5. User runs `curl -fsSL https://get.corelink.io | sh -s -- --token=ct_xxx --region=ord` (writes config, runs `corelink ping`)
6. User runs `corelink bazel-init` (CLI detects `WORKSPACE` / `MODULE.bazel`, appends 3 lines to `.bazelrc` idempotently)
7. User runs `bazel build //...` twice → second build is mostly cache hits → `/welcome` pane animates "First cache hit. Your build was 8× faster."

**Deferred out of wizard** (per plg §4 + customer-dev §2.13):

- **DPA accept** → moves to first team-member invite (the moment the
  tenant actually becomes a data controller for someone else). Solo-of-one
  needs only the privacy notice link in the footer per LGPD Art. 9.
- **Billing / card on file** → never asked at signup. Asked only at
  upgrade click, via **Stripe Checkout Session (hosted)**, not in-app
  Elements — kills the `BillingStep.tsx` scaffold problem.
- **Region pick** → Geo-IP default; change in Settings.
- **Plan pick** → everyone starts Free.
- **Tenant name** → defaults to `${github_handle}-default`.

**Patterns explicitly skipped** (plg §6):

- No public sandbox (build-cache value needs user's own artefacts)
- No reverse trial (paid features are quotas, not capability)
- No CC upfront (signup volume collapses 2-5×)
- No drip campaigns pre-activation
- No Pendo/Userflow click-through tour (wrong shape for devs)

---

## §5 North Star Metric: W-CHPA

[Source: metrics-instrumentation §2.]

**NSM = Weekly Cache-Hit-Producing Accounts.** Distinct tenants whose
API key produced ≥1 successful cache HIT (HTTP 200 on
`GET /v1/cache/{key}` serving bytes) in the trailing 7 days. Single D1
query, no external dependency.

**Year-1 targets (conservative, design-partner-grade):** M1 = 10 W-CHPA,
M3 = 30, M6 = 100, M12 = 300. Beating them = re-plan.

**Tier-2 inputs (6 metrics that drive Tier-1):**

| # | Metric | Healthy band |
|---|---|---|
| T2-01 | Signups / week | Trending up; M3 target = 10/wk |
| T2-02 | Activation rate (signup → first HIT within 7d) | ≥30% median, aim 50%+ |
| T2-03 | Time-to-first-HIT median | <60 min good, <10 min elite |
| T2-04 | Cache-hit rate per active tenant | ≥70% healthy; <50% sustained = bug |
| T2-05 | D7 retention | ≥40% acceptable, ≥60% strong |
| T2-06 | MRR & free→paid % | F2P ≥5% baseline, ≥10% strong |

**Tier-3 diagnostics off-dashboard** (Sentry / Cloudflare Analytics /
Stripe / Clerk / Drata dashboards). Consulted reactively when Tier-1/2
breaks bands. Never on the weekly review.

**Cadence.** Daily 3-min phone glance (yesterday HITs + Stripe today);
weekly 15-min Monday digest email (auto-generated by Cron Worker, Monday
09:00 BRT); monthly 60-min deep-dive (cohort matrix, churn list, "one
thing to change"). Quarterly: Sean Ellis PMF survey. Total budget
~90 min/month.

**Pre-committed decision rules** [metrics §6]:

1. **T2-02 activation <30% for 4w** → pause all marketing, fix
   onboarding funnel (PostHog instrumentation), no acquisition until ≥30% for 2 weeks.
2. **T2-05 D7 retention <20% for 4w** → stop the underperforming
   channel; if only one channel, PMF survey + pivot decision.
3. **Monthly logo churn >5% for 2 months** → personally call last 5
   churned tenants; synthesis as sealed audit.
4. **MRR <$1k by month 3** → signal not kill-switch. Run PMF survey;
   ≥40% continue, <40% formal pivot decision at month 4.
5. **PMF survey "very disappointed" <25%** → no PMF; triage ICP / wedge / product.
6. **Per-tenant cache-hit <50% for 2w with >100 req/wk** → personally
   reach out; bug or config issue.
7. **Analytics stack >$50/mo** → cut the most expensive tool, not metrics.

---

## §6 Phases (concrete, time-boxed)

> **Cumulative wall-clock target: ~8 weeks from today (2026-05-27) to
> public launch + iteration. Phase 4 runs weeks 9–16; Phase 5 trigger-based.**

### Phase 0 — Fix the broken (weeks 1–2)

[Source: launch-readiness-check §6. Current 0/5 GREEN.]

Operator + code work to flip the 2 RED + 2 YELLOW gates. Order matters
(each unblocks the next).

| # | Item | Effort | Status to flip |
|---|---|---|---|
| 0.1 | Apply Pages secrets (`CLERK_SECRET_KEY`, `STRIPE_SECRET_KEY`, `RESEND_API_KEY`) to `corelink-admin-ui` | ~30 min operator | L2 RED → unblocks everything |
| 0.2 | Re-smoke `/sign-up`, `/en/onboarding`, `/en/legal/*`; capture Pages logs if still 500 | ~15 min operator | L2 verify |
| 0.3 | Author/redirect `/legal/{privacy,terms,sub-processors}` on docs site (add `@docusaurus/plugin-client-redirects` or canonical `apps/docs/src/pages/legal/*.tsx`) | ~2 h code | L4 RED → GREEN |
| 0.4 | Add hero block + single primary CTA to `apps/docs/docs/index.mdx` (demote Diátaxis cards below) | ~2 h code | L1 YELLOW → GREEN |
| 0.5 | Replace `BillingStep.tsx` scaffold with **deletion** + Stripe Checkout Session redirect on upgrade click | ~2 days code | L2 + L3 unblock |
| 0.6 | Lift `provisional: true` from `apps/docs/src/lib/pricing.ts` per §3 above; swap CTAs to Checkout Session | ~30 min | L3 YELLOW → GREEN |
| 0.7 | Unstub pricing page to match §3 (3 tiers: Free/Pro/Enterprise) | ~3 h code | L3 GREEN |

**Gate to Phase 1.** All 5 launch-readiness items GREEN.

### Phase 1 — Pre-launch foundation (weeks 3–4)

| # | Item | Effort | Source |
|---|---|---|---|
| 1.1 | Ship `https://get.corelink.io` install-script Worker + `corelink` CLI flag handling | ~2 days | plg §5 |
| 1.2 | Implement auto-provision-on-signup (replaces wizard `tenant`+`region-plan`+`pat`) | ~3 days | plg §5 |
| 1.3 | Implement `corelink bazel-init` (stub `buck2-init`, `cargo-init` for later) | ~1 week | plg §5 |
| 1.4 | Build `/welcome` SSE pane (replaces `/onboarding/done`) | ~3 days | plg §5 |
| 1.5 | Define `first_cache_hit` event in CAS data plane, deduped per tenant | ~1 day | plg §3.2 |
| 1.6 | Ship `humangr-labs/corelink-bazel-example` demo repo (200 LOC, pre-applied bazel-init) | ~2 days | plg §5 |
| 1.7 | File Stripe Atlas DE C-Corp ($500 one-time); open Mercury during Atlas flow | ~1 h founder | legal-ops §2.3 + §7.1 |
| 1.8 | File **83(b) within 30 days** of incorporation | ~30 min founder | legal-ops §7.1 — single most expensive mistake to miss |
| 1.9 | Buy Termageddon $119/yr; publish Privacy + ToS + Cookie + Disclaimer; adopt Common Paper DPA + CSA + SLA (free, CC BY 4.0); build `/sub-processors` page | ~3 h founder | legal-ops §1.3 + §7.3 |
| 1.10 | Plausible $9/mo on docs; PostHog Cloud Free in admin-ui; Sentry Free DSN in 3 apps + workers; Axiom Free for CF Worker logs | ~2 h | legal-ops §5 + metrics §8 |
| 1.11 | Wire weekly digest Cron Worker (Monday 09:00 BRT) → 7 saved D1 SQL views → §5 email | ~8 h | metrics §8.4 |
| 1.12 | Cal.com Free single 30-min "Discovery / Sandbox Tour" event, linked from landing + footer | ~30 min | customer-dev §7 + legal-ops §4 |
| 1.13 | HubSpot Free CRM seeded with the 50 P1 accounts from §1 | ~30 min | customer-dev §7 |
| 1.14 | **Build 50-account ICP list**: job-posting mining (LinkedIn / Ashby / Greenhouse: `"Bazel" AND "remote cache"`, `"sccache" AND "CI"`); GitHub signal (`path:.bazelrc remote_cache`); HN/Reddit complaints last 90 days | ~1 day founder | customer-dev §6 wk1 |
| 1.15 | Pre-write Show HN draft (title, first comment, repo polish) per distribution §4 template | ~2 h | distribution §4 |
| 1.16 | 20-min customer interview script ready (verbatim from customer-dev §3); recording + recap-email templates | ~1 h | customer-dev §3 |

### Phase 2 — Soft launch / customer discovery (weeks 5–6)

[Source: customer-dev §6 wk2–wk4 + solo-saas §5 P2/P11.]

| # | Item | Effort |
|---|---|---|
| 2.1 | Mon wk5: identify mutuals for all 50 accounts; send warm-intro asks (template customer-dev §4.1) | 3 h |
| 2.2 | Tue–Wed wk5: send cold emails to ~25–35 accounts without mutuals (template §4.2). Cap 15/day from personal `gustavo@humangr.com`, no bulk-mailer | 6 h |
| 2.3 | Thu wk5: Twitter DM (§5 reply-then-DM pattern) to 10 accounts with strong social presence | 2 h |
| 2.4 | Fri wk5: follow-up touch 2 for non-responders (§4.3) | 1 h |
| 2.5 | Weeks 5–6: book 12+ interviews; run them 3–4/call/week max; Mom-Test discipline (no demo, ≤30% talk-time, recap email within 60 min) | ongoing |
| 2.6 | **MID-CYCLE DECISION GATE (Day 22 of Phase 2).** Score first 6 interviews /25. If <3 score ≥18 → pause outreach, re-scope ICP (broaden one axis), re-run §1 list-build on narrower segment | 2 h decision |
| 2.7 | Build the 1–2 things customers explicitly ask for (Forge dogfood + most-requested integration) | up to 1 week |
| 2.8 | First "build-in-public" Twitter posts: 2x/week tech findings (no pitch); per solo-saas §5 P6 (opt-in only) | ongoing |

**Gate to Phase 3.** ≥12 completed interviews; ≥4 score ≥18/25; ≥6 referrals received. If not: do not proceed to Show HN; re-segment ICP.

### Phase 3 — Show HN + wide launch (weeks 7–8)

[Source: distribution-channels §2 + §3 + solo-saas §5 P3.]

| Day | Item | Source |
|---|---|---|
| Tue 7:00 PT wk7 | **Show HN.** Title: `Show HN: CoreLink – self-hosted REAPI cache for Bazel/sccache with BYOK + verifiable audit`. NOT a marketing post — technical deep-dive (TLA+ tenant-isolation model + RFC-6962 audit chain + 4-KMS BYOK). First comment within minutes from same account: architecture in 3-5 bullets + pricing + roadmap items wanting feedback + repo link. **Be at keyboard 6 h.** Reply to EVERY comment. Tailscale-Avery pattern: 24 h of manual activations + signup hand-holding. | distribution §4 |
| Same day wk7 | X thread (template distribution §5) tied to Show HN, hour of submission. Pin thread for the day | distribution §5 |
| Same day wk7 | BazelBuild Slack `#general`: "Just shipped, would love feedback" (after 2-week prior lurk-and-help). NOT a pitch | distribution §1 |
| +1 day | Reply to overnight HN comments | — |
| +2 days | r/bazel post: tech writeup (architecture deep-dive), CoreLink as substrate not headline | distribution §1 row 7 |
| +3 days | IH milestone post: "Launched on HN — here's what happened" | distribution §3 |
| +4 days | r/devops post: "we built this to fix X" framing | — |
| +5 days | dev.to tutorial: "Set up CoreLink with Bazel in 5 minutes" (with `canonical_url` back to own domain) | distribution §1 row 4 |
| End wk8 | First retro: per-channel UTM table; which channel converted best | metrics §4 |

**Hard pause triggers** during Phase 3 [distribution §3]:

- Signup → activation <10% after 100 signups → fix product before more marketing
- Show HN dies before front page (<20 upvotes in 2h) → don't burn other channels; debug + re-launch in 8 weeks with better angle
- Production incident >2 h during launch window → pull all amplification, focus on stability

### Phase 4 — Iterate to PMF (weeks 9–16)

[Source: customer-dev §6 wk5–wk8 + solo-saas §5 P11.]

| # | Item |
|---|---|
| 4.1 | White-glove integration (Collison installation) for top 3 most-engaged interviewees. Free; founder hands on their machine or pair-programming. |
| 4.2 | Lightweight 1-page **design-partner agreement**: 6 months free + direct Slack access to founder; in return, public logo + quote when it works. |
| 4.3 | Build the ONE friction unanimously called out in wk3-4 interviews (their favorite, not yours). |
| 4.4 | Convert design partners to paid (script: *"We've been running CoreLink against your stack for X weeks. You saw [metric]. Move to paid: $25/mo or $1 for first 3 months if budget cycle is blocker. Point isn't revenue; it's prioritized feature requests + direct line."*) — Lenny's "step 2: one company pays meaningfully". |
| 4.5 | Sean Ellis PMF survey to all tenants with ≥2 HITs in last 14 days. Target ≥40% "very disappointed". |
| 4.6 | Comparison pages published in priority order [competitive §5]: CoreLink vs BuildBuddy → vs EngFlow → vs bazel-remote+S3 → vs sccache+S3 → vs Nx Cloud → vs Turborepo. 1,500–2,500 words each, honest, link to public TLA+ spec + audit-log re-derivation walkthrough. |
| 4.7 | Educational content cadence: one technical deep-dive per 2 weeks (BLAKE3 internals, CAS vs LRU, audit-chain re-derivation walkthrough, Forge dogfood numbers). Plausible-Marko pattern. |
| 4.8 | Bug-bash + retention focus. Every customer email answered personally within 24 h. No Intercom / Zendesk before 200 customers. |

**Phase 4 success.** ≥10 paying customers (≥1 paying >$100/mo); Sean
Ellis ≥40% on n≥10 active users; cumulative ≥1 case study a customer
puts their name on.

**Phase 4 failure recovery:**

- 10 customers but <40% "very disappointed" → signups, not PMF. Do NOT
  scale marketing; interview the non-disappointed cohort.
- <10 customers but >40% "very disappointed" → PMF real, distribution
  bottleneck. Re-run Phase 1 list-build on wider account set.
- Both fail → ICP wrong. Honest move is pivot conversation, not another
  sprint.

### Phase 5 — Trigger-based scaling (post-PMF)

Pre-committed responses to revenue milestones. No date-based gate;
revenue triggers them.

| Trigger | Action | Source |
|---|---|---|
| **$1k MRR** | v0.2 launch tweet; review pricing; kill free-tier abuse | solo-saas §7 |
| **$5k MRR** | Swap support: Gmail → Plain Foundation ($35/mo) | legal-ops §6.3 |
| **$10k MRR** | Cut over Lemon Squeezy → Stripe direct + Stripe Tax (saves ~$200/mo at $10k MRR); engage US CPA ($2k/yr); engage BR contador; buy E&O + Cyber ($1,500/yr starter); open hiring question (still NO is valid) | legal-ops §7.6 + solo-saas §7 |
| **First enterprise asks "do you have SOC 2?"** | Start Drata (already wired per Wave-32) → Type I 6mo lead-time. Honest "starting next month" answer accepted by most | ROADMAP-TO-GA Phase 3 |
| **First DPA redline** | One-shot $300–$800 lawyer review (UpCounsel / Atrium / Lexion) only on customer redline | legal-ops §7.6 |
| **First SAFE / priced round** | Upgrade legal to Cooley / Goodwin / Orrick free-startup tier; D&O insurance | legal-ops §7.6 |
| **$30k MRR** | Open international / multi-region / multi-language question; consider 2nd hire | solo-saas §7 |
| **$50k ARR or first enterprise** | SOC 2 Type I via Drata (already configured) | legal-ops §7.6 |

---

## §7 What we DON'T do pre-PMF

[Sources: solo-saas §6, distribution §7, customer-dev §8, pricing §7, plg §6.]

- **No paid ads** (Google / Meta / LinkedIn). Burns runway, teaches nothing.
- **No conferences / BazelCon booth / KubeCon attendance** as primary channel ($5k+ all-in, opportunity cost). Re-evaluate at $5k MRR.
- **No SDR / outbound sales motion / cold-LinkedIn**. Wrong audience-targeting; P1 lives on HN/Reddit/Slack.
- **No 5-tier pricing.** Three tiers max (Free / Pro / Enterprise).
- **No per-seat pricing pre-$1M ARR.** Adoption tax exactly when buyer wants to spread the tool.
- **No usage rate-card / credits abstraction.** Buyer who needs a spreadsheet to estimate next month's bill will not buy.
- **No SOC 2 preemptively.** $20–40k useless without trust-fabric of customers; Phase 5 trigger only.
- **No formal pentest preemptively.** $5–15k, do on customer ask only.
- **No enterprise gate review / 13-signer Go/No-Go.** Wrong scale for solo.
- **No PR firm / press releases.** Tech press doesn't cover infra startups under $1M ARR.
- **No hiring before $10k MRR.** DHH, Linear, Levels — all explicit.
- **No raising outside capital before $30k MRR**, and only if the product wants to be enterprise-scaled (it may not).
- **No feature built without ≥2 distinct paying customers requesting it.**
- **No Intercom / Zendesk / ticketing system before 200 customers.** Plain Gmail.
- **No pixel-polishing marketing site before shipping product.** README first.
- **No Show HN twice in 4 weeks** (community remembers); save re-launches for genuine milestones (GA, major feature, OSS milestone).
- **No targeting "everyone"** — every adjacent persona ignored is bandwidth saved.
- **No multi-region / multi-language / multi-currency commitment before $30k MRR.**
- **No Brazilian LTDA** until first BR-invoiced customer in R$ or first BR employee.
- **No Drata switch / Vanta switch** — already wired per Wave-32, stay.

---

## §8 Risk register (solo-founder failure modes)

[Source: solo-saas §4 + customer-dev §6 + plg §2.]

| # | Risk | Pre-mitigation |
|---|---|---|
| R1 | **Uku-Täht year-1 stall** (solo doing dev + marketing simultaneously stalls at $400 MRR for 14 months) | After 6 months, if MRR < $1k AND PMF ≥40% on small N, evaluate: recruit a marketing co-founder à la Marko, or commit to 12–24-month solo grind. Not before. |
| R2 | **Premature scaling** (paid ads / conferences before PMF) | §7 hard "no" list; Sean Ellis ≥40% gate before any paid acquisition |
| R3 | **Building unrequested features** (42% of SaaS failures cite no market need) | Rule: no feature without ≥2 distinct paying customers asking; design-partner Slack as the requirements channel |
| R4 | **First 3 design partners churn before feedback captured** | Hand-deliver onboarding for first 3; weekly 15-min check-in calls (carries forward from ROADMAP-TO-GA R-X1) |
| R5 | **Enterprise asks everything at once and solo can't deliver** | Honest "starting SOC 2 next month" answer ready; most accept roadmap commitment in lieu of cert |
| R6 | **Real-world traffic surfaces invariant bugs missed by property tests** | Canary deploy 5%→25%→100% (S-13 substrate); auto-rollback at error-budget breach (carries from R-X3) |
| R7 | **Solo-founder burnout from oncall** | Phase 1 oncall = best-effort, not 24/7 SLA; honest with customers about response window |
| R8 | **Vendor outage during customer's first hour** (Clerk / Stripe / CF) | Status page + vendor SLAs; document fallback per WI-S19-001 chaos handling |
| R9 | **Free-tier abuse before enforcement** | Hard cap at 100% + email warning at 80% per §3; per-tenant quotas already wired (S-10 + S-13); daily R2 storage cost monitor |
| R10 | **Solo-founder cashflow gap** (no Brazilian dividend treaty + 10% withholding effective 2026-01-01) | Pay self as US-source contractor invoice from Brazil; defer dividend question until exit/acquisition; Q4 2026 review with BR contador |
| R11 | **Mercury KYC rejects Brazilian founder** | Fall back to Relay ($0/mo, similar features) or Wise Business ($31/mo paid plan) |
| R12 | **Stripe Checkout breaks mid-launch** | Webhook signature verify + idempotency wired (WI-S19-004); test with real $1 charge before any launch gate marked done |

---

## §9 Cost ladder

[Source: legal-ops §6.]

| Phase | Recurring | Notes |
|---|---|---|
| Pre-launch (Phase 0–1) | **~$24/mo** + $500 Atlas one-time + $300/yr DE franchise prorated | Termageddon $9.92/mo + CF Workers Paid $5/mo + Plausible $9/mo. Everything else free tier. |
| Launch (Phase 3, ~$0 MRR) | **~$24/mo** + 5.4% of revenue (Lemon Squeezy MoR) | No incremental tooling; defer Plain / Resend Pro |
| Phase 4 ($2–5k MRR) | **~$70–90/mo** + 5.4% of revenue | + Resend Pro $20/mo + Plain Foundation $35/mo + Notion Plus $10/mo |
| Phase 5 ($10k MRR) | **~$400–500/mo** + 3.7% of revenue | Swap MoR → Stripe direct + Stripe Tax 0.5%; + Sentry Team $26/mo; + US CPA ~$167/mo amortized; + E&O+Cyber ~$125/mo |

**Year-1 ops / legal / tooling burn (excluding salary, payment-fee
percentage, infra-at-scale): ~$3,800.** Single hour of US tech-startup
lawyer ($400–650) costs more than the entire Year-1 launch legal stack.

**Payment processing choice.** Launch with **Lemon Squeezy / Stripe
Managed Payments** (5% + $0.50, MoR handles VAT/MOSS/US-state-nexus).
Cut over to Stripe direct + Stripe Tax (0.5%) at $10k MRR or first
enterprise pilot. Pilots > $5k ACV: Stripe Invoices directly (no MoR).

---

## §10 Decision gates (pre-committed)

Each is bound to a single metric + window + action. Monday-morning
emotion does not vote.

| # | Trigger | Window | Action |
|---|---|---|---|
| DG1 | Phase-0 launch-readiness | once | Do not announce until all 5 gates GREEN |
| DG2 | Phase-2 ICP signal | Day 22 of Phase 2 | <3 of 6 interviews score ≥18/25 → pause outreach, re-scope ICP, re-run §1 list-build on narrower segment |
| DG3 | Show-HN debug | 2 h after submission | <20 upvotes → don't burn other channels; debug + re-launch in 8 weeks |
| DG4 | Activation | 4 consecutive weeks | T2-02 <30% → pause marketing, fix onboarding |
| DG5 | Retention | 4 consecutive weeks | T2-05 D7 <20% → stop underperforming channel |
| DG6 | Churn | 2 consecutive months | Logo churn >5% → personally call last 5 churned + sealed audit |
| DG7 | Revenue signal | month 3 | MRR <$1k → Sean Ellis survey; ≥40% continue, <40% formal pivot decision at month 4 |
| DG8 | PMF survey | quarterly | <25% "very disappointed" → no PMF; ICP / wedge / product triage |
| DG9 | Per-tenant QoS | 2 consecutive weeks | Per-tenant cache-hit <50% with >100 req/wk → personally reach out |
| DG10 | Cost creep | monthly | Analytics stack >$50/mo → cut most expensive tool, not metrics |

---

## §11 Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-27 | Gustavo (via Claude Opus 4.7) | Initial research-grounded launch roadmap synthesizing 9 Wave-1 research audits (ICP, competitive, distribution, pricing, solo-SaaS playbook, PLG/onboarding, customer development, legal-ops, metrics-instrumentation) plus the launch-readiness check. Replaces enterprise framing of `ROADMAP-TO-GA.md` (which retains the Phase-3 enterprise-when-asked trigger list as fallback). 8 phases (0–5 + DON'T list + risks + cost ladder + decision gates). Reconciles two cross-audit disagreements: (a) free-tier ceiling — adopted pricing §5's 10 GB (matches CF R2 free tier exactly) over plg §1.4's 5 GB; (b) Pro tier name — adopted pricing §5's "Pro" framing (maps to buyer pattern recognition). All other recommendations align across the 9 audits. |
