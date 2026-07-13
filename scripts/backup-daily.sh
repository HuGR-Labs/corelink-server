#!/usr/bin/env bash
# R-6 prep: Daily backup script.
#
# Snapshots all CoreLink durable state (D1 + R2 cold-tier + KV) into a
# dedicated backup R2 bucket with GPG-encrypted artifacts, emits an
# audit event with checksum, and enforces a 30-minute wall-clock SLA.
#
# Usage:
#   ./scripts/backup-daily.sh [--env staging|production] [--dry-run]
#
# Required env:
#   BACKUP_GPG_RECIPIENT   GPG key id / email used to encrypt artifacts.
#   BACKUP_R2_BUCKET       Dedicated backup R2 bucket (90d retention).
#                          MUST NOT collide with hot or cold bucket.
#   CORELINK_ENV           staging | production (defaults to --env arg).
#
# Optional env:
#   D1_DATABASES           Space-separated list of D1 db names.
#                          Default: corelink-prod-d1 (the real consolidated
#                          prod D1; the old corelink_core/audit/billing names
#                          never existed in the account).
#   KV_NAMESPACES          Space-separated KV namespace IDs (not bindings — the
#                          backup runs without the app wrangler.toml, so it must
#                          address KV by --namespace-id). Default: the durable
#                          CoreLink prod namespaces (jwks / idempotency /
#                          pilot-signup). Ephemeral KV (cache / rate-limit /
#                          session) is intentionally omitted — it regenerates.
#   R2_COLD_BUCKET         Source cold-tier R2 bucket.
#                          Default: corelink-cold-${CORELINK_ENV}.
#   AUDIT_OUTBOX_URL       Endpoint that ingests corelink.* audit events.
#
# SLA: < 30 minutes wall.
#
# This script is intentionally idempotent for re-runs within the same UTC
# day: existing manifest entries are overwritten, never duplicated.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC2034  # REPO_ROOT reserved for sibling-script sourcing.
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DATE_UTC="$(date -u +%Y-%m-%d)"
TS_START="$(date -u +%s)"
SLA_SECONDS=1800

# ---------------------------------------------------------------------------
# Args + env
# ---------------------------------------------------------------------------
ENV_OVERRIDE=""
DRY_RUN="false"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --env) ENV_OVERRIDE="$2"; shift 2 ;;
        --dry-run) DRY_RUN="true"; shift ;;
        -h|--help)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        *) echo "[FATAL] unknown arg: $1" >&2; exit 64 ;;
    esac
done

CORELINK_ENV="${ENV_OVERRIDE:-${CORELINK_ENV:-staging}}"
BACKUP_GPG_RECIPIENT="${BACKUP_GPG_RECIPIENT:-}"
BACKUP_R2_BUCKET="${BACKUP_R2_BUCKET:-corelink-backups-${CORELINK_ENV}}"
R2_COLD_BUCKET="${R2_COLD_BUCKET:-corelink-cold-${CORELINK_ENV}}"
D1_DATABASES="${D1_DATABASES:-corelink-prod-d1}"
KV_NAMESPACES="${KV_NAMESPACES:-924c6c0f9ee4439f96ec3a75a98eef8b 0e4fbd39e30c4610a9b7b66f1f4847e1 13005d94c4404387b21c46960ea05380}"
AUDIT_OUTBOX_URL="${AUDIT_OUTBOX_URL:-}"

# ---------------------------------------------------------------------------
# Logging helpers (NEVER log secrets / passphrases).
# ---------------------------------------------------------------------------
log()  { echo "[$(date -u +%H:%M:%SZ)] $*"; }
fail() { echo "[FATAL] $*" >&2; exit 1; }

log "=== corelink backup-daily ==="
log "env=${CORELINK_ENV} dry_run=${DRY_RUN} date=${DATE_UTC}"
log "backup_bucket=${BACKUP_R2_BUCKET}"
log "d1_databases=${D1_DATABASES}"
log "kv_namespaces=${KV_NAMESPACES}"
log "sla_seconds=${SLA_SECONDS}"

# Guard: refuse to run against prod from an ARBITRARY CI context (e.g. a PR
# build). The sanctioned daily backup automation
# (.github/workflows/backup-daily.yml — DD-1) opts in explicitly by exporting
# BACKUP_ALLOW_CI=true, which is the SRE-owned, scheduled, prod-backup lane.
# This keeps incidental CI from ever touching prod while letting the one
# blessed scheduled workflow run the live export.
if [[ "${CORELINK_ENV}" == "production" && "${CI:-false}" == "true" \
      && "${BACKUP_ALLOW_CI:-false}" != "true" ]]; then
    fail "production backups from CI require BACKUP_ALLOW_CI=true (sanctioned scheduled lane only)"
fi

# Required env validation.
if [[ -z "${BACKUP_GPG_RECIPIENT}" && "${DRY_RUN}" != "true" ]]; then
    fail "BACKUP_GPG_RECIPIENT required (cannot encrypt artifacts)"
fi

# ---------------------------------------------------------------------------
# Workspace
# ---------------------------------------------------------------------------
WORK_DIR="$(mktemp -d -t corelink-backup-XXXXXX)"
trap 'rm -rf "${WORK_DIR}"' EXIT
log "workspace=${WORK_DIR}"

MANIFEST="${WORK_DIR}/manifest.json"
printf '{\n  "date": "%s",\n  "env": "%s",\n  "artifacts": [\n' \
    "${DATE_UTC}" "${CORELINK_ENV}" > "${MANIFEST}"
FIRST_ARTIFACT="true"

append_manifest() {
    local kind="$1" name="$2" path="$3" checksum="$4" bytes="$5"
    if [[ "${FIRST_ARTIFACT}" == "true" ]]; then
        FIRST_ARTIFACT="false"
    else
        printf ',\n' >> "${MANIFEST}"
    fi
    printf '    {"kind":"%s","name":"%s","path":"%s","sha256":"%s","bytes":%s}' \
        "${kind}" "${name}" "${path}" "${checksum}" "${bytes}" >> "${MANIFEST}"
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

bytes_of() {
    if command -v stat >/dev/null 2>&1; then
        # Try GNU stat first, fall back to BSD stat.
        stat -c%s "$1" 2>/dev/null || stat -f%z "$1"
    else
        wc -c < "$1"
    fi
}

encrypt_artifact() {
    # Encrypts in-place ($1 -> $1.gpg, removes plaintext on success).
    local plain="$1" cipher="$1.gpg"
    if [[ "${DRY_RUN}" == "true" ]]; then
        cp "${plain}" "${cipher}"
        log "DRY-RUN: skipped GPG encrypt for $(basename "${plain}")"
    else
        # NOTE: passphrase MUST come from gpg-agent / yubikey; we do NOT
        # accept BACKUP_GPG_PASSPHRASE on the command line by design.
        gpg --batch --yes \
            --trust-model always \
            --recipient "${BACKUP_GPG_RECIPIENT}" \
            --output "${cipher}" \
            --encrypt "${plain}" \
            || fail "gpg encrypt failed for $(basename "${plain}")"
    fi
    rm -f "${plain}"
    echo "${cipher}"
}

upload_artifact() {
    # Uploads $1 to R2 path $2 under BACKUP_R2_BUCKET.
    local local_path="$1" remote_key="$2"
    if [[ "${DRY_RUN}" == "true" ]]; then
        log "DRY-RUN: would upload ${local_path} -> r2://${BACKUP_R2_BUCKET}/${remote_key}"
        return 0
    fi
    wrangler r2 object put \
        "${BACKUP_R2_BUCKET}/${remote_key}" \
        --file "${local_path}" \
        --remote \
        || fail "wrangler r2 object put failed for ${remote_key}"
}

# ---------------------------------------------------------------------------
# Phase 1 — D1 exports per database.
# ---------------------------------------------------------------------------
log "Phase 1: D1 exports"
for db in ${D1_DATABASES}; do
    plain="${WORK_DIR}/d1-${db}-${DATE_UTC}.sql"
    log "  exporting d1 db=${db}"
    if [[ "${DRY_RUN}" == "true" ]]; then
        printf -- '-- dry-run d1 export %s %s\n' "${db}" "${DATE_UTC}" > "${plain}"
    else
        wrangler d1 export "${db}" --remote --output="${plain}" \
            || fail "wrangler d1 export failed for ${db}"
    fi
    checksum="$(sha256_of "${plain}")"
    size="$(bytes_of "${plain}")"
    cipher="$(encrypt_artifact "${plain}")"
    remote_key="${DATE_UTC}/d1/$(basename "${cipher}")"
    upload_artifact "${cipher}" "${remote_key}"
    append_manifest "d1" "${db}" "${remote_key}" "${checksum}" "${size}"
done

# ---------------------------------------------------------------------------
# Phase 2 — R2 cold-tier snapshot.
# ---------------------------------------------------------------------------
log "Phase 2: R2 cold-tier snapshot (source=${R2_COLD_BUCKET})"
r2_inventory="${WORK_DIR}/r2-cold-inventory-${DATE_UTC}.json"
# The R2 cold-tier snapshot is an OPTIONAL belt-and-suspenders copy of the CAS
# blobs, which are ALREADY multi-region replicated (corelink-cas-{iad,lhr,nrt,
# sam,syd}). It requires rclone + RCLONE_CONF_BASE64 + a cold bucket. When those
# are not provisioned, SKIP it (warning, manifest note) rather than FATAL — a
# fatal here would also abort the CRITICAL D1 (done above) + KV (Phase 3) backup.
if [[ "${DRY_RUN}" == "true" ]]; then
    printf '{"dry_run":true,"bucket":"%s","date":"%s"}\n' \
        "${R2_COLD_BUCKET}" "${DATE_UTC}" > "${r2_inventory}"
elif ! command -v rclone >/dev/null 2>&1 \
        || [[ ! -f "${HOME}/.config/rclone/rclone.conf" ]]; then
    echo "::warning ::Phase 2 SKIPPED — R2 cold-tier not configured (rclone / RCLONE_CONF_BASE64 / ${R2_COLD_BUCKET} absent). CAS is multi-region replicated; provision to enable the cold snapshot." >&2
    log "Phase 2 SKIPPED: cold-tier unconfigured (CAS is multi-region replicated)"
    append_manifest "r2_inventory" "${R2_COLD_BUCKET}" "SKIPPED_UNCONFIGURED" "" "0"
    r2_inventory=""
else
    wrangler r2 object list "${R2_COLD_BUCKET}" --remote --json \
        > "${r2_inventory}" \
        || fail "wrangler r2 object list failed for ${R2_COLD_BUCKET}"

    # rclone sync for full byte-for-byte snapshot. The R2 backup remote
    # MUST be configured as 'corelink-r2-backup' in ${HOME}/.config/rclone.
    rclone sync \
        "corelink-r2-source:${R2_COLD_BUCKET}" \
        "corelink-r2-backup:${BACKUP_R2_BUCKET}/${DATE_UTC}/r2-cold/" \
        --transfers 16 \
        --checksum \
        --immutable \
        || fail "rclone sync failed for cold-tier"
fi
if [[ -n "${r2_inventory}" ]]; then
    inv_checksum="$(sha256_of "${r2_inventory}")"
    inv_size="$(bytes_of "${r2_inventory}")"
    inv_cipher="$(encrypt_artifact "${r2_inventory}")"
    inv_key="${DATE_UTC}/r2/$(basename "${inv_cipher}")"
    upload_artifact "${inv_cipher}" "${inv_key}"
    append_manifest "r2_inventory" "${R2_COLD_BUCKET}" "${inv_key}" \
        "${inv_checksum}" "${inv_size}"
fi

# ---------------------------------------------------------------------------
# Phase 3 — KV namespace dumps.
# ---------------------------------------------------------------------------
log "Phase 3: KV namespace dumps"
for ns in ${KV_NAMESPACES}; do
    plain="${WORK_DIR}/kv-${ns}-${DATE_UTC}.jsonl"
    keys_tmp="${WORK_DIR}/kv-${ns}-keys.json"
    log "  dumping kv namespace=${ns}"
    if [[ "${DRY_RUN}" == "true" ]]; then
        printf '{"dry_run":true,"namespace":"%s"}\n' "${ns}" > "${plain}"
    else
        wrangler kv key list --namespace-id "${ns}" --remote > "${keys_tmp}" \
            || fail "wrangler kv key list failed for ${ns}"
        : > "${plain}"
        # Iterate keys and emit JSONL {"k":"...","v":"..."}. We use jq
        # if available; otherwise we require it (fail fast).
        if ! command -v jq >/dev/null 2>&1; then
            fail "jq is required for KV dump but not installed"
        fi
        while IFS= read -r key; do
            [[ -z "${key}" ]] && continue
            value="$(wrangler kv key get --namespace-id "${ns}" --remote "${key}" \
                || echo "")"
            jq -cn --arg k "${key}" --arg v "${value}" \
                '{k:$k, v:$v}' >> "${plain}"
        done < <(jq -r '.[].name' "${keys_tmp}")
    fi
    checksum="$(sha256_of "${plain}")"
    size="$(bytes_of "${plain}")"
    cipher="$(encrypt_artifact "${plain}")"
    remote_key="${DATE_UTC}/kv/$(basename "${cipher}")"
    upload_artifact "${cipher}" "${remote_key}"
    append_manifest "kv" "${ns}" "${remote_key}" "${checksum}" "${size}"
done

# ---------------------------------------------------------------------------
# Phase 4 — Manifest + retention metadata.
# ---------------------------------------------------------------------------
TS_END="$(date -u +%s)"
ELAPSED=$((TS_END - TS_START))
printf '\n  ],\n  "elapsed_seconds": %s,\n  "sla_seconds": %s,\n  "sla_met": %s,\n  "retention_days": 90\n}\n' \
    "${ELAPSED}" "${SLA_SECONDS}" \
    "$([[ ${ELAPSED} -le ${SLA_SECONDS} ]] && echo true || echo false)" \
    >> "${MANIFEST}"

manifest_checksum="$(sha256_of "${MANIFEST}")"
manifest_size="$(bytes_of "${MANIFEST}")"
manifest_remote="${DATE_UTC}/manifest.json"
upload_artifact "${MANIFEST}" "${manifest_remote}"

log "manifest sha256=${manifest_checksum} bytes=${manifest_size}"
log "elapsed=${ELAPSED}s sla=${SLA_SECONDS}s"

# ---------------------------------------------------------------------------
# Phase 5 — Audit event emission.
# ---------------------------------------------------------------------------
audit_payload=$(jq -cn \
    --arg event "corelink.backup.daily.completed" \
    --arg env "${CORELINK_ENV}" \
    --arg date "${DATE_UTC}" \
    --arg manifest_key "${manifest_remote}" \
    --arg manifest_sha256 "${manifest_checksum}" \
    --argjson elapsed "${ELAPSED}" \
    --argjson sla "${SLA_SECONDS}" \
    --argjson sla_met "$([[ ${ELAPSED} -le ${SLA_SECONDS} ]] && echo true || echo false)" \
    '{event:$event, env:$env, date:$date, manifest_key:$manifest_key,
      manifest_sha256:$manifest_sha256, elapsed_seconds:$elapsed,
      sla_seconds:$sla, sla_met:$sla_met}')

if [[ -n "${AUDIT_OUTBOX_URL}" && "${DRY_RUN}" != "true" ]]; then
    curl --fail --silent --show-error \
        --max-time 10 \
        -H "content-type: application/json" \
        -X POST "${AUDIT_OUTBOX_URL}" \
        -d "${audit_payload}" \
        || log "WARN: audit outbox emit failed (will retry via outbox cron)"
else
    log "audit event (not sent — no outbox url): ${audit_payload}"
fi

# ---------------------------------------------------------------------------
# Phase 6 — SLA enforcement.
# ---------------------------------------------------------------------------
if [[ ${ELAPSED} -gt ${SLA_SECONDS} ]]; then
    fail "backup wall-clock SLA breached: ${ELAPSED}s > ${SLA_SECONDS}s"
fi

log "=== backup-daily OK (elapsed=${ELAPSED}s) ==="
exit 0
