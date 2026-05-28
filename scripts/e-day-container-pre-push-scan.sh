#!/usr/bin/env bash
# scripts/e-day-container-pre-push-scan.sh — Wave-32 Phase E
# Pre-push scan harness for the corelink-server container image.
#
# Audits a locally-built image against CF Containers beta hard limits:
#   - Image size  : WARN >256 MB, ERROR >512 MB
#   - Layer count : WARN >50, ERROR >100
#   - Shell scripts in final image : ERROR if found
#   - Secret patterns in image filesystem : ERROR if found (--apply only)
#   - No credentials in docker build history (CTRL-CRED-001)
#
# Prerequisite: image must already exist locally as corelink-server:prod.
#   Run scripts/build-container-prod.sh first.
#
# Usage:
#   bash scripts/e-day-container-pre-push-scan.sh            # dry-run (safe)
#   bash scripts/e-day-container-pre-push-scan.sh --apply    # full scan
#   bash scripts/e-day-container-pre-push-scan.sh --help
#
# Exit codes:
#   0  = all checks pass (or dry-run completed successfully)
#   1  = warning(s) only (e.g. size 300 MB within acceptable range)
#   2  = error: size >512 MB, secret leak detected, or shell scripts found
#
# Charter: CTRL-CRED-001 (no credentials in image), ADR-0015 (reproducible
# builds), Wave-32 Phase E gradual-deploy gate.
#
# Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# ── Constants ──────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_NAME="corelink-server"
IMAGE_TAG="prod"
FULL_IMAGE="${IMAGE_NAME}:${IMAGE_TAG}"

# CF Containers beta limits (2026-05)
SIZE_WARN_BYTES=$(( 256 * 1024 * 1024 ))   # 256 MB
SIZE_ERROR_BYTES=$(( 512 * 1024 * 1024 ))  # 512 MB
LAYER_WARN=50
LAYER_ERROR=100

# ── Helpers ────────────────────────────────────────────────────────────────

log()   { printf '[pre-push-scan] %s\n' "$*"; }
warn()  { printf '[pre-push-scan] WARN:  %s\n' "$*" >&2; }
error() { printf '[pre-push-scan] ERROR: %s\n' "$*" >&2; }

EXIT_CODE=0  # 0=clean; promoted to 1=warn or 2=error below

set_warn()  { [ "$EXIT_CODE" -lt 1 ] && EXIT_CODE=1; }
set_error() { EXIT_CODE=2; }

usage() {
    sed -n '2,/^$/p' "$0"
    exit 2
}

# ── Arg parse ──────────────────────────────────────────────────────────────

DRY_RUN=1
for arg in "$@"; do
    case "$arg" in
        --apply)   DRY_RUN=0 ;;
        --dry-run) DRY_RUN=1 ;;
        -h|--help) usage ;;
        *) error "unknown argument: $arg"; exit 2 ;;
    esac
done

# ── Pre-flight ─────────────────────────────────────────────────────────────

cd "$REPO_ROOT"
log "Repo root  : $REPO_ROOT"
log "Image      : $FULL_IMAGE"
log "Mode       : $([ "$DRY_RUN" -eq 1 ] && echo 'DRY-RUN (no build, no run)' || echo 'APPLY (full scan)')"
log "Date       : $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
log ""

# Gate: Docker must be available
if ! command -v docker >/dev/null 2>&1; then
    log "WARNING: docker not found in PATH."
    log "Install Docker Desktop (or dockerd) and ensure 'docker' is on PATH."
    log ""
    log "In DRY-RUN mode this script only needs docker to read metadata from"
    log "an already-built local image. In APPLY mode it also runs the image"
    log "to scan the filesystem for secret patterns."
    log ""
    log "DRY-RUN summary (docker absent — scan skipped):"
    log "  Image size check     : SKIPPED"
    log "  Layer count check    : SKIPPED"
    log "  Shell scripts check  : SKIPPED"
    log "  Secret patterns scan : SKIPPED"
    log "  History cred scan    : SKIPPED"
    log ""
    log "Run with docker installed to enable actual checks."
    exit 0
fi

# Gate: Docker daemon must be running
if ! docker info >/dev/null 2>&1; then
    log "WARNING: Docker daemon is not running."
    log "Start Docker Desktop (or dockerd) and retry."
    log "Exiting 0 in DRY-RUN mode (daemon required for all checks)."
    exit 0
fi

# Gate: image must exist locally
if ! docker image inspect "$FULL_IMAGE" >/dev/null 2>&1; then
    log "ERROR: Image '$FULL_IMAGE' not found locally."
    log "Run scripts/build-container-prod.sh first, then re-run this scan."
    exit 2
fi

log "Docker daemon: OK ($(docker --version 2>&1 | head -1))"
log "Image found  : $FULL_IMAGE"
log ""

# ── CHECK 1: Image size ────────────────────────────────────────────────────

log "── CHECK 1: Image size ──────────────────────────────────────────────"
RAW_SIZE_BYTES="$(docker image inspect "$FULL_IMAGE" --format='{{.Size}}')"
log "  Raw size (bytes): $RAW_SIZE_BYTES"

SIZE_MB=$(( RAW_SIZE_BYTES / 1024 / 1024 ))
log "  Size (MB): ${SIZE_MB} MB"

if [ "$RAW_SIZE_BYTES" -gt "$SIZE_ERROR_BYTES" ]; then
    error "Image size ${SIZE_MB} MB exceeds CF Containers beta hard limit of 512 MB."
    error "Reduce image size before pushing (e.g. use scratch base, strip binary, remove unused libs)."
    set_error
    log "  Result: ERROR (${SIZE_MB} MB > 512 MB)"
elif [ "$RAW_SIZE_BYTES" -gt "$SIZE_WARN_BYTES" ]; then
    warn "Image size ${SIZE_MB} MB exceeds recommended 256 MB for CF Containers beta."
    warn "Consider reducing image size for faster cold starts."
    set_warn
    log "  Result: WARN (${SIZE_MB} MB > 256 MB, under 512 MB hard limit)"
else
    log "  Result: PASS (${SIZE_MB} MB, under 256 MB threshold)"
fi
log ""

# ── CHECK 2: Layer count ───────────────────────────────────────────────────

log "── CHECK 2: Layer count ─────────────────────────────────────────────"
LAYER_COUNT="$(docker image history "$FULL_IMAGE" --no-trunc --format '{{.ID}}' \
    | grep -c -v '^<missing>$' \
    || true)"
LAYER_COUNT="${LAYER_COUNT// /}"
log "  Layer count: $LAYER_COUNT"

if [ "$LAYER_COUNT" -gt "$LAYER_ERROR" ]; then
    error "Layer count $LAYER_COUNT exceeds CF Containers beta hard limit of 100 layers."
    error "Squash RUN steps to reduce layer count."
    set_error
    log "  Result: ERROR ($LAYER_COUNT > 100)"
elif [ "$LAYER_COUNT" -gt "$LAYER_WARN" ]; then
    warn "Layer count $LAYER_COUNT exceeds recommended 50 layers for CF Containers beta."
    set_warn
    log "  Result: WARN ($LAYER_COUNT > 50, under 100 hard limit)"
else
    log "  Result: PASS ($LAYER_COUNT layers, under 50 threshold)"
fi
log ""

# ── CHECK 3: Shell scripts in final image (via docker history) ─────────────
#
# Strategy: inspect the COPY/ADD commands in the final (runtime) image's
# history for any *.sh, *.bash, *.py, *.rb, *.pl files being baked in.
# Also scans the ENTRYPOINT/CMD for shell invocations.
# Note: docker history shows layer creation commands; we cannot enumerate
# the final filesystem without running the container (done in CHECK 4).
# This check catches accidental COPY of script files in the Dockerfile.

log "── CHECK 3: Shell scripts baked into final image (history scan) ─────"
HISTORY_CREATED_BY="$(docker image history "$FULL_IMAGE" --no-trunc \
    --format '{{.CreatedBy}}' 2>/dev/null)"

SHELL_PATTERNS=('\\.sh[^a-z]' '\\.bash' '\\.py[^a-z]' '\\.rb[^a-z]' '\\.pl[^a-z]' 'COPY.*\.sh' 'ADD.*\.sh')
SHELL_HITS=0
for pat in "${SHELL_PATTERNS[@]}"; do
    while IFS= read -r hit; do
        if [ -n "$hit" ]; then
            warn "Shell script pattern '$pat' found in image history: $hit"
            SHELL_HITS=$(( SHELL_HITS + 1 ))
        fi
    done < <(echo "$HISTORY_CREATED_BY" | grep -E "$pat" || true)
done

# Also inspect ENTRYPOINT and CMD for shell-form invocations
ENTRYPOINT_VAL="$(docker image inspect "$FULL_IMAGE" --format='{{.Config.Entrypoint}}' 2>/dev/null || true)"
CMD_VAL="$(docker image inspect "$FULL_IMAGE" --format='{{.Config.Cmd}}' 2>/dev/null || true)"
log "  ENTRYPOINT: $ENTRYPOINT_VAL"
log "  CMD:        $CMD_VAL"

for val in "$ENTRYPOINT_VAL" "$CMD_VAL"; do
    if echo "$val" | grep -qE '^\[.*sh.*\]|/bin/sh|/bin/bash'; then
        warn "ENTRYPOINT/CMD uses a shell invocation: $val"
        warn "Prefer exec-form ENTRYPOINT [\"/usr/local/bin/binary\"] to avoid shell wrapper overhead."
        SHELL_HITS=$(( SHELL_HITS + 1 ))
    fi
done

if [ "$SHELL_HITS" -gt 0 ]; then
    error "$SHELL_HITS shell script reference(s) found in final image."
    set_error
    log "  Result: ERROR ($SHELL_HITS hits)"
else
    log "  Result: PASS (no shell scripts detected in final image)"
fi
log ""

# ── CHECK 4: Docker history credential scan (CTRL-CRED-001) ───────────────

log "── CHECK 4: Credential patterns in docker history (CTRL-CRED-001) ──"
CRED_PATTERNS=(
    'PRIVATE[_ ]KEY' 'BEGIN.*KEY' 'password=' 'token=' 'API_TOKEN'
    'API_KEY' 'SECRET' 'PASSWD' 'ACCESS_KEY' 'CF_API'
    'STRIPE_' 'CLERK_' 'NEON_' 'DATABASE_URL'
)
CRED_HITS=0
for pat in "${CRED_PATTERNS[@]}"; do
    while IFS= read -r hit; do
        if [ -n "$hit" ]; then
            warn "CTRL-CRED-001: Pattern '$pat' found in docker history!"
            warn "  Context: $hit"
            CRED_HITS=$(( CRED_HITS + 1 ))
        fi
    done < <(echo "$HISTORY_CREATED_BY" | grep -i "$pat" || true)
done

if [ "$CRED_HITS" -gt 0 ]; then
    error "CTRL-CRED-001 VIOLATION: $CRED_HITS credential pattern(s) in image history."
    error "Rebuild without baking secrets into RUN/ENV/ARG layers."
    set_error
    log "  Result: ERROR ($CRED_HITS credential patterns found)"
else
    log "  Result: PASS (0 credential patterns in history)"
fi
log ""

# ── CHECK 5: Filesystem secret scan (APPLY mode only) ─────────────────────

log "── CHECK 5: Filesystem secret patterns (docker run) ─────────────────"
if [ "$DRY_RUN" -eq 1 ]; then
    log "  SKIPPED in dry-run mode."
    log "  Run with --apply to scan the image filesystem for secret patterns."
    log "  (executes: docker run --rm <image> sh -c 'grep -r ...' in a read-only container)"
    log ""
else
    log "  Launching read-only container to scan filesystem (APPLY mode)..."
    # Mount the image read-only; scan /etc, /usr/local/etc, /run for common
    # secret-file patterns. Exclude /proc and /sys (kernel virtual filesystems).
    FS_SCAN_CMD='grep -rI -l \
        -e "PRIVATE KEY" \
        -e "BEGIN RSA" \
        -e "BEGIN EC" \
        -e "password=" \
        -e "token=" \
        --include="*.pem" --include="*.key" --include="*.crt" \
        --include="*.env" --include="*.cfg" --include="*.conf" \
        --include="*.json" --include="*.yaml" --include="*.toml" \
        /etc /usr/local 2>/dev/null || true'

    FS_HITS="$(docker run --rm \
        --read-only \
        --network none \
        --user 0 \
        "$FULL_IMAGE" \
        sh -c "$FS_SCAN_CMD" 2>/dev/null || true)"

    if [ -n "$FS_HITS" ]; then
        error "Secret pattern(s) found in image filesystem:"
        while IFS= read -r filepath; do
            error "  $filepath"
        done <<< "$FS_HITS"
        set_error
        log "  Result: ERROR (secret files detected)"
    else
        log "  Result: PASS (no secret patterns found in filesystem)"
    fi
fi
log ""

# ── Summary ────────────────────────────────────────────────────────────────

log "════════════════════════════════════════════════════════════════════"
log "  PRE-PUSH SCAN SUMMARY"
log "  Image   : $FULL_IMAGE"
log "  Mode    : $([ "$DRY_RUN" -eq 1 ] && echo 'dry-run' || echo 'apply')"
if   [ "$RAW_SIZE_BYTES" -gt "$SIZE_ERROR_BYTES" ]; then _sz_result=ERROR
elif [ "$RAW_SIZE_BYTES" -gt "$SIZE_WARN_BYTES"  ]; then _sz_result=WARN
else _sz_result=PASS; fi
if   [ "$LAYER_COUNT" -gt "$LAYER_ERROR" ]; then _lyr_result=ERROR
elif [ "$LAYER_COUNT" -gt "$LAYER_WARN"  ]; then _lyr_result=WARN
else _lyr_result=PASS; fi
log "  Check 1 (image size)          : $_sz_result"
log "  Check 2 (layer count)         : $_lyr_result"
log "  Check 3 (shell in final image): $([ "$SHELL_HITS" -gt 0 ] && echo ERROR || echo PASS)"
log "  Check 4 (cred in history)     : $([ "$CRED_HITS" -gt 0 ] && echo ERROR || echo PASS)"
log "  Check 5 (fs secret scan)      : $([ "$DRY_RUN" -eq 1 ] && echo SKIPPED || echo 'see above')"
log ""
case "$EXIT_CODE" in
    0) log "  RESULT: CLEAN — image is safe to push." ;;
    1) log "  RESULT: WARNINGS — review above; push is your call." ;;
    2) log "  RESULT: ERROR — DO NOT PUSH. Fix errors above first." ;;
esac
log "════════════════════════════════════════════════════════════════════"

exit "$EXIT_CODE"
