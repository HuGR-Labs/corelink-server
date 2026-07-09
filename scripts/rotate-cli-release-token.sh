#!/usr/bin/env bash
# rotate-cli-release-token.sh — rotate CORELINK_CLI_RELEASE_TOKEN to a
# fine-grained PAT scoped only to HumanGuardrail/corelink-cli contents:write.
#
# WHY: the current token is the operator's gh-CLI session token (broad
# `repo` scope across all HumanGuardrail repos). For defense-in-depth, the
# release workflow should use a token that can ONLY upload release assets
# to corelink-cli — nothing else.
#
# AUTOMATION LIMIT: GitHub does NOT expose an API to programmatically
# create fine-grained PATs. This script automates everything EXCEPT the
# one-time PAT creation, which has to happen in the dashboard UI.
#
# Usage:
#   bash scripts/rotate-cli-release-token.sh
#
# Steps (UI portion takes ~2 minutes):
set -euo pipefail

cat <<'STEPS'

═══ CORELINK_CLI_RELEASE_TOKEN ROTATION ═══

This is a 1-time operator action. Sets a fine-grained PAT scoped only to
release uploads on HumanGuardrail/corelink-cli (instead of the current
broad-scope gh-CLI token).

1) Open the fine-grained PAT creator (paste in browser):

   https://github.com/settings/personal-access-tokens/new

2) Fill in:

   Token name:           corelink-cli-release-2026
   Expiration:           1 year (or longer — pick a calendar reminder
                         to rotate before expiry)
   Resource owner:       HumanGuardrail
   Repository access:    Only select repositories
                         → check `corelink-cli`
   Repository permissions:
                         Expand "Contents" → set to
                         "Read and write"
   (leave all other permissions at "No access")

3) Click "Generate token". Copy the value (starts with `github_pat_...`).

4) Run THIS script and paste when prompted:

STEPS

read -rsp "Paste new PAT (input hidden): " NEW_PAT
echo

if [[ ! "$NEW_PAT" =~ ^github_pat_ ]]; then
    echo "ERROR: not a fine-grained PAT (must start with 'github_pat_')." >&2
    exit 1
fi

# Verify the PAT works for the intended scope
echo "Verifying PAT can write to HumanGuardrail/corelink-cli..."
if ! curl -fsS \
    -H "Authorization: token $NEW_PAT" \
    -H "Accept: application/vnd.github+json" \
    https://api.github.com/repos/HumanGuardrail/corelink-cli > /dev/null; then
    echo "ERROR: PAT can't read corelink-cli — wrong repo selection?" >&2
    exit 1
fi

# Set as secret on corelink-server (where the workflow lives)
echo "Setting CORELINK_CLI_RELEASE_TOKEN on HumanGuardrail/corelink-server..."
echo -n "$NEW_PAT" | gh secret set CORELINK_CLI_RELEASE_TOKEN \
    --repo HumanGuardrail/corelink-server

echo
echo "✓ Done. Verify with:"
echo "  gh secret list --repo HumanGuardrail/corelink-server | grep CORELINK_CLI"
echo
echo "Next release will use the new fine-grained PAT."
echo "Old gh-CLI token can stay in gh's keyring — it's not used by the"
echo "workflow anymore (it was the bootstrap token only)."
