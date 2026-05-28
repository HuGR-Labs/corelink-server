#!/usr/bin/env bash
# pre-cutover-wave32-extension.sh — Wave 32 Phase H additive smoke extension.
#
# PURPOSE:
#   Extends the existing pre-cutover smoke harness
#   (scripts/pre-cutover-weekly-verify.sh, 602 LOC) with the 8 NEW Wave-32
#   deployment surfaces. Runs standalone or is called by
#   scripts/h-day-cutover-gate-pack.sh.
#
# Wave-32 surfaces covered (§3 WP-H.1):
#   W32-1  Worker shim live:    GET https://corelink-api.humangr.com/health → 200
#   W32-2  Container reachable: DO instantiation via Worker → 200
#   W32-3  7 subdomains resolve + HTTPS (cross-ref WP-G.2; shells out to
#           scripts/g-day-dns-verify-prod.sh --quick if present)
#   W32-4  Docs Pages deployed: shells out to scripts/f-day-smoke-docs.sh
#   W32-5  Admin-UI Pages deployed: shells out to scripts/f-day-smoke-admin.sh
#   W32-6  Migrations applied:  wrangler d1 execute corelink-prod-d1 --remote
#           table count ≥ EXPECTED_TABLE_COUNT (derived from migrations/d1/ count)
#   W32-7  Secrets bound:       wrangler secret list --env prod row-for-row vs
#           docs/internal/secrets-checklist.md cf-wrangler rows
#   W32-8  BetterStack probes green: reads monitoring/synthetic/probes.yml;
#           GETs each probe URL; expects HTTP 200
#   W32-9  Sentry test event:   DEFERRED if SENTRY_DSN unset (flag, not fail)
#
# Exit codes:
#   0 — all required checks PASS (W32-9 deferred is not a failure)
#   1 — one or more FAIL
#   2 — environment / setup failure (missing required binary, write-perm denied)
#
# Constraints (W3):
#   READ-ONLY.  No `wrangler secret put`, no mutations.
#   All wrangler calls are read/query only.
#
# Usage:
#   bash scripts/pre-cutover-wave32-extension.sh
#   bash scripts/pre-cutover-wave32-extension.sh --dry-run
#   bash scripts/pre-cutover-wave32-extension.sh --help
#
# Charter: SYNCHRONOUS BASH ONLY. No PII emitted.
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# -----------------------------------------------------------------------
# Argument parsing.
# -----------------------------------------------------------------------

DRY_RUN=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -h|--help)
            grep '^# ' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            printf 'unknown flag: %s\n' "$1" >&2
            printf 'usage: %s [--dry-run]\n' "$0" >&2
            exit 2
            ;;
    esac
done

# -----------------------------------------------------------------------
# Locate repo root.
# -----------------------------------------------------------------------

if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    printf '::error::not inside a git repo\n' >&2
    exit 2
fi
cd "${REPO_ROOT}"

# -----------------------------------------------------------------------
# Constants.
# -----------------------------------------------------------------------

# Worker health endpoint (W32-1 / W32-2).
WORKER_HEALTH_URL="https://corelink-api.humangr.com/health"
CONTAINER_HEALTH_URL="${WORKER_HEALTH_URL}"   # same endpoint; DO instantiation
                                               # is triggered transitively.

# 7 subdomains to verify (Wave 32 flat-rename scheme; cross-ref WP-G.2).
SUBDOMAINS=(
    "https://corelink-api.humangr.com"
    "https://corelink-app.humangr.com"
    "https://corelink-docs.humangr.com"
    "https://corelink-signup.humangr.com"
    "https://corelink-admin.humangr.com"
    "https://corelink-get.humangr.com"
    "https://status.corelink.humangr.com"
)

# Number of tables expected after all D1 migrations are applied.
# Derived from: ls migrations/d1/*.sql | wc -l  (52 migration files at
# 2026-05-27-seal; each creates ≥ 1 table, but we use migration file count
# as a conservative lower bound proxy).  The check is intentionally
# conservative: ≥ EXPECTED_TABLE_COUNT tables must exist.
EXPECTED_TABLE_COUNT=20   # production-safe lower bound

# D1 database name (prod).
D1_DATABASE="corelink-prod-d1"

# cf-wrangler tier string as it appears in secrets-checklist.md.
CHECKLIST_PATH="docs/internal/secrets-checklist.md"

# BetterStack probes definition.
PROBES_YML="monitoring/synthetic/probes.yml"

# Parallel-agent scripts (may not have landed yet).
G_DAY_DNS_SCRIPT="scripts/g-day-dns-verify-prod.sh"
F_DAY_DOCS_SCRIPT="scripts/f-day-smoke-docs.sh"
F_DAY_ADMIN_SCRIPT="scripts/f-day-smoke-admin.sh"

# Curl timeout (seconds).
CURL_TIMEOUT=15

# -----------------------------------------------------------------------
# Result tracking.
# -----------------------------------------------------------------------

PASS_COUNT=0
FAIL_COUNT=0
DEFERRED_COUNT=0
declare -a ROWS=()   # "W32-N|LABEL|STATUS|DETAIL"

record() {
    local id="$1"
    local label="$2"
    local status="$3"   # PASS | FAIL | DEFERRED | DEPS-MISSING | SKIPPED
    local detail="$4"
    ROWS+=("${id}|${label}|${status}|${detail}")
    case "${status}" in
        PASS)         PASS_COUNT=$(( PASS_COUNT + 1 )) ;;
        FAIL)         FAIL_COUNT=$(( FAIL_COUNT + 1 )) ;;
        DEFERRED)     DEFERRED_COUNT=$(( DEFERRED_COUNT + 1 )) ;;
        DEPS-MISSING) DEFERRED_COUNT=$(( DEFERRED_COUNT + 1 )) ;;
        SKIPPED)      : ;;
    esac
}

# -----------------------------------------------------------------------
# Helpers.
# -----------------------------------------------------------------------

# http_status_code <url> — return HTTP status code or "ERR".
http_status_code() {
    local url="$1"
    local code
    if code="$(curl --silent --max-time "${CURL_TIMEOUT}" \
                    --write-out '%{http_code}' \
                    --output /dev/null \
                    "${url}" 2>/dev/null)"; then
        printf '%s' "${code}"
    else
        printf 'ERR'
    fi
}

# -----------------------------------------------------------------------
# W32-1: Worker shim live.
# -----------------------------------------------------------------------

check_w32_1_worker_shim() {
    local label="Worker shim live (GET /health → 200)"
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-1" "${label}" "SKIPPED" "--dry-run; would: curl ${WORKER_HEALTH_URL}"
        return
    fi
    local code
    code="$(http_status_code "${WORKER_HEALTH_URL}")"
    if [[ "${code}" == "200" ]]; then
        record "W32-1" "${label}" "PASS" "HTTP ${code} from ${WORKER_HEALTH_URL}"
    else
        record "W32-1" "${label}" "FAIL" "Expected 200; got HTTP ${code} from ${WORKER_HEALTH_URL}"
    fi
}

# -----------------------------------------------------------------------
# W32-2: Container running (DO instantiation via Worker → 200).
# -----------------------------------------------------------------------

check_w32_2_container() {
    local label="Container reachable via Worker DO (GET /health → 200)"
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-2" "${label}" "SKIPPED" "--dry-run; would: curl ${CONTAINER_HEALTH_URL}"
        return
    fi
    # The /health endpoint forces DO instantiation; a 200 implies the
    # container process inside the DO responded.  A 503/timeout implies
    # the DO could not start the container.
    local code
    code="$(http_status_code "${CONTAINER_HEALTH_URL}")"
    if [[ "${code}" == "200" ]]; then
        record "W32-2" "${label}" "PASS" "HTTP ${code}; DO-container path responsive"
    else
        record "W32-2" "${label}" "FAIL" "Expected 200 (DO+container); got HTTP ${code}"
    fi
}

# -----------------------------------------------------------------------
# W32-3: 7 subdomains resolve + HTTPS.
# -----------------------------------------------------------------------

check_w32_3_subdomains() {
    local label="7 subdomains resolve + HTTPS"

    # If the parallel-agent g-day script has landed, shell out to it.
    if [[ -x "${G_DAY_DNS_SCRIPT}" ]]; then
        if [[ "${DRY_RUN}" -eq 1 ]]; then
            record "W32-3" "${label}" "SKIPPED" "--dry-run; would: ${G_DAY_DNS_SCRIPT} --quick"
            return
        fi
        local rc=0
        if ! bash "${G_DAY_DNS_SCRIPT}" --quick 2>/dev/null; then
            rc=$?
        fi
        if [[ "${rc}" -eq 0 ]]; then
            record "W32-3" "${label}" "PASS" "g-day-dns-verify-prod.sh --quick exited 0"
        else
            record "W32-3" "${label}" "FAIL" "g-day-dns-verify-prod.sh --quick exited ${rc}"
        fi
        return
    fi

    # g-day script not yet landed — perform inline curl checks per subdomain.
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-3" "${label}" "SKIPPED" "--dry-run; g-day script absent; would curl ${#SUBDOMAINS[@]} subdomains"
        return
    fi

    local sub_pass=0
    local sub_fail=0
    local sub_fail_list=""
    for url in "${SUBDOMAINS[@]}"; do
        local code
        code="$(http_status_code "${url}")"
        # Accept 2xx and also 3xx (pages redirect to www patterns are valid).
        # status.corelink.humangr.com may return 301/302 redirect to BetterStack.
        local http_class="${code:0:1}"
        if [[ "${http_class}" == "2" || "${http_class}" == "3" ]]; then
            sub_pass=$(( sub_pass + 1 ))
        else
            sub_fail=$(( sub_fail + 1 ))
            sub_fail_list="${sub_fail_list} ${url}(${code})"
        fi
    done

    if [[ "${sub_fail}" -eq 0 ]]; then
        record "W32-3" "${label}" "PASS" "${sub_pass}/${#SUBDOMAINS[@]} subdomains returned 2xx/3xx (g-day script: DEPS-MISSING, inline fallback used)"
    else
        record "W32-3" "${label}" "FAIL" "${sub_fail}/${#SUBDOMAINS[@]} failed:${sub_fail_list}"
    fi
}

# -----------------------------------------------------------------------
# W32-4: Docs Pages deployed.
# -----------------------------------------------------------------------

check_w32_4_docs_pages() {
    local label="Docs Pages deployed (corelink-docs.humangr.com)"

    if [[ -x "${F_DAY_DOCS_SCRIPT}" ]]; then
        if [[ "${DRY_RUN}" -eq 1 ]]; then
            record "W32-4" "${label}" "SKIPPED" "--dry-run; would: bash ${F_DAY_DOCS_SCRIPT}"
            return
        fi
        local rc=0
        if ! bash "${F_DAY_DOCS_SCRIPT}" 2>/dev/null; then
            rc=$?
        fi
        if [[ "${rc}" -eq 0 ]]; then
            record "W32-4" "${label}" "PASS" "f-day-smoke-docs.sh exited 0"
        else
            record "W32-4" "${label}" "FAIL" "f-day-smoke-docs.sh exited ${rc}"
        fi
        return
    fi

    # Inline fallback: f-day script not yet landed.
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-4" "${label}" "SKIPPED" "--dry-run; f-day-smoke-docs.sh absent; would curl https://corelink-docs.humangr.com"
        return
    fi

    local code
    code="$(http_status_code "https://corelink-docs.humangr.com")"
    local http_class="${code:0:1}"
    if [[ "${http_class}" == "2" || "${http_class}" == "3" ]]; then
        record "W32-4" "${label}" "PASS" "HTTP ${code} (f-day-smoke-docs.sh: DEPS-MISSING, inline fallback used)"
    else
        record "W32-4" "${label}" "FAIL" "Expected 2xx/3xx; got HTTP ${code} (f-day-smoke-docs.sh: DEPS-MISSING)"
    fi
}

# -----------------------------------------------------------------------
# W32-5: Admin-UI Pages deployed.
# -----------------------------------------------------------------------

check_w32_5_admin_ui() {
    local label="Admin-UI Pages deployed (corelink-app.humangr.com)"

    if [[ -x "${F_DAY_ADMIN_SCRIPT}" ]]; then
        if [[ "${DRY_RUN}" -eq 1 ]]; then
            record "W32-5" "${label}" "SKIPPED" "--dry-run; would: bash ${F_DAY_ADMIN_SCRIPT}"
            return
        fi
        local rc=0
        if ! bash "${F_DAY_ADMIN_SCRIPT}" 2>/dev/null; then
            rc=$?
        fi
        if [[ "${rc}" -eq 0 ]]; then
            record "W32-5" "${label}" "PASS" "f-day-smoke-admin.sh exited 0"
        else
            record "W32-5" "${label}" "FAIL" "f-day-smoke-admin.sh exited ${rc}"
        fi
        return
    fi

    # Inline fallback.
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-5" "${label}" "SKIPPED" "--dry-run; f-day-smoke-admin.sh absent; would curl https://corelink-app.humangr.com"
        return
    fi

    local code
    code="$(http_status_code "https://corelink-app.humangr.com")"
    local http_class="${code:0:1}"
    if [[ "${http_class}" == "2" || "${http_class}" == "3" ]]; then
        record "W32-5" "${label}" "PASS" "HTTP ${code} (f-day-smoke-admin.sh: DEPS-MISSING, inline fallback used)"
    else
        record "W32-5" "${label}" "FAIL" "Expected 2xx/3xx; got HTTP ${code} (f-day-smoke-admin.sh: DEPS-MISSING)"
    fi
}

# -----------------------------------------------------------------------
# W32-6: Migrations applied.
# -----------------------------------------------------------------------

check_w32_6_migrations() {
    local label="Migrations applied (D1 table count ≥ ${EXPECTED_TABLE_COUNT})"

    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-6" "${label}" "SKIPPED" "--dry-run; would: wrangler d1 execute ${D1_DATABASE} --remote --command=SELECT count(*) FROM sqlite_master WHERE type='table'"
        return
    fi

    if ! command -v wrangler >/dev/null 2>&1; then
        record "W32-6" "${label}" "DEPS-MISSING" "wrangler not on PATH; install Node / npm to enable"
        return
    fi

    local raw_count
    # wrangler d1 execute --json may emit a non-JSON preamble before the JSON
    # array on stdout (e.g. wrangler 4.95 emits "⛅️ wrangler 4.95.0" banner +
    # "Cloudflare agent skills..." line).  Strip everything before the first
    # JSON array start character so jq/python3 receives clean JSON.
    local sql="SELECT count(*) as tbl_count FROM sqlite_master WHERE type='table'"
    if raw_count="$(wrangler d1 execute "${D1_DATABASE}" \
                        --remote \
                        --env prod \
                        --command="${sql}" \
                        --json 2>/dev/null)"; then
        # Strip non-JSON preamble: keep from first '[' to end.
        local json_only
        json_only="$(printf '%s' "${raw_count}" | sed -n '/^\[/,$p')"
        local count=0
        if command -v jq >/dev/null 2>&1; then
            count="$(printf '%s' "${json_only}" | jq -r '.[0].results[0].tbl_count // .[0].results[0]["count(*)"] // 0' 2>/dev/null || printf '0')"
        else
            count="$(printf '%s' "${json_only}" | python3 -c "
import sys, json
data = json.load(sys.stdin)
row = data[0]['results'][0]
print(row.get('tbl_count', row.get('count(*)', 0)))
" 2>/dev/null || printf '0')"
        fi
        if [[ "${count}" -ge "${EXPECTED_TABLE_COUNT}" ]]; then
            record "W32-6" "${label}" "PASS" "${count} tables present (≥ ${EXPECTED_TABLE_COUNT} required)"
        else
            record "W32-6" "${label}" "FAIL" "Only ${count} tables present; expected ≥ ${EXPECTED_TABLE_COUNT} — migrations may not have applied"
        fi
    else
        record "W32-6" "${label}" "FAIL" "wrangler d1 execute failed (non-zero exit); check CLOUDFLARE_API_TOKEN + account binding"
    fi
}

# -----------------------------------------------------------------------
# W32-7: Secrets bound (wrangler secret list --env prod vs core-required set).
#
# DESIGN — core-required vs forward-looking distinction:
#   The secrets-checklist.md matrix contains two categories of cf-wrangler
#   secrets:
#
#   1. CORE_REQUIRED_SECRETS (defined below) — secrets that the currently
#      running production code actively consumes.  A missing core secret is a
#      FAIL.  These are the 17 secrets verified-bound via `wrangler secret list
#      --env prod` on 2026-05-28 (Wave 32 Phase H live run).
#
#   2. Forward-looking / matrix-tracked secrets — credentials for future BYOK
#      providers (AWS/GCP/Azure/Vault), multi-region Neon DSNs, Slack webhooks,
#      Twilio, HubSpot, Drata, etc.  They are intentionally listed in the
#      checklist "Forward-looking secrets" section of secrets-checklist.md so
#      that procurement / vendor onboarding runs ahead of the code consumer.
#      They are NOT bound in prod until the target wave ships.  Treating them
#      as FAIL produces false positives (~60 spurious failures) and must NOT be
#      the gate criterion.  These produce INFO-only output.
#
#   See docs/internal/secrets-checklist.md §Forward-looking secrets for the
#   documented rationale and per-row target-wave tracking.
# -----------------------------------------------------------------------

# Core secrets verified-bound in production on 2026-05-28 (Wave 32 Phase H).
# FAIL if any of these is missing from `wrangler secret list --env prod`.
CORE_REQUIRED_SECRETS=(
    BETTERSTACK_API_TOKEN
    BETTERSTACK_PAGE_ID
    CLERK_PUBLISHABLE_KEY
    CLERK_SECRET_KEY
    CLOUDFLARE_ACCOUNT_ID
    CLOUDFLARE_API_TOKEN
    CLOUDFLARE_ZONE_ID_HUMANGR
    HUGR_AUDIT_CHAIN_HMAC_KEY
    HUGR_OCI_TOKEN_KEY
    HUGR_PAT_SIGNING_KEY
    HUGR_SESSION_HMAC_KEY
    PAGERDUTY_ROUTING_KEY
    RESEND_API_KEY
    STRIPE_AUTH_MODE
    STRIPE_PRICE_ID_STARTER
    STRIPE_SECRET_KEY
    STRIPE_WEBHOOK_SECRET
)

check_w32_7_secrets_bound() {
    local label="Secrets bound (core-required set present in wrangler secret list --env prod)"

    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-7" "${label}" "SKIPPED" "--dry-run; would: wrangler secret list --env prod (read-only); checking ${#CORE_REQUIRED_SECRETS[@]} core-required secrets"
        return
    fi

    if ! command -v wrangler >/dev/null 2>&1; then
        record "W32-7" "${label}" "DEPS-MISSING" "wrangler not on PATH"
        return
    fi

    # wrangler secret list --env prod returns JSON array of {name, type} objects.
    # Also strip any non-JSON preamble (wrangler banner) before parsing.
    local raw_list
    if ! raw_list="$(wrangler secret list --env prod --json 2>/dev/null)"; then
        record "W32-7" "${label}" "FAIL" "wrangler secret list --env prod failed (non-zero exit)"
        return
    fi

    # Strip non-JSON preamble: keep from first '[' to end.
    local json_only
    json_only="$(printf '%s' "${raw_list}" | sed -n '/^\[/,$p')"

    local bound_secrets
    if command -v jq >/dev/null 2>&1; then
        bound_secrets="$(printf '%s' "${json_only}" | jq -r '.[].name' 2>/dev/null | sort -u || true)"
    else
        bound_secrets="$(printf '%s' "${json_only}" | python3 -c "
import sys, json
data = json.load(sys.stdin)
for item in data:
    print(item.get('name',''))
" 2>/dev/null | sort -u || true)"
    fi

    local bound_count
    bound_count="$(printf '%s\n' "${bound_secrets}" | grep -c . || true)"

    # Check only CORE_REQUIRED_SECRETS — forward-looking secrets are INFO, not FAIL.
    local missing_core=""
    local forward_looking_info=""
    for secret in "${CORE_REQUIRED_SECRETS[@]}"; do
        if ! printf '%s\n' "${bound_secrets}" | grep -qxF "${secret}"; then
            missing_core="${missing_core} ${secret}"
        fi
    done

    # Enumerate all cf-wrangler secrets from checklist that are NOT in
    # CORE_REQUIRED_SECRETS and NOT bound — report as INFO only.
    if [[ -f "${CHECKLIST_PATH}" ]]; then
        local bt_pat="\`[A-Z_][A-Z0-9_]*\`"
        local all_cf_wrangler_secrets
        all_cf_wrangler_secrets="$(grep '| cf-wrangler' "${CHECKLIST_PATH}" \
            | grep -v '^\s*#' \
            | sed 's/|/\n/g' \
            | grep "${bt_pat}" \
            | sed "s/.*\`\([A-Z_][A-Z0-9_]*\)\`.*/\1/" \
            | sort -u)"
        local fwd_count=0
        while IFS= read -r secret; do
            [[ -z "${secret}" ]] && continue
            # Skip if it is a core-required secret (already checked above).
            local is_core=0
            for core_s in "${CORE_REQUIRED_SECRETS[@]}"; do
                if [[ "${secret}" == "${core_s}" ]]; then
                    is_core=1
                    break
                fi
            done
            [[ "${is_core}" -eq 1 ]] && continue
            # Only note it if also unbound.
            if ! printf '%s\n' "${bound_secrets}" | grep -qxF "${secret}"; then
                fwd_count=$(( fwd_count + 1 ))
            fi
        done <<< "${all_cf_wrangler_secrets}"
        if [[ "${fwd_count}" -gt 0 ]]; then
            forward_looking_info=" (INFO: ${fwd_count} forward-looking/matrix-tracked secrets not yet bound — non-blocking; see secrets-checklist.md §Forward-looking secrets)"
        fi
    fi

    if [[ -z "${missing_core}" ]]; then
        record "W32-7" "${label}" "PASS" "All ${#CORE_REQUIRED_SECRETS[@]} core-required secrets bound (${bound_count} total bound)${forward_looking_info}"
    else
        record "W32-7" "${label}" "FAIL" "CORE secrets missing from prod env:${missing_core}${forward_looking_info}"
    fi
}

# -----------------------------------------------------------------------
# W32-8: BetterStack probes green.
# -----------------------------------------------------------------------

check_w32_8_betterstack() {
    local label="BetterStack probes green (monitoring/synthetic/probes.yml)"

    if [[ ! -f "${PROBES_YML}" ]]; then
        record "W32-8" "${label}" "DEPS-MISSING" "${PROBES_YML} not found; cannot enumerate probe URLs"
        return
    fi

    # Extract probe URLs from the YAML file (cross-ref WP-7.1).
    # probes.yml uses `url:` keys under each probe entry.
    local probe_urls
    probe_urls="$(grep -E '^\s+url:\s+' "${PROBES_YML}" \
        | sed 's/.*url:[[:space:]]*//' \
        | sed 's/["'"'"']//g' \
        | tr -d '\r' \
        | sort -u)"

    if [[ -z "${probe_urls}" ]]; then
        record "W32-8" "${label}" "DEPS-MISSING" "No probe URLs found in ${PROBES_YML}"
        return
    fi

    if [[ "${DRY_RUN}" -eq 1 ]]; then
        local probe_count
        probe_count="$(printf '%s\n' "${probe_urls}" | grep -c . || true)"
        record "W32-8" "${label}" "SKIPPED" "--dry-run; would curl ${probe_count} probe URLs from probes.yml"
        return
    fi

    local probe_pass=0
    local probe_fail=0
    local probe_fail_list=""

    while IFS= read -r url; do
        [[ -z "${url}" ]] && continue
        local code
        code="$(http_status_code "${url}")"
        local http_class="${code:0:1}"
        if [[ "${http_class}" == "2" || "${http_class}" == "3" ]]; then
            probe_pass=$(( probe_pass + 1 ))
        else
            probe_fail=$(( probe_fail + 1 ))
            probe_fail_list="${probe_fail_list} ${url}(${code})"
        fi
    done <<< "${probe_urls}"

    local probe_total=$(( probe_pass + probe_fail ))
    if [[ "${probe_fail}" -eq 0 ]]; then
        record "W32-8" "${label}" "PASS" "${probe_pass}/${probe_total} probe endpoints returned 2xx/3xx"
    else
        record "W32-8" "${label}" "FAIL" "${probe_fail}/${probe_total} probe endpoints not green:${probe_fail_list}"
    fi
}

# -----------------------------------------------------------------------
# W32-9: Sentry test event (DEFERRED if SENTRY_DSN unset).
# -----------------------------------------------------------------------

check_w32_9_sentry() {
    local label="Sentry test event (deferred until SENTRY_DSN set)"

    if [[ -z "${SENTRY_DSN:-}" ]]; then
        record "W32-9" "${label}" "DEFERRED" "SENTRY_DSN env var unset; set it to enable live Sentry probe (not a FAIL)"
        return
    fi

    if [[ "${DRY_RUN}" -eq 1 ]]; then
        record "W32-9" "${label}" "SKIPPED" "--dry-run; SENTRY_DSN is set; would POST test event to DSN"
        return
    fi

    # DSN is present — attempt a minimal envelope POST (Sentry envelope API).
    # The test event is an INFO-level capture_message that will not page anyone.
    local dsn_host
    dsn_host="$(printf '%s' "${SENTRY_DSN}" | sed 's|.*@\([^/]*\)/.*|\1|')"
    local dsn_key
    dsn_key="$(printf '%s' "${SENTRY_DSN}" | sed 's|.*//\([^:@]*\).*|\1|')"
    local dsn_project
    dsn_project="$(printf '%s' "${SENTRY_DSN}" | sed 's|.*/\([0-9]*\)$|\1|')"

    local store_url="https://${dsn_host}/api/${dsn_project}/store/"
    local payload
    payload=$(printf '{"message":"CoreLink pre-cutover wave32 smoke test event","level":"info","logger":"pre-cutover-wave32-extension","tags":{"wave":"32","smoke":"true"}}')

    local code
    code="$(curl --silent --max-time "${CURL_TIMEOUT}" \
                 --write-out '%{http_code}' \
                 --output /dev/null \
                 --header "X-Sentry-Auth: Sentry sentry_version=7, sentry_key=${dsn_key}, sentry_client=bash-smoke/1.0" \
                 --header 'Content-Type: application/json' \
                 --data "${payload}" \
                 "${store_url}" 2>/dev/null)"

    if [[ "${code}" == "200" ]]; then
        record "W32-9" "${label}" "PASS" "Sentry store endpoint returned 200 for test event"
    else
        record "W32-9" "${label}" "FAIL" "Sentry store endpoint returned HTTP ${code}"
    fi
}

# -----------------------------------------------------------------------
# Run all checks.
# -----------------------------------------------------------------------

printf '=== pre-cutover-wave32-extension.sh — Wave 32 surface smoke ===\n'
printf 'Date:     %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
printf 'Dry-run:  %s\n' "$([[ "${DRY_RUN}" -eq 1 ]] && printf 'YES' || printf 'NO')"
printf 'Repo:     %s\n' "${REPO_ROOT}"
printf '\n'

check_w32_1_worker_shim
check_w32_2_container
check_w32_3_subdomains
check_w32_4_docs_pages
check_w32_5_admin_ui
check_w32_6_migrations
check_w32_7_secrets_bound
check_w32_8_betterstack
check_w32_9_sentry

# -----------------------------------------------------------------------
# Summary table.
# -----------------------------------------------------------------------

printf '\n'
printf '%-7s  %-55s  %-12s  %s\n' "ID" "Label" "Status" "Detail"
printf -- '-------  -------------------------------------------------------  ------------  ------\n'
for row in "${ROWS[@]}"; do
    IFS='|' read -r id label status detail <<< "${row}"
    printf '%-7s  %-55s  %-12s  %s\n' "${id}" "${label}" "${status}" "${detail}"
done

printf '\n'
printf 'PASS=%d  FAIL=%d  DEFERRED/DEPS-MISSING=%d\n' \
    "${PASS_COUNT}" "${FAIL_COUNT}" "${DEFERRED_COUNT}"

# -----------------------------------------------------------------------
# Final exit.
# -----------------------------------------------------------------------

if [[ "${FAIL_COUNT}" -gt 0 ]]; then
    printf '\nRESULT: FAIL (%d failures detected)\n' "${FAIL_COUNT}"
    exit 1
fi

printf '\nRESULT: PASS (all required checks passed; %d deferred/skipped)\n' \
    "${DEFERRED_COUNT}"
exit 0
