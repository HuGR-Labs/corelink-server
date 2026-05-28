#!/usr/bin/env bash
# f-day-smoke-docs.sh — Wave 32 Phase F.1 smoke test for corelink-docs.humangr.com
#
# Tests 5 URLs (plus locale fallback awareness) against the live docs deployment.
# Exits with the number of failures (0 = all green).
#
# Usage:
#   bash scripts/f-day-smoke-docs.sh          # live smoke
#   bash scripts/f-day-smoke-docs.sh --help   # show this help
#
# Spec ref: specs/_audits/2026-05-27-w32-phaseF1-docs-pages-prep-seal.md
# Charter:  §0.1, §0.6, W1-W4; CTRL-CRED-001 (no credentials used here)
#
# Constraint: curl --max-time 10 on all probes (avoid hangs).
# EN-only fallback: if build/pt-BR/ doesn't exist, locale URLs are skipped (WARN, not FAIL).

set -euo pipefail

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
BASE="https://corelink-docs.humangr.com"
TIMEOUT=10   # seconds per curl call

# ---------------------------------------------------------------------------
# Colour helpers (no-op if not a TTY)
# ---------------------------------------------------------------------------
_RED="" _GRN="" _YLW="" _BLD="" _RST=""
if [ -t 1 ]; then
  _RED="\033[0;31m"; _GRN="\033[0;32m"; _YLW="\033[0;33m"
  _BLD="\033[1m";    _RST="\033[0m"
fi

# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------
FAIL_COUNT=0
PASS_COUNT=0
SKIP_COUNT=0
SMOKE_START=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
pass() {
  PASS_COUNT=$(( PASS_COUNT + 1 ))
  printf '%s[PASS]%s %s\n' "${_GRN}" "${_RST}" "$*"
}

fail() {
  FAIL_COUNT=$(( FAIL_COUNT + 1 ))
  printf '%s[FAIL]%s %s\n' "${_RED}" "${_RST}" "$*" >&2
}

# shellcheck disable=SC2329
warn() {
  printf '%s[WARN]%s %s\n' "${_YLW}" "${_RST}" "$*"
}

skip() {
  SKIP_COUNT=$(( SKIP_COUNT + 1 ))
  printf '%s[SKIP]%s %s\n' "${_YLW}" "${_RST}" "$*"
}

step() {
  printf '\n%s── %s --%s\n' "${_BLD}" "$*" "${_RST}"
}

# probe <name> <url> <expected_status> <content_grep>
# content_grep: literal string to grep for in body (empty string = skip content check)
probe() {
  local name="$1"
  local url="$2"
  local expected_status="$3"
  local content_grep="$4"

  local t0 elapsed http_code body
  t0=$(date +%s%3N 2>/dev/null || echo "0")

  http_code=$(curl -s -o /tmp/_smoke_body.tmp -w "%{http_code}" \
    --max-time "${TIMEOUT}" \
    --connect-timeout 8 \
    "${url}" 2>/dev/null || echo "000")
  body=$(cat /tmp/_smoke_body.tmp 2>/dev/null || echo "")

  elapsed=$(( $(date +%s%3N 2>/dev/null || echo "0") - t0 ))

  local status_ok=false
  local content_ok=false
  local content_note=""

  # Status check
  if [ "${http_code}" = "${expected_status}" ]; then
    status_ok=true
  fi

  # Content check
  if [ -z "${content_grep}" ]; then
    content_ok=true
    content_note="(no content assertion)"
  elif echo "${body}" | grep -q "${content_grep}"; then
    content_ok=true
    content_note="contains '${content_grep}'"
  else
    content_note="missing '${content_grep}'"
  fi

  # Result row
  if ${status_ok} && ${content_ok}; then
    pass "$(printf '%-45s  HTTP %-3s  %dms  %s' "${name}" "${http_code}" "${elapsed}" "${content_note}")"
  else
    local detail=""
    if ! ${status_ok}; then
      detail="status: expected ${expected_status} got ${http_code}"
    fi
    if ! ${content_ok}; then
      detail="${detail:+${detail}; }content: ${content_note}"
    fi
    fail "$(printf '%-45s  HTTP %-3s  %dms  FAIL: %s' "${name}" "${http_code}" "${elapsed}" "${detail}")"
  fi
}

# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------
show_usage() {
  cat <<EOF
f-day-smoke-docs.sh — Wave 32 Phase F.1 docs smoke test

Usage:
  bash scripts/f-day-smoke-docs.sh [--help]

Options:
  --help    Show this message.

Probes ${BASE}:

  URL                                      Expected  Content assertion
  ──────────────────────────────────────── ───────── ─────────────────────────
  /                                        200       <title>CoreLink
  /blog                                    200       Blog
  /compare/vs-buildbuddy                   200       (status only)
  /legal/sub-processors                    200       (status only)
  /blog/why-blake3                         200       (status only)

EN-only fallback:
  If build/pt-BR/ is absent, locale-specific checks are logged as SKIP (not FAIL).

Exit code: number of failures (0 = all green).

BetterStack probe cross-ref: corelink-docs.humangr.com (WP-7.1).
EOF
  exit 0
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
for arg in "$@"; do
  case "${arg}" in
    --help|-h) show_usage ;;
    *)
      printf "${_RED}[ERROR]${_RST} Unknown argument: %s\n" "${arg}" >&2
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Prerequisite: curl
# ---------------------------------------------------------------------------
if ! command -v curl >/dev/null 2>&1; then
  printf '%s[ERROR]%s curl not found in PATH.\n' "${_RED}" "${_RST}" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# EN-only fallback detection
# Infer from the build directory whether locale builds completed.
# The deploy script will have run apps/docs/build at this point.
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
LOCALE_BUILD_PRESENT=false
if [ -d "${REPO_ROOT}/apps/docs/build/pt-BR" ]; then
  LOCALE_BUILD_PRESENT=true
fi

# ---------------------------------------------------------------------------
# Smoke run
# ---------------------------------------------------------------------------
printf '%sf-day-smoke-docs.sh — Wave 32 Phase F.1 docs smoke%s\n' "${_BLD}" "${_RST}"
printf "Target:  %s\n" "${BASE}"
printf "Started: %s\n" "${SMOKE_START}"
if ${LOCALE_BUILD_PRESENT}; then
  printf "Locale:  4-locale build detected (pt-BR present)\n"
else
  printf "Locale:  EN-only fallback (apps/docs/build/pt-BR not found)\n"
fi
printf "\n"

# Column header
printf '%s%-45s  %-8s  %-6s  %s%s\n' "${_BLD}" "URL" "STATUS" "TIME" "NOTE" "${_RST}"
printf '%s\n' "---------------------------------------------------------------------------------------------"

step "Core pages (5 required)"

# [1] Root — must have CoreLink in title
probe "[1] root /"                              "${BASE}/"                              "200" "<title>CoreLink"

# [2] Blog index — must contain "Blog"
probe "[2] /blog"                               "${BASE}/blog"                          "200" "Blog"

# [3] Compare page — status only (content varies by build)
probe "[3] /compare/vs-buildbuddy"              "${BASE}/compare/vs-buildbuddy"         "200" ""

# [4] Legal sub-processors — status only
probe "[4] /legal/sub-processors"               "${BASE}/legal/sub-processors"          "200" ""

# [5] Blog post why-blake3 — status only (post WP-4.1)
probe "[5] /blog/why-blake3"                    "${BASE}/blog/why-blake3"               "200" ""

step "Locale paths (EN-only fallback aware)"

if ${LOCALE_BUILD_PRESENT}; then
  # Full locale build: smoke locale root paths
  probe "[6] /pt-BR/"                           "${BASE}/pt-BR/"                        "200" ""
  probe "[7] /es-419/"                          "${BASE}/es-419/"                       "200" ""
  probe "[8] /de/"                              "${BASE}/de/"                           "200" ""
else
  skip "[6] /pt-BR/ — EN-only fallback; locale build not present in apps/docs/build"
  skip "[7] /es-419/ — EN-only fallback; locale build not present in apps/docs/build"
  skip "[8] /de/ — EN-only fallback; locale build not present in apps/docs/build"
fi

# ---------------------------------------------------------------------------
# Summary table
# ---------------------------------------------------------------------------
SMOKE_END=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

printf "\n"
printf '%s\n' "═══════════════════════════════════════════════════════════════════"
printf "SMOKE SUMMARY\n"
printf "  Target:   %s\n" "${BASE}"
printf "  Started:  %s\n" "${SMOKE_START}"
printf "  Finished: %s\n" "${SMOKE_END}"
printf "  PASS:     %d\n" "${PASS_COUNT}"
printf "  FAIL:     %d\n" "${FAIL_COUNT}"
printf "  SKIP:     %d\n" "${SKIP_COUNT}"
if [ "${FAIL_COUNT}" -eq 0 ]; then
  printf "  Result:   %sALL GREEN%s\n" "${_GRN}" "${_RST}"
else
  printf "  Result:   %s%d FAILURE(S) — see [FAIL] lines above%s\n" "${_RED}" "${FAIL_COUNT}" "${_RST}"
fi
printf '%s\n' "═══════════════════════════════════════════════════════════════════"

# Cleanup temp file
rm -f /tmp/_smoke_body.tmp

exit "${FAIL_COUNT}"
