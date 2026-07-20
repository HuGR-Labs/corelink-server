#!/usr/bin/env bash
# seed-dogfood-tenant.sh — seed ONE dogfood/E2E `tenant` row in prod CONFIG_DB so a
# subsequent `mint-dogfood-pat.sh` for that tenant can persist its `pat` row.
#
# WHY: D1 enforces the `pat.tenant_id` → `tenant(tenant_id)` FOREIGN KEY. So a PAT
# minted for a *fresh* UUID with no `tenant` row hard-fails the D1 INSERT
# (`FOREIGN KEY constraint failed`, code 7500) — the token is minted-but-not-
# persisted (inert; never authenticates). `mint-dogfood-pat.sh` only writes the
# `pat` row, never the `tenant` row. This is the missing mirror: it seeds the
# `tenant` row (+ optionally a near-zero `tenant_quota`, for a deterministic-402
# quota persona) so a DEDICATED persona tenant can be provisioned FK-safely.
#
# Idempotent: `INSERT OR IGNORE` on `tenant`; `INSERT OR REPLACE` on `tenant_quota`
# (so re-runs converge the budget). Teardown mirror: `teardown-dogfood-tenant.sh`.
#
# SECRETS: none hard-coded. The D1 write uses `wrangler` (CLOUDFLARE_API_TOKEN in
# .env.local). Prints no secret.
#
# Usage:
#   set -a; source .env.local; set +a
#   scripts/admin/seed-dogfood-tenant.sh \
#     --tenant <uuid>             (required — the dedicated persona tenant id)
#     [--region enam|weur|apac|…] (default: enam — the US R2 CAS region)
#     [--quota-micros <N>]        (optional — also seed tenant_quota with this
#                                  monthly_budget_usd_micros + accrued=0; e.g. 1
#                                  ⇒ ANY billable op 402s on the first write, no
#                                  accrued pre-load, deterministic)
#     [--yes]                     (required to write; else DRY-RUN)
#
# Exit: 0 ok · 1 op failed · 2 args/env · 127 missing tool

set -euo pipefail

TENANT=""
REGION="enam"
QUOTA_MICROS=""
CONFIRMED=false
WRANGLER="${WRANGLER:-worker/node_modules/.bin/wrangler}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tenant) TENANT="${2:?}"; shift 2 ;;
    --region) REGION="${2:?}"; shift 2 ;;
    --quota-micros) QUOTA_MICROS="${2:?}"; shift 2 ;;
    --yes) CONFIRMED=true; shift ;;
    -h|--help) grep '^#' "$0" | tail -n +2 | sed 's/^# \?//'; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

command -v python3 >/dev/null || { echo "python3 required" >&2; exit 127; }
[[ -n "$TENANT" ]] || { echo "ERROR: --tenant <uuid> required" >&2; exit 2; }
[[ -n "${CLOUDFLARE_API_TOKEN:-}" ]] || { echo "ERROR: CLOUDFLARE_API_TOKEN unset (source .env.local)" >&2; exit 2; }

NOW_MS="$(python3 -c 'import time;print(int(time.time()*1000))')"
# Deterministic non-PII email_hash (NOT NULL) derived from the tenant id — a
# dogfood tenant has no real email; this satisfies the column without inventing
# a plausible address.
EMAIL_HASH="$(printf 'dogfood:%s' "$TENANT" | python3 -c 'import sys,hashlib;print(hashlib.sha256(sys.stdin.buffer.read()).hexdigest())')"

# tenant row — the same columns signup-worker's insertTenant writes (minus the
# Clerk/Stripe identity, which a dogfood tenant has none of). tenant_state=active.
TENANT_SQL="$(python3 - "$TENANT" "$REGION" "$EMAIL_HASH" "$NOW_MS" <<'PY'
import sys
tid,region,eh,now=sys.argv[1:5]
def q(s): return "'" + s.replace("'","''") + "'"
print(
 "INSERT OR IGNORE INTO tenant "
 "(tenant_id, primary_region, tenant_state, email_hash, created_at_ms, updated_at_ms, created_ms, updated_ms) "
 f"VALUES ({q(tid)}, {q(region)}, 'active', {q(eh)}, {int(now)}, {int(now)}, {int(now)}, {int(now)});"
)
PY
)"

echo "[seed-dogfood-tenant] tenant=$TENANT region=$REGION quota_micros=${QUOTA_MICROS:-<none>}" >&2
if ! $CONFIRMED; then
  echo "[seed-dogfood-tenant] DRY-RUN (no --yes): would INSERT OR IGNORE the tenant row${QUOTA_MICROS:+ + INSERT OR REPLACE tenant_quota}. Re-run with --yes." >&2
  exit 0
fi

"$WRANGLER" d1 execute CONFIG_DB --env prod --remote --command "$TENANT_SQL" >&2 \
  || { echo "ERROR: tenant-row seed failed" >&2; exit 1; }
echo "[seed-dogfood-tenant] ✅ tenant row seeded (idempotent)." >&2

if [[ -n "$QUOTA_MICROS" ]]; then
  QUOTA_SQL="$(python3 - "$TENANT" "$QUOTA_MICROS" "$NOW_MS" <<'PY'
import sys
tid,micros,now=sys.argv[1:4]
def q(s): return "'" + s.replace("'","''") + "'"
print(
 "INSERT OR REPLACE INTO tenant_quota "
 "(tenant_id, monthly_budget_usd_micros, accrued_usd_micros, cycle_anchor_ms, updated_at_ms) "
 f"VALUES ({q(tid)}, {int(micros)}, 0, {int(now)}, {int(now)});"
)
PY
)"
  "$WRANGLER" d1 execute CONFIG_DB --env prod --remote --command "$QUOTA_SQL" >&2 \
    || { echo "ERROR: tenant_quota seed failed" >&2; exit 1; }
  echo "[seed-dogfood-tenant] ✅ tenant_quota seeded (budget=${QUOTA_MICROS}µ\$, accrued=0) — any billable op 402s on first write." >&2
fi

echo "[seed-dogfood-tenant] done. Now: mint-dogfood-pat.sh --tenant $TENANT --scope cas:rw --yes" >&2
