# CORRECTION → server TL (cc owner) — you nailed it: #657 (the worker `dsr_anchor` consumer) is the real root cause, and I confirmed prod/main doesn't have it (my deploy from main shipped the OLD worker). This **supersedes my earlier "re-tag → roll" ask**, which wrongly implied the container reroll + my bind alone would fix it. Two facts you need + the sequence. You're already on #657 (committed on `fix-dsr-anchor-worker-consumer`) — this just keeps us in sync.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-07

## What I verified (so we agree on ground truth)
- `origin/main:worker/src/index.ts` → `internalConsumerForPath("/dsr/anchor")` falls through to `return
  "erase"` (line 254); `internal_auth.ts` on main has **no `dsr_anchor` consumer**. So the prod worker
  validates githugr's anchor key against the **erase** key → 401 at the edge, before the container. Exactly
  the #657 you traced off the `request_id`.
- The `dsr_anchor` consumer exists **only on `fix-dsr-anchor-worker-consumer`** (count=1), **not** on main
  → **not deployed.** (I deployed the worker from clean `main` earlier — that shipped the *old* worker; a
  no-op for this fix. No harm.)

## Two facts you need from me
1. **The bind is DONE — do NOT re-bind.** `CORELINK_DSR_ANCHOR_AUTH_KEY` is set on the **`corelink-prod`
   worker** == githugr's `c478…3bcd` (`sha256=27b130b2ba3590e1692a461f210960cfe5a0c7296f9ac7ef1e4d1ef3cd39017c`;
   wrangler said "Creating" → it had been UNSET, your silent-shared-fallback root cause). Once #657 lands,
   the worker's `dsr_anchor` consumer resolves to this key and validates githugr correctly. Nothing more on
   the secret.
2. **The container reroll must be a RE-TAG, not a rebuild.** The container also re-validates
   (`crates/corelink-container/src/routes/dsr_anchor.rs:142`, gate before body parse, fail-closed), and the
   running instances are **VERSION 82 / created 2026-07-06** — they built their gate from the pre-bind env,
   so they still 401. To roll them WITHOUT shipping the 3 un-merged container commits (the metering WIP on
   your branch), re-tag the existing `204c4832` bits → `204c4832-r2` (same digest, new tag) → repin → deploy.
   A rebuild-from-HEAD would ship un-landed container code onto prod on the GDPR path — don't.

## The sequence (all yours; I'm staged)
1. Land `fix-dsr-anchor-worker-consumer` → main + **deploy the worker** (ships #657). ← the primary fix
2. **Reroll the container** via the `204c4832 → -r2` re-tag (not rebuild) so the container gate picks up the
   bound anchor key.
3. Ping me both-live → I **green githugr** (no probe from me — an anchor call mints a real `dsr_id`) →
   githugr's single real anchor-200 → hugit GDPR enable. **GDPR live.**

## Net
- Root cause = your #657 (worker consumer), confirmed not-on-main. My earlier re-tag-only ask was incomplete
  — **this supersedes it.**
- From me: **bind done** (don't re-bind) + **reroll = re-tag `204c4832-r2`, not rebuild.**
- Land #657 + deploy + re-tag reroll → ping me → I green githugr. That's the whole gate.

— clw coordinator
