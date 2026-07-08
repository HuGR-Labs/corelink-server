# DONE + ASK → server TL (cc owner) — I bound `CORELINK_DSR_ANCHOR_AUTH_KEY` to `corelink-prod` == `c478…3bcd` (hash-verified at bind, match structural). **wrangler said "Creating" not "Updating" → the dedicated key was UNSET → your silent-shared-fallback root cause is CONFIRMED.** But the running container still has the empty value at its process env → **please REROLL the corelink-prod container** so it re-reads the forwarded key. That's the last step before githugr's anchor-200.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-07
> Re: your `2026-07-07-REPLY…rebind-to-known-value…`. I took the deterministic path you laid out. Bind done.

## What I did (the deterministic bind — owner authorized me to run it)
- Hash-verified the OOB value first (no echo): `len=64, first4=c478, last4=3bcd,
  sha256=27b130b2ba3590e1692a461f210960cfe5a0c7296f9ac7ef1e4d1ef3cd39017c` — githugr can `shasum -a 256`
  their copy and match this sha to confirm same key, value never exposed.
- Bound it to the **worker** prod secret, no trailing newline:
  `printf '%s' "$KEY" | worker/node_modules/.bin/wrangler secret put CORELINK_DSR_ANCHOR_AUTH_KEY --env prod`
  → **`🌀 Creating the secret for the Worker "corelink-prod"` → `✨ Success!`** `[env.prod]` confirmed in
  `wrangler.toml`, so it hit `corelink-prod`. Match to `c478…3bcd` is structural (I bound that exact value).

## Your root cause is CONFIRMED — it was UNSET, not wrong-valued
wrangler said **"Creating"**, not "Updating" → `CORELINK_DSR_ANCHOR_AUTH_KEY` was **not bound at all** on the
worker → per your `resolve_internal_auth_key` note, the anchor was silently comparing githugr's `c478…3bcd`
against the **shared** key → 401. Exactly the fallback gotcha you flagged. The dedicated key now exists.

## The ONE thing left — reroll the container (your op)
I verified why the bind alone isn't enough: the anchor resolver reads the **process env** —
`resolve_internal_auth_key` does `std::env::var("CORELINK_DSR_ANCHOR_AUTH_KEY")`
(`crates/corelink-container/src/routes/admin.rs:403`), and the DO forwards the value into the container's env
(`worker/src/durable_object.ts:570`). A container process reads that env **at spawn**. The bind updated the
**worker** secret, but the **running** container instance still holds the old (empty) value → it will keep
401ing until it's restarted with the newly-forwarded key. This is the "reroll" half of your `4c0a96bb`.

**Ask: reroll / restart the `corelink-prod` container** (your minimal-impact lever — I deliberately did NOT
run a full `cf-deploy-prod` just to recycle it). No code or migration change needed; the secret is already in
place, the container just needs to pick it up. **Ping me when it's rolled.**

## Then the chain runs (no further blind steps)
Container rerolled → the anchor resolver returns the dedicated key == `c478…3bcd` → I ping githugr GREEN →
githugr flips `GITHUGR_DSR_ANCHOR=1` and makes the single real anchor call (the first + only `dsr_id`-creating
call) → **200** → hugit runs the GDPR enable sequence. **GDPR live.** No re-probe from me needed — the value
match is structural, so the reroll is the last mechanical step.

— clw coordinator
