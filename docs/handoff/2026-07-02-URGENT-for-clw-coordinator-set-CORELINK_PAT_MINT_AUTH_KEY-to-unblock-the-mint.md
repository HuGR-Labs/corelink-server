# URGENT (pilot-blocker) → clw coordinator — set `CORELINK_PAT_MINT_AUTH_KEY` on the prod worker to activate the mint fix. Without it, ALL token minting stays down (503 "upstream mint failure").

> **From:** CoreLink Server TL · **To:** clw coordinator · **cc** owner · **Date:** 2026-07-02

## The bug (mine, from WP1) + the fix
WP1 (DD-HIGH) made the container's `/_internal/pat/mint` gate REQUIRE the dedicated `CORELINK_PAT_MINT_AUTH_KEY` with NO shared-key fallback — but the Worker mint callers still presented the SHARED `CORELINK_INTERNAL_AUTH_KEY`, so every mint has been failing-closed since deploy 1 (latent until githugr's end-to-end test surfaced it: **provisioning is correct, the mint 503s**). I've fixed the 4 Worker callers to present `CORELINK_PAT_MINT_AUTH_KEY ?? CORELINK_INTERNAL_AUTH_KEY` (dedicated-if-set). **But the dedicated key must actually be SET for the mint to work** (the container's gate requires it; unset → fail-closed).

## What you run (one secret, on the WORKER — it forwards to the container)
```
V=$(openssl rand -hex 32)
printf '%s' "$V" | worker/node_modules/.bin/wrangler secret put CORELINK_PAT_MINT_AUTH_KEY --env prod
# repeat for --env prod-sam / prod-lhr / prod-nrt / prod-syd (same V — the 5 prod worker envs)
```
- **ONE value across all 5 prod worker envs.** The DO forwards `this.env.CORELINK_PAT_MINT_AUTH_KEY` to the container (`durable_object.ts:563`), so the SAME secret is both (a) presented by the Worker mint callers AND (b) checked by the container gate — set it once per worker env and both sides match.
- `printf '%s'` — no trailing newline.
- **Must be ≥32 chars** (the container rejects a shorter key → route fails-closed). `openssl rand -hex 32` = 64 chars, fine.
- **Order:** the code fix (Worker sending the dedicated key) ships in my next deploy; set the secret ANY time (before or after the deploy — the Worker reads it at request time). The mint works once BOTH the deploy is live AND the secret is set.

## Why this preserves the DD-HIGH (not a loosening)
Once `CORELINK_PAT_MINT_AUTH_KEY` is set, the shared `CORELINK_INTERNAL_AUTH_KEY` NO LONGER authorizes the mint — a shared-key leak can't mint PATs. That's exactly the WP1 blast-radius goal, now actually realized (WP1 shipped the gate but the key was never provisioned + callers never updated — my miss).

Ping me when set. I deploy the code fix + then ping githugr to re-run the isolation check → flip. Routing via owner.

— CoreLink Server TL
