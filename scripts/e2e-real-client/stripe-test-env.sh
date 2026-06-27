#!/usr/bin/env bash
# stripe-test-env.sh — point the e2e billing WEBHOOK journeys at a LOCAL
# wrangler-dev signup-worker, so they NEVER fire synthetic Stripe events at prod.
#
# WHY THIS EXISTS:
#   tests/e2e-user-journeys/src/journeys/billing.rs has two opt-in journeys that
#   POST a SIGNED test Stripe event to the signup-worker's /webhooks/stripe and
#   observe the entitlement move:
#     - "signed Stripe webhook -> tier upgrade reflected"     (customer.subscription.updated, active)
#     - "signed cancel webhook -> entitlement revoked"        (customer.subscription.deleted)
#   They GATE unless ALL of these are present:
#     CORELINK_E2E_STRIPE_WEBHOOK_TEST=1
#     CORELINK_E2E_SIGNUP_WORKER_ENDPOINT   (the receiver base URL)
#     CORELINK_E2E_STRIPE_WEBHOOK_SECRET    (the whsec_… that signs the event)
#     CORELINK_E2E_STRIPE_SUBSCRIPTION_ID   (keys tenant_billing)
#     CORELINK_E2E_STRIPE_CUSTOMER_ID       (keys tier_selections)
#   Pointing CORELINK_E2E_SIGNUP_WORKER_ENDPOINT at PROD would fire synthetic
#   events at the live receiver — forbidden. This script instead reuses the #496
#   harness (scripts/e2e-stripe-webhook-local.sh) to boot a LOCAL wrangler-dev
#   signup-worker (real migration DDL, real index.ts routing, a self-signed
#   whsec) and emits env that points those two journeys at 127.0.0.1.
#
# REUSE OF #496: this script does NOT re-implement the boot/schema/whsec. It
# calls `e2e-stripe-webhook-local.sh --setup` (a sourceable mode added to that
# file) which applies the SAME real-DDL schema and boots the SAME dev server,
# then prints the runtime handles (PORT / WHSEC / PERSIST / WRANGLER / WORKER_DIR
# / STATE_FILE). We add ONE thing the journeys need that the 15/15 scenarios do
# not: a billing+entitlement row keyed by a freshly-minted sub_e2e_/cus_e2e_ id
# pair, so the upgrade/cancel webhooks the journeys POST key correctly.
#
# SCOPE: this un-gates the webhook DELIVERY (the SIGNED POST now lands on a LOCAL
# receiver, correctly processed + persisted in the local D1). The journeys'
# post-delivery verification still reads the API/data-plane the OTHER e2e env
# vars point at (GET /v1/customer/billing for the upgrade; a CAS write for the
# cancel) — so a fully-green run also needs those pointed at a tenant consistent
# with the seeded ids. That data-plane wiring is out of this WP's scope.
#
# Does NOT touch the Clerk-session journeys (authed checkout-session creation /
# tier-select): those need a real Clerk session (W1), not a signed webhook.
#
# Usage:
#   eval "$(bash scripts/e2e-real-client/stripe-test-env.sh up)"   # boot + export env
#   …run the billing journeys…
#   bash scripts/e2e-real-client/stripe-test-env.sh down           # stop the harness
#
# Env knobs:
#   PORT                          local dev port (default 8799; must match for `down`)
#   CORELINK_E2E_TENANT           seed tenant_id (default harness-tenant-e2e)
#   CORELINK_E2E_STRIPE_ENV_FILE  where the exports are persisted (for re-source)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HARNESS="$REPO_ROOT/scripts/e2e-stripe-webhook-local.sh"
PORT="${PORT:-8799}"
TENANT="${CORELINK_E2E_TENANT:-harness-tenant-e2e}"
ENV_FILE="${CORELINK_E2E_STRIPE_ENV_FILE:-${TMPDIR:-/tmp}/corelink-e2e-stripe-env.${PORT}.sh}"

CMD="${1:-up}"

usage() {
  echo "usage: $0 [up|down]" >&2
  echo "  up   — boot/reuse the local signup-worker (#496) and emit e2e env on STDOUT" >&2
  echo "  down — stop the harness started by 'up' (same PORT)" >&2
}

[[ -f "$HARNESS" ]] || { echo "ERROR: harness not found at $HARNESS" >&2; exit 1; }

case "$CMD" in
  up)
    # 1. Boot/reuse the #496 harness in --setup mode and capture its emitted
    #    runtime handles (its progress goes to STDERR; only KEY=VALUE on STDOUT).
    out="$(PORT="$PORT" bash "$HARNESS" --setup)"
    # Defines HARNESS_PORT / HARNESS_WHSEC / HARNESS_PERSIST / HARNESS_WRANGLER /
    # HARNESS_WORKER_DIR / HARNESS_STATE_FILE.
    eval "$out"

    # 2. Mint a test sub/cus id pair and pre-seed a billing + entitlement row
    #    keyed by them, so the signed upgrade (customer.subscription.updated) and
    #    cancel (customer.subscription.deleted) the journeys POST find their row.
    #    customer.subscription.updated keys tenant_billing by subscription_id and
    #    re-activates tier_selections via that row; cancel keys by subscription_id
    #    too — so the row must exist and be non-canceled before the events land.
    SUB="sub_e2e_$(date +%s)${RANDOM}"
    CUS="cus_e2e_$(date +%s)${RANDOM}"
    NOW="$(node -e 'console.log(Date.now())')"

    node "$HARNESS_WRANGLER" d1 execute CONFIG_DB --local \
      --persist-to "$HARNESS_PERSIST" --config "$HARNESS_WORKER_DIR/wrangler.toml" \
      --yes --command \
      "INSERT INTO tenant_billing
         (tenant_id, stripe_customer_id, stripe_subscription_id,
          status, plan, schema_version, created_at_ms, updated_at_ms)
       VALUES ('$TENANT','$CUS','$SUB','paid','solo',1,$NOW,$NOW);" >&2

    node "$HARNESS_WRANGLER" d1 execute CONFIG_DB --local \
      --persist-to "$HARNESS_PERSIST" --config "$HARNESS_WORKER_DIR/wrangler.toml" \
      --yes --command \
      "INSERT INTO tier_selections
         (tenant_id, tier, subscription_state, stripe_customer_id,
          subscription_started_at_ms, schema_version, correlation_id)
       VALUES ('$TENANT','solo','active','$CUS',$NOW,1,'seed:e2e');" >&2

    echo "▶ seeded local D1: tenant=$TENANT sub=$SUB cus=$CUS" >&2

    # 3. Emit the e2e env (eval-able on STDOUT) and persist it for re-sourcing.
    {
      echo "export CORELINK_E2E_STRIPE_WEBHOOK_TEST=1"
      echo "export CORELINK_E2E_SIGNUP_WORKER_ENDPOINT=http://127.0.0.1:$HARNESS_PORT"
      echo "export CORELINK_E2E_STRIPE_WEBHOOK_SECRET=$HARNESS_WHSEC"
      echo "export CORELINK_E2E_STRIPE_SUBSCRIPTION_ID=$SUB"
      echo "export CORELINK_E2E_STRIPE_CUSTOMER_ID=$CUS"
    } | tee "$ENV_FILE"
    echo "▶ env persisted → $ENV_FILE   (stop with: $0 down)" >&2
    ;;

  down)
    PORT="$PORT" bash "$HARNESS" --teardown
    rm -f "$ENV_FILE" 2>/dev/null || true
    ;;

  -h|--help|help)
    usage
    ;;

  *)
    usage
    exit 2
    ;;
esac
