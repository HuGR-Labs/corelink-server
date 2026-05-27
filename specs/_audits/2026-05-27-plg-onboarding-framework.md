---
id: "AUDIT-2026-05-27-PLG-ONBOARDING-FRAMEWORK"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "plg", "onboarding", "growth", "ttfv", "activation", "strategy"]
related:
  - "AUDIT-2026-05-27-LAUNCH-READINESS-CHECK"
---

# PLG + onboarding framework for CoreLink

> **Scope.** Research-backed product-led-growth (PLG) framework, applied concretely to CoreLink's signup → activation → paid funnel. Defines a single canonical activation event, a measurable TTFV target, the step-by-step onboarding flow that hits that target, and the instrumentation plan to know whether it actually works. Solo-founder feasible — no Customer-Success team in the loop.

> **Method.** (a) Synthesize external benchmarks from PLG canon (Lenny Rachitsky, Wes Bush, OpenView/ChartMogul, Mixpanel, Productled.org, Pulseahead, ProdPad, Supabase). (b) Cross-reference against the current CoreLink artefacts surfaced by `2026-05-27-launch-readiness-check.md`: docs landing (`apps/docs/docs/index.mdx`), Clerk sign-up (`apps/admin-ui/src/app/sign-up/[[...sign-up]]/page.tsx`), onboarding wizard (`apps/admin-ui/src/app/[locale]/onboarding/` — six steps), pilot intake (`apps/docs/src/pages/pilot/apply.tsx`), six-page tutorial (`apps/docs/docs/tutorial/{01..06}.mdx`). (c) Sanity-check that no analytics SDK is wired today (`grep -r posthog|amplitude|mixpanel apps/admin-ui/src/` returns only the cookie-consent category labels — confirmed no client present).

> **Verdict up-front.** CoreLink's current funnel cannot hit a competitive TTFV. The wizard has **six gates** before a user can write their first cache entry (tenant → region-plan → DPA → PAT → billing → done, *then* docs tutorial 01–06). For a Bazel adopter, realistic time-to-first-cache-hit today is **45–90 minutes** assuming everything works (it doesn't — §2 in launch-readiness lists the 500s + Stripe scaffold). Target proposed below: **≤ 10 minutes** to first cache hit, **≤ 5 minutes** to first authenticated `corelink ping`.

---

## §1 PLG fundamentals + dev-tool benchmarks

### §1.1 TTFV — what "good" looks like for dev tools

Published benchmarks (cited inline):

| Source | Metric | Number |
|---|---|---|
| Count.co / Mixpanel composite | Top-quartile SaaS TTFV | **< 5 min** from signup to first value event |
| ProductGrowth.in 2026 onboarding benchmarks | Top-quartile activation rate / D7 retention | 40 %+ activation, 30 %+ D7 |
| PayProGlobal calc | Industry-average SaaS TTV | ~1 day 12 h 23 min (median 1 day 1 h 54 min) — *most products are slow* |
| Prospeo / Usertourly | "Above 2 days" trigger | Prioritize friction audit |
| ProductGrowth.in | Interactive onboarding effect | -30 % TTFV on average vs static |

Dev-tool category specifics (folklore + DevRel write-ups, treat as directional):

- **Plausible Analytics** — single `<script>` tag, first event visible in dashboard within **~1 min** of paste.
- **Sentry** — `npx @sentry/wizard` flow targets **< 5 min** to first captured error; their public onboarding heuristic is "developer sees their own data within one coffee".
- **Vercel** — `git push` to deployed URL in **< 90 s** for a Next.js starter.
- **Supabase** — Craft Ventures case study: aligned the entire funnel around **"first database initialized"** as the single activation event; that single keystone pulled Auth, Storage, Realtime adoption downstream ([Craft Ventures](https://medium.com/craft-ventures/inside-supabases-breakout-growth-lessons-scaling-to-4-5m-devs-powering-ai-vibe-coding-dc574acfafaa)).
- **BuildBuddy (closest analogue)** — Quickstart is "paste `--remote_cache=…` + `--remote_headers=x-buildbuddy-api-key=…` into your `.bazelrc`, run `bazel build`". First cache hit on the **second** build of the same target. Setup is two lines of config + one env var. Time depends entirely on user's build duration; auth + config **< 5 min**.

**Adopted CoreLink target:** median **TTFV ≤ 10 min** (signup → first cache hit visible in user's own CI or local Bazel), p75 ≤ 20 min. Anything above the p75 threshold triggers a friction audit (per Prospeo guidance).

### §1.2 Activation events — definition and good shape

Activation = the in-product event after which downstream retention curves bend up. Properties of a good activation event (synthesis from Pocus, OpenView, Hightouch PQL guides):

1. **Unambiguous** — single SQL/DB predicate, no judgement.
2. **Implies value received** — user got something out, not just put effort in.
3. **Fast** — reachable in one session, ideally < 10 min.
4. **Hard to fake** — discriminates real users from tyre-kickers and scripts.
5. **Predicts retention** — historically correlates with D7 / D28 return.

Pocus's heuristic for dev tools: "first successful API call in a production environment" — *not* a sandbox/example call ([Pocus](https://www.pocus.com/blog/the-definitive-pql-guide-part-1)).

### §1.3 Friction-reduction patterns

From OpenView, Lenny Rachitsky's "Freemium vs. Trial vs. Reverse-Trial" thread, and ChartMogul's 2026 study (cited via Pulseahead) on credit-card-required vs opt-in trials:

| Choice | Convention | When to use | CoreLink fit |
|---|---|---|---|
| Credit card upfront | Higher activation→paid (31.4 % CC-required vs 8.9 % opt-in per ChartMogul 2026) but **collapses signup volume by 2-5x** | When LTV is high, ICP is enterprise, support cost > acquisition cost | **Skip for launch.** Solo founder needs signup volume + word-of-mouth, not high friction. |
| SSO / OAuth (GitHub) | Removes "password" cognitive load for devs; +15–25 % signup completion in dev-tool segments (folklore) | Always, for B2B dev tools | **Adopt.** Clerk already supports GitHub OAuth — wire it as the first button. |
| CLI-first vs GUI-first | Dev-infra products (Bazel, Buck2, Cargo) live in the terminal; forcing them through a GUI wizard is hostile | When the persona is technical | **Adopt CLI-first** with GUI as fallback. |
| Wizard depth | Stripe Atlas, Linear, Vercel: ≤ 3 mandatory steps before first value | Always | **Cut CoreLink wizard 6 → 2 mandatory steps.** Defer DPA + billing + region pick. |
| Prefilled defaults | Linear creates a "starter workspace" with sample issues; Supabase auto-names the first project | Always | **Adopt.** Auto-create "default" tenant, default region (closest CF region by Geo-IP), no plan choice on signup. |

### §1.4 Reverse trial vs freemium vs free trial

ChartMogul 2026 conversion ranges (via Pulseahead): opt-in trial 8.9 % → paid; CC-required trial 31.4 %; good freemium 3–5 %; great freemium 6–8 % ([Pulseahead](https://www.pulseahead.com/blog/trial-to-paid-conversion-benchmarks-in-saas)). Lenny Rachitsky's interview with Lauryn Isford (Airtable / Notion patterns): **reverse trial > free trial > freemium** for products where the paid-tier value is non-obvious from the free tier ([Lenny's Newsletter](https://www.lennysnewsletter.com/p/freemium-trials-free)). ProdPad's definition: reverse trial = start everyone on full paid features for N days, then downgrade to free if no card ([ProdPad](https://www.prodpad.com/glossary/reverse-trial/)).

**CoreLink fit:**
- Build-cache value *is* obvious from any tier (cache hit = build faster). Paid-tier value is bandwidth, multi-region, multi-tenant SSO, audit log retention, support SLA.
- A free tier with **non-trivial monthly bandwidth + storage quota** ("5 GB cache, 50 GB egress/month") is the natural shape — devs need real headroom to validate before pitching internally.
- **Adopt freemium**, not reverse trial. Reverse-trial works when the paid features are core (Notion AI, Linear projects); CoreLink's paid features are quotas + governance, which graduates from free naturally as usage scales. Reverse trial would feel like a bait-and-switch ("you used the cache, now it's locked").

### §1.5 Conversion-funnel benchmarks (B2B dev tools)

Triangulated from MAccelerator, FirstPageSage, Userpilot, Pulseahead (all 2025–2026):

| Stage | Median | Top-quartile | CoreLink starting target |
|---|---|---|---|
| Landing visitor → signup start | 1.4 % B2B SaaS overall, 3.5–7.1 % dev-tools | 8–10 % | 4 % |
| Signup start → signup complete | 60–75 % | 85 %+ | 75 % |
| Signup complete → activated (TTFV met) | 20 % industry, top-quartile **40 %+** | 50 %+ | 35 % |
| Activated → paid (freemium) | 3–5 % good, 6–8 % great | 10 %+ | 5 % |
| End-to-end visitor → paid | ~0.05–0.2 % | 0.5 %+ | 0.15 % |

These are **targets to measure against**, not promises. The point of §7 instrumentation is to know which stage is leaking.

---

## §2 Current CoreLink onboarding — gaps vs PLG canon

Cross-referenced against `2026-05-27-launch-readiness-check.md` and the wizard step files. Code paths are absolute relative to repo root.

| Gap | Where | PLG canon violated | Severity |
|---|---|---|---|
| No primary signup CTA above the fold | `apps/docs/docs/index.mdx` | "One primary CTA, single decision" (Unusual VC self-serve PLG MVP guide) | RED for funnel-start |
| Six-step wizard before first cache write | `apps/admin-ui/src/app/[locale]/onboarding/{tenant,region-plan,dpa,pat,billing,done}` | "≤ 3 mandatory steps to first value" (Stripe, Linear, Vercel pattern) | RED for TTFV |
| Billing step is a scaffold (raw `payment_method_id` text input) | `apps/admin-ui/src/app/[locale]/onboarding/billing/BillingStep.tsx:57` | "Defer money question until after value is delivered" (Lenny / Wes Bush) | RED for activation |
| All `[locale]/*` routes 500 in prod | Pages secrets not applied — flagged blocking in Wave-32 Phase F | "Self-serve experience must actually work" (table-stakes) | RED, blocks everything |
| Pilot funnel is the only working path, token-gated | `apps/docs/src/pages/pilot/apply.tsx` → `corelink-signup.humangr.com/v1/signup/pilot/{token}` | Sales-led, not PLG. Invite-only is fine as a parallel funnel; not as the sole front door | YELLOW — fine for design partners, not for launch |
| No analytics SDK wired (only cookie-consent category exists) | `grep -r posthog\|amplitude\|mixpanel apps/admin-ui/src/` returns nothing functional | "If you can't measure activation you can't optimize it" (every PLG source) | RED for §7 below |
| No public sandbox / try-without-signup | Nothing exists | Tailscale, Linear, Excalidraw, Plausible-demo pattern | YELLOW — nice-to-have, expensive to build |
| Tutorial is six pages (01-installation → 06-verify-cache-hit) | `apps/docs/docs/tutorial/` | Long-form is fine in docs; the *wizard* should not duplicate this length | YELLOW — keep the docs, shrink the wizard |
| No copy-paste single-command setup | Tutorial pages 1–6 require manual config of `wrangler.toml`, PAT, endpoint URL | "Single curl-pipe-sh" pattern (Sentry wizard, Vercel CLI, Tailscale `up`) | YELLOW — biggest unlock for TTFV |
| DPA gating is in the critical path | `apps/admin-ui/src/app/[locale]/onboarding/dpa/DpaStep.tsx` with scroll-tracking | Legally needed *before processing personal data* — for first-cache use the user is the only data subject, can defer to first-team-invite | YELLOW — legal sign-off needed to move it |

**Net:** TTFV today (if everything worked) ≈ 45–90 min for a Bazel user. Target ≤ 10 min. Gap is ~5–10× and structural, not cosmetic.

---

## §3 Recommended CoreLink TTFV target + activation event

### §3.1 TTFV target

| Metric | Target | Stretch |
|---|---|---|
| Median TTFV (signup → first cache hit) | **≤ 10 min** | ≤ 5 min |
| p75 TTFV | ≤ 20 min | ≤ 10 min |
| Median time from signup → authenticated `corelink ping` | **≤ 5 min** | ≤ 2 min |
| % of signups that reach activation in same session | **≥ 35 %** | ≥ 50 % |

Single-session activation matters more than absolute time: a user who closes the tab to context-switch will not come back.

### §3.2 Canonical activation event — exactly one

> **`first_cache_hit`** = the **second** successful CAS read for the same content-addressed key inside a tenant, where the first read was a miss-then-write within the previous 24 h, and both operations carry the same authenticated `tenant_id` + `client_id` (CLI or CI principal).

Why this and not the alternatives:

| Candidate | Reject reason |
|---|---|
| `tenant_created` | Implies effort, not value. Wizard-completion vanity metric. |
| `pat_issued` | Same — effort, no value yet. |
| `first_cas_write` | One-way push could be a script kicking the tyres; doesn't prove the user got speedup. |
| `first_cache_hit` ✓ | Implies (a) two real builds happened, (b) the cache returned a real artefact, (c) the user's build was actually faster on the second run. Hard to fake without real Bazel/Buck2/cargo activity. |
| `100_cache_hits` | Better predictor of retention but too slow — moves TTFV target out of single-session range. Use as the **PQL** threshold (§7) instead. |

The event is unambiguous in the DB: it is the second row in `cas_events` table where `event_type='read_hit'` and `(tenant_id, content_hash)` matches a prior `read_miss` within 24 h. A Drizzle / SQL definition fits in ≤ 10 lines.

**Secondary milestones** to track but not call "activation":
- `signup_started`, `signup_completed` — funnel top.
- `pat_issued`, `first_cli_authed` (= first `corelink ping` returning 200 with the new PAT) — pre-activation TTFV checkpoint.
- `first_cas_write` — pre-activation, "trying it" signal.
- `first_cache_hit` — **activation**.
- `10_cache_hits` (D1), `100_cache_hits` (D7) — engagement curve.
- `team_member_invited` — expansion signal.
- `paid_subscription_started` — monetization.

---

## §4 New onboarding flow — step-by-step with per-step time targets

Designed to deliver `first_cache_hit` in median ≤ 10 min for a user who already has a Bazel project on disk. Each step has (i) wall-clock target, (ii) what the user sees, (iii) what the system does, (iv) event logged.

| # | Step | Target | User-side | System-side | Event |
|---|---|---|---|---|---|
| 0 | Land on `corelink-docs.humangr.com/` | n/a | Hero with single primary CTA "Start free — 5 GB free monthly" | nothing | `landing_view` |
| 1 | Click CTA → Clerk sign-up with **GitHub OAuth first**, email/password second | 30 s | One click for GitHub users; one email + magic link for others | Clerk session; webhook fires `user.created` to `corelink-signup.humangr.com` | `signup_started`, `signup_completed` |
| 2 | **Auto-provision** tenant + free-plan subscription + nearest region (Geo-IP) + initial PAT | 2 s server-side, invisible to user | Single "Setting up your cache…" splash | (a) `INSERT tenant`, (b) attach `plan=free`, (c) compute closest region from CF Geo-IP, (d) issue PAT with `scope=cas:rw`, (e) seed sample `wrangler.toml` snippet in DB for download | `tenant_created`, `pat_issued`, `region_assigned` |
| 3 | Land on `/welcome` — **one screen, three things**: (a) one-line copy-paste install command, (b) PAT shown once with copy button, (c) live status pane that updates when the system sees first call | 60 s read time | One-line copy-paste: `curl -fsSL https://get.corelink.io \| sh -s -- --token=ct_xxx --region=ord` | Polls `cas_events` table for `tenant_id` once per 2 s | `welcome_view` |
| 4 | User runs the one-liner in their terminal | 30 s download + 5 s exec | Single command: downloads CLI, writes config to `~/.corelink/config.toml`, runs `corelink ping`. Returns "Cache reachable in 12 ms. Try `corelink bazel-init` next." | Server sees first authenticated `/v1/ping` from tenant; live pane on `/welcome` flips to "Connected" with green check | `first_cli_authed` |
| 5 | User runs `corelink bazel-init` in their repo | 60 s | CLI detects `WORKSPACE` / `MODULE.bazel`, appends three lines to `.bazelrc` (`build --remote_cache=…`, `build --remote_header=…`, `build --remote_upload_local_results=true`), prints "Run `bazel build //…` twice to see your first cache hit." | n/a yet | `bazel_init_completed` |
| 6 | User runs `bazel build //...` (first time — populates cache) | depends on user's build (1–10 min) | Bazel build with cache miss, every action uploaded | First `/v1/cas/{hash}` writes land in CAS | `first_cas_write` (per write, but the first one is the funnel event) |
| 7 | User runs `bazel build //...` (second time — cache hits) | < 30 s for most actions | Bazel reports `INFO: 247 processes: 247 remote cache hit.` | Second `/v1/cas/{hash}` reads hit existing keys | **`first_cache_hit` — ACTIVATION** |
| 8 | `/welcome` live pane animates: "First cache hit. Your build was 8× faster." Shows next-action card: "Wire this into CI" with copy-paste GitHub Actions snippet | passive | n/a | `welcome_activation_shown` |

**Cumulative target time, assuming a 60-second `bazel build`:** 30 s + 60 s + 30 s + 60 s + 60 s + 60 s + 30 s = **~5 min** to activation. For larger builds the wall clock is dominated by Bazel itself, not the wizard — which is the right shape.

**What got deferred out of the wizard:**
- **DPA accept** → first time the tenant invites a second member (the moment they actually become a data controller for someone else). Self-service of own account doesn't require pre-collection DPA — only the privacy notice, which is a passive link in the footer per LGPD Art. 9.
- **Region pick** → auto-selected from Geo-IP; "change region" lives in Settings, not the critical path.
- **Plan pick** → everyone starts on Free. Upgrade prompt fires when usage crosses 70 % of quota (in-product), not in the wizard.
- **Card on file** → never asked at signup. Asked only when (a) user hits quota and wants to keep going, or (b) user clicks "upgrade" from settings. Stripe Checkout Session (hosted) handles this, not in-app Elements — kills the `BillingStep.tsx` scaffold problem.
- **Tenant name** → defaults to `${user.github_handle}-default` or `${user.email_local_part}-default`; user can rename in Settings.

---

## §5 Patterns to ADOPT

For each: source, decision, concrete CoreLink action.

| Pattern | Source | CoreLink action |
|---|---|---|
| **Single primary CTA above fold** | Unusual VC self-serve PLG playbook | Add hero block to `apps/docs/docs/index.mdx` (or convert root to `apps/docs/src/pages/index.tsx`). One H1, one sub-head, one button. CTA = `https://corelink-app.humangr.com/sign-up`. Demote the three Diátaxis cards below the fold. (Already in launch-readiness §1 recommendation; this audit re-affirms.) |
| **Copy-paste one-liner install** | Sentry wizard, Vercel CLI, Tailscale `up`, Plausible script tag | Ship `https://get.corelink.io` as a CF Worker serving a signed install script. Single command does: download binary for OS+arch, write `~/.corelink/config.toml` with `token=` and `region=` from query params, run `corelink ping`, print next-action. ~2 days work. |
| **GitHub OAuth first, email second** | Vercel, Supabase, Linear | Reorder Clerk `<SignUp />` providers in `apps/admin-ui/src/app/sign-up/[[...sign-up]]/page.tsx`. GitHub at top. Free in Clerk; ~10 min of config. |
| **Auto-provision defaults (tenant, region, PAT, plan)** | Linear "starter workspace", Supabase initialization-as-activation | Add `auto-provision-on-signup` webhook handler in `corelink-signup.humangr.com`. Replace the `tenant` + `region-plan` + `pat` wizard steps. ~3 days work including tests. |
| **`corelink bazel-init` (CLI does the config writing)** | `npx @sentry/wizard`, `vercel init`, `supabase init` | Add `corelink bazel-init`, `corelink buck2-init`, `corelink cargo-init` subcommands. Detect repo type, append config lines idempotently. ~1 week including the three integrations. |
| **Live activation pane on `/welcome`** | Vercel deployment-progress pane, Supabase project-ready toast | Server-Sent Events stream from `corelink-app` polling `cas_events` for the tenant. Switch state on `first_cli_authed`, `first_cas_write`, `first_cache_hit`. ~3 days. |
| **In-product analytics (PostHog OSS, self-hostable)** | PostHog tutorial cited in search results; cookie consent already has the `analytics` category wired | Add PostHog client behind the existing cookie-consent `analytics` gate. EU-region PostHog Cloud or self-hosted on Cloudflare. ~1 day to wire, plus event taxonomy from §7. |
| **Reverse-funnel pricing CTA (Stripe Checkout Session)** | Stripe's own onboarding, Linear, Cal.com | Replace the `BillingStep.tsx` scaffold with **no in-app billing step at all**. When user upgrades, redirect to Stripe Checkout Session, success-URL flips plan via webhook. Kills launch-readiness §3 Stripe blocker. ~2 days. |
| **Sample project / clone-and-go demo repo** | `vercel/next.js-examples`, `supabase/examples`, `bazelbuild/examples` | Ship `humangr-labs/corelink-bazel-example` — a 200-LOC Bazel project that already has `bazel-init` applied, scripted to demo a cache-miss-then-hit. Linked from `/welcome` as "Don't have a Bazel project handy? Try this one." ~2 days. |
| **Free tier with real headroom** | Lenny / Wes Bush "freemium = magnet, paid = scale" | Lock launch quota at **5 GB cache, 50 GB egress/month, single region, unlimited PATs, 1 seat**. Generous enough to validate; small enough that real adopters upgrade within ~4 weeks. |

---

## §6 Patterns to SKIP — with rationale

| Pattern | Why skip for CoreLink |
|---|---|
| **Public sandbox (try without signup, à la `try.tailscale.com`)** | Build-cache value requires the user's own build artefacts. A sandbox would either need to ship a canned Bazel build (high cost, doesn't prove value for *user's* repo) or expose raw CAS read/write (which is just a free tier with no auth — abuse magnet). Free tier with GitHub-OAuth-instant-signup is the right substitute. |
| **Reverse trial** | §1.4 — paid features are quotas + governance, not core capability. Reverse trial would feel like a bait-and-switch when quotas re-apply. |
| **Credit card upfront on free tier** | ChartMogul 31.4 % vs 8.9 % conversion looks tempting, but signup volume collapse is wrong for a solo founder pre-PMF. Re-evaluate at $50K MRR. |
| **Slack / Discord auto-invite on signup** | Community-of-one is worse than no community. Build community to ≥ 50 active members **before** auto-inviting; until then a passive footer link is enough. |
| **"5 stars on GitHub" in-product prompt** | Folklore is mixed for B2B; works for OSS devtools (Cal.com, Plausible), feels desperate from a paid product. Skip until OSS components exist (e.g. `corelink-cli` is open-source — *that* repo can ask). |
| **Long-form interactive product tour (Pendo / Userpilot style)** | Wrong shape for devs — they want the terminal, not click-through tooltips. The one-line install + live `/welcome` pane *is* the tour. |
| **Email drip campaign before activation** | Reverse signal: if the wizard didn't activate them, an email won't. Spend the dev-budget on shortening TTFV instead. Re-evaluate after baseline activation rate is known. |
| **Forum / community at launch** | Slack/Discord/Discourse all need ≥ daily attention from the founder. GitHub Discussions on `humangr-labs/corelink` is enough for the first 50 users — async, public, indexable. |
| **Custom in-product onboarding builder (Appcues, Userflow)** | Adds a dependency and a recurring SaaS bill for a problem solved by 200 LOC of React on `/welcome`. Build, don't buy, at this stage. |
| **Gamification (badges, streaks)** | Wrong audience. Devs respond to "your build was 8× faster", not "you earned the Cache Champion badge". |

---

## §7 Conversion-funnel instrumentation plan

### §7.1 Event taxonomy

Single canonical table, single source of truth. All client + server events go through one ingest endpoint and into one append-only `analytics_events` table (D1 or Postgres — D1 sufficient for first 12 months at projected volume).

| Stage | Event name | Source | Required props | Notes |
|---|---|---|---|---|
| Acquisition | `landing_view` | docs site (CF Worker on `corelink-docs`) | `path`, `referrer`, `utm_*`, `session_id` | Anonymous, no PII, fired pre-consent |
| Acquisition | `landing_cta_click` | docs site | `cta_id`, `session_id` | Same |
| Acquisition | `signup_started` | admin-ui sign-up page | `session_id`, `auth_provider` (`github` / `email`) | Fired on Clerk widget mount |
| Acquisition | `signup_completed` | Clerk webhook → signup-worker | `user_id`, `auth_provider`, `created_at`, `email_domain` | Server-side, post-consent |
| Onboarding | `tenant_created` | signup-worker auto-provision | `tenant_id`, `user_id`, `region`, `plan` | Always `plan=free` at signup post-§4 redesign |
| Onboarding | `pat_issued` | signup-worker | `tenant_id`, `pat_id`, `scope`, `created_at` | First PAT only — subsequent issuance is `pat_issued_additional` |
| Onboarding | `welcome_view` | admin-ui `/welcome` | `tenant_id`, `seconds_since_signup` | SSE stream attaches here |
| Activation pre-cursor | `first_cli_authed` | CAS data plane on first authenticated `/v1/ping` | `tenant_id`, `pat_id`, `cli_version`, `user_agent`, `seconds_since_signup` | TTFV milestone #1 |
| Activation pre-cursor | `first_cas_write` | CAS data plane | `tenant_id`, `pat_id`, `content_hash`, `bytes`, `seconds_since_signup` | TTFV milestone #2 |
| **ACTIVATION** | **`first_cache_hit`** | CAS data plane (deduped per tenant — only the first one ever) | `tenant_id`, `pat_id`, `content_hash`, `seconds_since_signup`, `seconds_since_first_cas_write` | **§3.2 canonical event** |
| Engagement | `cache_hits_10` / `cache_hits_100` / `cache_hits_1k` | nightly batch over `cas_events` | `tenant_id`, `day`, `cumulative_hits` | Retention proxies |
| Engagement | `team_member_invited` | admin-ui invite action | `tenant_id`, `inviter_user_id`, `invitee_email_domain` | Expansion signal |
| Monetization | `pricing_view` | docs site | `path`, `session_id`, `tenant_id?` | If logged in, attach `tenant_id` |
| Monetization | `checkout_started` | Stripe Checkout Session creation | `tenant_id`, `target_plan` | Server-side |
| Monetization | `paid_subscription_started` | Stripe webhook | `tenant_id`, `plan`, `mrr_usd` | The ka-ching event |
| Monetization | `plan_downgraded` / `subscription_canceled` | Stripe webhook | `tenant_id`, `from_plan`, `to_plan`, `reason?` | Churn |

### §7.2 Funnel SQL — one query, one truth

Cohort by signup day. Each row = one signed-up user. Boolean columns for each milestone. Pivot for funnel rates. This is the only funnel definition that ships:

```sql
-- conceptual; D1 / Postgres-compatible
WITH cohort AS (
  SELECT user_id, tenant_id, MIN(created_at) AS signed_up_at
  FROM analytics_events
  WHERE event_name = 'signup_completed'
  GROUP BY user_id, tenant_id
)
SELECT
  date_trunc('day', signed_up_at) AS cohort_day,
  COUNT(*) AS signups,
  COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM analytics_events e
       WHERE e.tenant_id = cohort.tenant_id AND e.event_name = 'first_cli_authed'
         AND e.created_at < signed_up_at + INTERVAL '10 minutes')) AS authed_10m,
  COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM analytics_events e
       WHERE e.tenant_id = cohort.tenant_id AND e.event_name = 'first_cache_hit'
         AND e.created_at < signed_up_at + INTERVAL '10 minutes')) AS activated_10m,
  COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM analytics_events e
       WHERE e.tenant_id = cohort.tenant_id AND e.event_name = 'first_cache_hit'
         AND e.created_at < signed_up_at + INTERVAL '24 hours')) AS activated_24h,
  COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM analytics_events e
       WHERE e.tenant_id = cohort.tenant_id AND e.event_name = 'paid_subscription_started'
         AND e.created_at < signed_up_at + INTERVAL '30 days')) AS paid_30d
FROM cohort
GROUP BY 1 ORDER BY 1 DESC;
```

### §7.3 Privacy + consent

- All events server-side fire regardless of consent (legitimate-interest necessary product-operation events under LGPD Art. 7 II and GDPR Art. 6(1)(b)). Use a strict allow-list: tenant_id (pseudonymous), event name, timestamp, technical metadata.
- Client-side events (`landing_view`, `landing_cta_click`) fire **only** if `cookies.analytics === true` per the existing `CookiePolicyPage.tsx` consent table — re-use that gate. Default = off in EU/BR jurisdictions per Geo-IP.
- Email and IP are never stored in `analytics_events`. `email_domain` only (e.g. `acme.com`) for ICP segmentation.
- Retention: 13 months rolling, then aggregated to weekly cohort summary.

### §7.4 Solo-founder realistic stack

| Layer | Recommendation | Cost |
|---|---|---|
| Ingest | Single CF Worker `corelink-analytics`, POST `/v1/event`, write to D1 `analytics_events` table | $0 marginal |
| Storage | D1 (fits comfortably below 5 GB for first 12 months) | $0 |
| Dashboard | Metabase OSS on a $7/mo Fly.io VM, pointed at D1 via SQLite proxy; or PostHog Cloud EU starter | $7–25/mo |
| Alerts | One nightly cron in the worker: if `activated_24h / signups < 0.20` for 3 days running → email Gustavo | $0 |

No Segment, no Amplitude, no Mixpanel. Add those when ARR > $250K or when the dashboard query is too slow — neither will be true in year 1.

### §7.5 The three numbers Gustavo looks at every Monday

1. **Median TTFV (signup → `first_cache_hit`)** — target ≤ 10 min.
2. **D1 activation rate** = signups where `first_cache_hit` happened within 24 h / total signups — target ≥ 35 %.
3. **Free → paid 30-day conversion** — target ≥ 5 %.

If (1) regresses, audit wizard friction. If (2) regresses, audit the docs / install one-liner. If (3) regresses, audit upgrade prompts + quota thresholds.

---

## §8 Recommended next-action sequence (interlocks with launch-readiness §7)

Ordered so each step unblocks the next. Bracketed numbers reference the steps already in `2026-05-27-launch-readiness-check.md` §7.

1. **(operator, ~30 min) [launch §7.1]** Apply Pages secrets. Pre-requisite for everything.
2. **(operator, ~1 h)** Add PostHog (or self-hosted equivalent) client to admin-ui + docs sites behind the existing cookie-consent `analytics` gate. Wire the `signup_started`, `signup_completed`, `landing_view`, `pricing_view` events. Establishes baseline measurement before any change.
3. **(code, ~3 days)** Implement auto-provision-on-signup (replaces wizard steps `tenant` + `region-plan` + `pat`). Wire `tenant_created`, `pat_issued`, `region_assigned` server events.
4. **(code, ~2 days)** Build `https://get.corelink.io` install-script Worker + the matching `corelink` CLI ingestion of `--token=` / `--region=` flags. Wire `first_cli_authed` event on first authenticated `/v1/ping`.
5. **(code, ~1 week)** Implement `corelink bazel-init` (and stub `buck2-init`, `cargo-init` for later). Append `.bazelrc` lines idempotently. Wire `bazel_init_completed`.
6. **(code, ~3 days)** Build the live `/welcome` SSE pane. Replace `/onboarding/done` route. Wire `welcome_view`, `welcome_activation_shown`.
7. **(code, ~1 day)** Define `first_cache_hit` event in the CAS data plane (CF Worker), with dedupe per tenant. The single most important line of code in the funnel.
8. **(code, ~2 days)** Replace `BillingStep.tsx` scaffold with **deletion** + Stripe Checkout Session redirect from settings page on upgrade click. Wire `checkout_started`, `paid_subscription_started` (webhook).
9. **(code, ~2 days)** Author `humangr-labs/corelink-bazel-example` demo repo. Linked from `/welcome`.
10. **(code, ~1 day)** Ship the Monday-three-numbers dashboard (Metabase or a single static HTML page rendered by a CF Worker hitting D1).
11. **(verification, ~2 h)** End-to-end smoke: fresh GitHub account → signup → run install one-liner → run `corelink bazel-init` on the example repo → `bazel build //...` twice → confirm `first_cache_hit` event lands within 10 min. Stopwatch the median.
12. **(ongoing, weekly)** Read the three numbers, audit whichever regressed.

Total scoped effort if Gustavo executes solo: ~4 weeks of focused work after launch-readiness §7 steps 1–6 land. Parallelizable down to ~2.5 weeks if some pieces are delegated to agents.

---

## §9 Sources

- [Count.co — TTFV formula, benchmarks & tips](https://count.co/metric/time-to-first-value)
- [ProductGrowth.in — SaaS onboarding benchmarks 2026](https://productgrowth.in/insights/saas/saas-onboarding-benchmarks/)
- [PayProGlobal — TTFV definition + calculator](https://payproglobal.com/answers/what-is-saas-time-to-first-value-ttfv/)
- [Prospeo — Time-to-value benchmarks and reduction](https://prospeo.io/s/time-to-value-ttv)
- [Mixpanel — Product-led growth in 2026, complete guide](https://mixpanel.com/blog/product-led-growth/)
- [Productled.org — PLG metrics foundations](https://www.productled.org/foundations/product-led-growth-metrics)
- [Lenny's Newsletter — Freemium vs trial vs free](https://www.lennysnewsletter.com/p/freemium-trials-free)
- [Lenny's Newsletter — What is a good free-to-paid conversion](https://www.lennysnewsletter.com/p/what-is-a-good-free-to-paid-conversion)
- [ProdPad — Reverse trial glossary](https://www.prodpad.com/glossary/reverse-trial/)
- [Pulseahead — Trial-to-paid conversion benchmarks (ChartMogul 2026 cited)](https://www.pulseahead.com/blog/trial-to-paid-conversion-benchmarks-in-saas)
- [Pocus — Definitive PQL guide](https://www.pocus.com/blog/the-definitive-pql-guide-part-1)
- [OpenView — Guide to product-qualified leads](https://openviewpartners.com/blog/your-guide-to-product-qualified-leads-pqls/)
- [Hightouch — Definitive PQL guide](https://hightouch.com/blog/the-definitive-guide-to-product-qualified-leads)
- [Unusual VC — Self-serve PLG MVP playbook](https://www.unusual.vc/field-guide/nail-your-self-serve-mvp-product/)
- [Craft Ventures — Inside Supabase's breakout growth (initialization-as-activation)](https://medium.com/craft-ventures/inside-supabases-breakout-growth-lessons-scaling-to-4-5m-devs-powering-ai-vibe-coding-dc574acfafaa)
- [BuildBuddy — Bazel remote caching + RBE explained](https://www.buildbuddy.io/blog/bazels-remote-caching-and-remote-execution-explained/)
- [BuildBuddy — RBE setup quickstart](https://www.buildbuddy.io/docs/rbe-setup/)
- [Bazel — Remote caching reference](https://bazel.build/remote/caching)
- [PostHog — Sign-up funnel tutorial (Next.js + Supabase)](https://posthog.com/tutorials/nextjs-supabase-signup-funnel)
- [MAccelerator — B2B SaaS conversion benchmarks 2025](https://maccelerator.la/en/blog/entrepreneurship/b2b-saas-conversion-rate-benchmarks-2025/)
- [FirstPageSage — Average SaaS conversion rates 2026](https://firstpagesage.com/seo-blog/average-saas-conversion-rates/)
- [Causalfunnel — B2B SaaS funnel benchmarks 2026](https://www.causalfunnel.com/blog/b2b-saas-funnel-conversion-benchmarks-2026-data-insights/)
- [DEV — One-line installer pattern caveats](https://dev.to/vineethnkrishnan/why-my-one-line-installer-worked-everywhere-except-wsl-44ab)
- [DEV — PLG and what it means for DevRel (Matt Stratton)](https://dev.to/mattstratton/product-led-growth-and-what-it-means-for-devrel-10mj)
- Internal: `specs/_audits/2026-05-27-launch-readiness-check.md` (cited throughout for current CoreLink state)

## §10 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
