---
id: "AUDIT-2026-05-27-TWITTER-STARTER-PACK"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "twitter", "x", "build-in-public", "launch-content", "dm-templates"]
---

# Twitter / X Starter Pack — @corelinkdev account launch

> **Purpose.** Ready-to-paste assets for launching the CoreLink presence on X. Bio, first-week tweets, reply-then-DM patterns, pinned tweet, and a 20-account follow-and-engage list. Voice: technical, honest, build-in-public adjacent. No marketing fluff.
>
> **Source.** Conversion of `specs/_audits/2026-05-27-customer-development-playbook.md` §5 + Pieter Levels' indie-hacker DM playbook (commit `fd1fc6d1` in `main`). Account list cross-referenced with `specs/_audits/2026-05-27-distribution-channels.md` §1.1.
>
> **Hard rules** (do not break):
> - Do NOT cold-DM from a zero-presence account. Spend 2+ weeks tweeting build/CI war stories before any DM. Otherwise reply rate is < 1%.
> - Reply-rate target: cold DM 2-4% (bad), reply-then-DM 15-25% (good). Always prefer the reply-then-DM pattern.
> - The word "CoreLink" appears in roughly 1 in 5 of your tweets, not in every one. The other 4/5 are pure technical signal.
> - No emojis. Engineers filter accounts that lead with emojis.
> - Schedule with Typefully free or native X scheduler. Do not pay for tooling pre-PMF.

---

## §1 — Handle selection

**Primary target handle:** `@corelinkdev` (160-char bio below).

**Fallbacks in order** (in case `@corelinkdev` is taken — verify before booking):
1. `@corelink_dev`
2. `@usecorelink`
3. `@corelinkhq`
4. `@trycorelink`

**Do NOT use:** `@corelink` (taken by an unrelated brand per most platforms' history of squatters), `@corelinkio` (domain mismatch with `corelink.humangr.com`), anything with numbers or year suffixes.

**Display name:** `CoreLink` (no emoji, no "dev tools", no tagline in the name).

**Profile photo:** Repo's `favicon.svg` or a 400x400 PNG of the CoreLink logo on a flat background. Not a selfie — the operator account is Gustavo's personal handle; this is the product account.

**Header image:** Plain background, single line of text: `shared content-addressable cache — bazel, sccache, docker, ml.` No screenshots, no logos.

---

## §2 — Bio (160 chars max — engineered to count)

**Primary bio (142 chars including spaces — under the 160 limit):**

```
shared content-addressable cache for bazel, sccache, docker, and ml. byok-encrypted. built by a solo founder at @humangr. corelink.humangr.com
```

**Reasoning per token:**
- `shared content-addressable cache` — exact 3-word product noun, indexed by every dev searching "build cache" / "remote cache".
- `for bazel, sccache, docker, and ml` — the 4 substrates; signals breadth without claiming "everything".
- `byok-encrypted` — the P1 (Series B/C, SOC 2) anxiety-killer in one hyphenated token.
- `built by a solo founder at @humangr` — honesty signal; reduces bus-factor objections by acknowledging them.
- `corelink.humangr.com` — link in bio is the same URL as the link field, so the bio works even when X strips the link.

**Fallback bio (alternative, 138 chars):**

```
content-addressable cache for bazel/sccache/docker/ml builds. byok encryption. solo-founder build-in-public. github: HumanGuardrail/corelink
```

Use the fallback if `@humangr` mention can't be linked (account doesn't exist yet on X).

**Location field:** `Brazil` or `BR`. Honest, geographic, no "Globe" or "Earth" nonsense.

**Link field:** `https://corelink.humangr.com`

---

## §3 — Pinned tweet draft

The pinned tweet must do one job: convert a curious profile-visitor into a click or a follow. Not a sales pitch.

```
i'm a solo founder building a shared content-addressable cache
for bazel, sccache, docker, and ml builds.

byok-encrypted. self-hostable on cloudflare workers. apache-2.0
client sdk.

writing here about what i'm learning from ~30 platform-lead
interviews + what's actually shipping.

repo: github.com/HumanGuardrail/corelink
```

**Why this works:**
- Line 1 = clear "what is this account" sentence.
- Lines 2-3 = three technical credibility tokens (BYOK, Cloudflare Workers, Apache-2.0). Engineers scan for these.
- Line 4 = the build-in-public promise — sets reader expectation for the timeline content.
- Line 5 = one CTA (the repo). The bio link handles the landing-page CTA; the pinned tweet sends to the deeper-signal artifact.

---

## §4 — Five first-week tweet drafts

These are build-in-public adjacent — technical observations and honest reports, not announcements. Posted on alternating days during week 1; expect 5-50 engagements each on a zero-following account ramping via the §6 engagement list.

### §4.1 — Day 1: opening shot (specific number, specific decision)

```
day 1 of building in public.

the question that ate this week: should the cache key for a
docker layer include the build platform flag, or treat
linux/amd64 + linux/arm64 as siblings of one logical layer?

i went with siblings. saves ~38% storage on multi-arch repos
in the 5 i benchmarked. costs a tiny bit of correctness in the
edge case where someone pins to a digest mid-build.

curious what other people picked.
```

### §4.2 — Day 2: counter-intuitive interview finding

```
talked to 6 platform leads at series b/c saas companies this
week about their build cache setup.

the most common complaint isn't slow ci.

it's that the cache hit rate dashboard looks great (89%, 92%)
while devs are still waiting 18 minutes per pr — because
lockfile churn invalidates the whole graph downstream and
the hit-rate metric averages it away.

anyone else seeing this pattern?
```

### §4.3 — Day 3: a tradeoff i'm losing sleep over

```
the tradeoff that's keeping me up:

content-addressable cache wants to be a single global namespace
(maximum dedup, max hit rate).

soc 2 / gdpr / vpc-isolated buyers want tenant-scoped namespaces
(no shared blob plane across orgs).

you can have ~80% of both with per-tenant encryption + a shared
deduplicating blob layer (blob is opaque ciphertext, key derives
from tenant material). but every "we have byok" sales conversation
asks the same follow-up: "what does your tenant boundary actually
look like at the storage layer?"

curious how teams running self-hosted bazel-remote handle this
today — do you trust the s3 bucket boundary or do you encrypt
client-side?
```

### §4.4 — Day 5: open-source decision in public

```
licensed the corelink client sdk apache-2.0 this morning.

the server stays source-available (BSL 1.1, converts to apache
after 4 years) — same model as cockroachdb and sentry pre-2019.

reasoning: the client sdk is what every potential user touches
first; making it permissive maximizes adoption surface. the
server is where the operational complexity (and the value) lives,
so BSL protects against amazon-style fork-and-host without
blocking legitimate self-hosting by a paying customer.

if you've shipped under BSL — what would you do differently?
```

### §4.5 — Day 7: weekly retro (sets the cadence reader can expect)

```
week 1 retro of building corelink in public:

- 6 platform-lead interviews booked, 4 completed
- biggest surprise: byok is a top-3 ask, not a "nice to have"
- shipped: cache-key derivation for multi-arch docker (siblings, not separate keys)
- broken: the cli's progress bar lies when uploads are batched
- next week: ship the bazel remote-cache grpc adapter, run 4 more interviews

writing one of these every sunday. follow if that's interesting.
```

---

## §5 — Reply-then-DM patterns (15-25% conversion vs 2-4% cold DM)

**Principle.** A cold DM from a zero-presence account converts at ~1%. A public substantive reply to their tweet, *then* a DM after they engage back, converts at 15-25%. Always prefer pattern 2 or 3 below over pattern 1.

### §5.1 — Pattern A: the "comparison" reply-then-DM

**Step 1 — public reply (no link, no pitch):**

> "the cache-invalidation-by-lockfile-churn thing you mentioned in tweet 3 is the #1 pattern i've seen across 6 platform-lead interviews this month. the workaround i keep hearing is path-scoped invalidation — splitting the dep graph at the package-manager boundary. has [target's company] tried that?"

**Step 2 — wait for them to engage back (reply, like, follow). DO NOT DM before this.**

**Step 3 — DM:**

```
hey [name] — appreciated the back-and-forth on the lockfile thread.

if useful, i've been logging the patterns across ~30 platform-lead
interviews. happy to send the raw notes (anonymized) — or trade them
for 20 min on your team's cache setup.

i'm a solo founder building in the space, but this isn't a pitch —
i'm trying to validate the problem shape before i overbuild.
```

### §5.2 — Pattern B: the "i hit the same problem" reply-then-DM

**Step 1 — public reply:**

> "hit this exact thing last week trying to make the bazel remote_cache grpc client respect a 10-minute upload timeout on cold-start. ended up patching the timeout in the request-level grpc context instead of the client-builder. happy to share the diff if useful."

**Step 2 — wait for engagement. If they ask for the diff, send a gist link in the reply.**

**Step 3 — DM (only after they've replied at least once):**

```
glad the diff helped — that timeout issue was a 4-hour rabbit hole for
me too.

while i have you: i'm researching the cache-side of this stack right
now (solo founder, building a shared content-addressable cache). would
you have 20 min in the next 2 weeks for a workflow walkthrough on
your end? no demo, no slides. i'll send back a written summary of
patterns from the other ~30 conversations.
```

### §5.3 — Pattern C: the "you and i are the only people thinking about this" reply-then-DM

**Step 1 — public reply on a niche-technical thread (BYOK, audit chain, tenant-scoped storage):**

> "the byok-with-shared-blob-layer tradeoff you described is the one i've been chewing on for two weeks. the only path i've found to ~80% of both is per-tenant encryption keys + a deduplicating ciphertext blob layer. but it requires client-side encryption before the upload hits the dedup hash, which forces you to give up cross-tenant dedup entirely. did [their company] find a better way?"

**Step 2 — wait for engagement. Replies to deep-niche threads convert at 30%+ when the reply is substantive.**

**Step 3 — DM:**

```
your thread is the most coherent take i've seen on the byok-vs-dedup
tradeoff. i'm working on the same problem from the cache-vendor side
(solo founder, content-addressable cache product) and would love 20
min of your time on the architecture trade-space.

zero pitch — just two engineers comparing notes. i'll trade you
whatever i've learned from talking to ~30 platform leads about how
they're handling it today.
```

---

## §6 — 20 accounts to follow + engage (cross-referenced from distribution-channels audit)

**Source for handles:** `specs/_audits/2026-05-27-distribution-channels.md` §1.1 (10 named accounts) plus 10 additions from the customer-discovery channels and `specs/_audits/2026-05-27-icp-customer-discovery.md` §3.1 P1 communities (Platform Engineering Podcast, DPE Summit, bazel-discuss). All handles to be re-verified before first engagement (X handle namespace shifts).

### §6.1 — Tier 1: high-signal individual voices (engage first, ~once per week each)

| # | Handle | URL | Why follow / how to engage |
|---|---|---|---|
| 1 | `@bazelbuild` | https://x.com/bazelbuild | Official Bazel account. Retweets community wins. Reply substantively on remote-cache / RBE posts. |
| 2 | `@aspect_build` | https://x.com/aspect_build | Bazel consultancy; blog posts hit r/bazel and HN. Engage on their architecture deep-dives. |
| 3 | `@nxdevtools` | https://x.com/nxdevtools | Nx team — competitor-adjacent but engaging. Reply on monorepo cost / Turborepo comparison posts. |
| 4 | `@turborepo` | https://x.com/turborepo | Vercel-owned, ~30k followers; broadest monorepo reach. Engage on remote-cache pricing posts. |
| 5 | `@depotdev` | https://x.com/depotdev | Closest direct analogue (remote Docker build cache). Engage on architecture, not pricing. |
| 6 | `@earthly_dev` | https://x.com/earthly_dev | Build-tool community. Engage on caching + reproducibility threads. |
| 7 | `@kelseyhightower` | https://x.com/kelseyhightower | Infra OG, ~190k followers. One retweet = thousands of impressions. Engage on platform-engineering threads. |
| 8 | `@copyconstruct` (Cindy Sridharan) | https://x.com/copyconstruct | Distributed-systems voice, ~80k followers. Engage on storage / consistency / observability threads. |
| 9 | `@mitchellh` (Mitchell Hashimoto) | https://x.com/mitchellh | DX-tool builder, ~110k followers. Engage on CLI / install-experience posts. |
| 10 | `@b0rk` (Julia Evans) | https://x.com/b0rk | Explains infra concepts, ~200k followers. If she writes about caches, the lift is enormous. Reply with technical depth, not flattery. |

### §6.2 — Tier 2: ICP-proximal accounts (follow, engage when on-topic, monthly cadence)

| # | Handle | URL | Why follow |
|---|---|---|---|
| 11 | `@buildbuddy_io` | https://x.com/buildbuddy_io | Direct competitor — read their roadmap and customer wins for positioning intel. |
| 12 | `@engflow` | https://x.com/engflow | Enterprise competitor — read their compliance posts; that's the P1 anxiety surface. |
| 13 | `@platformengineering` (or `@PlatformEngHQ`) | https://x.com/platformengineering | Platform Engineering community account; the exact P1 audience. |
| 14 | `@getdx` | https://x.com/getdx | DX Engineering Enablement Podcast hosts; P1 listens to their show. |
| 15 | `@dpe_summit` | https://x.com/dpe_summit | DPE Summit conference handle; P1 attendees congregate here annually. |
| 16 | `@cloudflaredev` | https://x.com/cloudflaredev | CoreLink ships on Workers + R2; engaging here builds credibility with the underlying-platform crowd. |
| 17 | `@theprimeagen` | https://x.com/theprimeagen | Influence vector for the indie / P3 segment that graduates to P1. One signal-boost moves significant traffic. |
| 18 | `@fireship_dev` | https://x.com/fireship_dev | Same logic as ThePrimeagen — wide reach across the JS / monorepo segment. |
| 19 | `@matt_dz` (Matt Dziuban / similar build-tool indie) | https://x.com/matt_dz | Active indie-build-tool voice; honest about tradeoffs. *Verify handle before engaging.* |
| 20 | `@dhh` (David Heinemeier Hansson) | https://x.com/dhh | Frequent voice on build-cost + monorepo critique; an engaged reply on a CI-cost thread reaches platform engineers at scale. |

**Engagement rule per account:** at most 1 substantive reply per account per week. More than that reads as inauthentic. Spread the 20 accounts across the week (≈3 / day) so the cadence is sustainable.

### §6.3 — Verification step (run before first engagement)

For each of the 20 handles above:

- [ ] Open the X handle, confirm it's the actual person/org (not a parody / impersonator).
- [ ] Confirm the account has posted in the last 90 days (dead accounts waste engagement budget).
- [ ] Note their typical post topic to filter for on-topic engagement opportunities (build/CI/cache/monorepo/platform-engineering).

Replace any dead/wrong handles before booking the engagement cadence.

---

## §7 — Posting cadence (weeks 1-8)

| Day | Cadence | Type |
|---|---|---|
| Mon | 1 tweet | technical observation / build-in-public update |
| Tue | 0-1 tweets + 3 engagement replies | engagement-focused day; quote-tweet or reply to a §6 Tier 1 account if on-topic |
| Wed | 1 tweet | longer-form (thread, 3-5 tweets) on a real architecture decision |
| Thu | 0-1 tweets + 3 engagement replies | engagement-focused day |
| Fri | 0 tweets | quiet day — no one reads tech-twitter Friday afternoon |
| Sat | 0 tweets | quiet |
| Sun | 1 tweet | weekly retro tweet (see §4.5 template) |

**Volume target:** 4 originals + 6 substantive replies per week. Below that you stay invisible. Above that you starve the interview-running and code-writing work — which is the actual job.

**Stop scheduling and pivot to manual posts if any of these happen:**
- Followers grow > 20% week-over-week (suggests you're chasing reach over signal — recalibrate).
- Reply ratio (your replies vs your originals) drops below 1.5x (means you're broadcasting, not conversing).
- Any single tweet pulls > 100k impressions (engage that thread manually for 48h — that's where the conversions live).

---

## §8 — DO NOT — Twitter anti-patterns for engineers

- Emojis in the bio, name, or tweets. Engineers filter them out.
- "🚀 launching today!" / "excited to announce" / "the future of X" — never.
- Threads where the hook is "1/" but the content is marketing fluff. The hook earns the read; weak hooks burn reputation.
- Following 5,000 people in a week to inflate the ratio. Detected by X's spam filters; account gets shadow-throttled.
- Buying followers / engagement. Detectable, public-shaming risk, account ban risk.
- Cold DMs before posting publicly for 2 weeks. Conversion < 1%.
- Replying with "great post!" / "this is amazing" — never. Either reply with a substantive technical observation or stay quiet.
- Quoting someone's tweet to subtweet them ("interesting take, but here's why it's wrong..."). Reply directly in the thread instead — that's where the engagement is, and it's not cowardly.
- Posting the same tweet to LinkedIn at the same time. The platforms have opposite voice expectations; cross-posting reads as low effort.
- Pinning a tweet that isn't a clear "what is this account" sentence. Visitors decide to follow in 4 seconds; the pin has to do that work.

---

**END OF TWITTER STARTER PACK.** For the underlying DM strategy + build-in-public adjacency principle, see `specs/_audits/2026-05-27-customer-development-playbook.md` §5. For the broader distribution channel ratings + the rest of the 30-day launch calendar, see `specs/_audits/2026-05-27-distribution-channels.md` (commit `fd1fc6d1`).
