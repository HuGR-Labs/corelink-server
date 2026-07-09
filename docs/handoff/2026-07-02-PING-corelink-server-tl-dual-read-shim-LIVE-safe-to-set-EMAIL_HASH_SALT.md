# PING → clw coordinator — dual-read shim is LIVE (4th deploy, 5/5). Safe to set `EMAIL_HASH_SALT` now — the 5 pending invites + 123 legacy tenants will NOT break.

> **From:** CoreLink Server TL · **To:** clw coordinator · **cc** owner · **Date:** 2026-07-02
> **Re:** your F2 "waiting your shim-live ping."

## Shim is live
The email-hash dual-read shipped (#585, 4th cf-deploy 5/5):
- **Writes** salt when `EMAIL_HASH_SALT` is set (unchanged intent).
- **Lookups** (invite-accept match) now try the salted hash THEN the legacy unsalted hash (`emailHashCandidates`, deduped) → a legacy pre-salt row still binds.
- Also fixed a latent bug: the signup-worker `emailHashFor` was UNSALTED-ONLY (never read the salt) — now salt-aware, so setting the salt actually takes effect on the signup-worker (before this, it wouldn't have).

**Net: your 5 pending invites + 123 legacy tenants keep working after you set the salt** (their legacy hashes still match via the dual-read; new writes salt).

## Go ahead — the ONE 6-target set
```
V=$(openssl rand -hex 32)
printf '%s' "$V" | worker/node_modules/.bin/wrangler secret put EMAIL_HASH_SALT --env prod
# same V for --env prod-sam / prod-lhr / prod-nrt / prod-syd, AND the signup-worker
printf '%s' "$V" | apps/signup-worker/node_modules/.bin/wrangler secret put EMAIL_HASH_SALT
```
**Same value across all 6** (5 main-worker prod envs + signup-worker) — a mismatch would split salted lookups. `printf '%s'` (no newline). No quiet-window fragility needed anymore (dual-read covers the transition), but same-value-everywhere is the one hard requirement.

## Verify after
A new invite created post-salt → accept it → binds (salted path). An OLD pending invite (pre-salt) → accept it → still binds (legacy path). Both work = dual-read confirmed.

That closes F2 + the CTRL-PRIV-001 salting DD item. Ping me if any bind fails. Routing via owner.

— CoreLink Server TL
