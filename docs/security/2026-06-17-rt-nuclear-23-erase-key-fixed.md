# rt-nuclear #23 — erase-key end-to-end agreement: FIXED (not moot)

**Finding #23:** "Worker/container key mismatch silently breaks BOTH erase
surfaces in the intended full-split config."

## Verdict: a real latent mismatch existed; fixed Worker-side.

Server PR #317 fixed the **container** side (DSR + CAS-erase now gate on
`erase_auth_key_from_env()` = `CORELINK_ERASE_AUTH_KEY`, shared-key fallback).
The **Worker edge** that the live DSR driver must clear was NOT aligned.

### Evidence — the two erase callers

1. **Main Worker proxy** (`worker/src/index.ts`): maps `/_internal/dsr/*` →
   `erase` consumer (`internalConsumerForPath`), verifies the inbound header
   against `resolveConsumerKey(env, "erase")`, then STRIPS it and RE-INJECTS
   `x-corelink-internal-auth = resolveConsumerKey(env, "erase")` before
   forwarding to the container DO. ✓ Matches the container.

2. **signup-worker** (`apps/signup-worker/src/webhooks/dsr_consumer.ts`,
   `dsr_verify_cron.ts`) — the LIVE GDPR erasure driver (Clerk `user.deleted`
   → queue → `/_internal/dsr/erase`, plus the 24h verify cron). It calls
   `${CORELINK_API_BASE}/_internal/dsr/{erase,verify}` THROUGH the main Worker
   (the `CORELINK_API_SVC` service binding → `service = "corelink-prod"`, per
   `apps/signup-worker/wrangler.toml`), and injected only the SHARED
   `CORELINK_INTERNAL_AUTH_KEY`.

### Why that breaks in the full-split config

The main Worker's `/_internal/dsr/*` gate verifies against
`resolveConsumerKey(env, "erase")`. In the full-split config — where a dedicated
`CORELINK_ERASE_AUTH_KEY` (≥32 chars) is provisioned on the main Worker — that
resolves to the ERASE key, not the shared key. The signup-worker, sending the
SHARED key, would be **401'd** by the main Worker the moment the dedicated key
is deployed. The erase + verify surfaces both break silently — exactly the
full-split config #23 warns about. The shared-fallback only masked this while no
dedicated erase key existed.

### Fix (Worker-only)

New `apps/signup-worker/src/lib/erase-auth-key.ts::resolveEraseAuthKey(env)` —
erase-first (`CORELINK_ERASE_AUTH_KEY`), shared-fallback
(`CORELINK_INTERNAL_AUTH_KEY`), `null` when neither — mirroring the container's
`erase_auth_key_from_env()` and the main Worker's `resolveConsumerKey(env,
"erase")`. Both signup-worker DSR callers (erase consumer + verify cron, incl.
the cron's inert-guard) now inject the resolved erase key, so the keys agree in
every config. The ≥32-char floor stays authoritative at the two verify gates;
this resolver only selects WHICH key to send. `CORELINK_ERASE_AUTH_KEY`
documented as an optional secret in `apps/signup-worker/wrangler.toml`.

Tests: `tests/erase_auth_key.test.ts` (resolution matrix) + a full-split
regression in `tests/dsr_consumer.test.ts` (asserts the dedicated erase key is
sent when set).
