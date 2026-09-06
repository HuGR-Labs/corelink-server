#!/usr/bin/env bash
# verify-b062-prod-pins.sh — read-only B-062 production image evidence.
#
# This is deliberately separate from deploy-container-prod.sh: B-062 measures
# all five live applications, while the deploy helper is scoped to one env and
# may use the stale-prone applications LIST endpoint to resolve an id. The
# application ids below are stable ids recorded in
# docs/operator/container-region-cost-policy.md; every state read here is an
# authoritative GET by id.
#
# The image's commit must be an ancestor of MAIN_REF. This proves that live
# production is executing code that originated on main without pretending that
# an older, reachable commit is the current main tip. The output reports both
# facts so an operator can decide whether a repin/deploy is still warranted.
#
# Read-only: this script never calls PATCH, POST, DELETE, wrangler, or the
# applications LIST endpoint.
#
# Usage:
#   bash scripts/verify-b062-prod-pins.sh
#   MAIN_REF=origin/main bash scripts/verify-b062-prod-pins.sh
#
# Required environment:
#   CLOUDFLARE_ACCOUNT_ID
#   CLOUDFLARE_CONTAINERS_API_TOKEN (the scoped Containers API read token)
#
# Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CF_API_BASE="https://api.cloudflare.com/client/v4"
MAIN_REF="${MAIN_REF:-origin/main}"

log()  { printf '[verify-b062-prod-pins] %s\n' "$*"; }
fail() { printf '[verify-b062-prod-pins] FAIL: %s\n' "$*" >&2; }
die()  { fail "$*"; exit 2; }

cd "$REPO_ROOT"

# Local operator convenience. Values are never printed; CI should inject the
# same variables directly because .env.local is intentionally gitignored.
if [ -f "$REPO_ROOT/.env.local" ]; then
    set -a
    # shellcheck source=/dev/null
    source "$REPO_ROOT/.env.local"
    set +a
fi

[ -n "${CLOUDFLARE_ACCOUNT_ID:-}" ] || die "CLOUDFLARE_ACCOUNT_ID is not set"
[ -n "${CLOUDFLARE_CONTAINERS_API_TOKEN:-}" ] || die "CLOUDFLARE_CONTAINERS_API_TOKEN is not set"
command -v curl >/dev/null 2>&1 || die "curl is required"
command -v jq >/dev/null 2>&1 || die "jq is required"

MAIN_SHA="$(git rev-parse "${MAIN_REF}^{commit}" 2>/dev/null)" || \
    die "MAIN_REF '${MAIN_REF}' is not resolvable; fetch full main history before measuring"

# env, region, application id, expected image name, expected API application name
APPS=(
    'prod|iad|a033572c-0803-4866-b3a3-61f4812843b1|corelink-prod-corelinkserver-prod|corelink-prod-corelinkserver-prod'
    'prod-sam|sam|a0337243-13cb-46ef-adb1-781294294404|corelink-prod-sam-corelinkserver-prod|corelink-prod-sam-corelinkserver-prod-sam'
    'prod-lhr|lhr|a03faa1b-70c4-40eb-aa38-9f70d43de992|corelink-prod-lhr-corelinkserver-prod|corelink-prod-lhr-corelinkserver-prod-lhr'
    'prod-nrt|nrt|a033417c-db96-467b-a65c-83951d1fa5d1|corelink-prod-nrt-corelinkserver-prod|corelink-prod-nrt-corelinkserver-prod-nrt'
    'prod-syd|syd|a030dd8d-c24c-41bb-bf03-a1fb3542eac0|corelink-prod-syd-corelinkserver-prod|corelink-prod-syd-corelinkserver-prod-syd'
)

log "main ref: ${MAIN_REF} (${MAIN_SHA})"
log "reading five applications by id via GET (never the applications list)"

first_tag=""
failures=0
for app in "${APPS[@]}"; do
    IFS='|' read -r env_name region app_id expected_image_name expected_app_name <<< "$app"
    endpoint="${CF_API_BASE}/accounts/${CLOUDFLARE_ACCOUNT_ID}/containers/applications/${app_id}"

    if ! response="$(curl -sS --max-time 30 \
        -H "Authorization: Bearer ${CLOUDFLARE_CONTAINERS_API_TOKEN}" \
        -H 'Content-Type: application/json' "$endpoint")"; then
        fail "${env_name}/${region} (${app_id}): GET transport failed"
        failures=$((failures + 1))
        continue
    fi

    state="$(printf '%s\n' "$response" | jq -r '
        if (.success == true and .result != null) then
          [
            (.result.name // ""),
            (.result.configuration.image // ""),
            ((.result.health.instances.healthy // 0) | tostring),
            ((.result.health.instances.failed // 0) | tostring),
            ((.result.instances // 0) | tostring),
            ((.result.version // 0) | tostring),
            (.result.updated_at // "")
          ] | @tsv
        else empty end' 2>/dev/null || true)"
    if [ -z "$state" ]; then
        api_errors="$(printf '%s\n' "$response" | jq -r 'if (.errors // []) | length > 0 then "api_error" else "invalid_response" end' 2>/dev/null || printf 'invalid_response')"
        fail "${env_name}/${region} (${app_id}): ${api_errors}"
        failures=$((failures + 1))
        continue
    fi

    IFS=$'\t' read -r actual_name image healthy failed desired version updated_at <<< "$state"
    if [ "$actual_name" != "$expected_app_name" ]; then
        fail "${env_name}/${region} (${app_id}): unexpected application name '${actual_name}'"
        failures=$((failures + 1))
    fi
    case "$image" in
        "registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/${expected_image_name}:"*) ;;
        *)
            fail "${env_name}/${region} (${app_id}): unexpected image '${image}'"
            failures=$((failures + 1))
            continue
            ;;
    esac

    tag="${image##*:}"
    if [[ ! "$tag" =~ ^[0-9a-f]{7,40}-r[0-9]+$ ]]; then
        fail "${env_name}/${region} (${app_id}): image tag '${tag}' is not a commit-rN pin"
        failures=$((failures + 1))
        continue
    fi
    tag_sha="${tag%-r*}"
    pin_commit="$(git rev-parse "${tag_sha}^{commit}" 2>/dev/null || true)"
    if [ -z "$pin_commit" ]; then
        fail "${env_name}/${region} (${app_id}): pin ${tag_sha} is not in local git history"
        failures=$((failures + 1))
    elif ! git merge-base --is-ancestor "$pin_commit" "$MAIN_REF"; then
        fail "${env_name}/${region} (${app_id}): pin ${tag_sha} is not reachable from ${MAIN_REF}"
        failures=$((failures + 1))
    fi

    if [ -z "$first_tag" ]; then
        first_tag="$tag"
    elif [ "$tag" != "$first_tag" ]; then
        fail "${env_name}/${region} (${app_id}): pin ${tag} differs from ${first_tag}"
        failures=$((failures + 1))
    fi

    if ! [[ "$healthy" =~ ^[0-9]+$ && "$failed" =~ ^[0-9]+$ && "$desired" =~ ^[0-9]+$ ]]; then
        fail "${env_name}/${region} (${app_id}): malformed health counts"
        failures=$((failures + 1))
    elif [ "$failed" -ne 0 ] || [ "$healthy" -lt 1 ]; then
        fail "${env_name}/${region} (${app_id}): health healthy=${healthy} failed=${failed} desired=${desired}"
        failures=$((failures + 1))
    fi

    log "${env_name}/${region}: image=${image} healthy=${healthy} failed=${failed} desired=${desired} version=${version} updated_at=${updated_at}"
done

if [ "$failures" -ne 0 ]; then
    fail "${failures} B-062 measurement check(s) failed"
    exit 1
fi

log "PASS: all five live applications converge on ${first_tag}, whose commit is reachable from ${MAIN_REF} (${MAIN_SHA})."
log "NOTE: reachability is not tip equality; inspect the main-to-pin container diff before any repin/deploy decision."
