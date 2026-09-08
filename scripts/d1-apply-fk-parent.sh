#!/usr/bin/env bash
# Apply D1 FK-parent rebuilds through one transaction and record the exact
# migration filename in Wrangler's ledger. `wrangler d1 migrations apply`
# executes each statement with foreign-key enforcement between statements;
# migration 0064 therefore fails on any populated tenant table. This helper is
# called by every production migration runner before the ordinary apply.
set -euo pipefail

usage() {
    printf 'usage: %s --binding NAME --env ENV --remote\n' "$0" >&2
}

BINDING=""
ENV=""
REMOTE=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --binding) BINDING="${2:-}"; shift 2 ;;
        --env) ENV="${2:-}"; shift 2 ;;
        --remote) REMOTE=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'fatal: unknown argument: %s\n' "$1" >&2; usage; exit 2 ;;
    esac
done
if [[ -z "$BINDING" || -z "$ENV" || "$REMOTE" != true ]]; then
    printf 'fatal: --binding, --env and --remote are required\n' >&2
    usage
    exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MIGRATION="$REPO_ROOT/migrations/d1/0064_tenant_tier_max.sql"
WRANGLER_CMD="${WRANGLER_CMD:-wrangler}"
log() { printf '[d1-apply-fk-parent] %s\n' "$*"; }
err() { printf '[d1-apply-fk-parent] ERROR: %s\n' "$*" >&2; }
# The callers pass either a pinned binary or the explicit `npx wrangler@4`
# command. Tokenize that operator-controlled command once, then invoke the
# canonical Wrangler subcommand through an array; an arbitrary shell string
# must never become an executable query fragment.
read -r -a WRANGLER_ARGS <<< "$WRANGLER_CMD"
if [[ ${#WRANGLER_ARGS[@]} -eq 0 ]]; then
    err "WRANGLER_CMD is empty; refusing remote inspection"
    exit 1
fi

if [[ ! -f "$MIGRATION" ]]; then
    err "missing canonical FK-parent migration: $MIGRATION"
    exit 1
fi

run() { "${WRANGLER_ARGS[@]}" d1 execute "$BINDING" --env "$ENV" --remote "$@"; }

query_json() {
    local query="$1" output
    if ! output="$(run --command "$query" --json 2>&1)"; then
        err "Wrangler D1 query failed; refusing to infer database state"
        err "${output:0:240}"
        return 1
    fi
    if ! python3 -c 'import json,sys
payload=json.load(sys.stdin)
if (not isinstance(payload, list) or len(payload) != 1 or
        not isinstance(payload[0], dict) or
        not isinstance(payload[0].get("results"), list) or
        any(not isinstance(row, dict) for row in payload[0]["results"])):
    raise SystemExit(1)' <<< "$output" >/dev/null 2>&1; then
        err "Wrangler D1 query returned invalid JSON; refusing to infer database state"
        return 1
    fi
    printf '%s' "$output"
}

json_has_name() {
    local payload="$1" expected="$2"
    python3 -c 'import json,sys
payload=json.load(sys.stdin)
rows=[]
if isinstance(payload,list):
    for item in payload:
        if isinstance(item,dict) and isinstance(item.get("results"),list): rows.extend(item["results"])
elif isinstance(payload,dict) and isinstance(payload.get("results"),list):
    rows=payload["results"]
raise SystemExit(0 if any(isinstance(row,dict) and row.get("name")==sys.argv[1] for row in rows) else 1)' "$expected" <<< "$payload"
}

json_has_sql_literal() {
    local payload="$1" literal="$2"
    python3 -c 'import json,sys
payload=json.load(sys.stdin)
rows=[]
if isinstance(payload,list):
    for item in payload:
        if isinstance(item,dict) and isinstance(item.get("results"),list): rows.extend(item["results"])
elif isinstance(payload,dict) and isinstance(payload.get("results"),list):
    rows=payload["results"]
if len(rows) != 1 or "sql" not in rows[0]:
    raise SystemExit(2)
raise SystemExit(0 if sys.argv[1] in str(rows[0]["sql"]) else 1)' "$literal" <<< "$payload"
}

# A fresh database has no tenant table yet; ordinary migrations own creation in
# order. The populated-DB hazard starts only once tenant exists.
TABLES="$(query_json "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('tenant','d1_migrations') ORDER BY name")" || exit 1
if ! json_has_name "$TABLES" tenant; then
    log "tenant table absent; ordinary migrations apply owns 0064"
    exit 0
fi
if ! json_has_name "$TABLES" d1_migrations; then
    err "tenant exists but d1_migrations ledger is absent; refusing an untracked rebuild"
    exit 1
fi

APPLIED="$(query_json "SELECT name FROM d1_migrations WHERE name = '0064_tenant_tier_max.sql' LIMIT 1")" || exit 1
if json_has_name "$APPLIED" 0064_tenant_tier_max.sql; then
    log "0064_tenant_tier_max.sql already recorded; no special apply needed"
    exit 0
fi
PREREQUISITE="$(query_json "SELECT name FROM d1_migrations WHERE name = '0063_pat_customer_keys.sql' LIMIT 1")" || exit 1
if ! json_has_name "$PREREQUISITE" 0063_pat_customer_keys.sql; then
    err "0063 prerequisite is not recorded; refusing to execute 0064 out of order"
    exit 1
fi

# If the widened CHECK is already present but the ledger is missing, replaying
# the DROP/RENAME rebuild is unsafe. Stop for an operator ledger repair rather
# than turning desynchronisation into a second destructive rebuild.
TENANT_SCHEMA="$(query_json "SELECT sql FROM sqlite_master WHERE type='table' AND name='tenant'")" || exit 1
if json_has_sql_literal "$TENANT_SCHEMA" "'max'"; then
    err "tenant already accepts max but 0064 is unrecorded; repair d1_migrations ledger manually"
    exit 1
else
    schema_rc=$?
    if (( schema_rc > 1 )); then
        err "tenant schema probe returned no unambiguous row; refusing to infer database state"
        exit 1
    fi
fi

TMP_SQL="$(mktemp "${TMPDIR:-/tmp}/corelink-0064.XXXXXX.sql")"
cleanup() { rm -f "$TMP_SQL"; }
trap cleanup EXIT
cp "$MIGRATION" "$TMP_SQL"
# Wrangler records names as paths relative to migrations_dir. Keep this INSERT
# in the same execute-file transaction as 0064, so schema success can never be
# observed without its idempotency ledger entry.
echo 'INSERT INTO d1_migrations (name) VALUES ('"'"'0064_tenant_tier_max.sql'"'"');' >> "$TMP_SQL"

log "applying 0064_tenant_tier_max.sql via execute --file (single transaction)"
run --file "$TMP_SQL" >/dev/null
log "0064 applied and recorded atomically in d1_migrations"
