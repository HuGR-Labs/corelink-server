#!/usr/bin/env bash
# deploy-pages-docs-prod.sh — Cloudflare Pages deploy runner for apps/docs.
#
# Framework:   Docusaurus 3.10.1
# Build cmd:   pnpm run build  (= docusaurus build)
# Output dir:  apps/docs/build/
# CF project:  corelink-docs
# Custom domain: docs.corelink.humangr.com  (set post-apply via CF dashboard)
#
# Usage:
#   bash scripts/deploy-pages-docs-prod.sh              # dry-run (default)
#   bash scripts/deploy-pages-docs-prod.sh --apply      # actually deploy
#   bash scripts/deploy-pages-docs-prod.sh --skip-build # dry-run, skip build
#
# Spec:  specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md §4 Phase F
# Audit: specs/_audits/sealed/2026-05-26-w32-phaseF-prep.md
#
# Charter compliance:
#   CTRL-CRED-001  — Docusaurus has no server-side secrets; no leak surface.
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
APP_DIR="${REPO_ROOT}/apps/docs"
BUILD_CMD="pnpm run build"
DIST_DIR="${APP_DIR}/build"
CF_PROJECT="corelink-docs"
CF_ENV="prod"
VERIFY_FQDN="corelink-docs.pages.dev"   # pages.dev subdomain (pre-DNS-cutover)

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
Usage: deploy-pages-docs-prod.sh [OPTIONS]

Options:
  --dry-run      (default) Print what would be deployed; do NOT call wrangler deploy.
  --apply        Actually deploy to Cloudflare Pages.  Requires Pages:Edit token scope.
  --skip-build   Skip the build step (use existing dist dir).
  --help         Show this message.

Environment:
  CLOUDFLARE_API_TOKEN      — Must have Account:Cloudflare Pages:Edit scope for --apply.
  CLOUDFLARE_ACCOUNT_ID     — Account identifier.
  SOURCE_DATE_EPOCH         — Reproducible-build epoch (default: 1748217600).

Pre-flight checks:
  1. wrangler installed (repo-local node_modules/.bin/wrangler).
  2. CLOUDFLARE_API_TOKEN + CLOUDFLARE_ACCOUNT_ID set.
  3. CF token has Pages:Edit scope (BLOCKING for --apply; documented gate for dry-run).
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
# Main
# ---------------------------------------------------------------------------
main() {
  log_step "=== deploy-pages-docs-prod.sh ==="
  log_info "Mode:     ${MODE}"
  log_info "App:      ${APP_DIR}"
  log_info "Dist dir: ${DIST_DIR}"
  log_info "Project:  ${CF_PROJECT}"

  # Step 1: Load credentials.
  load_env_local

  # Step 2: Verify wrangler.
  verify_wrangler

  # Step 3: CF token scope check.
  #   In dry-run: we still probe and WARN (not exit) so the audit doc shows the gate state.
  #   In apply:   verify_token_pages_scope exits 1 on failure (enforced in deploy_pages_project).
  if [ "${MODE}" = "--dry-run" ]; then
    log_step "Dry-run token scope probe (non-blocking):"
    if verify_token_pages_scope 2>&1; then
      log_info "Token scope: Pages:Edit PRESENT — apply would proceed."
    else
      log_warn "Token scope: Pages:Edit MISSING — apply is blocked (expected; see audit §3)."
      log_warn "This is a documented hard gate, not an error in dry-run mode."
    fi
  fi

  # Step 4: Build.
  if [ "${SKIP_BUILD}" = "false" ]; then
    build_app "${APP_DIR}" "${BUILD_CMD}"
  else
    log_warn "--skip-build: using existing dist dir."
  fi

  # Step 5: Dist stats + SHA-256.
  log_step "Dist-dir stats:"
  dist_stats "${DIST_DIR}"

  log_step "Dist-dir SHA-256 (ADR-0015 baseline):"
  local sha256
  sha256=$(compute_dist_sha256 "${DIST_DIR}")
  log_info "  sha256: ${sha256}"

  # Step 6: Deploy (or dry-run).
  deploy_pages_project "${CF_PROJECT}" "${DIST_DIR}" "${CF_ENV}" "${MODE}"

  # Step 7: Verify (apply only).
  if [ "${MODE}" = "--apply" ]; then
    verify_deploy "${VERIFY_FQDN}"
    log_info ""
    log_info "Post-deploy checklist (manual, operator):"
    log_info "  [ ] CF dashboard: add custom domain docs.corelink.humangr.com"
    log_info "  [ ] Phase G DNS: CNAME docs.corelink.humangr.com -> corelink-docs.pages.dev"
    log_info "  [ ] Verify https://docs.corelink.humangr.com returns HTTP 200"
  fi

  log_step "=== deploy-pages-docs-prod.sh DONE ==="
}

main "$@"
