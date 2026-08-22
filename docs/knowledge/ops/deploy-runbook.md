---
type: "Runbook"
title: "Launch-day production deploy sequence (Phases D→H)"
description: "The operator runbook for cutting CoreLink to production: live secrets, D1 migrations, container build/canary, Pages/DNS, and the Worker cutover with auto-rollback."
source_files:
  - "docs/operator/launch-day-sequence-2026-06-09.md"
checkpoint_sha: "e5d8696d07beca20c0a3ff19cf0b7675044be375"
provenance: "AUTHORED"
tags: ["ops", "deploy", "launch", "cutover", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Launch-day production deploy sequence (Phases D→H)

This is the consolidated, verified-as-of-pre-flight runbook for taking CoreLink live: load the LIVE
Clerk/Stripe secrets the deploy gate hard-requires, apply the D1 migrations, build and canary the
container, confirm Pages/DNS, then deploy the Worker and run the cutover checklist — which auto-rolls-back
on a post-cutover smoke failure. It exists because the live keys and several owner-only steps are the
one thing automation cannot do, and getting their ordering wrong is how a launch stalls. Related: [the dev CF Worker deployment runbook](/ops/dev-deployment.md).

# Role

It is the single source of truth for the GA cutover ordering and the owner-only preconditions, so the
operator can execute Phases D→H deterministically and roll back at any point.

# How it works

- The operator sets the LIVE Clerk/Stripe keys in Cloudflare; the launch-day runbook records that `.env.local` holds TEST keys `docs/operator/launch-day-sequence-2026-06-09.md:9-15`. **CORRECTION 2026-08-22:** that was true when written, but `.env.local` now ALSO carries live credentials under `*_LIVE_*` names — `CLERK_LIVE_SECRET_KEY` (`sk_live_`), `CLERK_LIVE_PUBLISHABLE_KEY` (`pk_live_`), `STRIPE_LIVE_SECRET_KEY` (`rk_live_`, restricted), `STRIPE_LIVE_WEBHOOK_SECRET` — alongside the plainly-named test ones. Verified live: Stripe `GET /v1/prices` returns `livemode:true`; Clerk Backend API `GET /v1/instance` returns `environment_type: production`. Treat "`.env.local` is test-only" as FALSE: grep `^[A-Z_]*LIVE[A-Z_]*=` before assuming a live check is impossible, and redact on the variable NAME (a `whsec_` value has no mode infix and will print in full).
- Phase D1 pushes the MVP secret allowlist into Cloudflare and verifies deployment `docs/operator/launch-day-sequence-2026-06-09.md:24-29`.
- Phase D2 applies the D1 migrations (dry-run lists the real count, then `--apply`) `docs/operator/launch-day-sequence-2026-06-09.md:31-33`.
- Phase E builds, pre-push-scans, pushes, and 5%→100% canaries the container `docs/operator/launch-day-sequence-2026-06-09.md:35-39`.
- Phases F and G are verify-only for Pages and the flat-scheme DNS `docs/operator/launch-day-sequence-2026-06-09.md:41-45`.
- Phase H deploys the Worker, runs the 18-check smoke, then the 15-item cutover checklist `docs/operator/launch-day-sequence-2026-06-09.md:47-52`.

# Invariants

- The Worker deploy gate hard-requires the live Stripe + Clerk + PagerDuty secrets before it will deploy `docs/operator/launch-day-sequence-2026-06-09.md:12-15`.
- No `CORELINK_DPA_VERSION` key in Cloudflare means no signups — it is part of the required secret set `docs/operator/launch-day-sequence-2026-06-09.md:26-28`.
- The cutover auto-rolls-back on a failed post-cutover smoke `docs/operator/launch-day-sequence-2026-06-09.md:49-53`.

# Gotchas

- The customer "Manage billing" portal is a STUB returning a fake Stripe URL at launch — checkout + webhook provisioning are unaffected, but the self-service portal must be wired post-launch `docs/operator/launch-day-sequence-2026-06-09.md:73-75`.
- The Clerk CSP host is an AUTH-critical owner flag: if the Clerk Frontend-API host in the `admin-ui` CSP does not resolve, the sign-in widget is CSP-blocked `docs/operator/launch-day-sequence-2026-06-09.md:57-60`. **The live host is `clerk.corelink-app.humangr.com`** — it is baked into the `pk_live_` publishable key and hardcoded in `apps/admin-ui/src/lib/csp.ts:137,166,169` (`script-src` / `connect-src` / `frame-src`). Check THAT host when auth breaks. The launch-day flag named `clerk.corelink.humangr.com`, which is NXDOMAIN and was never the CSP host; that name is a launch-day-era error, corrected 2026-08-22. Note that `clerk.corelink-app.humangr.com` shares a parent name with the deliberately-retired `corelink-app.humangr.com` app subdomain but is a SEPARATE, auth-critical DNS record — never remove it.

# Citations

1. `docs/operator/launch-day-sequence-2026-06-09.md:9-15` — operator-only LIVE secret step.
2. `docs/operator/launch-day-sequence-2026-06-09.md:12-15` — the deploy gate's hard-required secrets.
3. `docs/operator/launch-day-sequence-2026-06-09.md:24-29` — Phase D1 secrets + verify.
4. `docs/operator/launch-day-sequence-2026-06-09.md:26-28` — no DPA key ⇒ no signups.
5. `docs/operator/launch-day-sequence-2026-06-09.md:31-33` — Phase D2 D1 migrations.
6. `docs/operator/launch-day-sequence-2026-06-09.md:35-39` — Phase E container build/canary.
7. `docs/operator/launch-day-sequence-2026-06-09.md:41-45` — Phases F/G Pages + DNS verify.
8. `docs/operator/launch-day-sequence-2026-06-09.md:47-52` — Phase H Worker deploy + cutover.
9. `docs/operator/launch-day-sequence-2026-06-09.md:49-53` — auto-rollback on cutover smoke fail.
10. `docs/operator/launch-day-sequence-2026-06-09.md:57-60` — Clerk CSP owner flag.
11. `docs/operator/launch-day-sequence-2026-06-09.md:73-75` — billing portal stub.
