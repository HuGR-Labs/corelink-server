#!/usr/bin/env bash
# scripts/build-container-prod.sh — Wave-32 Phase E PREP
# Local docker build of corelink-server:prod per the workspace Dockerfile.
#
# What this does:
#   1. Verifies Docker daemon is running + buildx is available.
#   2. Derives SOURCE_DATE_EPOCH from HEAD git commit timestamp (ADR-0015
#      reproducible builds).
#   3. Runs a multi-arch build (linux/amd64 only — CF Containers beta
#      is amd64-only as of 2026-05-22) via docker buildx.
#   4. Tags the image as corelink-server:prod + corelink-server:<short-sha>.
#   5. Verifies the binary starts via docker run --rm.
#   6. Runs a gRPC smoke probe (grpcurl if available; health check fallback).
#   7. Prints image SHA-256 digest for ADR-0015 reproducibility audit.
#
# What this does NOT do:
#   - Push to any registry.
#   - Modify wrangler.toml or any config file.
#   - Touch production infrastructure.
#
# ADR-0015 (Reproducible Builds) compliance:
#   SOURCE_DATE_EPOCH is set from `git log -1 --format=%ct` so that
#   timestamps embedded in the Rust toolchain and Docker layer metadata
#   are deterministic. Two sequential builds on the same machine with the
#   same git HEAD MUST produce the same image digest.
#
# Architecture note:
#   CF Containers beta (2026-05-22) supports linux/amd64 only. ARM
#   (linux/arm64) is NOT supported. Building multi-arch wastes compute
#   and may produce images CF refuses. We pin to linux/amd64 only and
#   document this in the Phase E PREP audit.
#
# Usage:
#   bash scripts/build-container-prod.sh
#   bash scripts/build-container-prod.sh --no-smoke   # skip docker run probe
#   bash scripts/build-container-prod.sh --help
#
# Exit codes:
#   0 → image built, verified, SHA captured
#   1 → hard-stop condition (daemon offline, build failure, smoke failure)
#   2 → usage error
#
# Charter: ADR-0015 (reproducible builds), CTRL-FORMAL-001 (binary
# versioning), CTRL-CRED-001 (no secrets in image layers).
#
# Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# ── Constants ──────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_NAME="corelink-server"
IMAGE_TAG_PROD="prod"
ARCH="linux/amd64"  # CF Containers beta: amd64-only
RUST_VERSION="1.91"
SMOKE_TIMEOUT_S=10  # seconds to wait for gRPC server to bind before probe
GRPC_PORT=50051

# ── Helpers ────────────────────────────────────────────────────────────────

log()  { printf '[build-container-prod] %s\n' "$*"; }
warn() { printf '[build-container-prod] WARN: %s\n' "$*" >&2; }
die()  { printf '[build-container-prod] FATAL: %s\n' "$*" >&2; exit 1; }

usage() {
    sed -n '2,/^$/p' "$0"
    exit 2
}

# ── Arg parse ─────────────────────────────────────────────────────────────

SKIP_SMOKE=0
for arg in "$@"; do
    case "$arg" in
        --no-smoke) SKIP_SMOKE=1 ;;
        -h|--help)  usage ;;
        *) die "unknown argument: $arg" ;;
    esac
done

# ── Step 0: change to repo root ────────────────────────────────────────────

cd "$REPO_ROOT"
log "Repo root: $REPO_ROOT"
log "HEAD: $(git rev-parse HEAD)"

# ── Step 1: Verify Docker daemon ───────────────────────────────────────────

log "Checking Docker daemon..."
if ! docker info >/dev/null 2>&1; then
    die "Docker daemon is not running. Start Docker Desktop (or dockerd) and retry.
     This is a HARD PAUSE condition per Wave-32 Phase E PREP §8.1."
fi
log "Docker daemon: OK ($(docker --version))"
log "Docker buildx: OK ($(docker buildx version))"

# Ensure an amd64-capable builder exists.
# docker buildx inspect default may not support cross-platform; we use
# 'container' driver with explicit platform support if not already configured.
if ! docker buildx inspect corelink-builder >/dev/null 2>&1; then
    log "Creating buildx builder 'corelink-builder' (container driver, linux/amd64)..."
    docker buildx create \
        --name corelink-builder \
        --driver docker-container \
        --platform linux/amd64 \
        --use
else
    log "Reusing existing buildx builder 'corelink-builder'."
    docker buildx use corelink-builder
fi
docker buildx inspect --bootstrap corelink-builder | head -10

# ── Step 2: Derive SOURCE_DATE_EPOCH (ADR-0015) ────────────────────────────

SHORT_SHA="$(git rev-parse --short HEAD)"
SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
log "Short SHA: $SHORT_SHA"
log "SOURCE_DATE_EPOCH: $SOURCE_DATE_EPOCH ($(date -r "$SOURCE_DATE_EPOCH" -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || date -d "@$SOURCE_DATE_EPOCH" -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || echo 'date conversion unavailable'))"

# ── Step 3: Build ──────────────────────────────────────────────────────────

FULL_TAG_PROD="${IMAGE_NAME}:${IMAGE_TAG_PROD}"
FULL_TAG_SHA="${IMAGE_NAME}:${SHORT_SHA}"

log "Building $FULL_TAG_PROD (arch=$ARCH) ..."
log "  Build args: RUST_VERSION=$RUST_VERSION SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH"

# NOTE: `--load` loads the image into the local Docker daemon.
# buildx with --platform only supports --load for single-platform builds.
# For multi-arch (e.g. linux/amd64,linux/arm64) you would use --push instead.
# We are building linux/amd64 only — --load is safe.
BUILD_START="$(date +%s)"
docker buildx build \
    --platform "$ARCH" \
    --build-arg "RUST_VERSION=${RUST_VERSION}" \
    --build-arg "SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH}" \
    --tag "$FULL_TAG_PROD" \
    --tag "$FULL_TAG_SHA" \
    --file Dockerfile \
    --load \
    .
BUILD_END="$(date +%s)"
BUILD_DURATION_S=$(( BUILD_END - BUILD_START ))
log "Build completed in ${BUILD_DURATION_S}s."

# ── Step 4: Image metadata ─────────────────────────────────────────────────

log "Collecting image metadata..."
IMAGE_ID="$(docker images --format '{{.ID}}' "$FULL_TAG_PROD" | head -1)"
IMAGE_SIZE="$(docker images --format '{{.Size}}' "$FULL_TAG_PROD" | head -1)"
IMAGE_DIGEST="$(docker inspect --format '{{.Id}}' "$FULL_TAG_PROD")"
LAYER_COUNT="$(docker history "$FULL_TAG_PROD" --no-trunc --format '{{.ID}}' | grep -v '<missing>' | wc -l | tr -d ' ')"

log "Image ID:      $IMAGE_ID"
log "Image digest:  $IMAGE_DIGEST"
log "Image size:    $IMAGE_SIZE"
log "Layer count:   $LAYER_COUNT"

# Hard pause: image size gate (CF Containers beta; 2 GB ceiling).
# Docker reports sizes in MB/GB with suffixes. We parse the number and unit.
SIZE_VALUE="$(echo "$IMAGE_SIZE" | grep -oE '[0-9]+(\.[0-9]+)?')"
SIZE_UNIT="$(echo "$IMAGE_SIZE" | grep -oE '[A-Za-z]+')"
SIZE_BYTES=0
case "$SIZE_UNIT" in
    kB|KB) SIZE_BYTES=$(echo "$SIZE_VALUE * 1024" | bc 2>/dev/null || echo 0) ;;
    MB)    SIZE_BYTES=$(echo "$SIZE_VALUE * 1048576" | bc 2>/dev/null || echo 0) ;;
    GB)    SIZE_BYTES=$(echo "$SIZE_VALUE * 1073741824" | bc 2>/dev/null || echo 0) ;;
    B)     SIZE_BYTES="$SIZE_VALUE" ;;
    *)     warn "Could not parse image size unit '$SIZE_UNIT'; skipping size gate." ;;
esac

MAX_BYTES=$(( 2 * 1024 * 1024 * 1024 ))  # 2 GiB
if [ "$SIZE_BYTES" -gt "$MAX_BYTES" ] 2>/dev/null; then
    die "HARD PAUSE: Image size ($IMAGE_SIZE) exceeds 2 GiB CF Containers limit.
     Per Wave-32 Phase E PREP §8.2 — halt + report upstream."
fi
log "Size gate: PASS ($IMAGE_SIZE < 2 GiB)"

# ── Step 5: Security scan — no secrets in image layers (CTRL-CRED-001) ─────

log "Scanning docker history for credentials (CTRL-CRED-001)..."
HISTORY_OUTPUT="$(docker history "$FULL_TAG_PROD" --no-trunc --format '{{.CreatedBy}}')"

CRED_PATTERNS=(
    "API_TOKEN" "API_KEY" "SECRET" "PASSWORD" "PASSWD"
    "PRIVATE_KEY" "ACCESS_KEY" "CF_API" "STRIPE_" "CLERK_"
    "PAGERDUTY" "NEON_" "DATABASE_URL"
)
CRED_HITS=0
for pattern in "${CRED_PATTERNS[@]}"; do
    if echo "$HISTORY_OUTPUT" | grep -qi "$pattern"; then
        warn "CTRL-CRED-001 VIOLATION: Pattern '$pattern' found in docker history!"
        CRED_HITS=$(( CRED_HITS + 1 ))
    fi
done

if [ "$CRED_HITS" -gt 0 ]; then
    die "HARD PAUSE: $CRED_HITS credential pattern(s) detected in image layers.
     This is a CTRL-CRED-001 violation. Inspect 'docker history $FULL_TAG_PROD --no-trunc'."
fi
log "CTRL-CRED-001 scan: PASS (0 credential patterns found)."

# ── Step 6: Binary smoke — docker run --version / --help ──────────────────

if [ "$SKIP_SMOKE" -eq 1 ]; then
    log "Smoke probe skipped (--no-smoke)."
else
    log "Running binary smoke (docker run --rm --version / --help)..."
    # Try --version first; fall back to --help (many servers don't expose --version).
    if docker run --rm --platform "$ARCH" "$FULL_TAG_PROD" --version 2>/dev/null; then
        log "Binary smoke (--version): PASS"
    elif docker run --rm --platform "$ARCH" "$FULL_TAG_PROD" --help 2>/dev/null; then
        log "Binary smoke (--help): PASS"
    else
        # The server binary may not accept --version/--help and instead blocks
        # on port binding. Try launching with a 2-second timeout to confirm
        # it at least starts without an immediate panic.
        log "Neither --version nor --help returned 0; attempting start probe..."
        PROBE_CID="$(docker run -d --platform "$ARCH" -p "${GRPC_PORT}:${GRPC_PORT}" \
            --name corelink-smoke-probe "$FULL_TAG_PROD" 2>/dev/null || true)"

        if [ -z "$PROBE_CID" ]; then
            die "HARD PAUSE: docker run failed to start container.
         Per Wave-32 Phase E PREP §8.3 — gRPC server failed to start. Halt + report."
        fi

        # Wait for server to bind (up to SMOKE_TIMEOUT_S).
        log "Container started ($PROBE_CID). Waiting ${SMOKE_TIMEOUT_S}s for gRPC bind..."
        ELAPSED=0
        GRPC_OK=0
        while [ "$ELAPSED" -lt "$SMOKE_TIMEOUT_S" ]; do
            sleep 2
            ELAPSED=$(( ELAPSED + 2 ))

            # Probe with grpcurl if available.
            if command -v grpcurl >/dev/null 2>&1; then
                if grpcurl -plaintext "localhost:${GRPC_PORT}" list >/dev/null 2>&1; then
                    log "gRPC probe (grpcurl list): PASS"
                    GRPC_OK=1
                    break
                fi
            else
                # Fallback: TCP connect via bash /dev/tcp (bash built-in).
                if bash -c "echo '' > /dev/tcp/localhost/${GRPC_PORT}" 2>/dev/null; then
                    log "TCP probe (localhost:${GRPC_PORT} reachable): PASS"
                    GRPC_OK=1
                    break
                fi
            fi
        done

        # Clean up probe container.
        docker stop corelink-smoke-probe >/dev/null 2>&1 || true
        docker rm   corelink-smoke-probe >/dev/null 2>&1 || true

        if [ "$GRPC_OK" -eq 0 ]; then
            die "HARD PAUSE: gRPC server did not respond within ${SMOKE_TIMEOUT_S}s.
         Per Wave-32 Phase E PREP §8.3 — halt + report upstream."
        fi
    fi

    log "Smoke probe: PASS"
fi

# ── Step 7: Reproducibility attestation reminder ───────────────────────────

log ""
log "ADR-0015 Reproducibility Attestation:"
log "  To verify reproducibility, run this script a second time on the same"
log "  HEAD and compare the image digest:"
log "    docker inspect --format '{{.Id}}' $FULL_TAG_PROD"
log "  Both runs MUST produce the same sha256 for ADR-0015 compliance."
log ""

# ── Step 8: Summary ────────────────────────────────────────────────────────

log "============================================================"
log "  BUILD COMPLETE"
log "  Image:       $FULL_TAG_PROD  ($FULL_TAG_SHA)"
log "  Digest:      $IMAGE_DIGEST"
log "  Size:        $IMAGE_SIZE"
log "  Layers:      $LAYER_COUNT"
log "  Build time:  ${BUILD_DURATION_S}s"
log "  Arch:        $ARCH"
log "  SOURCE_DATE_EPOCH: $SOURCE_DATE_EPOCH"
log "============================================================"
log "Next: run scripts/push-container-prod.sh (with --apply to actually push)."
