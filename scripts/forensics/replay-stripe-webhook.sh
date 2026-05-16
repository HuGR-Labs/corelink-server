#!/usr/bin/env bash
# replay-stripe-webhook.sh — replay a single Stripe webhook event by id
# against a target environment (staging by default — refuses prod
# without explicit IC sign-off).
#
# Used during incident forensics when a webhook delivery is suspected
# to be stuck (see FORENSICS-GUIDE.md §8.2).
#
# Usage:
#   ./scripts/forensics/replay-stripe-webhook.sh \
#       --event-id evt_1Nf... \
#       --env      staging
#
#   # Prod replay requires explicit override + IC sign-off:
#   ./scripts/forensics/replay-stripe-webhook.sh \
#       --event-id evt_1Nf... \
#       --env      prod \
#       --i-am-ic  "alice@humangr.com" \
#       --confirm-prod
#
# Output: JSON on stdout. Exit 0 on success, non-zero on failure.

set -euo pipefail

EVENT_ID=""
ENV="staging"
IC_EMAIL=""
CONFIRM_PROD="false"

usage() {
    cat <<'EOF'
Usage: replay-stripe-webhook.sh --event-id <evt_...> --env <staging|prod>
                                [--i-am-ic <email>] [--confirm-prod]

Required:
  --event-id      Stripe event id (must start with "evt_").
  --env           Target environment. Default: staging.

Prod-only:
  --i-am-ic       IC email (for audit trail).
  --confirm-prod  Explicit prod confirmation. Without this, prod is refused.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --event-id)     EVENT_ID="${2:-}"; shift 2 ;;
        --env)          ENV="${2:-}"; shift 2 ;;
        --i-am-ic)      IC_EMAIL="${2:-}"; shift 2 ;;
        --confirm-prod) CONFIRM_PROD="true"; shift ;;
        -h|--help)      usage; exit 0 ;;
        *)              echo "unknown arg: $1" >&2; usage >&2; exit 2 ;;
    esac
done

if [[ -z "${EVENT_ID}" ]]; then
    echo "error: --event-id is required" >&2
    usage >&2
    exit 2
fi

if [[ ! "${EVENT_ID}" =~ ^evt_[A-Za-z0-9]+$ ]]; then
    echo "error: --event-id must match 'evt_[A-Za-z0-9]+' (got: ${EVENT_ID})" >&2
    exit 2
fi

case "${ENV}" in
    staging|prod) ;;
    *) echo "error: --env must be 'staging' or 'prod' (got: ${ENV})" >&2; exit 2 ;;
esac

if [[ "${ENV}" == "prod" ]]; then
    if [[ "${CONFIRM_PROD}" != "true" ]] || [[ -z "${IC_EMAIL}" ]]; then
        echo "error: prod replay requires --confirm-prod AND --i-am-ic <email>" >&2
        echo "       refusing to proceed; ask the IC and try again" >&2
        exit 3
    fi
    echo "WARNING: replaying ${EVENT_ID} against PROD under IC ${IC_EMAIL}" >&2
    echo "         this WILL re-dispatch handlers (idempotency catches most mutations" >&2
    echo "         but not all). 5 second hold..." >&2
    sleep 5
fi

# 1. Fetch the raw event from Stripe.
echo "step 1/3: fetching event ${EVENT_ID} from Stripe..." >&2
if ! command -v stripe >/dev/null 2>&1; then
    echo "error: stripe CLI not found on PATH" >&2
    exit 4
fi

EVENT_JSON="$(stripe events retrieve "${EVENT_ID}" 2>/dev/null || true)"
if [[ -z "${EVENT_JSON}" ]]; then
    echo "error: failed to retrieve ${EVENT_ID} from Stripe" >&2
    exit 4
fi

# 2. Check current idempotency state in target env.
echo "step 2/3: checking idempotency state in ${ENV}..." >&2
DB_NAME="corelink-${ENV}"
EXISTS_JSON="$(wrangler d1 execute "${DB_NAME}" --remote --json \
    --command "SELECT event_id, outcome, processed_at_ms
               FROM stripe_webhook_events_processed
               WHERE event_id = '${EVENT_ID}';" 2>/dev/null || echo "[]")"

# 3. Re-POST to the webhook endpoint with the original signature.
echo "step 3/3: dispatching to ${ENV} webhook endpoint..." >&2
WEBHOOK_URL="https://api.${ENV}.corelink.humangr.com/webhook/stripe"
RESPONSE="$(printf '%s' "${EVENT_JSON}" | curl -sS -X POST \
    -H "Content-Type: application/json" \
    -H "Stripe-Signature: t=replay,v1=replay-${EVENT_ID}" \
    --data-binary @- \
    "${WEBHOOK_URL}" -w "\n%{http_code}" || true)"

HTTP_CODE="${RESPONSE##*$'\n'}"
BODY="${RESPONSE%$'\n'*}"

# 4. Emit structured result.
jq -n \
    --arg event_id "${EVENT_ID}" \
    --arg env      "${ENV}" \
    --arg ic       "${IC_EMAIL}" \
    --arg http     "${HTTP_CODE}" \
    --arg body     "${BODY}" \
    --argjson pre  "${EXISTS_JSON}" \
    '{
        event_id:    $event_id,
        env:         $env,
        ic_email:    $ic,
        pre_replay_state: $pre,
        replay_http_code: ($http | tonumber? // -1),
        replay_body:      $body,
        ok:               (($http | tonumber? // -1) >= 200 and ($http | tonumber? // -1) < 300)
    }'

if [[ "${HTTP_CODE}" =~ ^2[0-9][0-9]$ ]]; then
    exit 0
else
    exit 5
fi
