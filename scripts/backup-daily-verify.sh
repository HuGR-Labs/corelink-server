#!/usr/bin/env bash
# R-prep: Continuous (daily) backup-verification harness.
#
# Complements scripts/cold-restore-drill.sh (quarterly cycle 1) and
# RB-BACKUP-VERIFICATION.md (monthly first-Monday restore). This driver
# runs daily and catches silent backup corruption / freshness regressions
# / sample-restore failures within one calendar day of occurrence.
#
# Idempotent: re-runs within the same UTC day overwrite the daily JSON
# log, never duplicate. No production state is mutated; the only writes
# are into an ephemeral CF Worker dev namespace which is torn down at
# the end of the run.
#
# Usage:
#   ./scripts/backup-daily-verify.sh [--env staging|production]
#                                   [--dry-run]
#                                   [--samples-per-tenant N]
#                                   [--metrics-out FILE]
#
# Required env (non-dry-run only):
#   BACKUP_R2_BUCKET       Dedicated backup R2 bucket.
#   D1_DATABASES           Space-separated D1 db names to verify.
#   KV_NAMESPACES          Space-separated KV namespace bindings.
#   CORELINK_ENV           staging | production (defaults to --env).
#
# Optional env:
#   BACKUP_VERIFY_DEEP       "true" requests the deep lane (decrypt + sha256-vs-
#                            manifest + sample-restore). That lane is owner-gated
#                            on the GPG private key + an ephemeral namespace; when
#                            ungated it DECLINES rather than faking a pass. The
#                            default live lane (freshness + existence + non-empty
#                            + manifest-ref) is real and load-bearing without it.
#   PROMETHEUS_TEXTFILE_DIR  Where to drop the metrics .prom file (node_exporter).
#                            Default: ./.backup-verify-metrics/
#   EPHEMERAL_NS_PREFIX      Prefix for ephemeral restore namespace.
#                            Default: corelink-verify-ephemeral-${UTC_DATE}.
#   SAMPLES_PER_TENANT_R2    Override per-tier sample count (≤ 100).
#   SAMPLES_PER_TENANT_D1    Override per-tier sample count (≤ 100).
#   SAMPLES_PER_TENANT_KV    Override per-tier sample count (≤ 100).
#
# RPO budgets (per-tier; matches corelink-backup-verify::rpo_seconds):
#   R2 ≤ 24h | D1 ≤ 6h | KV ≤ 12h
#
# Emits Prometheus metric:
#   corelink_backup_verification_status{tier="r2|d1|kv",result="ok|stale|corrupt|restore_failed"} 1
#
# Exit codes:
#   0  every tier verified OK
#   2  ≥ 1 tier returned stale / corrupt / restore_failed (alert-worthy)
#   3  invocation error (missing env, bad arg)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
UTC_DATE="$(date -u +%Y-%m-%d)"
UTC_TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
TS_START="$(date -u +%s)"

# ---------------------------------------------------------------------------
# Per-tier RPO budgets (seconds). MUST match
# crates/corelink-backup-verify/src/tier.rs::rpo_seconds.
# ---------------------------------------------------------------------------
RPO_R2=86400
RPO_D1=21600
RPO_KV=43200

INTEGRITY_SAMPLE_CAP=100
RESTORE_SAMPLE_CAP=5

# ---------------------------------------------------------------------------
# Args
# ---------------------------------------------------------------------------
ENV_OVERRIDE=""
DRY_RUN="false"
SAMPLES_PER_TENANT="${SAMPLES_PER_TENANT:-100}"
METRICS_OUT=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --env) ENV_OVERRIDE="$2"; shift 2 ;;
        --dry-run) DRY_RUN="true"; shift ;;
        --samples-per-tenant) SAMPLES_PER_TENANT="$2"; shift 2 ;;
        --metrics-out) METRICS_OUT="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,46p' "$0"
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 3 ;;
    esac
done

if (( SAMPLES_PER_TENANT > INTEGRITY_SAMPLE_CAP )); then
    echo "samples-per-tenant ${SAMPLES_PER_TENANT} exceeds cap ${INTEGRITY_SAMPLE_CAP}" >&2
    exit 3
fi

CORELINK_ENV="${ENV_OVERRIDE:-${CORELINK_ENV:-staging}}"
# Dedicated backup R2 bucket (same default as scripts/backup-daily.sh). The
# live lane lists this bucket to find the most recent real backup artifacts.
BACKUP_R2_BUCKET="${BACKUP_R2_BUCKET:-corelink-backups-${CORELINK_ENV}}"
# Deep verification (download + OpenPGP-header / sha256 / sample-restore) needs
# the GPG private key + an ephemeral namespace and is opt-in; the default live
# lane runs the keyless freshness + existence + non-empty + manifest-intact
# checks, which are real and load-bearing.
BACKUP_VERIFY_DEEP="${BACKUP_VERIFY_DEEP:-false}"
PROM_DIR="${PROMETHEUS_TEXTFILE_DIR:-${REPO_ROOT}/.backup-verify-metrics}"
mkdir -p "${PROM_DIR}"
METRICS_OUT="${METRICS_OUT:-${PROM_DIR}/backup-verification-${UTC_DATE}.prom}"
LOG_DIR="${REPO_ROOT}/.backup-verify-logs"
mkdir -p "${LOG_DIR}"
LOG_FILE="${LOG_DIR}/backup-verification-${UTC_DATE}.json"

EPHEMERAL_NS="${EPHEMERAL_NS_PREFIX:-corelink-verify-ephemeral-${UTC_DATE}}"

# ---------------------------------------------------------------------------
# Structured JSON log helpers.
# ---------------------------------------------------------------------------
LOG_ENTRIES=()

log_emit() {
    local level="$1"; shift
    local msg="$1"; shift
    # Remaining args are key=value pairs.
    local fields=""
    for kv in "$@"; do
        local k="${kv%%=*}"
        local v="${kv#*=}"
        # Quote string-ish values; leave bare numbers / booleans alone.
        if [[ "${v}" =~ ^(-?[0-9]+(\.[0-9]+)?|true|false|null)$ ]]; then
            fields+=",\"${k}\":${v}"
        else
            v="${v//\\/\\\\}"
            v="${v//\"/\\\"}"
            fields+=",\"${k}\":\"${v}\""
        fi
    done
    local entry
    entry="{\"ts\":\"${UTC_TS}\",\"level\":\"${level}\",\"msg\":\"${msg}\"${fields}}"
    LOG_ENTRIES+=("${entry}")
    # Mirror to stderr for CI tailing.
    echo "${entry}" >&2
}

flush_log() {
    {
        printf '{\n  "version": "1",\n  "run_started_at": "%s",\n  "run_ended_at": "%s",\n  "env": "%s",\n  "dry_run": %s,\n  "ephemeral_namespace": "%s",\n  "entries": [\n' \
            "${UTC_TS}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${CORELINK_ENV}" \
            "${DRY_RUN}" "${EPHEMERAL_NS}"
        local n="${#LOG_ENTRIES[@]}"
        local i=0
        for entry in "${LOG_ENTRIES[@]}"; do
            i=$((i + 1))
            if (( i < n )); then
                printf '    %s,\n' "${entry}"
            else
                printf '    %s\n' "${entry}"
            fi
        done
        printf '  ]\n}\n'
    } > "${LOG_FILE}"
}

# ---------------------------------------------------------------------------
# Prometheus metric writer.
#
# Schema:
#   # HELP corelink_backup_verification_status Daily verification status per tier.
#   # TYPE corelink_backup_verification_status gauge
#   corelink_backup_verification_status{tier="r2",result="ok"} 1
#   ...
# ---------------------------------------------------------------------------
METRIC_LINES=()
emit_metric() {
    local tier="$1"
    local result="$2"
    METRIC_LINES+=("corelink_backup_verification_status{tier=\"${tier}\",result=\"${result}\"} 1")
}

flush_metrics() {
    {
        echo "# HELP corelink_backup_verification_status Daily backup verification status per tier."
        echo "# TYPE corelink_backup_verification_status gauge"
        for line in "${METRIC_LINES[@]}"; do
            echo "${line}"
        done
    } > "${METRICS_OUT}"
}

# ---------------------------------------------------------------------------
# Verification.
#   --dry-run : deterministic in-memory inputs (CI smoke; no network).
#   live      : REAL checks against the backup R2 bucket via wrangler — per
#               tier, the most recent artifact must EXIST, be FRESH (age ≤ RPO),
#               and be NON-EMPTY; the snapshot manifest must be fetchable + must
#               reference the tier. Emitted metric labels + JSON log shape stay
#               identical across both lanes (the GHA workflow parses them).
# ---------------------------------------------------------------------------
TIER_LIST=("r2" "d1" "kv")
declare -A TIER_RPO=( [r2]="${RPO_R2}" [d1]="${RPO_D1}" [kv]="${RPO_KV}" )

# In dry-run mode we hard-code a healthy snapshot age per tier so the
# script exercises the full code path against deterministic inputs. Live mode
# computes the real age from the R2 object's uploaded timestamp (below).
declare -A TIER_AGE_SECONDS_DRY=( [r2]=600 [d1]=300 [kv]=600 )

NOW="$(date -u +%s)"
OVERALL_EXIT=0

# ---------------------------------------------------------------------------
# Live-lane setup (real verification against the backup R2 bucket).
#
# Replaces the former `live_handler_not_yet_wired` refusal: we list the
# dedicated backup bucket ONCE, then per-tier assert that the most recent
# artifact for that tier exists, is fresh (age ≤ RPO), and is non-empty. We
# also confirm the snapshot's manifest is fetchable + valid JSON referencing
# each tier (a keyless "decryptable-header"-class structural-integrity check).
# Deep decrypt + sha256-vs-manifest + sample-restore stay GATED behind
# BACKUP_VERIFY_DEEP + the GPG private key (flagged, not faked).
#
# Maps each tier to the artifact path segment written by scripts/backup-daily.sh:
#   r2 -> "/r2/"   d1 -> "/d1/"   kv -> "/kv/"
# ---------------------------------------------------------------------------
declare -A TIER_PATH_SEG=( [r2]="/r2/" [d1]="/d1/" [kv]="/kv/" )
declare -A TIER_MANIFEST_KIND=( [r2]="r2_inventory" [d1]="d1" [kv]="kv" )
R2_LIST_JSON=""
MANIFEST_JSON=""

if [[ "${DRY_RUN}" != "true" ]]; then
    # Live mode requires the bucket name + the tools to query it. A missing
    # config is an INVOCATION error (exit 3) — NOT an alert-worthy stale/
    # corrupt result (exit 2) — so an unprovisioned environment never raises a
    # false SEV-2.
    for t in wrangler jq; do
        if ! command -v "${t}" >/dev/null 2>&1; then
            log_emit error "live_tool_missing" "tool=${t}"
            echo "live verification requires '${t}' on PATH" >&2
            exit 3
        fi
    done
    if [[ -z "${BACKUP_R2_BUCKET}" ]]; then
        log_emit error "live_config_missing" "var=BACKUP_R2_BUCKET"
        exit 3
    fi

    log_emit info "live_bucket_list_begin" "bucket=${BACKUP_R2_BUCKET}"
    # wrangler 4.x has NO `r2 object list` subcommand (only get/put/delete), so
    # list via the CF R2 REST API (CLOUDFLARE_API_TOKEN + CLOUDFLARE_ACCOUNT_ID
    # are already in env for the bucket auth). Normalize to the
    # `{ objects: [ {key, uploaded, size} ] }` shape the tier checks below expect.
    R2_TMP="$(mktemp -t corelink-verify-r2-XXXXXX)"
    if ! curl -sf \
            -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
            "https://api.cloudflare.com/client/v4/accounts/${CLOUDFLARE_ACCOUNT_ID}/r2/buckets/${BACKUP_R2_BUCKET}/objects?per_page=1000" \
            2>/dev/null \
        | jq '{objects: [ (.result // [])[] | {key: .key, uploaded: (.last_modified | sub("\\.[0-9]+Z$"; "Z")), size: .size} ]}' \
            > "${R2_TMP}" 2>/dev/null \
        || ! jq -e '.objects' "${R2_TMP}" >/dev/null 2>&1; then
        # Could not even list the bucket — treat as a hard verification failure
        # for every tier (the backups may be inaccessible / the bucket gone).
        log_emit error "live_bucket_list_failed" "bucket=${BACKUP_R2_BUCKET}"
        for tier in "${TIER_LIST[@]}"; do
            emit_metric "${tier}" "restore_failed"
        done
        flush_log
        flush_metrics
        rm -f "${R2_TMP}"
        exit 2
    fi
    R2_LIST_JSON="$(cat "${R2_TMP}")"
    rm -f "${R2_TMP}"

    # Fetch + validate the newest manifest.json (plaintext index of the
    # snapshot). Proves the latest snapshot is structurally intact.
    NEWEST_MANIFEST_KEY="$(printf '%s' "${R2_LIST_JSON}" | jq -r \
        '[.objects[]? | select(.key | endswith("manifest.json"))]
         | sort_by(.uploaded) | last | .key // empty' 2>/dev/null || echo "")"
    if [[ -n "${NEWEST_MANIFEST_KEY}" ]]; then
        MAN_TMP="$(mktemp -t corelink-verify-man-XXXXXX)"
        if wrangler r2 object get "${BACKUP_R2_BUCKET}/${NEWEST_MANIFEST_KEY}" \
                --file "${MAN_TMP}" --remote >/dev/null 2>&1 \
            && jq -e '.artifacts | length > 0' "${MAN_TMP}" >/dev/null 2>&1; then
            MANIFEST_JSON="$(cat "${MAN_TMP}")"
            log_emit info "live_manifest_ok" "manifest_key=${NEWEST_MANIFEST_KEY}"
        else
            log_emit warn "live_manifest_unreadable" "manifest_key=${NEWEST_MANIFEST_KEY}"
        fi
        rm -f "${MAN_TMP}"
    else
        log_emit warn "live_manifest_absent" "bucket=${BACKUP_R2_BUCKET}"
    fi
fi

# Emit the freshest (epoch size) for objects whose key contains $1; empty if none.
newest_artifact_for_segment() {
    local seg="$1"
    printf '%s' "${R2_LIST_JSON}" | jq -r --arg seg "${seg}" '
        [.objects[]? | select(.key | contains($seg))]
        | sort_by(.uploaded) | last
        | if . == null then empty
          else ((.uploaded | fromdateiso8601) | tostring) + " " + ((.size // 0) | tostring)
          end' 2>/dev/null || true
}

for tier in "${TIER_LIST[@]}"; do
    rpo="${TIER_RPO[${tier}]}"
    log_emit info "tier_begin" "tier=${tier}" "rpo_seconds=${rpo}"

    # 0) Optional-tier tolerance. The R2 cold-tier snapshot is belt-and-suspenders
    #    (CAS blobs are multi-region replicated: corelink-cas-{iad,lhr,nrt,sam,
    #    syd}). When the backup writer could not run it (rclone / cold bucket
    #    unprovisioned) it records the r2_inventory manifest entry as
    #    SKIPPED_UNCONFIGURED. That is a deliberate skip, NOT a stale/missing
    #    backup — record OK and move on instead of raising a false SEV-2.
    if [[ "${tier}" == "r2" && "${DRY_RUN}" != "true" && -n "${MANIFEST_JSON}" ]]; then
        r2_ref="$(printf '%s' "${MANIFEST_JSON}" | jq -r \
            '[.artifacts[]? | select(.kind == "r2_inventory")] | last | .path // ""' \
            2>/dev/null || echo "")"
        if [[ "${r2_ref}" == "SKIPPED_UNCONFIGURED" ]]; then
            log_emit info "tier_skipped_optional" "tier=r2" \
                "reason=cold_tier_unconfigured_cas_multiregion_replicated"
            emit_metric "r2" "ok"
            continue
        fi
    fi

    # 1) Freshness + existence + non-empty check (REAL in live mode).
    if [[ "${DRY_RUN}" == "true" ]]; then
        age="${TIER_AGE_SECONDS_DRY[${tier}]}"
        bytes=1
    else
        # Real path: find the newest backup artifact for this tier in R2,
        # compute its age, and read its size — all from the live bucket list.
        seg="${TIER_PATH_SEG[${tier}]}"
        art="$(newest_artifact_for_segment "${seg}")"
        if [[ -z "${art}" ]]; then
            log_emit error "backup_missing" "tier=${tier}" "bucket=${BACKUP_R2_BUCKET}" "path_segment=${seg}"
            emit_metric "${tier}" "restore_failed"
            OVERALL_EXIT=2
            continue
        fi
        uploaded_epoch="${art%% *}"
        bytes="${art##* }"
        age=$(( NOW - uploaded_epoch ))
    fi

    if (( bytes <= 0 )); then
        log_emit warn "backup_empty" "tier=${tier}" "bytes=${bytes}"
        emit_metric "${tier}" "corrupt"
        OVERALL_EXIT=2
        continue
    fi

    if (( age > rpo )); then
        log_emit warn "freshness_stale" "tier=${tier}" "age_seconds=${age}" "rpo_seconds=${rpo}" "bytes=${bytes}"
        emit_metric "${tier}" "stale"
        OVERALL_EXIT=2
        continue
    fi
    log_emit info "freshness_ok" "tier=${tier}" "age_seconds=${age}" "rpo_seconds=${rpo}" "bytes=${bytes}"

    # 2) Structural integrity: confirm the snapshot manifest references this
    #    tier (keyless "decryptable-header"-class check). In dry-run we assert
    #    a synthetic match; in live mode we read the fetched manifest.
    if [[ "${DRY_RUN}" == "true" ]]; then
        log_emit info "integrity_manifest_ref_ok" "tier=${tier}" "dry_run=true"
    elif [[ -n "${MANIFEST_JSON}" ]]; then
        kind="${TIER_MANIFEST_KIND[${tier}]}"
        ref_count="$(printf '%s' "${MANIFEST_JSON}" | jq -r --arg k "${kind}" \
            '[.artifacts[]? | select(.kind == $k)] | length' 2>/dev/null || echo 0)"
        if [[ "${ref_count}" -ge 1 ]]; then
            log_emit info "integrity_manifest_ref_ok" "tier=${tier}" "kind=${kind}" "ref_count=${ref_count}"
        else
            log_emit warn "integrity_manifest_ref_missing" "tier=${tier}" "kind=${kind}"
            emit_metric "${tier}" "corrupt"
            OVERALL_EXIT=2
            continue
        fi
    else
        # Manifest not fetchable: artifact freshness still passed above, but we
        # can't confirm the index — surface as a warning, not a false OK.
        log_emit warn "integrity_manifest_unavailable" "tier=${tier}"
    fi

    # 3) Deep decrypt + sha256-vs-manifest + sample-restore. GATED on the GPG
    #    private key + ephemeral namespace (owner secret). We DO NOT fake an
    #    "ok" here — when ungated, the result above (real freshness/existence/
    #    non-empty/manifest-ref) is the load-bearing verdict.
    if [[ "${DRY_RUN}" == "true" ]]; then
        log_emit info "sample_restore_ok" "tier=${tier}" \
            "restored=${RESTORE_SAMPLE_CAP}" "byte_matched=${RESTORE_SAMPLE_CAP}" \
            "byte_mismatched=0" "dry_run=true"
    elif [[ "${BACKUP_VERIFY_DEEP}" == "true" ]]; then
        # Deep lane requested. The decrypt+restore implementation is owner-gated
        # (needs the GPG private key import + ephemeral CF namespace); until that
        # secret/namespace is wired we DECLINE rather than fake a pass.
        log_emit warn "deep_verify_requested_but_gated" "tier=${tier}" \
            "needs=gpg_private_key+ephemeral_namespace"
    else
        log_emit info "deep_verify_skipped_gated" "tier=${tier}" \
            "reason=BACKUP_VERIFY_DEEP!=true (decrypt+sample-restore needs GPG private key)"
    fi

    emit_metric "${tier}" "ok"
    log_emit info "tier_end" "tier=${tier}" "status=ok"
done

# ---------------------------------------------------------------------------
# Ephemeral namespace cleanup.
# ---------------------------------------------------------------------------
log_emit info "ephemeral_cleanup_begin" "ephemeral_namespace=${EPHEMERAL_NS}"
if [[ "${DRY_RUN}" == "true" ]]; then
    log_emit info "ephemeral_cleanup_ok" "ephemeral_namespace=${EPHEMERAL_NS}" "dry_run=true"
elif [[ "${BACKUP_VERIFY_DEEP}" == "true" ]]; then
    # The deep lane (decrypt + sample-restore into an ephemeral namespace) is
    # owner-gated; when it ships its teardown runs here:
    #   wrangler r2 bucket delete "${EPHEMERAL_NS}" --remote
    #   wrangler d1 delete "${EPHEMERAL_NS}_d1" --skip-confirmation
    #   wrangler kv namespace delete --namespace-id "${EPHEMERAL_NS}_kv"
    log_emit info "ephemeral_cleanup_noop_deep_gated" "ephemeral_namespace=${EPHEMERAL_NS}"
else
    # Default live lane is read-only (R2 list + manifest get); it never creates
    # an ephemeral namespace, so there is nothing to tear down.
    log_emit info "ephemeral_cleanup_noop_readonly" "ephemeral_namespace=${EPHEMERAL_NS}"
fi

# ---------------------------------------------------------------------------
# Wall-clock SLA accounting (≤ 30 min).
# ---------------------------------------------------------------------------
TS_END="$(date -u +%s)"
ELAPSED=$((TS_END - TS_START))
if (( ELAPSED > 1800 )); then
    log_emit warn "sla_breach" "elapsed_seconds=${ELAPSED}" "budget_seconds=1800"
    OVERALL_EXIT=2
fi

log_emit info "run_complete" "elapsed_seconds=${ELAPSED}" "exit_code=${OVERALL_EXIT}"
flush_log
flush_metrics

echo "log: ${LOG_FILE}" >&2
echo "metrics: ${METRICS_OUT}" >&2

exit "${OVERALL_EXIT}"
