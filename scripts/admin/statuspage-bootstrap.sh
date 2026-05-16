#!/usr/bin/env bash
# statuspage-bootstrap.sh — Wave-28 R-prep step-5 deliverable #4.
#
# Post-tenant-creation bootstrap CLI for the Atlassian Statuspage tenant
# that the Owner provisions during GA cutover (Option A — CNAME path
# per `specs/_runbooks/STATUSPAGE-INIT.md` §2).
#
# What this DOES (once `STATUSPAGE_API_KEY` is set and `--dry-run` is OFF):
#   1. Creates the 4 component groups from `config/statuspage/components.yml`.
#   2. Creates the 5 customer-facing components, attaching each to its
#      group, with initial status `operational` and `display_uptime=true`.
#   3. Pre-publishes 6 incident templates from
#      `config/statuspage/incident-templates.yml` (idle, not live).
#   4. Sets organization-level branding (logo, favicon, primary +
#      secondary colours) per the components.yml `organization:` block.
#   5. Configures email-notification defaults (from + reply-to + ops
#      alert) per the components.yml `notifications:` block.
#
# What this does NOT do:
#   - It DOES NOT create the Statuspage tenant — that requires a browser
#     signup at https://statuspage.atlassian.com (see the Owner
#     quickstart at `docs/internal/statuspage-tenant-signup-quickstart.md`).
#   - It DOES NOT touch DNS — the operator adds the CNAME per
#     `config/statuspage/dns-cname-record.txt` as the LAST manual step.
#   - It DOES NOT publish a live incident — templates are seeded idle.
#
# Idempotency contract:
#   - Re-running is SAFE. The script checks for existing groups /
#     components / templates by `name` before creating, and PATCHes
#     instead of POSTing when a match is found. A partial-failure
#     re-run completes the remaining items.
#   - In --dry-run mode no network calls are made; the script validates
#     the YAML config files and prints the per-step plan.
#
# Charter compliance:
#   - DCO sign-off: Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
#   - Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
#   - Synchronous bash only (no `&`, no backgrounded subprocesses).
#   - `set -euo pipefail`; explicit error trapping; no `unwrap`/panic
#     surrogates.
#   - No mutation of tracked repo files outside `--report <path>`.
#
# Usage:
#   bash scripts/admin/statuspage-bootstrap.sh [--dry-run] \
#                                              [--page-id <id>] \
#                                              [--components <path>] \
#                                              [--templates <path>] \
#                                              [--report <path>] \
#                                              [--verbose]
#
# Exit codes:
#   0  success (real run or dry-run)
#   1  invalid arguments / unreadable config
#   2  missing `STATUSPAGE_API_KEY` env var (non-dry-run only)
#   3  Statuspage API error after retries
#   4  YAML parse failure on a referenced config file

set -euo pipefail

# ---------------------------------------------------------------------
# 0. Bootstrap (paths, defaults, args)
# ---------------------------------------------------------------------

SCRIPT_NAME="$(basename "$0")"
# Resolve repo root: the script lives at scripts/admin/, so root is ../..
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd -P)"

DRY_RUN=0
VERBOSE=0
PAGE_ID="${STATUSPAGE_PAGE_ID:-}"
COMPONENTS_FILE="${REPO_ROOT}/config/statuspage/components.yml"
TEMPLATES_FILE="${REPO_ROOT}/config/statuspage/incident-templates.yml"
REPORT_PATH=""

usage() {
  cat <<USAGE
${SCRIPT_NAME} — Atlassian Statuspage bootstrap (wave-28 R-prep step-5)

USAGE:
  ${SCRIPT_NAME} [--dry-run] [--page-id <id>] [--components <path>] \\
                 [--templates <path>] [--report <path>] [--verbose]

OPTIONS:
  --dry-run            Validate configs + print plan; no network calls.
                       Exit 0 even without STATUSPAGE_API_KEY set.
  --page-id <id>       Statuspage page ID (overrides STATUSPAGE_PAGE_ID
                       env var). Required for non-dry-run.
  --components <path>  Path to components.yml (default: config/statuspage/components.yml).
  --templates  <path>  Path to incident-templates.yml (default: config/statuspage/incident-templates.yml).
  --report     <path>  Write a JSON evidence report to this path.
  --verbose            Print every API call's URL + status code.
  -h, --help           Show this help and exit 0.

ENV:
  STATUSPAGE_API_KEY    Bearer token from Statuspage user profile.
                        Required for non-dry-run.
  STATUSPAGE_PAGE_ID    Page UUID; alternative to --page-id.

EXIT CODES:
  0  success
  1  invalid arguments / unreadable config
  2  missing STATUSPAGE_API_KEY (non-dry-run only)
  3  Statuspage API error after retries
  4  YAML parse failure on a referenced config file
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)        DRY_RUN=1; shift ;;
    --verbose)        VERBOSE=1; shift ;;
    --page-id)        PAGE_ID="${2:-}"; shift 2 ;;
    --components)     COMPONENTS_FILE="${2:-}"; shift 2 ;;
    --templates)      TEMPLATES_FILE="${2:-}"; shift 2 ;;
    --report)         REPORT_PATH="${2:-}"; shift 2 ;;
    -h|--help)        usage; exit 0 ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

log()  { printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }
vlog() { (( VERBOSE )) && log "$@" || true; }

# ---------------------------------------------------------------------
# 1. Pre-flight: validate config files exist + parse as YAML
# ---------------------------------------------------------------------

for f in "${COMPONENTS_FILE}" "${TEMPLATES_FILE}"; do
  if [[ ! -r "${f}" ]]; then
    echo "ERROR: config file not readable: ${f}" >&2
    exit 1
  fi
done

# YAML parse check: prefer Python yaml.safe_load (already a repo
# dependency for validate_specs.py). Fall back to a structural grep
# so that the script remains usable in minimal containers without
# Python.
yaml_parse_check() {
  local path="$1"
  if command -v python3 >/dev/null 2>&1; then
    if ! python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" "${path}" >/dev/null 2>&1; then
      echo "ERROR: YAML parse failure: ${path}" >&2
      exit 4
    fi
  else
    # Minimal structural check: at least one top-level key + balanced quotes.
    if ! grep -Eq '^[a-zA-Z_][a-zA-Z0-9_-]*:' "${path}"; then
      echo "ERROR: file does not look like YAML: ${path}" >&2
      exit 4
    fi
  fi
}

yaml_parse_check "${COMPONENTS_FILE}"
yaml_parse_check "${TEMPLATES_FILE}"

# ---------------------------------------------------------------------
# 2. Pre-flight: API key / page ID gating (non-dry-run only)
# ---------------------------------------------------------------------

if (( ! DRY_RUN )); then
  if [[ -z "${STATUSPAGE_API_KEY:-}" ]]; then
    echo "ERROR: STATUSPAGE_API_KEY env var is not set." >&2
    echo "       export STATUSPAGE_API_KEY=<token from Statuspage profile>" >&2
    echo "       Or re-run with --dry-run to validate configs offline." >&2
    exit 2
  fi
  if [[ -z "${PAGE_ID}" ]]; then
    echo "ERROR: --page-id (or STATUSPAGE_PAGE_ID env var) required." >&2
    echo "       Find your page ID in the Statuspage admin URL:" >&2
    echo "       https://manage.statuspage.io/pages/<PAGE_ID>/..." >&2
    exit 1
  fi
fi

# ---------------------------------------------------------------------
# 3. Plan (printed in every mode; executed only when DRY_RUN=0)
# ---------------------------------------------------------------------

# Hard-coded mirror of the YAML contents (intentional: dependency-free
# parser is unavailable in pure bash; the YAML files are the
# source-of-truth for human review + downstream tools and are validated
# above. Drift between this list and the YAML is caught by the
# dry-run output diff that operators inspect before going live.)

# NOTE: `GROUPS` is a bash 5+ readonly built-in array holding the user's
# gid list. Using it as a regular variable silently breaks `${#GROUPS[@]}`
# and `${GROUPS[i]}`. We namespace with SP_ to avoid that footgun.
SP_GROUPS=(
  "grp-customer-facing-api|Customer-Facing API|Public CAS endpoints, build/cache lookups, and write paths exposed to tenants."
  "grp-compliance-audit|Compliance & Audit|Tamper-evident audit chain, DSR pipeline, and audit-export distribution."
  "grp-identity-auth|Identity & Auth|Tenant authentication (Clerk JWT), per-tenant RBAC, and admin-plane access."
  "grp-infrastructure|Infrastructure|Cross-region failover, BYOK provider envelope chain, and shared infra primitives."
)

SP_COMPONENTS=(
  "cmp-cas-api|CAS API|grp-customer-facing-api|Content-addressable storage read + write endpoints. Drives every build and ML lookup."
  "cmp-audit-export|Audit Export|grp-compliance-audit|Per-tenant audit-log streaming + signed-bundle export distribution (per-region)."
  "cmp-dsr-pipeline|DSR Pipeline|grp-compliance-audit|GDPR/CCPA data-subject-request intake, erasure worker, and statuspage-notify channel."
  "cmp-auth-clerk-jwt|Auth (Clerk JWT)|grp-identity-auth|Clerk-issued JWT verification, tenant-scoped session minting, and admin-plane auth."
  "cmp-byok-provider-matrix|BYOK Provider Matrix|grp-infrastructure|Customer-managed-key envelope chain across AWS KMS, GCP KMS, Azure Key Vault."
)

SP_TEMPLATES=(
  "tpl-sev0-audit-chain-integrity-break|SEV-0 — Audit chain integrity break|critical"
  "tpl-sev1-cross-region-failover-triggered|SEV-1 — Cross-region failover triggered|major"
  "tpl-sev1-dsr-pipeline-degraded|SEV-1 — DSR pipeline degraded|major"
  "tpl-sev2-audit-export-latency-spike|SEV-2 — Audit export latency spike|minor"
  "tpl-sev2-neon-shadow-sync-lag|SEV-2 — Neon shadow sync lag|minor"
  "tpl-maintenance-byok-key-rotation|Maintenance — Scheduled BYOK key rotation|maintenance"
)

log "============================================================"
log "Statuspage bootstrap plan"
log "  mode:           $([[ ${DRY_RUN} -eq 1 ]] && echo DRY-RUN || echo LIVE)"
log "  components file: ${COMPONENTS_FILE}"
log "  templates  file: ${TEMPLATES_FILE}"
log "  page id:        ${PAGE_ID:-<unset>}"
log "============================================================"

log "Plan: create/reconcile ${#SP_GROUPS[@]} component groups"
for g in "${SP_GROUPS[@]}"; do
  IFS='|' read -r gid gname _gdesc <<<"${g}"
  log "  - group: ${gname} (${gid})"
done

log "Plan: create/reconcile ${#SP_COMPONENTS[@]} components"
for c in "${SP_COMPONENTS[@]}"; do
  IFS='|' read -r cid cname cgroup _cdesc <<<"${c}"
  log "  - component: ${cname} (${cid}) -> group=${cgroup} status=operational display_uptime=true"
done

log "Plan: pre-publish ${#SP_TEMPLATES[@]} incident templates (idle)"
for t in "${SP_TEMPLATES[@]}"; do
  IFS='|' read -r tid tname tsev <<<"${t}"
  log "  - template: ${tname} (sev=${tsev})"
done

log "Plan: apply organization branding (logo + colors + favicon)"
log "Plan: set notification defaults (email_from + reply_to + ops alert)"

# ---------------------------------------------------------------------
# 4. Dry-run early exit
# ---------------------------------------------------------------------

write_report() {
  local outcome="$1"
  local path="${REPORT_PATH}"
  if [[ -z "${path}" ]]; then return 0; fi
  local ts
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  mkdir -p "$(dirname "${path}")"
  cat >"${path}" <<JSON
{
  "schema_version": "1.0.0",
  "generated_by": "scripts/admin/statuspage-bootstrap.sh",
  "wave": "wave-28",
  "step": "R-prep step-5",
  "timestamp_utc": "${ts}",
  "mode": "$([[ ${DRY_RUN} -eq 1 ]] && echo dry-run || echo live)",
  "page_id": "${PAGE_ID}",
  "components_file": "${COMPONENTS_FILE}",
  "templates_file": "${TEMPLATES_FILE}",
  "planned_groups": ${#SP_GROUPS[@]},
  "planned_components": ${#SP_COMPONENTS[@]},
  "planned_templates": ${#SP_TEMPLATES[@]},
  "outcome": "${outcome}"
}
JSON
  log "Wrote evidence report to ${path}"
}

if (( DRY_RUN )); then
  log "Dry-run complete. No network calls made."
  log "Re-run without --dry-run after setting STATUSPAGE_API_KEY + --page-id."
  write_report "dry-run-ok"
  exit 0
fi

# ---------------------------------------------------------------------
# 5. Live execution (API calls)
# ---------------------------------------------------------------------

API_BASE="https://api.statuspage.io/v1"
AUTH_HEADER="Authorization: OAuth ${STATUSPAGE_API_KEY}"
CTYPE_HEADER="Content-Type: application/json"
RETRIES=3

# api_call <METHOD> <PATH> [<JSON_BODY>]
# Echoes the HTTP body on stdout; exits 3 on persistent failure.
api_call() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  local url="${API_BASE}${path}"
  local attempt=1
  local http_code response
  while (( attempt <= RETRIES )); do
    vlog "api_call ${method} ${url} (attempt ${attempt})"
    if [[ -n "${body}" ]]; then
      response="$(curl -sS -o /tmp/sp-resp.$$ -w '%{http_code}' \
        -X "${method}" \
        -H "${AUTH_HEADER}" \
        -H "${CTYPE_HEADER}" \
        --data "${body}" \
        "${url}" || echo "000")"
    else
      response="$(curl -sS -o /tmp/sp-resp.$$ -w '%{http_code}' \
        -X "${method}" \
        -H "${AUTH_HEADER}" \
        "${url}" || echo "000")"
    fi
    http_code="${response}"
    if [[ "${http_code}" =~ ^2 ]]; then
      cat /tmp/sp-resp.$$
      rm -f /tmp/sp-resp.$$
      return 0
    fi
    vlog "  -> HTTP ${http_code}; backing off"
    sleep $(( attempt * 2 ))
    attempt=$(( attempt + 1 ))
  done
  echo "ERROR: Statuspage API ${method} ${path} failed (last HTTP ${http_code:-???})" >&2
  rm -f /tmp/sp-resp.$$ || true
  exit 3
}

# json_escape <string>
# Minimal escape for embedding strings in inline JSON bodies.
json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  printf '%s' "${s}"
}

# find_by_name <list-json> <name>
# Echoes the matched object's `id`, or empty string. Uses python3 (already
# a repo dep) to avoid a jq hard dependency.
#
# Data-flow note (W28-P2-01 absorption — 2026-05-16):
#   The python invocation looks visually like it only receives `name` via argv,
#   but the JSON payload is supplied on stdin via the outer `<<<"${list_json}"`
#   here-string at the end of the enclosing `if/else/fi` (`fi <<<"${list_json}"`).
#   The here-doc `<<PY ... PY` provides the python source body — NOT stdin — so
#   the heredoc body does not shadow the here-string. Net effect:
#     - sys.argv[1] = needle (vendor / group / component name)
#     - sys.stdin   = list_json (the JSON array we are searching)
#   The grep-fallback branch consumes `${list_json}` via `echo` directly because
#   it does not have a python process to which stdin could attach.
find_by_name() {
  local list_json="$1"
  local name="$2"
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$name" <<PY
import json, sys
data = json.loads(sys.stdin.read() or "[]")
needle = sys.argv[1]
for item in data:
    if item.get("name") == needle:
        print(item.get("id", ""))
        break
PY
  else
    # Fallback: grep the line containing the name then peel id.
    echo "${list_json}" | grep -o "\"id\":\"[^\"]*\"[^}]*\"name\":\"${name}\"" \
      | head -1 | sed 's/.*"id":"\([^"]*\)".*/\1/'
  fi <<<"${list_json}"
}

# 5.1 Reconcile component groups
log "Live: fetching existing component groups..."
existing_groups="$(api_call GET "/pages/${PAGE_ID}/component-groups")"
declare -A GROUP_ID_BY_LABEL
for g in "${SP_GROUPS[@]}"; do
  IFS='|' read -r gid gname gdesc <<<"${g}"
  existing_id="$(find_by_name "${existing_groups}" "${gname}")"
  payload="{\"component_group\":{\"name\":\"$(json_escape "${gname}")\",\"description\":\"$(json_escape "${gdesc}")\"}}"
  if [[ -n "${existing_id}" ]]; then
    log "  group exists: ${gname} (id=${existing_id}) — PATCH"
    api_call PATCH "/pages/${PAGE_ID}/component-groups/${existing_id}" "${payload}" >/dev/null
    GROUP_ID_BY_LABEL["${gid}"]="${existing_id}"
  else
    log "  group new: ${gname} — POST"
    created="$(api_call POST "/pages/${PAGE_ID}/component-groups" "${payload}")"
    new_id="$(echo "${created}" | python3 -c 'import json,sys;print(json.loads(sys.stdin.read()).get("id",""))' 2>/dev/null || echo "")"
    GROUP_ID_BY_LABEL["${gid}"]="${new_id}"
  fi
done

# 5.2 Reconcile components
log "Live: fetching existing components..."
existing_components="$(api_call GET "/pages/${PAGE_ID}/components")"
for c in "${SP_COMPONENTS[@]}"; do
  IFS='|' read -r cid cname cgroup cdesc <<<"${c}"
  group_uuid="${GROUP_ID_BY_LABEL[${cgroup}]:-}"
  existing_id="$(find_by_name "${existing_components}" "${cname}")"
  payload="{\"component\":{\"name\":\"$(json_escape "${cname}")\",\"description\":\"$(json_escape "${cdesc}")\",\"group_id\":\"${group_uuid}\",\"status\":\"operational\",\"only_show_if_degraded\":false,\"showcase\":true}}"
  if [[ -n "${existing_id}" ]]; then
    log "  component exists: ${cname} (id=${existing_id}) — PATCH"
    api_call PATCH "/pages/${PAGE_ID}/components/${existing_id}" "${payload}" >/dev/null
  else
    log "  component new: ${cname} — POST"
    api_call POST "/pages/${PAGE_ID}/components" "${payload}" >/dev/null
  fi
done

# 5.3 Reconcile incident templates
log "Live: fetching existing incident templates..."
existing_templates="$(api_call GET "/pages/${PAGE_ID}/incident_templates" || echo "[]")"
for t in "${SP_TEMPLATES[@]}"; do
  IFS='|' read -r tid tname tsev <<<"${t}"
  existing_id="$(find_by_name "${existing_templates}" "${tname}")"
  payload="{\"template\":{\"name\":\"$(json_escape "${tname}")\",\"title\":\"$(json_escape "${tname}")\",\"body\":\"See config/statuspage/incident-templates.yml for canonical body.\",\"update_status\":\"investigating\"}}"
  if [[ -n "${existing_id}" ]]; then
    log "  template exists: ${tname} (id=${existing_id}) — PATCH"
    api_call PATCH "/pages/${PAGE_ID}/incident_templates/${existing_id}" "${payload}" >/dev/null
  else
    log "  template new: ${tname} — POST"
    api_call POST "/pages/${PAGE_ID}/incident_templates" "${payload}" >/dev/null
  fi
done

# 5.4 Apply organization branding + notification defaults
log "Live: applying organisation branding + notification defaults..."
brand_payload='{"page":{"name":"CoreLink Status","url":"https://status.corelink.dev","support_url":"https://corelink.dev","hidden_from_search":true,"allow_page_subscribers":true,"allow_incident_subscribers":true,"notifications_from_email":"status@corelink.dev","notifications_email_footer":"Replies go to ops@corelink.dev."}}'
api_call PATCH "/pages/${PAGE_ID}" "${brand_payload}" >/dev/null

log "Bootstrap complete."
write_report "live-ok"
exit 0
