#!/usr/bin/env bash
# Run the explicitly selected real-network integration harnesses.
#
# This script is intentionally a small allow-list, not a generic cargo wrapper.
# It is called only by .github/workflows/real-ignored-harnesses.yml, which is a
# protected workflow_dispatch lane. In particular, the PAT seed harness is not
# an allowed profile: it prints a credential and seed SQL and must never run in
# CI, whether the caller has a secret or not.

set -euo pipefail

readonly SCRIPT_NAME="${0##*/}"
readonly PROFILE="${1:-}"
readonly CARGO_BIN="${CARGO_BIN:-cargo}"

usage() {
  printf 'usage: %s {d1|r2|stripe|neon|all}\n' "$SCRIPT_NAME" >&2
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 2
}

require_env() {
  local name value
  for name in "$@"; do
    value="${!name-}"
    [[ -n "$value" ]] || die "required environment variable is missing: $name"
  done
}

require_https() {
  local name="$1"
  [[ "${!name}" == https://* ]] || die "$name must use https://"
}

run_cargo() {
  # --locked makes the live lane execute the repository's resolved dependency
  # graph. --ignored is deliberately present only in this allow-listed runner.
  "$CARGO_BIN" test --locked "$@" -- --ignored --nocapture
}

preflight_d1() {
  # StorageEnv requires the complete native storage tuple even for D1 tests:
  # this prevents a test from silently taking an in-memory fallback.
  require_env \
    CLOUDFLARE_ACCOUNT_ID CF_API_TOKEN D1_DATABASE_ID \
    R2_S3_ENDPOINT R2_S3_ACCESS_KEY_ID R2_S3_SECRET_ACCESS_KEY
  require_https R2_S3_ENDPOINT
}

preflight_r2() {
  require_env \
    CLOUDFLARE_ACCOUNT_ID CF_API_TOKEN D1_DATABASE_ID \
    R2_S3_ENDPOINT R2_S3_ACCESS_KEY_ID R2_S3_SECRET_ACCESS_KEY \
    R2_TEST_BUCKET
  require_https R2_S3_ENDPOINT
}

preflight_stripe() {
  # The Stripe tests route through the HuGR wallet broker. Requiring the
  # test-only reference prevents a manually dispatched run from accidentally
  # pointing at a live-mode wallet credential.
  require_env \
    HUGR_WALLET_BASE HUGR_WALLET_TOKEN HUGR_STRIPE_REF \
    STRIPE_AUTH_MODE STRIPE_PRICE_ID_STARTER
  require_https HUGR_WALLET_BASE
  [[ "$HUGR_WALLET_TOKEN" == hugrw_* ]] || die "HUGR_WALLET_TOKEN must be a wallet-broker token"
  [[ "$HUGR_STRIPE_REF" == stripe-prod-test ]] || \
    die "HUGR_STRIPE_REF must be the provisioned Stripe test reference"
  [[ "$STRIPE_AUTH_MODE" == wallet-broker ]] || \
    die "STRIPE_AUTH_MODE must be wallet-broker for the real Stripe profile"
  [[ "$STRIPE_PRICE_ID_STARTER" == price_* ]] || \
    die "STRIPE_PRICE_ID_STARTER must be a Stripe price identifier"
}

preflight_neon() {
  require_env NEON_TEST_DSN
  [[ "$NEON_TEST_DSN" == postgresql://* ]] || die "NEON_TEST_DSN must be a PostgreSQL DSN"
}

run_d1() {
  run_cargo --package corelink-server --lib d1_acquire_lock_then_held_then_release
  run_cargo --package corelink-server --lib d1_dpa_and_active_subscription_reads
  run_cargo --package corelink-server --lib d1_persist_free_active_does_not_count_as_a_subscription
  run_cargo --package corelink-server --lib d1_http_cas_meta_round_trip
  run_cargo --package corelink-server --lib d1_http_tenant_admin_lookup_round_trip
  run_cargo --package corelink-server --lib d1_audit_write_blocking_records_oaudit_phase
}

run_r2() {
  run_cargo --package corelink-server --lib r2_cas_list_concurrent_path_fails_closed_on_bad_audit_creds
  run_cargo --package corelink-server --lib storage_r2_round_trip
  run_cargo --package corelink-server --lib cas_idempotent_rewrite_reports_durable_false
  run_cargo --package corelink-server --lib delete_if_present_credits_size_once_then_none
  run_cargo --package corelink-server --lib r2_cas_exists_batch_fails_closed_on_bad_audit_creds
}

run_stripe() {
  run_cargo --package corelink-stripe-real --features live-integration --test live_integration
}

run_neon() {
  # The five ignored tests are compiled only with neon-real; no PAT or D1
  # credential is inherited by this profile.
  run_cargo --package corelink-audit-chain --features neon-real --test neon_shadow_real
}

case "$PROFILE" in
  d1) preflight_d1; run_d1 ;;
  r2) preflight_r2; run_r2 ;;
  stripe) preflight_stripe; run_stripe ;;
  neon) preflight_neon; run_neon ;;
  all)
    # IMPORTANT: every preflight is side-effect-free. Keep this complete block
    # before the first run_* call: a missing late credential must not allow an
    # earlier real D1/R2/Stripe mutation to happen.
    preflight_d1
    preflight_r2
    preflight_stripe
    preflight_neon
    run_d1
    run_r2
    run_stripe
    run_neon
    ;;
  *)
    usage
    # An empty/unknown selector must never fall through to a broad
    # `cargo test --ignored` invocation. In particular, `seed` is not a
    # supported selector.
    die "unknown harness profile: ${PROFILE:-<empty>}"
    ;;
esac
