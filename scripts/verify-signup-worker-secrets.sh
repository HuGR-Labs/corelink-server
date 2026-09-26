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
#     - CORELINK_ERASE_AUTH_KEY — dedicated ≥32-char key for irreversible
#       /_internal/audit/drain; never falls back to the shared key
#     - STRIPE_WEBHOOK_SECRET — Stripe webhook signature (the LIVE money path)
#     - STRIPE_PRICE_ID_TEAM / _PRO / _STARTER — tier resolution
#     - EMAIL_HASH_SALT — shared identity pseudonymisation key
#     - DSR_DLQ_ALERT_ENDPOINT / DSR_DLQ_ALERT_AUTH_TOKEN — owner-approved
#       HTTPS critical-alert sink and its dedicated bearer credential; absence
#       would leave only a local log and is therefore blocked before deploy.
#     - DSR_DLQ_REDRIVE_AUTH_KEY — dedicated redrive authority credential;
#       absence keeps the authenticated recovery route unavailable.
#   The signup Worker is deployed separately from the root Worker, and the
#   root Worker has four regional production destinations. If any of these is
#   unset on one destination, signups /
#   Stripe webhooks fail SILENTLY (4xx at the Worker; Svix/Stripe stop
#   retrying) unless this gate stops the signup-worker deploy first.
#
#   This read-only name check is wired immediately before `wrangler deploy` in
#   `.github/workflows/signup-worker-deploy.yml`; it lists deployed secret NAMES
#   (never values) and fails that Worker deployment when a required name is
#   missing. It remains useful as the operator's manual pre-launch check. The
#   workflow is scoped to signup-worker, this script, and itself, so it cannot
#   false-fail an unrelated deployment.
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
  CORELINK_ERASE_AUTH_KEY
  STRIPE_WEBHOOK_SECRET
  STRIPE_PRICE_ID_TEAM
  STRIPE_PRICE_ID_PRO
  STRIPE_PRICE_ID_STARTER
  EMAIL_HASH_SALT
  DSR_DLQ_ALERT_ENDPOINT
  DSR_DLQ_ALERT_AUTH_TOKEN
  DSR_DLQ_REDRIVE_AUTH_KEY
)

# Six explicit deployment destinations must carry the same salt. Keeping this
# list explicit makes a newly-added production Worker fail review until its
# secret gate is wired, rather than silently inheriting the wrong environment.
DESTINATIONS=(
  "corelink-prod|wrangler.toml|prod"
  "corelink-prod-sam|wrangler.toml|prod-sam"
  "corelink-prod-lhr|wrangler.toml|prod-lhr"
  "corelink-prod-nrt|wrangler.toml|prod-nrt"
  "corelink-prod-syd|wrangler.toml|prod-syd"
  "corelink-signup-worker|apps/signup-worker/wrangler.toml|"
)

if [ ! -f "$SIGNUP_CONFIG" ]; then
  printf '%s fatal: signup-worker config not found at %s\n' "$LOG_PREFIX" "$SIGNUP_CONFIG" >&2
  exit 2
fi

WRANGLER_CMD="npx --yes wrangler@4"
if command -v wrangler >/dev/null 2>&1; then
  WRANGLER_CMD="wrangler"
fi

MISSING=0
for destination in "${DESTINATIONS[@]}"; do
  IFS='|' read -r worker config env_name <<< "$destination"
  config_path="$REPO_ROOT/$config"
  if [ ! -f "$config_path" ]; then
    printf '%s fatal: config for %s not found at %s\n' "$LOG_PREFIX" "$worker" "$config_path" >&2
    exit 2
  fi
  printf '%s listing deployed secrets for %s …\n' "$LOG_PREFIX" "$worker"
  if [ -n "$env_name" ]; then
    LIST_JSON="$($WRANGLER_CMD secret list --config "$config_path" --env "$env_name" 2>/dev/null)"
  else
    LIST_JSON="$($WRANGLER_CMD secret list --config "$config_path" 2>/dev/null)"
  fi
  if [ -z "$LIST_JSON" ]; then
    printf '%s fatal: could not list secrets for %s (wrangler auth? CLOUDFLARE_API_TOKEN / CLOUDFLARE_ACCOUNT_ID set?)\n' "$LOG_PREFIX" "$worker" >&2
    exit 2
  fi
  # The signup-worker owns the webhook/payment secret set. The five root and
  # regional Workers participate only in identity pseudonymisation here, so
  # requiring their unrelated signup secrets would make this gate false-fail.
  if [ "$worker" = "corelink-signup-worker" ]; then
    destination_required=("${REQUIRED[@]}")
  else
    destination_required=(EMAIL_HASH_SALT)
  fi
  for name in "${destination_required[@]}"; do
    if printf '%s' "$LIST_JSON" | grep "\"${name}\"" >/dev/null; then
      printf '  ok      %-24s (%s)\n' "$name" "$worker"
    else
      printf '  MISSING %-24s (%s)\n' "$name" "$worker"
      MISSING=$((MISSING + 1))
    fi
  done
done

if [ "$MISSING" -gt 0 ]; then
  printf '%s FAIL: %d required destination secret(s) missing. Set each with the destination config/env above.\n' "$LOG_PREFIX" "$MISSING" >&2
  exit 1
fi

printf '%s OK: signup-worker secrets and EMAIL_HASH_SALT on all six destinations are populated.\n' "$LOG_PREFIX"
exit 0
