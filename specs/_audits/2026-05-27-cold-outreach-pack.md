---
id: "AUDIT-2026-05-27-COLD-OUTREACH-PACK"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "cold-outreach", "email-templates", "p1-persona", "launch-content", "go-to-market"]
---

# Cold Outreach Pack — P1 (Platform Lead, Series B/C)

> **Purpose.** Ready-to-paste cold outreach assets tuned to **Persona P1** ("Priya, Platform Lead at compliance-aware Series B/C SaaS, 50-250 eng, Bazel or sccache shop"). All variants are sized to the 50-125-word Clay/Smartlead 2026 benchmark range. Each variant has subject-line A/B options. Replace `[bracketed]` tokens per-send; never send a template with a bracket still in it.
>
> **Source.** Conversion of `specs/_audits/2026-05-27-customer-development-playbook.md` §4 (commit `fd1fc6d1` in `main`), tuned with P1 persona signals from `specs/_audits/2026-05-27-icp-customer-discovery.md` §3.1.
>
> **Hard rules** (do not break):
> - One CTA per email. One. Two CTAs cuts click rate by ~50%.
> - Personalization signal in the first sentence — must be specific (a commit SHA, a job posting, a conference talk). "Saw your company is doing well" is not personalization.
> - Hand-sent from `gustavo@humangr.com`. NOT Mailshake / Lemlist for the first 50 sends. Hand-sent reply rate is ~3x mass-mailer.
> - Volume cap: 15 cold sends per day. Above that, deliverability degrades within a week.
> - Stop at 4 touches total (Day 0, 3, 7, 14). Touch 5+ is harassment and torches sender reputation.
> - Never send to `info@` / `contact@` / `team@`. Only to a named human inbox.
> - Reply-rate floor: 5%. Below 3% over a 20-send batch = the personalization signal is generic; fix before sending more.

---

## §1 — Three cold email variants (no warm intro available)

Three variants exist so the same target company can be hit on three different personalization angles (one per touch), or so you can A/B which angle resonates for the P1 segment.

### §1.1 — Variant A: the GitHub-signal angle (highest conversion for P1)

**Subject line A/B:**
- A: `[Company]'s .bazelrc — quick question`
- B: `your remote_cache setup at [Company]`

**Body (118 words):**

```
Hi [FirstName],

Pulled up [Company]'s `[repo]` repo this morning — saw the
`grpcs://[their-cache-host]` remote_cache line in `.bazelrc` and
the `actions/cache@v4` step in `.github/workflows/[workflow].yml`.
That stack (Bazel + GHA cache as fallback) is exactly the one I'm
researching right now.

I'm a solo founder building a shared content-addressable cache —
early days, not pitching. Before I build the wrong thing, I'm doing
30 conversations with platform leads who actually deal with cache
pain daily.

Would you trade 20 minutes for whatever I've learned from the other
29? No demo, no slides — just your workflow. I'll send you a
written summary of every interview pattern I've seen.

Calendar: [Cal.com link] — or reply with a time.

Gustavo
HuGR / CoreLink
```

---

### §1.2 — Variant B: the job-posting angle (when GitHub signal is private)

**Subject line A/B:**
- A: `[Company]'s Platform Engineer JD — quick q`
- B: `your Bazel + remote-execution role`

**Body (112 words):**

```
Hi [FirstName],

Saw [Company]'s job posting for a [exact title — e.g. "Senior Platform
Engineer, Build Infrastructure"]. The line about "[exact phrase from
JD — e.g. 'scaling our Bazel remote cache across 8 regions'"] caught
my eye — that's exactly the problem I'm researching right now.

I'm a solo founder building a shared content-addressable cache for
the Bazel / sccache / Docker layer. Not pitching. Before I overbuild,
I'm trading 20-min workflow conversations with platform leads in the
trenches — and giving back a written summary of what I've heard from
the other ~30.

Worth a chat in the next 2 weeks?

Calendar: [Cal.com link]

Gustavo
```

---

### §1.3 — Variant C: the conference-talk angle (highest personalization signal)

**Subject line A/B:**
- A: `your BazelCon talk on monorepo CI`
- B: `that point you made about cache invalidation`

**Body (114 words):**

```
Hi [FirstName],

Re-watched your [BazelCon 2026 / DPE Summit / KubeCon] talk on
"[exact talk title]" last week. The slide where you said "[exact
quote — e.g. 'we lost 40 min/day to lockfile churn invalidating the
whole graph']" — that has been the #1 complaint in the 12 interviews
I've done so far.

Quick context: I'm a solo founder building a shared content-addressable
cache. Not pitching anything — researching the problem before I
overbuild.

Would you trade 20 minutes for whatever I've learned from the other
~30 platform leads I'm talking to? No demo, no slides. I'll send back
a written summary of the patterns.

Calendar: [Cal.com link]

Gustavo
```

---

## §2 — Warm-intro forwardable blurbs

Use these when the target has a mutual connection on LinkedIn / Twitter. The format is two emails: (a) the ask to the mutual, (b) the forwardable blurb the mutual pastes when forwarding.

### §2.1 — Warm-intro variant 1: the "quick favor" ask (preferred)

**Subject:** `Quick favor — intro to [TargetName] at [TargetCo]?`

**Body (105 words):**

```
Hey [MutualName],

Hope the [recent-thing-they-shipped — keep specific] launch went well.

Quick favor: I'm researching how teams running [Bazel monorepos / heavy
Docker pipelines / sccache + S3 at scale] handle build caching. I'm
not selling — trying to understand the problem space before I
overbuild.

[TargetName] at [TargetCo] runs [specific team / specific public signal
verified via LinkedIn or job posting]. Would you be comfortable
forwarding the blurb below, or making an intro?

No worries if it's awkward — totally understand.

Thanks,
Gustavo

---
Forwardable blurb below.
```

### §2.2 — The forwardable blurb the mutual pastes (paste verbatim into the same email or in a separate reply)

```
Hey [TargetName] — Gustavo here (intro'd by [MutualName]). I'm
researching how teams handling [their specific stack — e.g. "Bazel
monorepos with strict SOC 2 controls"] deal with build and CI
caching. Solo founder, not selling anything.

Would you have 20 min in the next 2 weeks for a no-demo
conversation about your current setup? I'm trading every interview
for a written summary of patterns I've seen across the other ~30
conversations.

Calendar: [Cal.com link]

— Gustavo
HuGR / CoreLink
```

---

## §3 — 3-touch follow-up sequence (Day 0, 3, 7) + breakup (Day 14)

The pacing differs from the source playbook's 14-day window in two ways: (a) compresses to a 7-day decision interval per modern B2B benchmarks (most reply-arrivals fall in days 1-3, 4-7, then a long tail), (b) reserves Day 14 for the breakup. **Hard stop at touch 4. Touch 5 is harassment.**

### §3.1 — Day 0: initial send

Use one of the three variants in §1 (or the warm-intro path in §2 when available). Default to **Variant A (GitHub signal)** for any account with public Bazel-shop signals; fall back to Variant B when signal is private.

### §3.2 — Day 3: bump with new value

**Subject (keep the same thread — do NOT change subject line on reply):** `Re: [original subject]`

**Body (78 words):**

```
[FirstName] — bumping in case it got buried.

In the last 5 conversations with platform leads, the most common pattern
I heard: cache hit rate looks great on the dashboard, but lockfile or
toolchain churn quietly invalidates ~40 min/day per dev. Curious if
that shape resonates with [Company] or if your pain looks different.

Same low-friction ask: 20 min, no demo, written summary in return.

[Cal.com link]

Gustavo
```

### §3.3 — Day 7: short, peer-pattern social proof

**Subject:** `Re: [original subject]`

**Body (62 words):**

```
[FirstName] — last bump.

This week talked to two AI-infra teams running PyTorch + Bazel. Common
thread: BYOK is non-negotiable for SOC 2, but every off-the-shelf
remote cache punts on it. Worth a 20-min trade to compare notes on
your stack?

If now's not the time, totally fine — can I check back in Q3?

[Cal.com]

Gustavo
```

### §3.4 — Day 14: breakup email (opens a door, does not close it)

**Subject:** `closing the loop — [Company]`

**Body (74 words):**

```
[FirstName] — closing the loop on my end.

If build or CI caching ever becomes a hair-on-fire problem at
[Company], my line is open. No CRM nurture sequence, no SDR will
chase you — just me.

In case useful, here's the 1-pager of what I've learned from 25
platform-lead conversations so far: [Notion link / gist URL]

Best,
Gustavo
HuGR / CoreLink
```

**Stop here.** Do not send touch 5. If the breakup gets a reply, that's the signal — re-engage with the recap quality of a fresh first-touch, not a sales pitch.

---

## §4 — Post-interview thank-you / recap email (send within 60 min of hang-up)

Send this *every* time, even for calls that scored low. It accomplishes three things: confirms understanding (Mom-Test rule), leaves a paper trail you can revisit when you ship, and creates a warm channel for the explicit referral ask.

**Subject:** `Recap from our chat — 5 bullets`

**Body (paste, then fill the 5 bullets verbatim from your call notes):**

```
[FirstName],

Huge thanks for the 20 minutes. To make sure I didn't mis-hear, here's
what I took away:

1. [Quoted pain point in their words — verbatim if possible]
2. [Current tool / workaround they use]
3. [Dollar or hour cost they estimated]
4. [Biggest anxiety about switching]
5. [What would have to be true for them to switch]

Correct me where I'm wrong — your edits are more valuable than the
original 20 minutes.

Also: you mentioned [referral name] runs into the same thing. Mind
sending a 2-line intro this week? Below is the forwardable blurb in
case it makes life easier.

Thanks again,
Gustavo

---
Forwardable blurb for [referral name]:

Hey [referral first name] — Gustavo here, [FirstName] suggested I
reach out. I'm researching how teams with [their stack shape] handle
build caching. Not selling. Would you have 20 min in the next 2 weeks
for a no-demo conversation? Calendar: [Cal.com link]
```

---

## §5 — Per-touch reply-rate targets (calibrate before scaling volume)

| Touch | Day | Reply-rate target | Below = fix what |
|---|---|---|---|
| 1 (cold) | 0 | >= 5% | personalization signal is generic; tighten to a commit/JD/talk quote |
| 2 (value-bump) | 3 | >= 3% incremental | the "value" sentence isn't resonant; rotate insight per cycle |
| 3 (peer pattern) | 7 | >= 2% incremental | peer pattern isn't credible; use a more concrete number/name |
| 4 (breakup) | 14 | 4-8% (counterintuitively high) | n/a — this is the floor; under 4% means cumulative thread quality was low |
| Warm-intro | 0 | >= 30% | the mutual is too distant; pick a closer connection |
| Recap email | post-call | >= 60% reply or open | the 5 bullets were vague; re-quote verbatim from notes |

Cumulative cold-conversion target across 4 touches: **8-12% booked-call rate per 20-account batch.** Below 6% on a clean ICP list = ICP is wrong; re-scope before more outbound.

---

## §6 — Pre-send checklist (run before EVERY send)

- [ ] Subject line < 50 chars, no marketing speak ("revolutionize", "10x", "AI-powered", "game-changer" — never).
- [ ] Personalization signal in first sentence is **specific** (a SHA, a JD phrase, a talk quote, a commit). "Saw your company" is not personalization.
- [ ] Exactly one CTA. Calendar link OR reply with a time — not both as separate asks.
- [ ] Word count between 60 and 125. Outside this band = lower reply rate per Clay 2026 data.
- [ ] No HTML signature, no logo, no `Sent from my iPhone`. Plain text only.
- [ ] From-address is `gustavo@humangr.com`. Not a no-reply, not a brand alias.
- [ ] No `info@` / `contact@` / `team@` in TO field.
- [ ] All `[brackets]` are filled. (Spot-check: search the draft for `[` before sending.)
- [ ] Cal.com link tested in incognito window today. (Stale links happen.)
- [ ] Send rate today is at or under 15 emails. Above 15 = deliverability decay within a week.

---

## §7 — Anti-patterns (delete from drafts on sight)

- "Hope this finds you well" → delete.
- "I'd love to schedule a quick demo" → no — you want a conversation, not a demo.
- "Just following up" / "circling back" → delete; lead with the value instead.
- "Wanted to bump this to the top of your inbox" → say nothing about the meta; just send the value.
- HTML signature with logos → routes to spam filters.
- Sending to `info@` / `contact@` → never.
- Mass-merge with no per-recipient first sentence → never.
- Two CTAs ("book a call OR reply OR check out our site") → pick one.
- Superlatives ("fastest", "best", "industry-leading") → delete; engineers filter you out.
- Sending without a Cal.com link → friction; always include it.

---

**END OF COLD OUTREACH PACK.** For the persona profile that informs every signal-choice above, see `specs/_audits/2026-05-27-icp-customer-discovery.md` §3.1. For the source frameworks, conversion benchmarks, and the broader 60-day plan that this pack feeds into, see `specs/_audits/2026-05-27-customer-development-playbook.md` (commit `fd1fc6d1`).
