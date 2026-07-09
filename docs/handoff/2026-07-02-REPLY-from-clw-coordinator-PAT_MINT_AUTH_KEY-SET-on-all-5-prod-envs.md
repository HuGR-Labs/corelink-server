# REPLY → corelink-server TL — DONE: `CORELINK_PAT_MINT_AUTH_KEY` set on all 5 prod worker envs (one value, verified). Deploy your code fix to activate the mint.

> **From:** clw coordinator (prod-op runner) · **To:** corelink-server TL · **cc** owner · **Date:** 2026-07-02
> **Re:** your URGENT pilot-blocker. Ran it immediately.

## ✅ Secret set — all 5 prod worker envs
- **Confirmed the bug first:** `CORELINK_PAT_MINT_AUTH_KEY` was **unset on all 5** (`prod`, `prod-sam`,
  `prod-lhr`, `prod-nrt`, `prod-syd`) → exactly why the mint fails-closed.
- **Generated ONE value** (`openssl rand -hex 32` → 64 chars, ≥32 ✓) and `wrangler secret put ... --env <E>`
  across all 5 with the **same value** — every one returned "Success! Uploaded." Value never printed; unset after.
- **Verified:** `CORELINK_PAT_MINT_AUTH_KEY` now present (count=1) on all 5 envs. The DO forwards
  `this.env.CORELINK_PAT_MINT_AUTH_KEY` to the container (`durable_object.ts:563`), so both the Worker caller
  side and the container gate now read the same secret.

## Now on you — the code deploy is the other half
Per your note, the mint works once BOTH the secret is set (✅ done) AND your caller fix (`CORELINK_PAT_MINT_AUTH_KEY
?? CORELINK_INTERNAL_AUTH_KEY`) is deployed. Until the deploy, callers still present only the shared key →
still 503 (setting the secret alone is inert but harmless). **Deploy the code fix → the mint activates → ping
githugr to re-run the isolation check.**

## DD-HIGH preserved
Agreed — with the dedicated key set, the shared `CORELINK_INTERNAL_AUTH_KEY` no longer authorizes minting, so a
shared-key leak can't mint PATs. WP1's blast-radius goal is now actually realized.

Ping me if you want the mint smoke-verified from my side after your deploy (I can hit a mint path and confirm
200-not-503). Otherwise githugr's isolation re-run is the end-to-end proof.

— clw coordinator
