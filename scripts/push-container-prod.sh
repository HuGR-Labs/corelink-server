#!/usr/bin/env bash
# scripts/push-container-prod.sh — Wave-32 Phase E PREP
# Push the locally-built corelink-server:prod image to Cloudflare Containers
# registry via wrangler.
#
# What this does:
#   1. Verifies the local corelink-server:prod image exists (build gate).
#   2. Verifies wrangler is installed + CF credentials are available.
#   3. In dry-run mode (default): prints what would be pushed without acting.
#   4. In apply mode (--apply): executes `wrangler containers push` to push
#      the image to the CF Containers registry bound to the corelink worker.
#
# What this does NOT do:
#   - Deploy the Worker or start a Container (Phase E APPLY, blocked on D).
#   - Modify wrangler.toml or any config file.
#   - Touch D1, KV, R2, or DO infrastructure.
#
# Default mode: --dry-run (SAFE — no push, prints intent only).
# To actually push: pass --apply.
#
# CF Containers beta notes (2026-05-22):
#   - Registry is account-scoped; images are associated with the worker
#     binding via `[[containers]] image` in wrangler.toml.
#   - `wrangler containers push` authenticates via CF_API_TOKEN env var
#     OR a pre-existing `wrangler login` browser session.
#   - Image must be linux/amd64 (CF beta amd64-only).
#   - The push command tags and uploads the image; the actual container
#     instantiation happens at `wrangler deploy` time (Phase E APPLY).
#
# Usage:
#   bash scripts/push-container-prod.sh            # dry-run (safe)
#   bash scripts/push-container-prod.sh --apply    # actually push
#   bash scripts/push-container-prod.sh --help
#
# Exit codes:
#   0 → dry-run printed / image pushed successfully
#   1 → hard-stop condition (image missing, auth failure, push failure)
#   2 → usage error
#
# Charter: CTRL-CRED-001 (CF_API_TOKEN from env, NOT embedded in script),
# ADR-0015 (reproducible builds — image digest logged pre/post push).
#
# Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# ── Constants ──────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# CF Containers auto-names the registry path as:
#   <worker-name>-<class-name>-<env>   (all lowercased, hyphens only)
#
# Derivation from wrangler.toml:
#   worker name  (env.prod)    : corelink-prod   ← [env.prod] name = "corelink-prod"
#   DO class name              : CoreLinkServer  ← [[containers]] class_name = "CoreLinkServer"
#   env                        : prod
#   lowercased concatenation   : corelink-prod-corelinkserver-prod
#
# Full registry path:
#   registry.cloudflare.com/<account-id>/corelink-prod-corelinkserver-prod:<tag>
#
# IF you rename the worker or the DO class, update this constant AND the
# image = "..." in wrangler.toml [[env.prod.containers]] together.
IMAGE_NAME="corelink-prod-corelinkserver-prod"

IMAGE_TAG_PROD="prod"
FULL_TAG_PROD="${IMAGE_NAME}:${IMAGE_TAG_PROD}"

# CF Containers registry push command (wrangler containers push).
# As of wrangler 4.x (2026-05-22 beta), the subcommand is:
#   wrangler containers push <image-ref> [--env <environment>]
# The worker name comes from wrangler.toml `name = "corelink-prod"` (env.prod).
WRANGLER_ENV="prod"

# ── Helpers ────────────────────────────────────────────────────────────────

log()  { printf '[push-container-prod] %s\n' "$*"; }
warn() { printf '[push-container-prod] WARN: %s\n' "$*" >&2; }
die()  { printf '[push-container-prod] FATAL: %s\n' "$*" >&2; exit 1; }

usage() {
    sed -n '2,/^$/p' "$0"
    exit 2
}

# ── Arg parse ─────────────────────────────────────────────────────────────

DRY_RUN=1
for arg in "$@"; do
    case "$arg" in
        --apply)   DRY_RUN=0 ;;
        --dry-run) DRY_RUN=1 ;;
        -h|--help) usage ;;
        *) die "unknown argument: $arg" ;;
    esac
done

# ── Step 0: repo root ──────────────────────────────────────────────────────

cd "$REPO_ROOT"
log "Repo root: $REPO_ROOT"
log "HEAD: $(git rev-parse HEAD)"
SHORT_SHA="$(git rev-parse --short HEAD)"
FULL_TAG_SHA="${IMAGE_NAME}:${SHORT_SHA}"

# ── Step 0b: Sanity gate — IMAGE_NAME must match wrangler.toml ────────────
#
# wrangler.toml [[env.prod.containers]] image = "registry.cloudflare.com/<acct>/<name>:<tag>"
# Extract <name> from that line and verify it matches IMAGE_NAME above.
# This prevents silent drift if the worker or class is renamed in one place only.

EXPECTED_IMAGE_NAME="$(grep -E '^image = "registry\.cloudflare\.com/[^/]+/[^:"]+'  \
    "$REPO_ROOT/wrangler.toml" | head -1 | sed -E 's|.*registry\.cloudflare\.com/[^/]+/([^:]+):.*|\1|')"

if [ -z "$EXPECTED_IMAGE_NAME" ]; then
    die "Could not extract image name from wrangler.toml. Is the [[env.prod.containers]] image= line present?"
fi

if [ "$IMAGE_NAME" != "$EXPECTED_IMAGE_NAME" ]; then
    die "IMAGE_NAME drift detected!
     Script has:       IMAGE_NAME=$IMAGE_NAME
     wrangler.toml has: $EXPECTED_IMAGE_NAME
     Update IMAGE_NAME in this script (and the derivation comment) to match wrangler.toml,
     or fix wrangler.toml if you renamed the worker/class."
fi

log "Sanity gate (IMAGE_NAME vs wrangler.toml): PASS  ($IMAGE_NAME)"

# ── Step 1: Pre-push gate — image MUST exist locally ──────────────────────

log "Checking for local image $FULL_TAG_PROD..."
if ! docker image inspect "$FULL_TAG_PROD" >/dev/null 2>&1; then
    die "Pre-push gate FAILED: image '$FULL_TAG_PROD' not found locally.
     Run scripts/build-container-prod.sh first."
fi

IMAGE_DIGEST="$(docker inspect --format '{{.Id}}' "$FULL_TAG_PROD")"
IMAGE_SIZE="$(docker images --format '{{.Size}}' "$FULL_TAG_PROD" | head -1)"
log "Image found: $FULL_TAG_PROD"
log "  Digest: $IMAGE_DIGEST"
log "  Size:   $IMAGE_SIZE"
log "Pre-push gate: PASS"

# ── Step 2: Verify Docker daemon ───────────────────────────────────────────

if ! docker info >/dev/null 2>&1; then
    die "Docker daemon is not running. Start Docker Desktop (or dockerd) and retry."
fi

# ── Step 3: Verify wrangler ────────────────────────────────────────────────

# Phase D APPLY patch: use npx wrangler@latest (wrangler not in global PATH).
WRANGLER_CMD="npx wrangler@latest"
log "Wrangler: $($WRANGLER_CMD --version 2>&1 | head -1)"

# ── Step 4: Verify CF authentication ──────────────────────────────────────

# Auth precedence: CLOUDFLARE_API_TOKEN (primary; set in .env.local) >
# CF_API_TOKEN (legacy alias) > wrangler login session.
# CTRL-CRED-001: never embed credentials in this script.
if [ -n "${CLOUDFLARE_API_TOKEN:-}" ]; then
    log "CF auth: CLOUDFLARE_API_TOKEN env var set (CTRL-CRED-001: not logging value)."
elif [ -n "${CF_API_TOKEN:-}" ]; then
    log "CF auth: CF_API_TOKEN env var set (CTRL-CRED-001: not logging value)."
elif $WRANGLER_CMD whoami >/dev/null 2>&1; then
    log "CF auth: wrangler login session active ($($WRANGLER_CMD whoami 2>&1 | head -1))."
else
    die "CF auth FAILED: neither CLOUDFLARE_API_TOKEN/CF_API_TOKEN env var set nor wrangler login session found.
     Set CLOUDFLARE_API_TOKEN from .env.local or run 'npx wrangler@latest login'."
fi

# ── Step 5: Derive push target ─────────────────────────────────────────────

# wrangler containers push pushes the image referenced by the [[containers]]
# block in wrangler.toml for the given environment. The image tag we built
# locally must match (or be retag'd to) what wrangler expects.
# As of wrangler 4.x beta the push command accepts a local image reference.

log ""
if [ "$DRY_RUN" -eq 1 ]; then
    log "DRY-RUN MODE — the following command would be executed with --apply:"
    log ""
    log "  npx wrangler@latest containers push $FULL_TAG_PROD --env $WRANGLER_ENV"
    log ""
    log "What this would do:"
    log "  1. Authenticate to Cloudflare Containers registry using CF_API_TOKEN."
    log "  2. Tag and push local image $FULL_TAG_PROD to the CF registry"
    log "     associated with worker 'corelink' (prod environment)."
    log "  3. Image SHA-256 logged by CF registry; reproducible-build"
    log "     attestation preserved (ADR-0015)."
    log "  4. Does NOT deploy the worker or start a container."
    log "     (Deployment is Phase E APPLY, blocked on Phase D green.)"
    log ""
    log "Local image to be pushed:"
    log "  Tag:    $FULL_TAG_PROD"
    log "  Tag:    $FULL_TAG_SHA"
    log "  Digest: $IMAGE_DIGEST"
    log "  Size:   $IMAGE_SIZE"
    log ""
    log "DRY-RUN complete. Run with --apply when Phase D is green."
else
    log "APPLY MODE — executing push..."
    log ""
    log "Pushing $FULL_TAG_PROD to Cloudflare Containers registry (env: $WRANGLER_ENV)..."
    log "Image digest (pre-push): $IMAGE_DIGEST"

    # Execute the push.
    # NOTE: If `wrangler containers push` command syntax changes in a future
    # wrangler release, update this line and document the change in the
    # Phase E APPLY audit doc.
    # Phase D APPLY patch: use npx wrangler@latest (wrangler not in global PATH).
    $WRANGLER_CMD containers push "$FULL_TAG_PROD" --env "$WRANGLER_ENV"

    log ""
    log "Push completed."
    log "Image digest (post-push, ADR-0015 attestation):"
    docker inspect --format '  {{.Id}}' "$FULL_TAG_PROD"
    log ""
    log "Next steps (Phase E APPLY):"
    log "  1. wrangler deploy --env prod   (deploys worker + DO + container)"
    log "  2. bash scripts/verify-container-prod.sh  (post-deploy smoke)"
fi

# ── Step 6: Summary ────────────────────────────────────────────────────────

log ""
log "============================================================"
if [ "$DRY_RUN" -eq 1 ]; then
    log "  PUSH DRY-RUN COMPLETE (no push executed)"
else
    log "  PUSH COMPLETE"
fi
log "  Image:   $FULL_TAG_PROD  ($FULL_TAG_SHA)"
log "  Digest:  $IMAGE_DIGEST"
log "  Size:    $IMAGE_SIZE"
log "  Mode:    $([ "$DRY_RUN" -eq 1 ] && echo dry-run || echo applied)"
log "============================================================"
