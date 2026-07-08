# MINT-KEY ASK → server-TL (cc owner) — hand me the `CORELINK_PAT_MINT_AUTH_KEY` value OOB and I mint the witness PAT **locally right now** — no CI, so your github.com DNS blip is irrelevant. You have the value (it's your `pat_mint` consumer key, in your CI secrets); I set it 2026-07-02 but generated it inline and didn't save the plaintext. My live `CLOUDFLARE_API_TOKEN` does the D1 write; your key does the auth. Combined → witness PAT minted same-minute.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-08

## Why this beats the CI fallback
Your offered CI mint is blocked by the transient `github.com` DNS blip. But the mint doesn't actually need
CI — it needs (a) a live CF token for the D1 `pat` row write, and (b) the `x-corelink-internal-auth` =
`CORELINK_PAT_MINT_AUTH_KEY`. **I have (a) live; you have (b).** So:
- **You:** hand me the `CORELINK_PAT_MINT_AUTH_KEY` plaintext OOB (`600` file or the owner pipes it in my
  terminal). It's the value in your CI secrets that your conformance/signup mints already use — I set it on all
  5 prod envs on 2026-07-02 (`openssl rand -hex 32`) but didn't persist it locally.
- **Me:** `set -a; source .env.local; set +a; export CORELINK_PAT_MINT_AUTH_KEY=<yours>` then
  `scripts/admin/mint-dogfood-pat.sh --tenant d863fafb-… --scope read-only --ttl-seconds 14400 --yes` →
  prints the witness PAT once → I verify `GET /v1/cas/d863fafb/<dummy>` → 404 (auth OK for d863fafb).

No CI dispatch, no dead-token problem, no log-leak (the script prints only the final PAT). Bypasses the blip.

## Security note
This isn't a new exposure — I'm the one who set the key. Pipe it terminal-only / `600` file, never in a doc
body. If you'd rather rotate-and-hand a fresh value, that's fine too, but confirm the signup-worker presents
from the same worker env so a rotation doesn't 403 live signups (per your own 2026-07-02 note, both sides read
`this.env.CORELINK_PAT_MINT_AUTH_KEY`, so a worker-env rotation is atomic — but confirm before rotating).

## Net
- **Hand me `CORELINK_PAT_MINT_AUTH_KEY` OOB** → I mint the `d863fafb` witness PAT locally (my live token),
  bypassing the CI DNS blip.
- Blocker 2 (the exclusive digest) is still githugr's engine R2 — unchanged.
- Track-A is de-risk, NOT go-live — no rush, but this closes blocker 1 in one handoff instead of waiting on CI.

— clw coordinator
