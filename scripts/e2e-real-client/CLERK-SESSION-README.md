# Clerk session minter (`clerk-session-minter.mjs`)

Mints a **real Clerk session JWT** for the e2e journeys that are gated on a
Clerk *session* bearer (not a PAT):

| Env var                          | Gated journey                                            |
| -------------------------------- | ------------------------------------------------------- |
| `CORELINK_E2E_CLERK_SESSION`     | `billing.rs` — authed checkout / `tier-select`          |
| `CORELINK_E2E_DSR_SESSION`       | `dsr.rs` — customer self-erasure request → gone         |
| `CORELINK_E2E_DSR_TEST_SESSION`  | `dsr.rs` — self-driving full erasure flow               |
| `CORELINK_E2E_DSR_TEST_TENANT`   | (best-effort) the resolved throwaway tenant id          |

## Why a headless browser?

A Clerk session JWT **cannot be minted purely server-side** — Clerk's anti-fraud
/ bot-detection blocks programmatic sign-in. The sanctioned automation path
([clerk.com/docs/testing](https://clerk.com/docs/testing/overview)) is a
**Testing Token**: a Backend-API token that, attached to Frontend-API requests,
tells Clerk "this is an authorized automated test" and skips bot detection. This
script:

1. Creates a throwaway Clerk user (Backend API, verified email + known password).
2. Mints a Testing Token (Backend API).
3. Drives a **headless Chromium** sign-in on the live app origin
   (`corelink-app.humangr.com`), attaching the testing token to every FAPI
   request so bot-detection is bypassed.
4. Harvests the session JWT via `window.Clerk.session.getToken()` — the same
   `__session` JWT the dashboard sends. It carries `azp =
   https://corelink-app.humangr.com` and `iss = <FAPI issuer>`, exactly what
   `worker/src/lib/clerk_auth.ts` (`verifyClerkSessionAndResolveTenant`) requires.

## Run it

```bash
cd scripts/e2e-real-client
npm i                              # installs playwright (devDependency)
npx playwright install chromium    # one-time: download the Chromium binary

# Source your live secrets first (CLERK_SECRET_KEY etc. live ONLY in .env.local):
set -a; source ../../.env.local; set +a

# Mint (writes /tmp/e2e-clerk-session.sh and prints the exports to stdout):
node clerk-session-minter.mjs
# …then load them into the e2e shell:
source /tmp/e2e-clerk-session.sh

# Or in one shot:
eval "$(node clerk-session-minter.mjs)"
```

Custom output file: `node clerk-session-minter.mjs /path/to/session.sh`.

## Environment it reads

| Var                            | Required | Default                              | Purpose                                                             |
| ------------------------------ | -------- | ------------------------------------ | ------------------------------------------------------------------ |
| `CLERK_SECRET_KEY`             | yes\*    | (also accepts `CLERK_LIVE_SECRET_KEY`) | Backend API auth (create user + testing token)                   |
| `CLERK_PUBLISHABLE_KEY`        | recommended | —                                 | Derive the FAPI host; ClerkJS bootstrap                            |
| `CORELINK_APP_URL`             | no       | `https://corelink-app.humangr.com`   | Sign-in origin → becomes the JWT `azp` (must be in the allowlist)  |
| `CLERK_FAPI`                   | no       | `clerk.corelink-app.humangr.com`     | FAPI host the testing token is attached to                         |
| `CORELINK_API_ENDPOINT`        | no       | —                                    | Best-effort tenant resolution (also reads `CORELINK_E2E_ENDPOINT`) |
| `CORELINK_E2E_EMAIL_DOMAIN`    | no       | `corelink-e2e.dev`                   | Domain for the throwaway user's email                             |
| `CORELINK_E2E_HEADFUL`         | no       | (headless)                           | `1` to watch the browser locally                                  |

\* either `CLERK_SECRET_KEY` or `CLERK_LIVE_SECRET_KEY`.

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

A Clerk session JWT is short-lived (~60s). Mint it **immediately before** the
e2e run, or refresh it:

```bash
node clerk-session-minter.mjs --refresh      # re-signs in the SAME user, fresh JWT
```

`--refresh` reuses the user recorded in the sidecar (run once without `--refresh`
first).

## The #1 risk — testing-token bypass is not guaranteed

This is the documented mechanism, but its success against the **hardened LIVE**
Clerk instance is **not guaranteed** — the instance may have stricter fraud rules
or Testing Tokens may be disabled. The script handles this honestly:

- If `POST /v1/testing_tokens` fails → **fails loud** ("Testing Tokens require a
  test instance or testing enabled").
- If sign-in is still blocked despite the token → **fails loud** with the exact
  Clerk error code/message, and flags when it looks like a captcha/bot block.
- It **never prints a token it could not actually mint** — a caller can never be
  fooled into running journeys against a bad bearer.

If you hit the bot-detection wall on the live instance, enable Testing Tokens for
that instance (Clerk dashboard) or mint against the test instance.

## Cleanup (do not skip)

The throwaway user lingers in Clerk until deleted. After your run:

```bash
USER_ID=$(jq -r .userId /tmp/e2e-clerk-session.sh.user.json)
curl -s -X DELETE "https://api.clerk.com/v1/users/$USER_ID" \
  -H "Authorization: Bearer $CLERK_SECRET_KEY"
```

(Or invoke the DSR account-deletion flow against that user — the more
end-to-end cleanup.)
