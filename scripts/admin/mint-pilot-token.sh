#!/usr/bin/env bash
# mint-pilot-token.sh — Operator-facing CLI to mint pilot signup tokens.
#
# Wave-29 stream-1 deliverable #3, closes DEBT-027 engineering-side.
#
# Mints N pilot tokens for `https://corelink-signup.humangr.com/pilot/<token>`
# redemption. Tokens are signed with the `SIGNUP_TOKEN_KEY` HMAC-SHA256
# secret consumed by the production backend
# (`apps/server/src/routes/signup.rs`).
#
# Token format:
#
#   pilot_<env>_<unix_ms>_<16-hex-random>.<hmac-hex>
#
# Where:
#   - `env` ∈ {staging, prod}
#   - `unix_ms` = mint timestamp (Unix epoch ms; route TTL = 14 days)
#   - `16-hex-random` = `openssl rand -hex 8` (8 random bytes)
#   - `hmac-hex` = `HMAC-SHA256(SIGNUP_TOKEN_KEY,
#                              "pilot_<env>_<unix_ms>_<16-hex>")`,
#                 hex-encoded
#
# The route verifies in constant time via `subtle::ConstantTimeEq`.
#
# Usage:
#   SIGNUP_TOKEN_KEY=<hex-or-utf8-secret> \
#     ./scripts/admin/mint-pilot-token.sh \
#       --env <staging|prod> \
#       --count <N> \
#       [--ttl-days <D>]              (informational; route TTL fixed at 14d)
#
# Output (one token per line on stdout):
#   pilot_prod_1700000000000_0123456789abcdef.deadbeef...
#   pilot_prod_1700000000001_fedcba9876543210.cafef00d...
#
# These tokens go into the operator's outreach emails (per wave-28
# pilot-comms templates `docs/internal/pilot-comms-templates.md`).
#
# Exit codes:
#   0  success — N tokens emitted on stdout
#   1  invalid arguments
#   2  `SIGNUP_TOKEN_KEY` env var missing
#   3  openssl missing
#
# Charter compliance:
#   - DCO sign-off: Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
#   - synchronous bash only (no background work)
#
# Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

set -euo pipefail

SCRIPT_NAME="$(basename "$0")"
ENV_KIND=""
COUNT=""
TTL_DAYS="14"

usage() {
    cat <<EOF
Usage: $SCRIPT_NAME --env <staging|prod> --count <N> [--ttl-days <D>]

Required env var:
  SIGNUP_TOKEN_KEY    HMAC-SHA256 secret (UTF-8 string; bound to the
                      backend's \`SIGNUP_TOKEN_KEY\` env / Worker secret).

Options:
  --env <kind>        \`staging\` or \`prod\`.
  --count <N>         number of tokens to mint (positive integer).
  --ttl-days <D>      informational only — the route TTL is fixed at
                      14 days. Default: 14.
  -h, --help          show this help.

Example:
  SIGNUP_TOKEN_KEY=\$(cat ~/.corelink/signup-key) \\
    $SCRIPT_NAME --env prod --count 5
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --env)
            ENV_KIND="${2:-}"; shift 2;;
        --count)
            COUNT="${2:-}"; shift 2;;
        --ttl-days)
            TTL_DAYS="${2:-}"; shift 2;;
        -h|--help)
            usage; exit 0;;
        *)
            echo "ERROR: unknown argument: $1" >&2
            usage >&2
            exit 1;;
    esac
done

# Argument validation.
if [[ -z "$ENV_KIND" ]] || [[ -z "$COUNT" ]]; then
    echo "ERROR: --env and --count are required" >&2
    usage >&2
    exit 1
fi
if [[ "$ENV_KIND" != "staging" && "$ENV_KIND" != "prod" ]]; then
    echo "ERROR: --env must be \`staging\` or \`prod\` (got: $ENV_KIND)" >&2
    exit 1
fi
if ! [[ "$COUNT" =~ ^[1-9][0-9]*$ ]]; then
    echo "ERROR: --count must be a positive integer (got: $COUNT)" >&2
    exit 1
fi
if ! [[ "$TTL_DAYS" =~ ^[1-9][0-9]*$ ]]; then
    echo "ERROR: --ttl-days must be a positive integer (got: $TTL_DAYS)" >&2
    exit 1
fi

if [[ -z "${SIGNUP_TOKEN_KEY:-}" ]]; then
    echo "ERROR: SIGNUP_TOKEN_KEY env var is required" >&2
    echo "       Production wiring binds this to the Cloudflare Workers" >&2
    echo "       secret; for local mint, run:" >&2
    echo "         SIGNUP_TOKEN_KEY=\"\$(openssl rand -hex 32)\" $SCRIPT_NAME ..." >&2
    exit 2
fi

if ! command -v openssl >/dev/null 2>&1; then
    echo "ERROR: openssl is required (used for the HMAC + random bytes)" >&2
    exit 3
fi

NOW_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

# Mint loop — one token per iteration. Each token is independent
# (independent 16-hex random body); the timestamp is monotonic per
# token via a per-iteration micro-offset.
for i in $(seq 1 "$COUNT"); do
    rand_hex="$(openssl rand -hex 8)"
    # Add a per-iteration ms offset so each token has a distinct mint
    # timestamp (and therefore a distinct signature even if `rand`
    # collided — defence in depth).
    minted_at_ms="$((NOW_MS + i - 1))"
    body="pilot_${ENV_KIND}_${minted_at_ms}_${rand_hex}"
    # HMAC-SHA256(key, body) → hex.
    sig_hex="$(printf "%s" "$body" \
        | openssl dgst -sha256 -hmac "$SIGNUP_TOKEN_KEY" -hex \
        | awk '{print $NF}')"
    printf "%s.%s\n" "$body" "$sig_hex"
done
