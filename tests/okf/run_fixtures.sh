#!/usr/bin/env bash
# =============================================================================
# run_fixtures.sh — self-running acceptance suite for scripts/validate_okf.py
#
# This is the MECHANICAL PROOF of the contract's done-oracle (§4): "validator
# completeness = each check has >=1 known-BAD fixture it catches AND the
# known-GOOD golden bundle passes all checks."
#
#   - GOOD bundle  -> must exit 0 with the valid banner.
#   - Each bad/<CHECK>/ -> must exit non-zero AND surface its expected check ID.
#   - C5b (PR-diff dependent) -> proven by a throwaway git repo, not a static
#     bundle (see tests/okf/fixtures/bad/C5b/README.md).
#
# Deps: bash + git + python3 (PyYAML needed only for the C10/C10b manifest
# fixtures, matching the gate; CI installs it).
#
# Exit 0 only if every fixture behaves exactly as specified.
# =============================================================================
set -uo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"
VAL="scripts/validate_okf.py"
FIX="tests/okf/fixtures"
NONE="/nonexistent-okf-manifest.yaml"   # forces C10/C10b to skip-with-warning

pass=0
total=0
misses=()

ok()   { echo "  PASS  $1"; pass=$((pass + 1)); }
miss() { echo "  MISS  $1"; misses+=("$2"); }

# --- GOOD bundle: must pass everything (default invocation, no manifest) ------
assert_good() {
  total=$((total + 1))
  local out rc
  out="$(python3 "$VAL" --bundle "$FIX/good" --manifest "$NONE" 2>&1)"
  rc=$?
  if [ $rc -eq 0 ] && printf '%s' "$out" | grep -q 'OKF-CoreLink profile valid'; then
    ok "good bundle exits 0 (valid)"
  else
    miss "good bundle (exit $rc): $(printf '%s' "$out" | tail -1)" "good"
  fi
}

# --- A bad fixture: must exit non-zero AND fire its expected check ID ----------
# usage: assert_bad <label> <expected-check-id> <validator args...>
assert_bad() {
  local label="$1" exp="$2"
  shift 2
  total=$((total + 1))
  local out rc fired
  out="$(python3 "$VAL" "$@" 2>&1)"
  rc=$?
  if [ $rc -ne 0 ] && printf '%s\n' "$out" | grep -q "^\[$exp\]"; then
    fired="$(printf '%s\n' "$out" | grep '^\[' | tr -d '[]' | tr '\n' ' ')"
    ok "$label fires [$exp]   (all fired: ${fired%% })"
  else
    miss "$label (exit $rc, expected [$exp]; fired: $(printf '%s\n' "$out" | grep '^\[' | tr '\n' ' '))" "$label"
  fi
}

# --- C5b: real git two-revision harness ---------------------------------------
# Builds a temp repo, commits a concept, then a commit that bumps ONLY the
# checkpoint_sha line (body byte-identical) -> [C5b] MUST fire. Negative control:
# a commit that also edits the body -> [C5b] MUST NOT fire.
assert_c5b() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    printf 'alpha\nbeta\ngamma\n' > src.txt
    git add src.txt
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"

    mkdir -p docs/knowledge/auth
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [x](/auth/x.md)
EOF
    cat > docs/knowledge/auth/x.md <<EOF
---
type: "AuthMechanism"
title: "Phantom reconcile (git harness)"
description: "body identical across the SHA bump."
source_files:
  - "src.txt"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Phantom reconcile (git harness)

Lead paragraph held byte-identical across the checkpoint bump.

# How it works
- the source anchor line (\`src.txt:1\`).

# Invariants
- the anchor stays present (\`src.txt:1\`).

# Citations
1. \`src.txt:1\` — the anchor.
EOF
    git add -A
    git commit -q -m A
    sha_a="$(git rev-parse HEAD)"

    # PR commit B: bump ONLY the checkpoint_sha line, body unchanged.
    sed -i.bak "s/$sha0/$sha_a/" docs/knowledge/auth/x.md && rm -f docs/knowledge/auth/x.md.bak
    git add -A
    git commit -q -m B

    echo "POS" > "$tmp/.pos"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$sha_a" > "$tmp/.pos_out" 2>&1 || true

    # Negative control commit C: bump checkpoint AND change a body line.
    sed -i.bak "s/Lead paragraph held byte-identical/Lead paragraph genuinely reconciled/" docs/knowledge/auth/x.md && rm -f docs/knowledge/auth/x.md.bak
    git add -A
    git commit -q -m C
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$sha_a" > "$tmp/.neg_out" 2>&1 || true
  )

  # positive: [C5b] present
  total=$((total + 1))
  if grep -q '^\[C5b\]' "$tmp/.pos_out" 2>/dev/null; then
    ok "C5b git-harness: SHA bump w/o body edit fires [C5b]"
  else
    miss "C5b git-harness positive (no [C5b]): $(tail -1 "$tmp/.pos_out" 2>/dev/null)" "C5b-pos"
  fi
  # negative control: [C5b] absent
  total=$((total + 1))
  if grep -q '^\[C5b\]' "$tmp/.neg_out" 2>/dev/null; then
    miss "C5b negative control fired [C5b] on a real body edit" "C5b-neg"
  else
    ok "C5b git-harness: real body edit does NOT fire [C5b] (negative control)"
  fi
  rm -rf "$tmp"
}

# --- C5: real git two-revision freshness harness ------------------------------
# C5 freshness is also a two-tree diff (checkpoint_sha..HEAD), so a static bundle
# would have to hardcode a host-repo commit SHA — exactly the non-portable trap
# that bit the old tests/okf/fixtures/bad/C5/ bundle. We build a throwaway repo:
# commit A defines a file, the concept cites line 2 with checkpoint_sha=A, then
# commit B CHANGES line 2 -> [C5] MUST fire. Negative control: a concept citing
# line 5 (unchanged between A and HEAD) -> [C5] MUST NOT fire (anti-churn).
assert_c5() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    printf 'line1\nline2-original\nline3\nline4\nline5-stable\nline6\n' > data.txt
    git add data.txt
    git commit -q -m base
    sha_a="$(git rev-parse HEAD)"

    mkdir -p docs/knowledge/ops
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [x](/ops/x.md)
EOF
    # POSITIVE concept: cites line 2 (which will change at commit B).
    write_concept() {  # $1 = cited line number
      cat > docs/knowledge/ops/x.md <<EOF
---
type: "Runbook"
title: "Freshness anchor (hermetic)"
description: "cites a specific line of data.txt."
source_files:
  - "data.txt"
checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# Freshness anchor (hermetic)

Lead paragraph for the hermetic C5 case.

# How it works
- the cited anchor (\`data.txt:$1\`).

# Invariants
- the cited anchor is reconciled (\`data.txt:$1\`).

# Citations
1. \`data.txt:$1\` — the anchor under test.
EOF
    }
    write_concept 2
    git add -A
    git commit -q -m A

    # commit B: change ONLY line 2 (the cited line).
    sed -i.bak 's/line2-original/line2-CHANGED/' data.txt && rm -f data.txt.bak
    git add -A
    git commit -q -m B
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.pos_out" 2>&1 || true

    # negative control: re-cite line 5 (unchanged between A and HEAD).
    write_concept 5
    git add -A
    git commit -q -m neg
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.neg_out" 2>&1 || true
  )

  total=$((total + 1))
  if grep -q '^\[C5\]' "$tmp/.pos_out" 2>/dev/null; then
    ok "C5 git-harness: changed cited line fires [C5]"
  else
    miss "C5 git-harness positive (no [C5]): $(tail -1 "$tmp/.pos_out" 2>/dev/null)" "C5-pos"
  fi
  total=$((total + 1))
  if grep -q '^\[C5\]' "$tmp/.neg_out" 2>/dev/null; then
    miss "C5 negative control fired [C5] on an unchanged cited line" "C5-neg"
  else
    ok "C5 git-harness: unchanged cited line does NOT fire [C5] (negative control)"
  fi
  rm -rf "$tmp"
}

# --- C5 POSITION-SHIFT: the blind spot the content-anchor closes ---------------
# Proves the hardening. Commit A defines a file; the concept cites lines 3-4
# (content "cite-line-a"/"cite-line-b") with checkpoint_sha=A. Commit B inserts
# TWO lines ABOVE the cited range — the authored content slides to lines 5-6 but
# the cite still says 3-4. The OLD two-tree-diff ∩ cited-range check saw the
# insertion hunk only at new-lines {1,2}, which does NOT intersect [3,4] -> it
# reported 0-stale (the blind spot). The NEW content-anchor check compares the
# CONTENT at lines 3-4 (was "cite-line-a/b", now the shifted-down original head)
# and MUST fire [C5]. Negative control: a file left untouched between A and HEAD
# must NOT fire (its blob is identical -> trivially fresh).
assert_c5_shift() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # cited content lives at lines 3-4 at checkpoint A.
    printf 'head1\nhead2\ncite-line-a\ncite-line-b\ntail1\n' > shift.txt
    # an untouched companion file (negative control: blob identical A..HEAD).
    printf 'stable1\nstable2\nstable3\n' > stable.txt
    git add shift.txt stable.txt
    git commit -q -m base
    sha_a="$(git rev-parse HEAD)"

    mkdir -p docs/knowledge/ops
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [shift](/ops/shift.md)
- [stable](/ops/stable.md)
EOF
    cat > docs/knowledge/ops/shift.md <<EOF
---
type: "Runbook"
title: "Position-shift anchor (hermetic)"
description: "cites a byte-identical range that gets shifted DOWN by an insertion above."
source_files:
  - "shift.txt"
checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# Position-shift anchor (hermetic)

Lead paragraph for the hermetic position-shift case.

# How it works
- the cited anchor block (\`shift.txt:3-4\`).

# Invariants
- the cited anchor block is reconciled (\`shift.txt:3-4\`).

# Citations
1. \`shift.txt:3-4\` — the anchor content under test.
EOF
    cat > docs/knowledge/ops/stable.md <<EOF
---
type: "Runbook"
title: "Stable anchor (negative control)"
description: "cites a file untouched between checkpoint and HEAD."
source_files:
  - "stable.txt"
checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# Stable anchor (negative control)

Lead paragraph for the untouched-file control.

# How it works
- the untouched anchor (\`stable.txt:2\`).

# Invariants
- the untouched anchor holds (\`stable.txt:2\`).

# Citations
1. \`stable.txt:2\` — unchanged between A and HEAD.
EOF
    git add -A
    git commit -q -m A

    # commit B: insert TWO lines ABOVE the cited range in shift.txt ONLY.
    # cited content "cite-line-a/b" moves from lines 3-4 to lines 5-6; the cite
    # still points at lines 3-4, which now hold the shifted-down head lines.
    printf 'INSERTED-1\nINSERTED-2\nhead1\nhead2\ncite-line-a\ncite-line-b\ntail1\n' > shift.txt
    git add -A
    git commit -q -m B

    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.shift_out" 2>&1 || true

    # Demonstrate the OLD diff-∩-range check would NOT have flagged it: the only
    # changed HEAD-side new-lines are {1,2}, disjoint from the cited range [3,4].
    git diff --unified=0 "$sha_a" HEAD -- shift.txt | grep '^@@' > "$tmp/.shift_hunks" 2>&1 || true
  )

  # positive: the content-anchor C5 fires on the pure position-shift AND the STALE
  # offender line NAMES the specific drifted cite (not just "some [C5] fired").
  total=$((total + 1))
  if grep -q '^\[C5\]' "$tmp/.shift_out" 2>/dev/null \
     && grep -q 'STALE: cited content `shift.txt:3-4`' "$tmp/.shift_out" 2>/dev/null; then
    ok "C5 position-shift: insertion-above slides cite off content -> fires [C5] naming shift.txt:3-4"
  else
    miss "C5 position-shift positive (no [C5] STALE naming shift.txt:3-4): $(tail -1 "$tmp/.shift_out" 2>/dev/null)" "C5-shift-pos"
  fi
  # corroboration: prove the OLD hunk ∩ cited-range check would have passed
  # (the only changed new-lines are 1,2 — disjoint from [3,4]) AND the untouched
  # file did NOT get flagged (no stable.txt in the stale output).
  total=$((total + 1))
  if grep -q '+1,2 @@' "$tmp/.shift_hunks" 2>/dev/null \
     && ! grep -q 'stable.txt' "$tmp/.shift_out" 2>/dev/null; then
    ok "C5 position-shift: old diff-∩-range would PASS (hunk @+1,2 ∌ [3,4]); untouched file not flagged"
  else
    miss "C5 position-shift corroboration (hunks: $(cat "$tmp/.shift_hunks" 2>/dev/null | tr '\n' ' '); stable flagged?)" "C5-shift-corr"
  fi
  rm -rf "$tmp"
}

# --- C5 INSERT-BELOW: the DISCRIMINATOR that proves content-anchor, not blob ----
# A blob-anchor (fire whenever the file changed at all) and a content-anchor
# (fire only when the CITED lines' content changed) agree on every test above —
# so none of them actually PROVE we are content-anchored. This one separates
# them. Commit A defines a file with cited content at lines 2-3; commit B inserts
# a line BELOW the cited range. The file BLOB changes (a blob-anchor WOULD fire),
# but the content at lines 2-3 is byte-identical -> the content-anchor MUST NOT
# fire [C5]. If this ever fires, C5 has silently regressed to a blob-anchor.
assert_c5_below() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # cited content lives at lines 2-3 at checkpoint A.
    printf 'head1\ncite-line-a\ncite-line-b\ntail1\ntail2\n' > below.txt
    git add below.txt
    git commit -q -m base
    sha_a="$(git rev-parse HEAD)"

    mkdir -p docs/knowledge/ops
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [below](/ops/below.md)
EOF
    cat > docs/knowledge/ops/below.md <<EOF
---
type: "Runbook"
title: "Insert-below discriminator (hermetic)"
description: "code inserted BELOW the cited range; the cited lines' content is unchanged."
source_files:
  - "below.txt"
checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# Insert-below discriminator (hermetic)

Lead paragraph for the insert-below content-anchor discriminator.

# How it works
- the cited anchor block (\`below.txt:2-3\`).

# Invariants
- the cited anchor block holds (\`below.txt:2-3\`).

# Citations
1. \`below.txt:2-3\` — content stays put; only a line BELOW it is inserted.
EOF
    git add -A
    git commit -q -m A

    # commit B: insert a line BELOW the cited range (after line 3). The blob
    # CHANGES but lines 2-3 (cite-line-a/cite-line-b) are byte-identical.
    printf 'head1\ncite-line-a\ncite-line-b\nINSERTED-BELOW\ntail1\ntail2\n' > below.txt
    git add -A
    git commit -q -m B
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.below_out" 2>&1 || true
    # record that the file blob genuinely changed A..HEAD (a blob-anchor WOULD fire).
    if git diff --quiet "$sha_a" HEAD -- below.txt; then echo "blobrc=0"; else echo "blobrc=1"; fi > "$tmp/.below_blobrc"
  )

  # NEGATIVE discriminator: cited content unchanged -> [C5] must NOT fire (bundle valid).
  total=$((total + 1))
  if ! grep -q '^\[C5\]' "$tmp/.below_out" 2>/dev/null \
     && grep -q 'OKF-CoreLink profile valid' "$tmp/.below_out" 2>/dev/null; then
    ok "C5 insert-below: cited content unchanged -> does NOT fire [C5] (content-anchor, not blob-anchor)"
  else
    miss "C5 insert-below discriminator fired [C5] or invalid bundle: $(tail -1 "$tmp/.below_out" 2>/dev/null)" "C5-below"
  fi
  # corroboration: the blob DID change, so a blob-anchor would have mis-fired here.
  total=$((total + 1))
  if grep -q 'blobrc=1' "$tmp/.below_blobrc" 2>/dev/null; then
    ok "C5 insert-below: file blob changed A..HEAD (a blob-anchor WOULD mis-fire; content-anchor did not)"
  else
    miss "C5 insert-below corroboration (blob unchanged? $(cat "$tmp/.below_blobrc" 2>/dev/null))" "C5-below-corr"
  fi
  rm -rf "$tmp"
}

# --- C5 ADDED-FILE: a cited source absent at checkpoint but present at HEAD ------
# The "incomparable -> conservatively stale" branch of cited_range_drifted. The
# concept cites added.txt with checkpoint_sha=A, but added.txt did NOT exist at A
# (it is introduced in the same commit as the concept). C3/C6 pass (the file is
# present in the working tree at HEAD), but C5 cannot anchor the cite to any
# checkpoint content -> MUST fire [C5] naming added.txt.
assert_c5_added_file() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # checkpoint A: added.txt does NOT exist yet.
    printf 'placeholder\n' > other.txt
    git add other.txt
    git commit -q -m base
    sha_a="$(git rev-parse HEAD)"

    mkdir -p docs/knowledge/ops
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [added](/ops/added.md)
EOF
    # added.txt is introduced HERE (so it is absent at the checkpoint sha_a).
    printf 'newly-added-line\nsecond\n' > added.txt
    cat > docs/knowledge/ops/added.md <<EOF
---
type: "Runbook"
title: "Added-file anchor (hermetic)"
description: "cites a file that did not exist at the checkpoint commit."
source_files:
  - "added.txt"
checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# Added-file anchor (hermetic)

Lead paragraph for the added-file (incomparable) C5 branch.

# How it works
- the cited anchor (\`added.txt:1\`).

# Invariants
- the cited anchor holds (\`added.txt:1\`).

# Citations
1. \`added.txt:1\` — absent at the checkpoint, present at HEAD.
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.added_out" 2>&1 || true
  )

  # positive: file absent at checkpoint -> incomparable -> [C5] naming added.txt.
  total=$((total + 1))
  if grep -q '^\[C5\]' "$tmp/.added_out" 2>/dev/null \
     && grep -q 'added.txt:1' "$tmp/.added_out" 2>/dev/null; then
    ok "C5 added-file: cite absent at checkpoint, present at HEAD -> fires [C5] naming added.txt"
  else
    miss "C5 added-file positive (no [C5] naming added.txt): $(tail -1 "$tmp/.added_out" 2>/dev/null)" "C5-added"
  fi
  rm -rf "$tmp"
}

echo "OKF-CoreLink validator acceptance suite"
echo "---------------------------------------"
assert_good

# Bad fixtures using default invocation (manifest skipped).
assert_bad C1  C1  --bundle "$FIX/bad/C1"  --manifest "$NONE"
assert_bad C2  C2  --bundle "$FIX/bad/C2"  --manifest "$NONE"
assert_bad C3  C3  --bundle "$FIX/bad/C3"  --manifest "$NONE"
assert_bad C4  C4  --bundle "$FIX/bad/C4"  --manifest "$NONE"
assert_c5
assert_c5_shift
assert_c5_below
assert_c5_added_file
assert_c5b
assert_bad C6  C6  --bundle "$FIX/bad/C6"  --manifest "$NONE"
assert_bad C6b C6b --bundle "$FIX/bad/C6b" --manifest "$NONE"
assert_bad C6c C6c --bundle "$FIX/bad/C6c" --manifest "$NONE"
assert_bad C7  C7  --bundle "$FIX/bad/C7"  --manifest "$NONE"
assert_bad C8  C8  --bundle "$FIX/bad/C8"  --manifest "$NONE"
assert_bad C9  C9  --bundle "$FIX/bad/C9"  --manifest "$NONE"
# Manifest checks need their fixture manifest; isolate C10 from C10b via surface root.
assert_bad C10  C10  --bundle "$FIX/bad/C10"  --manifest "$FIX/bad/C10/manifest.yaml"  --surface-root "$FIX/bad/C10/empty-surface"
assert_bad C10b C10b --bundle "$FIX/bad/C10b" --manifest "$FIX/bad/C10b/manifest.yaml" --surface-root "$FIX/bad/C10b/surface"

echo "---------------------------------------"
if [ ${#misses[@]} -eq 0 ]; then
  echo "✅ $pass/$total fixtures behaved as specified"
  exit 0
else
  echo "⛔ ${#misses[@]} fixture(s) misbehaved: ${misses[*]}"
  echo "   ($pass/$total passed)"
  exit 1
fi
