# Launch uptime monitoring + alerting — status & activation (2026-06-18)

> READ-ONLY investigation deliverable. WP-D ("launch uptime monitoring + alerting").
> Scope frozen by tech lead. Every claim below is grounded in a `file:line` ref.
> No code/config was changed.

## TL;DR

There are **two separate things** in this repo, and the brief's premise (use
`corelink-synthetic-pager` as the uptime monitor) is **based on a name collision** —
`corelink-synthetic-pager` is **NOT an uptime monitor**. The actual launch uptime
monitor is a **Better Stack (a.k.a. BetterUptime) synthetic-probe layer** plus a set of
**scheduled GitHub-Actions E2E smokes that page PagerDuty on failure**.

| Layer | What it is | State for launch |
|---|---|---|
| **`corelink-synthetic-pager`** | A weekly **synthetic *page-drill*** (tests that on-call *acks* a page within 5-min MTTA). NOT a prod health check. | **Dormant skeleton** — pure-logic only; CF Worker + PagerDuty wiring explicitly deferred. Irrelevant to "is prod up". |
| **Better Stack probes** (`monitoring/synthetic/probes.yml`) | 7 HTTP uptime monitors hitting `/health` on every prod surface, 30–60s, 3 regions, + a public status page. | **Defined + dry-run-verified, but `--apply` NOT yet run** → monitors likely not live. Email-alert delivery needs a paid-tier upgrade. **This is the real launch uptime monitor.** |
| **Scheduled E2E smokes** (GitHub Actions) | Money-path + signup-path + PAT-auth probes against PROD every 6h; **page PagerDuty SEV-1 on failure**. | **Wired, gated on owner secrets.** Run only once the owner sets `STRIPE_*` / `CLERK_SECRET_KEY` / `PAGERDUTY_ROUTING_KEY` repo secrets. The deepest launch coverage we have. |

**Verdict on the named artifact:** the brief asks to activate `corelink-synthetic-pager`
as an uptime monitor — that is the **wrong artifact**; it's a page-ack drill, not a
health probe. The launch-ready uptime monitor is the **Better Stack probe layer**, and
its activation is a one-command owner step plus a tier decision. Detail below.

---

## 1. What exists

### 1a. `corelink-synthetic-pager` — a page-ACK drill, not an uptime monitor

- **Where:** originally crate `corelink-synthetic-pager`, **absorbed** into
  `corelink-telemetry` as an inline module — `crates/corelink-telemetry/src/synthetic_pager.rs:1`
  (`mod synthetic_pager;` declared at `crates/corelink-telemetry/src/lib.rs:88`).
- **What it IS:** a **weekly synthetic SEV-2 *page drill*** (WI-S20-006) that emits a
  fake page and **measures whether the on-call engineer acknowledges within a 5-minute
  MTTA budget** — `crates/corelink-telemetry/src/synthetic_pager.rs:1-11`. It is
  explicitly *"the pure-logic skeleton … plus the `DrillRecorder` trait surface every
  production CF Cron Worker adapter + PagerDuty receiver WILL satisfy"*
  (`synthetic_pager.rs:5-11`). It tests **people + paging plumbing**, not **prod
  health**.
- **What it checks:** nothing in prod. It emits to a *dedicated* `synthetic-drill`
  PagerDuty service (severity `sev2_synthetic`, isolated so it can never be confused
  with a real SEV-2) — `synthetic_pager.rs:21-24,68-71`; runbook
  `specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md:30-38`.
- **Cadence (intended):** weekly CF Cron `0 14 * * 1` (Mondays 14:00 UTC), region
  rotating on a 4-week cycle — `wrangler.toml:211-219`,
  `RB-SYNTHETIC-PAGE-DRILL.md:48-68`.
- **On "failure" (MTTA breach):** escalates PagerDuty Tier-2/Tier-3 + notifies owner;
  feeds a GA Evidence Gate (30-day acked streak) — `RB-SYNTHETIC-PAGE-DRILL.md:106-125`.
- **The SHA-pinned image** the brief references is the (future) CF Cron Worker for THIS
  drill: `ghcr.io/humangr-labs/corelink-synthetic-pager@sha256:<PIN_AT_RELEASE>` —
  `wrangler.toml:221-222`. The pin is a **placeholder** (`<PIN_AT_RELEASE>`), not a real
  digest.

### 1b. Better Stack synthetic probes — the real uptime monitor

- **Manifest:** `monitoring/synthetic/probes.yml` — **7 HTTP monitors**, version-controlled.
- **Apply tooling:** `scripts/apply-betterstack-probes.sh` (idempotent GET→PATCH/POST
  against `https://uptime.betterstack.com/api/v2/monitors`; dry-run by default,
  `--apply` is owner-only) — `scripts/apply-betterstack-probes.sh:1-17,24,232-311`.
- **What it probes + cadence** (`probes.yml`, mirrored in `monitoring/synthetic/README.md`):

  | # | Name | URL | Interval | Assertion |
  |---|---|---|---|---|
  | 1 | API health | `https://corelink-api.humangr.com/health` | **30s** | status 200 + JSON `$.status == "ok"` (`probes.yml:26-44`) |
  | 2 | App UI | `https://corelink-app.humangr.com` | 60s | 200 (`probes.yml:47-65`) |
  | 3 | Docs | `https://corelink-docs.humangr.com` | 60s | 200 (`probes.yml:68-86`) |
  | 4 | Signup health | `https://corelink-signup.humangr.com/health` | 60s | 200 (`probes.yml:91-110`) |
  | 5 | CLI install | `https://corelink-get.humangr.com` | 60s | 200 + `text/plain` (`probes.yml:113-134`) |
  | 6 | Admin health | `https://corelink-admin.humangr.com/health` | 60s | 200 (`probes.yml:137-156`) |
  | 7 | CLI get HEAD | `https://corelink-get.humangr.com` | 60s | 200 (HEAD) (`probes.yml:159-178`) |

  All probes run from 3 regions (`us-east`, `eu`, `ap`); `confirmation_period = 0`
  (alert on first failure). The `/health` target is real: the prod Worker serves
  `GET /health` → `{"status":"ok",...}` (`worker/src/index.ts:449`) and the container
  serves a readiness `GET /_health` (`crates/corelink-container/src/main.rs:8,87`).
- **Status page:** Better Stack page `247652`, custom domain
  `https://status.corelink.humangr.com` (DNS-live: CNAME → `hugrl.betteruptime.com`,
  `specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md:13-16`).
- **Delivery / alerting:** Better Stack monitors page/alert on their own; the
  **status-page email-subscribe channel requires a Better Stack paid-tier upgrade** —
  free tier does NOT support it (`…phaseA-betterstack-live.md:24`,
  "Subscribable … requires BetterStack paid tier upgrade … ~US$25-30/mo").
- **Token:** `BETTERSTACK_API_TOKEN`, read from env only (`apply-betterstack-probes.sh:15,232-238`).
  Related deployed secret: `STATUSPAGE_URL` for `corelink-statuspage-real`
  (`docs/internal/secrets-checklist.md:195`).

### 1c. Scheduled prod E2E smokes — the deepest launch coverage (these DO page)

These are GitHub-Actions cron jobs that exercise **real launch-critical user journeys**
against PROD and **page PagerDuty SEV-1 on failure** (Events API v2,
`PAGERDUTY_ROUTING_KEY`). They are gated behind a "secret present?" job so they
**skip (not fail)** until the owner sets the launch-day secrets.

- **Money path** — `.github/workflows/e2e-stripe-checkout.yml`: every 6h (`cron 30 */6 * * *`,
  line 41); drives `scripts/e2e-stripe-checkout.sh` (create Stripe sub → POST signed
  `checkout.session.completed` to `corelink-signup.humangr.com/webhooks/stripe` → poll
  D1 `tenant_billing` for `status=paid`). On fail → opens `sev-1` issue
  (`e2e-stripe-checkout.yml:157-187`) **+ PagerDuty SEV-1 page** (lines 189-231). Gated on
  `STRIPE_SECRET_KEY` + `STRIPE_WEBHOOK_SECRET` (lines 62-83).
- **Signup + PAT-auth path** — `.github/workflows/e2e-clerk-signup.yml`: every 6h
  (`cron 0 */6 * * *`, line 31); creates a real Clerk user → polls D1 tenant+PAT →
  exercises `GET /v1/users/me` with the minted PAT. On fail → issue + **PagerDuty page**.
  Gated on `CLERK_SECRET_KEY`.
- **App/docs/install deploy-regression** — `.github/workflows/e2e-prod.yml`: daily
  (`cron 0 7 * * *`, line 30); Playwright against the live app/docs/install URLs. **No
  PagerDuty page** — failure surfaces only as a red workflow + uploaded report
  (`e2e-prod.yml:80-86`).
- **Audit-chain integrity** — `.github/workflows/audit-chain-daily-verify.yml`: pages a
  **SEV-0** on a per-day chain break (lines 217-265).

PagerDuty schedule/escalation IaC exists at `infra/pagerduty/schedule.yaml` (3-tier
follow-the-sun rotation), applied via the Terraform PagerDuty provider at the PRR ship
gate.

---

## 2. State — with evidence

- **`corelink-synthetic-pager`: DORMANT skeleton.** Ships *"without Cloudflare Worker
  secrets bound to `PAGERDUTY_API_KEY` + `SYNTHETIC_PAGE_ROUTING_KEY`"*; the cron is
  *"trait + fake here, real CF Cron + PagerDuty in PRR ship gate"*
  (`synthetic_pager.rs:46-56`). Production wiring (the CF Cron, the Events-API POST, the
  ack webhook receiver, D1 migration apply) is an explicit **deferred** list
  (`synthetic_pager.rs:73-82`). The wrangler image pin is a literal `<PIN_AT_RELEASE>`
  placeholder (`wrangler.toml:221-222`). The CLI (`corelink-synthetic-pager-cli`) is
  *"deferred to PRR ship gate"* (`RB-SYNTHETIC-PAGE-DRILL.md:147-149`). → **Not live; not
  needed for launch uptime.**
- **Better Stack probes: PARTIAL — defined + dry-run-verified, `--apply` NOT yet run.**
  The probe manifest + apply script are **CREATED** and dry-run-clean
  (`specs/_audits/2026-05-27-synthetic-monitoring-seal.md:51,99`), but live apply is
  flagged as the *"first live apply"* still to come (`…synthetic-monitoring-seal.md:139`)
  and live-apply is an explicit **owner-only** step (`apply-betterstack-probes.sh:6`,
  README "Live apply (Owner-only)"). Phase-A SEAL deferred the component dashboards +
  monitor creation to "Phase H … *after* container deploy"
  (`…phaseA-betterstack-live.md:18-27`). I found **no evidence in-repo that `--apply` has
  been run** (no apply log/SEAL recording created monitor IDs). → **Treat the 7 monitors
  as NOT yet live until the owner confirms.**
- **Scheduled E2E smokes: WIRED but GATED.** All three are committed workflows with cron
  triggers, but each is behind a secret-presence gate that **skips** until the owner sets
  launch-day secrets (`e2e-stripe-checkout.yml:62-83`, `e2e-clerk-signup.yml` gate). The
  Stripe + Clerk smokes are the only monitors that page PagerDuty AND probe the money +
  signup paths. → **Active the moment the owner sets the secrets; today they self-skip.**

---

## 3. Gaps for launch

### Coverage gaps

1. **No cache read/write probe.** The whole product is a content-addressable cache
   (native CAS/AC, Bazel REAPI, Turborepo, sccache). **None** of the 7 Better Stack
   probes nor any scheduled E2E exercises an authenticated **cache PUT→GET round-trip**.
   `/health` returning `{"status":"ok"}` (`worker/src/index.ts:449`) does **not** prove
   the cache plane works (R2 binding, PAT verify, tenant isolation, the
   InMemory-fallback DATA-LOSS trap called out at `cf-deploy-prod.yml:113-117`). **This
   is the biggest "we'd-NOT-know-within-minutes" gap.**
2. **No exchange/token-exchange probe.** The githugr token-exchange + tenant-lookup
   internal-auth endpoints (per MEMORY) are not probed.
3. **`/health` is shallow.** It's a liveness ping, not a readiness deep-probe. The
   container exposes a richer `/_health` surfacing `storage = r2 | inmemory`
   (`worker/src/index.ts:490-492`, `crates/corelink-container/src/main.rs:110`), but **no
   monitor asserts `storage == r2`** — so a silent fall-back to InMemory (data loss)
   would NOT page.
4. **App-UI probe is shallow.** Probe 2 only asserts the Clerk-gated app root returns
   200; it does not catch a broken `/sign-up` (the exact class of regression
   `e2e-prod.yml:3-5` was built for — but `e2e-prod` does not page).

### Alert-delivery gaps (the "unverified paging" issue, confirmed + corrected)

- **Correction to the prior note.** MEMORY/`post-golive-alerting-todo` said *"PagerDuty
  paging delivery is UNVERIFIED (owner has no mobile app) → add an email notification
  rule + re-test."* That is **still accurate and still open.** The repo's primary alert
  transport for the load-bearing smokes is **PagerDuty Events v2**
  (`e2e-stripe-checkout.yml:189-231`, `e2e-clerk-signup.yml`, `audit-chain-daily-verify.yml`),
  and PagerDuty delivery to the owner has **not been verified to actually land** — the
  owner has no PD mobile app, and there is no email fallback rule wired.
- **Slack is NOT a fallback today.** `SLACK_WEBHOOK_URL_*` are pre-procured but
  *"not yet wired into a code consumer"* (`docs/internal/secrets-checklist.md:281`); the
  prod deploy explicitly does **not** gate on them and routes alerting via PagerDuty
  (`.github/workflows/cf-deploy-prod.yml:96-100`).
- **Better Stack email-subscribe needs paid tier.** Status-page email subscriptions are
  a paid-tier feature (`…phaseA-betterstack-live.md:24`) — so even the uptime layer has
  no zero-cost email alert path today.
- **Net:** at launch, if prod breaks, the *only* delivery is a PagerDuty page the owner
  may never see. **An email (or SMS) alert rule + a verified test-page that LANDS is the
  single most important launch alerting action.**

---

## 4. Activation path (what the owner executes / approves at launch)

Ordered, minimal, and split into **owner-gated** (secrets / external dashboards / a
verified test page) vs **landable** (small in-repo config/code a contributor can PR).

### A. OWNER-GATED (must be done by the owner — cannot be done in-repo)

1. **[Better Stack] Run the live apply** (turns on the 7 uptime monitors):
   ```bash
   source .env.local            # provides BETTERSTACK_API_TOKEN
   bash scripts/apply-betterstack-probes.sh --apply
   ```
   Then confirm 7 monitors exist + green on page `247652` /
   `https://status.corelink.humangr.com`. (Owner-only per
   `apply-betterstack-probes.sh:6`.)
2. **[PagerDuty] Verify a page actually LANDS + add an email/SMS fallback.** Set the
   `PAGERDUTY_ROUTING_KEY` repo secret, then fire a manual page and confirm the owner
   receives it on a channel they actually monitor. **Add a PagerDuty notification rule
   that emails (and/or SMS via the already-procured Twilio creds,
   `secrets-checklist.md:271-272`) the owner**, since there is no PD mobile app. This
   directly closes the long-standing "unverified paging" item.
3. **[GitHub Actions secrets] Enable the deep E2E smokes** by setting the repo secrets
   the gates wait on — `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `CLERK_SECRET_KEY`,
   `CLOUDFLARE_API_TOKEN`, `PAGERDUTY_ROUTING_KEY`
   (`e2e-stripe-checkout.yml:25-29`, `e2e-clerk-signup.yml` header). The money-path +
   signup smokes then run every 6h automatically and page on failure.
4. **[Tier decision] Optionally upgrade Better Stack** (~$25-30/mo) to unlock status-page
   email subscriptions (`…phaseA-betterstack-live.md:24`) — or rely on PagerDuty email
   from step 2 and skip this.

### B. LANDABLE (small in-repo change a contributor can PR — owner just reviews)

5. **Add a cache read/write uptime probe** (closes the #1 coverage gap). Two options,
   pick one:
   - *Better Stack:* add an 8th probe to `monitoring/synthetic/probes.yml` doing an
     authenticated CAS/AC `PUT` then `GET` of a tiny fixed blob — **but** Better Stack
     probes are single-request, so a true round-trip needs option (b).
   - *GitHub-Actions smoke (recommended):* add `.github/workflows/e2e-cache.yml`
     (mirroring `e2e-stripe-checkout.yml`'s gate + PagerDuty-page structure) that uses a
     dedicated test-tenant PAT to PUT a blob to `corelink-api.humangr.com`, GET it back,
     assert the digest, and **page on mismatch**. This is the only thing that proves the
     core product works end-to-end.
6. **Deepen the API health assertion to catch the InMemory data-loss trap.** Add a probe
   (or extend the cache smoke) that hits the container deep-probe and asserts
   `storage == "r2"` (the field is already exposed —
   `crates/corelink-container/src/main.rs:110`, `worker/src/index.ts:490-492`).
7. **Make `e2e-prod` page on `/sign-up` regression**, or fold a signup-page check into
   the existing Clerk smoke, so a broken sign-up flow pages instead of just reddening a
   workflow (`e2e-prod.yml:80-86`).

> Note: This brief is READ-ONLY — none of B above was implemented; they are design-only
> recommendations for follow-up PRs.

---

## 5. Risks / unknowns (where the owner must confirm)

- **Did `--apply` ever run?** I found no in-repo apply log or SEAL recording created
  Better Stack monitor IDs. **The owner must check the Better Stack dashboard (page
  `247652`) to confirm whether the 7 monitors are live or still un-applied.** I'm
  treating them as NOT live.
- **Does a PagerDuty page actually reach the owner?** Unverified in-repo and historically
  flagged unverified (no PD mobile app). Must be tested live before relying on it for
  launch.
- **Are the launch-day secrets set in GitHub Actions?** Cannot be read from the repo
  (CF/GH secrets are write-only). If unset, all three deep E2E smokes are silently
  skipping right now.
- **`status.corelink.humangr.com` TLS** was "in flight" at Phase-A SEAL time
  (`…phaseA-betterstack-live.md:42-47`) — owner should re-confirm the cert issued.
- **Name-collision risk going forward:** future work must not conflate
  `corelink-synthetic-pager` (page-ack drill) with the Better Stack uptime probes — they
  are unrelated systems that both say "synthetic".
