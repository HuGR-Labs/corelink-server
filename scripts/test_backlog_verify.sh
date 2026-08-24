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

# A fixture carries the `### B-NNN` heading a real item carries: the gate now
# requires the heading and the block to name the same id, so a headless fixture
# would fail on THAT instead of on the defect the cell is actually testing.
item() { # $1 id, $2 status, $3 verify, $4 last-verified
  printf '### %s — fixture\n\n```backlog\nid: %s\nrepo: corelink-server\nowner: tl\nstatus: %s\nverify: %s\nverify-means: test fixture\nlast-verified: %s\n```\n' \
    "$1" "$1" "$2" "$3" "$4"
}

echo "backlog gate"

cell "a truthful item passes" 0 CONFIRMED "$(item B-1 open '"true"' 2026-08-23)"

# YAML reads a bare `true` as a boolean, which silently stops being a command.
# This file's own first draft made exactly that mistake and CI caught it.
cell "an unquoted YAML boolean verify is BROKEN, not run" 1 BROKEN "$(item B-1 open true 2026-08-23)"

# The core contract: the world moved, the file did not.
cell "a claim the repo contradicts is DRIFTED" 1 DRIFTED "$(item B-1 open '"false"' 2026-08-23)"

# The decay rule — the thing that would have caught this project's stale notes.
cell "an unverifiable claim goes STALE once it ages out" 1 STALE "$(item B-1 open manual 2026-07-01)"
cell "a freshly verified manual claim is fine" 0 CONFIRMED "$(item B-1 open manual 2026-08-20)"

# A malformed item must never be silently skipped: skipping is the failure mode
# the whole gate exists to prevent.
cell "a missing required field is BROKEN, not ignored" 1 BROKEN \
  "$(printf '### B-1 — fixture\n\n```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\n```\n')"
cell "an invalid status is BROKEN" 1 BROKEN "$(item B-1 nearly-done '"true"' 2026-08-23)"
cell "a duplicate id is BROKEN" 1 BROKEN "$(item B-1 open '"true"' 2026-08-23; item B-1 open '"true"' 2026-08-23)"
cell "unparseable YAML is BROKEN" 1 BROKEN "$(printf '### B-1 — fixture\n\n```backlog\nid: [B-1\n```\n')"

# A deleted item passes every per-item check — the survivors are all still true.
# Only density catches it. This cell exists because exactly that happened while
# this file was being written, and the gate reported all-green.
cell "a gap in the ids is a hard failure (an item was deleted)" 2 - \
  "$(item B-1 open '"true"' 2026-08-23; item B-3 open '"true"' 2026-08-23)"

# An empty file is far more likely to be a broken format than genuinely no work.
cell "an empty backlog is a hard failure, not a pass" 2 - "$(printf 'no items here\n')"

# Every reference from outside BACKLOG.md cites the HEADING; the gate reads the
# BLOCK. On 2026-08-24 two items held each other's ids — both unique, so the
# duplicate check was satisfied while the register pointed at the wrong work.
cell "a heading and its block naming different ids is a hard failure" 2 "heading says B-1" \
  "$(printf '### B-1 — fixture\n\n```backlog\nid: B-2\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# A block with no heading at all is orphaned prose-side: nothing outside the
# file can cite it, and a reader scrolling past sees no item there.
cell "a block with no heading above it is a hard failure" 2 "no \`### B-" \
  "$(printf '```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

echo
if [[ "$fails" -eq 0 ]]; then echo "backlog gate: all cells passed"; exit 0; fi
echo "backlog gate: $fails cell(s) failed"; exit 1
