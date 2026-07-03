#!/usr/bin/env bash
# stripe-reconcile-webhook-events.sh — reconcile the LIVE Stripe webhook
# endpoint's `enabled_events` to the code's single source of truth
# (apps/signup-worker/src/webhooks/handled-stripe-events.json).
#
# WHY THIS EXISTS:
#   The signup-worker is the AUTHORITATIVE billing + DOWNGRADE handler. Its
#   dispatch allowlist (HANDLED_EVENT_TYPES) is generated from that JSON. If the
#   Stripe dashboard endpoint is subscribed to FEWER events than the code handles,
#   a cancel / plan-downgrade / terminal dunning-failure is NEVER delivered → the
#   customer is never downgraded → a churned customer keeps paid access
#   (revenue leak). Hand-clicking the event list in the dashboard is exactly what
#   drifts. This script makes the reconciliation deterministic + idempotent, with
#   the code as the one source of truth.
#
# WHAT IT DOES:
#   1. Reads the canonical `enabled_events` from the JSON.
#   2. Lists the account's webhook endpoints; finds the one whose url matches the
#      signup-worker endpoint (default: the `endpoint_url` in the JSON; override
#      with --url).
#   3. Diffs its live `enabled_events` vs canonical → prints ADD / REMOVE.
#   4. Flags any OTHER endpoint pointing at a *.humangr.com host (duplicate /
#      stray-CLI-listener detection) — REPORT ONLY, never deletes.
#   5. DRY-RUN by default. With --yes (or CONFIRM_LIVE=1) it SETS enabled_events
#      to exactly the canonical list (Stripe replaces the array), then re-reads to
#      verify. Re-running when already in sync is a no-op.
#
# SAFETY (mirrors scripts/ops/stripe-setup-tiers.sh):
#   - DRY-RUN unless you opt in with --yes OR CONFIRM_LIVE=1.
#   - NO key is hard-coded or read by this script. Auth is entirely the Stripe
#     CLI's own (`stripe login` / STRIPE_API_KEY in the CLI's env). A live key
#     (rk_live_/sk_live_) in STRIPE_API_KEY already puts the CLI in live mode; a
#     plain --live uses the config's live key. The script prints the resolved
#     account identity and requires confirmation before any write.
#   - This script only ever mutates ONE field (enabled_events) on ONE endpoint.
#     It never creates, deletes, or touches secrets.
#
# Usage:
#   scripts/ops/stripe-reconcile-webhook-events.sh [--yes] [--live] [--test]
#                                                  [--account <acct>] [--url <url>]
#                                                  [--json <path>] [--help]
#
#   (no flags)        DRY-RUN: print the diff; make NO changes.
#   --yes             Confirm intent to WRITE enabled_events (alias CONFIRM_LIVE=1).
#   --live            Pass `--live` to the Stripe CLI (operate on LIVE data).
#   --test            Explicit test-mode intent (Stripe CLI defaults to test).
#   --account <acct>  Pin a specific Stripe account (passthrough --account).
#   --url <url>       Override the endpoint URL to reconcile (default: the JSON's
#                     endpoint_url).
#   --json <path>     Override the source-of-truth JSON path.
#   --help            Print this header and exit.
#
# Exit codes:
#   0   — success (in sync, dry-run completed, or applied+verified)
#   1   — a Stripe operation failed / endpoint not found / verify mismatch
#   2   — usage / argument error
#   127 — `stripe` CLI or python3 not found in PATH

set -euo pipefail

readonly SCRIPT_NAME="$(basename "$0")"
readonly LOG_PREFIX="[$SCRIPT_NAME]"
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly DEFAULT_JSON="$REPO_ROOT/apps/signup-worker/src/webhooks/handled-stripe-events.json"

log()  { printf '%s [%s] %s\n' "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*"; }
err()  { printf '%s [%s] ERROR: %s\n' "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*" >&2; }
warn() { printf '%s [%s] WARN: %s\n'  "$LOG_PREFIX" "$(date -u +%H:%M:%SZ)" "$*"; }

usage() { grep '^#' "$0" | tail -n +2 | sed 's/^# \?//'; }

# ── CLI parse ────────────────────────────────────────────────────────────────

CONFIRMED=false
LIVE=false
ACCOUNT=""
URL_OVERRIDE=""
JSON_PATH="$DEFAULT_JSON"

[[ "${CONFIRM_LIVE:-}" == "1" ]] && CONFIRMED=true

while [[ $# -gt 0 ]]; do
    case "$1" in
        --yes)     CONFIRMED=true; shift ;;
        --live)    LIVE=true; shift ;;
        --test)    LIVE=false; shift ;;
        --account) ACCOUNT="${2:-}"; [[ -z "$ACCOUNT" ]] && { err "--account requires a value"; exit 2; }; shift 2 ;;
        --url)     URL_OVERRIDE="${2:-}"; [[ -z "$URL_OVERRIDE" ]] && { err "--url requires a value"; exit 2; }; shift 2 ;;
        --json)    JSON_PATH="${2:-}"; [[ -z "$JSON_PATH" ]] && { err "--json requires a value"; exit 2; }; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *)         err "unknown argument: $1"; usage >&2; exit 2 ;;
    esac
done

# ── Tooling ──────────────────────────────────────────────────────────────────

STRIPE_BIN="${STRIPE_BIN:-stripe}"
command -v "$STRIPE_BIN" >/dev/null 2>&1 || { err "Stripe CLI not found (looked for: $STRIPE_BIN). https://stripe.com/docs/stripe-cli"; exit 127; }
command -v python3 >/dev/null 2>&1 || { err "python3 not found in PATH."; exit 127; }
[[ -f "$JSON_PATH" ]] || { err "source-of-truth JSON not found: $JSON_PATH"; exit 1; }

# A live key in STRIPE_API_KEY already drives live mode; don't ALSO pass --live
# (the sibling stripe-setup-tiers.sh documents this CLI quirk).
if [[ "${STRIPE_API_KEY:-}" == rk_live_* || "${STRIPE_API_KEY:-}" == sk_live_* ]]; then
    LIVE=true
fi
STRIPE_FLAGS=()
if $LIVE && [[ -z "${STRIPE_API_KEY:-}" ]]; then
    STRIPE_FLAGS+=("--live")
fi
[[ -n "$ACCOUNT" ]] && STRIPE_FLAGS+=("--account" "$ACCOUNT")

# Per-command flags follow the resource+operation (Stripe CLI v1.x quirk).
stripe_cli() { "$STRIPE_BIN" "$@" "${STRIPE_FLAGS[@]}"; }

# ── Canonical set (from the JSON) ────────────────────────────────────────────

TARGET_URL="$URL_OVERRIDE"
if [[ -z "$TARGET_URL" ]]; then
    TARGET_URL="$(python3 -c "import json,sys; print(json.load(open(sys.argv[1])).get('endpoint_url',''))" "$JSON_PATH")"
fi
[[ -n "$TARGET_URL" ]] || { err "no endpoint URL (JSON has no endpoint_url and --url not given)"; exit 2; }

# Canonical events, newline-separated + sorted.
CANONICAL="$(python3 -c "import json,sys; print('\n'.join(sorted(json.load(open(sys.argv[1]))['enabled_events'])))" "$JSON_PATH")"

# ── Banner + account eyeball ─────────────────────────────────────────────────

MODE_LABEL="TEST"; $LIVE && MODE_LABEL="LIVE"
log ""
log "==============================================================="
log " CoreLink Stripe webhook-event reconcile"
log " Mode:     $MODE_LABEL  (Stripe CLI ${STRIPE_FLAGS[*]:-<test, no flags>})"
log " Endpoint: $TARGET_URL"
log " Source:   $JSON_PATH"
log "==============================================================="
log ""
log "Canonical enabled_events (source of truth):"
while IFS= read -r e; do printf '%s     • %s\n' "$LOG_PREFIX" "$e"; done <<<"$CANONICAL"
log ""

ACCT_DESC="$(stripe_cli get /v1/account 2>/dev/null | python3 -c "
import sys, json
try: a = json.load(sys.stdin)
except Exception: sys.exit(0)
parts = [a.get('id','?')]
biz = a.get('settings',{}).get('dashboard',{}).get('display_name') or a.get('business_profile',{}).get('name')
if biz: parts.append(biz)
if a.get('email'): parts.append(a['email'])
print('  |  '.join(str(p) for p in parts))
" 2>/dev/null || true)"
if [[ -n "$ACCT_DESC" ]]; then
    log "Resolved Stripe account: $ACCT_DESC"
else
    warn "Could not resolve the Stripe account identity — is the CLI logged in?"
fi
$LIVE && warn "LIVE MODE: this mutates enabled_events on a LIVE Stripe endpoint."
log ""

# ── Fetch endpoints ──────────────────────────────────────────────────────────

ENDPOINTS_JSON="$(stripe_cli get /v1/webhook_endpoints -d "limit=100" 2>/dev/null || true)"
if [[ -z "$ENDPOINTS_JSON" ]]; then
    err "failed to list webhook endpoints (auth? mode? network?). Aborting."
    exit 1
fi

# Report ALL humangr.com endpoints + flag duplicates on the SAME url (stray CLI
# listeners). Purely informational — never mutated.
log "Webhook endpoints on this account (humangr.com hosts):"
# The endpoints JSON is passed via env (ENDPOINTS_JSON) so stdin is free for the
# heredoc program (`python3 -` reads its program from stdin).
ENDPOINTS_JSON="$ENDPOINTS_JSON" python3 - "$TARGET_URL" <<'PY'
import os, json, sys
target = sys.argv[1]
data = json.loads(os.environ["ENDPOINTS_JSON"]).get("data", [])
same_url = 0
for ep in data:
    url = ep.get("url", "")
    if "humangr.com" not in url:
        continue
    n = len(ep.get("enabled_events", []))
    status = ep.get("status", "?")
    marker = "  <-- reconcile target" if url == target else ""
    print(f"  [{status:8}] {n:3} events  {url}{marker}")
    if url == target:
        same_url += 1
if same_url > 1:
    print(f"  WARN: {same_url} endpoints share the target URL — a stray `stripe listen` "
          f"CLI listener may be Active on prod. Review + delete the extra in the dashboard.")
PY
log ""

# ── Diff ─────────────────────────────────────────────────────────────────────

# Find the target endpoint id + its current events; compute add/remove.
DIFF_OUT="$(ENDPOINTS_JSON="$ENDPOINTS_JSON" python3 - "$TARGET_URL" "$CANONICAL" <<'PY'
import os, json, sys
target = sys.argv[1]
canonical = sorted(x for x in sys.argv[2].splitlines() if x)
data = json.loads(os.environ["ENDPOINTS_JSON"]).get("data", [])
matches = [ep for ep in data if ep.get("url") == target]
if not matches:
    print("NOTFOUND")
    sys.exit(0)
if len(matches) > 1:
    # Reconcile the one with the MOST events (the real prod endpoint), but warn.
    matches.sort(key=lambda e: len(e.get("enabled_events", [])), reverse=True)
    print("MULTI")
ep = matches[0]
current = set(ep.get("enabled_events", []))
canon = set(canonical)
add = sorted(canon - current)
remove = sorted(current - canon)
print("ID\t" + ep.get("id", ""))
for a in add:    print("ADD\t" + a)
for r in remove: print("REMOVE\t" + r)
PY
)"

if grep -q '^NOTFOUND$' <<<"$DIFF_OUT"; then
    err "no webhook endpoint found with url = $TARGET_URL"
    err "Existing endpoints are listed above. Create it in the dashboard first, or pass --url."
    exit 1
fi
grep -q '^MULTI$' <<<"$DIFF_OUT" && warn "multiple endpoints share the target URL — reconciling the one with the most events."

EP_ID="$(awk -F'\t' '$1=="ID"{print $2}' <<<"$DIFF_OUT")"
mapfile -t TO_ADD    < <(awk -F'\t' '$1=="ADD"{print $2}'    <<<"$DIFF_OUT")
mapfile -t TO_REMOVE < <(awk -F'\t' '$1=="REMOVE"{print $2}' <<<"$DIFF_OUT")

log "Target endpoint: $EP_ID"
if [[ ${#TO_ADD[@]} -eq 0 && ${#TO_REMOVE[@]} -eq 0 ]]; then
    log "✅ Already in sync — enabled_events matches the source of truth. No changes."
    exit 0
fi
log "Drift detected:"
for a in "${TO_ADD[@]}";    do printf '%s     + ADD    %s\n' "$LOG_PREFIX" "$a"; done
for r in "${TO_REMOVE[@]}"; do printf '%s     - REMOVE %s\n' "$LOG_PREFIX" "$r"; done
log ""

# ── Apply (or plan) ──────────────────────────────────────────────────────────

# Build the -d enabled_events[] args for the FULL canonical set (Stripe replaces
# the array on update).
UPDATE_ARGS=()
while IFS= read -r e; do UPDATE_ARGS+=("-d" "enabled_events[]=$e"); done <<<"$CANONICAL"

if ! $CONFIRMED; then
    log "DRY-RUN (no --yes / CONFIRM_LIVE=1): NO changes made."
    log "To apply, run:"
    log "  STRIPE_API_KEY=<rk_live_… restricted key with Webhook Endpoints write> \\"
    log "    $0 --yes${LIVE:+ --live}"
    log "(equivalently the command the script would issue:)"
    printf '%s   stripe post /v1/webhook_endpoints/%s' "$LOG_PREFIX" "$EP_ID"
    for arg in "${UPDATE_ARGS[@]}"; do printf ' %q' "$arg"; done
    printf ' %s\n' "${STRIPE_FLAGS[*]}"
    exit 0
fi

log "Applying: setting enabled_events to the canonical ${#TO_ADD[@]}+ set on $EP_ID …"
if ! stripe_cli post "/v1/webhook_endpoints/$EP_ID" "${UPDATE_ARGS[@]}" >/dev/null; then
    err "update failed."
    exit 1
fi

# Verify by re-reading.
VERIFY="$(stripe_cli get "/v1/webhook_endpoints/$EP_ID" 2>/dev/null | python3 -c "
import json, sys
canon = sorted(x for x in sys.argv[1].splitlines() if x)
ep = json.load(sys.stdin)
cur = sorted(ep.get('enabled_events', []))
print('OK' if cur == canon else 'MISMATCH: ' + ','.join(cur))
" "$CANONICAL" 2>/dev/null || echo "VERIFY_ERROR")"

if [[ "$VERIFY" == "OK" ]]; then
    log "✅ DONE — endpoint $EP_ID enabled_events now matches the source of truth."
    exit 0
fi
err "post-update verify did not match: $VERIFY"
exit 1
