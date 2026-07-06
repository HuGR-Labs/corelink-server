#!/usr/bin/env bash
# scripts/deploy-container-prod.sh — robust container deploy with rollout verify.
#
# WHY THIS EXISTS (problem #2 — "the first deploy doesn't roll the container")
# ---------------------------------------------------------------------------
# A plain `wrangler deploy --env <env>` after repinning the
# [[env.<env>.containers]] image to a new tag REPORTS SUCCESS and prints the
# image diff, but — observed repeatedly on the CF Containers beta — the RUNNING
# Firecracker instances frequently stay on the OLD image until a SECOND
# `wrangler deploy`. The Worker/DO code rolls on the first deploy; the container
# application's rollout to live instances lags and a single deploy returns before
# the new image is actually serving. The painful manual workaround was "just run
# wrangler deploy twice." That is exactly the kind of silent-stale-binary risk
# the build-side fix (Dockerfile first-party clean) also targets: a fix you
# pushed can sit un-deployed while the deploy says "done."
#
# WHAT THIS SCRIPT DOES (the fix)
# ---------------------------------------------------------------------------
#   1. Reads the PINNED image tag from wrangler.toml for the requested env
#      ([[env.<env>.containers]] image = "registry.cloudflare.com/.../<name>:<tag>").
#   2. Runs `wrangler deploy --env <env>`.
#   3. POLLS the CF Containers applications API until the application whose image
#      name matches this env reports `configuration.image` == the pinned ref
#      (i.e. the rollout actually took). The poll is the source of truth — NOT
#      wrangler's exit code.
#   4. If the rollout has not converged within the per-deploy grace window, it
#      re-runs `wrangler deploy` (the documented "second deploy" workaround) and
#      re-polls. Bounded number of redeploys; bounded total time.
#   5. Exits 0 ONLY when the running application image matches the pin; non-zero
#      (loud) otherwise — so CI / the operator can't believe a stale deploy.
#
# This converts the flaky "deploy, hope, maybe deploy again" dance into a
# deterministic verify-and-retry with an authoritative convergence check.
#
# Auth / config:
#   - CLOUDFLARE_API_TOKEN (or CF_API_TOKEN) — CF API auth (CTRL-CRED-001:
#     read from env / .env.local, never embedded here).
#   - CLOUDFLARE_ACCOUNT_ID — required for the Containers applications API.
#   - WRANGLER — wrangler binary (default: worker/node_modules/.bin/wrangler,
#     the 4.x pin; v3 cannot parse [[containers]] arrays). Falls back to
#     `npx wrangler@latest` if that path is absent.
#
# Usage:
#   bash scripts/deploy-container-prod.sh                 # dry-run, env=prod
#   bash scripts/deploy-container-prod.sh --apply         # deploy + verify prod
#   bash scripts/deploy-container-prod.sh --apply --env prod-sam
#   bash scripts/deploy-container-prod.sh --apply --max-redeploys 3 --timeout 600
#   bash scripts/deploy-container-prod.sh --help
#
# Exit codes:
#   0 → running application image == pinned tag (rollout verified)
#   1 → deploy/verify failed (rollout never converged, or wrangler error)
#   2 → usage / precondition error
#
# Signed-off-by: Gustavo Schneiter <gustavomalleths@gmail.com>
# Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CF_API_BASE="https://api.cloudflare.com/client/v4"

# ── Defaults (overridable via flags) ───────────────────────────────────────
APPLY=0
ENV_NAME="prod"
MAX_REDEPLOYS=2          # initial deploy + up to this many extra deploys
POLL_INTERVAL_S=10       # seconds between rollout polls
TIMEOUT_S=420            # total wall-clock budget for convergence (per script)
GRACE_PER_DEPLOY_S=120   # how long to wait for one deploy to converge before redeploy

log()  { printf '[deploy-container-prod] %s\n' "$*"; }
warn() { printf '[deploy-container-prod] WARN: %s\n' "$*" >&2; }
die()  { printf '[deploy-container-prod] FATAL: %s\n' "$*" >&2; exit 1; }

usage() { sed -n '2,/^$/p' "$0"; exit 2; }

# ── Arg parse ──────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply)          APPLY=1; shift ;;
        --dry-run)        APPLY=0; shift ;;
        --env)            ENV_NAME="${2:?--env needs a value}"; shift 2 ;;
        --max-redeploys)  MAX_REDEPLOYS="${2:?}"; shift 2 ;;
        --timeout)        TIMEOUT_S="${2:?}"; shift 2 ;;
        --poll-interval)  POLL_INTERVAL_S="${2:?}"; shift 2 ;;
        --grace)          GRACE_PER_DEPLOY_S="${2:?}"; shift 2 ;;
        -h|--help)        usage ;;
        *) die "unknown argument: $1 (try --help)" ;;
    esac
done

cd "$REPO_ROOT"

# ── Load .env.local for CF creds (gitignored; absent in CI where vars are set) ─
if [ -f "$REPO_ROOT/.env.local" ]; then
    set -a
    # shellcheck source=/dev/null
    source "$REPO_ROOT/.env.local"
    set +a
fi

# Normalize auth env var names (wrangler + CF API both accept CLOUDFLARE_API_TOKEN).
CF_TOKEN="${CLOUDFLARE_API_TOKEN:-${CF_API_TOKEN:-}}"
ACCT="${CLOUDFLARE_ACCOUNT_ID:-}"

# ── Resolve wrangler ───────────────────────────────────────────────────────
WRANGLER="${WRANGLER:-$REPO_ROOT/worker/node_modules/.bin/wrangler}"
if [ ! -x "$WRANGLER" ]; then
    warn "local wrangler not found at $WRANGLER; falling back to 'npx wrangler@latest'"
    WRANGLER="npx wrangler@latest"
fi

# ── Extract the pinned image ref for this env from wrangler.toml ────────────
# CF Containers auto-names the registry image `corelink-<env>-corelinkserver-prod`
# (see scripts/push-container-multiregion.sh ENVS) — note the trailing `-prod` is
# the DO/class suffix and is the SAME for every env; the env lives in the middle.
# We anchor on the EXACT, env-unique image name so the match is order-independent
# (ENV_NAME=prod must NOT pick up corelink-prod-sam-...). The bare `./Dockerfile`
# image lines (staging/dev blocks) are excluded by the registry.cloudflare.com
# prefix requirement.
EXPECTED_IMG_NAME="corelink-${ENV_NAME}-corelinkserver-prod"
PINNED_REF="$(grep -E '^image = "registry\.cloudflare\.com/[^/]+/'"${EXPECTED_IMG_NAME}"':[^"]+"' wrangler.toml \
    | head -1 | sed -E 's/^image = "([^"]+)".*/\1/')"

[ -n "$PINNED_REF" ] || die "could not find a pinned [[env.$ENV_NAME.containers]] image named '$EXPECTED_IMG_NAME' in wrangler.toml.
     Repin the image tag first (see scripts/push-container-multiregion.sh output)."

PINNED_IMAGE_NAME="$(echo "$PINNED_REF" | sed -E 's|.*/([^/:]+):[^:]+$|\1|')"
PINNED_TAG="$(echo "$PINNED_REF" | sed -E 's|.*:([^:]+)$|\1|')"

log "Env:            $ENV_NAME"
log "Pinned image:   $PINNED_REF"
log "  image name:   $PINNED_IMAGE_NAME"
log "  image tag:    $PINNED_TAG"
log "Wrangler:       $WRANGLER"
log "Convergence:    poll ${POLL_INTERVAL_S}s, grace ${GRACE_PER_DEPLOY_S}s/deploy, total ${TIMEOUT_S}s, max $MAX_REDEPLOYS deploy(s)"

# ── Root-cause guard (2026-07-05 stale-pin incident) ──────────────────────────
# A stale pin "converges" happily (running == pinned == stale) and silently ships
# a container BEHIND main — that is exactly how the container sat at c1337115
# (#594) for 109 commits. FAIL before deploying if the pinned SHA is behind
# current container code. Override only for a deliberate rollback to an old image.
if [ "${SKIP_PIN_FRESHNESS:-0}" != "1" ]; then
    log "Container-pin freshness gate (set SKIP_PIN_FRESHNESS=1 to override for a rollback)…"
    if ! bash "$(dirname "$0")/check-container-pin-fresh.sh"; then
        die "container pin is STALE (see error above). Rebuild via container-build-push-prod.yml + repin wrangler.toml to HEAD before deploying."
    fi
fi

# ── Query the running application image via the CF Containers applications API ─
# GET /accounts/{acct}/containers/applications → result[] each has a
# configuration.image. We match the application whose image NAME component equals
# the pinned image name (env-unique), then return its current image ref.
# Echoes the running ref on stdout, or empty string if not found / API error.
running_image_ref() {
    local resp
    resp="$(curl -s --max-time 20 \
        -H "Authorization: Bearer $CF_TOKEN" \
        -H "Content-Type: application/json" \
        "${CF_API_BASE}/accounts/${ACCT}/containers/applications" 2>/dev/null || echo '')"
    [ -n "$resp" ] || { echo ""; return 0; }
    echo "$resp" | jq -r --arg name "$PINNED_IMAGE_NAME" '
        (.result // [])
        | map(select((.configuration.image // "") | test("/" + $name + ":")))
        | (.[0].configuration.image // "")
    ' 2>/dev/null || echo ""
}

# Returns 0 if the running application image == pinned ref.
rollout_converged() {
    local running
    running="$(running_image_ref)"
    if [ "$running" = "$PINNED_REF" ]; then
        return 0
    fi
    log "  running image: ${running:-<unknown / app not found>}  (want: $PINNED_REF)"
    return 1
}

# ── Dry-run ────────────────────────────────────────────────────────────────
if [ "$APPLY" -eq 0 ]; then
    log ""
    log "DRY-RUN — would execute:"
    log "  $WRANGLER deploy --env $ENV_NAME"
    log "  then poll ${CF_API_BASE}/accounts/<acct>/containers/applications"
    log "  until the '$PINNED_IMAGE_NAME' application's configuration.image == $PINNED_REF,"
    log "  redeploying up to $MAX_REDEPLOYS time(s) if the rollout hasn't taken."
    log ""
    log "Run with --apply to actually deploy + verify."
    exit 0
fi

# ── Preconditions for apply ────────────────────────────────────────────────
[ -n "$CF_TOKEN" ] || die "CLOUDFLARE_API_TOKEN / CF_API_TOKEN not set (needed for the rollout-verify API)."
[ -n "$ACCT" ]     || die "CLOUDFLARE_ACCOUNT_ID not set (needed for the Containers applications API)."
command -v jq   >/dev/null 2>&1 || die "jq not found (required to parse the CF API response)."
command -v curl >/dev/null 2>&1 || die "curl not found."

# ── Deploy + verify-and-retry loop ─────────────────────────────────────────
START_TS="$(date +%s)"
deploys=0
converged=0

deploy_once() {
    deploys=$(( deploys + 1 ))
    log ""
    log "── Deploy attempt $deploys/$MAX_REDEPLOYS — $WRANGLER deploy --env $ENV_NAME ──"
    if ! $WRANGLER deploy --env "$ENV_NAME"; then
        die "wrangler deploy failed (env $ENV_NAME, attempt $deploys)."
    fi
}

while [ "$deploys" -lt "$MAX_REDEPLOYS" ]; do
    deploy_once
    deploy_start="$(date +%s)"

    # Poll until convergence, the per-deploy grace window, or the total budget.
    while true; do
        now="$(date +%s)"
        total_elapsed=$(( now - START_TS ))
        deploy_elapsed=$(( now - deploy_start ))

        if rollout_converged; then
            converged=1
            log ""
            log "ROLLOUT CONVERGED after $deploys deploy(s), ${total_elapsed}s total."
            log "  Running image now matches the pin: $PINNED_REF"
            break
        fi

        if [ "$total_elapsed" -ge "$TIMEOUT_S" ]; then
            break
        fi
        if [ "$deploy_elapsed" -ge "$GRACE_PER_DEPLOY_S" ]; then
            warn "Deploy $deploys did not converge within ${GRACE_PER_DEPLOY_S}s grace — will redeploy (this is the known CF first-deploy-doesn't-roll behavior)."
            break
        fi
        sleep "$POLL_INTERVAL_S"
    done

    [ "$converged" -eq 1 ] && break

    now="$(date +%s)"
    if [ $(( now - START_TS )) -ge "$TIMEOUT_S" ]; then
        break
    fi
done

# ── Verdict ────────────────────────────────────────────────────────────────
log ""
log "============================================================"
if [ "$converged" -eq 1 ]; then
    log "  DEPLOY + ROLLOUT VERIFIED"
    log "  Env:    $ENV_NAME"
    log "  Image:  $PINNED_REF"
    log "  Deploys used: $deploys"
    log "============================================================"
    exit 0
else
    final="$(running_image_ref)"
    warn "ROLLOUT DID NOT CONVERGE within ${TIMEOUT_S}s / $deploys deploy(s)."
    warn "  Wanted:  $PINNED_REF"
    warn "  Running: ${final:-<unknown / app not found>}"
    warn "  The running container may still be on the OLD image. Investigate:"
    warn "    - Was the new tag actually pushed? (scripts/push-container-multiregion.sh)"
    warn "    - Does wrangler.toml pin match the pushed tag?"
    warn "    - Check CF dashboard → Containers → applications rollout status."
    log "============================================================"
    exit 1
fi
