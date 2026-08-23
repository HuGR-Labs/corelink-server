#!/usr/bin/env bash
# Prove the backlog gate can actually go red.
#
# A gate nobody has watched fail is a gate that proves nothing — this project has
# shipped several of those and paid for it. So each cell here constructs a backlog
# that SHOULD fail and asserts that it does, plus one that should pass.
#
# Run: bash scripts/test_backlog_verify.sh
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
VERIFY="$HERE/backlog_verify.py"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT

fails=0
pass() { echo "  PASS  $1"; }
fail() { echo "  FAIL  $1"; fails=$((fails + 1)); }

# $1 name, $2 expected exit, $3 expected verdict substring (or "-"), $4 body
cell() {
  local name="$1" want="$2" verdict="$3" body="$4" out rc
  printf '%s\n' "$body" > "$SANDBOX/b.md"
  out="$(python3 "$VERIFY" --file "$SANDBOX/b.md" --today 2026-08-23 2>&1)"; rc=$?
  if [[ "$rc" -ne "$want" ]]; then
    fail "$name (exit $rc, want $want)"; echo "$out" | sed 's/^/        /'; return
  fi
  if [[ "$verdict" != "-" ]] && ! grep -q "$verdict" <<<"$out"; then
    fail "$name (no $verdict in output)"; echo "$out" | sed 's/^/        /'; return
  fi
  pass "$name"
}

item() { # $1 id, $2 status, $3 verify, $4 last-verified
  printf '```backlog\nid: %s\nrepo: corelink-server\nowner: tl\nstatus: %s\nverify: %s\nverify-means: test fixture\nlast-verified: %s\n```\n' \
    "$1" "$2" "$3" "$4"
}

echo "backlog gate"

cell "a truthful item passes" 0 CONFIRMED "$(item B-1 open true 2026-08-23)"

# The core contract: the world moved, the file did not.
cell "a claim the repo contradicts is DRIFTED" 1 DRIFTED "$(item B-1 open false 2026-08-23)"

# The decay rule — the thing that would have caught this project's stale notes.
cell "an unverifiable claim goes STALE once it ages out" 1 STALE "$(item B-1 open manual 2026-07-01)"
cell "a freshly verified manual claim is fine" 0 CONFIRMED "$(item B-1 open manual 2026-08-20)"

# A malformed item must never be silently skipped: skipping is the failure mode
# the whole gate exists to prevent.
cell "a missing required field is BROKEN, not ignored" 1 BROKEN \
  "$(printf '```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\n```\n')"
cell "an invalid status is BROKEN" 1 BROKEN "$(item B-1 nearly-done true 2026-08-23)"
cell "a duplicate id is BROKEN" 1 BROKEN "$(item B-1 open true 2026-08-23; item B-1 open true 2026-08-23)"
cell "unparseable YAML is BROKEN" 1 BROKEN "$(printf '```backlog\nid: [B-1\n```\n')"

# An empty file is far more likely to be a broken format than genuinely no work.
cell "an empty backlog is a hard failure, not a pass" 2 - "$(printf 'no items here\n')"

echo
if [[ "$fails" -eq 0 ]]; then echo "backlog gate: all cells passed"; exit 0; fi
echo "backlog gate: $fails cell(s) failed"; exit 1
