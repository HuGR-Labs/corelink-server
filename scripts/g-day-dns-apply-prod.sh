#!/usr/bin/env bash
# g-day-dns-apply-prod.sh — Wave 32 Phase G.1
# Creates/updates CNAME records for all 6 corelink-*.humangr.com subdomains
# via the Cloudflare DNS API (proxied, orange-cloud).
#
# Usage:
#   ./scripts/g-day-dns-apply-prod.sh --dry-run          (default; safe)
#   ./scripts/g-day-dns-apply-prod.sh --live             (requires CF_API_TOKEN + CF_ZONE_ID)
#
# Idempotency: GET by name first → PATCH if record exists, POST if new.
# TLS: Universal SSL Free covers *.humangr.com — no extra cert needed.
#
# shellcheck disable=SC2317  # functions are called dynamically

set -euo pipefail

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
ZONE_ID="${CF_ZONE_ID:-REPLACE_WITH_ZONE_ID}"
CF_API_BASE="https://api.cloudflare.com/client/v4"
# Worker target: the corelink production Worker (all routes proxy to same worker)
WORKER_ROUTE_TARGET="corelink-worker.humangr.com"

# All 6 flat-name subdomains backed by CF Worker routes:
# (status.corelink.humangr.com is Phase A; NOT a Worker route — excluded here)
SUBDOMAINS=(
  "corelink-api.humangr.com"
  "corelink-signup.humangr.com"
  "humangr.com"
  "corelink-app.humangr.com"
  "corelink-docs.humangr.com"
  "corelink-get.humangr.com"
)

# ---------------------------------------------------------------------------
# Flags
# ---------------------------------------------------------------------------
DRY_RUN=true
for arg in "$@"; do
  case "$arg" in
    --live)     DRY_RUN=false ;;
    --dry-run)  DRY_RUN=true  ;;
    *)
      echo "ERROR: unknown flag: $arg" >&2
      echo "Usage: $0 [--dry-run|--live]" >&2
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { echo "[$(date -u +%H:%M:%SZ)] $*"; }
info() { log "INFO  $*"; }
warn() { log "WARN  $*"; }

validate_token() {
  if [[ -z "${CF_API_TOKEN:-}" ]]; then
    echo "ERROR: CF_API_TOKEN is not set. Export it before running --live." >&2
    exit 1
  fi
  if [[ "$ZONE_ID" == "REPLACE_WITH_ZONE_ID" ]]; then
    echo "ERROR: CF_ZONE_ID is not set. Export it before running --live." >&2
    exit 1
  fi
  info "Validating CF_API_TOKEN..."
  local resp
  resp=$(curl -sf -X GET \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -H "Content-Type: application/json" \
    "${CF_API_BASE}/user/tokens/verify") || {
    echo "ERROR: Token validation request failed." >&2
    exit 1
  }
  local status
  status=$(echo "$resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('result',{}).get('status','unknown'))")
  if [[ "$status" != "active" ]]; then
    echo "ERROR: CF_API_TOKEN is not active (status=$status)." >&2
    exit 1
  fi
  info "Token is active."
}

# Build the JSON body for a CNAME record pointing to the Worker subdomain.
# All records are proxied (orange-cloud) so Cloudflare handles TLS via Universal SSL Free.
cname_body() {
  local name="$1"
  python3 - <<EOF
import json
print(json.dumps({
    "type": "CNAME",
    "name": "${name}",
    "content": "${WORKER_ROUTE_TARGET}",
    "ttl": 1,
    "proxied": True,
    "comment": "Wave 32 Phase G.1 — managed by g-day-dns-apply-prod.sh"
}))
EOF
}

# Look up an existing DNS record by exact name; echo its ID or empty string.
get_record_id() {
  local name="$1"
  curl -sf -X GET \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -H "Content-Type: application/json" \
    "${CF_API_BASE}/zones/${ZONE_ID}/dns_records?type=CNAME&name=${name}" \
  | python3 -c "
import sys, json
data = json.load(sys.stdin)
results = data.get('result', [])
print(results[0]['id'] if results else '')
"
}

# POST a new CNAME record.
create_record() {
  local name="$1"
  local body
  body=$(cname_body "$name")
  curl -sf -X POST \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "$body" \
    "${CF_API_BASE}/zones/${ZONE_ID}/dns_records" \
  | python3 -c "
import sys, json
data = json.load(sys.stdin)
ok = data.get('success', False)
rid = data.get('result', {}).get('id', 'N/A')
print(f'  created  id={rid}  success={ok}')
"
}

# PATCH an existing CNAME record (idempotent update).
update_record() {
  local name="$1"
  local record_id="$2"
  local body
  body=$(cname_body "$name")
  curl -sf -X PATCH \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "$body" \
    "${CF_API_BASE}/zones/${ZONE_ID}/dns_records/${record_id}" \
  | python3 -c "
import sys, json
data = json.load(sys.stdin)
ok = data.get('success', False)
rid = data.get('result', {}).get('id', 'N/A')
print(f'  patched  id={rid}  success={ok}')
"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  if $DRY_RUN; then
    info "=== DRY-RUN MODE — no API calls will be made ==="
    info "Zone ID placeholder: ${ZONE_ID}"
    info "Worker route target: ${WORKER_ROUTE_TARGET}"
    info ""
    info "Would apply the following 6 CNAME records (proxied=true):"
    info ""
    for subdomain in "${SUBDOMAINS[@]}"; do
      body=$(cname_body "$subdomain")
      info "--- ${subdomain} ---"
      info "  POST ${CF_API_BASE}/zones/${ZONE_ID}/dns_records"
      info "  Body: ${body}"
      info ""
    done
    info "Idempotency behaviour (--live):"
    info "  1. GET /zones/{zone_id}/dns_records?type=CNAME&name={name}"
    info "  2. If record exists → PATCH /zones/{zone_id}/dns_records/{id}"
    info "  3. If record absent → POST /zones/{zone_id}/dns_records"
    info ""
    info "TLS: Universal SSL Free certificate covers *.humangr.com"
    info "     No extra cert provisioning needed."
    info ""
    info "Run with --live to apply changes (requires CF_API_TOKEN + CF_ZONE_ID)."
    return 0
  fi

  # --live path
  info "=== LIVE MODE — applying DNS changes ==="
  validate_token

  local applied=0
  local skipped=0

  for subdomain in "${SUBDOMAINS[@]}"; do
    info "Processing: ${subdomain}"
    local record_id
    record_id=$(get_record_id "$subdomain")
    if [[ -n "$record_id" ]]; then
      info "  Record exists (id=${record_id}), patching..."
      update_record "$subdomain" "$record_id"
      skipped=$((skipped + 1))
    else
      info "  No existing record, creating..."
      create_record "$subdomain"
      applied=$((applied + 1))
    fi
  done

  info ""
  info "Done. created=${applied} patched=${skipped} total=${#SUBDOMAINS[@]}"
}

main "$@"
