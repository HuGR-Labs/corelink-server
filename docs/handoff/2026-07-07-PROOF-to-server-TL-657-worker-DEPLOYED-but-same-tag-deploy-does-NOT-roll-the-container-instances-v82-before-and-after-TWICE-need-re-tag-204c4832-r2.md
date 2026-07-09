# PROOF → server TL (cc owner) — I ran your one-command twice. **#657 worker is DEPLOYED (edge gate fixed) ✅.** But the same-tag deploy does **NOT** roll the container instances — hard evidence: the `d863fafb` instance is VERSION 82 / created `2026-07-06` **before AND after both runs, unchanged.** So the container's own anchor gate still 401s. The last step is a real instance roll: **re-tag `204c4832 → r2` (NOT rebuild)** — a same-tag deploy provably cannot do it.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-07
> Re: your `2026-07-07-EXECUTE-NOW…one-command…`. I fired it (from clean `main fca77754`, #657 confirmed on
> origin/main). Worker half worked. Container half did not. Evidence below — the poll doesn't force a roll.

## What worked ✅
`git checkout main && pull` (fca77754, `index.ts dsr_anchor=1`) → `SKIP_PIN_FRESHNESS=1
deploy-container-prod.sh --apply --env prod` → Worker deployed, **Current Version ID `e0523dfd`**. The
**#657 edge gate is live**: the worker now routes `/dsr/anchor` to the `dsr_anchor` consumer and validates
against `CORELINK_DSR_ANCHOR_AUTH_KEY` (which I bound = `c478…3bcd`). Worker gate: PASS.

## What did NOT work — the container never rolled (hard proof)
`wrangler containers instances <corelink-prod app>`, the `d863fafb` instance (the tenant a DSR anchor call
hits), **identical before and after the deploy, both times I ran it today**:
```
d863fafb-…  running  ewr12  VERSION 82  CREATED 2026-07-06T14:28:36Z
```
And wrangler printed **`no changes … No changes to be made`** for the container app on both runs; the script
"CONVERGED in 16s" only because `running_image == pin` was **already** true (`204c4832`). **The poll verifies
the image matches the pin — it does not restart a same-image instance.** CF containers read
`std::env::var("CORELINK_DSR_ANCHOR_AUTH_KEY")` at process **spawn** (`build_state_from_env` → the gate at
`dsr_anchor.rs:142`); the v82 process started `2026-07-06` when the key was UNSET, so it built its gate on the
shared fallback and **cannot** pick up the now-bound key without an actual restart. It hasn't restarted → the
**container gate still 401s**, even though the worker gate now passes.

**Chain right now:** worker (post-#657) validates + forwards → container (v82, stale gate) → **401.** The
anchor still fails. So I am **holding** — I will NOT green githugr (a green burns a real `dsr_id` on a failing
anchor).

## The one thing left — force a real instance roll (re-tag, not rebuild)
A same-tag deploy can't roll it (proven twice). Force the roll with a **new image revision of the identical
bits**:
1. Re-tag `…corelinkserver-prod:204c4832-r1` → **`204c4832-r2`** (same digest, new tag) via
   `push-container-multiregion.sh` — **NOT** a rebuild (a rebuild ships the un-merged container/metering
   commits onto prod; keep the bits at `204c4832`).
2. Repin `wrangler.toml [[env.prod.containers]]` (+ regional `prod-*` if the anchor serves them) `-r1 → -r2`.
3. Deploy → the **changed tag** forces a version bump → instances roll → respawn on the identical bits + the
   now-forwarded anchor key → the container gate accepts `c478…3bcd`.
(If you have a CF instance-restart/rollout lever that doesn't need a tag change, use it — but `wrangler
containers delete` takes the *application* ID = destroys the app, so that's not it.)

## Verify + hand-off
- Confirm the roll by the instance itself: `wrangler containers instances …` → `d863fafb` shows a **new
  VERSION and CREATED = today** (not 82 / 2026-07-06). The image-pin poll is NOT sufficient proof here.
- Ping me on a real roll → I **green githugr** → their single anchor call → 200 + `dsr_id` → hugit erase →
  410 Gone. GDPR live.

## Net
- **#657 worker: DEPLOYED ✅** (edge gate fixed, my bind consulted).
- **Container: NOT rolled** (v82 before+after, twice — proof). Same-tag deploy is a no-op for instance
  restart. **Need the `204c4832 → r2` re-tag roll.** That's the whole remaining gate. Ping me when
  `d863fafb` shows a fresh VERSION/CREATED.

— clw coordinator
