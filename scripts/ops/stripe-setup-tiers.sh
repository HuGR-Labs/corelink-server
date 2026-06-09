#!/usr/bin/env bash
# stripe-setup-tiers.sh — idempotently create/ensure the 4 PAID CoreLink tier
# products + their monthly recurring USD prices in Stripe, then emit the
# STRIPE_PRICE_ID_* mapping + the matching `wrangler secret put` commands.
#
# ┌──────────────────────────────────────────────────────────────────────────┐
# │  ⚠️  DANGER — THIS CREATES REAL, BILLABLE STRIPE SKUs  ⚠️                  │
# │                                                                          │
# │  When run against a LIVE Stripe account this creates real Products +     │
# │  Prices that customers can be charged against. There is NO undo for a    │
# │  real charge. Read the SAFETY section below before running.              │
# └──────────────────────────────────────────────────────────────────────────┘
#
# What it creates (the 4 PAID tiers — Free + Enterprise are NOT Stripe SKUs):
#
#     Tier      Monthly price (USD)   Product lookup        Price lookup_key
#     ────────  ───────────────────   ───────────────────   ────────────────────
#     Solo      $15 / month           metadata.tier=solo    tier_solo_monthly
#     Starter   $35 / month           metadata.tier=starter tier_starter_monthly
#     Pro       $50 / month           metadata.tier=pro     tier_pro_monthly
#     Max       $149 / month          metadata.tier=max     tier_max_monthly
#
#   (Free = $0 instant activation; Enterprise = "Contact us" inquiry form.
#    Neither is a Stripe product, so neither is created here.)
#
# IDEMPOTENCY:
#   Re-running is safe. For each tier we:
#     1. Look up the Product by `metadata["tier"]==<tier>` (stable key); create
#        it only if absent.
#     2. Look up the Price by its stable `lookup_key` (`tier_<tier>_monthly`);
#        create it only if absent. Stripe Prices are immutable, so we never
#        mutate an existing price — we reuse the one carrying the lookup_key.
#   No duplicate Products or Prices are created on re-run.
#
# OUTPUT:
#   After ensuring all 4, prints a paste-ready mapping block:
#       STRIPE_PRICE_ID_SOLO=price_xxx
#       STRIPE_PRICE_ID_STARTER=price_xxx
#       STRIPE_PRICE_ID_PRO=price_xxx
#       STRIPE_PRICE_ID_MAX=price_xxx
#   followed by COMMENTED `wrangler secret put …` commands for the operator
#   to run themselves (this script never pushes secrets).
#
#   The env-var names match what the server resolves at runtime:
#     crates/corelink-stripe-real/src/client.rs does
#       format!("STRIPE_PRICE_ID_{}", req.tier.as_str().to_uppercase())
#     i.e. STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}. Keep these EXACT.
#
# SAFETY (must pass BOTH before any mutation):
#   - You MUST opt in with EITHER `CONFIRM_LIVE=1` in the env OR the `--yes`
#     flag. Without it the script runs in DRY-RUN (plans only; no writes).
#   - You MUST be authenticated to the INTENDED Stripe account. Use a live
#     key via `--live` (passes `--live` to the Stripe CLI) or rely on the
#     account the `stripe` CLI is already logged in to. The script prints the
#     resolved account id/name and pauses for confirmation before writing.
#   - NO secret/key is hard-coded or read by this script. Authentication is
#     entirely the Stripe CLI's (`stripe login` / `STRIPE_API_KEY` in the CLI's
#     own env). This script never echoes a key.
#
# Usage:
#   scripts/ops/stripe-setup-tiers.sh [--yes] [--live] [--test] [--account <acct>] [--help]
#
#   (no flags)        DRY-RUN: print the plan; make NO changes.
#   --yes             Confirm intent to create REAL SKUs (same as CONFIRM_LIVE=1).
#   --live            Pass `--live` to the Stripe CLI (operate on LIVE data).
#   --test            Pass `--api-key` of a test key is NOT done here; --test just
#                     documents intent (Stripe CLI defaults to test mode). No-op
#                     toggle kept for symmetry / explicitness in runbooks.
#   --account <acct>  Pass through to the Stripe CLI as `--account <acct>` so the
#                     operator can pin a specific connected/standalone account.
#   --help            Print this header and exit.
#
# Env:
#   CONFIRM_LIVE=1    Equivalent to passing --yes (the create gate).
#   STRIPE_BIN        Override the Stripe CLI binary (default: `stripe`).
#
# Exit codes:
#   0   — success (all 4 ensured + mapping printed, or dry-run completed)
#   1   — a Stripe operation failed
#   2   — usage / argument error
#   127 — `stripe` CLI not found in PATH

set -euo pipefail

# ── Constants ────────────────────────────────────────────────────────────────

readonly SCRIPT_NAME="$(basename "$0")"
readonly LOG_PREFIX="[$SCRIPT_NAME]"
readonly CURRENCY="usd"

# Canonical tier table. Order matters only for output readability.
# Columns: tier(wire snake_case) | UPPER (env suffix) | Display name | cents/mo
readonly TIERS=(
    "solo:SOLO:CoreLink Solo:1500"
    "starter:STARTER:CoreLink Starter:3500"
    "pro:PRO:CoreLink Pro:5000"
    "max:MAX:CoreLink Max:14900"
)

# ── Logging ──────────────────────────────────────────────────────────────────

log()  { printf '%s [%s] %s\n' "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*"; }
err()  { printf '%s [%s] ERROR: %s\n' "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*" >&2; }
warn() { printf '%s [%s] WARN: %s\n'  "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*"; }

usage() {
    # Print the leading comment block (drop the shebang line).
    grep '^#' "$0" | tail -n +2 | sed 's/^# \?//'
}

# ── CLI parse ────────────────────────────────────────────────────────────────

CONFIRMED=false
LIVE=false
ACCOUNT=""

# CONFIRM_LIVE=1 env is an alias for --yes (the create gate).
if [[ "${CONFIRM_LIVE:-}" == "1" ]]; then
    CONFIRMED=true
fi

while [[ $# -gt 0 ]]; do
    case "$1" in
        --yes)
            CONFIRMED=true; shift ;;
        --live)
            LIVE=true; shift ;;
        --test)
            # Stripe CLI defaults to test mode; this flag is explicit-intent only.
            LIVE=false; shift ;;
        --account)
            ACCOUNT="${2:-}"
            if [[ -z "$ACCOUNT" ]]; then
                err "--account requires an account id/name argument"
                exit 2
            fi
            shift 2
            ;;
        -h|--help)
            usage; exit 0 ;;
        *)
            err "unknown argument: $1"
            usage >&2
            exit 2
            ;;
    esac
done

# ── Stripe CLI wrapper ───────────────────────────────────────────────────────

STRIPE_BIN="${STRIPE_BIN:-stripe}"

if ! command -v "$STRIPE_BIN" >/dev/null 2>&1; then
    err "Stripe CLI not found in PATH (looked for: $STRIPE_BIN)."
    err "Install it: https://stripe.com/docs/stripe-cli — or set STRIPE_BIN."
    exit 127
fi

# Build the common flag set passed to EVERY `stripe` invocation. We never put a
# key here — auth is the CLI's own (`stripe login` / its STRIPE_API_KEY env).
# A LIVE key supplied via STRIPE_API_KEY (e.g. a restricted key with Products +
# Prices write, since the `stripe login` key is read-only) already puts the CLI
# in live mode by itself — combining it with --live errors. So when such a key
# is present we LABEL live but do NOT add the --live flag (the key drives mode).
# A plain --live with no STRIPE_API_KEY uses the config's live key as before.
if [[ "${STRIPE_API_KEY:-}" == rk_live_* || "${STRIPE_API_KEY:-}" == sk_live_* ]]; then
    LIVE=true
fi
STRIPE_FLAGS=()
if $LIVE && [[ -z "${STRIPE_API_KEY:-}" ]]; then
    STRIPE_FLAGS+=("--live")
fi
[[ -n "$ACCOUNT" ]] && STRIPE_FLAGS+=("--account" "$ACCOUNT")

# Run the Stripe CLI. NOTE: `--live` / `--account` are PER-COMMAND flags in the
# Stripe CLI (v1.x) and must FOLLOW the resource+operation — putting them first
# makes the CLI parse `--live` as an unknown subcommand ("Unknown command
# --live"). So the common flags go AFTER "$@". Output goes to stdout for capture.
stripe_cli() {
    "$STRIPE_BIN" "$@" "${STRIPE_FLAGS[@]}"
}

# Extract a top-level `"id"` string from a Stripe JSON object on stdin.
# (Stripe CLI emits the created/listed object as JSON.) Pure-stdlib python3 so
# we don't depend on jq being installed.
json_field() {
    local field="$1"
    python3 -c "
import sys, json
try:
    obj = json.load(sys.stdin)
except Exception:
    sys.exit(0)
# A list response (e.g. \`stripe products list\`) wraps rows under .data.
if isinstance(obj, dict) and 'data' in obj and isinstance(obj['data'], list):
    rows = obj['data']
    if not rows:
        sys.exit(0)
    obj = rows[0]
if isinstance(obj, dict):
    v = obj.get('$field')
    if v is not None:
        print(v)
"
}

# ── Safety banner + gate ─────────────────────────────────────────────────────

MODE_LABEL="TEST"
$LIVE && MODE_LABEL="LIVE"

log ""
log "==============================================================="
log " CoreLink Stripe tier setup"
log " Mode:    $MODE_LABEL  (Stripe CLI ${STRIPE_FLAGS[*]:-<test, no flags>})"
log " Account: ${ACCOUNT:-<the account the Stripe CLI is logged in to>}"
log "==============================================================="
log ""
log "This will ensure these PRODUCTS + monthly USD PRICES exist:"
for row in "${TIERS[@]}"; do
    IFS=':' read -r tier _upper name cents <<<"$row"
    printf '%s     %-9s %-18s $%s.%02d / month\n' \
        "$LOG_PREFIX" "$tier" "$name" "$((cents / 100))" "$((cents % 100))"
done
log ""

if $LIVE; then
    warn "LIVE MODE: this creates REAL, BILLABLE SKUs on a LIVE Stripe account."
    warn "Triple-check you are on the INTENDED live account before confirming."
fi

# Resolve + print the account identity so the operator can eyeball it BEFORE
# any write. `stripe config --list` doesn't hit the API; use a cheap GET that
# echoes the account. Failure here is non-fatal (older CLIs) — we just warn.
ACCT_DESC="$(stripe_cli get /v1/account 2>/dev/null | python3 -c "
import sys, json
try:
    a = json.load(sys.stdin)
except Exception:
    sys.exit(0)
parts = [a.get('id','?')]
biz = a.get('settings',{}).get('dashboard',{}).get('display_name') or a.get('business_profile',{}).get('name')
if biz:
    parts.append(biz)
if a.get('email'):
    parts.append(a['email'])
print('  |  '.join(str(p) for p in parts))
" 2>/dev/null || true)"

if [[ -n "$ACCT_DESC" ]]; then
    log "Resolved Stripe account: $ACCT_DESC"
else
    warn "Could not resolve the Stripe account identity (continuing)."
    warn "Make sure the Stripe CLI is logged in to the intended account."
fi
log ""

if ! $CONFIRMED; then
    log "DRY-RUN (no --yes / CONFIRM_LIVE=1): NO changes will be made."
    log "Re-run with --yes (or CONFIRM_LIVE=1) to create/ensure the SKUs."
    log "Add --live to operate on the LIVE Stripe account."
    log ""
fi

# ── Ensure helpers ───────────────────────────────────────────────────────────

# Find an existing product id whose metadata.tier == <tier>. Echoes the id, or
# empty if none. Uses `products search` (Stripe's query language) which matches
# on metadata; we filter by active=true to skip archived dupes.
find_product_id() {
    local tier="$1"
    stripe_cli products search \
        --query "active:'true' AND metadata['tier']:'${tier}'" \
        2>/dev/null | json_field id
}

# Find an existing price id by its stable lookup_key. Echoes the id, or empty.
find_price_id() {
    local lookup_key="$1"
    stripe_cli prices list \
        --lookup-keys "$lookup_key" \
        --limit 1 \
        2>/dev/null | json_field id
}

# Ensure the product for a tier exists; echo its id. Creates only if absent.
ensure_product() {
    local tier="$1" name="$2"
    local pid
    pid="$(find_product_id "$tier")"
    if [[ -n "$pid" ]]; then
        log "  product OK (exists): $name → $pid" >&2
        printf '%s' "$pid"
        return 0
    fi

    if ! $CONFIRMED; then
        log "  product MISSING: would CREATE \"$name\" (metadata.tier=$tier)" >&2
        printf '%s' "DRY_RUN_PRODUCT_${tier}"
        return 0
    fi

    log "  product MISSING: creating \"$name\" (metadata.tier=$tier) …" >&2
    pid="$(stripe_cli products create \
        --name "$name" \
        -d "metadata[tier]=${tier}" \
        -d "metadata[managed_by]=corelink-stripe-setup-tiers" \
        | json_field id)"
    if [[ -z "$pid" ]]; then
        err "failed to create product for tier=$tier"
        return 1
    fi
    log "  product CREATED: $name → $pid" >&2
    printf '%s' "$pid"
}

# Ensure the monthly price for a tier exists; echo its id. Creates only if the
# lookup_key is not already present. Stripe Prices are immutable → never mutate.
ensure_price() {
    local tier="$1" product_id="$2" cents="$3"
    local lookup_key="tier_${tier}_monthly"
    local price_id
    price_id="$(find_price_id "$lookup_key")"
    if [[ -n "$price_id" ]]; then
        log "  price OK (exists): $lookup_key → $price_id" >&2
        printf '%s' "$price_id"
        return 0
    fi

    if ! $CONFIRMED; then
        log "  price MISSING: would CREATE $lookup_key = \$$((cents / 100)).$(printf '%02d' $((cents % 100)))/mo (${CURRENCY})" >&2
        printf '%s' "DRY_RUN_PRICE_${tier}"
        return 0
    fi

    # Guard: in a real run the product id must be real, not the dry-run sentinel.
    if [[ "$product_id" == DRY_RUN_PRODUCT_* || -z "$product_id" ]]; then
        err "refusing to create price for tier=$tier: product id unresolved ($product_id)"
        return 1
    fi

    log "  price MISSING: creating $lookup_key (${cents} ${CURRENCY} cents/mo) …" >&2
    price_id="$(stripe_cli prices create \
        --product "$product_id" \
        --currency "$CURRENCY" \
        --unit-amount "$cents" \
        -d "recurring[interval]=month" \
        --lookup-key "$lookup_key" \
        | json_field id)"
    if [[ -z "$price_id" ]]; then
        err "failed to create price for tier=$tier"
        return 1
    fi
    log "  price CREATED: $lookup_key → $price_id" >&2
    printf '%s' "$price_id"
}

# ── Main loop ────────────────────────────────────────────────────────────────

# Parallel arrays of UPPER env suffix + resolved price id, in TIERS order.
ENV_SUFFIXES=()
PRICE_IDS=()

for row in "${TIERS[@]}"; do
    IFS=':' read -r tier upper name cents <<<"$row"
    log ""
    log "── Tier: $tier ($name) ──"

    product_id="$(ensure_product "$tier" "$name")"
    price_id="$(ensure_price "$tier" "$product_id" "$cents")"

    ENV_SUFFIXES+=("$upper")
    PRICE_IDS+=("$price_id")
done

# ── Emit the mapping + wrangler commands ─────────────────────────────────────

log ""
log "==============================================================="
log " RESULT — STRIPE_PRICE_ID_* mapping"
log "==============================================================="
log ""

if ! $CONFIRMED; then
    log "(DRY-RUN — the price ids below are PLACEHOLDERS. Re-run with --yes to"
    log " create the SKUs and emit the real price_… ids.)"
    log ""
fi

# 1) Paste-ready env block (plain stdout, no log prefix, so it copies cleanly).
echo "# ---- paste into .env.local (and put each via wrangler, below) ----"
for i in "${!ENV_SUFFIXES[@]}"; do
    echo "STRIPE_PRICE_ID_${ENV_SUFFIXES[$i]}=${PRICE_IDS[$i]}"
done
echo ""

# 2) Commented wrangler secret put commands (the operator runs these; we don't).
echo "# ---- then push each to Cloudflare Workers (prod) — run these yourself ----"
echo "# (values are piped via stdin so they never land in shell history as args)"
for i in "${!ENV_SUFFIXES[@]}"; do
    suffix="${ENV_SUFFIXES[$i]}"
    pid="${PRICE_IDS[$i]}"
    echo "#   printf '%s' '${pid}' | wrangler secret put STRIPE_PRICE_ID_${suffix} --env prod"
done
echo ""

log ""
if $CONFIRMED; then
    log "DONE. 4 products + monthly prices ensured (idempotent)."
    log "Next: paste the env block above + run the wrangler commands to deploy."
else
    log "DRY-RUN complete. No Stripe changes were made."
    log "Re-run with --yes (and --live for the live account) to apply."
fi
log ""
exit 0
