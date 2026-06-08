#!/usr/bin/env bash
# backfill-admin-scope-prod.sh — demote legacy `admin`-scoped prod PATs to the
# least-privilege cache scope `read-write`.
#
# WHY: early prod PATs were minted with scope='admin' (a superset that grants
# cache rw AND would match any future admin-route gate). Self-serve customer
# tokens only need cache read+write. The provisioning path now mints
# `read-write` (the value allowed by the live `pat.scope` CHECK); this back-fill
# brings the 25 legacy `admin` rows in line for least privilege.
#
# NO MIGRATION NEEDED: `read-write` is already a CHECK-valid value
# (migration 0037: scope IN ('read-write','read-only','admin')), and the code
# (`scope.rs` requires_cache_read/write) accepts it as of the additive fix. This
# is a pure data UPDATE — safe to run any time, idempotent.
#
# SAFETY:
#   * READ-ONLY by default — prints how many rows WOULD change + the live
#     scope distribution. No mutation without --apply.
#   * --apply runs exactly: UPDATE pat SET scope='read-write' WHERE scope='admin'
#     Idempotent (re-running after success affects 0 rows).
#   * Talks to the D1 HTTP API directly (no wrangler version dependency).
#
# USAGE:
#   bash scripts/backfill-admin-scope-prod.sh            # dry-run (COUNT only)
#   bash scripts/backfill-admin-scope-prod.sh --apply    # execute the UPDATE
#
# REQUIRES (from .env.local, gitignored): CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_API_TOKEN
set -euo pipefail

APPLY=0
[[ "${1:-}" == "--apply" ]] && APPLY=1

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$REPO_ROOT/.env.local"
[[ -f "$ENV_FILE" ]] || { echo "FATAL: $ENV_FILE not found (holds the prod D1 credentials)"; exit 2; }
set -a; # shellcheck disable=SC1090
source "$ENV_FILE"; set +a

ACC="${CLOUDFLARE_ACCOUNT_ID:-}"
TOK="${CLOUDFLARE_API_TOKEN:-}"
DBID="d64742ea-e102-40b2-a844-ff02e3f94562"   # corelink-prod-d1 (apps/signup-worker/wrangler.toml)
[[ -n "$ACC" && -n "$TOK" ]] || { echo "FATAL: CLOUDFLARE_ACCOUNT_ID / CLOUDFLARE_API_TOKEN missing in .env.local"; exit 2; }

d1() {
  # $1 = SQL. Prints the raw JSON response.
  curl -sS -X POST "https://api.cloudflare.com/client/v4/accounts/${ACC}/d1/database/${DBID}/query" \
    -H "Authorization: Bearer ${TOK}" -H "Content-Type: application/json" \
    --data "{\"sql\": \"$1\"}"
}

ok() { echo "$1" | jq -e '.success == true' >/dev/null 2>&1; }

echo "── prod D1 pat.scope distribution (read-only) ─────────────────────────"
DISTRO="$(d1 "SELECT scope, count(*) AS n FROM pat GROUP BY scope ORDER BY n DESC;")"
ok "$DISTRO" || { echo "FATAL: D1 query failed:"; echo "$DISTRO" | jq '.errors'; exit 1; }
echo "$DISTRO" | jq -r '.result[0].results[] | "  \(.scope)\t\(.n)"'

ADMIN_N="$(echo "$DISTRO" | jq -r '(.result[0].results[] | select(.scope=="admin") | .n) // 0')"
echo "  → ${ADMIN_N} row(s) with scope='admin' would be demoted to 'read-write'."

if [[ "$APPLY" -eq 0 ]]; then
  echo
  echo "DRY-RUN. No changes made. Re-run with --apply to execute the UPDATE."
  exit 0
fi

if [[ "$ADMIN_N" == "0" ]]; then
  echo "Nothing to do — 0 admin rows. (Idempotent no-op.)"; exit 0
fi

echo
echo "── APPLY: UPDATE pat SET scope='read-write' WHERE scope='admin' ───────"
RES="$(d1 "UPDATE pat SET scope='read-write' WHERE scope='admin';")"
if ! ok "$RES"; then
  echo "FATAL: UPDATE failed:"; echo "$RES" | jq '.errors'; exit 1
fi
CHANGED="$(echo "$RES" | jq -r '.result[0].meta.changes // .result[0].meta.rows_written // "?"')"
echo "  UPDATE ok — rows changed: ${CHANGED}"
echo "── post-back-fill distribution ───────────────────────────────────────"
d1 "SELECT scope, count(*) AS n FROM pat GROUP BY scope ORDER BY n DESC;" \
  | jq -r '.result[0].results[] | "  \(.scope)\t\(.n)"'
echo "Done."
