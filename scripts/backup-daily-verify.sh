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
# Verification — pure-bash simulation that mirrors the trait contract of
# `corelink-backup-verify::BackupVerifier`. The real CF Worker handler
# will substitute wrangler API calls for the simulated stages below; the
# semantics + emitted metric labels + JSON log shape MUST stay identical
# (the GHA workflow alerts on parsed metric / log shape).
# ---------------------------------------------------------------------------
TIER_LIST=("r2" "d1" "kv")
declare -A TIER_RPO=( [r2]="${RPO_R2}" [d1]="${RPO_D1}" [kv]="${RPO_KV}" )

# In dry-run mode we hard-code a healthy snapshot age per tier so the
# script exercises the full code path against deterministic, in-memory
# inputs. The real handler replaces this with R2 list-objects + D1 query
# + KV list bindings calls.
declare -A TIER_AGE_SECONDS_DRY=( [r2]=600 [d1]=300 [kv]=600 )

NOW="$(date -u +%s)"
OVERALL_EXIT=0

for tier in "${TIER_LIST[@]}"; do
    rpo="${TIER_RPO[${tier}]}"
    log_emit info "tier_begin" "tier=${tier}" "rpo_seconds=${rpo}"

    # 1) Freshness check.
    if [[ "${DRY_RUN}" == "true" ]]; then
        age="${TIER_AGE_SECONDS_DRY[${tier}]}"
    else
        # Real path: list latest snapshot for this tier; compute age.
        # Placeholder — real implementation lives in the deferred CF Worker
        # handler. Until then, the live (non-dry-run) lane refuses to run.
        log_emit error "live_handler_not_yet_wired" "tier=${tier}"
        emit_metric "${tier}" "stale"
        OVERALL_EXIT=2
        continue
    fi

    if (( age > rpo )); then
        log_emit warn "freshness_stale" "tier=${tier}" "age_seconds=${age}" "rpo_seconds=${rpo}"
        emit_metric "${tier}" "stale"
        OVERALL_EXIT=2
        continue
    fi
    log_emit info "freshness_ok" "tier=${tier}" "age_seconds=${age}" "rpo_seconds=${rpo}"

    # 2) Integrity sample (deterministic — every sample matches in dry-run).
    log_emit info "integrity_sample" "tier=${tier}" \
        "samples_per_tenant=${SAMPLES_PER_TENANT}" "matched=${SAMPLES_PER_TENANT}" \
        "mismatched=0" "missing=0"

    # 3) Sample-restore into ephemeral namespace.
    log_emit info "sample_restore_begin" "tier=${tier}" \
        "ephemeral_namespace=${EPHEMERAL_NS}" "restore_count=${RESTORE_SAMPLE_CAP}"
    log_emit info "sample_restore_ok" "tier=${tier}" \
        "restored=${RESTORE_SAMPLE_CAP}" "byte_matched=${RESTORE_SAMPLE_CAP}" \
        "byte_mismatched=0"

    emit_metric "${tier}" "ok"
    log_emit info "tier_end" "tier=${tier}" "status=ok"
done

# ---------------------------------------------------------------------------
# Ephemeral namespace cleanup.
# ---------------------------------------------------------------------------
log_emit info "ephemeral_cleanup_begin" "ephemeral_namespace=${EPHEMERAL_NS}"
if [[ "${DRY_RUN}" == "true" ]]; then
    log_emit info "ephemeral_cleanup_ok" "ephemeral_namespace=${EPHEMERAL_NS}" "dry_run=true"
else
    # Real path teardown would invoke:
    #   wrangler r2 bucket delete "${EPHEMERAL_NS}" --remote
    #   wrangler d1 delete "${EPHEMERAL_NS}_d1" --skip-confirmation
    #   wrangler kv namespace delete --namespace-id "${EPHEMERAL_NS}_kv"
    log_emit info "ephemeral_cleanup_skipped_live_handler_deferred" \
        "ephemeral_namespace=${EPHEMERAL_NS}"
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
