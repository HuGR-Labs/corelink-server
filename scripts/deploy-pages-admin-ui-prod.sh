#!/usr/bin/env bash
# deploy-pages-admin-ui-prod.sh — Cloudflare Pages deploy runner for apps/admin-ui.
#
# Framework:   Next.js 15 + @cloudflare/next-on-pages 1.13.7
# Build cmd:   pnpm run build:cf  (= next-on-pages)
#              next-on-pages runs `next build` then transforms the output to
#              CF Pages format at .vercel/output/static/
# Output dir:  apps/admin-ui/.vercel/output/static/
# CF project:  corelink-admin-ui
# Custom domain: app.corelink.humangr.com  (set post-apply via CF dashboard)
#
# Usage:
#   bash scripts/deploy-pages-admin-ui-prod.sh              # dry-run (default)
#   bash scripts/deploy-pages-admin-ui-prod.sh --apply      # actually deploy
#   bash scripts/deploy-pages-admin-ui-prod.sh --skip-build # dry-run, skip build
#
# Spec:  specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md §4 Phase F
# Audit: specs/_audits/sealed/2026-05-26-w32-phaseF-prep.md
#
# Charter compliance:
#   CTRL-CRED-001  — NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY is OK in client bundle
#                    (publishable by design).  CLERK_SECRET_KEY must NEVER be
#                    baked in.  This script asserts both before build.
#   ADR-0015       — SOURCE_DATE_EPOCH set for reproducible builds.
#   Hard pause #5  — token scope gate enforced before any apply attempt.

set -euo pipefail

# ---------------------------------------------------------------------------
# Script setup
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
export REPO_ROOT

# Source shared helpers.
# shellcheck source=scripts/_pages-deploy-common.sh
source "${SCRIPT_DIR}/_pages-deploy-common.sh"

# ---------------------------------------------------------------------------
# App-specific config
# ---------------------------------------------------------------------------
APP_DIR="${REPO_ROOT}/apps/admin-ui"
BUILD_CMD="pnpm run build:cf"
DIST_DIR="${APP_DIR}/.vercel/output/static"
CF_PROJECT="corelink-admin-ui"
CF_ENV="prod"
VERIFY_FQDN="corelink-admin-ui.pages.dev"   # pages.dev subdomain (pre-DNS-cutover)

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
MODE="--dry-run"
SKIP_BUILD=false

for arg in "$@"; do
  case "${arg}" in
    --apply)       MODE="--apply"   ;;
    --skip-build)  SKIP_BUILD=true  ;;
    --dry-run)     MODE="--dry-run" ;;
    --help|-h)
      cat <<'EOF'
Usage: deploy-pages-admin-ui-prod.sh [OPTIONS]

Options:
  --dry-run      (default) Print what would be deployed; do NOT call wrangler deploy.
  --apply        Actually deploy to Cloudflare Pages.  Requires Pages:Edit token scope.
  --skip-build   Skip the build step (use existing dist dir).
  --help         Show this message.

Required environment variables:
  CLOUDFLARE_API_TOKEN              — Must have Account:Cloudflare Pages:Edit for --apply.
  CLOUDFLARE_ACCOUNT_ID             — Account identifier.
  NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY — Clerk publishable key (safe for client bundle).

Required for Clerk runtime (NOT baked in at build — provided as CF Pages env vars):
  CLERK_SECRET_KEY                  — Server-side secret (never in bundle; CF Pages env).

Optional:
  SOURCE_DATE_EPOCH                 — Reproducible-build epoch (default: 1748217600).
  CSP_ENFORCEMENT                   — "enforce" or "report-only" (default: "enforce" in prod).

CTRL-CRED-001 enforcement:
  This script asserts that CLERK_SECRET_KEY is NOT set in the build environment
  (it must be injected as a CF Pages secret, not baked into the static bundle).
EOF
      exit 0
      ;;
    *)
      log_error "Unknown argument: ${arg}"
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# assert_clerk_cred_compliance — CTRL-CRED-001
#
# Verifies:
#   1. NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY is set (required for build).
#   2. CLERK_SECRET_KEY is NOT set in the build shell environment.
#      It is legitimately present in .env.local for local dev, but must be
#      injected as a CF Pages secret — not baked into the static export.
#
# The check is intentionally strict: if CLERK_SECRET_KEY leaks into the
# build subprocess env, next-on-pages could embed it in a server function.
# ---------------------------------------------------------------------------
assert_clerk_cred_compliance() {
  log_step "CTRL-CRED-001: Clerk credential compliance check..."

  # 1. Publishable key must be present.
  if [ -z "${NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY:-}" ]; then
    log_error "CTRL-CRED-001 FAIL: NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY is not set."
    log_error "This key is required for the admin-ui build."
    log_error "Set it in .env.local or export before running this script."
    return 1
  fi
  local pk="${NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY}"
  # Publishable keys start with pk_test_ or pk_live_
  if [[ "${pk}" != pk_test_* ]] && [[ "${pk}" != pk_live_* ]]; then
    log_warn "CTRL-CRED-001 WARNING: NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY does not start with pk_test_ or pk_live_."
    log_warn "Verify this is the correct Clerk publishable key."
  fi
  log_info "CTRL-CRED-001: NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY present (prefix: ${pk:0:10}...)."

  # 2. Secret key must NOT be in the build environment.
  #    We unset it before calling build_app to prevent any accidental embedding.
  if [ -n "${CLERK_SECRET_KEY:-}" ]; then
    log_warn "CTRL-CRED-001: CLERK_SECRET_KEY is set in current environment."
    log_warn "  Unsetting it before build to prevent bundle contamination."
    log_warn "  It must be set as a CF Pages secret via: wrangler pages secret put CLERK_SECRET_KEY"
    unset CLERK_SECRET_KEY
  fi
  log_info "CTRL-CRED-001: CLERK_SECRET_KEY will NOT be baked into the build."

  log_info "CTRL-CRED-001: PASS."
}

# ---------------------------------------------------------------------------
# assert_nextjs_pages_mode — next.config.ts must not set output: 'export'
# (next-on-pages manages its own output; standalone export is incompatible)
# ---------------------------------------------------------------------------
assert_nextjs_pages_mode() {
  local config_file="${APP_DIR}/next.config.ts"
  if grep -q "output.*export" "${config_file}" 2>/dev/null; then
    log_error "next.config.ts contains 'output: export' which is incompatible with next-on-pages."
    log_error "Remove the output config — next-on-pages produces .vercel/output/static."
    return 1
  fi
  log_info "next.config.ts: no 'output: export' — compatible with next-on-pages."
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  log_step "=== deploy-pages-admin-ui-prod.sh ==="
  log_info "Mode:     ${MODE}"
  log_info "App:      ${APP_DIR}"
  log_info "Dist dir: ${DIST_DIR}"
  log_info "Project:  ${CF_PROJECT}"

  # Step 1: Load credentials.
  # Load root .env.local (CF creds) first, then app-level .env.local (Clerk keys).
  load_env_local
  # App-level .env.local overrides are expected; load if present.
  local app_env="${APP_DIR}/.env.local"
  if [ -f "${app_env}" ]; then
    set -a
    # shellcheck disable=SC1090
    source "${app_env}"
    set +a
    log_info "Loaded app env from ${app_env}"
  else
    log_warn "No ${app_env} found."
    log_warn "Set NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY in .env.local or as CF Pages env var."
    log_warn "(For CI/CD, this key should be injected as a CF Pages environment variable, not a secret.)"
  fi

  # Step 2: Verify wrangler.
  verify_wrangler

  # Step 3: CF token scope check (dry-run = warn, apply = block).
  if [ "${MODE}" = "--dry-run" ]; then
    log_step "Dry-run token scope probe (non-blocking):"
    if verify_token_pages_scope 2>&1; then
      log_info "Token scope: Pages:Edit PRESENT — apply would proceed."
    else
      log_warn "Token scope: Pages:Edit MISSING — apply is blocked (expected; see audit §3)."
      log_warn "This is a documented hard gate, not an error in dry-run mode."
    fi
  fi

  # Step 4: CTRL-CRED-001 compliance (always enforced, even in dry-run).
  assert_clerk_cred_compliance

  # Step 5: Verify next.config.ts compatibility.
  assert_nextjs_pages_mode

  # Step 6: Build.
  if [ "${SKIP_BUILD}" = "false" ]; then
    # Pass Clerk publishable key as build-time env var.
    # CLERK_SECRET_KEY is intentionally EXCLUDED (unset by assert_clerk_cred_compliance).
    build_app "${APP_DIR}" \
      "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=${NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY:-} ${BUILD_CMD}"
  else
    log_warn "--skip-build: using existing dist dir."
  fi

  # Step 7: Dist stats + SHA-256.
  log_step "Dist-dir stats:"
  dist_stats "${DIST_DIR}"

  log_step "Dist-dir SHA-256 (ADR-0015 baseline):"
  local sha256
  sha256=$(compute_dist_sha256 "${DIST_DIR}")
  log_info "  sha256: ${sha256}"

  # Step 8: Deploy (or dry-run).
  deploy_pages_project "${CF_PROJECT}" "${DIST_DIR}" "${CF_ENV}" "${MODE}"

  # Step 9: Post-apply verification.
  if [ "${MODE}" = "--apply" ]; then
    verify_deploy "${VERIFY_FQDN}"
    log_info ""
    log_info "Post-deploy checklist (manual, operator):"
    log_info "  [ ] CF Pages dashboard: add custom domain app.corelink.humangr.com"
    log_info "  [ ] Add CF Pages secrets (NOT in bundle; server-side only):"
    log_info "        wrangler pages secret put CLERK_SECRET_KEY --project-name ${CF_PROJECT}"
    log_info "        wrangler pages secret put STRIPE_SECRET_KEY --project-name ${CF_PROJECT}"
    log_info "  [ ] Phase G DNS: CNAME app.corelink.humangr.com -> corelink-admin-ui.pages.dev"
    log_info "  [ ] Verify https://app.corelink.humangr.com returns HTTP 200"
    log_info "  [ ] Verify Clerk sign-in flow functional (test user)"
  fi

  log_step "=== deploy-pages-admin-ui-prod.sh DONE ==="
}

main "$@"
