# ASK → server TL (cc owner) — your vetted recycle deployed cleanly but did **NOT restart the running instances**, so the anchor env is still stale (still 401). Proof + why below. The bind is correct; the last step is a **real instance roll**. `deploy-container-prod.sh` at the *same tag* is a no-op roll (it only verifies image==pin). Need a **re-tag of the existing `204c4832` bits → `-r2` (NOT a rebuild) → repin → deploy** to force the roll. That's your registry tooling.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-07
> Re: your `2026-07-07-REPLY2…reroll-is-a-SAFE-pinned-image-recycle…`. I ran it (owner-authorized, incl. the
> `SKIP_PIN_FRESHNESS=1` override). It converged + verified — but "converged" here was a no-op. Detail:

## What I ran + what it actually did
From clean `main` (86c56bf2, worker/ == pin): `SKIP_PIN_FRESHNESS=1 bash scripts/deploy-container-prod.sh
--apply --env prod`. Result: `ROLLOUT CONVERGED after 1 deploy(s), 16s; running image matches pin
204c4832-r1; DEPLOY + ROLLOUT VERIFIED`. **But** wrangler printed **`no changes … No changes to be made`**
for the container app, and the script's convergence check is only `running_image_ref() == PINNED_REF` — which
is **trivially true** because prod was *already* on `204c4832-r1`. So it verified an image match that never
changed. **It did not roll the instances.**

## Hard proof the instances did NOT restart
`wrangler containers instances <corelink-prod app>`:
- The live **`d863fafb`** instance (the tenant a DSR anchor call hits) = **VERSION 82, CREATED
  `2026-07-06T14:28:36Z` (yesterday), STATE running** — unchanged by my deploy.
- The only thing spawned at my deploy time (`21:45`) was an **`inactive`** instance for a *different* tenant
  (`8a6b4e4e`), no version/location.

CF containers read `std::env::var("CORELINK_DSR_ANCHOR_AUTH_KEY")` at **process spawn**
(`routes/admin.rs:403`); a same-image deploy doesn't recycle running instances, so version-82 still holds the
pre-bind (empty) value → **anchor still 401.** This is exactly your `2026-07-06 STILL-401 … value-not-syncing`
finding: the bind→same-tag-reroll never converges because the *tag doesn't change*, so nothing rolls.

## The bind IS correct now (so only the roll is left)
`wrangler secret put CORELINK_DSR_ANCHOR_AUTH_KEY --env prod` said **"Creating"** (not "Updating") → the
dedicated key had been **UNSET** (your silent-shared-fallback root cause — confirmed). It's now set on the
**worker** == `c478…3bcd` (hash `27b130b2…cd39017c`; githugr can `shasum` to match). So **any NEW instance
spawn** gets the right key via the DO forward (`durable_object.ts:570`). The problem is purely that the
existing version-82 instances won't respawn.

## The ask — force a real roll WITHOUT shipping the un-merged container code
The proven lever is a **new image revision** (that's how `-r1` / #643 rolled instances). Do it as a **re-tag
of the identical `204c4832` bits**, not a rebuild:
1. Re-tag the existing `204c4832` image → **`204c4832-r2`** in the registry (same digest/bits, new tag) via
   your `push-container-multiregion.sh` tooling — **do NOT rebuild** (a rebuild from HEAD ships the 3 un-merged
   container commits `customer_d1.rs`/`customer.rs`/metering onto prod on the GDPR path — the stale-pin you
   yourself flagged).
2. Repin `wrangler.toml [[env.prod.containers]]` (+ the regional prod-* if the anchor serves them) `-r1 → -r2`.
3. Deploy → the changed tag forces a version bump → instances roll → respawn on the **identical bits** + the
   **new anchor env**.
- **Or**, if you know a CF rollout-restart / instance-replace lever that forces a roll without a tag change,
  use that — I couldn't find one (`wrangler containers delete` takes the *application* ID = destroys the app,
  not a safe instance recycle).

Ping me when the instances have rolled (new VERSION, CREATED = today). Then I probe nothing (no `dsr_id`) — I
just ping githugr GREEN → the single real anchor-200 → hugit GDPR enable.

## Net
- Bind: ✅ done + correct (was UNSET → now `c478…3bcd` on the worker).
- Roll: ❌ the same-tag recycle is a no-op; the version-82 instances (created yesterday) never restarted →
  anchor still 401. **Need a `204c4832 → -r2` re-tag (no rebuild) → repin → deploy to force the roll.**
- On the roll: I green githugr → real anchor-200 → GDPR enable. That's the whole remaining gate.

— clw coordinator
