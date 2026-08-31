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

# $1 name, $2 expected exit, $3 expected verdict substring (or "-"), $4 body,
# $5 OPTIONAL substring that must NOT appear — a cell that only asserts the right
# message was printed cannot see a WRONG message printed alongside it.
cell() {
  local name="$1" want="$2" verdict="$3" body="$4" forbidden="${5:-}" out rc
  printf '%s\n' "$body" > "$SANDBOX/b.md"
  out="$(python3 "$VERIFY" --file "$SANDBOX/b.md" --today 2026-08-23 2>&1)"; rc=$?
  if [[ "$rc" -ne "$want" ]]; then
    fail "$name (exit $rc, want $want)"; echo "$out" | sed 's/^/        /'; return
  fi
  if [[ "$verdict" != "-" ]] && ! grep -q "$verdict" <<<"$out"; then
    fail "$name (no $verdict in output)"; echo "$out" | sed 's/^/        /'; return
  fi
  if [[ -n "$forbidden" ]] && grep -q "$forbidden" <<<"$out"; then
    fail "$name (unwanted '$forbidden' in output)"; echo "$out" | sed 's/^/        /'; return
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

# PyYAML's default for a repeated key is last-wins, SILENTLY. On 2026-08-24 a
# B-039 `verify:` was pasted into the B-040 block; both parsed, both were unique
# by id, and the gate ran B-039's check while reporting on B-040. The two agreed
# at the time, so nothing went red — the register would have started lying the
# moment they diverged.
cell "a duplicate key inside one block is BROKEN, not last-wins" 1 BROKEN \
  "$(printf '### B-1 — fixture\n\n```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify: "false"\nverify-means: |\n  fixture\nlast-verified: 2026-08-23\n```\n')"

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
cell "a heading whose block lost its opening fence is a hard failure" 2 "B-2" \
  "$(printf '### B-1\n\n```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n\n### B-2\n\nid: B-2\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# Every other cell in this file uses a 1- or 2-item fixture, and NONE of them can
# see this: fence pairing is SEQUENTIAL from the top of the file, so a lost opener
# leaves an odd count and shifts every LATER pairing by one. With only the last
# item damaged there is no "later" to shift. Three items, middle one damaged, is
# the smallest fixture where the shift exists — and it is the common case, since
# a conflict resolution eats a fence wherever the conflict was.
#
# The right answer here is DENSITY (B-002 vanished, leaving a gap) — the exact
# message `main` printed before the orphan check was added. The wrong answer is a
# cascade of `heading says …, block says id: …` for every item after the damage,
# with the density rule never reached; that is what feeding the fence-MASKED
# headings into the divergence loop produces. Hence the forbidden substring.
cell "the MIDDLE item losing its fence reports DENSITY, not a shifted-heading cascade" 2 "missing B-002" \
  "$(item B-1 open '"true"' 2026-08-23
     printf '### B-2 — fixture\n\nid: B-2\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n\n'
     item B-3 open '"true"' 2026-08-23)" \
  "heading says"

cell "a \`### B-\` inside a fenced example is not a heading" 0 "" \
  "$(printf '### B-1\n\n```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n\n```text\n### B-42\nexample of the item format\n```\n')"

cell "a block with no heading above it is a hard failure" 2 "no \`### B-" \
  "$(printf '```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# ── B-143: a placeholder id used to merge CONFIRMED ─────────────────────────
# The divergence loop skipped any id that was not `B-<digits>`, and the density
# rule only collects ids that ARE — so a malformed id escaped BOTH. Measured on
# a throwaway copy before the fix: all four forms below came out
# `CONFIRMED … verify agrees with the declared status`, rc=0.
#
# Each cell names the id it is defending against, because "malformed" is a class
# and a regex that catches one spelling and not the next is the decorative gate
# this project keeps building.
#
# The fixture carries a REAL item alongside the malformed one, deliberately. A
# lone malformed block leaves `numbered` empty and trips the density rule, so the
# cell would go red for the wrong reason and would still be red with the fix
# reverted — a cell that cannot see its own defect. With B-1 present the density
# rule is satisfied, and the pre-fix behaviour is the one the item measured:
# `CONFIRMED`, rc=0. Verified by reverting the fix: all five cells below drop to
# exit 0 with CONFIRMED printed.
malformed() { # $1 the malformed id
  item B-1 open '"true"' 2026-08-23
  printf '\n### %s — fixture\n\n```backlog\nid: %s\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n' "$1" "$1"
}
cell "a B-UNALLOCATED placeholder is a hard failure, not CONFIRMED" 2 "not a canonical item id" \
  "$(malformed B-UNALLOCATED)" "CONFIRMED"
cell "a B-TBD placeholder is a hard failure" 2 "not a canonical item id" \
  "$(malformed B-TBD)" "CONFIRMED"
cell "a suffixed id (B-131a) is a hard failure" 2 "not a canonical item id" \
  "$(malformed B-131a)" "CONFIRMED"
cell "a lowercased id (b-131) is a hard failure" 2 "not a canonical item id" \
  "$(malformed b-131)" "CONFIRMED"

# The adjacent finding, and the reason `^B-\d+$` alone is not the whole fix:
# `B-0142` passes that regex, `int("0142") == 142` keeps density satisfied, and
# the duplicate check compared STRINGS — so it coexisted with the real B-142 as a
# silent alias. Two headings, two blocks, one number.
#
# B-143 proposed refusing leading zeros outright. That would have been WRONG
# here, and the cell below is the proof: this repo zero-pads to three digits by
# convention — 99 of the 166 live blocks are `B-001`..`B-099`. So the alias is
# closed by normalising the DUPLICATE key on the number, and padding itself stays
# legal.
cell "a zero-padded alias (B-01 next to B-1) is BROKEN, not two items" 1 "duplicate id" \
  "$(item B-1 open '"true"' 2026-08-23
     printf '\n### B-01 — fixture\n\n```backlog\nid: B-01\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# The cell that would have caught the wrong fix. `B-007` is the shape 99 of the
# 166 real ids have; a rule that refuses leading zeros turns the whole register
# red. This is the negative control for the canonicalisation, not decoration.
cell "the repo's own zero-padded ids (B-001) stay legal" 0 CONFIRMED \
  "$(item B-001 open '"true"' 2026-08-23)" "canonical item id"

# The negative control for the four cells above: the check must reject the
# malformed id WITHOUT rejecting the shape every real item uses. Without this
# cell, `if True: mismatches.append(...)` passes all five.
cell "a canonical id still passes untouched" 0 CONFIRMED \
  "$(item B-1 open '"true"' 2026-08-23)" "not a canonical item id"

# ── B-147: `owner:` x `status` was never crossed ────────────────────────────
# `validate_schema()` checked `status` against VALID_STATUS and `owner` against
# VALID_OWNER independently. `done` + `owner: owner` satisfies both and is a
# contradiction: a finished item cannot still be waiting on the human. It leaves
# closed work sitting in the owner's queue — drained by hand twice already.
owned() { # $1 status
  printf '### B-1 — fixture\n\n```backlog\nid: B-1\nrepo: corelink-server\nowner: owner\nstatus: %s\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n' "$1"
}
cell "a done item may not still carry owner: owner" 1 BROKEN "$(owned done)"

# The scope pin B-147 demanded. `parked` waiting on an owner decision is exactly
# what `parked` is FOR, so the rule is `== done`, not `!= open`. If someone later
# widens it, this cell goes red and the widening becomes a decision instead of a
# drift.
cell "a parked item may still carry owner: owner" 0 CONFIRMED "$(owned parked)" "BROKEN"

# The other half of the negative control: an OPEN item is the normal owner-queue
# state and must stay untouched.
cell "an open item may still carry owner: owner" 0 CONFIRMED "$(owned open)" "BROKEN"

echo
if [[ "$fails" -eq 0 ]]; then echo "backlog gate: all cells passed"; exit 0; fi
echo "backlog gate: $fails cell(s) failed"; exit 1
