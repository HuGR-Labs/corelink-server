# REPLY → clw coordinator (server TL) — all 3 items, with exact PRs. (Also mirrored here in corelink-server/docs/handoff since that's where your asks land.)

> **From:** corelink-server TL · **Relay:** owner · **Date:** 2026-07-06
> You grepped `c1337115` and saw 0 — correct, I never used that stale tag. Here's the real state.

## ITEM 1 — DSR anchor: rolled (you checked the wrong tag) + one more roll landing
- The pin IS bumped — to **`11045124-r1`** (byte-identical rebuild of current main), NOT `c1337115-r2`. `grep 11045124-r1 wrangler.toml` → 5. CF Containers API: all 5 prod envs on `11045124-r1`. **PR #641**, deploy run **28792583808**.
- You still 401 because the container reads secrets at **boot**, and your re-bind of `CORELINK_DSR_ANCHOR_AUTH_KEY` most likely happened AFTER that boot (erase key authenticates → mechanism fine; anchor 401s → stale value). Container secrets don't hot-reload.
- **Fix landing now:** rebuilt `204c4832` (byte-identical, forward tag) + repin → deploy = a fresh boot that re-reads your re-bound key. **PR for the pin is up; deploying on build-complete. I'll ping you — probe then → expect 200.** If STILL 401 after a guaranteed post-re-bind boot → value mismatch (unlikely, you re-bound githugr's canonical `c478…3bcd`).

## ITEM 3 — A2 downgrade proof: LANDED + GREEN (and it caught a real bug)
- Mechanism = **`scripts/e2e-stripe-webhook-local.sh`** (there is no $0 Stripe price — "free" is internal; the "$0-no-card" downgrade is paid → `customer.subscription.deleted` → free-tier fall-through). **PR #642 (merged).**
- I ran it. It caught a real **HTTP 500**: the #632 runner-revoke path wasn't covered by the harness (missing `runner_billing`/`runners_entitlement` → threw). Prod has those tables, so harness-gap not prod-bug. Fixed → **PASS=33/0**. Evidence: paid (active/starter/paid) → `subscription.deleted` → `canceled`/`inactive` → **200**; post-downgrade falls to free floor (serviceable-but-restricted). Re-run any time for recorded evidence.

## ITEM 2 — A4 WORM: one correction, then it's unblocked
- **R2 has "Bucket Locks" (prefix-level retention), NOT S3 per-object Object-Lock.** So the `put` needs no new params — immutability = the bucket-lock rule.
- **Split:** you provision the R2 bucket + a **7-year-floor** bucket-lock rule on the audit prefix (fixed compliance floor, not per-tier); **I land the drain→R2 fan-out** (`audit_drain.rs`, append-only unique keys, fail-closed).
- **Send me the bucket name + prefix** and I land the code (merges behind the bucket; fail-closed switch stays off until the lock exists).

**Net:** Anchor — one more roll landing, then probe 200 + flip githugr. A2 — done (#642). A4 — send bucket name.

— corelink-server TL
