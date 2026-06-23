#!/usr/bin/env bash
# stripe-setup-runners.sh — idempotently create/ensure the 5 CoreLink RUNNERS
# tier Products + Prices in Stripe (the SEPARATE Runners entitlement axis; NOT
# the cache tiers — see scripts/ops/stripe-setup-tiers.sh for those).
#
# Owner-ratified loss-proof ladder (corelink-runners docs/product/pricing.md §2,
# 2026-06-16) on the real ~$0.10/vCPU-h Cloudflare-Containers basis:
#
#     Tier      Monthly (USD)   concurrency   vCPU-h/mo   Price lookup_key
#     Starter   $16             20            100         runner_starter_monthly
#     Pro       $40             40            240         runner_pro_monthly
#     Team      $100            80            600         runner_team_monthly
#     Scale     $200            160           1,200       runner_scale_monthly
#     Max       $400            320           2,400       runner_max_monthly
#
# IDEMPOTENT: a Price is found by its stable `lookup_key`; if present it is
# reused (never mutated — Stripe prices are immutable), so re-runs are safe.
# The concurrency/vCPU-h ladder is ALSO stamped into the Product+Price metadata
# for self-documentation, but the load-bearing price→entitlement mapping lives
# in the container (`RUNNER_PRICE_ENV_TABLE`, main.rs) keyed on the price id.
#
# After running, set the printed ids as the worker secrets:
#   STRIPE_PRICE_ID_RUNNER_{STARTER,PRO,TEAM,SCALE,MAX}
# then cycle the container so the seed handler activates.
#
# Auth: STRIPE_LIVE_SECRET_KEY (an rk_live restricted key with Products+Prices
# write). Live mode by construction (the key is live).
#
# Usage: STRIPE_LIVE_SECRET_KEY=rk_live_… bash scripts/ops/stripe-setup-runners.sh [--apply]
#   (without --apply: DRY-RUN — reports what it would create, mutates nothing.)
set -euo pipefail

readonly LOG_PREFIX="[stripe-setup-runners]"
readonly API="https://api.stripe.com/v1"
APPLY=0
[ "${1:-}" = "--apply" ] && APPLY=1

# tier:Display:cents:concurrency:vcpu_h
readonly RUNNER_TIERS=(
    "starter:Starter:1600:20:100"
    "pro:Pro:4000:40:240"
    "team:Team:10000:80:600"
    "scale:Scale:20000:160:1200"
    "max:Max:40000:320:2400"
)

: "${STRIPE_LIVE_SECRET_KEY:?STRIPE_LIVE_SECRET_KEY (rk_live) required}"
readonly KEY="$STRIPE_LIVE_SECRET_KEY"
log() { printf '%s %s\n' "$LOG_PREFIX" "$*"; }

api() { # METHOD PATH [curl -d args…]
    local method="$1" path="$2"; shift 2
    curl -s -X "$method" "${API}${path}" -u "${KEY}:" "$@"
}

# Find an existing active price id by lookup_key (echoes id or empty).
price_by_lookup() {
    api GET "/prices?limit=1&active=true&lookup_keys[]=$1" \
        | python3 -c 'import sys,json;d=json.load(sys.stdin);print((d.get("data") or [{}])[0].get("id",""))'
}

echo "$LOG_PREFIX mode: $([ "$APPLY" = 1 ] && echo APPLY || echo DRY-RUN)"
declare -a RESULTS=()
for row in "${RUNNER_TIERS[@]}"; do
    IFS=':' read -r tier name cents conc vcpuh <<<"$row"
    lookup="runner_${tier}_monthly"
    existing="$(price_by_lookup "$lookup")"
    if [ -n "$existing" ]; then
        log "Runners $name: price EXISTS ($lookup) → $existing (reuse)"
        RESULTS+=("STRIPE_PRICE_ID_RUNNER_${tier^^}=$existing")
        continue
    fi
    if [ "$APPLY" != 1 ]; then
        log "Runners $name: would CREATE product + price \$$((cents/100)) ($lookup, conc=$conc, vcpu_h=$vcpuh)"
        continue
    fi
    # Create the Product (metadata stamps the entitlement for self-doc).
    pid="$(api POST "/products" \
        -d "name=CoreLink Runners ${name}" \
        -d "metadata[managed_by]=corelink-runners-setup" \
        -d "metadata[tier]=runner_${tier}" \
        -d "metadata[max_concurrency]=${conc}" \
        -d "metadata[max_vcpu_h]=${vcpuh}" \
        | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))')"
    [ -z "$pid" ] && { log "ERROR: product create failed for $name"; exit 1; }
    # Create the Price (immutable; lookup_key makes it re-findable).
    prid="$(api POST "/prices" \
        -d "product=${pid}" \
        -d "unit_amount=${cents}" \
        -d "currency=usd" \
        -d "recurring[interval]=month" \
        -d "lookup_key=${lookup}" \
        -d "metadata[max_concurrency]=${conc}" \
        -d "metadata[max_vcpu_h]=${vcpuh}" \
        | python3 -c 'import sys,json;print(json.load(sys.stdin).get("id",""))')"
    [ -z "$prid" ] && { log "ERROR: price create failed for $name"; exit 1; }
    log "Runners $name: CREATED product $pid + price $prid (\$$((cents/100))/mo)"
    RESULTS+=("STRIPE_PRICE_ID_RUNNER_${tier^^}=$prid")
done

echo
echo "$LOG_PREFIX price ids (set as worker secrets):"
printf '  %s\n' "${RESULTS[@]}"
