# Runbook — Runner cold-signup go-live (Option B, GitHub App 144561227)

**Task:** #68. **Owner-executed** (secret values live only in your GitHub App
settings; a Worker cannot self-set secrets). **Status of the CODE: DONE + live** —
this runbook is *config only*.

## TL;DR — what is and isn't done

The Option-B install→provision→resolve path is **fully built, tested, wired, and
proven-live once** (task #41: `acquire 200 Held`, d863fafb). There is **no code to
write.** What remains is a GitHub-App-settings + secret-binding step to flip the App
from **org-only dogfood** to **public cold self-serve**.

| Piece | State |
|---|---|
| Install callback (identity-gated, OAuth ownership proof) `github_install_callback.ts` | ✅ built, 28/28 tests |
| Shared idempotent write `writeInstallationProvision` (map + allowlist) | ✅ built |
| Internal provisioning primitive `POST /internal/v1/runner/provision-installation` | ✅ built + wired (signup-worker) |
| Runner-mint resolve side `handleRunnerMint` step 5a (installation_id → `tenant_gh_installation_map`) | ✅ built + wired (`worker/src/index.ts:885`), frozen 2026-07-08 (#674) |
| App identity secrets `GITHUB_APP_ID` / `_PRIVATE_KEY` / `_WEBHOOK_SECRET` / `_SETUP_TOKEN` / `INSTALL_STATE_SIGNING_KEY` | ✅ **bound** on `corelink-signup-worker` |
| OAuth ownership-proof creds `GITHUB_APP_CLIENT_ID` / `_CLIENT_SECRET` | ❌ **NOT bound** ← the gap |
| `GITHUB_APP_PUBLIC` flag + App toggled public | ❌ not set (App is still private/org-only) |
| admin-ui "Install" button vars (`GITHUB_APP_SLUG`, `ADMIN_UI_PUBLIC_URL`, `INSTALL_STATE_SIGNING_KEY` mirror) | ❌ unverified/unset |

## Why the current state is *safe*, not broken

With the OAuth creds **unbound**, the install callback **skips** the ownership
proof — but that path is `public:false` **org-only** (only a HuGR-Labs org
member can install), so the dogfood is unaffected and no stranger can bind an
installation. A **structural guard** (`github_install_callback.ts:283-290`) makes
the proof **mandatory** the moment `GITHUB_APP_PUBLIC` is truthy: a public App with
unbound OAuth creds is a hard `403`. **⇒ Order matters: bind the creds BEFORE you
toggle public**, or every cold install 403s (fail-closed, but non-functional).

## Steps (owner)

1. **GitHub App 144561227 settings** (github.com → the org's Developer settings → the App):
   - Enable **"Request user authorization (OAuth) during installation"**.
   - **Generate a client secret.** Copy the **Client ID** and **Client secret** (shown once).
   - Confirm **Setup URL** = `https://corelink-signup.humangr.com/install/github/callback`
     and **"Redirect on update"** is on (so re-installs re-provision).
2. **Bind the two OAuth secrets** (use `printf`, never `echo` — a trailing `\n`
   breaks the value):
   ```sh
   cd apps/signup-worker
   printf '%s' "<CLIENT_ID>"     | node_modules/.bin/wrangler secret put GITHUB_APP_CLIENT_ID     --name corelink-signup-worker
   printf '%s' "<CLIENT_SECRET>" | node_modules/.bin/wrangler secret put GITHUB_APP_CLIENT_SECRET --name corelink-signup-worker
   ```
3. **Set `GITHUB_APP_PUBLIC=true`** (a `[vars]` entry in `apps/signup-worker/wrangler.toml`,
   then `wrangler deploy`, OR a secret) **and toggle the App to Public** in GitHub App settings.
4. **admin-ui "Install" button** (separate deploy unit): bind `GITHUB_APP_SLUG`
   (the App's slug), `ADMIN_UI_PUBLIC_URL` (`https://humangr.com`), and the
   `INSTALL_STATE_SIGNING_KEY` **mirror** (byte-identical to the signup-worker's —
   a cross-deployable test pins them equal). Without these the button `503`s.

## Prove it live (server TL drives, once step 2–3 land)

A **cold** tenant installs the App on their repo → `setup_url` callback verifies the
signed state (tenant identity) + the OAuth ownership proof (they control the
installation) → `writeInstallationProvision` seeds `tenant_gh_installation_map`
(installation→tenant) + `runner_repo_allowlist`. Then a runner job hits
`POST /internal/v1/runner/mint` with `installation_id=144561227` →
`handleRunnerMint` resolves tenant via the map → allowlist + entitlement gates → a
short-TTL `cas:rw` PAT. Acceptance = `acquire 200 Held` on a tenant that installed
**itself** (no manual seed).

## Faster dogfood proof (NO config needed — server TL can drive now)

To prove the **resolve** chain without waiting on step 2–3, seed the map directly
via the internal-auth primitive (the Option-A trigger of the *same* shared write):
`POST https://corelink-signup.humangr.com/internal/v1/runner/provision-installation`
`Authorization: Bearer $CORELINK_INTERNAL_AUTH_KEY`
`{ "installation_id": "144561227", "tenant_id": "<tenant>", "repositories": ["HuGR-Labs/corelink-cold-organic-e2e"] }`
→ then runners drives the mint with that `installation_id`. This proves
map→allowlist→entitlement→mint end-to-end; only the *self-serve public install UX*
still needs step 2–3. (Needs the runners TL to name `<tenant>`.)
