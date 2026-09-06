# smoke-install — end-to-end CI test for the install one-liner

This directory contains an end-to-end smoke test that exercises the full
install path advertised in our marketing/onboarding docs:

```sh
curl -fsSL https://corelink-get.humangr.com | sh -s -- --token="$CORELINK_TEST_TOKEN"
```

## What it tests

The test runs in a clean `debian:bookworm-slim` container (no Rust toolchain,
no pre-installed corelink binary, no auth state, only `curl` + CA certs) and
performs the full bootstrap:

1. `curl -fsSL $CORELINK_GET_URL | sh -s -- --token=$CORELINK_TEST_TOKEN`
   — fetches the install script from the Cloudflare Worker
2. The install script downloads the `corelink-cli` binary from GitHub Releases
3. `corelink --version` — confirms the binary executes
4. `corelink doctor` — confirms the binary can reach the API and perform its
   authenticated CAS write check

## When it runs

The workflow `.github/workflows/smoke-install.yml` runs:

- **On push to `main`** when anything under `apps/get-corelink-worker/**` or the
  workflow itself changes — catches regressions in the install Worker.
- **Daily at 06:00 UTC** (cron) — catches external regressions: CLI binary
  disappearing from GitHub Releases, `corelink-get.humangr.com` going down,
  Worker config drift, API endpoint outages, expired test PAT.
- **On `workflow_dispatch`** — manual re-run for debugging.

Hard timeout: 5 minutes.

## What it protects against

A failed run means one of the following is broken in production:

- The install Worker at `corelink-get.humangr.com` (HTTP error, malformed
  script, etc.)
- The `corelink-cli` GitHub Release artifact (missing, wrong arch, corrupted)
- The CoreLink API (`corelink doctor` cannot authenticate or reach the server)
- The CI PAT (`CORELINK_CANARY_PAT`, shared with `cas-canary`) has been revoked or expired

## Configuration

The workflow expects a GitHub Actions secret:

- **`CORELINK_CANARY_PAT`** — reused from `cas-canary.yml`; already bound, so
  there is nothing to mint. It is deliberately NOT low-privilege: it needs
  `cas:rw`, because `corelink doctor`'s storage_write check performs a real
  production CAS PUT. (`CORELINK_TEST_TOKEN_CI` is retired and never existed.)
  Rotating it takes BOTH workflows red. It is set in the
  repo/org secrets after repo provisioning. **DO NOT** bake a real token
  into the Dockerfile or workflow YAML. There is no token baked into the image.
  The CI smoke receives only the `CORELINK_CANARY_PAT` secret. A generated
  token-shaped value is not a valid substitute because the installer invokes
  `corelink whoami` before returning.

## Running locally

```sh
docker build -f apps/get-corelink-worker/test/smoke-install.Dockerfile \
  -t corelink-smoke .

docker run --rm \
  -e CORELINK_TEST_TOKEN="$CORELINK_CANARY_PAT" \
  corelink-smoke \
  sh -c 'curl -fsSL "$CORELINK_GET_URL" | sh -s -- --token="$CORELINK_TEST_TOKEN" && corelink --version'
```

`CORELINK_CANARY_PAT` must be a real PAT: the installer invokes `corelink whoami`
before returning, so a generated or token-shaped value is not a valid smoke input.

To test against a staging Worker, also pass `-e CORELINK_GET_URL=https://...`.
