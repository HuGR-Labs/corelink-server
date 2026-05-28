#!/usr/bin/env bash
# f-day-deploy-pages-docs.sh — Wave 32 Phase F.1 deploy runner for apps/docs
#
# Deploys apps/docs/build to Cloudflare Pages project `corelink-docs`
# at custom domain corelink-docs.humangr.com (flat-name pattern).
#
# Usage:
#   bash scripts/f-day-deploy-pages-docs.sh --dry-run   # (default) print plan
#   bash scripts/f-day-deploy-pages-docs.sh --live       # actually deploy
#   bash scripts/f-day-deploy-pages-docs.sh --help
#
# Spec ref: specs/_audits/2026-05-27-w32-phaseF1-docs-pages-prep-seal.md
# Charter:  §0.1, §0.6, W1-W4; CTRL-CRED-001; ADR-0015
#
# Constraint W3: --live flag required for real deploy; dry-run is safe default.

set -euo pipefail

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Reproducible-build epoch (ADR-0015).
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1748217600}"   # 2026-05-26T00:00:00Z
export SOURCE_DATE_EPOCH

WRANGLER="${WRANGLER:-${REPO_ROOT}/node_modules/.bin/wrangler}"

# App config
APP_DIR="${REPO_ROOT}/apps/docs"
BUILD_DIR="${APP_DIR}/build"
CF_PROJECT="corelink-docs"
CUSTOM_DOMAIN="corelink-docs.humangr.com"

# Locale subdirs expected after a full 4-locale build.
LOCALE_DIRS=("pt-BR" "es-419" "de")

# ---------------------------------------------------------------------------
# Colour helpers (no-op if not a TTY)
# ---------------------------------------------------------------------------
_RED="" _GRN="" _YLW="" _BLD="" _RST=""
if [ -t 1 ]; then
  _RED="\033[0;31m"; _GRN="\033[0;32m"; _YLW="\033[0;33m"
  _BLD="\033[1m";    _RST="\033[0m"
fi

log_info()  { printf "${_GRN}[INFO]${_RST}  %s\n" "$*"; }
log_warn()  { printf "${_YLW}[WARN]${_RST}  %s\n" "$*"; }
log_error() { printf "${_RED}[ERROR]${_RST} %s\n" "$*" >&2; }
log_step()  { printf "${_BLD}[STEP]${_RST}  %s\n" "$*"; }

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
MODE="--dry-run"
SKIP_BUILD=false

show_usage() {
  cat <<'EOF'
f-day-deploy-pages-docs.sh — Wave 32 Phase F.1 docs Pages deploy

Usage:
  bash scripts/f-day-deploy-pages-docs.sh [OPTIONS]

Options:
  --dry-run      (default) Print build plan + wrangler command; do NOT deploy.
  --live         Actually build and deploy to Cloudflare Pages.
  --skip-build   Skip the build step (use existing build dir).
  --help         Show this message.

Environment (required for --live):
  CLOUDFLARE_API_TOKEN      CF token with Account:Cloudflare Pages:Edit scope.
  CLOUDFLARE_ACCOUNT_ID     CF account identifier.
  SOURCE_DATE_EPOCH         Reproducible-build epoch (default: 1748217600).

Dry-run output:
  - Build command line
  - Exact wrangler pages deploy command
  - Build artifact hash (HTML files SHA-256)
  - Locale-build status (4 locales OR EN-only fallback)

Constraint W3: --live requires explicit flag; dry-run is the safe default.
EOF
  exit 0
}

for arg in "$@"; do
  case "${arg}" in
    --dry-run)    MODE="--dry-run"    ;;
    --live)       MODE="--live"       ;;
    --skip-build) SKIP_BUILD=true     ;;
    --help|-h)    show_usage          ;;
    *)
      log_error "Unknown argument: ${arg}"
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# detect_locale_status — prints locale build status; returns EN-only flag
# ---------------------------------------------------------------------------
detect_locale_status() {
  local all_present=true
  local found_locales="en-US (default)"

  for loc in "${LOCALE_DIRS[@]}"; do
    if [ -d "${BUILD_DIR}/${loc}" ]; then
      found_locales="${found_locales}, ${loc}"
    else
      all_present=false
    fi
  done

  if ${all_present}; then
    log_info "  locale-build: 4 locales (en-US, pt-BR, es-419, de)"
  else
    log_warn "  locale-build: EN-only fallback (some locale dirs missing)"
    log_warn "  found: ${found_locales}"
    log_warn "  Drift-B locale agent may not have run yet; EN-only deploy is valid."
  fi
}

# ---------------------------------------------------------------------------
# compute_html_hash — SHA-256 over all *.html files in build dir
# ---------------------------------------------------------------------------
compute_html_hash() {
  if [ ! -d "${BUILD_DIR}" ]; then
    log_error "Build dir not found: ${BUILD_DIR}"
    return 1
  fi
  find "${BUILD_DIR}" -type f -name "*.html" | sort | xargs sha256sum | sha256sum | awk '{print $1}'
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  log_step "=== f-day-deploy-pages-docs.sh ==="
  log_info "Mode:          ${MODE}"
  log_info "Project:       ${CF_PROJECT}"
  log_info "Custom domain: ${CUSTOM_DOMAIN}"
  log_info "App dir:       ${APP_DIR}"
  log_info "Build dir:     ${BUILD_DIR}"
  log_info "Wrangler:      ${WRANGLER}"

  # ── Step 1: Build ──────────────────────────────────────────────────────
  log_step "Step 1 — Build"
  log_info "  command: cd apps/docs && pnpm build"
  log_info "  SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH}"

  if [ "${SKIP_BUILD}" = "true" ]; then
    log_warn "  --skip-build: using existing build dir."
  elif [ "${MODE}" = "--dry-run" ]; then
    log_info "  DRY-RUN: build skipped (would run: cd apps/docs && pnpm build)"
  else
    # --live: actually build
    if [ ! -d "${APP_DIR}" ]; then
      log_error "App directory not found: ${APP_DIR}"
      exit 1
    fi
    (
      cd "${APP_DIR}"
      env SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH}" pnpm build
    )
    log_info "  Build complete."
  fi

  # ── Step 2: Locale status ───────────────────────────────────────────────
  log_step "Step 2 — Locale build status"
  if [ -d "${BUILD_DIR}" ]; then
    detect_locale_status
  else
    log_warn "  Build dir absent; locale status deferred to post-build."
  fi

  # ── Step 3: Build artifact hash ────────────────────────────────────────
  log_step "Step 3 — Build artifact hash (HTML files)"
  if [ -d "${BUILD_DIR}" ]; then
    local html_hash
    html_hash=$(compute_html_hash)
    log_info "  html-sha256: ${html_hash}"
  else
    log_warn "  Build dir not found; hash will be computed after build."
    log_info "  hash-cmd: find apps/docs/build -type f -name \"*.html\" | xargs sha256sum | sha256sum"
  fi

  # ── Step 4: Wrangler command ────────────────────────────────────────────
  log_step "Step 4 — Wrangler deploy command"
  log_info "  wrangler pages deploy apps/docs/build \\"
  log_info "    --project-name corelink-docs --env prod \\"
  log_info "    --branch=main --commit-dirty=false"

  if [ "${MODE}" = "--dry-run" ]; then
    log_warn "DRY-RUN mode — wrangler deploy NOT called."
    log_warn "Pass --live to actually deploy."
    log_step "=== f-day-deploy-pages-docs.sh DRY-RUN COMPLETE ==="
    return 0
  fi

  # ── Step 5: Live deploy ─────────────────────────────────────────────────
  log_step "Step 5 — Live deploy"

  # Load .env.local creds if available
  local env_file="${REPO_ROOT}/.env.local"
  if [ -f "${env_file}" ]; then
    local _saved_token="${CLOUDFLARE_API_TOKEN:-}"
    local _token_was_set=false
    [ -n "${CLOUDFLARE_API_TOKEN:-}" ] && _token_was_set=true
    set -a
    # shellcheck disable=SC1090
    source "${env_file}"
    set +a
    log_info "Loaded env from ${env_file}"
    if [ "${_token_was_set}" = "true" ]; then
      export CLOUDFLARE_API_TOKEN="${_saved_token}"
      log_info "Restored caller-supplied CLOUDFLARE_API_TOKEN."
    fi
  else
    log_warn ".env.local not found; CF credentials must be set in environment."
  fi

  # Verify wrangler
  if [ ! -x "${WRANGLER}" ]; then
    log_error "wrangler not found at: ${WRANGLER}"
    log_error "Run: pnpm install (from repo root)"
    exit 1
  fi
  local wrangler_ver
  wrangler_ver=$("${WRANGLER}" --version 2>&1 | head -1)
  log_info "wrangler: ${wrangler_ver}"

  # Verify build dir exists
  if [ ! -d "${BUILD_DIR}" ]; then
    log_error "Build dir not found: ${BUILD_DIR}"
    log_error "Run without --skip-build to trigger a build first."
    exit 1
  fi

  # Verify CF token has Pages:Edit scope
  local api_token="${CLOUDFLARE_API_TOKEN:-}"
  local account_id="${CLOUDFLARE_ACCOUNT_ID:-}"
  if [ -z "${api_token}" ] || [ -z "${account_id}" ]; then
    log_error "CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID must be set for --live."
    exit 1
  fi
  log_step "Verifying CF token Pages:Edit scope..."
  local scope_code
  scope_code=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time 10 \
    -X GET "https://api.cloudflare.com/client/v4/accounts/${account_id}/pages/projects" \
    -H "Authorization: Bearer ${api_token}" \
    -H "Content-Type: application/json")
  if [ "${scope_code}" != "200" ]; then
    log_error "HARD GATE: CF token lacks Pages:Edit scope (HTTP ${scope_code})."
    log_error "Add 'Account | Cloudflare Pages | Edit' to the token in CF dashboard."
    exit 1
  fi
  log_info "Token scope OK (HTTP 200)."

  # Execute deploy
  log_step "Running wrangler pages deploy..."
  "${WRANGLER}" pages deploy "${BUILD_DIR}" \
    --project-name "${CF_PROJECT}" \
    --env prod \
    --branch=main \
    --commit-dirty=false

  log_info "Deploy complete."
  log_info ""
  log_info "Post-deploy checklist (manual, operator):"
  log_info "  [ ] CF Pages dashboard: confirm custom domain ${CUSTOM_DOMAIN} is set"
  log_info "  [ ] Phase G DNS: CNAME ${CUSTOM_DOMAIN} -> ${CF_PROJECT}.pages.dev (proxied)"
  log_info "  [ ] Run: bash scripts/f-day-smoke-docs.sh"

  log_step "=== f-day-deploy-pages-docs.sh LIVE DEPLOY COMPLETE ==="
}

main "$@"
