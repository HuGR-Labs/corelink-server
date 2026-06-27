# Clerk session minter (`clerk-session-minter.mjs`)

Mints a **real Clerk session JWT** for the e2e journeys that are gated on a
Clerk *session* bearer (not a PAT):

| Env var                          | Gated journey                                            |
| -------------------------------- | ------------------------------------------------------- |
| `CORELINK_E2E_CLERK_SESSION`     | `billing.rs` — authed checkout / `tier-select`          |
| `CORELINK_E2E_DSR_SESSION`       | `dsr.rs` — customer self-erasure request → gone         |
| `CORELINK_E2E_DSR_TEST_SESSION`  | `dsr.rs` — self-driving full erasure flow               |
| `CORELINK_E2E_DSR_TEST_TENANT`   | (best-effort) the resolved throwaway tenant id          |

## How it works (FAPI-direct — no browser)

A Clerk session JWT **cannot be minted with a plain Backend-API call** — but it
**can** be minted by driving Clerk's **Frontend API (FAPI)** directly with two
sanctioned Backend-API primitives, with **no browser and no `window.Clerk`**.
(The old approach navigated headless Chromium to the prod app and waited for
`window.Clerk` — which FAILS, because the prod SPA does not expose ClerkJS to a
headless context.) The robust flow:

1. Create a throwaway Clerk user (Backend API). Capture `user_id`.
2. `POST /v1/sign_in_tokens { user_id }` (Backend API) → a one-time **ticket**
   (Clerk's "sign a user in without a password" primitive).
3. `POST /v1/testing_tokens` (Backend API) → a **Testing Token** (the documented
   bot-detection bypass), attached to FAPI as `?__clerk_testing_token=`.
4. `POST https://<FAPI>/v1/client/sign_ins` with body
   `strategy=ticket&ticket=<signInToken>` (form-encoded) → consumes the ticket,
   creates the client (`Set-Cookie __client`) and a completed sign-in carrying
   `created_session_id`. A tiny cookie jar carries `__client` across calls.
5. `POST https://<FAPI>/v1/client/sessions/<sessionId>/tokens` → `{ jwt }` — the
   ~60s default `__session` JWT.
6. **Tenant-readiness gate (default ON — `--no-wait` to skip).** A throwaway user
   gets a CoreLink tenant only when the signup-worker `user.created` webhook fires
   (async, seconds); until then a session-authed call **401/403s** (no tenant
   row). The minter **polls `${CORELINK_API_ENDPOINT or https://corelink-api.humangr.com}/v1/users/me`**
   with the bearer until it returns **200**. Because the JWT lives ~60s, it
   **re-mints a fresh JWT before every probe** (cheap re-mint on the same live
   session; full re-sign-in if the session lapsed), so the probe always uses a
   live bearer. It emits **only after a 200** — so the session is *guaranteed
   tenant-ready*. If the budget (`CORELINK_E2E_TENANT_WAIT_MS`, default ~120s)
   expires still-401, it **fails loud** (`tenant never provisioned within Ns —
   signup-worker user.created webhook lag; check Svix`).
7. **Freshness.** Once tenant-ready, it mints **one final fresh JWT right before
   emit**, so the consumer gets a full ~60s window.

Every FAPI call sends `Origin: https://corelink-app.humangr.com`, because Clerk
derives the JWT's **`azp` from the request Origin** — that is what makes the
token pass `worker/src/lib/clerk_auth.ts` (`CLERK_AZP_ALLOWLIST`). It also
carries `iss = <FAPI issuer>` and `sub = user_id`, exactly what
`verifyClerkSessionAndResolveTenant` requires.

## Run it

```bash
cd scripts/e2e-real-client
# No browser needed. Pure node (>=18) fetch — playwright/Chromium NOT required.

# Source your live secrets first (CLERK_SECRET_KEY etc. live ONLY in .env.local):
set -a; source ../../.env.local; set +a

# Mint (writes /tmp/e2e-clerk-session.sh and prints the exports to stdout).
# Default: BLOCKS until the throwaway user's tenant is provisioned (200 on
# /v1/users/me), re-minting a fresh JWT before each probe — emits only when ready:
node clerk-session-minter.mjs
# …then load them into the e2e shell:
source /tmp/e2e-clerk-session.sh

# Or in one shot:
eval "$(node clerk-session-minter.mjs)"

# Skip the readiness gate (emit immediately; session may 401 until the webhook):
node clerk-session-minter.mjs --no-wait
```

Custom output file: `node clerk-session-minter.mjs /path/to/session.sh`.

## Environment it reads

| Var                            | Required | Default                              | Purpose                                                             |
| ------------------------------ | -------- | ------------------------------------ | ------------------------------------------------------------------ |
| `CLERK_SECRET_KEY`             | yes\*    | (also accepts `CLERK_LIVE_SECRET_KEY`) | Backend API auth (create user + sign-in token + testing token)  |
| `CLERK_PUBLISHABLE_KEY`        | optional | —                                    | Fallback to derive the FAPI host (not otherwise required)          |
| `CORELINK_APP_URL`             | no       | `https://corelink-app.humangr.com`   | FAPI `Origin` → becomes the JWT `azp` (must be in the allowlist)   |
| `CLERK_FAPI`                   | no       | `clerk.corelink-app.humangr.com`     | FAPI host the sign-in is driven against                            |
| `CLERK_JS_VERSION`             | no       | `5.57.0`                             | `_clerk_js_version` advertised to FAPI (override if rejected)      |
| `CORELINK_API_ENDPOINT`        | no       | —                                    | Best-effort tenant resolution (also reads `CORELINK_E2E_ENDPOINT`) |
| `CORELINK_E2E_EMAIL_DOMAIN`    | no       | `corelink-e2e.dev`                   | Domain for the throwaway user's email                             |
| `CORELINK_E2E_TENANT_WAIT_MS`  | no       | `120000`                             | Budget for the tenant-readiness gate (poll `/v1/users/me` → 200)  |
| `CORELINK_E2E_TENANT_WAIT_INTERVAL_MS` | no | `3000`                            | Delay between readiness probes                                     |

\* either `CLERK_SECRET_KEY` or `CLERK_LIVE_SECRET_KEY`.

The readiness probe targets `${CORELINK_API_ENDPOINT or https://corelink-api.humangr.com}/v1/users/me`.

## Environment it emits

To **stdout** (source-able) and to the output file (`/tmp/e2e-clerk-session.sh`,
mode `0600`):

```sh
export CORELINK_E2E_CLERK_SESSION='<jwt>'
export CORELINK_E2E_DSR_SESSION='<jwt>'
export CORELINK_E2E_DSR_TEST_SESSION='<jwt>'
export CORELINK_E2E_DSR_TEST_TENANT='<tenant-id>'   # only if resolvable
```

The Clerk `user_id` is printed to **stderr** so you can DSR-delete it on cleanup.
A sidecar `<outfile>.user.json` (mode `0600`) records the user creds for
`--refresh`.

## ⚠️ TTL — the session JWT lives ~60 seconds

A Clerk session JWT is short-lived (~60s). The minter blocks on the readiness
gate and then mints **one final fresh JWT right before emit**, so you get a full
~60s window — but that window is still only ~60s. Mint it **immediately before**
the run, or refresh it:

```bash
node clerk-session-minter.mjs --refresh      # re-signs in the SAME user, fresh JWT
```

`--refresh` reuses the user recorded in the sidecar (run once without `--refresh`
first). On refresh, the readiness wait is **skipped** automatically if a prior
run already recorded the tenant as provisioned (sidecar `tenantReady` flag), so
`--refresh` returns a fresh JWT fast.

### Recommended usage for the full suite

Because the JWT is ~60s, **run the Clerk-session journeys IMMEDIATELY after the
mint.** Those journeys — DSR ×4 (request → gone, full self-erasure) + checkout +
tier-select — are fast HTTP and comfortably finish inside the window. So:

- **Source the env from this minter LAST, right before the run**, and make sure
  the Clerk-session journeys execute **within ~60s of the mint**:

  ```bash
  set -a; source ../../.env.local; set +a
  source <(node clerk-session-minter.mjs)   # tenant-ready + fresh on emit
  # …run the Clerk-session journeys NOW (DSR / checkout / tier-select)…
  ```

> **Follow-up (longer-lived approach):** have the suite re-read a refreshed
> session file on a 401 — i.e. a background `--refresh` loop rewriting
> `/tmp/e2e-clerk-session.sh` that the suite re-sources on auth failure — so a
> multi-minute run never trips the 60s TTL. Not yet wired.

## Fail-loud diagnostics

Every Backend-API and FAPI call fails loud — the script **never prints a JWT it
could not actually mint**, so a caller can never be fooled into running journeys
against a bad bearer. On any non-2xx FAPI call it prints the **method**, the
**full URL** (testing token redacted), the **status**, and the **full response
body**, so the next run shows exactly which call/param is wrong:

```
Clerk FAPI POST https://clerk.corelink-app.humangr.com/v1/client/sign_ins?_clerk_js_version=5.57.0&__clerk_testing_token=<redacted> → 422
  response body: {"errors":[{"code":"...","message":"..."}]}
```

Likely first-run tweaks (the FAPI contract may need 1-2 iterations):

- `POST /v1/testing_tokens` fails → Testing Tokens need a test instance OR an
  instance with testing enabled (Clerk dashboard).
- `sign_ins` returns non-`complete` / `client not found` → the full sign_in/error
  body is printed; adjust `CLERK_JS_VERSION` or the FAPI host as indicated.
- token has the wrong `azp` → confirm `CORELINK_APP_URL` matches the worker's
  `CLERK_AZP_ALLOWLIST` (it is sent as the FAPI `Origin`).

## Cleanup (do not skip)

The throwaway user lingers in Clerk until deleted. After your run:

```bash
USER_ID=$(jq -r .userId /tmp/e2e-clerk-session.sh.user.json)
curl -s -X DELETE "https://api.clerk.com/v1/users/$USER_ID" \
  -H "Authorization: Bearer $CLERK_SECRET_KEY"
```

(Or invoke the DSR account-deletion flow against that user — the more
end-to-end cleanup.)
