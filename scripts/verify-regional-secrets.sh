#!/usr/bin/env bash
# verify-regional-secrets.sh — confirm the 3 container-storage secrets are
# present on each regional Worker env (sam/lhr/nrt/syd). Wrangler exposes
# secret *names* via `wrangler secret list` but never values, so this is
# pure presence verification (CTRL-CRED-001 compliant).
#
# Usage:
#   scripts/verify-regional-secrets.sh

set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly WRANGLER="${WRANGLER:-$REPO_ROOT/worker/node_modules/.bin/wrangler}"
readonly REGIONS=(sam lhr nrt syd)
readonly REQUIRED=(R2_S3_ACCESS_KEY_ID R2_S3_SECRET_ACCESS_KEY CF_API_TOKEN)

if [[ ! -x "$WRANGLER" ]]; then
    echo "wrangler not found at $WRANGLER" >&2
    exit 127
fi

FAIL=0
for region in "${REGIONS[@]}"; do
    env="prod-$region"
    printf '\n── %s ──\n' "$env"
    NAMES=$("$WRANGLER" secret list --env "$env" 2>&1 | grep -oE '"name": "[^"]+"' | sed 's/"name": "//;s/"$//' || true)
    for req in "${REQUIRED[@]}"; do
        if grep -qx "$req" <<<"$NAMES"; then
            printf '  [PASS] %s\n' "$req"
        else
            printf '  [FAIL] %s — missing\n' "$req"
            FAIL=$((FAIL + 1))
        fi
    done
done

printf '\n'
if [[ "$FAIL" -eq 0 ]]; then
    printf 'All %d regional envs have all %d required secrets.\n' "${#REGIONS[@]}" "${#REQUIRED[@]}"
    exit 0
else
    printf '%d missing secret(s). Run: scripts/put-secrets-regional.sh --apply\n' "$FAIL"
    exit 1
fi
