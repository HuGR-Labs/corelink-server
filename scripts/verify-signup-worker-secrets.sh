#!/usr/bin/env bash
# verify-signup-worker-secrets.sh — pre-launch check for the
# corelink-signup-worker Worker's required secrets.
#
# WHY THIS EXISTS (brutal-audit B3):
#   The signup-worker (apps/signup-worker, Worker name
#   "corelink-signup-worker") owns the secrets that gate the ENTIRE
#   signup → provision → first-payment money path:
#     - CLERK_WEBHOOK_SECRET  — Svix signature on inbound Clerk webhooks
#     - CLERK_SECRET_KEY      — Clerk Backend API (writes tenant metadata)
#     - CORELINK_INTERNAL_AUTH_KEY — calls /_internal/pat/mint on the container
#     - STRIPE_WEBHOOK_SECRET — Stripe webhook signature (the LIVE money path)
#     - STRIPE_PRICE_ID_TEAM / _PRO / _STARTER — tier resolution
#   But this Worker is deployed SEPARATELY from the main worker: it is a
#   single-default-env Worker (`wrangler ... ` with NO `--env prod`), so it
#   is OUTSIDE the scope of scripts/put-secrets-prod.sh and the
#   cf-deploy-prod.yml secret gate (both target the main `--env prod`
#   worker). If any of these is unset on corelink-signup-worker, signups /
#   Stripe webhooks fail SILENTLY (4xx at the Worker; Svix/Stripe stop
#   retrying) — and nothing in CI catches it today.
#
#   This script is the operator's manual pre-launch gate. It is read-only:
#   it lists deployed secret NAMES via wrangler (never values) and reports
#   any required name that is missing. It is intentionally NOT wired into a
#   deploy workflow (the signup-worker has no deploy workflow yet) so it can
#   never false-fail an unrelated deploy.
#
# USAGE:
#   bash scripts/verify-signup-worker-secrets.sh            # report
#   CLOUDFLARE_API_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… bash scripts/verify-signup-worker-secrets.sh
#
# EXIT CODES:
#   0 — all required signup-worker secrets are populated
#   1 — one or more required secrets are missing
#   2 — invocation / tooling error (wrangler missing, config not found)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIGNUP_CONFIG="$REPO_ROOT/apps/signup-worker/wrangler.toml"
LOG_PREFIX="[verify-signup-worker-secrets]"

# Required SECRETS only (D1 bindings, [vars], and optional/degradable
# bindings such as ANALYTICS_* / SENTRY_* are deliberately excluded —
# their absence does not break the signup/payment path).
REQUIRED=(
  CLERK_WEBHOOK_SECRET
  CLERK_SECRET_KEY
  CORELINK_INTERNAL_AUTH_KEY
  STRIPE_WEBHOOK_SECRET
  STRIPE_PRICE_ID_TEAM
  STRIPE_PRICE_ID_PRO
  STRIPE_PRICE_ID_STARTER
)

if [ ! -f "$SIGNUP_CONFIG" ]; then
  printf '%s fatal: signup-worker config not found at %s\n' "$LOG_PREFIX" "$SIGNUP_CONFIG" >&2
  exit 2
fi

WRANGLER_CMD="npx --yes wrangler@4"
if command -v wrangler >/dev/null 2>&1; then
  WRANGLER_CMD="wrangler"
fi

printf '%s listing deployed secrets for corelink-signup-worker …\n' "$LOG_PREFIX"
LIST_JSON="$($WRANGLER_CMD secret list --config "$SIGNUP_CONFIG" 2>/dev/null)"
if [ -z "$LIST_JSON" ]; then
  printf '%s fatal: could not list secrets (wrangler auth? CLOUDFLARE_API_TOKEN / CLOUDFLARE_ACCOUNT_ID set?)\n' "$LOG_PREFIX" >&2
  exit 2
fi

MISSING=0
for name in "${REQUIRED[@]}"; do
  if printf '%s' "$LIST_JSON" | grep -q "\"${name}\""; then
    printf '  ok      %s\n' "$name"
  else
    printf '  MISSING %s\n' "$name"
    MISSING=$((MISSING + 1))
  fi
done

if [ "$MISSING" -gt 0 ]; then
  printf '%s FAIL: %d required signup-worker secret(s) missing. Set each with:\n' "$LOG_PREFIX" "$MISSING" >&2
  printf '       %s secret put <NAME> --config %s\n' "$WRANGLER_CMD" "$SIGNUP_CONFIG" >&2
  exit 1
fi

printf '%s OK: all %d required signup-worker secrets are populated.\n' "$LOG_PREFIX" "${#REQUIRED[@]}"
exit 0
