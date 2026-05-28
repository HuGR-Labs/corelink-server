#!/usr/bin/env bash
# f-day-deploy-pages-admin.sh — Wave 32 Phase F.2 D-day runner: deploy
# apps/admin-ui to Cloudflare Pages project `corelink-admin-ui`.
#
# Custom domain: corelink-app.humangr.com  (flat-name; Phase 0.J Clerk
# nodejs runtime fix already landed — /sign-up + /sign-in use nodejs runtime).
#
# Prerequisites (cross-ref WP-D.2):
#   CLERK_PUBLISHABLE_KEY          — (90d rotation; CF Pages env var, NOT secret)
#   CLERK_SECRET_KEY               — (90d rotation; CF Pages secret, server-side only)
#   STRIPE_SECRET_KEY              — (90d rotation; CF Pages secret)
#   RESEND_API_KEY                 — (Phase 1 newsletter; CF Pages secret)
#   RESEND_NEWSLETTER_AUDIENCE_ID  — Gustavo-manual provisioning; CF Pages secret
#   SENTRY_DSN                     — (deferred; no-op if absent; CF Pages env var)
#
# Usage:
#   bash scripts/f-day-deploy-pages-admin.sh --dry-run   # (default) print plan
#   bash scripts/f-day-deploy-pages-admin.sh --live      # actually deploy
#   bash scripts/f-day-deploy-pages-admin.sh --skip-build
#   bash scripts/f-day-deploy-pages-admin.sh --help
#
# Charter compliance:
#   CTRL-CRED-001  — CLERK_SECRET_KEY never baked in build bundle; must be
#                    injected as CF Pages secret.  Asserted before build.
#   ADR-0015       — SOURCE_DATE_EPOCH set for reproducible builds.
#   Hard pause #5  — CF token Pages:Edit scope checked before any --live run.
#
# Spec:  specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md §3 Phase F
# Audit: specs/_audits/2026-05-27-w32-phaseF2-admin-pages-prep-seal.md

set -euo pipefail

# ---------------------------------------------------------------------------
# Script setup
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
export REPO_ROOT

# Source shared helpers.
# shellcheck source=scripts/_pages-deploy-common.sh
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/_pages-deploy-common.sh"

# ---------------------------------------------------------------------------
# App-specific config
# ---------------------------------------------------------------------------
APP_DIR="${REPO_ROOT}/apps/admin-ui"
BUILD_CMD="pnpm build"
DIST_DIR="${APP_DIR}/.next"
CF_PROJECT="corelink-admin-ui"
# CF_ENV retained for documentation; wrangler pages deploy uses --branch, not --env.
# shellcheck disable=SC2034
CF_ENV="prod"
CF_BRANCH="main"
CUSTOM_DOMAIN="corelink-app.humangr.com"
PAGES_DEV_FQDN="corelink-admin-ui.pages.dev"

# Secrets required in CF Pages before live deploy (cross-ref WP-D.2).
# Format: "NAME:description:rotation_days:is_optional"
REQUIRED_PAGES_SECRETS=(
  "CLERK_PUBLISHABLE_KEY:Clerk publishable key (safe for client bundle; CF Pages env var):90:false"
  "CLERK_SECRET_KEY:Clerk server-side secret — must NOT be baked in bundle:90:false"
  "STRIPE_SECRET_KEY:Stripe live secret key:90:false"
  "RESEND_API_KEY:Resend email API key (Phase 1 newsletter):90:false"
  "RESEND_NEWSLETTER_AUDIENCE_ID:Resend newsletter audience ID (Gustavo-manual provisioning):0:false"
  "SENTRY_DSN:Sentry DSN for error tracking (deferred; no-op if absent):0:true"
)

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
MODE="--dry-run"
SKIP_BUILD=false

for arg in "$@"; do
  case "${arg}" in
    --live)        MODE="--live"    ;;
    --dry-run)     MODE="--dry-run" ;;
    --skip-build)  SKIP_BUILD=true  ;;
    --help|-h)
      cat <<'EOF'
Usage: f-day-deploy-pages-admin.sh [OPTIONS]

Options:
  --dry-run      (default) Print deploy plan + build hash; do NOT call wrangler deploy.
  --live         Actually build + deploy to Cloudflare Pages.
                 Requires Pages:Edit token scope (hard gate).
  --skip-build   Skip the build step (use existing .next dir).
  --help         Show this message.

Required environment (CF — auto-loaded from .env.local):
  CLOUDFLARE_API_TOKEN   — Must have Account:Cloudflare Pages:Edit for --live.
  CLOUDFLARE_ACCOUNT_ID  — Account identifier.

Required environment (Clerk build-time — auto-loaded from .env.local or
apps/admin-ui/.env.local):
  CLERK_PUBLISHABLE_KEY  — Clerk publishable key (safe for bundle; NEXT_PUBLIC_).

CF Pages secrets (injected post-deploy via wrangler pages secret put, NOT baked in):
  CLERK_SECRET_KEY               — Server-side secret (90d rotation).
  STRIPE_SECRET_KEY              — Stripe live key (90d rotation).
  RESEND_API_KEY                 — Resend email API key.
  RESEND_NEWSLETTER_AUDIENCE_ID  — Resend newsletter audience (Gustavo-manual).
  SENTRY_DSN                     — Sentry DSN (deferred; no-op if absent).

CTRL-CRED-001 enforcement:
  CLERK_SECRET_KEY is unset from the build environment before pnpm build runs.
  It must be injected as a CF Pages secret — NOT baked into the static bundle.

CF deploy command (executed with --live):
  wrangler pages deploy apps/admin-ui/.next \
    --project-name corelink-admin-ui --branch main

Post-live checklist:
  [ ] CF Pages dashboard: confirm deployment visible under corelink-admin-ui
  [ ] Add/verify CF Pages secrets (wrangler pages secret put <NAME> --project-name corelink-admin-ui):
        CLERK_SECRET_KEY, STRIPE_SECRET_KEY, RESEND_API_KEY,
        RESEND_NEWSLETTER_AUDIENCE_ID, SENTRY_DSN (optional)
  [ ] Phase G DNS: CNAME corelink-app.humangr.com -> corelink-admin-ui.pages.dev
  [ ] Run f-day-smoke-admin.sh to verify 4-URL smoke passes
EOF
      exit 0
      ;;
    *)
      log_error "Unknown argument: ${arg}"
      log_error "Run with --help for usage."
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# print_secrets_checklist — enumerate all 5 required CF Pages secrets.
# Cross-ref WP-D.2 (put-secrets-prod.sh / secrets-mvp-allowlist.txt).
# ---------------------------------------------------------------------------
print_secrets_checklist() {
  log_step "Pre-deploy CF Pages secrets checklist (cross-ref WP-D.2):"
  log_info "  Project: ${CF_PROJECT}"
  log_info "  Inject via: wrangler pages secret put <NAME> --project-name ${CF_PROJECT}"
  log_info ""

  local idx=1
  for entry in "${REQUIRED_PAGES_SECRETS[@]}"; do
    local name rotation is_optional description
    name="$(echo "${entry}" | cut -d: -f1)"
    description="$(echo "${entry}" | cut -d: -f2)"
    rotation="$(echo "${entry}" | cut -d: -f3)"
    is_optional="$(echo "${entry}" | cut -d: -f4)"

    local rotation_note=""
    if [ "${rotation}" -gt 0 ] 2>/dev/null; then
      rotation_note="  (${rotation}d rotation)"
    fi

    local optional_note=""
    if [ "${is_optional}" = "true" ]; then
      optional_note="  [OPTIONAL — no-op if absent]"
    fi

    log_info "  [${idx}] ${name}${rotation_note}${optional_note}"
    log_info "       ${description}"
    idx=$(( idx + 1 ))
  done

  log_info ""
  log_info "  Total required: 5 (CLERK_PUBLISHABLE_KEY, CLERK_SECRET_KEY,"
  log_info "    STRIPE_SECRET_KEY, RESEND_API_KEY, RESEND_NEWSLETTER_AUDIENCE_ID)"
  log_info "  Optional: 1 (SENTRY_DSN — deferred; no-op if absent)"
}

# ---------------------------------------------------------------------------
# assert_clerk_cred_compliance — CTRL-CRED-001
#
# 1. CLERK_PUBLISHABLE_KEY or NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY must be set.
# 2. CLERK_SECRET_KEY must NOT be present in the build environment.
# ---------------------------------------------------------------------------
assert_clerk_cred_compliance() {
  log_step "CTRL-CRED-001: Clerk credential compliance check..."

  # Accept either naming convention (Next.js NEXT_PUBLIC_ prefix or raw).
  local pk="${NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY:-${CLERK_PUBLISHABLE_KEY:-}}"
  if [ -z "${pk}" ]; then
    log_error "CTRL-CRED-001 FAIL: Neither CLERK_PUBLISHABLE_KEY nor"
    log_error "  NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY is set."
    log_error "  Set in .env.local or apps/admin-ui/.env.local before running."
    return 1
  fi
  if [[ "${pk}" != pk_test_* ]] && [[ "${pk}" != pk_live_* ]]; then
    log_warn "CTRL-CRED-001 WARNING: Clerk publishable key does not start"
    log_warn "  with pk_test_ or pk_live_. Verify the key is correct."
  fi
  log_info "CTRL-CRED-001: Clerk publishable key present (prefix: ${pk:0:10}...)."

  # CLERK_SECRET_KEY must NOT be in build env — unset if present.
  if [ -n "${CLERK_SECRET_KEY:-}" ]; then
    log_warn "CTRL-CRED-001: CLERK_SECRET_KEY detected in environment."
    log_warn "  Unsetting before build to prevent bundle contamination."
    log_warn "  Inject as CF Pages secret: wrangler pages secret put CLERK_SECRET_KEY"
    unset CLERK_SECRET_KEY
  fi
  log_info "CTRL-CRED-001: CLERK_SECRET_KEY will NOT be baked into the build."
  log_info "CTRL-CRED-001: PASS."
}

# ---------------------------------------------------------------------------
# compute_build_hash — deterministic SHA-256 over .next output for dry-run.
# Gracefully skips if .next does not exist (build step not yet run).
# ---------------------------------------------------------------------------
compute_build_hash() {
  if [ ! -d "${DIST_DIR}" ]; then
    log_warn "  .next dir not found — build hash unavailable (run without --skip-build)."
    echo "UNAVAILABLE"
    return 0
  fi
  local digest
  digest=$(find "${DIST_DIR}" -type f | sort | xargs sha256sum 2>/dev/null | sha256sum | awk '{print $1}')
  echo "${digest}"
}

# ---------------------------------------------------------------------------
# print_wrangler_command — emit the exact wrangler command for dry-run output.
# ---------------------------------------------------------------------------
print_wrangler_command() {
  log_step "Wrangler deploy command (would execute with --live):"
  log_info "  ${WRANGLER} pages deploy ${DIST_DIR} \\"
  log_info "    --project-name ${CF_PROJECT} --branch ${CF_BRANCH}"
  log_info ""
  log_info "  Equivalent to:"
  log_info "    wrangler pages deploy apps/admin-ui/.next \\"
  log_info "      --project-name corelink-admin-ui --branch main"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  log_step "=== f-day-deploy-pages-admin.sh ==="
  log_info "Mode:         ${MODE}"
  log_info "App dir:      ${APP_DIR}"
  log_info "Dist dir:     ${DIST_DIR}"
  log_info "CF project:   ${CF_PROJECT}"
  log_info "CF branch:    ${CF_BRANCH}"
  log_info "Custom domain: ${CUSTOM_DOMAIN}"

  # Step 1: Load credentials.
  load_env_local
  local app_env="${APP_DIR}/.env.local"
  if [ -f "${app_env}" ]; then
    set -a
    # shellcheck disable=SC1090
    source "${app_env}"
    set +a
    log_info "Loaded app env from ${app_env}"
  else
    log_warn "No ${app_env} found — ensure CLERK_PUBLISHABLE_KEY is set."
  fi

  # Step 2: Verify wrangler (blocking for --live; warn-only for --dry-run).
  if [ "${MODE}" = "--live" ]; then
    verify_wrangler
  else
    verify_wrangler 2>&1 || log_warn "wrangler not found — dry-run continues (required for --live)."
  fi

  # Step 3: Print pre-deploy secrets checklist (always; dry-run and live).
  print_secrets_checklist

  # Step 4: CTRL-CRED-001 compliance (blocking for --live; warn-only for --dry-run).
  if [ "${MODE}" = "--live" ]; then
    assert_clerk_cred_compliance
  else
    assert_clerk_cred_compliance 2>&1 || log_warn "CTRL-CRED-001: Clerk key not set — dry-run continues (required for --live)."
  fi

  # Step 5: CF token scope check.
  if [ "${MODE}" = "--dry-run" ]; then
    log_step "Dry-run token scope probe (non-blocking):"
    if verify_token_pages_scope 2>&1; then
      log_info "Token scope: Pages:Edit PRESENT — live deploy would proceed."
    else
      log_warn "Token scope: Pages:Edit MISSING — live deploy is blocked."
      log_warn "  See specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md §2 pre-flight."
    fi
  fi

  # Step 6: Build (skip in dry-run unless dist already exists; always build for --live).
  if [ "${SKIP_BUILD}" = "false" ] && [ "${MODE}" = "--live" ]; then
    log_step "Building admin-ui..."
    # Normalise publishable key as NEXT_PUBLIC_ prefix for Next.js.
    local pk="${NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY:-${CLERK_PUBLISHABLE_KEY:-}}"
    build_app "${APP_DIR}" \
      "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=${pk} ${BUILD_CMD}"
  elif [ "${SKIP_BUILD}" = "true" ]; then
    log_warn "--skip-build: using existing dist dir."
  else
    log_info "Dry-run mode: skipping build step (use --live to trigger build)."
  fi

  # Step 7: Dist stats + build hash.
  log_step "Dist-dir stats:"
  dist_stats "${DIST_DIR}"

  log_step "Build hash (ADR-0015 — deterministic SHA-256 over .next):"
  local build_hash
  build_hash=$(compute_build_hash)
  log_info "  build_hash: ${build_hash}"

  # Step 8: Print wrangler command (dry-run) or execute (live).
  if [ "${MODE}" = "--dry-run" ]; then
    print_wrangler_command
    log_warn "DRY-RUN complete — no deploy executed. Pass --live to deploy."
  else
    # --live: verify token scope (blocking), then deploy.
    verify_token_pages_scope

    log_step "Running wrangler pages deploy..."
    "${WRANGLER}" pages deploy "${DIST_DIR}" \
      --project-name "${CF_PROJECT}" \
      --branch "${CF_BRANCH}"
    log_info "Deploy submitted to CF Pages project: ${CF_PROJECT}"

    # Step 9: Post-apply verification.
    log_step "Post-deploy verification (pages.dev subdomain)..."
    verify_deploy "${PAGES_DEV_FQDN}"

    log_info ""
    log_info "Post-deploy checklist (operator action required):"
    log_info "  [ ] CF Pages dashboard: confirm deployment visible under ${CF_PROJECT}"
    log_info "  [ ] Add/verify CF Pages secrets:"
    log_info "        wrangler pages secret put CLERK_SECRET_KEY --project-name ${CF_PROJECT}"
    log_info "        wrangler pages secret put STRIPE_SECRET_KEY --project-name ${CF_PROJECT}"
    log_info "        wrangler pages secret put RESEND_API_KEY --project-name ${CF_PROJECT}"
    log_info "        wrangler pages secret put RESEND_NEWSLETTER_AUDIENCE_ID --project-name ${CF_PROJECT}"
    log_info "        wrangler pages secret put SENTRY_DSN --project-name ${CF_PROJECT}  # optional"
    log_info "  [ ] Phase G DNS: CNAME ${CUSTOM_DOMAIN} -> ${PAGES_DEV_FQDN}"
    log_info "  [ ] Run: bash scripts/f-day-smoke-admin.sh"
    log_info "  [ ] Verify https://${CUSTOM_DOMAIN} returns HTTP 200"
  fi

  log_step "=== f-day-deploy-pages-admin.sh DONE ==="
}

main "$@"
