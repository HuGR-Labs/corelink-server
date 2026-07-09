#!/usr/bin/env bash
# mint-dogfood-pat.sh — mint ONE tenant-scoped CoreLink PAT for a dogfood/E2E proof
# and persist its D1 `pat` row, then print the token plaintext ONCE on stdout.
#
# WHY: corelink-runners' fabricd (FABRIC_AUTH_BACKEND=corelink) validates an
# acquire's `Authorization: Bearer <PAT>` against CoreLink introspect. It consumes
# PATs; it does not mint them. This produces one for the box-backend E2E proof.
#
# HOW: the container's `/_internal/pat/mint` is a PURE function (HMAC over the
# token + Argon2id hash) — it does NOT persist. The caller (normally the
# signup-worker / session-exchange) writes the D1 `pat` row. This script does both,
# EXACTLY mirroring `mintScopedPat` in worker/src/lib/session_exchange.ts.
#
# SECRETS: NONE are hard-coded. Auth is `CORELINK_PAT_MINT_AUTH_KEY` (the dedicated
# `/_internal/pat/mint` gate) from the environment (source .env.local). The D1 write
# uses `wrangler` (CLOUDFLARE_API_TOKEN in .env.local). This script never echoes a
# secret except the FINAL token plaintext (the whole point) — hand that to the
# consumer over a secure channel, it is a live credential.
#
# Usage:
#   set -a; source .env.local; set +a
#   scripts/admin/mint-dogfood-pat.sh \
#     [--tenant <uuid>]        (default: ee30f7ba-fc25-4d71-939e-ebe130b4c6a3 — spawn-worker CLW_TENANT)
#     [--scope cas:rw|read-write|read-only|admin]  (default: cas:rw; read-only now mintable end-to-end)
#     [--ttl-seconds <N>]      (default: 86400 = 24h)
#     [--principal <uuid>]     (default: a fresh random dogfood UUID)
#     [--api-base <url>]       (default: https://corelink-api.humangr.com)
#     [--yes]                  (required to actually mint+write; else DRY-RUN)
#
# Exit: 0 ok (token on stdout) · 1 op failed · 2 args/env · 127 missing tool

set -euo pipefail

TENANT="ee30f7ba-fc25-4d71-939e-ebe130b4c6a3"
SCOPE="cas:rw"
TTL=86400
PRINCIPAL=""
API_BASE="https://corelink-api.humangr.com"
CONFIRMED=false
WRANGLER="${WRANGLER:-worker/node_modules/.bin/wrangler}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tenant) TENANT="${2:?}"; shift 2 ;;
    --scope) SCOPE="${2:?}"; shift 2 ;;
    --ttl-seconds) TTL="${2:?}"; shift 2 ;;
    --principal) PRINCIPAL="${2:?}"; shift 2 ;;
    --api-base) API_BASE="${2:?}"; shift 2 ;;
    --yes) CONFIRMED=true; shift ;;
    -h|--help) grep '^#' "$0" | tail -n +2 | sed 's/^# \?//'; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

command -v python3 >/dev/null || { echo "python3 required" >&2; exit 127; }
command -v curl >/dev/null || { echo "curl required" >&2; exit 127; }
[[ -n "${CORELINK_PAT_MINT_AUTH_KEY:-}" ]] || { echo "ERROR: CORELINK_PAT_MINT_AUTH_KEY unset (source .env.local)" >&2; exit 2; }

# Canonical D1 `pat.scope` label — the CHECK domain is exactly
# ('read-write','read-only','admin') (migration 0037). This is the SINGLE
# source of truth: we send $CANON to BOTH the mint route AND the D1 INSERT, so
# the minted scope bits and the persisted label can never disagree. The mint
# route accepts this canonical set (plus `cas:rw` as a back-compat alias of
# `read-write`), so `cas:rw`→`read-write` is mapped EXPLICITLY here rather than
# relying on the route's alias, and `read-only` mints end-to-end.
case "$SCOPE" in
  cas:rw|read-write) CANON="read-write" ;;
  cas:r|read-only)   CANON="read-only" ;;
  admin)             CANON="admin" ;;
  *) echo "unsupported scope: $SCOPE" >&2; exit 2 ;;
esac
[[ -n "$PRINCIPAL" ]] || PRINCIPAL="$(python3 -c 'import uuid;print(uuid.uuid4())')"
NOW_MS="$(python3 -c 'import time;print(int(time.time()*1000))')"

echo "[mint-dogfood-pat] tenant=$TENANT scope=$SCOPE(->canonical $CANON; minted+persisted) ttl=${TTL}s principal=$PRINCIPAL" >&2
if ! $CONFIRMED; then
  echo "[mint-dogfood-pat] DRY-RUN (no --yes): would POST $API_BASE/_internal/pat/mint then INSERT the pat row. Re-run with --yes." >&2
  exit 0
fi

# 1) mint (pure function — returns token + argon2id hash; does NOT persist)
RESP="$(curl -fsS -X POST "$API_BASE/_internal/pat/mint" \
  -H "x-corelink-internal-auth: $CORELINK_PAT_MINT_AUTH_KEY" \
  -H "content-type: application/json" \
  -d "$(python3 -c "import json,sys;print(json.dumps({'tenant_id':sys.argv[1],'principal_id':sys.argv[2],'scopes':sys.argv[3],'ttl_seconds':int(sys.argv[4])}))" "$TENANT" "$PRINCIPAL" "$CANON" "$TTL")")" \
  || { echo "ERROR: mint call failed (auth? host? key?)" >&2; exit 1; }

# parse the mint response
read -r PAT_ID TOKEN_ID EXPIRES_MS TOKEN HASH < <(python3 - "$RESP" <<'PY'
import json,sys
o=json.loads(sys.argv[1])
print(o["pat_id"], o["token_id"], o.get("expires_ms",0), o["token_plaintext"], o["hash"])
PY
) || { echo "ERROR: unexpected mint response shape: $RESP" >&2; exit 1; }

# 2) persist the D1 pat row — EXACT columns of mintScopedPat's INSERT
#    (shown_once_token := token_id, never the plaintext; shown_once_consumed := 1)
SQL=$(python3 - "$PAT_ID" "$TENANT" "$HASH" "$CANON" "$EXPIRES_MS" "$TOKEN_ID" "$NOW_MS" "$PRINCIPAL" <<'PY'
import sys
pat_id,tenant,h,scope,exp,tok,now,princ=sys.argv[1:9]
def q(s): return "'" + s.replace("'","''") + "'"
print(
 "INSERT INTO pat (pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, "
 "shown_once_token, shown_once_consumed, created_ms, principal_id) VALUES ("
 f"{q(pat_id)}, {q(tenant)}, {q(h)}, {q(scope)}, {int(exp)}, {q(tok)}, "
 f"{q(tok)}, 1, {int(now)}, {q(princ)});"
)
PY
)
"$WRANGLER" d1 execute CONFIG_DB --env prod --remote --command "$SQL" >&2 \
  || { echo "ERROR: D1 pat-row insert failed (token minted but NOT persisted → it will NOT authenticate; revoke by token_id=$TOKEN_ID if needed)" >&2; exit 1; }

echo "[mint-dogfood-pat] ✅ minted + persisted. token_id=$TOKEN_ID pat_id=$PAT_ID expires_ms=$EXPIRES_MS" >&2
echo "[mint-dogfood-pat] PAT (hand to the consumer over a secure channel):" >&2
printf '%s\n' "$TOKEN"
