---
id: "AUDIT-2026-05-27-CUSTOMER-DEVELOPMENT-PLAYBOOK"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "go-to-market", "customer-development", "solo-startup", "playbook", "mom-test", "jtbd", "pmf"]
---

# Customer Development Playbook — Find CoreLink's First 10 Customers

> **Scope.** Research-backed, copy-paste-ready customer development playbook for a solo founder taking CoreLink (shared content-addressable cache for builds + packages + Docker + ML) from zero to first 10 paying customers in 60 days. No SDR, no $5k/mo on Apollo, no marketing budget.
>
> **Method.** Synthesizes Rob Fitzpatrick's *The Mom Test*, Bob Moesta's *Jobs-to-be-Done* Switch interview, Rahul Vohra's *Superhuman PMF engine* (40% benchmark), Paul Graham's *Do Things That Don't Scale*, Lenny Rachitsky's B2B PMF guide, Pieter Levels' build-in-public tactics, and indie hacker first-10-customer retrospectives.
>
> **Companion docs.** `specs/_audits/2026-05-27-launch-readiness-check.md` (landing/signup gates) — this audit assumes those gates are passing before Week 2 of the 60-day plan.

---

## §1 Frameworks cited

| # | Framework | Source | Applied where |
|---|-----------|--------|---------------|
| 1 | **The Mom Test — 3 rules** | Rob Fitzpatrick, *The Mom Test* (2013). Synthesis: <https://www.atlantaventures.com/blog/the-3-rules-to-customer-interviews-from-the-mom-test> | §2 (rules), §3 (interview script) |
| 2 | **JTBD Switch Interview + 4 Forces of Progress** | Bob Moesta + Chris Spiek, *Competing Against Luck*. Talk: <https://businessofsoftware.org/talks/bob-moesta-and-chris-spiek-uncovering-the-jobs-to-be-done/>. Founder's guide: <https://gonogo.team/jobs-to-be-done> | §3 (questions Q6–Q10 use push/pull/anxiety/habit) |
| 3 | **Superhuman PMF Engine — 40% "very disappointed"** | Rahul Vohra, First Round Review: <https://review.firstround.com/how-superhuman-built-an-engine-to-find-product-market-fit/>. Coda playbook: <https://coda.io/@rahulvohra/superhuman-product-market-fit-engine> | §6 Week 8 (Sean Ellis survey to first 10 users) |
| 4 | **Do Things That Don't Scale** | Paul Graham (2013): <https://www.paulgraham.com/ds.html>. "Collison installation," Airbnb door-to-door photos | §6 (manual onboarding, white-glove integration) |
| 5 | **B2B PMF — one company first** | Lenny Rachitsky: <https://www.lennysnewsletter.com/p/finding-product-market-fit> | §6 (Step 1: one company to love it; Step 2: one company to pay meaningfully) |
| 6 | **Build-in-public + Twitter virality** | Pieter Levels case studies: <https://www.systemscowboy.com/pieter-levels-indie-hacker-strategy/>, <https://www.onemilliongoal.com/p/pieter-levels-the-king-of-indie-hacking> | §5 (Twitter DM + build-in-public adjacency) |
| 7 | **HN Show HN launch mechanics** | <https://www.markepear.dev/blog/dev-tool-hacker-news-launch>, <https://dev.to/dfarrell/how-to-crush-your-hacker-news-launch-10jk> | §6 Week 6 (Show HN) |
| 8 | **Cold email conversion benchmarks** | Clay GTM blog: <https://www.clay.com/blog/b2b-cold-email-templates>; Smartlead: <https://www.smartlead.ai/blog/cold-email-templates>; Autobound: <https://www.autobound.ai/blog/cold-email-templates-guide> | §4 (template length 50–125 words, 5–15% reply rate target, 3–5 follow-ups) |
| 9 | **First-10-SaaS-customer indie playbook** | Superframeworks: <https://superframeworks.com/blog/get-first-10-customers>; Indie Hackers thread: <https://www.indiehackers.com/post/b2b-saas-founders-how-did-you-find-your-first-10-customers-8310b994e4> | §6 (community-led validation, 20–30 valuable contributions before pitch) |
| 10 | **CRM-lite for solopreneurs** | <https://editorialge.com/crm-for-solopreneurs/>; <https://getcoherence.io/blog/solo-founder-crm-comparison-guide-2026> | §7 (HubSpot Free recommendation + fallback) |

---

## §2 Top 5 Mom-Test rules applied to CoreLink interviews

| # | Rule (Fitzpatrick) | CoreLink-specific translation |
|---|--------------------|-------------------------------|
| 1 | **Talk about their life, not your idea.** | NEVER say "CoreLink" in the first 18 minutes. Talk about *their* CI, *their* Docker image rebuilds, *their* ML pipelines. The word "cache" should come from *them*, not you. |
| 2 | **Ask about past behavior, not hypotheticals.** | Banned: "Would you use a shared cache?" Required: "Walk me through the last time a CI run took >30 minutes — what was rebuilding?" |
| 3 | **Listen more than you talk.** | Hard target: founder talks ≤ 30% of the 20-minute call. If you finish a call and you talked > 7 min, the data is contaminated — flag it in your notes. |
| 4 | **Anchor pain in time/money/specifics.** | After every pain mention, ask one of: "How often does that happen?", "How long did it cost you?", "What did you do instead?", "Who else got pulled in?" Vague pain = no pain. |
| 5 | **A compliment is a red flag.** | "That sounds cool" / "interesting idea" / "we'd love to try it" = ZERO signal. Only signals: (a) they pulled out their calendar to schedule a follow-up, (b) they introduced you to a colleague *during* the call, (c) they asked "how do I install it right now?". Log compliments in a separate "fluff" column — never as validation. |

**Two non-Mom-Test rules borrowed from Moesta JTBD that pair with the above:**

- **Anchor every story to a specific date.** "What was happening the week you first searched for a build cache?" — dates unlock the push/pull forces. Without a date, you get a generalization.
- **Ask "Why was today the day?"** — this is the single best JTBD question for users who recently adopted a competing tool (BuildBuddy, EngFlow, sccache, Bazel Remote Cache, Nx Cloud, Turborepo Remote Cache). It surfaces the *triggering event*.

---

## §3 Customer interview script (verbatim, 20 minutes)

> **Pre-call setup.** Record (with consent) via Otter or Granola. Camera on. No screenshare of CoreLink, ever. Have *their* GitHub org open in a tab so you can reference their tech stack without asking. Keep a paper notepad for "I'm taking notes" optics; type into a structured doc after the call.

**Opening (60 sec) — set the frame, kill sales anxiety**

> "Thanks for the 20 minutes. Quick context: I'm not selling anything today and I'm not going to demo anything. I'm researching how teams like yours handle [build/CI/Docker/ML — pick the one matching their stack]. The most useful thing you can do is talk about *your* current workflow, not react to my ideas. Cool?"

This line single-handedly raises data quality by 2x — it gives the interviewee permission to be candid and signals you won't waste their time pitching.

---

### Block A — Problem validation (5 questions, ~8 min). Goal: confirm the pain exists in *their* past behavior.

**Q1.** "Walk me through the last time you waited on a build, a CI run, or a Docker rebuild that felt way too slow. What were you doing? When was it?"
- *Listen for: date, frequency, what they were blocked from doing, who else was involved.*

**Q2.** "How do you handle build/CI/image caching today? What's set up?"
- *Listen for: existing tools (sccache, Bazel RBE, Nx Cloud, Turborepo, GitHub Actions cache, S3 hack, nothing). Tool names = real adoption.*

**Q3.** "When that setup breaks or under-performs, what happens? Walk me through the most recent time."
- *Listen for: workaround behavior. Workarounds = unmet need.*

**Q4.** "Who on your team complains about this most? Has anyone tried to fix it?"
- *Listen for: internal champions and failed projects. Failed internal projects = market signal.*

**Q5.** "If you could wave a wand and have one thing change about your build/CI/image pipeline, what would it be — and why that specifically?"
- *The "why that specifically" is mandatory; it converts a wish into a job-to-be-done.*

---

### Block B — Pain quantification (5 questions, ~8 min). Goal: dollarize the pain. JTBD Switch Forces woven in.

**Q6 (push).** "On a typical week, how many engineering-hours get burned waiting on builds/CI/image rebuilds across your team? How did you arrive at that number?"
- *Push to math: team size × wait minutes × frequency. If they refuse to estimate, the pain is not top-of-mind.*

**Q7 (pull).** "Last time your team evaluated a cache/build-acceleration tool — what triggered the evaluation, and what did you end up doing?"
- *Mom-Test gold: past evaluation behavior = real intent. "We never evaluated" = no pull yet; deprioritize.*

**Q8 (anxiety).** "If you imagined swapping your current cache setup for something new tomorrow, what would your team be nervous about?"
- *Listen for: security/SOC2, lock-in, migration cost, "who maintains it." These are objection inputs for §4 emails.*

**Q9 (habit).** "What would have to be true for you to stop using [their current tool / homegrown hack]?"
- *Habit = strongest force blocking switch per Moesta. If they can't articulate a trigger, they won't switch.*

**Q10 (cost).** "What's the rough monthly spend on CI minutes + cache infra today? And what's the unspoken cost — slow PR reviews, devs context-switching, on-call ML retrains?"
- *Aim for two numbers: line-item infra $ AND fully-loaded engineering opportunity cost.*

---

### Block C — Closing (3 minutes). Goal: extract referral + ONE willingness-to-pay signal. NO pitch.

**Q11 (referral, ALWAYS ask — non-negotiable).** "Who else on your team — or at another company — runs into this same problem and would be worth me talking to? Mind doing a quick intro?"
- *Best metric of validation: do they volunteer 2+ names unprompted? Per indie-hacker retros, referrals are the #1 source of paying customers 1–10.*

**Q12 (willingness-to-pay, framed for past behavior — Mom-Test compliant).** "Has your team ever paid for a build/CI/cache tool? What was the approval process — who signed, what was the budget line?"
- *NOT "would you pay $X for CoreLink." That's hypothetical = useless. Past purchase behavior + budget owner identity is the only real signal.*

**Q13 (commitment escalator — only if Q11 and Q12 went well).** "If I had something to show you in 3 weeks that addressed [exact pain they described in Q3], would you give me 30 minutes to try it against your real workload?"
- *A "yes + calendar invite right now" is the commitment. A "sure, ping me" is a polite no — log as such.*

**Close.** "This was incredibly useful. I'll email you a 5-bullet summary of what I heard so you can correct me if I misunderstood. Thanks for the time."

---

### Post-call hygiene (15 min, do within 60 min of hang-up)

1. Score interview 1–5 on each: problem severity, frequency, dollar pain, budget authority, switch willingness. Total /25.
2. Fill JTBD Forces grid: Push / Pull / Anxiety / Habit (one sentence each, quoted verbatim where possible).
3. Send the recap email (template in §4.4) within 60 minutes.
4. If they offered a referral in Q11 — send the intro-request email TODAY. Latency kills referrals.
5. Log to CRM (§7).

---

## §4 Cold email templates (3 variants + 1 follow-up + 1 recap)

> **Why these work.** 50–125 words (highest reply-rate range per Clay/Smartlead 2026 benchmarks). One CTA only (boosts clicks 371%). Specific first line proving research. Low-commitment ask (2x reply rate vs "book a meeting"). 3–5 follow-ups planned (most replies come from touch #2 or #3).

### §4.1 — Warm intro (highest conversion; use first for every account)

**Subject:** Quick favor — intro to [TargetName] at [TargetCo]?

```
Hey [MutualName],

Hope the [recent-thing-they-shipped] launch is going well.

Quick favor: I'm doing research on how teams running [Bazel / large
monorepos / heavy Docker pipelines / ML training] handle build caching.
I'm not selling anything — just trying to understand the problem space
before I overbuild.

[TargetName] at [TargetCo] runs [specific team / specific project I
verified via LinkedIn or job posting]. Would you be comfortable
forwarding the note below, or making an intro?

No worries if it's awkward — totally understand.

Thanks,
Gustavo

---
Forwardable blurb:

Hey [TargetName] — Gustavo here (intro'd by [MutualName]). I'm
researching how teams handling [their specific stack] deal with build
& CI caching. Not selling anything. Would you have 20 min in the next
2 weeks for a no-demo conversation about your current setup?
Calendar: [Cal.com link]
```

### §4.2 — Cold outreach (no mutual; use when warm intro unavailable)

**Subject:** [TargetCo]'s Bazel setup — quick question

```
Hi [FirstName],

Saw [TargetCo]'s [SPECIFIC SIGNAL — e.g. "job posting for a Build &
Release engineer mentioning Bazel + remote execution at scale" / "your
talk at BazelCon on monorepo CI times" / "the recent commit adding
sccache to your CI matrix"]. That's exactly the stack I'm researching.

I'm building a shared content-addressable cache (early days, not
pitching). Before I build the wrong thing, I'm doing 30 conversations
with people who actually deal with this pain daily.

Would you trade 20 minutes for whatever I learn from the other 29
conversations? No demo, no slides — just your workflow. I'll send a
written summary of everything I hear.

Calendar: [Cal.com link] — or reply with a time.

Gustavo
HuGR / CoreLink
```

**Word count:** 118. **CTA count:** 1. **Personalization signal:** mandatory bracketed line — never send without it.

### §4.3 — Follow-up sequence (3 touches over 14 days; stop after touch 4)

**Touch 2 (Day 4) — bump with new value:**

```
[FirstName] — bumping this in case it got buried.

In the last 5 conversations, the most common complaint I heard was
[ONE SPECIFIC FINDING — e.g. "CI cache invalidates on lockfile churn,
costing ~40min/day per dev"]. Curious if that resonates with [TargetCo]
or if your pain is different.

Same low-friction ask: 20 min, no demo. [Cal.com link]
```

**Touch 3 (Day 11) — short, social proof:**

```
[FirstName] — last bump.

Talked to [SimilarCompany pattern, e.g. "two AI infra teams running
PyTorch builds"] this week. Common thread: [insight]. Worth a 20-min
trade?

If now's not the time, totally fine — can I check back in Q3? [Cal.com]
```

**Touch 4 (Day 18) — breakup, opens a door:**

```
[FirstName] — closing the loop on my side. If build/CI caching ever
becomes a hair-on-fire problem at [TargetCo], my line is open.

In case useful, here's the 1-pager of what I've learned from 25
conversations so far: [Link to public notion / gist].

Best,
Gustavo
```

### §4.4 — Post-interview recap email (send within 60 min)

```
Subject: Recap from our chat — 5 bullets

[FirstName],

Huge thanks for the 20 minutes. To make sure I didn't mis-hear, here's
what I took away:

1. [Quoted pain point in their words]
2. [Current tool / workaround they use]
3. [Dollar / hour cost they estimated]
4. [Biggest anxiety about switching]
5. [What would have to be true for them to switch]

Correct me where I'm wrong — your edits are more valuable than the
original 20 minutes.

Also: you mentioned [NAME] runs into the same thing. Mind sending a
2-line intro this week?

Thanks again,
Gustavo
```

This recap email accomplishes 3 things: (1) confirms understanding (Mom-Test rule: validate what you heard), (2) leaves a paper trail you can revisit when you ship, (3) explicit referral ask while warm.

---

## §5 Twitter / X DM template

> **Why DMs work for CoreLink.** Per Pieter Levels' indie-hacker playbook, Twitter remains the highest-signal channel for solo founders in dev-tools — *if* you build a public presence first. **Do not send cold DMs from a zero-presence account.** Spend 2 weeks tweeting build/CI war stories (build-in-public adjacency), then DM. Otherwise reply rate is < 1%.

### §5.1 — Cold DM (after engaging with their content 1–2x first)

```
Hey [FirstName] — your thread on [SPECIFIC TWEET, e.g. "Docker layer
caching across CI runners"] hit close to home; been thinking about
the exact same problem from a different angle.

Doing zero-pitch research with ~30 folks running similar setups. Got
20 min in the next 2 weeks for a workflow-deep-dive? I'll trade you a
written summary of everything I've heard from the other 29.

If not, no worries — keep crushing it.
```

### §5.2 — Reply-then-DM pattern (preferred over cold DM)

1. Reply *publicly* to their tweet with one substantive observation (no link, no pitch).
2. If they engage back, DM with: *"Loved your thread. Mind if I send a longer reply over DM? Don't want to clutter the timeline."*
3. After they say yes, send §5.1 verbatim minus the opening line.

**Reply rate (per Levels-style retros + indie hacker threads):** cold DM ≈ 2–4%; reply-then-DM ≈ 15–25%.

### §5.3 — Build-in-public adjacency tweets (post 2x/week, weeks 1–8)

These are *not* pitches — they're trust-builders that prime DM responses:

- *"Day [N] of building a build cache. Today's surprise: [specific technical finding]. Anyone else hit this?"*
- *"Talked to [N] teams running Bazel this month. The #1 complaint isn't speed — it's [insight]. What's everyone else seeing?"*
- *"Repo of [specific public open-source project] would save [N] minutes per build with a content-addressable cache. Anyone from [their org] want to pair on it?"*

---

## §6 60-day find-first-10 playbook (week by week)

### Week 1 (Days 1–7): WHO + WHERE — build list of 50 target accounts

**Goal:** 50 named target companies with named contact + 1 personalization signal each.

**Tactics:**
- **Reverse-engineer competitor customer lists.** Search `site:buildbuddy.io customer` / `site:engflow.com case-study`; mirror logos. Use `gh search code` for `buildbuddy.io` / `engflow.com` / `nx.app/cloud` strings across public GitHub — every match is a CoreLink-shaped lead.
- **Job-posting mining.** LinkedIn + AshbyHQ + Greenhouse searches: `"Bazel" AND "remote cache"`, `"sccache" AND "CI"`, `"Docker layer caching" AND ("CI" OR "build")`, `"monorepo" AND ("Turborepo" OR "Nx")`. Job postings = budgeted pain.
- **GitHub-driven outreach.** Search `path:.bazelrc remote_cache`, `path:**/Dockerfile FROM` orgs with 50+ active repos, `path:.github/workflows actions/cache@v` in popular Rust/TS monorepos. Each hit = a real user of a substitute.
- **Conference adjacency (no booth).** Pull speaker list from BazelCon 2026, KubeCon CI/CD track, MLOps World — DM speakers (§5.2 pattern). Speakers > attendees for signal density.
- **Community lurking.** Bazel Slack, Buildkite Slack, Nx Discord, r/rust + r/devops; identify 20 people complaining about cache/build pain in last 90 days — they're pre-qualified.

**Deliverable:** spreadsheet/HubSpot with 50 rows: `company | contact_name | contact_email | personalization_signal | source | warm_path_y/n`.

**Metric:** 50 rows by EOD Day 7. **If you can't find 50, the ICP is wrong — pause and revisit.**

**Failure mode + recovery:** if you hit Day 5 with < 25 rows, ICP is over-narrow. Broaden one axis (e.g. add "JS monorepo with > 100 packages" alongside "Bazel C++ team").

---

### Week 2 (Days 8–14): OUTREACH — warm intros first, cold second

**Goal:** 15 booked 20-min calls on the calendar.

**Sequence:**
- **Day 8 (Mon):** identify mutuals for all 50 via LinkedIn. Send §4.1 warm-intro asks for everyone with a mutual (~15–25 of 50).
- **Day 9–10:** send §4.2 cold emails to the remaining ~25–35 accounts. Cap 15/day to keep deliverability clean. Use personal domain (e.g. `gustavo@humangr.com`); avoid bulk-mailers (Mailshake/Lemlist) for the first wave — hand-sent gets 3x reply rate per Clay benchmarks.
- **Day 11:** Twitter DM round (§5) for the 10 accounts with strongest social presence.
- **Day 12–14:** follow-up touch 2 (§4.3) for non-responders from Day 9.

**Metric:** target 15 booked calls (30% booking rate on warm, 5–10% on cold). If < 8 booked by Day 14, the email itself is the problem — A/B the subject line and the personalization line before the volume.

**Failure mode + recovery:** if < 5% reply rate on cold, the personalization signal is generic. Tighten it: a job posting + a specific commit > a job posting alone.

---

### Week 3–4 (Days 15–30): INTERVIEW — 10–15 calls, Mom-Test discipline

**Goal:** 12+ completed interviews, structured JTBD/scoring data.

**Cadence:** 3–4 calls/week max. More than that and you'll skip post-call recap, losing 50% of the signal.

**Per call:** §3 script verbatim. Recap email (§4.4) within 60 min. Referral ask in every recap.

**Mid-cycle decision gate (Day 22):** review first 6 interviews. Categorize each /25 (per §3 scoring). If < 3 are 18+ scoring, the ICP or the value prop is off — pause outreach, re-scope ICP, re-run §1 of this playbook on a narrower segment.

**Metric:** ≥ 12 completed interviews; ≥ 4 scoring 18+/25; ≥ 6 referrals received.

**Failure mode + recovery:** if everyone says "interesting but not now" — that's not a personalization fail, that's a *priority* fail. Pain isn't top-3. Pivot ICP toward teams where build cost is a board-level metric (AI infra, gaming, large monorepos with mandated CI SLAs).

---

### Week 5–7 (Days 31–45): BUILD — fix top friction, ship one specific request

> **Paul Graham principle:** "Recruit users manually" + "Collison installation." For the first 10 you do white-glove integration. You SSH in if needed. You write their `.bazelrc` for them.

**Goal:** ship the single most-requested integration/friction-fix surfaced in Week 3–4 interviews. Get 3 design-partner companies running CoreLink against real workloads.

**Tactics:**
- **Pick ONE friction:** the unanimous top complaint from the interviews. Not your favorite engineering problem — theirs.
- **White-glove integration:** offer to do the install yourself for the top 3 most-engaged interviewees. Free, your hands on their machine (or pair-programming session).
- **Design-partner agreement (lightweight):** 1-page memo. They get 6 months free + direct Slack access to you; you get a public logo + a quote when it works.
- **Public build-in-public posts (§5.3 cadence):** "Shipped X for [Anon Customer]. Cut their CI 'cargo build' from 14min to 90s." (Anonymize until they OK a name-drop.)

**Metric:** 3 design partners with CoreLink running against real workloads. At least 1 *measurable* improvement number (the CI minute delta is your launch quote).

**Failure mode + recovery:** if you can't get to 3 design partners by Day 45, you don't have PMF signal — DO NOT proceed to Week 8 close. Go back to Week 3 and do 10 more interviews on a different ICP slice.

---

### Week 8 (Days 46–60): CLOSE — convert design partners + Show HN

**Goal:** 10 paying customers. Charge $1 if needed. Money changes the relationship.

**Sequence:**

- **Day 46–50: Convert design partners to paid.** Conversation script:
  > "We've been running CoreLink against your stack for [X weeks]. You saw [specific metric]. I want to move you to a paid plan — it's $[N]/mo (or $1 for the first 3 months if budget cycle is the blocker). The point of charging isn't the revenue; it's that paid customers get prioritized feature requests and a direct line to me. Cool?"

  Per Lenny Rachitsky's B2B PMF guide, **Step 2 is "get one company to pay a meaningful amount."** $1 is fine; the act of opening procurement is the signal.

- **Day 51–53: Run the Superhuman survey on all design partners + interviewed leads who showed strong intent.** The exact question (Vohra/First Round):
  > *"How would you feel if you could no longer use CoreLink?"*
  > a) Very disappointed
  > b) Somewhat disappointed
  > c) Not disappointed
  > d) N/A — I no longer use it

  Track % "Very disappointed." 40% is the PMF threshold. < 40% means iterate, don't scale spend.

- **Day 54–58: Show HN launch.** Title: `Show HN: CoreLink — shared content-addressable cache for builds, packages, Docker, and ML`. Post Tue/Wed 9am ET. First comment from you = the design-partner result with a real number. Per `markepear.dev` HN playbook: modest language, no "fastest/best," respond to every critical comment in < 30 min with "you're right, here's what we're doing about it."

- **Day 59–60: Convert HN-driven signups.** Hand-onboard everyone who signs up post-HN within 24 hours. Reply to every signup with a personal Calendly link.

**Metric:** 10 paying customers (even at $1) by Day 60. ≥ 1 has paid > $100/mo. Sean Ellis "very disappointed" score ≥ 40% on a sample of ≥ 10 active users.

**Failure mode + recovery:**
- 10 customers but < 40% "very disappointed" → you have signups, not PMF. Do NOT scale marketing; do another interview cycle on the non-disappointed cohort to understand the gap.
- < 10 customers but > 40% "very disappointed" on the few you have → PMF signal is real; the bottleneck is distribution. Re-run Weeks 1–2 on a wider account list.
- Both fail → ICP is wrong. The honest move is a pivot conversation, not another sprint.

---

## §7 Tools recommended (solo-founder realistic)

| Layer | Tool | Cost | Why |
|-------|------|------|-----|
| **CRM-lite** | **HubSpot Free** | $0 | Per 2026 solopreneur comparisons, fastest pipeline setup, 1000-contact ceiling fine for first 10 customers, no expiration. Notion fails as CRM after ~3 months (no email integration); Airtable too DIY. Start HubSpot Free, graduate to paid only when contact count > 500. <https://editorialge.com/crm-for-solopreneurs/> |
| **Calendar booking** | Cal.com (free) | $0 | OSS, multi-link (one link for 20-min discovery, one for white-glove install). Calendly free works equally well. |
| **Email sending** | Native Gmail / Workspace + Mixmax free (5 templates) | $0–$10/mo | Do NOT use Mailshake/Lemlist for first 50 sends — hand-sent ≈ 3x reply rate. Bulk-mailer only when volume > 100/wk. |
| **Personalization signal mining** | Manual GitHub + LinkedIn + Ashby/Greenhouse search | $0 | Clay ($149/mo) is overkill at 50 accounts; revisit at 500. |
| **Interview recording** | Granola (free tier) or Otter.ai (free tier) | $0 | Auto-transcript = searchable corpus. Critical for later pattern-matching across 10+ interviews. |
| **Public list of contacts/notes** | Notion (free) | $0 | Use for the public 1-pager linked in §4.3 touch 4 ("here's what I've learned"). |
| **Survey (Superhuman PMF question)** | Google Forms or Typeform free | $0 | Single-question survey; tooling is irrelevant. |
| **Twitter scheduling** | Typefully free / native X scheduler | $0 | For the build-in-public cadence (§5.3). |

**Total monthly cost:** $0–$10. **Do not spend more until > $500 MRR.**

---

## §8 Hard rules (the "do not break these" list)

1. **Never pitch in interview.** Block A and B = zero mention of CoreLink. Period. Q13 is the only commitment-test moment and even there you don't show product.
2. **Always ask for a referral (Q11).** Every. Single. Call. Referrals are the dominant Customer 1–10 channel per indie hacker retros.
3. **Always send the recap email within 60 minutes.** Not 24 hours — 60 minutes. After that, accuracy of recall drops 50%+ and the warmth is gone.
4. **Compliments are not data.** "Cool idea" goes in the fluff column, not the validation column. Only `pulled out calendar / introduced colleague mid-call / asked when it ships` count.
5. **Anchor every pain to time + money + frequency.** "Annoying" is not pain. "14 minutes per PR × 30 PRs/day across 12 devs" is pain.
6. **No demo, no slides, no screenshare of CoreLink for the first 25 interviews.** Even if they beg.
7. **Talk ≤ 30% of the call.** Time it. Flag contaminated calls.
8. **Charge money (even $1) before declaring PMF.** Free signups lie; payment doesn't. Lenny step 2.
9. **Stop at 4 follow-ups.** Touch 5+ is harassment and torches your domain reputation.
10. **Hand-onboard the first 10.** White-glove. Collison installation. Do not automate onboarding until customer 11.
11. **40% "Very disappointed" before scaling spend.** No paid ads, no SDRs, no conference booths until Sean Ellis ≥ 40%.
12. **One ICP at a time.** Resist the urge to chase 3 segments in parallel. Pick the highest-pain slice (per Week 3 scoring), kill the others until you hit 10 paying.
13. **Build in public, not in stealth.** Per Pieter Levels — your build-process tweets are the warm-up to every cold DM.
14. **If you contradict any of rules 1–13, write down why in the audit log.** No silent exceptions.

---

## §9 Open questions / known unknowns

- **Pricing experiment.** Playbook converts to paid at "$1 if needed." Real pricing band (per build-cache market: BuildBuddy / EngFlow / Nx Cloud) is $20–$500/seat/mo or usage-based on GB-cached. Defer pricing decision until customer 5; let the 5 interviews of paying customers price the product.
- **Ideal ICP slice.** Three candidates: (a) Bazel monorepo teams (50–500 eng), (b) AI/ML infra teams with heavy Docker rebuilds, (c) JS monorepo teams pissed at Nx Cloud or Turborepo pricing. Week 1 list-building should cover all three; Week 3 decision-gate picks one.
- **EU/SOC2 anxiety (Q8).** If multiple interviews surface SOC2 as a blocker, this gates Week 5 build prioritization — may need to ship a self-hosted/BYOK story before close. Cross-ref `corelink_wave32_prod_deploy.md` and Wave 33 µK (BYOK) work.

---

## §10 References (full URL list)

1. The Mom Test — 3 rules summary: <https://www.atlantaventures.com/blog/the-3-rules-to-customer-interviews-from-the-mom-test>
2. The Mom Test — UXtweak summary: <https://blog.uxtweak.com/the-mom-test/>
3. The Mom Test — mtlynch notes: <https://mtlynch.io/book-reports/the-mom-test/>
4. Bob Moesta — JTBD + Switch Interview: <https://www.intercom.com/blog/podcasts/bob-moesta-on-jobs-to-be-done/>
5. Bob Moesta + Chris Spiek — Uncovering JTBD: <https://businessofsoftware.org/talks/bob-moesta-and-chris-spiek-uncovering-the-jobs-to-be-done/>
6. JTBD founder's guide (2026): <https://gonogo.team/jobs-to-be-done>
7. Rahul Vohra — Superhuman PMF Engine (First Round): <https://review.firstround.com/how-superhuman-built-an-engine-to-find-product-market-fit/>
8. Rahul Vohra — PMF Engine Coda playbook: <https://coda.io/@rahulvohra/superhuman-product-market-fit-engine>
9. Paul Graham — Do Things That Don't Scale: <https://www.paulgraham.com/ds.html>
10. Lenny Rachitsky — B2B PMF guide: <https://www.lennysnewsletter.com/p/finding-product-market-fit>
11. Lenny Rachitsky — How to know you've got PMF: <https://www.lennysnewsletter.com/p/how-to-know-if-youve-got-productmarket>
12. Pieter Levels — Indie Hacker playbook: <https://www.systemscowboy.com/pieter-levels-indie-hacker-strategy/>
13. Pieter Levels — King of indie hacking: <https://www.onemilliongoal.com/p/pieter-levels-the-king-of-indie-hacking>
14. Pieter Levels — IndieHackers podcast: <https://www.indiehackers.com/podcast/043-pieter-levels-of-nomad-list>
15. Mark Epear — Launch dev tool on Hacker News: <https://www.markepear.dev/blog/dev-tool-hacker-news-launch>
16. dfarrell — How to crush your HN launch: <https://dev.to/dfarrell/how-to-crush-your-hacker-news-launch-10jk>
17. Clay — B2B cold email templates 2026: <https://www.clay.com/blog/b2b-cold-email-templates>
18. Smartlead — 19 cold email templates 2026: <https://www.smartlead.ai/blog/cold-email-templates>
19. Autobound — cold email templates guide: <https://www.autobound.ai/blog/cold-email-templates-guide>
20. Superframeworks — first 10 SaaS customers (2025): <https://superframeworks.com/blog/get-first-10-customers>
21. IndieHackers — B2B SaaS first 10 customers thread: <https://www.indiehackers.com/post/b2b-saas-founders-how-did-you-find-your-first-10-customers-8310b994e4>
22. Sal — Twitter cold DM formula: <https://salwriter.medium.com/the-ultimate-formula-for-sending-cold-dms-on-twitter-x-that-get-replies-no-spam-just-results-89fa4e188bda>
23. Editorialge — CRM for solopreneurs 2026: <https://editorialge.com/crm-for-solopreneurs/>
24. Coherence — Solo founder CRM comparison 2026: <https://getcoherence.io/blog/solo-founder-crm-comparison-guide-2026>
25. BuildBuddy: <https://www.buildbuddy.io/>
26. EngFlow: <https://www.engflow.com/product/remoteExecution>

---

**END OF PLAYBOOK.**
