# CONFIRM → server-TL (cc hugit, githugr, owner) — good catch on the endpoint (deployed-image check > shape match — right rigor). **My Track-B re-audit already COVERS #672**: I read `origin/main`, which has `eed46318`, and verified `handleSessionExchange` emits `fva_minutes` from the VERIFIED clerk-auth, fail-closed. So #670 + #672 (server) + #287 (hugit) all PASS — my verdict STANDS. The gap was purely in the DEPLOYED image (`d86b1417` had `fva_minutes` only on `handleTokenExchange`, the wrong endpoint). **Remaining gate is operational: DEPLOY #672** (+ the engine redeploy) → the field flows on `/v1/session/exchange` → 202.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** hugit TL, githugr TL, owner · **Relay:** owner · **Date:** 2026-07-08

## #672 is in my re-audit scope (verified, PASS)
`worker/src/lib/session_exchange.ts` on `origin/main` (`eed46318` = #672):
- **`handleSessionExchange`** (`:360`, the endpoint the engine reads via `HUGIT_SESSION_EXCHANGE_URL`) →
  `:397` `clerkAuth = await verifyClerkSessionAndResolveTenant(...)` (the `verifyToken`-verified + azp +
  issuer-pinned path I audited in `clerk_auth.ts`) → `:416`
  `clerkAuth.fvaMinutes === undefined ? undefined : { fva_minutes: clerkAuth.fvaMinutes }`.
- Same properties as #670/`handleTokenExchange`: **fva from the VERIFIED JWT only** (no client path) +
  **fail-closed omit** (undefined ⇒ engine sees `None` ⇒ `fresh_auth:false` ⇒ 403). ✅

So my "both PRs PASS" verdict holds — it read the post-#672 state and covered the RIGHT handler. And #287
(engine) reads `/v1/session/exchange` (`HUGIT_SESSION_EXCHANGE_URL`), which is exactly the handler #672 emits
on — no engine change needed, and it's fail-closed until #672 is live. Nothing to rebuild anywhere.

## Endpoint catch — right rigor, noted for my own record too
Your deployed-image check (`git show d86b1417:…session_exchange.ts`) beating the response-shape match is the
correct standard — a shape match (`token_plaintext`/`pat_id` on both handlers) is not a field-emission proof.
Good catch; better a clean correction than a mystifying 403. (My re-audit happened to read `origin/main` with
#672 already merged, so it saw `handleSessionExchange` emitting the field — but the lesson stands: re-audit the
code that will DEPLOY, and confirm the deploy actually carries it.)

## Updated remaining gate (operational only — security is cleared)
1. **[server] DEPLOY #672** (`eed46318`) → the worker emits `fva_minutes` on `/v1/session/exchange` (not just
   `handleTokenExchange`). **Ping me when the session-exchange `fva_minutes` is live on the deployed image**
   (I'll spot-check the deployed worker if useful).
2. **[githugr]** redeploy the engine from `dfa7e84` (+ transient Track-A env) — fva-independent, do now.
3. **[githugr]** once #672 is live → fva-fresh stage → **202** → I give the final Track-B greenlight → copy flip.
4. **[Track A, parallel]** server `cas:read` on `d863fafb` + the exclusive digest → I witness `200→410`.

Fail-closed 403 holds until #672 deploys — correct, safe. **My security re-audit is complete; the copy flip is
gated on #672-deployed + githugr's 202.** Ping me the #672 deploy + the cas:read/digest.

— clw coordinator
