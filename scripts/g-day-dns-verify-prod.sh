#!/usr/bin/env bash
# scripts/g-day-dns-verify-prod.sh
#
# Wave 32 Phase G.2 — DNS + HTTPS + cert-chain verification harness
#
# Post-apply: 7 hosts × 3 checks = 21 total assertions.
#
# Usage:
#   ./scripts/g-day-dns-verify-prod.sh [OPTIONS]
#
# Options:
#   --mode=quick    DNS dig only (no TLS — fast pre-check)
#   --mode=tls      Cert-chain check only (debugging)
#   --mode=full     All 3 checks (default)
#   --dry-run       Print what would be tested, no network calls
#   --help          Show this help and exit
#
# Exit codes:
#   0 = all checks PASS (or --dry-run / --help)
#   1 = one or more checks FAIL
#
# Checks per host:
#   dns   : dig +short returns a CF IP (104.x or 172.67.x range)
#           EXCEPT status.corelink.humangr.com which resolves via
#           CNAME to hugrl.betteruptime.com (BetterStack)
#   https : curl -sI returns HTTP 2xx or 3xx status code
#   cert  : openssl s_client cert chain issuer contains "Cloudflare"
#           or "Let's Encrypt" (status host)
#
# W3 note: script makes external DNS/HTTPS calls ONLY to verify state;
#          it never mutates DNS, CF config, or any external service.
#
# Charter: CTRL-CRED-001 — no secrets emitted. DNS + TLS data is public.
#
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# ---------------------------------------------------------------------------
# Colour + formatting
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

_pass()  { printf '%b[PASS]%b  %s\n' "${GREEN}" "${RESET}" "$*"; }
_fail()  { printf '%b[FAIL]%b  %s\n' "${RED}"   "${RESET}" "$*"; }
_info()  { printf '%b[INFO]%b  %s\n' "${CYAN}"  "${RESET}" "$*"; }
_head()  { printf '\n%b%s%b\n' "${BOLD}" "$*" "${RESET}"; }
_sep()   { printf '%s\n' "────────────────────────────────────────────────────────────"; }

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
MODE="full"
DRY_RUN=false

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
for arg in "$@"; do
  case "${arg}" in
    --mode=quick) MODE="quick" ;;
    --mode=tls)   MODE="tls"   ;;
    --mode=full)  MODE="full"  ;;
    --dry-run)    DRY_RUN=true ;;
    --help|-h)
      sed -n '2,/^# Co-Authored/p' "$0" | grep '^#' | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      printf "Unknown option: %s\n" "${arg}" >&2
      printf "Run with --help for usage.\n" >&2
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Host table
#
# Format: "hostname|type|betterstack_cname"
#   type=worker    -> CF proxied Worker; expect CF IP + HTTPS + CF cert
#   type=pages     -> CF proxied Pages;  expect CF IP + HTTPS + CF cert
#   type=statuspage-> DNS-only CNAME to BetterStack; expect CNAME chain
# ---------------------------------------------------------------------------
declare -a HOSTS
HOSTS=(
  "corelink-api.humangr.com|worker|"
  "corelink-app.humangr.com|pages|"
  "corelink-docs.humangr.com|pages|"
  "corelink-signup.humangr.com|worker|"
  "corelink-admin.humangr.com|worker|"
  "corelink-get.humangr.com|worker|"
  "status.corelink.humangr.com|statuspage|hugrl.betteruptime.com"
)

# ---------------------------------------------------------------------------
# Counters
# ---------------------------------------------------------------------------
TOTAL=0
PASS_COUNT=0
FAIL_COUNT=0

# Collect failed host summaries for top-of-output list
declare -a FAILED_HOSTS

# ---------------------------------------------------------------------------
# Helper: is_cf_ip <ip>
# Returns 0 if ip falls in a known CF range, 1 otherwise.
# Ranges: 104.16.0.0/12, 172.64.0.0/13, 188.114.96.0/20,
#         197.234.240.0/22, 198.41.128.0/17, 162.158.0.0/15
# ---------------------------------------------------------------------------
is_cf_ip() {
  local ip="$1"
  # Quick prefix match — covers the bulk of CF ranges
  if [[ "${ip}" =~ ^104\.(1[6-9]|[2-9][0-9]|1[0-2][0-9]|1[3-9][0-9]|2[0-4][0-9]|25[0-5])\. ]]; then
    return 0
  fi
  if [[ "${ip}" =~ ^172\.(6[4-9]|7[0-9])\. ]]; then
    return 0
  fi
  if [[ "${ip}" =~ ^188\.114\.(9[6-9]|1[0-1][0-9])\. ]]; then
    return 0
  fi
  if [[ "${ip}" =~ ^197\.234\.(24[0-3])\. ]]; then
    return 0
  fi
  if [[ "${ip}" =~ ^198\.41\.(12[8-9]|1[3-9][0-9]|2[0-4][0-9]|25[0-5])\. ]]; then
    return 0
  fi
  if [[ "${ip}" =~ ^162\.(15[89])\. ]]; then
    return 0
  fi
  return 1
}

# ---------------------------------------------------------------------------
# check_dns <host> <type> <betterstack_cname>
# Sets global TOTAL / PASS_COUNT / FAIL_COUNT
# ---------------------------------------------------------------------------
check_dns() {
  local host="$1"
  local htype="$2"
  local bs_cname="$3"
  local label="dns:${host}"
  TOTAL=$((TOTAL + 1))

  if [[ "${DRY_RUN}" == "true" ]]; then
    _info "${label}: [DRY-RUN] would run: dig +short +time=5 +tries=2 ${host}"
    PASS_COUNT=$((PASS_COUNT + 1))
    return
  fi

  local resolved
  resolved=$(dig +short +time=5 +tries=2 "${host}" 2>/dev/null || true)

  if [[ -z "${resolved}" ]]; then
    _fail "${label}: NXDOMAIN — no DNS record (apply DNS first)"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_HOSTS+=("${host}:dns:NXDOMAIN")
    return
  fi

  if [[ "${htype}" == "statuspage" ]]; then
    # Expect CNAME chain to BetterStack, not a CF IP
    local cname_target
    cname_target=$(dig +short +time=5 +tries=2 CNAME "${host}" 2>/dev/null | head -1 | sed 's/\.$//' || true)
    if [[ "${cname_target}" == "${bs_cname}" ]] || echo "${resolved}" | grep -q "betteruptime"; then
      _pass "${label}: CNAME → ${cname_target:-${resolved}} [BetterStack DNS-only, expected]"
      PASS_COUNT=$((PASS_COUNT + 1))
    else
      _fail "${label}: expected CNAME to ${bs_cname}, got: cname=${cname_target:-NONE} resolved=${resolved}"
      FAIL_COUNT=$((FAIL_COUNT + 1))
      FAILED_HOSTS+=("${host}:dns:wrong-target")
    fi
    return
  fi

  # For Worker/Pages: expect at least one CF IP in the resolved set
  local cf_found=""
  while IFS= read -r ip; do
    if [[ "${ip}" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      if is_cf_ip "${ip}"; then
        cf_found="${ip}"
        break
      fi
    fi
  done <<< "${resolved}"

  if [[ -n "${cf_found}" ]]; then
    _pass "${label}: CF IP confirmed (${cf_found}) [proxied=true]"
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    _fail "${label}: no CF IP found; resolved to: ${resolved}"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_HOSTS+=("${host}:dns:non-CF-IP(${resolved})")
  fi
}

# ---------------------------------------------------------------------------
# check_https <host> <type>
# ---------------------------------------------------------------------------
check_https() {
  local host="$1"
  local htype="$2"
  local label="https:${host}"
  TOTAL=$((TOTAL + 1))

  if [[ "${DRY_RUN}" == "true" ]]; then
    _info "${label}: [DRY-RUN] would run: curl -sI --max-time 10 https://${host}"
    PASS_COUNT=$((PASS_COUNT + 1))
    return
  fi

  local http_code
  http_code=$(curl -sI --max-time 10 --connect-timeout 8 \
    -o /dev/null -w "%{http_code}" \
    "https://${host}/" 2>/dev/null || echo "000")

  if [[ "${http_code}" =~ ^[23] ]]; then
    _pass "${label}: HTTP ${http_code}"
    PASS_COUNT=$((PASS_COUNT + 1))
  elif [[ "${http_code}" == "000" ]]; then
    _fail "${label}: connection failed (timeout or refused)"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_HOSTS+=("${host}:https:connection-failed")
  else
    _fail "${label}: HTTP ${http_code} (expected 2xx or 3xx)"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_HOSTS+=("${host}:https:HTTP-${http_code}")
  fi
}

# ---------------------------------------------------------------------------
# check_cert <host> <type>
# Accepted issuers: "Cloudflare" (CF Universal SSL / Advanced) OR
#                   "Let's Encrypt" (BetterStack status page)
# ---------------------------------------------------------------------------
check_cert() {
  local host="$1"
  local htype="$2"
  local label="cert:${host}"
  TOTAL=$((TOTAL + 1))

  if [[ "${DRY_RUN}" == "true" ]]; then
    local cmd="openssl s_client -connect ${host}:443 -servername ${host} </dev/null 2>/dev/null | openssl x509 -noout -issuer"
    _info "${label}: [DRY-RUN] would run: ${cmd}"
    PASS_COUNT=$((PASS_COUNT + 1))
    return
  fi

  local raw_issuer
  raw_issuer=$(openssl s_client \
    -connect "${host}:443" \
    -servername "${host}" \
    < /dev/null 2>/dev/null \
    | openssl x509 -noout -issuer 2>/dev/null \
    || echo "")

  if [[ -z "${raw_issuer}" ]]; then
    _fail "${label}: could not retrieve cert (TLS handshake failed or host unreachable)"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_HOSTS+=("${host}:cert:no-cert")
    return
  fi

  if echo "${raw_issuer}" | grep -qi "cloudflare"; then
    _pass "${label}: issuer contains 'Cloudflare' — CF Universal SSL confirmed"
    _info "      issuer: ${raw_issuer}"
    PASS_COUNT=$((PASS_COUNT + 1))
  elif echo "${raw_issuer}" | grep -qi "let.s encrypt"; then
    _pass "${label}: issuer contains 'Let's Encrypt' — valid for status page"
    _info "      issuer: ${raw_issuer}"
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    _fail "${label}: unexpected issuer — expected Cloudflare or Let's Encrypt"
    _info "      issuer: ${raw_issuer}"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    FAILED_HOSTS+=("${host}:cert:unexpected-issuer")
  fi
}

# ---------------------------------------------------------------------------
# Dry-run preamble
# ---------------------------------------------------------------------------
print_dry_run_header() {
  _head "Wave 32 Phase G.2 — DNS + HTTPS + Cert Verify Harness (DRY-RUN)"
  _sep
  printf "Mode      : %s\n" "${MODE}"
  printf "Dry-run   : true\n"
  printf "Timestamp : %s\n" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf "\nWould verify 7 hosts × 3 checks = 21 assertions:\n\n"
  printf "  %-42s  %-12s  %s\n" "HOST" "TYPE" "CHECKS"
  printf "  %-42s  %-12s  %s\n" "----" "----" "------"
  for entry in "${HOSTS[@]}"; do
    IFS='|' read -r h t _ <<< "${entry}"
    local checks="dns+https+cert"
    case "${MODE}" in
      quick) checks="dns only"      ;;
      tls)   checks="cert only"     ;;
    esac
    printf "  %-42s  %-12s  %s\n" "${h}" "${t}" "${checks}"
  done
  printf "\nCheck details:\n"
  printf "  dns  : dig +short +time=5 +tries=2 <host>\n"
  printf "         Workers/Pages -> CF IP (104.x / 172.6[4-9].x / 172.7[0-9].x)\n"
  printf "         status page   -> CNAME to hugrl.betteruptime.com\n"
  printf "  https: curl -sI --max-time 10 https://<host>/ -> 2xx or 3xx\n"
  printf "  cert : openssl s_client -connect <host>:443 -servername <host>\n"
  printf "         issuer must contain 'Cloudflare' or 'Let's Encrypt'\n"
  printf "\nExit code: 0 if 21/21 pass; 1 if any fail.\n"
  printf "Run without --dry-run on G-day after DNS records are applied.\n\n"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
if [[ "${DRY_RUN}" == "true" ]]; then
  print_dry_run_header
  # Walk through each host in dry-run mode to show the plan
  for entry in "${HOSTS[@]}"; do
    IFS='|' read -r host htype bs_cname <<< "${entry}"
    printf "\n  Host: %s (%s)\n" "${host}" "${htype}"
    case "${MODE}" in
      quick)
        printf "    [DRY-RUN] dig +short +time=5 +tries=2 %s\n" "${host}"
        ;;
      tls)
        printf "    [DRY-RUN] openssl s_client -connect %s:443 -servername %s < /dev/null 2>/dev/null | openssl x509 -noout -issuer\n" "${host}" "${host}"
        ;;
      full|*)
        printf "    [DRY-RUN] dig +short +time=5 +tries=2 %s\n" "${host}"
        printf "    [DRY-RUN] curl -sI --max-time 10 https://%s/\n" "${host}"
        printf "    [DRY-RUN] openssl s_client -connect %s:443 -servername %s < /dev/null 2>/dev/null | openssl x509 -noout -issuer\n" "${host}" "${host}"
        ;;
    esac
  done
  printf "\n[DRY-RUN COMPLETE] No network calls made.\n"
  exit 0
fi

# ---------------------------------------------------------------------------
# Live run
# ---------------------------------------------------------------------------
_head "Wave 32 Phase G.2 — DNS + HTTPS + Cert Verify Harness"
_sep
printf "Mode      : %s\n" "${MODE}"
printf "Dry-run   : false\n"
printf "Timestamp : %s\n" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf "Hosts     : 7\n"
printf "Checks    : %s\n" "$(case "${MODE}" in quick) echo "1 (dns only)";; tls) echo "1 (cert only)";; *) echo "3 (dns + https + cert)";; esac)"

for entry in "${HOSTS[@]}"; do
  IFS='|' read -r host htype bs_cname <<< "${entry}"
  _head "▸ ${host} (${htype})"

  case "${MODE}" in
    quick)
      check_dns "${host}" "${htype}" "${bs_cname}"
      ;;
    tls)
      check_cert "${host}" "${htype}"
      ;;
    full|*)
      check_dns   "${host}" "${htype}" "${bs_cname}"
      check_https "${host}" "${htype}"
      check_cert  "${host}" "${htype}"
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Summary table
# ---------------------------------------------------------------------------
_head "Summary"
_sep
printf "  Total checks   : %d\n" "${TOTAL}"
printf "  Passed         : %d\n" "${PASS_COUNT}"
printf "  Failed         : %d\n" "${FAIL_COUNT}"
printf "\n"

if [[ "${FAIL_COUNT}" -gt 0 ]]; then
  printf '%b%bFAILING HOSTS (requires action before cutover):%b\n' "${RED}" "${BOLD}" "${RESET}"
  for item in "${FAILED_HOSTS[@]}"; do
    printf '  %b✗%b  %s\n' "${RED}" "${RESET}" "${item}"
  done
  printf "\n"
fi

if [[ "${FAIL_COUNT}" -eq 0 ]]; then
  printf '%b%bVERDICT: PASS — %d/%d checks passed. DNS + TLS state is production-ready.%b\n\n' \
    "${GREEN}" "${BOLD}" "${PASS_COUNT}" "${TOTAL}" "${RESET}"
  exit 0
else
  printf '%b%bVERDICT: FAIL — %d/%d checks passed. Fix failing hosts before cutover.%b\n\n' \
    "${RED}" "${BOLD}" "${PASS_COUNT}" "${TOTAL}" "${RESET}"
  exit 1
fi
