#!/usr/bin/env bash
# Read-only deployment fence for the DSR DLQ redrive authority schema.
# Migration application is deliberately an approved operation outside rollout.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="$REPO_ROOT/apps/signup-worker/wrangler.toml"

if [[ "${1:-}" == "--config" ]]; then
  CONFIG="${2:?--config requires a Wrangler config path}"
  shift 2
fi
if (($#)); then
  echo "usage: $0 [--config <wrangler.toml>]" >&2
  exit 2
fi
if [[ ! -f "$CONFIG" ]]; then
  echo "DSR redrive schema fence: Wrangler config not found: $CONFIG" >&2
  exit 2
fi

WRANGLER_CMD=(npx --yes wrangler@4.95.0)
if command -v wrangler >/dev/null 2>&1; then
  WRANGLER_CMD=(wrangler)
fi

check_count() {
  local expected="$1" query="$2" label="$3" output count
  output="$("${WRANGLER_CMD[@]}" d1 execute CONFIG_DB --remote --config "$CONFIG" --command "$query" --json)" || {
    echo "DSR redrive schema fence: cannot query $label" >&2
    exit 1
  }
  count="$(printf '%s' "$output" | python3 -c '
import json, sys
try:
    payload = json.load(sys.stdin)
    rows = payload[0]["results"]
    value = rows[0]["count"]
    if not isinstance(value, int):
        raise ValueError("count is not an integer")
    print(value)
except (IndexError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
    raise SystemExit(f"unreadable D1 result: {error}")
')" || {
    echo "DSR redrive schema fence: unreadable result for $label" >&2
    exit 1
  }
  if [[ "$count" != "$expected" ]]; then
    echo "DSR redrive schema fence: $label expected $expected, got $count" >&2
    exit 1
  fi
}

check_count 1 \
  "SELECT COUNT(*) AS count FROM d1_migrations WHERE name = '0145_dsr_dlq_redrive_authority.sql'" \
  "migration 0145"
check_count 2 \
  "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name IN ('dsr_dlq_redrive_envelopes', 'dsr_dlq_redrive_audit')" \
  "redrive tables"
check_count 12 \
  "SELECT COUNT(*) AS count FROM pragma_table_info('dsr_dlq_redrive_envelopes') WHERE name IN ('event_id', 'dsr_id', 'tenant_id', 'queued_at_ms', 'legal_hold', 'requeue_count', 'state', 'actor_ref', 'approval_ref', 'expires_at_ms', 'claim_expires_at_ms', 'updated_at_ms')" \
  "redrive envelope columns"
check_count 2 \
  "SELECT COUNT(*) AS count FROM pragma_table_info('dsr_dlq_redrive_audit') WHERE name IN ('event_id', 'transition')" \
  "redrive audit columns"

echo "DSR redrive schema fence: migration 0145 and required tables/columns are present."
