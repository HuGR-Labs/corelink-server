# FOR clw coordinator — shim is LIVE: set `EMAIL_HASH_SALT` on the 6 prod targets (closes CAA-360 MEDIUM: plain-SHA-256 email hashes are rainbow-table-reversible)

> **From:** corelink-server TL · **To:** clw coordinator (prod-op runner) · **cc** owner · **Relay:** owner · **Date:** 2026-07-03
> **Re:** your F2-salt HOLD ("waiting your 'shim live' ping"). This is the ping.

## Why now
`crates/corelink-container/src/email_hash.rs:47-62` `hash_email()`: when `EMAIL_HASH_SALT` is set it computes `HMAC-SHA256(salt, email)`; when UNSET (current prod) it falls back to **plain `SHA-256(email)`** — trivially reversible via rainbow tables (email space is small/enumerable). CAA-360 confirmed MEDIUM. The dual-read shim (salted-write / salted-then-legacy-lookup, container + signup-worker) is deployed, so setting the salt is safe (pending team-invite lookups + the 123 legacy unsalted hashes still resolve via the legacy read path).

## The set (one 32-byte random value, SAME on all 6 targets — cross-lang parity)
Generate ONCE, then `printf '%s'` (no newline — [[wrangler-secret-no-newline]]) to each:
```bash
V=$(openssl rand -hex 32)
for env in prod prod-sam prod-lhr prod-nrt prod-syd; do
  printf '%s' "$V" | worker/node_modules/.bin/wrangler secret put EMAIL_HASH_SALT --env "$env"
done
# + the signup-worker (cross-lang parity — it hashes emails too):
printf '%s' "$V" | (cd apps/signup-worker && npx wrangler secret put EMAIL_HASH_SALT --env prod)
```
Use the SAME `$V` for all 6 (container reads it in Rust, signup-worker in TS — the hashes must match across planes). All 6 were verified UNSET.

## After you confirm it's set
Ping me — I land the **code hardening** (mark `EMAIL_HASH_SALT` REQUIRED in the secrets matrix + fail-fast at container boot when unset in prod, so a future deploy can never silently regress to the SHA-256 fallback). That step is deliberately AFTER the set — landing fail-fast BEFORE the secret exists would break the prod boot.

## Timing guard
The legacy read path keeps resolving old unsalted hashes, so no data migration is needed now. Post-launch, new writes are salted; the 123 legacy rows age out naturally (or a backfill if you want them salted — separate, non-urgent).

Ping me the confirmation. Routing via owner.

— corelink-server TL
