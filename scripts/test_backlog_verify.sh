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

# Focused B-148 harness. Its input is always the checked-out workflow path;
# /dev/fd and process substitution would only prove that a temporary stream can
# be parsed, not that the workflow we ship has the required trigger semantics.
b148_workflow_harness() {
  local workflow="$HERE/../.github/workflows/backlog-verify.yml"
  local single_quote_workflow="$SANDBOX/backlog-verify-single-quote.yml"
  local mutant="$SANDBOX/backlog-verify-mutant.yml"
  local replacement="$SANDBOX/backlog-verify-replacement.yml"
  local push_mutant="$SANDBOX/backlog-verify-push-mutant.yml"
  local schedule_mutant="$SANDBOX/backlog-verify-schedule-mutant.yml"
  local dispatch_mutant="$SANDBOX/backlog-verify-dispatch-mutant.yml"
  local paths_scalar_mutant="$SANDBOX/backlog-verify-paths-scalar-mutant.yml"
  local branches_scalar_mutant="$SANDBOX/backlog-verify-branches-scalar-mutant.yml"
  local noop_mutant="$SANDBOX/backlog-verify-noop-mutant.yml"
  local single_quote_mutant="$SANDBOX/backlog-verify-single-quote-mutant.yml"
  local single_quote_replacement="$SANDBOX/backlog-verify-single-quote-replacement.yml"
  local single_quote_push_mutant="$SANDBOX/backlog-verify-single-quote-push-mutant.yml"
  local single_quote_schedule_mutant="$SANDBOX/backlog-verify-single-quote-schedule-mutant.yml"
  local single_quote_dispatch_mutant="$SANDBOX/backlog-verify-single-quote-dispatch-mutant.yml"

  workflow_trigger_check() {
    python3 - "$1" <<'PY'
import sys
import yaml

with open(sys.argv[1], encoding="utf-8") as fh:
    doc = yaml.safe_load(fh) or {}
on = doc.get(True, doc.get("on")) or {}
expected = ".github/workflows/**"
pr = on.get("pull_request")
pr_paths = pr.get("paths") if isinstance(pr, dict) else None
if not isinstance(pr_paths, list):
    raise SystemExit("pull_request paths is not a list")
if expected not in pr_paths:
    raise SystemExit("pull_request does not cover .github/workflows/**")
push = on.get("push")
push_paths = push.get("paths") if isinstance(push, dict) else None
if not isinstance(push_paths, list):
    raise SystemExit("push paths is not a list")
if expected not in push_paths:
    raise SystemExit("push does not cover .github/workflows/**")
push_branches = push.get("branches") if isinstance(push, dict) else None
if not isinstance(push_branches, list):
    raise SystemExit("push branches is not a list")
if "main" not in push_branches:
    raise SystemExit("push trigger is not anchored to main")
schedule = on.get("schedule")
if not isinstance(schedule, list) or not schedule or not all(
    isinstance(entry, dict) and isinstance(entry.get("cron"), str) and entry["cron"]
    for entry in schedule
):
    raise SystemExit("schedule trigger is missing a cron")
if "workflow_dispatch" not in on:
    raise SystemExit("workflow_dispatch trigger is missing")
print("workflow trigger covers workflow paths, main push, schedule, and dispatch")
PY
  }

  b148_fixture_changed() {
    local source="$1" fixture="$2"
    [[ -s "$fixture" ]] && ! cmp -s "$source" "$fixture"
  }

  if workflow_trigger_check "$workflow" >/dev/null 2>&1; then
    pass "B-148 checked-out workflow has path/self/schedule/dispatch triggers"
  else
    fail "B-148 checked-out workflow has path/self/schedule/dispatch triggers"
  fi

  b148_generate_mutants() {
    local source="$1"
    local removal="$2"
    local replacement="$3"
    local push="$4"
    local schedule="$5"
    local dispatch="$6"

    if python3 - "$source" "$removal" "$replacement" "$push" \
      "$schedule" "$dispatch" <<'PY'
import sys
source, removal, replacement, push, schedule, dispatch = sys.argv[1:]
text = open(source, encoding="utf-8").read()
import re

glob = re.compile(r'^(?P<indent>[ \t]*)-[ \t]*["\']\.github/workflows/\*\*["\'][ \t]*$', re.MULTILINE)
matches = list(glob.finditer(text))
if len(matches) != 2:
    raise SystemExit(f"expected two workflow glob lines, found {len(matches)}")

def remove_match(match):
    return ""

def replace_match(match):
    return f'{match.group("indent")}- "BACKLOG.md"  # workflow coverage removed'

seen = [0]
def remove_second(match):
    match_number = seen[0]
    seen[0] += 1
    return "" if match_number == 1 else match.group(0)

open(removal, "w", encoding="utf-8").write(glob.sub(remove_match, text, count=1))
open(replacement, "w", encoding="utf-8").write(glob.sub(replace_match, text, count=1))
open(push, "w", encoding="utf-8").write(
    glob.sub(remove_second, text)
)
open(schedule, "w", encoding="utf-8").write(
    text.replace('    - cron: "17 6 * * *"\n', "", 1)
)
open(dispatch, "w", encoding="utf-8").write(
    text.replace("  workflow_dispatch: {}\n", "", 1)
)
PY
    then
      for generated in "$removal" "$replacement" "$push" "$schedule" "$dispatch"; do
        if ! b148_fixture_changed "$source" "$generated"; then
          fail "B-148 mutation fixture generation produced $generated"
          return 1
        fi
      done
    else
      fail "B-148 mutation fixture generation succeeded"
      return 1
    fi
  }

  b148_expect_rejected() {
    local fixture="$1"
    local description="$2"
    local expected_error="$3"
    local result
    if result=$(workflow_trigger_check "$fixture" 2>&1); then
      fail "$description"
    elif [[ "$result" == *"$expected_error"* ]]; then
      pass "$description"
    else
      fail "$description (unexpected checker failure)"
    fi
  }

  if b148_generate_mutants "$workflow" "$mutant" "$replacement" "$push_mutant" \
    "$schedule_mutant" "$dispatch_mutant"; then
    b148_expect_rejected "$mutant" "B-148 removal mutation is detected" \
      "pull_request does not cover"
    b148_expect_rejected "$replacement" "B-148 replacement mutation is detected" \
      "pull_request does not cover"
    b148_expect_rejected "$push_mutant" "B-148 self-trigger mutation is detected" \
      "push does not cover"
    b148_expect_rejected "$schedule_mutant" "B-148 schedule mutation is detected" \
      "schedule trigger is missing"
    b148_expect_rejected "$dispatch_mutant" "B-148 dispatch mutation is detected" \
      "workflow_dispatch trigger is missing"
  fi

  if python3 - "$workflow" "$paths_scalar_mutant" "$branches_scalar_mutant" <<'PY'
import re
import sys

source, paths_scalar, branches_scalar = sys.argv[1:]
text = open(source, encoding="utf-8").read()

paths_pattern = re.compile(
    r'^(  pull_request:\n)    paths:\n'
    r'(?:(?:^      - .*\n)|(?:^      #.*\n))+',
    re.MULTILINE,
)
text_paths, paths_count = paths_pattern.subn(
    r'\1    paths: ".github/workflows/**"\n', text, count=1
)
if paths_count != 1:
    raise SystemExit(f"expected one pull_request paths list, found {paths_count}")

branches_pattern = re.compile(r'^    branches: \[main\]\n', re.MULTILINE)
text_branches, branches_count = branches_pattern.subn(
    "    branches: main\n", text, count=1
)
if branches_count != 1:
    raise SystemExit(f"expected one push branches list, found {branches_count}")

open(paths_scalar, "w", encoding="utf-8").write(text_paths)
open(branches_scalar, "w", encoding="utf-8").write(text_branches)
PY
  then
    for generated in "$paths_scalar_mutant" "$branches_scalar_mutant"; do
      if ! b148_fixture_changed "$workflow" "$generated"; then
        fail "B-148 scalar mutation fixture generation produced $generated"
        return 1
      fi
    done
    b148_expect_rejected "$paths_scalar_mutant" \
      "B-148 scalar pull_request paths mutation is detected" \
      "pull_request paths is not a list"
    b148_expect_rejected "$branches_scalar_mutant" \
      "B-148 scalar push branches mutation is detected" \
      "push branches is not a list"
  else
    fail "B-148 scalar mutation fixture generation succeeded"
  fi

  # A no-op mutation must be rejected by the fixture-integrity check rather
  # than being mistaken for a checker rejection. This pins the distinction the
  # harness makes between a changed fixture and a copied source file.
  if ! cp "$workflow" "$noop_mutant"; then
    fail "B-148 no-op mutation fixture generation failed"
  elif b148_fixture_changed "$workflow" "$noop_mutant"; then
    fail "B-148 no-op mutation unexpectedly changed the fixture"
  else
    pass "B-148 no-op mutation is rejected as unchanged fixture"
  fi

  # YAML gives single-quoted and double-quoted scalars identical semantics. The
  # old fixture generator only recognized the latter and could therefore fail
  # before writing fixtures; the missing files were then mistaken for rejected
  # mutations. Keep this equivalent representation as a regression test. A
  # fixture-generation error is itself a failure, never a rejected mutation.
  if ! python3 - "$workflow" "$single_quote_workflow" <<'PY'
import sys
source, destination = sys.argv[1:]
text = open(source, encoding="utf-8").read()
double_glob = '      - ".github/workflows/**"'
single_glob = "      - '.github/workflows/**'"
if text.count(double_glob) != 2:
    raise SystemExit(f"expected two double-quoted workflow globs, found {text.count(double_glob)}")
text = text.replace(double_glob, single_glob)
if text.count(single_glob) != 2 or double_glob in text:
    raise SystemExit("single-quote fixture did not replace exactly both glob scalars")
open(destination, "w", encoding="utf-8").write(text)
PY
  then
    fail "B-148 single-quote regression fixture generation succeeded"
  elif ! workflow_trigger_check "$single_quote_workflow" >/dev/null 2>&1; then
    fail "B-148 single-quoted workflow remains semantically valid"
  elif b148_generate_mutants "$single_quote_workflow" "$single_quote_mutant" \
    "$single_quote_replacement" "$single_quote_push_mutant" \
    "$single_quote_schedule_mutant" "$single_quote_dispatch_mutant"; then
    pass "B-148 single-quoted workflow is covered by the mutation generator"
    b148_expect_rejected "$single_quote_mutant" \
      "B-148 single-quoted removal mutation is detected" "pull_request does not cover"
    b148_expect_rejected "$single_quote_replacement" \
      "B-148 single-quoted replacement mutation is detected" "pull_request does not cover"
    b148_expect_rejected "$single_quote_push_mutant" \
      "B-148 single-quoted self-trigger mutation is detected" "push does not cover"
    b148_expect_rejected "$single_quote_schedule_mutant" \
      "B-148 single-quoted schedule mutation is detected" "schedule trigger is missing"
    b148_expect_rejected "$single_quote_dispatch_mutant" \
      "B-148 single-quoted dispatch mutation is detected" "workflow_dispatch trigger is missing"
  fi
}

if [[ "${1:-}" == "--b148" ]]; then
  echo "B-148 workflow trigger mutation harness"
  b148_workflow_harness
  [[ "$fails" -eq 0 ]] || exit 1
  echo "B-148 harness: all cells passed"
  exit 0
fi

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

# B-146: explicit open-polarity declaration is rejected. The command is true
# in both fixtures; only the human declaration changes, so this pins the schema
# guard rather than accidentally testing command drift.
cell "done explicitly open is BROKEN" 1 BROKEN \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: done\nverify: "true"\nverify-means: open — still checking the defect\nlast-verified: 2026-08-23\n```\n')"
cell "done legacy prose passes" 0 CONFIRMED \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: done\nverify: "true"\nverify-means: test fixture\nlast-verified: 2026-08-23\n```\n')"
cell "done with an inversion declaration passes" 0 CONFIRMED \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: done\nverify: "true"\nverify-means: done — inverted regression guard\nlast-verified: 2026-08-23\n```\n')"
cell "parked with an explicit open marker remains legitimate" 0 CONFIRMED \
  "$(printf '### B-001 — fixture\n\n```backlog\nid: B-001\nrepo: corelink-server\nowner: tl\nstatus: parked\nverify: "true"\nverify-means: open — still waiting on the owner\nlast-verified: 2026-08-23\n```\n')"

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

b148_workflow_harness

echo
if [[ "$fails" -eq 0 ]]; then echo "backlog gate: all cells passed"; exit 0; fi
echo "backlog gate: $fails cell(s) failed"; exit 1
