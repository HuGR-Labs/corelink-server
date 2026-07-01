# DISPATCH → corelink-server TL — Go-Live Track A (the multi-tenant + billing spine)

> **From:** clw TL (cross-team go-live coordinator) · **To:** corelink-server TL · **Date:** 2026-06-30
> **Master:** `corelink-workspaces/docs/GO-LIVE-STACK-ROADMAP-2026-06-30.md` (dual-audited) + `…/GO-LIVE-DISPATCH-2026-06-30.md`.
> Reply in this `docs/handoff/` folder; I sweep it.

## ⛔ Scope (owner override, 2026-06-30) — NO tradeoffs / gambiarras
The user is an **arbitrary real user**; deliver **the complete product, exactly as promised, 100% working**.
Every item below is **launch-gating** and ships **done-properly or not at all**. No "defer to fast-follow".

## Your P0 track
1. **A1 — real multi-tenant identity, proven LIVE.** A real Clerk JWT → tenant-token minted end-to-end; per-org
   isolation proven on **live** data (not just TLA+). Exit: a non-owner tenant reads/writes only its own keyspace,
   demonstrated live. *(Never minted live today — this is the long pole.)*
2. **A2 — billing charge-path E2E with the 100%-off coupon.** Checkout → subscription → coupon → **$0 invoice**
   → metered usage recorded; confirm the **Stripe webhook → downgrade authority** (lapsed tenant loses access).
   Exit: a real Checkout→subscription→$0 round-trip proven; metering correct.
3. **A3 — kill the CAS hot-path latency + redeploy.** Ship the #368 WPs (drop the 2 sync D1 hops / warm-container,
   `docs/perf/2026-06-19-cas-hot-path-latency.md`) and **redeploy `corelink-fabricd` to current `main`** (carries
   the audit P0 fix + #228 introspect instrumentation). Exit: warm path fast; no fail-closed 503 in the user's session.
4. **Q1 (with githugr) — quota-exhaustion UX.** The coupon zeroes the invoice but does NOT raise the free-tier
   quota. A real project exceeding storage/vCPU mid-CI must get a **clear, honest error + a path**, NOT a silent
   fail-closed CAS reject. Exit: hitting the quota mid-run produces an understandable UX, not a dead session.
5. **#226 cost-killer — make it WORK reliably (it's the differentiator).** The #226 deploy already regressed
   `/readyz` + `/v1/leases` to 503 (incident `2026-06-29-…fabricd-226-…introspect-503`). Under no-tradeoffs it
   can't be flag-off if the decomposed $/PR is promised. Exit: regression fixed at the root; **proven by
   canary+load it CANNOT 503 lease-acquire**; non-zero $/PR renders.
6. **O6-signup — public signup/legal pages green.** `/sign-up` + `/legal/*` were 500ing. Exit: smoke green.
7. **A4 [P1, still gating] — audit-chain integrity.** R2 Object-Lock + keyed head before any immutability claim.

## Owner-pending inputs you depend on (I'm chasing them)
PD routing key + Logpush (O1), backup secrets (O2), the 100%-off coupon + webhook target (O5), enterprise-claims
descope (O3). Also: **confirm operationally** A1/A2/O1/O2 — my audit flagged these as your self-assessments,
unverifiable from a read-only checkout.

## What I need back
A per-item status (done / in-flight / blocked-on-owner) + your honest estimate on A1 (the identity long pole).
