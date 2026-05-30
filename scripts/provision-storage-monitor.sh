#!/usr/bin/env bash
# provision-storage-monitor.sh
#
# Idempotent: creates the BetterStack keyword monitor for /_health/container
# (storage=r2 body check) if it does not already exist at that URL.
# Safe to re-run — will NOT create duplicates.
#
# Usage:
#   source .env.local && bash scripts/provision-storage-monitor.sh
#
# Required env vars (from .env.local):
#   BETTERSTACK_API_TOKEN   — BetterStack API token
#
# Optional env vars:
#   BETTERSTACK_STATUS_PAGE_ID   — defaults to 247652
#   BETTERSTACK_SECTION_ID       — defaults to 328864 (API section)
#
# After creation, wire PagerDuty manually (API does not support it):
#   See specs/_runbooks/rb-storage-fallback.md §5

set -euo pipefail

TARGET_URL="https://corelink-api.humangr.com/_health/container"
REQUIRED_KEYWORD='"storage":"r2"'
STATUS_PAGE_ID="${BETTERSTACK_STATUS_PAGE_ID:-247652}"
SECTION_ID="${BETTERSTACK_SECTION_ID:-328864}"
API_BASE="https://uptime.betterstack.com/api/v2"

if [[ -z "${BETTERSTACK_API_TOKEN:-}" ]]; then
  echo "ERROR: BETTERSTACK_API_TOKEN is not set. Run: source .env.local" >&2
  exit 1
fi

AUTH_HEADER="Authorization: Bearer $BETTERSTACK_API_TOKEN"

# ── Step 1: Check for existing monitor at same URL ─────────────────────────
echo "==> Checking for existing monitor at $TARGET_URL ..."
MONITORS=$(curl -sf -H "$AUTH_HEADER" "$API_BASE/monitors")
EXISTING_ID=$(echo "$MONITORS" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for m in data.get('data', []):
    if m.get('attributes', {}).get('url') == '$TARGET_URL':
        print(m['id'])
        break
" 2>/dev/null || true)

if [[ -n "$EXISTING_ID" ]]; then
  echo "IDEMPOTENT: Monitor already exists at $TARGET_URL (ID: $EXISTING_ID)"
  echo "Skipping creation. Verifying keyword config..."
  EXISTING_KEYWORD=$(echo "$MONITORS" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for m in data.get('data', []):
    if m['id'] == '$EXISTING_ID':
        print(m.get('attributes', {}).get('required_keyword', ''))
        break
" 2>/dev/null || true)
  if [[ "$EXISTING_KEYWORD" == "$REQUIRED_KEYWORD" ]]; then
    echo "OK: required_keyword is correctly set."
  else
    echo "WARNING: required_keyword is '$EXISTING_KEYWORD', expected '$REQUIRED_KEYWORD'"
    echo "Update manually: PATCH $API_BASE/monitors/$EXISTING_ID"
  fi
  MONITOR_ID="$EXISTING_ID"
else
  # ── Step 2: Create the monitor ───────────────────────────────────────────
  echo "==> Creating monitor for $TARGET_URL ..."
  CREATE_RESP=$(curl -sf -X POST \
    -H "$AUTH_HEADER" \
    -H "Content-Type: application/json" \
    -d "{
      \"url\": \"$TARGET_URL\",
      \"pronounceable_name\": \"CoreLink container storage=r2 check\",
      \"monitor_type\": \"keyword\",
      \"required_keyword\": \"$REQUIRED_KEYWORD\",
      \"check_frequency\": 60,
      \"regions\": [\"us\", \"eu\", \"as\", \"au\"],
      \"expected_status_codes\": [200],
      \"request_timeout\": 10,
      \"recovery_period\": 60,
      \"verify_ssl\": true,
      \"ssl_expiration\": 30,
      \"domain_expiration\": 30,
      \"email\": true,
      \"paused\": false
    }" \
    "$API_BASE/monitors")

  MONITOR_ID=$(echo "$CREATE_RESP" | python3 -c "
import json, sys
data = json.load(sys.stdin)
print(data['data']['id'])
" 2>/dev/null || true)

  if [[ -z "$MONITOR_ID" ]]; then
    echo "ERROR: Monitor creation failed. Response:" >&2
    echo "$CREATE_RESP" >&2
    exit 1
  fi
  echo "CREATED: Monitor ID $MONITOR_ID"
fi

# ── Step 3: Add to status page (idempotent) ──────────────────────────────
echo "==> Checking status page resource for monitor $MONITOR_ID ..."
RESOURCES=$(curl -sf -H "$AUTH_HEADER" "$API_BASE/status-pages/$STATUS_PAGE_ID/resources")
EXISTING_RESOURCE=$(echo "$RESOURCES" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for r in data.get('data', []):
    if str(r.get('attributes', {}).get('resource_id')) == '$MONITOR_ID':
        print(r['id'])
        break
" 2>/dev/null || true)

if [[ -n "$EXISTING_RESOURCE" ]]; then
  echo "IDEMPOTENT: Status page resource already exists (ID: $EXISTING_RESOURCE)"
else
  echo "==> Adding monitor to status page $STATUS_PAGE_ID, section $SECTION_ID ..."
  RESOURCE_RESP=$(curl -sf -X POST \
    -H "$AUTH_HEADER" \
    -H "Content-Type: application/json" \
    -d "{
      \"resource_id\": $MONITOR_ID,
      \"resource_type\": \"Monitor\",
      \"public_name\": \"CoreLink Container Storage\",
      \"explanation\": \"Alerts if container falls back to in-memory storage (R2 degradation)\",
      \"status_page_section_id\": $SECTION_ID
    }" \
    "$API_BASE/status-pages/$STATUS_PAGE_ID/resources")
  RESOURCE_ID=$(echo "$RESOURCE_RESP" | python3 -c "
import json, sys
data = json.load(sys.stdin)
print(data['data']['id'])
" 2>/dev/null || true)
  if [[ -z "$RESOURCE_ID" ]]; then
    echo "WARNING: Could not add to status page. Response:" >&2
    echo "$RESOURCE_RESP" >&2
  else
    echo "CREATED: Status page resource ID $RESOURCE_ID"
  fi
fi

# ── Step 4: Verify final state ───────────────────────────────────────────
echo "==> Verifying monitor $MONITOR_ID ..."
VERIFY=$(curl -sf -H "$AUTH_HEADER" "$API_BASE/monitors/$MONITOR_ID")
echo "$VERIFY" | python3 -c "
import json, sys
attrs = json.load(sys.stdin)['data']['attributes']
print('  url:             ', attrs['url'])
print('  monitor_type:    ', attrs['monitor_type'])
print('  required_keyword:', repr(attrs['required_keyword']))
print('  check_frequency: ', attrs['check_frequency'], 's')
print('  status:          ', attrs['status'])
print('  policy_id:       ', attrs['policy_id'], '(null = PD not yet wired, see rb-storage-fallback.md §5)')
"

echo ""
echo "DONE. Monitor ID: $MONITOR_ID"
echo "Next: wire PagerDuty via BetterStack console — see specs/_runbooks/rb-storage-fallback.md §5"
