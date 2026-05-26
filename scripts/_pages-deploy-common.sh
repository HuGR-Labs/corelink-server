#!/usr/bin/env bash
# _pages-deploy-common.sh — shared helpers for Cloudflare Pages deploy scripts.
#
# Sourced by:
#   deploy-pages-docs-prod.sh
#   deploy-pages-admin-ui-prod.sh
#
# DO NOT execute directly.
#
# Charter: CTRL-CRED-001 — CLERK_SECRET_KEY must never appear in client bundle.
#          ADR-0015       — reproducible builds via SOURCE_DATE_EPOCH.
#
# Spec ref: specs/_audits/2026-05-22-wave32-prod-deploy-spec.md §3 Phase F
# Audit:    specs/_audits/2026-05-26-w32-phaseF-prep.md

set -euo pipefail

# ---------------------------------------------------------------------------
# Globals (override via environment before sourcing)
# ---------------------------------------------------------------------------
REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
WRANGLER="${WRANGLER:-${REPO_ROOT}/node_modules/.bin/wrangler}"

# Reproducible-build epoch (ADR-0015).  Callers may override; default to a
# fixed epoch so CI produces byte-for-byte identical output.
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1748217600}"   # 2026-05-26T00:00:00Z
export SOURCE_DATE_EPOCH

# ---------------------------------------------------------------------------
# Colour helpers (no-op if not a TTY)
# ---------------------------------------------------------------------------
_RED=""
_GRN=""
_YLW=""
_BLD=""
_RST=""
if [ -t 1 ]; then
  _RED="\033[0;31m"
  _GRN="\033[0;32m"
  _YLW="\033[0;33m"
  _BLD="\033[1m"
  _RST="\033[0m"
fi

log_info()  { printf "${_GRN}[INFO]${_RST}  %s\n" "$*"; }
log_warn()  { printf "${_YLW}[WARN]${_RST}  %s\n" "$*"; }
log_error() { printf "${_RED}[ERROR]${_RST} %s\n" "$*" >&2; }
log_step()  { printf "${_BLD}[STEP]${_RST}  %s\n" "$*"; }

# ---------------------------------------------------------------------------
# verify_wrangler — ensure wrangler binary exists
# ---------------------------------------------------------------------------
verify_wrangler() {
  log_step "Verifying wrangler installation..."
  if [ ! -x "${WRANGLER}" ]; then
    log_error "wrangler not found at: ${WRANGLER}"
    log_error "Run: pnpm install  (from repo root) to install devDependencies."
    return 1
  fi
  local ver
  ver=$("${WRANGLER}" --version 2>&1 | head -1)
  log_info "wrangler found: ${ver}"
}

# ---------------------------------------------------------------------------
# verify_token_pages_scope — probe CF Pages API and gate on Pages:Edit scope.
#
# The current production token (CLOUDFLARE_API_TOKEN in .env.local) has:
#   Account: Workers Scripts / KV / R2 / D1 Edit
#   Zone:    DNS / Workers Routes Edit + Zone Read (humangr.com)
# It is MISSING:
#   Account: Cloudflare Pages: Edit
#
# This function will EXIT 1 with a clear message if the scope is absent.
# Phase F APPLY is gated on this function returning 0.
#
# Token-bump options (operator decision):
#   A) Edit the existing token in CF dashboard and add "Cloudflare Pages: Edit"
#      under Account Resources.
#   B) Mint a separate corelink-pages-deploy token with minimal scope
#      (Pages:Edit only) and export as CLOUDFLARE_API_TOKEN before running
#      the apply scripts.
# ---------------------------------------------------------------------------
verify_token_pages_scope() {
  local account_id="${CLOUDFLARE_ACCOUNT_ID:-}"
  local api_token="${CLOUDFLARE_API_TOKEN:-}"

  if [ -z "${api_token}" ]; then
    log_error "CLOUDFLARE_API_TOKEN is not set."
    log_error "Source .env.local before running this script."
    return 1
  fi
  if [ -z "${account_id}" ]; then
    log_error "CLOUDFLARE_ACCOUNT_ID is not set."
    return 1
  fi

  log_step "Verifying CF token has Pages:Edit scope..."

  # Probe the Pages projects list endpoint — requires Pages:Edit.
  local http_code
  http_code=$(curl -s -o /dev/null -w "%{http_code}" \
    -X GET "https://api.cloudflare.com/client/v4/accounts/${account_id}/pages/projects" \
    -H "Authorization: Bearer ${api_token}" \
    -H "Content-Type: application/json")

  if [ "${http_code}" = "200" ]; then
    log_info "Token scope OK: Pages:Edit confirmed (HTTP 200 on pages/projects)."
    return 0
  fi

  # Any non-200 → scope missing or auth error.
  log_error "HARD GATE TRIGGERED — CF token lacks 'Pages: Edit' scope."
  log_error "Pages/projects probe returned HTTP ${http_code}."
  log_error ""
  log_error "Phase F APPLY is blocked until the token is bumped."
  log_error "See spec §2 pre-flight: 'missing Pages: Edit — token bump required for Phase F'."
  log_error ""
  log_error "Remediation (operator action required):"
  log_error "  Option A: CF dashboard → My Profile → API Tokens → Edit existing token"
  log_error "            → Add: Account | Cloudflare Pages | Edit"
  log_error "  Option B: Mint a new token with Pages:Edit and export as CLOUDFLARE_API_TOKEN"
  log_error ""
  log_error "After token bump, re-run with --apply to proceed."
  return 1
}

# ---------------------------------------------------------------------------
# build_app <app_path> <build_cmd> [env_vars...]
#
# Runs the build command inside app_path with SOURCE_DATE_EPOCH set.
# Passes any extra VAR=value pairs as environment for the subprocess.
# ---------------------------------------------------------------------------
build_app() {
  local app_path="$1"
  local build_cmd="$2"
  shift 2
  local extra_env=("$@")

  if [ ! -d "${app_path}" ]; then
    log_error "App directory not found: ${app_path}"
    return 1
  fi

  log_step "Building ${app_path} ..."
  log_info "  command:          ${build_cmd}"
  log_info "  SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH}"

  # Export extra env vars for the build subprocess.
  local env_prefix=""
  for kv in "${extra_env[@]:-}"; do
    env_prefix="${env_prefix}${kv} "
  done

  (
    cd "${app_path}"
    env SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH}" \
      bash -c "${env_prefix}${build_cmd}"
  )

  log_info "Build complete for ${app_path}."
}

# ---------------------------------------------------------------------------
# compute_dist_sha256 <dist_dir>
#
# Computes a deterministic SHA-256 over all files in dist_dir, sorted by path.
# Prints the hex digest.  Used for static-asset integrity baseline (ADR-0015).
# ---------------------------------------------------------------------------
compute_dist_sha256() {
  local dist_dir="$1"

  if [ ! -d "${dist_dir}" ]; then
    log_error "Dist directory not found: ${dist_dir}"
    return 1
  fi

  log_step "Computing dist-dir SHA-256 for: ${dist_dir}"

  local digest
  digest=$(find "${dist_dir}" -type f | sort | xargs sha256sum | sha256sum | awk '{print $1}')
  echo "${digest}"
}

# ---------------------------------------------------------------------------
# dist_stats <dist_dir>
#
# Prints file count + total size for dry-run output.
# ---------------------------------------------------------------------------
dist_stats() {
  local dist_dir="$1"

  if [ ! -d "${dist_dir}" ]; then
    log_warn "Dist directory not found: ${dist_dir} — was the build step skipped?"
    return 1
  fi

  local file_count total_size
  file_count=$(find "${dist_dir}" -type f | wc -l | tr -d ' ')
  total_size=$(du -sh "${dist_dir}" | awk '{print $1}')

  log_info "  dist dir:    ${dist_dir}"
  log_info "  file count:  ${file_count}"
  log_info "  total size:  ${total_size}"
}

# ---------------------------------------------------------------------------
# deploy_pages_project <project_name> <dist_dir> <cf_env> [--dry-run|--apply]
#
# Deploys dist_dir to the named Cloudflare Pages project.
# In dry-run mode: prints size + file count, does NOT call wrangler deploy.
# In apply mode:   calls wrangler pages deploy.
#
# Args:
#   project_name  — CF Pages project name (e.g. corelink-docs)
#   dist_dir      — local path to the built output directory
#   cf_env        — wrangler env name (prod)
#   mode          — "--dry-run" (default) or "--apply"
# ---------------------------------------------------------------------------
deploy_pages_project() {
  local project_name="$1"
  local dist_dir="$2"
  local cf_env="$3"
  local mode="${4:---dry-run}"

  log_step "Deploy Pages project: ${project_name}"
  log_info "  dist dir:    ${dist_dir}"
  log_info "  cf env:      ${cf_env}"
  log_info "  mode:        ${mode}"

  if [ ! -d "${dist_dir}" ]; then
    log_error "Dist directory not found: ${dist_dir}"
    log_error "Run the build step first."
    return 1
  fi

  dist_stats "${dist_dir}"

  if [ "${mode}" = "--dry-run" ]; then
    log_warn "DRY-RUN mode — wrangler deploy NOT called."
    log_warn "Pass --apply to actually deploy."
    log_info "Command that would run:"
    log_info "  ${WRANGLER} pages deploy ${dist_dir} \\"
    log_info "    --project-name ${project_name} \\"
    log_info "    --env ${cf_env}"
    return 0
  fi

  # --apply mode: verify token scope first, then deploy.
  verify_token_pages_scope

  log_step "Running wrangler pages deploy..."
  "${WRANGLER}" pages deploy "${dist_dir}" \
    --project-name "${project_name}" \
    --env "${cf_env}"

  log_info "Deploy complete for ${project_name}."
}

# ---------------------------------------------------------------------------
# verify_deploy <fqdn>
#
# POST-APPLY only: curls the deployed domain and asserts HTTP 200.
# ---------------------------------------------------------------------------
verify_deploy() {
  local fqdn="$1"

  log_step "Verifying deployment: https://${fqdn}"

  local http_code
  http_code=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time 30 \
    "https://${fqdn}")

  if [ "${http_code}" = "200" ]; then
    log_info "Verification OK: https://${fqdn} returned HTTP 200."
    return 0
  fi

  log_error "Verification FAILED: https://${fqdn} returned HTTP ${http_code}."
  log_error "Check CF Pages dashboard for build errors or DNS propagation."
  return 1
}

# ---------------------------------------------------------------------------
# load_env_local — source .env.local from repo root if not already loaded
# ---------------------------------------------------------------------------
load_env_local() {
  local env_file="${REPO_ROOT}/.env.local"
  if [ -f "${env_file}" ]; then
    # Export only lines that are KEY=VALUE (skip comments, blank lines).
    set -a
    # shellcheck disable=SC1090
    source "${env_file}"
    set +a
    log_info "Loaded env from ${env_file}"
  else
    log_warn ".env.local not found at ${env_file} — ensure CF credentials are set in environment."
  fi
}
