#!/usr/bin/env bash
# push-container-multiregion.sh — retag + push a locally-built container
# image to all 5 CF Containers registries (main prod + 4 regional envs).
#
# Pre-requisite: scripts/build-container-prod.sh has been run and
# `corelink-server:<sha>` exists locally. Pass the short SHA as $1, or
# default to `git rev-parse --short HEAD`.
#
# CF Containers auto-derives the registry path as:
#   registry.cloudflare.com/<account-id>/<worker>-<class>-<env>:<tag>
# where <worker> + <class> + <env> are read from the [[env.X.containers]]
# block in wrangler.toml.
#
# Usage:
#   scripts/push-container-multiregion.sh                    # dry-run (default)
#   scripts/push-container-multiregion.sh --apply            # push to all 5 registries
#   scripts/push-container-multiregion.sh --apply --sha f9859a59
#   scripts/push-container-multiregion.sh --apply --env prod,prod-sam  # subset

set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly WRANGLER="${WRANGLER:-$REPO_ROOT/worker/node_modules/.bin/wrangler}"

# (env, image-name) pairs matching wrangler.toml [[env.X.containers]] blocks.
# IMAGE_NAME is the registry path component; full URL is
# registry.cloudflare.com/<acct>/<IMAGE_NAME>:<tag>.
declare -a ENVS=(
    "prod:corelink-prod-corelinkserver-prod"
    "prod-sam:corelink-prod-sam-corelinkserver-prod"
    "prod-lhr:corelink-prod-lhr-corelinkserver-prod"
    "prod-nrt:corelink-prod-nrt-corelinkserver-prod"
    "prod-syd:corelink-prod-syd-corelinkserver-prod"
)

APPLY=false
SHA="$(cd "$REPO_ROOT" && git rev-parse --short HEAD)"
SUFFIX="r1"
SELECTED_ENVS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply)    APPLY=true; shift ;;
        --sha)      SHA="$2"; shift 2 ;;
        --suffix)   SUFFIX="$2"; shift 2 ;;
        --env)      IFS=',' read -ra SELECTED_ENVS <<<"$2"; shift 2 ;;
        --help|-h)
            sed -n '1,25p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

TAG="${SHA}-${SUFFIX}"
ACCT="$(grep '^CLOUDFLARE_ACCOUNT_ID=' "$REPO_ROOT/.env.local" | head -1 | cut -d= -f2-)"
LOCAL_REF="corelink-server:${SHA}"

log() { printf '[%s] %s\n' "$(basename "$0")" "$*"; }
err() { printf '[%s] ERROR: %s\n' "$(basename "$0")" "$*" >&2; }

log "Account ID:    $ACCT"
log "Local image:   $LOCAL_REF"
log "Target tag:    $TAG"
log ""

# Filter envs if --env given.
if [[ ${#SELECTED_ENVS[@]} -gt 0 ]]; then
    FILTERED=()
    for pair in "${ENVS[@]}"; do
        ek="${pair%%:*}"
        for s in "${SELECTED_ENVS[@]}"; do
            [[ "$ek" == "$s" ]] && FILTERED+=("$pair")
        done
    done
    ENVS=("${FILTERED[@]}")
fi

log "Push plan:"
for pair in "${ENVS[@]}"; do
    env="${pair%%:*}"
    img="${pair#*:}"
    full="registry.cloudflare.com/${ACCT}/${img}:${TAG}"
    log "  $env  →  $full"
done
log ""

if ! $APPLY; then
    log "DRY-RUN complete. Pass --apply to push."
    exit 0
fi

# Verify local image exists.
if ! docker image inspect "$LOCAL_REF" >/dev/null 2>&1; then
    err "Local image not found: $LOCAL_REF"
    err "Run scripts/build-container-prod.sh first, or pass --sha <short-sha> matching an existing local image."
    exit 1
fi

# Retag + push each.
N=0
TOTAL=${#ENVS[@]}
for pair in "${ENVS[@]}"; do
    N=$((N + 1))
    env="${pair%%:*}"
    img="${pair#*:}"
    full="registry.cloudflare.com/${ACCT}/${img}:${TAG}"

    log "[$N/$TOTAL] retag $LOCAL_REF → $full"
    if ! docker tag "$LOCAL_REF" "$full"; then
        err "[$N/$TOTAL] docker tag failed"
        exit 1
    fi

    log "[$N/$TOTAL] wrangler containers push --env $env"
    if ! "$WRANGLER" containers push "$full" --env "$env" 2>&1 | tail -10; then
        err "[$N/$TOTAL] wrangler push failed for $env"
        exit 1
    fi
    log "[$N/$TOTAL] $env :: $TAG OK"
    log ""
done

log "All $TOTAL pushes COMPLETE."
log "Next: bump tags in wrangler.toml + wrangler deploy --env <each>."
log "  sed -i.bak -E 's|(corelink-prod[^:]*-corelinkserver-prod):[^\"]+|\\1:${TAG}|g' wrangler.toml"
exit 0
