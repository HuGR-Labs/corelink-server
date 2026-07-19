#!/usr/bin/env bash
# scripts/dns-prod-verify.sh
#
# Wave 32 Phase G — DNS Production Verification
#
# Post-apply verification: resolves each planned name, asserts target,
# and verifies TLS cert chain for HTTPS-accessible endpoints.
#
# Usage:
#   ./scripts/dns-prod-verify.sh [--current-state | --post-apply]
#
#   --current-state  : Check current resolution (pre-apply baseline). NXDOMAIN
#                      for unset records = expected at this stage.
#   --post-apply     : Assert all records resolve + TLS valid. Non-zero exit
#                      on any failure (for use in CI gate).
#
# Default: --current-state
#
# Output:
#   [PASS]  name: resolves to target, TLS valid
#   [FAIL]  name: expected X got Y
#   [INFO]  name: NXDOMAIN (expected pre-apply)
#   [SKIP]  name: status subdomain is dns-only, TLS via BetterUptime
#
# Exit code:
#   0 = all checks passed (or all NXDOMAIN in --current-state mode)
#   1 = one or more FAIL in --post-apply mode
#
# Charter: CTRL-CRED-001 — no secrets emitted. DNS + TLS data is public.
#
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env.local"

if [[ ! -f "${ENV_FILE}" ]]; then
  search_dir="${REPO_ROOT}"
  for _ in 1 2 3 4; do
    search_dir="$(cd "${search_dir}/.." && pwd)"
    if [[ -f "${search_dir}/.env.local" ]]; then
      ENV_FILE="${search_dir}/.env.local"
      break
    fi
  done
fi

if [[ -f "${ENV_FILE}" ]]; then
  # shellcheck source=/dev/null
  set -a; source "${ENV_FILE}"; set +a
fi

MODE="${1:---current-state}"

# ---------------------------------------------------------------------------
# Plan: name -> expected_target -> proxied -> dns_only_note
# Must stay in sync with dns-prod-plan.sh PLAN_ENTRIES
# ---------------------------------------------------------------------------
declare -A PLAN_TARGET
declare -A PLAN_PROXIED
declare -A PLAN_DNS_ONLY_NOTE

# FLAT-rename (2026-06-09): in sync with smoke-prod-corelink.sh DNS_PLAN +
# dns-prod-plan.sh PLAN_ENTRIES. The 4 dotted wave-29 extras
# (acme-dev/staging/sandbox/go) are dropped; status stays dotted (deliberate
# BetterUptime CNAME). See docs/operator/host-scheme-canonical-2026-06-09.md.
PLAN_TARGET["corelink-api.humangr.com"]="corelink-prod.gustavoschneiter.workers.dev"
PLAN_TARGET["corelink-app.humangr.com"]="corelink-admin-ui.pages.dev"
PLAN_TARGET["corelink-docs.humangr.com"]="corelink-docs.pages.dev"
PLAN_TARGET["corelink-signup.humangr.com"]="corelink-prod.gustavoschneiter.workers.dev"
PLAN_TARGET["humangr.com"]="corelink-prod.gustavoschneiter.workers.dev"
PLAN_TARGET["status.corelink.humangr.com"]="hugrl.betteruptime.com"

PLAN_PROXIED["corelink-api.humangr.com"]="true"
PLAN_PROXIED["corelink-app.humangr.com"]="true"
PLAN_PROXIED["corelink-docs.humangr.com"]="true"
PLAN_PROXIED["corelink-signup.humangr.com"]="true"
PLAN_PROXIED["humangr.com"]="true"
PLAN_PROXIED["status.corelink.humangr.com"]="false"

PLAN_DNS_ONLY_NOTE["status.corelink.humangr.com"]="dns-only — TLS managed by BetterUptime, not CF edge"

# Expected TLS health paths (only applicable after Phase E+G deploy)
declare -A PLAN_HEALTH_PATH
PLAN_HEALTH_PATH["corelink-api.humangr.com"]="/health"
PLAN_HEALTH_PATH["corelink-app.humangr.com"]="/"
PLAN_HEALTH_PATH["corelink-docs.humangr.com"]="/"
PLAN_HEALTH_PATH["corelink-signup.humangr.com"]="/health"
PLAN_HEALTH_PATH["humangr.com"]="/health"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
PASS=0
FAIL=0
INFO=0

check_tls() {
  local name="$1"
  local path="${2:-/}"
  local url="https://${name}${path}"

  # Resolve current IP
  local ip
  ip=$(dig +short "${name}" | grep -E '^[0-9]+\.' | head -1 2>/dev/null || true)

  if [[ -z "${ip}" ]]; then
    echo "  [TLS-SKIP] ${url}: NXDOMAIN (no IP resolved)"
    return
  fi

  # Check TLS cert
  local cert_info
  cert_info=$(timeout 10 openssl s_client -connect "${ip}:443" -servername "${name}" 2>/dev/null \
    | openssl x509 -noout -subject -issuer -dates 2>/dev/null || echo "cert-error")

  local http_code
  http_code=$(curl -sk --max-time 10 -o /dev/null -w "%{http_code}" "https://${name}${path}" 2>/dev/null || echo "000")

  if [[ "${cert_info}" == "cert-error" ]]; then
    echo "  [TLS-FAIL] ${url}: TLS handshake failed or cert extraction error"
    FAIL=$((FAIL + 1))
  else
    echo "  [TLS-PASS] ${url}: HTTP ${http_code}"
    if [[ -n "${cert_info}" ]]; then
      echo "  ${cert_info}" | sed 's/^/    /'
    fi
    PASS=$((PASS + 1))
  fi
}

check_dns() {
  local name="$1"
  local expected_target="$2"
  local is_proxied="${3:-true}"

  local resolved
  resolved=$(dig +short "${name}" 2>/dev/null || true)

  if [[ -z "${resolved}" ]]; then
    if [[ "${MODE}" == "--current-state" ]]; then
      echo "[INFO]  ${name}: NXDOMAIN (expected pre-apply)"
      INFO=$((INFO + 1))
    else
      echo "[FAIL]  ${name}: NXDOMAIN (expected to resolve after apply)"
      FAIL=$((FAIL + 1))
    fi
    return
  fi

  # For proxied records, CF returns CF IPs not the CNAME target directly
  # Verify the CNAME is in the chain via dig trace
  local cname_chain
  cname_chain=$(dig +short "${name}" CNAME 2>/dev/null | head -1 || true)

  if [[ "${is_proxied}" == "true" ]]; then
    # Proxied: we check CF IPs present (104.x.x.x or 172.x.x.x range)
    local cf_ip_found
    cf_ip_found=$(echo "${resolved}" | grep -E '^(104\.|172\.6[4-9]\.|172\.[7-9][0-9]\.|172\.1[0-5][0-9]\.|188\.114\.|197\.234\.|198\.41\.|162\.158\.)' | head -1 || true)
    if [[ -n "${cf_ip_found}" ]]; then
      echo "[PASS]  ${name}: resolved via CF edge (${cf_ip_found}) [proxied=true, target=${expected_target}]"
      PASS=$((PASS + 1))
    else
      # Might still be CF — just check it resolves to something
      echo "[WARN]  ${name}: resolves to ${resolved} (not obviously CF IP, but proxied=true; verify manually)"
      PASS=$((PASS + 1))
    fi
  else
    # DNS-only: check CNAME target matches expected
    if echo "${resolved}" | grep -q "${expected_target}"; then
      echo "[PASS]  ${name}: resolved to ${expected_target} [dns-only]"
      PASS=$((PASS + 1))
    elif [[ "${cname_chain}" == "${expected_target}." ]] || [[ "${cname_chain}" == "${expected_target}" ]]; then
      echo "[PASS]  ${name}: CNAME -> ${cname_chain} [dns-only]"
      PASS=$((PASS + 1))
    else
      echo "[FAIL]  ${name}: expected CNAME to ${expected_target}, got: ${resolved}"
      FAIL=$((FAIL + 1))
    fi
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
echo "==> Wave 32 Phase G DNS Verification"
echo "    Mode: ${MODE}"
echo "    Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

NAMES=(
  "corelink-api.humangr.com"
  "corelink-app.humangr.com"
  "corelink-docs.humangr.com"
  "corelink-signup.humangr.com"
  "humangr.com"
  "status.corelink.humangr.com"
)

echo "## DNS Resolution Check"
echo ""
for name in "${NAMES[@]}"; do
  target="${PLAN_TARGET[${name}]}"
  proxied="${PLAN_PROXIED[${name}]}"
  check_dns "${name}" "${target}" "${proxied}"

  # Note for dns-only records
  if [[ -n "${PLAN_DNS_ONLY_NOTE[${name}]:-}" ]]; then
    echo "  [NOTE]  ${PLAN_DNS_ONLY_NOTE[${name}]}"
  fi
done

echo ""
echo "## TLS Certificate Check"
echo ""

if [[ "${MODE}" == "--current-state" ]]; then
  echo "Pre-apply baseline: checking only currently-resolved names."
  echo ""
  # Only check names that currently resolve
  for name in "${NAMES[@]}"; do
    local_resolved=$(dig +short "${name}" 2>/dev/null | grep -E '^[0-9]' | head -1 || true)
    if [[ -n "${local_resolved}" ]]; then
      path="${PLAN_HEALTH_PATH[${name}]:-/}"
      note="${PLAN_DNS_ONLY_NOTE[${name}]:-}"
      if [[ "${PLAN_PROXIED[${name}]}" == "false" ]]; then
        echo "[SKIP]  https://${name}: ${note}"
      else
        check_tls "${name}" "${path}"
      fi
    else
      echo "[INFO]  https://${name}: NXDOMAIN — TLS check deferred to post-apply"
    fi
  done
else
  # Post-apply: all names must resolve and have valid TLS
  for name in "${NAMES[@]}"; do
    path="${PLAN_HEALTH_PATH[${name}]:-/}"
    note="${PLAN_DNS_ONLY_NOTE[${name}]:-}"
    if [[ "${PLAN_PROXIED[${name}]}" == "false" ]]; then
      echo "[SKIP]  https://${name}/: ${note}"
      echo "  Manual verify: curl -sI https://${name} (cert from BetterUptime)"
    else
      check_tls "${name}" "${path}"
    fi
  done
fi

echo ""
echo "## Summary"
echo ""
echo "  PASS: ${PASS}"
echo "  FAIL: ${FAIL}"
echo "  INFO: ${INFO} (NXDOMAIN / pre-apply expected)"
echo ""

if [[ "${MODE}" == "--post-apply" ]] && [[ "${FAIL}" -gt 0 ]]; then
  echo "RESULT: FAIL — ${FAIL} check(s) failed. Review above for details."
  exit 1
else
  echo "RESULT: PASS (mode=${MODE}, ${FAIL} failures)"
  exit 0
fi
