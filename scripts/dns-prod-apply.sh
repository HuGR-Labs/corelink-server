#!/usr/bin/env bash
# scripts/dns-prod-apply.sh
#
# Wave 32 Phase G — DNS Production Apply
#
# Reads the JSON plan from dns-prod-plan.sh --plan-json and applies each
# record to the CF humangr.com zone via the DNS API.
#
# Usage:
#   ./scripts/dns-prod-apply.sh [--dry-run | --apply | --rollback SNAPSHOT_FILE]
#
# Default: --dry-run (safe by default — no changes applied)
#
# Rollback snapshot stored at: /tmp/dns-rollback-<TIMESTAMP>.json
#   Snapshot format documented below.
#
# Per-record log format:
#   [N/M] ACTION name -> target TYPE proxied=BOOL ... OK (id=<cf_record_id>)
#   [N/M] ACTION name -> target TYPE proxied=BOOL ... DRY-RUN (no change)
#   [N/M] ACTION name -> target TYPE proxied=BOOL ... SKIPPED (no-op)
#   [N/M] ACTION name -> target TYPE proxied=BOOL ... ERROR: <reason>
#
# Idempotency: re-applying on already-correct state = no-op (SKIPPED).
#
# Charter: CTRL-CRED-001 — no secrets emitted; DNS records are public data.
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

CF_API_TOKEN="${CF_API_TOKEN:-${CLOUDFLARE_API_TOKEN:-}}"
CF_ZONE_ID="${CF_ZONE_ID:-${CLOUDFLARE_ZONE_ID_HUMANGR:-}}"
CF_ACCOUNT_ID="${CF_ACCOUNT_ID:-${CLOUDFLARE_ACCOUNT_ID:-}}"

CF_API_BASE="https://api.cloudflare.com/client/v4"

# ---------------------------------------------------------------------------
# Parse args
# ---------------------------------------------------------------------------
MODE="--dry-run"
ROLLBACK_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)    MODE="--dry-run"; shift ;;
    --apply)      MODE="--apply";   shift ;;
    --rollback)   MODE="--rollback"; ROLLBACK_FILE="${2:-}"; shift 2 ;;
    *)
      echo "Usage: $0 [--dry-run | --apply | --rollback SNAPSHOT_FILE]" >&2
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Credential check
# ---------------------------------------------------------------------------
if [[ -z "${CF_API_TOKEN}" ]] || [[ -z "${CF_ZONE_ID}" ]]; then
  echo "ERROR: CF credentials absent. Set CLOUDFLARE_API_TOKEN + CLOUDFLARE_ZONE_ID_HUMANGR." >&2
  echo "HARD PAUSE TRIGGER 1: CF credentials absent." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
cf_api() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  if [[ -n "${body}" ]]; then
    curl -sf -X "${method}" "${CF_API_BASE}${path}" \
      -H "Authorization: Bearer ${CF_API_TOKEN}" \
      -H "Content-Type: application/json" \
      -d "${body}"
  else
    curl -sf -X "${method}" "${CF_API_BASE}${path}" \
      -H "Authorization: Bearer ${CF_API_TOKEN}" \
      -H "Content-Type: application/json"
  fi
}

get_record_id() {
  local name="$1"
  local type="$2"
  cf_api GET "/zones/${CF_ZONE_ID}/dns_records?name=${name}&type=${type}" | \
    python3 -c "
import json, sys
data = json.load(sys.stdin)
if data.get('result') and len(data['result']) > 0:
    print(data['result'][0]['id'])
" 2>/dev/null || true
}

get_live_record() {
  local name="$1"
  local type="$2"
  cf_api GET "/zones/${CF_ZONE_ID}/dns_records?name=${name}&type=${type}" | \
    python3 -c "
import json, sys
data = json.load(sys.stdin)
if data.get('result') and len(data['result']) > 0:
    r = data['result'][0]
    print(json.dumps({'id': r['id'], 'name': r['name'], 'type': r['type'],
                      'content': r['content'], 'proxied': r.get('proxied', False),
                      'ttl': r.get('ttl', 1)}))
" 2>/dev/null || echo "{}"
}

# ---------------------------------------------------------------------------
# Rollback: restore from snapshot
# ---------------------------------------------------------------------------
if [[ "${MODE}" == "--rollback" ]]; then
  if [[ -z "${ROLLBACK_FILE}" ]] || [[ ! -f "${ROLLBACK_FILE}" ]]; then
    echo "ERROR: --rollback requires a valid snapshot file path." >&2
    exit 1
  fi
  echo "==> ROLLBACK mode: restoring from ${ROLLBACK_FILE}"
  python3 - "${ROLLBACK_FILE}" "${CF_API_TOKEN}" "${CF_ZONE_ID}" <<'PYEOF'
import json, sys, urllib.request, urllib.error

snapshot_path, token, zone_id = sys.argv[1], sys.argv[2], sys.argv[3]
with open(snapshot_path) as f:
    snap = json.load(f)

base = f"https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records"
headers = {"Authorization": f"Bearer {token}", "Content-Type": "application/json"}

total = len(snap["pre_change_state"])
for i, rec in enumerate(snap["pre_change_state"], 1):
    action = rec.get("_rollback_action", "DELETE_NEW")
    name = rec.get("name", "?")
    if action == "DELETE_NEW":
        # Record was created by apply → delete it
        rec_id = rec.get("_applied_id")
        if not rec_id:
            print(f"[{i}/{total}] ROLLBACK SKIP {name}: no applied_id recorded")
            continue
        req = urllib.request.Request(f"{base}/{rec_id}", method="DELETE", headers=headers)
        try:
            with urllib.request.urlopen(req) as r:
                print(f"[{i}/{total}] ROLLBACK DELETE {name} (id={rec_id}) OK")
        except urllib.error.HTTPError as e:
            print(f"[{i}/{total}] ROLLBACK DELETE {name} ERROR: {e.code} {e.reason}")
    elif action == "RESTORE_ORIGINAL":
        # Record was updated → restore original content
        rec_id = rec.get("_applied_id", rec.get("id"))
        body = json.dumps({"name": rec["name"], "type": rec["type"],
                           "content": rec["content"], "proxied": rec["proxied"],
                           "ttl": rec["ttl"]}).encode()
        req = urllib.request.Request(f"{base}/{rec_id}", data=body, method="PATCH", headers=headers)
        try:
            with urllib.request.urlopen(req) as r:
                print(f"[{i}/{total}] ROLLBACK RESTORE {name} (id={rec_id}) OK")
        except urllib.error.HTTPError as e:
            print(f"[{i}/{total}] ROLLBACK RESTORE {name} ERROR: {e.code} {e.reason}")

print("Rollback complete.")
PYEOF
  exit 0
fi

# ---------------------------------------------------------------------------
# Generate plan JSON from dns-prod-plan.sh
# ---------------------------------------------------------------------------
PLAN_JSON=$("${SCRIPT_DIR}/dns-prod-plan.sh" --plan-json)
if [[ -z "${PLAN_JSON}" ]]; then
  echo "ERROR: dns-prod-plan.sh --plan-json returned empty output." >&2
  exit 1
fi

RECORDS=$(echo "${PLAN_JSON}" | python3 -c "import json,sys; d=json.load(sys.stdin); print(json.dumps(d['records']))")
TOTAL=$(echo "${RECORDS}" | python3 -c "import json,sys; print(len(json.load(sys.stdin)))")

# ---------------------------------------------------------------------------
# Pre-change snapshot for rollback
# ---------------------------------------------------------------------------
SNAPSHOT_TS=$(date -u +%Y%m%dT%H%M%SZ)
SNAPSHOT_FILE="/tmp/dns-rollback-${SNAPSHOT_TS}.json"
SNAPSHOT_ENTRIES="[]"

echo "==> Wave 32 Phase G DNS Apply"
echo "    Mode: ${MODE}"
echo "    Zone: humangr.com (${CF_ZONE_ID})"
echo "    Records: ${TOTAL}"
echo "    Snapshot (rollback): ${SNAPSHOT_FILE}"
echo ""

if [[ "${MODE}" == "--apply" ]]; then
  echo "    !!! APPLY MODE — DNS changes will be made to humangr.com !!!"
  echo "    Ctrl-C within 5 seconds to abort..."
  sleep 5
fi

# ---------------------------------------------------------------------------
# Process each record
# ---------------------------------------------------------------------------
i=0
SNAPSHOT_LIST=()

while IFS= read -r record; do
  i=$((i + 1))
  name=$(echo "${record}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r['name'])")
  type=$(echo "${record}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r['type'])")
  content=$(echo "${record}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r['content'])")
  proxied=$(echo "${record}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(str(r['proxied']).lower())")
  ttl=$(echo "${record}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r['ttl'])")
  notes=$(echo "${record}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r.get('notes',''))")

  prefix="[${i}/${TOTAL}]"
  desc="${name} -> ${content} ${type} proxied=${proxied}"

  # Check live state
  live_raw=$(get_live_record "${name}" "${type}")
  live_content=$(echo "${live_raw}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r.get('content',''))" 2>/dev/null || echo "")
  live_id=$(echo "${live_raw}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(r.get('id',''))" 2>/dev/null || echo "")
  live_proxied=$(echo "${live_raw}" | python3 -c "import json,sys; r=json.load(sys.stdin); print(str(r.get('proxied',False)).lower())" 2>/dev/null || echo "")

  if [[ -z "${live_content}" ]]; then
    # Record does not exist → CREATE
    SNAPSHOT_LIST+=("{\"_rollback_action\":\"DELETE_NEW\",\"name\":\"${name}\",\"type\":\"${type}\",\"content\":\"${content}\"}")

    if [[ "${MODE}" == "--dry-run" ]]; then
      echo "${prefix} CREATE ${desc} ... DRY-RUN (no change) [${notes}]"
    else
      body="{\"name\":\"${name}\",\"type\":\"${type}\",\"content\":\"${content}\",\"proxied\":${proxied},\"ttl\":${ttl}}"
      response=$(cf_api POST "/zones/${CF_ZONE_ID}/dns_records" "${body}") || {
        echo "${prefix} CREATE ${desc} ... ERROR: API call failed" >&2
        continue
      }
      success=$(echo "${response}" | python3 -c "import json,sys; print(json.load(sys.stdin).get('success'))")
      if [[ "${success}" == "True" ]]; then
        new_id=$(echo "${response}" | python3 -c "import json,sys; print(json.load(sys.stdin)['result']['id'])")
        # Update snapshot with applied ID
        SNAPSHOT_LIST[-1]="{\"_rollback_action\":\"DELETE_NEW\",\"_applied_id\":\"${new_id}\",\"name\":\"${name}\",\"type\":\"${type}\",\"content\":\"${content}\"}"
        echo "${prefix} CREATE ${desc} ... OK (id=${new_id})"
      else
        err=$(echo "${response}" | python3 -c "import json,sys; print(json.load(sys.stdin).get('errors'))")
        echo "${prefix} CREATE ${desc} ... ERROR: ${err}"
      fi
    fi

  elif [[ "${live_content}" == "${content}" ]] && [[ "${live_proxied}" == "${proxied}" ]]; then
    # Already correct → NO-OP
    echo "${prefix} SKIPPED ${desc} ... no-op (live matches plan)"

  else
    # Exists but differs → UPDATE
    SNAPSHOT_LIST+=("{\"_rollback_action\":\"RESTORE_ORIGINAL\",\"_applied_id\":\"${live_id}\",\"name\":\"${name}\",\"type\":\"${type}\",\"content\":\"${live_content}\",\"proxied\":${live_proxied},\"ttl\":1}")

    if [[ "${MODE}" == "--dry-run" ]]; then
      echo "${prefix} UPDATE ${desc} (live: ${live_content}, proxied=${live_proxied}) ... DRY-RUN (no change)"
    else
      body="{\"name\":\"${name}\",\"type\":\"${type}\",\"content\":\"${content}\",\"proxied\":${proxied},\"ttl\":${ttl}}"
      response=$(cf_api PATCH "/zones/${CF_ZONE_ID}/dns_records/${live_id}" "${body}") || {
        echo "${prefix} UPDATE ${desc} ... ERROR: API call failed" >&2
        continue
      }
      success=$(echo "${response}" | python3 -c "import json,sys; print(json.load(sys.stdin).get('success'))")
      if [[ "${success}" == "True" ]]; then
        echo "${prefix} UPDATE ${desc} ... OK (id=${live_id})"
      else
        err=$(echo "${response}" | python3 -c "import json,sys; print(json.load(sys.stdin).get('errors'))")
        echo "${prefix} UPDATE ${desc} ... ERROR: ${err}"
      fi
    fi
  fi

done < <(echo "${RECORDS}" | python3 -c "
import json, sys
records = json.load(sys.stdin)
for r in records:
    print(json.dumps(r))
")

# ---------------------------------------------------------------------------
# Write rollback snapshot
# ---------------------------------------------------------------------------
SNAPSHOT_JSON="{\"created_at\":\"${SNAPSHOT_TS}\",\"zone_id\":\"${CF_ZONE_ID}\",\"mode\":\"${MODE}\",\"pre_change_state\":["
FIRST=true
for entry in "${SNAPSHOT_LIST[@]:-}"; do
  [[ -z "${entry}" ]] && continue
  if [[ "${FIRST}" == "true" ]]; then
    SNAPSHOT_JSON+="${entry}"
    FIRST=false
  else
    SNAPSHOT_JSON+=",${entry}"
  fi
done
SNAPSHOT_JSON+="]}"

echo "${SNAPSHOT_JSON}" > "${SNAPSHOT_FILE}"
echo ""
echo "==> Done. Rollback snapshot: ${SNAPSHOT_FILE}"
echo "    To rollback: ./scripts/dns-prod-apply.sh --rollback ${SNAPSHOT_FILE}"
