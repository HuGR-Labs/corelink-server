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

cell "a truthful item passes" 0 CONFIRMED "$(item B-001 open '"true"' 2026-08-23)"

# YAML reads a bare `true` as a boolean, which silently stops being a command.
# This file's own first draft made exactly that mistake and CI caught it.
cell "an unquoted YAML boolean verify is BROKEN, not run" 1 BROKEN "$(item B-001 open true 2026-08-23)"

# The core contract: the world moved, the file did not.
cell "a claim the repo contradicts is DRIFTED" 1 DRIFTED "$(item B-001 open '"false"' 2026-08-23)"

# The decay rule — the thing that would have caught this project's stale notes.
cell "an unverifiable claim goes STALE once it ages out" 1 STALE "$(item B-001 open manual 2026-07-01)"
cell "a freshly verified manual claim is fine" 0 CONFIRMED "$(item B-001 open manual 2026-08-20)"

# A malformed item must never be silently skipped: skipping is the failure mode
# the whole gate exists to prevent.
cell "a missing required field is BROKEN, not ignored" 1 BROKEN \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: open\n```\n')"
cell "an invalid status is BROKEN" 1 BROKEN "$(item B-001 nearly-done '"true"' 2026-08-23)"
cell "a duplicate id is BROKEN" 1 BROKEN "$(item B-001 open '"true"' 2026-08-23; item B-001 open '"true"' 2026-08-23)"
cell "unparseable YAML is BROKEN" 1 BROKEN "$(printf '### B-001 — fixture\n\n```backlog\nid: [B-001\n```\n')"

# PyYAML's default for a repeated key is last-wins, SILENTLY. On 2026-08-24 a
# B-039 `verify:` was pasted into the B-040 block; both parsed, both were unique
# by id, and the gate ran B-039's check while reporting on B-040. The two agreed
# at the time, so nothing went red — the register would have started lying the
# moment they diverged.
cell "a duplicate key inside one block is BROKEN, not last-wins" 1 BROKEN \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify: "false"\nverify-means: |\n  fixture\nlast-verified: 2026-08-23\n```\n')"

# A deleted item passes every per-item check — the survivors are all still true.
# Only density catches it. This cell exists because exactly that happened while
# this file was being written, and the gate reported all-green.
cell "a gap in the ids is a hard failure (an item was deleted)" 2 - \
  "$(item B-001 open '"true"' 2026-08-23; item B-003 open '"true"' 2026-08-23)"

# An empty file is far more likely to be a broken format than genuinely no work.
cell "an empty backlog is a hard failure, not a pass" 2 - "$(printf 'no items here\n')"

# Every reference from outside BACKLOG.md cites the HEADING; the gate reads the
# BLOCK. On 2026-08-24 two items held each other's ids — both unique, so the
# duplicate check was satisfied while the register pointed at the wrong work.
cell "a heading and its block naming different ids is a hard failure" 2 "heading says B-001" \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-002\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# A block with no heading at all is orphaned prose-side: nothing outside the
# file can cite it, and a reader scrolling past sees no item there.
cell "a heading whose block lost its opening fence is a hard failure" 2 "B-002" \
  "$(printf '### B-001\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n\n### B-002\n\nid: B-002\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

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
  "$(item B-001 open '"true"' 2026-08-23
     printf '### B-002 — fixture\n\nid: B-002\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n\n'
     item B-003 open '"true"' 2026-08-23)" \
  "heading says"

cell "a \`### B-\` inside a fenced example is not a heading" 0 "" \
  "$(printf '### B-001\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n\n```text\n### B-042\nexample of the item format\n```\n')"

cell "a block with no heading above it is a hard failure" 2 "no \`### B-" \
  "$(printf '```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# B-143: a malformed id used to be SKIPPED by the heading check and INVISIBLE to
# the density check, so it merged CONFIRMED. Both loud checks (gap, duplicate)
# presuppose a well-formed id. Four spellings, because a gate that matches the
# literal `B-UNALLOCATED` would pass one cell and be decorative.
malformed() { # $1 the id to plant
  printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n\n'
  printf '### %s — fixture\n\n```backlog\nid: %s\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n' "$1" "$1"
}
for bad in B-UNALLOCATED B-131a b-131 B-TBD; do
  cell "a placeholder id ($bad) is a hard failure, not a silent skip" 2 "is not \`B-<digits>\`" \
    "$(malformed "$bad")"
done

# The negative side of the same rule: the canonical form must still pass. A cell
# that only watches the gate go red cannot see a gate that reds on everything.
cell "a canonical id is NOT flagged as malformed" 0 CONFIRMED \
  "$(item B-001 open '"true"' 2026-08-23)" "is not \`B-<digits>\`"

# Zero is not an allocatable backlog id. Test two spellings so the gate cannot
# merely reject the three-digit spelling while allowing another zero alias.
for zero in B-000 B-00; do
  cell "a non-positive id ($zero) is rejected" 2 "non-positive" \
    "$(item "$zero" open '"true"' 2026-08-23)" "CONFIRMED"
done

# B-167 CLOSED — this cell was pinned in its "KNOWN GAP" polarity with the note
# that it "must be INVERTED when B-167 is fixed, and its failure is the reminder".
# The reminder fired; this is the inversion. `B-01` satisfies `^B-\d+$`, keeps
# density happy (int("01")==1) and does not collide (the duplicate check compares
# strings), so nothing but a canonical-form rule can see it.
cell "a leading-zero id no longer aliases its twin (B-167)" 2 "is not canonical" \
  "$(item B-001 open '"true"' 2026-08-23; item B-01 open '"true"' 2026-08-23)" "CONFIRMED"

# B-147: `status` and `owner` were validated independently and never crossed.
# `owner: owner` means "still needs the human"; a done item does not.
cell "done + owner: owner is BROKEN — a finished item cannot still need the human" 1 BROKEN \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: owner\nstatus: done\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# Both negative sides, so the cross-check cannot be satisfied by rejecting
# `owner: owner` outright or by reading `!= open`. `parked` is EXCLUDED on
# purpose: an item parked because it waits on the owner is legitimate.
cell "done + owner: tl passes" 0 CONFIRMED \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: done\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"
cell "open + owner: owner passes — the legitimate case" 0 CONFIRMED \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: owner\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"
cell "parked + owner: owner passes — deliberately OUT of the rule" 0 CONFIRMED \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: owner\nstatus: parked\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"

# ── B-167: the canonical id form ────────────────────────────────────────────
# `^B-\d+$` (B-143) was necessary and not sufficient. `B-0142` satisfies it,
# `int("0142") == 142` keeps density happy, and the duplicate check compares
# STRINGS — so it merged alongside the real B-142 as a silent alias. Measured
# on main before this rule: `CONFIRMED B-0142 open`, rc=0.
#
# The rule chosen (of the three B-167 enumerated): the spelling must equal
# `f"B-{int(n):03d}"`.
alias_pair() { # $1 the non-canonical spelling of item 1
  item B-001 open '"true"' 2026-08-23
  printf '\n### %s — fixture\n\n```backlog\nid: %s\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n' "$1" "$1"
}
# The other half of "canonical": under-padding is refused too, so there is exactly
# ONE spelling per number rather than a rule that only bans the padded variant.
cell "an UNDER-padded id (B-1, where canonical is B-001) is refused" 2 "is not canonical" \
  "$(printf '### B-1 — fixture\n\n```backlog\nid: B-1\nrepo: corelink-server\nowner: tl\nstatus: open\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')" "CONFIRMED"

# The negative controls. Without them, "refuse every id" passes both cells above
# and takes the whole register red.
cell "the canonical three-digit form (B-001) passes" 0 CONFIRMED \
  "$(item B-001 open '"true"' 2026-08-23)" "is not canonical"

# The reason this rule is `f"B-{int(n):03d}"` and NOT `^B-\d{3}$`: four-digit ids
# must keep working, or the gate forbids the day the register reaches B-1000.
# `^B-\d{3}$` passes every other cell in this file and fails only this one.
cell "a four-digit id (B-1000) is canonical too — width is not frozen" 0 CONFIRMED \
  "$(python3 - <<'PYGEN'
for n in range(1, 1001):
    print(f"### B-{n:03d} — fixture\n")
    print("```backlog")
    print(f"id: B-{n:03d}\nrepo: corelink-server\nowner: tl\nstatus: open")
    print('verify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23')
    print("```\n")
PYGEN
)" "is not canonical"

# B-148: every workflow is a load-bearing input to at least one backlog verify
# command. The backlog gate must therefore trigger for the complete workflow
# directory, not merely for its own YAML file. This is intentionally tested by
# content mutation: deleting the glob must make the checker red, and a comment
# must not count as coverage.
workflow_trigger_check() {
  local workflow="$1"
  python3 - "$workflow" <<'PY'
import sys
import yaml

with open(sys.argv[1], encoding="utf-8") as fh:
    doc = yaml.safe_load(fh) or {}
on = doc.get(True, doc.get("on")) or {}
expected = ".github/workflows/**"
for event in ("pull_request", "push"):
    config = on.get(event) or {}
    paths = config.get("paths") if isinstance(config, dict) else None
    if expected not in (paths or []):
        raise SystemExit(f"{event} does not cover {expected}")
print("workflow trigger covers all workflow surfaces")
PY
}

workflow="$HERE/../.github/workflows/backlog-verify.yml"
if workflow_trigger_check "$workflow" >/dev/null 2>&1; then
  pass "B-148 workflow changes trigger backlog verification"
else
  fail "B-148 workflow changes trigger backlog verification"
fi

mutant="$SANDBOX/backlog-verify-mutant.yml"
sed '/^[[:space:]]*-[[:space:]]*"\.github\/workflows\/\*\*"/d' "$workflow" >"$mutant"
if workflow_trigger_check "$mutant" >/dev/null 2>&1; then
  fail "B-148 removal mutation is detected"
else
  pass "B-148 removal mutation is detected"
fi

comment_mutant="$SANDBOX/backlog-verify-comment-mutant.yml"
python3 - "$workflow" "$comment_mutant" <<'PY'
import sys
source, destination = sys.argv[1:]
text = open(source, encoding="utf-8").read()
old = '- ".github/workflows/**"'
if old not in text:
    raise SystemExit("workflow glob missing before replacement mutation")
open(destination, "w", encoding="utf-8").write(
    text.replace(old, '- "BACKLOG.md"  # workflow coverage removed', 1)
)
PY
if workflow_trigger_check "$comment_mutant" >/dev/null 2>&1; then
  fail "B-148 replacement mutation is detected"
else
  pass "B-148 replacement mutation is detected"
fi

echo
if [[ "$fails" -eq 0 ]]; then echo "backlog gate: all cells passed"; exit 0; fi
echo "backlog gate: $fails cell(s) failed"; exit 1
