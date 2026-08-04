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
#   3. POLLS the CF Containers **per-application** API
#      (GET /containers/applications/{id}) until that application reports
#      `configuration.image` == the pinned ref AND a healthy instance set
#      (i.e. the rollout actually took). The poll is the source of truth — NOT
#      wrangler's exit code.
#
#      WHY PER-APPLICATION AND NOT THE LIST (2026-08-04 false-failure incident)
#      ----------------------------------------------------------------------
#      This loop used to poll the LIST endpoint (GET /containers/applications)
#      and pick our application out of `result[]`. That list serves a STALE
#      snapshot: on 2026-08-04, for corelink-prod-nrt-corelinkserver-prod-nrt
#      (id a033417c-…), the list reported version 100 / image f7bb0961-r1 with
#      `updated_at` frozen at 15:27:12Z, while the per-application GET on the
#      same id reported version 101 / image 794ed958-r1 / 7 healthy instances /
#      `updated_at` 15:28:46Z — and the list was STILL frozen 17 minutes later,
#      i.e. far beyond this script's whole TIMEOUT_S budget. Five consecutive
#      list reads returned the identical stale record, so this is a stale
#      materialized view, not transient replica lag. It produced two false
#      deploy failures (run 30872385188 env prod, run 30923390611 env prod-nrt):
#      the container had genuinely rolled, the verifier read the stale list,
#      exceeded the budget and exited 1. The per-application GET is
#      authoritative. The list is still used — ONCE — to resolve the
#      application id from its (env-unique) image name; that mapping is
#      immutable, so a stale list cannot corrupt it.
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
#   - WRANGLER — the wrangler CLI. Accepts an executable PATH (default:
#     worker/node_modules/.bin/wrangler), a bare COMMAND NAME on $PATH (what CI
#     sets after `npm install -g wrangler@<pin>`), or a full command line.
#     v3 cannot parse [[containers]] arrays, so v4 is a hard floor. If none of
#     those resolve: FATAL on CI (a prod deploy never network-resolves its
#     toolchain), and locally a loud warning + the PINNED `npx wrangler@<pin>`.
#     The pin lives in scripts/_wrangler-pin.sh — one copy, shared with
#     .github/workflows/cf-deploy-prod.yml.
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
CONFIRM_POLLS=2          # consecutive good reads required before declaring convergence

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
# WRANGLER may be any of three forms:
#   1. a PATH to an executable  (default: worker/node_modules/.bin/wrangler)
#   2. a bare COMMAND NAME on $PATH — this is what CI sets (`WRANGLER: wrangler`,
#      after `npm install -g wrangler@$WRANGLER_PINNED_VERSION`)
#   3. a multi-word command line (e.g. `npx wrangler@4.95.0`), used by operators
#
# BUG FIXED 2026-08-04: this block used to test ONLY `[ ! -x "$WRANGLER" ]` — a
# path test that a bare command name can never satisfy. Form 2 was therefore
# silently discarded and EVERY production deploy fell through to
# `npx wrangler@latest`, resolving an unpinned wrangler from npm at deploy time.
# Measured on the last two prod deploys (runs 30923390611, 30918887614): CI
# installed 4.95.0, the deploy ran 4.118.0 — 23 minor versions past the audited
# pin. The pin had never been in force on this path.
# shellcheck source=scripts/_wrangler-pin.sh
. "$REPO_ROOT/scripts/_wrangler-pin.sh"

WRANGLER="${WRANGLER:-$REPO_ROOT/worker/node_modules/.bin/wrangler}"
WRANGLER_IS_CMDLINE=0
case "$WRANGLER" in *[[:space:]]*) WRANGLER_IS_CMDLINE=1 ;; esac

if [ -x "$WRANGLER" ]; then
    :                                                   # form 1 — executable path
elif command -v "$WRANGLER" >/dev/null 2>&1; then
    WRANGLER="$(command -v "$WRANGLER")"                # form 2 — name on $PATH
elif [ "$WRANGLER_IS_CMDLINE" -eq 1 ]; then
    log "using caller-supplied wrangler command line: $WRANGLER"   # form 3
elif [ -n "${CI:-}${GITHUB_ACTIONS:-}" ]; then
    # Fail-loud on CI. A production deploy must never resolve its toolchain from
    # the network at deploy time: that is exactly the defect above. If the
    # `npm install -g wrangler@$WRANGLER_PINNED_VERSION` step did not take, we
    # stop rather than ship prod with whatever npm's `latest` happens to be.
    die "wrangler not resolvable from WRANGLER='$WRANGLER' (not an executable path, not on \$PATH).
     Refusing to fall back to an unpinned 'npx wrangler' on CI — the pinned install step must have failed.
     Expected: 'npm install -g wrangler@$WRANGLER_PINNED_VERSION' before this script (see .github/workflows/cf-deploy-prod.yml)."
else
    # Local/dev convenience only, and still PINNED — never '@latest'.
    warn "wrangler not found at '$WRANGLER' and not on \$PATH; falling back to 'npx wrangler@$WRANGLER_PINNED_VERSION'."
    warn "  (this downloads wrangler at run time; prefer 'pnpm install --filter worker...' for a local binary)"
    WRANGLER="npx wrangler@$WRANGLER_PINNED_VERSION"
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
log "Wrangler:       $WRANGLER (pin $WRANGLER_PINNED_VERSION, scripts/_wrangler-pin.sh)"
log "Convergence:    poll ${POLL_INTERVAL_S}s, grace ${GRACE_PER_DEPLOY_S}s/deploy, total ${TIMEOUT_S}s, max $MAX_REDEPLOYS deploy(s), $CONFIRM_POLLS confirming read(s)"

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

# ── CF Containers applications API ─────────────────────────────────────────
# Echoes the response body on stdout, or empty string on any transport error.
cf_get() {
    curl -s --max-time 20 \
        -H "Authorization: Bearer $CF_TOKEN" \
        -H "Content-Type: application/json" \
        "${CF_API_BASE}$1" 2>/dev/null || echo ''
}

APP_ID=""   # resolved once from the list endpoint (name → id), then reused

# Resolve the application id for this env from the LIST endpoint into APP_ID.
# The list is only trusted for the name→id mapping (immutable for the life of the
# app), NEVER for rollout state — see the incident note in the header. Runs in
# the CALLER's shell (no command substitution) so the id is resolved once and
# reused. Returns non-zero while the app is not in the list yet (a first-ever
# deploy may need a retry), so a failed resolution is never cached.
ensure_app_id() {
    [ -n "$APP_ID" ] && return 0
    local resp id
    resp="$(cf_get "/accounts/${ACCT}/containers/applications")"
    [ -n "$resp" ] || return 1
    id="$(echo "$resp" | jq -r --arg name "$PINNED_IMAGE_NAME" '
        (.result // [])
        | map(select((.configuration.image // "") | test("/" + $name + ":")))
        | (.[0].id // "")
    ' 2>/dev/null || echo "")"
    [ -n "$id" ] && [ "$id" != "null" ] || return 1
    APP_ID="$id"
    log "  application id: $APP_ID  (resolved from the applications list; polled per-application)"
    return 0
}

# Authoritative per-application read. Echoes TSV:
#   <image ref>\t<healthy>\t<failed>\t<desired instances>\t<version>
# or empty string if the app is unresolved / the API read failed.
app_state() {
    local resp
    [ -n "$APP_ID" ] || { echo ""; return 0; }
    resp="$(cf_get "/accounts/${ACCT}/containers/applications/${APP_ID}")"
    [ -n "$resp" ] || { echo ""; return 0; }
    echo "$resp" | jq -r '
        if ((.success // false) and (.result != null)) then
            [ (.result.configuration.image // ""),
              (.result.health.instances.healthy // 0),
              (.result.health.instances.failed  // 0),
              (.result.instances // 0),
              (.result.version  // 0) ] | @tsv
        else empty end
    ' 2>/dev/null || echo ""
}

# Human-readable snapshot of the last poll, for the failure verdict.
LAST_STATE="<never read>"

# Returns 0 only when the per-application read shows BOTH:
#   - configuration.image == the pinned ref, and
#   - a sane instance set: failed == 0, and healthy > 0 whenever the app wants
#     instances at all (an idle scale-to-zero app legitimately has 0 of both).
# The health check is what stops a rolled-but-crashlooping container from
# passing as a good deploy — image match alone only proves CF accepted the
# config, not that the new image can actually run.
rollout_converged() {
    local state image healthy failed desired version
    ensure_app_id || true
    state="$(app_state)"
    if [ -z "$state" ]; then
        LAST_STATE="<unknown — app '$PINNED_IMAGE_NAME' not found or CF API read failed>"
        log "  running image: $LAST_STATE  (want: $PINNED_REF)"
        return 1
    fi
    IFS=$'\t' read -r image healthy failed desired version <<< "$state"
    # Defensive: never let a non-numeric field make an arithmetic test explode.
    [[ "$healthy" =~ ^[0-9]+$ ]] || healthy=0
    [[ "$failed"  =~ ^[0-9]+$ ]] || failed=0
    [[ "$desired" =~ ^[0-9]+$ ]] || desired=0
    LAST_STATE="image=${image:-<none>} version=${version} instances=${desired} healthy=${healthy} failed=${failed}"

    if [ "$image" != "$PINNED_REF" ]; then
        log "  running image: ${image:-<none>}  (want: $PINNED_REF)  [app $APP_ID v${version}]"
        return 1
    fi
    if [ "$failed" -ne 0 ]; then
        log "  image matches the pin but $failed instance(s) FAILED — not converged  [$LAST_STATE]"
        return 1
    fi
    if [ "$desired" -gt 0 ] && [ "$healthy" -lt 1 ]; then
        log "  image matches the pin but 0/${desired} instances healthy — not converged  [$LAST_STATE]"
        return 1
    fi
    return 0
}

# ── Dry-run ────────────────────────────────────────────────────────────────
# Deploys nothing. When creds are available it still exercises the READ path
# (list → id, then the per-application GET) and reports the state it observes,
# so the verifier can be checked against the live API without touching prod.
# Always exits 0: "not converged" here means "a deploy would be needed", not a
# failure — the fail-loud verdict belongs to --apply.
if [ "$APPLY" -eq 0 ]; then
    log ""
    log "DRY-RUN — would execute:"
    log "  $WRANGLER deploy --env $ENV_NAME"
    log "  then poll ${CF_API_BASE}/accounts/<acct>/containers/applications/<app-id>"
    log "  (per-application GET — the applications LIST is stale-prone and is used"
    log "   only once, to resolve <app-id> from the '$PINNED_IMAGE_NAME' image name)"
    log "  until configuration.image == $PINNED_REF with failed==0 and a healthy"
    log "  instance set, on $CONFIRM_POLLS consecutive reads,"
    log "  redeploying up to $MAX_REDEPLOYS time(s) if the rollout hasn't taken."
    log ""
    if [ -n "$CF_TOKEN" ] && [ -n "$ACCT" ] && command -v jq >/dev/null 2>&1 && command -v curl >/dev/null 2>&1; then
        log "Read-only probe of the live convergence check (no deploy):"
        if rollout_converged; then
            log "  CONVERGED already — running state: $LAST_STATE"
        else
            log "  NOT CONVERGED — running state: $LAST_STATE"
        fi
    else
        log "(no CF creds / jq / curl in this environment — skipping the read-only probe)"
    fi
    log ""
    log "Run with --apply to actually deploy + verify."
    exit 0
fi

# ── Preconditions for apply ────────────────────────────────────────────────
# Print the wrangler version that will ACTUALLY deploy. Before 2026-08-04 the
# deploy log never showed it, which is why a silent `npx wrangler@latest`
# fallback survived undetected across every prod deploy. Probed only on --apply
# so a dry-run never triggers an npx download. Report-only: a mismatch is loud
# but not fatal (a `--version` output-format change must not fail a prod deploy).
# shellcheck disable=SC2086  # $WRANGLER may be a multi-word command line (form 3)
WRANGLER_ACTUAL_VERSION="$($WRANGLER --version 2>/dev/null | tr -d '\r' | grep -Eo '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)"
if [ -z "$WRANGLER_ACTUAL_VERSION" ]; then
    warn "could not read a version from '$WRANGLER --version' (deploy continues; pin is $WRANGLER_PINNED_VERSION)."
elif [ "$WRANGLER_ACTUAL_VERSION" != "$WRANGLER_PINNED_VERSION" ]; then
    warn "wrangler $WRANGLER_ACTUAL_VERSION is NOT the audited pin $WRANGLER_PINNED_VERSION (scripts/_wrangler-pin.sh)."
else
    log "Wrangler version: $WRANGLER_ACTUAL_VERSION (== pin)"
fi

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
    good_reads=0   # consecutive converged reads (the settle rule)

    # Poll until convergence, the per-deploy grace window, or the total budget.
    while true; do
        now="$(date +%s)"
        total_elapsed=$(( now - START_TS ))
        deploy_elapsed=$(( now - deploy_start ))

        if rollout_converged; then
            good_reads=$(( good_reads + 1 ))
            if [ "$good_reads" -ge "$CONFIRM_POLLS" ]; then
                converged=1
                log ""
                log "ROLLOUT CONVERGED after $deploys deploy(s), ${total_elapsed}s total"
                log "  ($good_reads consecutive confirming reads of the per-application API)."
                log "  Running state: $LAST_STATE"
                break
            fi
            # Settle rule: one good read is not proof. Re-read before declaring
            # success, so a single inconsistent read (or a mid-roll snapshot)
            # cannot green-light a deploy.
            log "  converged read $good_reads/$CONFIRM_POLLS — re-confirming in ${POLL_INTERVAL_S}s  [$LAST_STATE]"
        else
            good_reads=0
        fi

        if [ "$total_elapsed" -ge "$TIMEOUT_S" ]; then
            break
        fi
        # Mid-confirmation (we already have a good read) → never redeploy on the
        # grace window; just finish confirming within the total budget.
        if [ "$good_reads" -eq 0 ] && [ "$deploy_elapsed" -ge "$GRACE_PER_DEPLOY_S" ]; then
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
    log "  App id: ${APP_ID:-<unresolved>}"
    log "  State:  $LAST_STATE"
    log "  Deploys used: $deploys"
    log "============================================================"
    exit 0
else
    # Re-read once so the verdict prints the freshest authoritative state.
    rollout_converged >/dev/null 2>&1 || true
    warn "ROLLOUT DID NOT CONVERGE within ${TIMEOUT_S}s / $deploys deploy(s)."
    warn "  Wanted:  $PINNED_REF"
    warn "  Running: $LAST_STATE"
    warn "  App id:  ${APP_ID:-<unresolved>}  (read via GET /containers/applications/<id>)"
    warn "  The running container may still be on the OLD image. Investigate:"
    warn "    - Was the new tag actually pushed? (scripts/push-container-multiregion.sh)"
    warn "    - Does wrangler.toml pin match the pushed tag?"
    warn "    - Check the authoritative per-application state by hand:"
    warn "        curl -H \"Authorization: Bearer \$CLOUDFLARE_API_TOKEN\" \\"
    warn "          \"${CF_API_BASE}/accounts/\${CLOUDFLARE_ACCOUNT_ID}/containers/applications/${APP_ID:-<id>}\" | jq .result"
    warn "      (the applications LIST endpoint is stale-prone — do not trust it here)"
    warn "    - Check CF dashboard → Containers → applications rollout status."
    log "============================================================"
    exit 1
fi
