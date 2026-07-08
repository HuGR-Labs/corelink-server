---
id: "AUDIT-2026-05-27-PHASE-0-EXECUTION-PLAN"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "phase-0", "execution-plan", "agent-dispatch", "launch", "fix-the-broken"]
references:
  - "specs/_audits/2026-05-27-launch-readiness-check.md"
  - "specs/_audits/2026-05-27-plg-onboarding-framework.md"
  - "specs/_audits/2026-05-27-metrics-instrumentation.md"
  - "specs/_audits/2026-05-27-legal-ops-setup.md"
---

# Phase 0 Execution Plan — fix-the-broken sprint

> **Mandate.** Turn Phase 0 of the launch roadmap (fix the 5 broken items from `2026-05-27-launch-readiness-check.md` + immediate PLG/metrics/legal prereqs) into a concrete agent-dispatch plan ready to fire today. Each item below is one agent prompt, one owner, one effort estimate, one parallelization band, one acceptance criterion, one dispatch order.
>
> **Source of truth for "broken".** Launch-readiness §6 verdict: 1/5 GREEN, 2 RED (L2 signup, L4 legal footer), 2 YELLOW (L1 landing CTA, L3 pricing stub), 1 NEEDS-INPUT (L5 customer pipeline — Gustavo-only, not in Phase 0). Phase 0 closes the 4 code-shaped items + PLG redesign prereqs + measurement baseline so that re-running launch-readiness returns 5/5 GREEN.
>
> **Wave context.** Wave-32 sealed prod deploy 2026-05-26 commit `4d4fb8f6`. Wave-33 reorg PAUSED Wave-32 Phases B-I. This Phase 0 is the **user-facing surface** fix sprint that unblocks public launch — it runs **in parallel** with Wave-33 (no shared files; Wave-33 is `crates/`, Phase 0 is `apps/admin-ui/`, `apps/docs/`, `apps/signup-worker/`, and net-new CLI/install-script repos).
>
> **DCO chain.** This plan only commits the planning doc. Dispatched agents commit their own work on per-branch worktrees with SEAL protocol.

---

## §1 Dispatch matrix

| # | Agent ID | Scope (one sentence) | Owner | Agent hours | Gustavo minutes | Parallel-safe with | Blocks |
|---|---|---|---|---|---|---|---|
| **A** | `LEGAL-FOOTER-WIRE` | Make `/legal/{privacy,terms,sub-processors}` resolve on `corelink-docs.humangr.com` (redirects to existing content; author new `/legal/terms` page from admin-ui content). | agent | 4 h | 0 | B, C, D, E, F, G, H | nothing |
| **B** | `CF-PAGES-SECRETS-PREP` | Generate exact wrangler-CLI script + CF-Pages-dashboard click-path doc for the 3 missing secrets (`CLERK_SECRET_KEY`, `STRIPE_SECRET_KEY`, `RESEND_API_KEY`), token sources, and post-apply smoke commands. Hand off to Gustavo. | agent (prep) → Gustavo (execute) | 1 h | 30 min | A, C, D, E, F, G, H | nothing (B execute unblocks L2 500s, but Phase 0 agents do not depend on B-execute) |
| **C** | `BILLINGSTEP-DELETE-WIRE-CHECKOUT` | Delete `BillingStep.tsx` scaffold + remove `billing` from wizard step graph; add "Upgrade" button on dashboard + settings → POST to new `/v1/checkout/session` Worker action returning Stripe Checkout Session URL → 303-redirect; wire the existing `paid_subscription_started` webhook. | agent | 6 h | 0 | A, B, D, E, F, G, H | nothing |
| **D** | `LANDING-CTA-HERO` | Add hero block to `apps/docs/docs/index.mdx` (or convert root to `apps/docs/src/pages/index.tsx`) with H1 promise + 1-line ICP + 1-line outcome + single primary CTA "Start in 5 min — free" → `https://corelink-app.humangr.com/sign-up`. Demote Diátaxis cards below fold. | agent | 3 h | 0 | A, B, C, E, F, G, H | nothing |
| **E** | `PRICING-UNSTUB-CHECKOUT-WIRE` | Remove `provisional: true` from every tier in `apps/docs/src/lib/pricing.ts`; collapse 5-tier → 3-tier (Free 10 GB + Pro $25/mo + Enterprise contact) per pricing-research consensus; rewrite CTAs: Free → `/sign-up`, Pro → Stripe Checkout Session redirect, Enterprise → `/contact` (mailto fallback). | agent | 3 h | 0 | A, B, C, D, F, G, H | nothing |
| **F** | `ONBOARDING-WIZARD-2STEP` | Collapse wizard `tenant → region-plan → dpa → pat → billing → done` → `signup → /welcome`. Add `auto-provision` server-side webhook handler in `corelink-signup.humangr.com` that creates tenant + nearest region (Geo-IP) + free plan + first PAT on Clerk `user.created`. Build `/welcome` SSE pane that polls for `first_cli_authed` + `first_cache_hit`. Defer DPA to first team-invite, defer billing to upgrade-click. | agent | 8 h | 0 | A, B, C, D, E, G, H | nothing |
| **G** | `METRICS-INSTRUMENT` | Create `analytics_events` D1 table + ingest Worker `corelink-analytics` POST `/v1/event`; wire 12 events from PLG §7.1 taxonomy; create 7 saved views (signup funnel, TTFV cohort, D1/D7/D30 activation, MRR, plan mix, regional latency); add nightly cron Worker emailing Gustavo the Monday-three-numbers; add Plausible `<script>` to `apps/docs/` + `apps/admin-ui/` landing/pricing/sign-up pages behind cookie-consent `analytics` gate. | agent | 6 h | 0 | A, B, C, D, E, F, H | nothing |
| **H** | `CORELINK-CLI-INSTALL-SCRIPT` | Bootstrap `HumanGuardrail/corelink-cli` repo (Rust workspace, `cargo new corelink`); ship `corelink ping`, `corelink bazel-init` (idempotent `.bazelrc` append), `corelink config show`; ship `get.corelink.io` CF Worker serving signed install script `curl -fsSL https://get.corelink.io \| sh -s -- --token=… --region=…` that downloads OS+arch binary, writes `~/.corelink/config.toml`, runs `corelink ping`. | agent | 8 h | 0 | A, B, C, D, E, F, G | nothing (but **F's `/welcome` SSE expects this CLI**; ship in parallel, contract is `POST /v1/ping` returning 200 with `tenant_id` claim from PAT) |

**Totals.**
- Agent effort if dispatched **serial**: ~39 h ≈ 5 days wall-clock.
- Agent effort if dispatched **parallel** (recommended): max(4, 1, 6, 3, 3, 8, 6, 8) = **8 h wall-clock ≈ 1 working day**.
- Gustavo effort: ~30 min for CF Pages secrets (B). Add ~2 h async for Atlas/Mercury/Termageddon (item I below, Phase 0 prereqs but **NOT on the critical path** — Phase 0 ships without those).

---

## §2 Per-agent prompt skeletons

Each prompt below is **drop-in ready** for the orchestrator's agent dispatcher. Format follows the same `PHASE-N-EXECUTION-PLAN`-style prompts already in use this wave. Hard-pause triggers come from `corelink_autonomous_execution_charter.md` §inflection-points.

---

### §2.A `LEGAL-FOOTER-WIRE`

**Mandate.** Make the three public footer-linked legal pages resolve 200 on `corelink-docs.humangr.com`. Today they 404 (verified launch-readiness §4 prod smoke 2026-05-27).

**Files to touch (read first).**
- `apps/docs/docusaurus.config.ts` (footer links at L143-170; install `@docusaurus/plugin-client-redirects` if absent).
- `apps/docs/docs/explanation/privacy/gdpr.mdx`, `lgpd-full.mdx` (existing privacy content).
- `apps/docs/docs/explanation/compliance/{dpa,sub-processors}.mdx` (existing DPA + sub-processors content).
- `apps/admin-ui/src/content/{dpa,privacy-notice}.en.{md,ts}` (canonical content source — mirror to docs).

**Files to write.**
- `apps/docs/src/pages/legal/privacy.tsx` — wraps `gdpr.mdx` content + adds public-facing styling; includes LGPD + GDPR controller-of-record block.
- `apps/docs/src/pages/legal/terms.tsx` — new page; pulls Common Paper CSA template (per `2026-05-27-legal-ops-setup.md` §1.2) — author plain-language T&C covering: service description, plan/payment, AUP, liability cap, governing law (Delaware, post-Atlas), termination.
- `apps/docs/src/pages/legal/sub-processors.tsx` — wraps `apps/admin-ui/src/content/sub-processors.json` as data source; renders table (Cloudflare, Clerk, Stripe, Neon, Resend, Sentry, PostHog/Plausible).

**Plus.** Register `@docusaurus/plugin-client-redirects` in `docusaurus.config.ts` with: `/legal/privacy` → `/legal/privacy` (no-op once page exists, kept for SEO), `/legal/dpa` → `/explanation/compliance/dpa`, `/privacy` → `/legal/privacy`.

**Acceptance.**
1. `curl -I https://corelink-docs.humangr.com/legal/privacy` → 200 (after deploy).
2. `curl -I https://corelink-docs.humangr.com/legal/terms` → 200.
3. `curl -I https://corelink-docs.humangr.com/legal/sub-processors` → 200.
4. Footer links in `docusaurus.config.ts` unchanged (all three already point at `/legal/*`).
5. Pages render plain HTML > 1 KB body (not JS-only hydrate — SEO + LLM crawl).
6. SEAL: codex score ≥ 8.5/10; commit message `docs(legal): wire /legal/{privacy,terms,sub-processors} on docs site`.

**Hard-pause triggers.** (a) Common Paper CSA template requires Gustavo signature-block decision → pause + ping; (b) sub-processors list contains an entry not yet in production (Neon? PostHog?) → pause + ask Gustavo to confirm sub-processor set; (c) Legal sign-off `[ ]` blocks `draft: true` lift → ship pages with `draft: false` but add note "subject to final legal review prior to GA".

---

### §2.B `CF-PAGES-SECRETS-PREP`

**Mandate.** Produce a **drop-in script** Gustavo can run in ≤ 5 min to apply the 3 blocking Pages secrets on `corelink-admin-ui`. Document also: where each secret comes from (Clerk dashboard, Stripe dashboard, Resend dashboard), the dashboard URLs, the wrangler CLI form, the post-apply smoke commands, and a rollback path.

**Files to read.**
- `specs/_audits/sealed/2026-05-26-w32-phaseF-apply-admin-ui-closure.md` §G (original BLOCKING flag).
- `apps/admin-ui/wrangler.toml` (Pages project name).
- `.env.local` (per memory, all Bucket-2 creds validated; copy values from there for the wrangler push — **NEVER** commit `.env.local`).

**Files to write.**
- `specs/_audits/2026-05-27-phase-0-pages-secrets-runbook.md` — runbook with:
  - Pre-flight: `wrangler whoami` + `wrangler pages project list | grep corelink-admin-ui`.
  - Apply 3 secrets in 3 commands: `wrangler pages secret put CLERK_SECRET_KEY --project-name=corelink-admin-ui` (paste from `.env.local`); same for `STRIPE_SECRET_KEY`, `RESEND_API_KEY`.
  - Trigger re-deploy: `wrangler pages deployment create` OR `git push` to redeploy branch.
  - Post-apply smoke (4 curls + expected 200/302).
  - Rollback: `wrangler pages secret delete <name>` + re-paste prior value.

**Acceptance.** Gustavo can execute runbook in ≤ 5 min start-to-finish without leaving the doc. Smoke commands return the expected status without ambiguity.

**Hard-pause triggers.** None — pure documentation; Gustavo executes downstream.

---

### §2.C `BILLINGSTEP-DELETE-WIRE-CHECKOUT`

**Mandate.** Per PLG framework §4: **delete in-app billing from signup**. Money question deferred to upgrade-click via Stripe-hosted Checkout Session. Removes the launch-readiness §2 blocker ("BillingStep is a scaffold").

**Files to delete.**
- `apps/admin-ui/src/app/[locale]/onboarding/billing/BillingStep.tsx` (the scaffold with raw `payment_method_id` text input).
- `apps/admin-ui/src/app/[locale]/onboarding/billing/page.tsx` (route).
- `apps/admin-ui/src/app/[locale]/onboarding/billing/` (directory).
- Any `createSetupIntentAction` server action that exists solely for BillingStep (verify with grep — if used elsewhere, keep).

**Files to edit.**
- `apps/admin-ui/src/app/[locale]/onboarding/` step-graph definition (likely `layout.tsx` or `wizardSteps.ts`): remove `billing` from the step list.
- `apps/admin-ui/src/app/[locale]/dashboard/` (or settings/billing): add `<UpgradeButton />` component that POSTs to `/api/checkout/session` and 303-redirects to returned URL.

**Files to write.**
- `apps/admin-ui/src/app/api/checkout/session/route.ts` — POST handler. Calls Stripe `checkout.sessions.create({ mode: 'subscription', line_items: [{ price: process.env.STRIPE_PRICE_PRO_MONTHLY, quantity: 1 }], success_url: '/upgraded?session_id={CHECKOUT_SESSION_ID}', cancel_url: '/pricing' })`. Authenticated via Clerk session; reads `tenant_id` from session metadata.
- `apps/admin-ui/src/components/UpgradeButton.tsx` — client component, calls the route handler, redirects.
- Webhook handler: extend existing `apps/signup-worker/src/webhooks/stripe.ts` (or create) to handle `checkout.session.completed` → set `tenants.plan = 'pro'` + emit `paid_subscription_started` analytics event.

**Acceptance.**
1. Grep confirms no remaining `BillingStep` references.
2. Onboarding wizard step count reduced by 1 (was 6, now 5 — `tenant → region-plan → dpa → pat → done`; full 2-step collapse happens in F).
3. Local Stripe-test-mode click on Upgrade redirects to Stripe Checkout in ≤ 2 s.
4. Webhook idempotency: replaying the same `checkout.session.completed` is a no-op.
5. SEAL: codex ≥ 8.5/10; commit `feat(admin-ui): remove BillingStep scaffold + wire Stripe Checkout Session on upgrade`.

**Hard-pause triggers.** (a) `STRIPE_PRICE_PRO_MONTHLY` not provisioned in Stripe dashboard → pause + ping Gustavo to create product + price (cheap, ~10 min on Gustavo's side); (b) existing webhook handler conflicts with the new event → triage + log + continue with extension.

---

### §2.D `LANDING-CTA-HERO`

**Mandate.** Add hero block above the fold on `corelink-docs.humangr.com/` with single primary CTA. Removes launch-readiness §1 YELLOW.

**Files to edit.**
- `apps/docs/docs/index.mdx` (current root, Docusaurus `slug: /`).
- OR convert root to `apps/docs/src/pages/index.tsx` if richer JSX needed (Docusaurus supports both; `pages/` wins if both exist — verify before deleting).
- `apps/docs/src/css/custom.css` (or `apps/docs/src/components/Hero/Hero.module.css`) for hero styling.

**Files to write.**
- `apps/docs/src/components/Hero/Hero.tsx` — H1 + sub + CTA. Copy below.
- `apps/docs/src/components/Hero/Hero.module.css`.

**Copy (Gustavo can revise post-merge).**
```
H1:       Build cache that survives every CI runner you'll ever spin up.
Sub:      Multi-tenant, content-addressable cache for Bazel, Buck2, Cargo,
          Docker, and ML jobs. Five GB free. No card needed.
Primary CTA: "Start in 5 min — free"  →  https://corelink-app.humangr.com/sign-up
Secondary:    "Read the docs"          →  /tutorial/01-installation
```

**Acceptance.**
1. `curl https://corelink-docs.humangr.com/ | grep -i "start in 5 min"` returns a match (server-rendered, not JS-hydrated).
2. CTA button is visible above the fold at 1440×900 (Gustavo will eyeball post-deploy).
3. Diátaxis 3-card nav still present below hero.
4. Lighthouse SEO score ≥ 90.
5. SEAL: codex ≥ 8.5/10; commit `feat(docs): add hero block + primary signup CTA above fold`.

**Hard-pause triggers.** (a) `apps/docs/src/pages/index.tsx` already exists and conflicts → merge logic, keep the better; (b) copy reads poorly — author best-guess + leave inline TODO for Gustavo.

---

### §2.E `PRICING-UNSTUB-CHECKOUT-WIRE`

**Mandate.** Lift `provisional: true` + collapse 5-tier to 3-tier + wire CTAs to real targets. Removes launch-readiness §3 YELLOW.

**Files to edit.**
- `apps/docs/src/lib/pricing.ts` — strip `provisional: true` from every tier. Collapse Starter/Team/Pro into single **Pro $25/mo flat** tier (per pricing research consensus: Free + one paid + Enterprise outperforms 5-tier confusion at launch). Keep Free ($0, 10 GB cache, 50 GB egress) and Enterprise (contact us).
- `apps/docs/src/pages/pricing.tsx` — rewrite CTAs:
  - Free tier → `<a href="https://corelink-app.humangr.com/sign-up">Start free</a>`
  - Pro tier → `<a href="https://corelink-app.humangr.com/upgrade?plan=pro">Upgrade to Pro</a>` (admin-ui `/upgrade` triggers the C-flow Checkout Session)
  - Enterprise → `<a href="mailto:gustavo@humangr.com?subject=CoreLink%20Enterprise">Contact sales</a>` (until proper contact form lands).
- `apps/docs/src/pages/pricing/calculator.tsx` — update to reflect 3-tier model.
- `CONTENT-REVIEW.md` — mark Finance/Legal/Product sign-off as `[x] 2026-05-27 — Gustavo Schneiter (sole-founder authority)` for the pricing page lift.

**Acceptance.**
1. Grep `provisional` in `apps/docs/src/lib/pricing.ts` returns 0 matches.
2. `pricing.tsx` shows exactly 3 cards: Free / Pro / Enterprise.
3. No CTA points at `/pilot/apply` (the closed-funnel form).
4. Calculator math correct for the new bandwidth/storage caps.
5. SEAL: codex ≥ 8.5/10; commit `feat(docs): unstub pricing — 3-tier launch (Free / Pro $25 / Enterprise)`.

**Hard-pause triggers.** (a) Gustavo wants 4+ tiers — author both versions, ship 3-tier, leave 4-tier in branch for review; (b) Pro price decision changes from $25 → pause + ping (this affects E + C `STRIPE_PRICE_PRO_MONTHLY`).

---

### §2.F `ONBOARDING-WIZARD-2STEP`

**Mandate.** Per PLG framework §4: collapse 6-step wizard to 2 (signup + welcome). Auto-provision tenant + region + plan + PAT server-side on Clerk `user.created`. Live `/welcome` pane streams activation events.

**Files to edit.**
- `apps/admin-ui/src/app/[locale]/onboarding/` — remove all step subdirs except a thin `welcome/` redirect target (delete `tenant/`, `region-plan/`, `pat/`, `done/`). DPA deferred to first team-invite (move logic to `apps/admin-ui/src/app/[locale]/team/invite/`).
- `apps/admin-ui/src/app/[locale]/welcome/page.tsx` — new top-level route (server component, reads tenant from Clerk session).
- `apps/admin-ui/src/app/[locale]/welcome/WelcomeStream.tsx` — client component, opens EventSource against `/api/welcome/stream`.
- `apps/admin-ui/src/app/api/welcome/stream/route.ts` — SSE handler streaming events from `analytics_events` table filtered by `tenant_id`.
- `apps/signup-worker/src/webhooks/clerk.ts` — extend Clerk `user.created` webhook handler: (1) create tenant with name `${user.github_handle || user.email_local}-default`, (2) Geo-IP closest CF region via `cf.colo`, (3) attach `plan=free`, (4) issue PAT with `scope=cas:rw`, (5) emit `tenant_created`, `region_assigned`, `pat_issued` events to `analytics_events`.

**Files to write.**
- `apps/admin-ui/src/components/InstallOneLiner.tsx` — copy-paste block rendering `curl -fsSL https://get.corelink.io | sh -s -- --token=${pat} --region=${region}` with one-click copy.
- `apps/admin-ui/src/components/ActivationStateBadge.tsx` — three states (waiting / cli-authed / activated) with green-check animation on transition.

**Activation event detection.** New `first_cache_hit` definition lives in the CAS data plane (CF Worker — Wave-33 Stream B may overlap; for Phase 0 add a lightweight emitter in `apps/cas-worker/src/middleware/analytics.ts` that fires the event on the second `read_hit` for the same `(tenant_id, content_hash)` within 24 h). Use SQL definition in PLG framework §3.2 verbatim.

**Acceptance.**
1. Wizard step count = 2 (`/sign-up` + `/welcome`); grep confirms removed step dirs.
2. New Clerk test user → automatic tenant + region + PAT in ≤ 2 s (measured via webhook log).
3. `/welcome` SSE stream emits events visible in browser dev-tools within 1 s of test event insertion.
4. End-to-end manual smoke: fresh signup → curl install one-liner → `corelink ping` → `/welcome` page flips badge from "Waiting" to "Connected". Wall-clock ≤ 5 min.
5. SEAL: codex ≥ 8.5/10; commit `feat(admin-ui,signup-worker): collapse 6-step wizard to signup+welcome with auto-provision`.

**Hard-pause triggers.** (a) DPA-legal disagrees with deferring DPA to first invite → pause + ping; (b) Clerk webhook signing keys missing → pause + log (depends on B-execute landing); (c) CAS data plane changes overlap with Wave-33 Stream B → coordinate with Wave-33 orchestrator before touching `apps/cas-worker/`.

---

### §2.G `METRICS-INSTRUMENT`

**Mandate.** Per metrics audit + PLG framework §7. Establish baseline measurement **before** Phase 0 ships so we can compare pre/post and avoid flying blind.

**Files to write.**
- `apps/analytics-worker/` — new CF Worker. `src/index.ts` (POST `/v1/event`), `src/schema.sql` (D1 table from PLG §7.1: id, event_name, tenant_id, user_id, session_id, properties JSON, created_at), `wrangler.toml`.
- `apps/analytics-worker/src/views/` — 7 saved SQL queries (one file each):
  1. `signup-funnel.sql` (landing_view → signup_started → signup_completed → first_cli_authed → first_cache_hit)
  2. `ttfv-cohort.sql` (median + p75 minutes signup → first_cache_hit by cohort day)
  3. `activation-d1-d7-d30.sql` (3 windows from PLG §7.2)
  4. `mrr-by-week.sql` (Stripe-derived)
  5. `plan-mix.sql` (free / pro / enterprise counts)
  6. `regional-latency-p99.sql` (cas-worker latency by CF colo)
  7. `weekly-three-numbers.sql` (the Monday email payload)
- `apps/analytics-worker/src/cron/weekly-email.ts` — runs `0 8 * * MON` UTC, executes `weekly-three-numbers.sql`, emails Gustavo via Resend.
- Plausible script tags:
  - `apps/docs/docusaurus.config.ts` → add `<script defer data-domain="corelink-docs.humangr.com" src="https://plausible.io/js/script.js">` via `headTags` (gated by Docusaurus consent plugin if present; else load only post-consent).
  - `apps/admin-ui/src/app/layout.tsx` → same, gated by existing cookie-consent `analytics` category.
- Wire ingest calls in 4 places (minimum viable):
  - `apps/admin-ui/src/app/sign-up/[[...sign-up]]/page.tsx` → fire `signup_started` on mount.
  - `apps/signup-worker/src/webhooks/clerk.ts` → fire `signup_completed`, `tenant_created`, `pat_issued` server-side.
  - `apps/cas-worker/src/middleware/analytics.ts` (Phase 0 emitter from F) → `first_cli_authed`, `first_cas_write`, `first_cache_hit`.
  - `apps/signup-worker/src/webhooks/stripe.ts` → `checkout_started`, `paid_subscription_started`.

**Acceptance.**
1. `wrangler d1 execute corelink-analytics --command="SELECT name FROM sqlite_master WHERE type='table'"` shows `analytics_events`.
2. POST `https://corelink-analytics.humangr.com/v1/event` with a test payload returns 200 + row appears in D1.
3. Each of 7 SQL files runs against D1 without error (validates schema match).
4. Plausible dashboard shows non-zero events on `corelink-docs.humangr.com` within 1 h of deploy.
5. Cron worker manually triggered emits an email to `gustavo@humangr.com` with the three numbers (even if all zeros pre-launch).
6. SEAL: codex ≥ 8.5/10; commit `feat(analytics): D1 ingest worker + 7 saved views + weekly email + Plausible install`.

**Hard-pause triggers.** (a) D1 binding name conflicts with existing wave's database → coordinate naming; (b) Plausible self-hosted vs Plausible Cloud cost decision unclear → default to Plausible Cloud Hobby ($9/mo) since metrics audit §7.4 budgets $7-25/mo; (c) Resend API rate-limited at free tier and weekly email exceeds → use Resend $20/mo if needed.

---

### §2.H `CORELINK-CLI-INSTALL-SCRIPT`

**Mandate.** Ship the CLI binary + install one-liner that F's `/welcome` pane expects. Without this, F's SSE pane never flips to "Connected" because there is no CLI to call `/v1/ping`.

**Files to write (new repo).**
- New repo: `HumanGuardrail/corelink-cli`. Rust workspace. `cargo new corelink` at root.
- `crates/corelink/src/main.rs` — clap-based CLI with `ping`, `bazel-init`, `buck2-init` (stub), `cargo-init` (stub), `config show` subcommands.
- `crates/corelink/src/commands/ping.rs` — reads `~/.corelink/config.toml`, HTTP GET to `${endpoint}/v1/ping` with `Authorization: Bearer ${token}`, prints "Cache reachable in {ms} ms" on 200, exits 0; non-zero on failure.
- `crates/corelink/src/commands/bazel_init.rs` — detects `WORKSPACE` or `MODULE.bazel` in CWD, appends idempotently to `.bazelrc`:
  ```
  # corelink-managed (do not edit between markers)
  build --remote_cache=${endpoint}
  build --remote_header=authorization=Bearer\ ${token}
  build --remote_upload_local_results=true
  # /corelink-managed
  ```
  Uses marker-block matching so re-run is no-op.
- `crates/corelink/src/config.rs` — read/write `~/.corelink/config.toml`.
- `.github/workflows/release.yml` — cross-compile to `linux-x86_64`, `linux-aarch64`, `darwin-x86_64`, `darwin-aarch64`, `windows-x86_64`; publish to GitHub Releases.

**Files to write (in this repo).**
- `apps/get-corelink-worker/` — new CF Worker on `get.corelink.io`. Serves a shell-script template:
  ```bash
  #!/bin/sh
  set -eu
  TOKEN=""; REGION=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --token=*) TOKEN="${1#--token=}";;
      --region=*) REGION="${1#--region=}";;
    esac; shift
  done
  [ -z "$TOKEN" ] && { echo "FATAL: --token required"; exit 2; }
  OS=$(uname -s | tr '[:upper:]' '[:lower:]')
  ARCH=$(uname -m)
  URL="https://github.com/HumanGuardrail/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}"
  curl -fsSL "$URL" -o /tmp/corelink
  chmod +x /tmp/corelink
  sudo mv /tmp/corelink /usr/local/bin/corelink
  mkdir -p ~/.corelink
  cat > ~/.corelink/config.toml <<EOF
  token = "$TOKEN"
  region = "${REGION:-auto}"
  endpoint = "https://corelink-api.humangr.com"
  EOF
  corelink ping
  echo "Next: cd into your Bazel repo, run: corelink bazel-init"
  ```
- `apps/get-corelink-worker/wrangler.toml` — bind to `get.corelink.io` custom domain.

**Acceptance.**
1. `cargo build --release` in the new repo succeeds for all 5 target triples (CI green).
2. `corelink ping` against a test token returns "Cache reachable in {N} ms" in ≤ 2 s.
3. `corelink bazel-init` run twice in a row leaves `.bazelrc` byte-identical (idempotency).
4. `curl -fsSL https://get.corelink.io | sh -s -- --token=$TEST_TOKEN --region=ord` end-to-end succeeds on Linux x86_64 + Darwin aarch64 (Gustavo manual test).
5. SEAL: codex ≥ 8.5/10; commits `chore(corelink-cli): bootstrap repo with ping + bazel-init + cross-compile CI` (new repo) and `feat(get-corelink-worker): install-script serving Worker on get.corelink.io` (this repo).

**Hard-pause triggers.** (a) `HumanGuardrail/corelink-cli` repo creation requires Gustavo GitHub-org-admin click → emit script for Gustavo + pause if not pre-created; (b) `get.corelink.io` DNS / CF custom-domain setup needs Gustavo CF-dashboard action → emit one-page runbook; (c) PAT scope `cas:rw` not yet defined in existing PAT issuance → coordinate with F before shipping CLI.

---

### §2.I (out of Phase 0 critical path — Gustavo async)

Tracked here so they don't get lost; **not** required for re-running launch-readiness to 5/5 GREEN.

| Item | Owner | Effort | Trigger |
|---|---|---|---|
| Stripe Atlas filing (Delaware C-corp) | Gustavo | ~30 min form + $500 | First paid customer in pipeline |
| Mercury account opening | Gustavo | ~20 min post-Atlas EIN | Post-Atlas |
| Termageddon $119/yr (auto-updating Privacy/Terms/Cookie) | Gustavo | ~1 h config | Replaces A-written stubs at first contract negotiation |
| Common Paper DPA published | Gustavo + A | ~1 h | Pre-first-enterprise-pilot signature |

These belong in Phase 1 of the launch roadmap, not Phase 0. Listed here to acknowledge they exist and the user mentioned them.

---

## §3 Dispatch sequence

**Day 0 — morning (T = 0).**

Dispatch in parallel (8 worktrees simultaneously, each on its own branch `worktree-agent-<sha>`):

| Slot | Agent | Why parallelizable |
|---|---|---|
| 1 | A `LEGAL-FOOTER-WIRE` | Touches only `apps/docs/`. |
| 2 | B `CF-PAGES-SECRETS-PREP` | Writes only `specs/_audits/*runbook.md`. |
| 3 | C `BILLINGSTEP-DELETE-WIRE-CHECKOUT` | Touches `apps/admin-ui/[locale]/onboarding/billing/`, `apps/admin-ui/api/checkout/`, `apps/signup-worker/webhooks/stripe.ts`. F also touches admin-ui but in disjoint subtree (`welcome/`, `onboarding/{tenant,region-plan,pat,done}/` — verify no shared layout.tsx edits at merge). |
| 4 | D `LANDING-CTA-HERO` | Touches only `apps/docs/docs/index.mdx` + new `apps/docs/src/components/Hero/`. |
| 5 | E `PRICING-UNSTUB-CHECKOUT-WIRE` | Touches only `apps/docs/src/{lib,pages}/pricing*`. |
| 6 | F `ONBOARDING-WIZARD-2STEP` | Touches `apps/admin-ui/[locale]/onboarding/`, `apps/admin-ui/[locale]/welcome/`, `apps/signup-worker/webhooks/clerk.ts`, `apps/cas-worker/middleware/analytics.ts`. **Coordinate with C** on `onboarding/` parent layout (merge order: C first since it only removes one subdir; F second). |
| 7 | G `METRICS-INSTRUMENT` | New worker `apps/analytics-worker/`. Adds Plausible tags in `apps/docs/docusaurus.config.ts` (D edits same file — merge order: G after D or hand-merge). Adds ingest calls F + C also touch — merge after F + C. |
| 8 | H `CORELINK-CLI-INSTALL-SCRIPT` | New repo (`HumanGuardrail/corelink-cli`) + new worker (`apps/get-corelink-worker/`). Zero overlap with anything else in this repo. |

**Day 0 — afternoon (T = +4h).**

Gustavo executes B-runbook (~30 min). This unblocks the prod L2 500s. Meanwhile agents A–H still running in parallel.

**Day 0 — evening (T = +8h).**

All 8 agents SEAL. Orchestrator merges in order:
1. H (new repo + new worker, zero conflict).
2. A (docs only).
3. D (docs only).
4. E (docs only — may conflict with A on `docusaurus.config.ts` redirects block; hand-merge if so).
5. G-Plausible (docusaurus.config.ts headTags — merge after A+D+E).
6. C (admin-ui billing removal).
7. F (admin-ui onboarding collapse — merge after C).
8. G-ingest-wires (touches files now in C+F-merged state — re-merge if needed).

**Day 1.**

Re-deploy `corelink-docs` + `corelink-admin-ui` + `corelink-signup` + new `corelink-analytics` + new `get-corelink-worker`. Gustavo re-runs B-secrets if Pages re-creates the project.

**Day 2.**

End-to-end smoke (manual, stopwatched):
1. Fresh GitHub account on incognito.
2. Visit `corelink-docs.humangr.com` → see hero + CTA.
3. Click CTA → Clerk sign-up via GitHub OAuth → ≤ 30 s.
4. Land on `/welcome` → see install one-liner + PAT + "Waiting…" badge.
5. Copy one-liner, paste in terminal → `corelink ping` returns < 2 s; badge flips to "Connected".
6. `cd ~/some-bazel-repo && corelink bazel-init && bazel build //... && bazel build //...` → second build shows cache hits.
7. `/welcome` badge flips to "Activated. Your build was {N}× faster."
8. Stopwatch — record TTFV. Compare to PLG framework §3.1 target (median ≤ 10 min).

Re-run `2026-05-27-launch-readiness-check.md` curl + footer-crawl + signup-happy-path → expect **5/5 GREEN**.

---

## §4 Gate criteria

Phase 0 ships when **all** of:

| # | Criterion | How verified | Source |
|---|---|---|---|
| 1 | `2026-05-27-launch-readiness-check.md` re-runs 5/5 GREEN | Re-execute audit §7 step 8 (curl smoke + footer crawl + signup happy-path) | launch-readiness §7.8 |
| 2 | Median TTFV < 15 min on first manual end-to-end test | Stopwatched §3 Day-2 smoke | PLG §3.1 |
| 3 | `first_cache_hit` analytics event fires for the orchestrator's own test signup | `SELECT * FROM analytics_events WHERE event_name='first_cache_hit'` returns ≥ 1 row | PLG §3.2 + G §7.1 |
| 4 | Three numbers email arrives at `gustavo@humangr.com` Monday 08:00 UTC | Manual inbox check on first Monday post-deploy | G §7.4 |
| 5 | All 8 agent worktrees SEALED with codex ≥ 8.5/10 | Per-WI SEAL doc in `specs/_audits/sealed/` | autonomous-execution-charter §SEAL |

If any of (1)–(5) fail, the failing item dispatches a **delta agent** to close the gap (no Phase 0 sign-off until 5/5).

---

## §5 Risks + mitigations

| Risk | Likelihood | Severity | Mitigation |
|---|---|---|---|
| C + F merge conflict in `apps/admin-ui/[locale]/onboarding/layout.tsx` (step-graph definition) | M | L | Per §3 merge order, C lands first; F rebases. |
| D + E + G all touch `apps/docs/docusaurus.config.ts` | M | L | Per §3 merge order; hand-merge headTags + redirects + footer in one pass. |
| H's `HumanGuardrail/corelink-cli` repo creation needs Gustavo GitHub-org-admin permission | H | M | H ships ready-to-bootstrap commit; if repo absent, agent emits script + pauses (hard-pause trigger). |
| F's CAS-data-plane emitter overlaps with Wave-33 Stream B (`crates/cas-worker/` refactor) | M | M | F edits `apps/cas-worker/middleware/analytics.ts` (current location); coordinate with Wave-33 orchestrator before touching. If Wave-33 already moved the file, rebase to new location. |
| B-execute delayed (Gustavo away) → L2 still 500s on Day 1 smoke | M | H | Phase 0 ships partial: §4 criterion #1 fails on L2 until B-execute lands; Phase 0 sign-off blocked but other agents not blocked. |
| Stripe `STRIPE_PRICE_PRO_MONTHLY` not provisioned → C upgrade flow returns 400 | M | M | C hard-pauses on missing env var; Gustavo creates product + price in ~10 min via Stripe dashboard. |
| `get.corelink.io` DNS not yet configured | H | M | H emits one-page runbook; Gustavo CF-dashboard click (~5 min). Fallback: install script served from `https://corelink-docs.humangr.com/install.sh` as interim. |
| Common Paper CSA template missing key clauses for B2B SaaS (e.g. uptime SLA) | L | L | A ships v1; Termageddon $119/yr (Phase 1) refines. |
| Plausible cookie-consent gate misfires in EU → no events captured | M | M | G defaults to PostHog Cloud EU starter as fallback (per metrics audit §7.4). |

---

## §6 Wall-clock summary (TL;DR)

- **T = 0 (Day 0 AM):** Dispatch 8 agents in parallel.
- **T = +4 h:** Gustavo executes B-runbook (~30 min).
- **T = +8 h (Day 0 EOD):** All 8 agents SEAL; orchestrator merges in dependency order.
- **T = +24 h (Day 1):** Re-deploy production surfaces.
- **T = +48 h (Day 2):** End-to-end stopwatched smoke; re-run launch-readiness.
- **T = +56 h (Day 2 EOD):** Phase 0 SEALED. 5/5 GREEN. Public launch unblocked.

Total: **~2.5 days wall-clock from dispatch to Phase 0 SEAL**, assuming Gustavo is available for the 30-min CF Pages secret window on Day 0.

---

## §7 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
