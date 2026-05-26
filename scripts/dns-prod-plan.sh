#!/usr/bin/env bash
# scripts/dns-prod-plan.sh
#
# Wave 32 Phase G — DNS Production Plan Generator
#
# Generates the CoreLink humangr.com DNS plan from wrangler.toml + Phase F
# Pages projects + BetterStack status page. Does NOT apply any changes.
#
# Usage:
#   ./scripts/dns-prod-plan.sh [--plan-md | --plan-json | --diff-against-live]
#
# Default: --plan-md
#
# Environment (from .env.local or shell):
#   CLOUDFLARE_API_TOKEN      — CF API token (Zone: DNS Edit scope)
#   CLOUDFLARE_ZONE_ID_HUMANGR — humangr.com zone ID
#   CLOUDFLARE_ACCOUNT_ID     — CF account ID
#
# Charter: CTRL-CRED-001 — no secrets emitted; DNS records are public data.
#
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# ---------------------------------------------------------------------------
# Resolve .env.local relative to this script's repo root
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env.local"

# Worktrees live under .claude/worktrees/<name>/; canonical .env.local is
# three levels up at the main repo root. Walk up until found (max 4 levels).
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

# ---------------------------------------------------------------------------
# Credential normalisation: accept CF_* or CLOUDFLARE_* naming
# ---------------------------------------------------------------------------
CF_API_TOKEN="${CF_API_TOKEN:-${CLOUDFLARE_API_TOKEN:-}}"
CF_ZONE_ID="${CF_ZONE_ID:-${CLOUDFLARE_ZONE_ID_HUMANGR:-}}"
CF_ACCOUNT_ID="${CF_ACCOUNT_ID:-${CLOUDFLARE_ACCOUNT_ID:-}}"

# ---------------------------------------------------------------------------
# Parse args
# ---------------------------------------------------------------------------
MODE="${1:---plan-md}"

# ---------------------------------------------------------------------------
# Hard pause trigger 1: validate required env
# ---------------------------------------------------------------------------
if [[ -z "${CF_API_TOKEN}" ]] || [[ -z "${CF_ZONE_ID}" ]] || [[ -z "${CF_ACCOUNT_ID}" ]]; then
  echo "ERROR: Missing CF credentials. Set CLOUDFLARE_API_TOKEN / CLOUDFLARE_ZONE_ID_HUMANGR / CLOUDFLARE_ACCOUNT_ID." >&2
  echo "HARD PAUSE TRIGGER 1: CF credentials absent." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# DNS plan definition
#
# Derived from:
#   - specs/_audits/2026-05-22-wave32-prod-deploy-spec.md §4 Phase G
#   - wrangler.toml [env.prod] (name = "corelink-prod")
#   - Phase F scope: Pages projects corelink-docs + corelink-admin-ui
#   - Phase A (complete): status.corelink.humangr.com → hugrl.betteruptime.com
#
# Format per entry:
#   name|type|target|proxied|ttl|notes
#
# Worker subdomain: after Phase B+E deploy the worker is accessible at
#   <name>.gustavoschneiter.workers.dev (workers.dev subdomain).
# Once custom domain routes are bound via wrangler.toml [[routes]], CF
# creates the CNAME automatically — but we manage it explicitly here for
# idempotency and audit purposes.
#
# Pages subdomain: <project>.pages.dev is the canonical CF Pages target.
# ---------------------------------------------------------------------------

PLAN_ENTRIES=(
  "api.corelink.humangr.com|CNAME|corelink-prod.gustavoschneiter.workers.dev|true|1|Worker prod entry point (all /v1/* routes)"
  "app.corelink.humangr.com|CNAME|corelink-admin-ui.pages.dev|true|1|Phase F Pages: corelink-admin-ui"
  "docs.corelink.humangr.com|CNAME|corelink-docs.pages.dev|true|1|Phase F Pages: corelink-docs (4 locales)"
  "signup.corelink.humangr.com|CNAME|corelink-prod.gustavoschneiter.workers.dev|true|1|Worker signup/pilot-onboard route"
  "admin.corelink.humangr.com|CNAME|corelink-prod.gustavoschneiter.workers.dev|true|1|Worker internal-admin route (Clerk-gated)"
  "acme-dev.corelink.humangr.com|CNAME|corelink-prod.gustavoschneiter.workers.dev|true|1|Worker ACME-dev environment (wave-29 inventory)"
  "staging.corelink.humangr.com|CNAME|corelink-staging.gustavoschneiter.workers.dev|true|1|Staging worker (wrangler --env staging)"
  "sandbox.corelink.humangr.com|CNAME|corelink-prod.gustavoschneiter.workers.dev|true|1|Sandbox/trial route (wave-29 inventory)"
  "go.corelink.humangr.com|CNAME|corelink-prod.gustavoschneiter.workers.dev|true|1|Redirect/go-links worker route"
  "status.corelink.humangr.com|CNAME|hugrl.betteruptime.com|false|1|Phase A (ALREADY EXISTS — dns-only per BetterUptime requirement)"
)

# ---------------------------------------------------------------------------
# Output: --plan-md
# ---------------------------------------------------------------------------
print_plan_md() {
  echo "# CoreLink humangr.com DNS Production Plan"
  echo ""
  echo "> Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "> Zone: humangr.com (${CF_ZONE_ID})"
  echo "> Account: ${CF_ACCOUNT_ID}"
  echo ""
  echo "## Plan Table"
  echo ""
  printf "| # | Source Name | Type | Target | Proxied | TTL | Notes |\n"
  printf "|---|-------------|------|--------|---------|-----|-------|\n"
  local i=1
  for entry in "${PLAN_ENTRIES[@]}"; do
    IFS='|' read -r name type target proxied ttl notes <<< "${entry}"
    local proxied_display
    proxied_display=$([ "${proxied}" = "true" ] && echo "yes (orange)" || echo "no (grey)")
    local ttl_display
    ttl_display=$([ "${ttl}" = "1" ] && echo "Auto" || echo "${ttl}s")
    printf "| %d | \`%s\` | %s | \`%s\` | %s | %s | %s |\n" \
      "${i}" "${name}" "${type}" "${target}" "${proxied_display}" "${ttl_display}" "${notes}"
    i=$((i + 1))
  done
  echo ""
  echo "## Proxied vs DNS-only rationale"
  echo ""
  echo "- **Proxied (orange cloud):** All Worker and Pages routes. CF edge enforces"
  echo "  WAF, rate-limiting, and TLS termination. Required for DO/Container routing."
  echo "- **DNS-only (grey cloud):** \`status.corelink.humangr.com\` only. BetterUptime"
  echo "  requires direct TLS handshake to issue its own cert on the custom domain."
  echo "  Already set in Phase A — must NOT be proxied."
  echo ""
  echo "## TLS / Certificate strategy"
  echo ""
  echo "- All proxied records: **CF Universal SSL** (auto-provisioned wildcard"
  echo "  \`*.humangr.com\` or per-hostname SAN). Zero operator action needed."
  echo "- status subdomain (DNS-only): **BetterUptime-managed cert** issued after"
  echo "  custom domain verification. TLS chain verified in Phase A gate."
  echo "- CF Universal SSL auto-renews 30d before expiry. No custom cert required."
}

# ---------------------------------------------------------------------------
# Output: --plan-json
# ---------------------------------------------------------------------------
print_plan_json() {
  echo "{"
  echo "  \"generated_at\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\","
  echo "  \"zone_id\": \"${CF_ZONE_ID}\","
  echo "  \"account_id\": \"${CF_ACCOUNT_ID}\","
  echo "  \"records\": ["
  local i=0
  local total=${#PLAN_ENTRIES[@]}
  for entry in "${PLAN_ENTRIES[@]}"; do
    IFS='|' read -r name type target proxied ttl notes <<< "${entry}"
    local comma
    comma=$([ $((i + 1)) -lt "${total}" ] && echo "," || echo "")
    printf '    {"name": "%s", "type": "%s", "content": "%s", "proxied": %s, "ttl": %s, "notes": "%s"}%s\n' \
      "${name}" "${type}" "${target}" "${proxied}" "${ttl}" "${notes}" "${comma}"
    i=$((i + 1))
  done
  echo "  ]"
  echo "}"
}

# ---------------------------------------------------------------------------
# Output: --diff-against-live
# ---------------------------------------------------------------------------
diff_against_live() {
  echo "# CoreLink DNS Diff — Plan vs Live humangr.com"
  echo ""
  echo "> Zone: humangr.com (${CF_ZONE_ID})"
  echo "> Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo ""

  # Fetch live records
  local live_json
  live_json=$(curl -sf \
    "https://api.cloudflare.com/client/v4/zones/${CF_ZONE_ID}/dns_records?per_page=100" \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -H "Content-Type: application/json") || {
    echo "ERROR: Failed to fetch live DNS records. Check CF_API_TOKEN scope (Zone: DNS Read)." >&2
    exit 1
  }

  local success
  success=$(echo "${live_json}" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('success','false'))")
  if [[ "${success}" != "True" ]]; then
    echo "ERROR: CF API returned failure:" >&2
    echo "${live_json}" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('errors'))" >&2
    exit 1
  fi

  echo "## Current live records (humangr.com zone)"
  echo ""
  printf "| Name | Type | Content | Proxied |\n"
  printf "|------|------|---------|--------|\n"
  echo "${live_json}" | python3 -c "
import json, sys
data = json.load(sys.stdin)
records = sorted(data['result'], key=lambda x: x['name'])
for r in records:
    proxied = 'yes' if r.get('proxied') else 'no'
    content = r.get('content', '')[:60]
    print(f\"| \`{r['name']}\` | {r['type']} | \`{content}\` | {proxied} |\")
"
  echo ""

  echo "## Planned records — delta (what apply would change)"
  echo ""
  printf "| Action | Name | Type | Target | Proxied | Conflict? |\n"
  printf "|--------|------|------|--------|---------|----------|\n"

  for entry in "${PLAN_ENTRIES[@]}"; do
    IFS='|' read -r name type target proxied ttl notes <<< "${entry}"

    # Check if name already exists in live records
    local live_content live_type live_proxied
    live_content=$(echo "${live_json}" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for r in data['result']:
    if r['name'] == '${name}' and r['type'] == '${type}':
        print(r.get('content', ''))
        break
" 2>/dev/null)

    live_type=$(echo "${live_json}" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for r in data['result']:
    if r['name'] == '${name}':
        print(r['type'])
        break
" 2>/dev/null)

    live_proxied=$(echo "${live_json}" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for r in data['result']:
    if r['name'] == '${name}' and r['type'] == '${type}':
        print('true' if r.get('proxied') else 'false')
        break
" 2>/dev/null)

    local action conflict_note
    if [[ -z "${live_content}" ]]; then
      # Record does not exist → CREATE
      if [[ -n "${live_type}" ]] && [[ "${live_type}" != "${type}" ]]; then
        action="CREATE (type conflict)"
        conflict_note="COLLISION: existing ${live_type} record"
      else
        action="CREATE"
        conflict_note=""
      fi
    elif [[ "${live_content}" == "${target}" ]] && [[ "${live_proxied}" == "${proxied}" ]]; then
      action="NO-OP (identical)"
      conflict_note=""
    elif [[ "${live_content}" != "${target}" ]]; then
      action="UPDATE content"
      conflict_note="live: \`${live_content}\`"
    else
      action="UPDATE proxied"
      conflict_note="live proxied=${live_proxied}"
    fi

    local proxied_display
    proxied_display=$([ "${proxied}" = "true" ] && echo "yes" || echo "no")
    printf "| %s | \`%s\` | %s | \`%s\` | %s | %s |\n" \
      "${action}" "${name}" "${type}" "${target}" "${proxied_display}" "${conflict_note}"
  done

  echo ""
  echo "## Hard pause collision check"
  echo ""
  local collision_found=false
  for entry in "${PLAN_ENTRIES[@]}"; do
    IFS='|' read -r name type target proxied ttl notes <<< "${entry}"
    local live_content_check
    live_content_check=$(echo "${live_json}" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for r in data['result']:
    if r['name'] == '${name}' and r['type'] == '${type}' and r.get('content','') != '${target}':
        print(r.get('content',''))
        break
" 2>/dev/null)
    if [[ -n "${live_content_check}" ]]; then
      echo "COLLISION DETECTED: \`${name}\` (${type}) already points to \`${live_content_check}\`"
      echo "  Planned target: \`${target}\`"
      echo "  Action: document here; do NOT auto-overwrite."
      collision_found=true
    fi
  done
  if [[ "${collision_found}" == "false" ]]; then
    echo "No collisions detected. All planned records are either new or identical to live."
  fi
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
case "${MODE}" in
  --plan-md)
    print_plan_md
    ;;
  --plan-json)
    print_plan_json
    ;;
  --diff-against-live)
    diff_against_live
    ;;
  *)
    echo "Usage: $0 [--plan-md | --plan-json | --diff-against-live]" >&2
    exit 1
    ;;
esac
