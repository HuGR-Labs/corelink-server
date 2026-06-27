---
type: "Runbook"
title: "Launch-day production deploy sequence (Phases D→H)"
description: "The operator runbook for cutting CoreLink to production: live secrets, D1 migrations, container build/canary, Pages/DNS, and the Worker cutover with auto-rollback."
source_files:
  - "docs/operator/launch-day-sequence-2026-06-09.md"
  - ".github/workflows/cf-deploy-prod.yml"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
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

- Only the operator can set the LIVE Clerk/Stripe keys; `.env.local` holds TEST keys only `docs/operator/launch-day-sequence-2026-06-09.md:9-15`.
- Phase D1 pushes the MVP secret allowlist into Cloudflare and verifies deployment `docs/operator/launch-day-sequence-2026-06-09.md:24-29`.
- Phase D2 applies the D1 migrations (dry-run lists the real count, then `--apply`) `docs/operator/launch-day-sequence-2026-06-09.md:31-33`.
- Phase E builds, pre-push-scans, pushes, and 5%→100% canaries the container `docs/operator/launch-day-sequence-2026-06-09.md:35-39`.
- Phases F and G are verify-only for Pages and the flat-scheme DNS `docs/operator/launch-day-sequence-2026-06-09.md:41-45`.
- Phase H deploys the Worker, runs the 18-check smoke, then the 15-item cutover checklist `docs/operator/launch-day-sequence-2026-06-09.md:47-52`.

# Invariants

- The Worker deploy gate hard-requires the live secrets before it will deploy: the `cf-deploy-prod`
  workflow has a `gate — CF secrets populated` job that lists CF Worker secrets and FAILS the deploy
  if a required secret is missing (`.github/workflows/cf-deploy-prod.yml:71-120`), preceded by a
  secrets-checklist drift gate (`.github/workflows/cf-deploy-prod.yml:44-66`). NOTE: the live
  Stripe/Clerk values themselves are an operator-only manual step (`.env.local` holds test keys), and
  Slack webhooks are intentionally NOT gated (alerting routes via the gated `PAGERDUTY_ROUTING_KEY`)
  — so this is a process+CI claim, with the CI half enforced at the cited workflow lines
  `docs/operator/launch-day-sequence-2026-06-09.md:12-15`.
- No `CORELINK_DPA_VERSION` key in Cloudflare means no signups — it is part of the required secret set `docs/operator/launch-day-sequence-2026-06-09.md:26-28`.
- The cutover auto-rolls-back on a failed post-cutover smoke `docs/operator/launch-day-sequence-2026-06-09.md:49-53`.

# Gotchas

- The customer "Manage billing" portal is a STUB returning a fake Stripe URL at launch — checkout + webhook provisioning are unaffected, but the self-service portal must be wired post-launch `docs/operator/launch-day-sequence-2026-06-09.md:63-65`.
- The Clerk CSP host is an AUTH-critical owner flag: if `clerk.corelink.humangr.com` does not resolve, the sign-in widget is CSP-blocked `docs/operator/launch-day-sequence-2026-06-09.md:57-60`.

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
11. `docs/operator/launch-day-sequence-2026-06-09.md:63-65` — billing portal stub.
12. `.github/workflows/cf-deploy-prod.yml:71-120` — the CI deploy gate: `gate — CF secrets populated` lists CF Worker secrets and fails the deploy if any `REQUIRED` secret is missing.
13. `.github/workflows/cf-deploy-prod.yml:44-66` — the preceding secrets-checklist drift gate (`scripts/secrets-checklist-verify.sh`).

> Process note: the remaining invariants here (auto-rollback on a failed post-cutover smoke; "no `CORELINK_DPA_VERSION` ⇒ no signups"; the operator-only LIVE Stripe/Clerk secret step) are **runbook/process statements** executed by the launch-day sequence + the operator, not single-line code enforcers — the only current-CI enforcer in scope is the `cf-deploy-prod` secrets gate cited above.
