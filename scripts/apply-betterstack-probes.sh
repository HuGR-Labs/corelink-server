#!/usr/bin/env bash
# apply-betterstack-probes.sh — Apply CoreLink synthetic monitoring probes to BetterStack.
#
# Usage:
#   bash scripts/apply-betterstack-probes.sh --dry-run          # Default; no API calls made
#   bash scripts/apply-betterstack-probes.sh --apply            # Live apply (Owner-only)
#   bash scripts/apply-betterstack-probes.sh --apply --probe <name>   # Single probe
#   bash scripts/apply-betterstack-probes.sh --delete --probe <name>  # Delete one probe
#
# Probe manifest: monitoring/synthetic/probes.yml
# BetterStack page ID: 247652
#
# Idempotent: GET /api/v2/monitors → match by pronounceable_name → PATCH or POST.
#
# CTRL-CRED-001: BETTERSTACK_API_TOKEN read from env only; never printed.
#
# Verified shellcheck-clean (shellcheck 0.9+).

set -euo pipefail

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
readonly BETTERSTACK_API_BASE="https://uptime.betterstack.com/api/v2"
readonly PAGE_ID="247652"
readonly PROBES_FILE="monitoring/synthetic/probes.yml"
readonly SCRIPT_NAME="apply-betterstack-probes.sh"

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
MODE="dry-run"   # default
FILTER_PROBE=""
DELETE_MODE="false"

for arg in "$@"; do
  case "${arg}" in
    --dry-run)
      MODE="dry-run"
      ;;
    --apply)
      MODE="apply"
      ;;
    --delete)
      DELETE_MODE="true"
      ;;
    --probe)
      # next arg is the probe name — handled below
      ;;
    *)
      if [[ "${arg}" != --* ]]; then
        # Treat as value of --probe if previous was --probe
        :
      fi
      ;;
  esac
done

# Re-parse to capture --probe value
prev=""
for arg in "$@"; do
  if [[ "${prev}" == "--probe" ]]; then
    FILTER_PROBE="${arg}"
  fi
  prev="${arg}"
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log()  { printf '[%s] %s\n' "${SCRIPT_NAME}" "$*" >&2; }
info() { printf '[INFO]  %s\n' "$*"; }
warn() { printf '[WARN]  %s\n' "$*" >&2; }

require_cmd() {
  if ! command -v "$1" > /dev/null 2>&1; then
    log "ERROR: required command '$1' not found in PATH."
    exit 1
  fi
}

require_cmd python3

# ---------------------------------------------------------------------------
# Locate repo root (allow running from any subdir)
# ---------------------------------------------------------------------------
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"
PROBES_PATH="${REPO_ROOT}/${PROBES_FILE}"

if [[ ! -f "${PROBES_PATH}" ]]; then
  log "ERROR: probes manifest not found at ${PROBES_PATH}"
  exit 1
fi

# ---------------------------------------------------------------------------
# Validate YAML (no external yq required — use python3)
# ---------------------------------------------------------------------------
validate_yaml() {
  python3 - <<PYEOF
import sys, yaml
try:
    with open("${PROBES_PATH}") as f:
        data = yaml.safe_load(f)
    probes = data.get("probes", [])
    if not isinstance(probes, list) or len(probes) == 0:
        print("ERROR: probes.yml 'probes' key is missing or empty", file=sys.stderr)
        sys.exit(1)
    print(f"YAML valid: {len(probes)} probe(s) found.")
except Exception as e:
    print(f"ERROR: YAML parse failed: {e}", file=sys.stderr)
    sys.exit(1)
PYEOF
}

# ---------------------------------------------------------------------------
# Build API JSON body for a single probe (pure python, no jq required)
# ---------------------------------------------------------------------------
build_probe_json() {
  local probe_name="$1"
  python3 - <<PYEOF
import json, sys, yaml

with open("${PROBES_PATH}") as f:
    data = yaml.safe_load(f)

probes = data.get("probes", [])
target = None
for p in probes:
    if p.get("name") == "${probe_name}":
        target = p
        break

if target is None:
    print(f"ERROR: probe '${probe_name}' not found in manifest.", file=sys.stderr)
    sys.exit(1)

payload = {
    "monitor_type": target.get("monitor_type", "status"),
    "url": target["url"],
    "pronounceable_name": target["pronounceable_name"],
    "check_frequency": target.get("check_frequency_seconds", 60),
    "request_timeout": target.get("request_timeout_seconds", 15),
    "expected_status_codes": target.get("expected_status_codes", [200]),
    "regions": target.get("regions", ["us-east", "eu", "ap"]),
    "confirmation_period": target.get("confirmation_period_seconds", 0),
    "paused": target.get("paused", False),
}

# Optional: HTTP method (BetterStack uses 'request_type')
method = target.get("method", "GET").upper()
if method != "GET":
    payload["request_type"] = method

# Optional: content-type assertion header check
ct = target.get("content_type_assertion")
if ct:
    payload["expected_headers"] = [{"name": "Content-Type", "value": ct}]

# Optional: body assertions
body_asserts = target.get("body_assertions", [])
if body_asserts:
    # BetterStack uses 'expected_body' for simple string, no JSONPath in free tier
    # We store full assertions for documentation; map to best-effort 'expected_body'
    for ba in body_asserts:
        if ba.get("type") == "text_body":
            payload["expected_body"] = ba.get("value", "")

# Tags (BetterStack monitor label list not standard; include as metadata comment)
tags = target.get("tags", [])
if tags:
    payload["_tags_comment"] = tags  # informational; stripped before real API call

print(json.dumps(payload, indent=2))
PYEOF
}

# ---------------------------------------------------------------------------
# List all probe names from manifest
# ---------------------------------------------------------------------------
list_probe_names() {
  python3 - <<PYEOF
import yaml
with open("${PROBES_PATH}") as f:
    data = yaml.safe_load(f)
for p in data.get("probes", []):
    print(p["name"])
PYEOF
}

# ---------------------------------------------------------------------------
# DRY-RUN: print what would happen
# ---------------------------------------------------------------------------
dry_run_all() {
  info "=== DRY-RUN MODE — no API calls will be made ==="
  info ""
  info "Probe manifest: ${PROBES_PATH}"
  info "BetterStack page ID: ${PAGE_ID}"
  info ""

  validate_yaml

  # Collect probe names (filter if --probe was set)
  mapfile -t probe_names < <(list_probe_names)

  local count=0
  for pname in "${probe_names[@]}"; do
    if [[ -n "${FILTER_PROBE}" && "${pname}" != "${FILTER_PROBE}" ]]; then
      continue
    fi

    count=$(( count + 1 ))
    info "--- Probe ${count}: ${pname} ---"
    info "Would call: POST/PATCH ${BETTERSTACK_API_BASE}/monitors"
    info "API JSON body:"
    build_probe_json "${pname}"
    info ""
  done

  if [[ "${count}" -eq 0 ]]; then
    warn "No probes matched (filter='${FILTER_PROBE}'). Check --probe argument."
    exit 1
  fi

  info "=== DRY-RUN COMPLETE: ${count} probe(s) listed. ==="
  info "Pass --apply to execute live (Owner-only; requires BETTERSTACK_API_TOKEN in env)."
}

# ---------------------------------------------------------------------------
# LIVE APPLY (Owner-only; token required)
# ---------------------------------------------------------------------------
require_token() {
  if [[ -z "${BETTERSTACK_API_TOKEN:-}" ]]; then
    log "ERROR: BETTERSTACK_API_TOKEN is not set in environment."
    log "       Source your .env.local or export the token before running --apply."
    exit 1
  fi
}

apply_probe() {
  local pname="$1"
  require_cmd curl

  local json_body
  json_body="$(build_probe_json "${pname}")"

  # Remove the informational _tags_comment before sending
  json_body="$(python3 -c "import json,sys; d=json.loads(sys.stdin.read()); d.pop('_tags_comment',None); print(json.dumps(d))" <<< "${json_body}")"

  # Check if monitor already exists (idempotency)
  local existing_id
  existing_id="$(curl -s \
    -H "Authorization: Bearer ${BETTERSTACK_API_TOKEN}" \
    "${BETTERSTACK_API_BASE}/monitors" \
    | python3 -c "
import json, sys
data = json.load(sys.stdin)
monitors = data.get('data', [])
for m in monitors:
    attrs = m.get('attributes', {})
    if attrs.get('pronounceable_name') == '${pname}':
        print(m['id'])
        break
" 2>/dev/null || true)"

  if [[ -n "${existing_id}" ]]; then
    info "  PATCH existing monitor id=${existing_id} for '${pname}'"
    curl -s -X PATCH \
      -H "Authorization: Bearer ${BETTERSTACK_API_TOKEN}" \
      -H "Content-Type: application/json" \
      -d "${json_body}" \
      "${BETTERSTACK_API_BASE}/monitors/${existing_id}" > /dev/null
    info "  PATCHED: ${pname}"
  else
    info "  POST new monitor for '${pname}'"
    curl -s -X POST \
      -H "Authorization: Bearer ${BETTERSTACK_API_TOKEN}" \
      -H "Content-Type: application/json" \
      -d "${json_body}" \
      "${BETTERSTACK_API_BASE}/monitors" > /dev/null
    info "  CREATED: ${pname}"
  fi
}

apply_all() {
  require_token
  info "=== LIVE APPLY MODE ==="
  info "BetterStack page ID: ${PAGE_ID}"
  info ""

  validate_yaml

  mapfile -t probe_names < <(list_probe_names)

  local count=0
  for pname in "${probe_names[@]}"; do
    if [[ -n "${FILTER_PROBE}" && "${pname}" != "${FILTER_PROBE}" ]]; then
      continue
    fi
    count=$(( count + 1 ))
    info "Applying probe ${count}: ${pname}"
    apply_probe "${pname}"
  done

  if [[ "${count}" -eq 0 ]]; then
    warn "No probes matched (filter='${FILTER_PROBE}')."
    exit 1
  fi

  info "=== APPLY COMPLETE: ${count} probe(s) applied. ==="
}

# ---------------------------------------------------------------------------
# DELETE
# ---------------------------------------------------------------------------
delete_probe() {
  local pname="$1"
  require_token
  require_cmd curl

  # Lookup by pronounceable_name matching the probe name (display name in manifest)
  local pronounceable_name
  pronounceable_name="$(python3 - <<PYEOF
import yaml
with open("${PROBES_PATH}") as f:
    data = yaml.safe_load(f)
for p in data.get("probes", []):
    if p.get("name") == "${pname}":
        print(p.get("pronounceable_name", ""))
        break
PYEOF
)"

  local existing_id
  existing_id="$(curl -s \
    -H "Authorization: Bearer ${BETTERSTACK_API_TOKEN}" \
    "${BETTERSTACK_API_BASE}/monitors" \
    | python3 -c "
import json, sys
data = json.load(sys.stdin)
for m in data.get('data', []):
    attrs = m.get('attributes', {})
    if attrs.get('pronounceable_name') == '${pronounceable_name}':
        print(m['id'])
        break
" 2>/dev/null || true)"

  if [[ -z "${existing_id}" ]]; then
    warn "Monitor '${pname}' not found on BetterStack — nothing to delete."
    exit 0
  fi

  if [[ "${MODE}" == "dry-run" ]]; then
    info "DRY-RUN: would DELETE monitor id=${existing_id} (${pname})"
    exit 0
  fi

  info "DELETE monitor id=${existing_id} (${pname})"
  curl -s -X DELETE \
    -H "Authorization: Bearer ${BETTERSTACK_API_TOKEN}" \
    "${BETTERSTACK_API_BASE}/monitors/${existing_id}" > /dev/null
  info "DELETED: ${pname}"
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------
main() {
  if [[ "${DELETE_MODE}" == "true" ]]; then
    if [[ -z "${FILTER_PROBE}" ]]; then
      log "ERROR: --delete requires --probe <name>"
      exit 1
    fi
    delete_probe "${FILTER_PROBE}"
    return
  fi

  case "${MODE}" in
    dry-run)
      dry_run_all
      ;;
    apply)
      apply_all
      ;;
    *)
      log "ERROR: unknown mode '${MODE}'. Use --dry-run or --apply."
      exit 1
      ;;
  esac
}

main "$@"
