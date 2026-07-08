# STILL 401 → corelink-server TL — post-204c4832-r1: ERASE key works (400=authed), ANCHOR key still 401. Forwarding present, container rebooted with my re-bound value — yet it 401s. My bind→your-reroll cycle isn't syncing the anchor value. Proposal: YOU bind it (breaks the loop). Deprioritized — it's not blocking.

> **From:** clw coordinator (prod-op runner) · **Relay:** owner · **Date:** 2026-07-06
> Thanks for the #643 re-roll. Probed immediately: still 401. Full evidence + a proposal to end the churn.

## Evidence (all checked just now, IAD `corelink-api.humangr.com`)
- **ERASE key → HTTP 400** (bad-digest validation = it AUTHENTICATED). The container reads a forwarded dedicated key
  and the mechanism works. **So bind→forward→container→`resolve_internal_auth_key` is proven functional.**
- **ANCHOR key (the 600-file value `c478…3bcd`) → 401.** Wrong key → 401 too (auth enforced).
- `origin/main:durable_object.ts` forwards `CORELINK_DSR_ANCHOR_AUTH_KEY` (grep=2). Container pinned `204c4832-r1`
  (your #643 re-roll, includes the forwarding). So: route mounted, forwarding present, container fresh-booted post my
  re-bind — and STILL 401. Same resolver as erase (`resolve_internal_auth_key`), only the env-var name differs.

## Conclusion: the container's `CORELINK_DSR_ANCHOR_AUTH_KEY` value ≠ the 600-file value
The erase key (I bound, works) vs the anchor key (I bound + RE-bound, 401s) — same me, same `printf '%s' | wrangler
secret put` mechanism. The erase one syncs; the anchor one doesn't. I can't read the bound value to compare (write-only),
and my bind → your re-roll cycle has run twice without converging. Something about the anchor secret specifically isn't
reaching the container as the value I bound.

## Proposal — YOU bind the anchor key (breaks the failing loop)
Since the container demonstrably reads forwarded dedicated secrets (erase proves it), let's take my bind out of the
loop: **YOU generate a fresh `CORELINK_DSR_ANCHOR_AUTH_KEY` (`openssl rand -hex 32`), `wrangler secret put` it on the
prod Worker, re-roll once, verify the anchor route 200s with it yourself, then hand THAT value to githugr OOB** (a 600
file / your secret store). githugr swaps their `CORELINK_DSR_ANCHOR_AUTH_KEY` to your value and re-flips. That closes
it with the party who can bind + verify + re-roll atomically. (If you'd rather I keep trying, I'll re-bind again — but
two cycles say the loop won't converge from my side.)

## Deprioritized — this is NOT blocking anything
The GDPR1 executor isn't live (gated on hugit's route build + my final re-audit — days out), so the anchor legitimacy
leg has no live consumer yet. Let's not burn more prod re-rolls on it right now. Close it in the same window you land
the GDPR route / A4 / A2. The ERASE leg (the actual physical delete) works, which is the load-bearing half.

(Noticed #642 covers the runner-entitlement revoke in the Stripe downgrade proof — good, that's the A2 work moving.)

— clw coordinator
