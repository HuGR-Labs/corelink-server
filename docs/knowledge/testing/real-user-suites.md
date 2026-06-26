---
type: "TestStrategy"
title: "Real-user black-box suites & remaining journeys"
description: "The state of CoreLink's real-user e2e coverage vs prod (real-client moat + black-box journey suite) and the three gated journeys that need an owner resource to finish."
source_files:
  - "docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["testing", "e2e", "real-user", "black-box", "go-live"]
timestamp: "2026-06-26T00:00:00Z"
---

# Real-user black-box suites & remaining journeys

CoreLink's go-live certificate is two black-box suites that sign up as a real user against prod and
exercise the product the way a customer would: the real-client moat (`run.sh`) and the black-box
journey suite (`provision-and-run-suite.sh`). This concept records their validated GREEN state and —
more importantly — the three journeys that remain GATED because each needs a resource that can't be
faithfully or safely automated from the CLI (a browser Clerk session, a real Stripe charge, a
near-limit account). It is the operational finish-checklist companion to the
[e2e strategy](/testing/e2e-strategy.md) gap map and the
[gate machinery](/testing/gate-machinery.md) quality audit.

# Role

It tells an operator exactly what is proven against prod today, what is still dark, and the precise
env/steps to close each remaining journey — distinguishing genuine owner steps (real money / browser
session) from engineering gaps.

# How it works

- The real-client moat suite (docker login+push+pull, cargo+sccache, brew, bazel, turbo, native
  CAS/AC) is validated 19 PASS / 0 FAIL → SHIP against prod (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:34-36`).
- The black-box journey suite is 77 PASS / 0 FAIL / 24 GATED → GREEN, covering the security matrix
  (cross-tenant isolation, real revocation, scope, tenant-path spoof, anon, native-plane forgery,
  cache-poison, mint-abuse), error/edge, concurrency, and rate-limit (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:37-41`).
- The full real credential lifecycle is exercised through the customer path: signup → tenant + PAT →
  `keys.create` (read-only) → revoke, with `pat.pat_id` (nested) as the revoke handle
  (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:42-44`).
- Journey 3 (quota hard-cap) was VALIDATED un-gated: a throwaway tenant with a tiny `$`-ceiling
  returned a clean 402 on native CAS PUT (ADR-0068), confirming the `tenant_quota` `$`-ceiling is the
  correct per-tenant lever (not the tier-driven byte cap) (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:4-11`).
- Journey 2 (Stripe webhook → tier) is 90%: the forgery half is proven live (wrong secret → 401,
  correct LIVE secret → signature accepted), but the clean tier-FLIP needs one real charge or a
  test-mode staging (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:12-22`).
- Journey 1 (DSR-delete + checkout) is still gated on a browser Clerk session JWT, because the prod
  Clerk anti-fraud `needs_client_trust` blocks server-side session minting — a Clerk feature, not a
  CoreLink gap (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:48-69`).

# Invariants

- Both suites self-provision real signup users and DSR-delete them on exit — the go-live cert is run,
  not asserted from memory (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:100-105`).
- The quota hard-cap enforces a clean 402/429 (never silent overage, never 5xx) when a tenant is over
  its `$`-ceiling (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:4-11`).
- The Stripe webhook forgery defense holds: an unsigned/wrong-secret event is rejected (401/400)
  before any state write (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:12-22`).

# Gotchas

- The two open journey-halves are genuinely OWNER steps (a real card charge; a ~60-second browser
  session-JWT capture that expires in ~60s), not engineering gaps — capture the JWT fresh right
  before the run (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:58-68`).
- Prod's live webhook secret is `STRIPE_LIVE_WEBHOOK_SECRET` (whsec_tT…), NOT the `.env.local`
  `STRIPE_WEBHOOK_SECRET` (whsec_1M…, stale/mismatched) — using the wrong one looks like a token
  mismatch (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:20-22`).
- The per-tenant quota lever is `tenant_quota` ($-ceiling), NOT `tenant_storage_state.bytes_quota` —
  the byte cap is re-seeded from the tier on every write, so it is tier-driven, not per-tenant
  settable (`docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:8-11`).

# Citations

1. `docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:4-11` — journey 3 (quota hard-cap) validated; the `$`-ceiling is the right lever.
2. `docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:12-22` — journey 2 (webhook → tier) 90%; forgery proven, flip needs a charge; the live secret name.
3. `docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:34-44` — validated state: real-client moat 19 PASS, black-box 77 PASS/24 GATED, credential lifecycle.
4. `docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:48-69` — journey 1 (DSR + checkout) gated on a browser Clerk session.
5. `docs/testing/2026-06-22-remaining-e2e-journeys-handoff.md:100-105` — how to re-run the green baseline (self-provision + DSR-delete on exit).
