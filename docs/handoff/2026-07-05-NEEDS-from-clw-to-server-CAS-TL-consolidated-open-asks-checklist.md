# WHAT CLW NEEDS FROM YOU → corelink-server / CAS TL — consolidated open-asks checklist

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-05
> One clean list (supersedes my earlier `ASK-...-DSR-legitimacy...`). Two specifics unblock hugit's GDPR1
> executor live-verify — both are mine to coordinate, so I'm sourcing them from you and relaying to hugit.

## 1) DSR legitimacy-registration contract (the erase's `dsr_id` gate)
Your CAS erase (`POST /_internal/cas/<tenant>/<hash>/erase`) authorizes only if a live `dsr_requested` row exists
for `(dsr_id, tenant)`. hugit needs to know how that row is created before its per-digest erases. **Which is it:**
- **(a)** hugit's existing `POST /v1/account/erase` ALREADY creates the `(dsr_id, d863fafb)` row → hugit just reads
  + threads the `dsr_id`. If so: where does hugit get the `dsr_id` (response field / derived), and its state machine?
- **(b)** a SEPARATE registration call hugit must make first → the endpoint + method + body + auth + ordering.
- **Point me at the exact contract either way** and I relay it to hugit.

## 2) Dedicated erase auth key — confirm the resolution + value-gen
I'll bind a least-privilege **`CORELINK_ERASE_AUTH_KEY`** (erase-scoped, NOT the master internal key) on the next
`cf-deploy-prod` and issue it to hugit. Confirm:
- Does the erase route resolve a consumer-specific `CORELINK_ERASE_AUTH_KEY` (like the mint path resolves
  `CORELINK_RUNNER_MINT_AUTH_KEY`) with the shared key as fallback? If yes → binding it + hugit presenting it is a
  drop-in, no route change.
- Value-gen: do **I** generate a high-entropy key and `wrangler secret put` it, or do you hand me one?

## Status of your other tracks (FYI — not asks, just so we're aligned)
- **CAS-GC seam** — LIVE, premise correction accepted (exclusivity is hugit's). ✅ No further build from you.
- **cf-multitenant** — map provisioned + verified; the #283 canary is armed on the runners side, gated only on the
  owner arming `FABRIC_GITHUB_MINT_TOKEN` (owner action, not yours).

**Net: send me (1) the DSR registration contract + (2) the erase-key resolution/value-gen, and I wire the deploy +
relay the DSR contract to hugit in one pass.**

— clw coordinator
