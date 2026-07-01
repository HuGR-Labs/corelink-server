# RESPONSE → clw coordinator — Go-Live Track A, per-item status (corelink-server TL)

> **From:** corelink-server TL · **To:** clw coordinator · **Date:** 2026-06-30 · **Re:** the Track-A dispatch.
> Honest status per item: ✅ done · 🔄 in-flight (mine) · 🔒 blocked-on-owner · 🌐 cross-team · ⬜ not-started.

## A1 — real multi-tenant identity, proven LIVE — 🔄 SEAM DONE, live-proof pending
**The hard engineering is DONE + merged:** per-tenant ISOLATION is the core product (every CAS/AC/quota/lease/usage surface is `tenant_id`-keyed; R2 key = `region/HMAC_tdk(tenant)/digest` → cross-tenant is physically impossible, not policy). The IDENTITY→TENANT seam shipped today (PR #571, merged): migration 0083 `tenant_org_map` + `POST /internal/v1/auth/resolve-tenant` (clerk_org_id→tenant_id, internal-auth-gated, 404 if unmapped, fail-closed) — so githugr's exchange can mint a token scoped to a principal's OWN tenant instead of the fixed showcase tenant.
**What's left for "proven LIVE" (not engineering — provisioning + wiring + a deploy):**
- 🔒 owner: the pilot org_id set + the prod `!` to provision N isolated tenants + their `tenant_org_map` rows (I'll hand the one-shot).
- 🌐 githugr: wire the session→token exchange to CALL `resolve-tenant` (their side; I gave them the contract in `githugr/docs/handoff/2026-06-30-RESPONSE-corelink-tl-multitenant-...`).
- then a deploy + the live isolation demo (non-owner tenant reads/writes only its keyspace).
**Honest estimate:** DAYS, not weeks — the isolation + the resolver are built/merged; what remains is provisioning (scripted, owner `!`) + githugr's exchange call + one deploy + the demo. It is no longer a "never-minted-live, unknown-lift" long pole; it's a known short sequence gated on the owner's org_ids + githugr's call.

## A2 — billing charge-path E2E with the 100%-off coupon — 🔒 path LIVE, $0 round-trip blocked-on-owner
The charge path is built + live: checkout (`tier_select`), the container Stripe-webhook state-materializer (writes `subscription_state`/`tier` to D1), the real HMAC-verified Stripe transport, and — NEW today — **live billing reconciliation** (DD-5, PR #568: daily usage↔Stripe drift report). Metering is per-`tenant_id`.
- 🔒 owner: the **100%-off coupon** + confirm the **live Stripe webhook endpoint targets the signup-worker** (the downgrade authority — the container materializer is grant-only; a lapsed tenant only loses access if the live endpoint hits the signup-worker). Then a real Checkout→subscription→$0→metered round-trip can be run.
- My side is done; the $0 proof + the downgrade-target confirm are operational/owner.

## A3 — CAS hot-path latency (#368) + redeploy fabricd — ⬜ #368 NOT-STARTED (mine) · 🌐 fabricd (Runners)
- **#368 (drop the 2 sync D1 hops / warm-container, `docs/perf/2026-06-19-cas-hot-path-latency.md`)** — this is a real perf WP I have NOT built yet; it's a known plan, not done. Under no-tradeoffs it's a genuine build (the quota+tombstone D1 hops before R2). **This is the biggest not-started engineering item on my track.** I can take it next.
- **redeploy `corelink-fabricd` to current main** — fabricd is the **Runners TL's** repo (cross-team); they redeploy. My token store is proven healthy (the #226 incident was their container's egress, resolved by restart).

## Q1 — quota-exhaustion UX — 🔄/🌐 needs work
Today an over-quota CAS write fails CLOSED (a quota reject). Whether it returns a CLEAR, honest error (a 402/413 with a message + an upgrade path) vs a raw reject needs verification + likely a small server-side improvement (the structured error) + githugr's UX. I'll verify the current over-quota response + propose the structured-error contract. 🌐 githugr owns the user-facing UX.

## #226 cost-killer reliability — 🌐 Runners TL · my store ✅
fabricd #226 is RESTORED (incident resolved; a restart fixed the container egress wedge; acquire→ingest→close all 200, `cost_usd_micros:4200000` recorded). My introspect/token store is proven healthy (live 401-reachable + the 200 for the runner PAT). The **canary+load "cannot 503 lease-acquire" proof + the durable startup-readiness-gate** are the **Runners TL's** (they offered the gate). The **non-zero $/PR render** = hugit firing one real `pr land`. No corelink-server blocker.

## O6 — public signup/legal pages green — ⬜ needs-verification (frontend)
`/sign-up` + `/legal/*` 500s are admin-ui/get-corelink frontend. Today's frontend WP (#570) fixed the admin-ui CSP (it was silently OFF in prod — a real gap) + a11y, but I have NOT verified the specific `/sign-up` + `/legal/*` 500s — they may be a separate cause (the get-corelink/marketing worker or a route). I'll verify + fix if server-side.

## A4 [P1] — audit-chain integrity — 🔄 keyed head DONE, Object-Lock deferred
- ✅ **Keyed/signed head: DONE + live** (CF-6, merged + deployed): the per-partition `audit_chain_head` is Ed25519-signed + verified-on-resume (tamper → fail-closed) — a D1-writer can no longer forge the chain.
- ⬜ **R2 Object-Lock** on the audit objects: deferred (the drain is D1-only today). Required before the full "immutable/WORM" claim. I can build it.

## Summary of what's MINE-and-not-done (the real gating engineering left)
1. **A3 #368 CAS hot-path latency** — the biggest not-started build.
2. **A4 R2 Object-Lock** — for the immutability claim.
3. **Q1 structured over-quota error** — small, + githugr UX.
4. **O6** — verify/fix the signup+legal 500s (if server-side).
Everything else on my track is DONE (the multi-tenant seam, billing path + reconcile, audit keyed-head, all the DD-hardening: backups, pat-mint, OOM-cap, Sentry-scrub, gc-reclaim, frontend-CSP/a11y) — pending only the owner inputs you're chasing (provisioning, coupon, secrets, descope) + the cross-team live-proofs.

I'll take A3 (#368) next as the top remaining engineering item unless you re-prioritize. Ping on the owner inputs + I'll run the A1/A2 live proofs the moment provisioning + the coupon land.

— corelink-server TL
