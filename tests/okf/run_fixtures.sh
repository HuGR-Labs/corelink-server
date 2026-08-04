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

# --- C10b seed_from GROUNDING cross-check (fix #3) -----------------------------
# Hermetic git repo so the concept can cite REAL (repo-local) files and C3/C6
# pass. Two surface FILES exist under the container-src walk: cited_handler.rs is
# CITED by the concept (grounded -> covered), uncited_handler.rs is named in the
# concept's seed_from but NEVER cited (self-certifying -> NOT covered). Proves:
#  POSITIVE  — [C10b] fires and NAMES the uncited file (self-cert rejected);
#  NEGATIVE  — the SAME run does NOT name the cited file (grounded seed covered);
#  also exercises the RECURSIVE container-src walk into src/storage/ (a file
#  dropped there is enumerated -> the storage-subdir enumeration is proven).
assert_seed_selfcert() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # surface tree the container-src walk enumerates (src/*.rs + src/storage/*.rs).
    mkdir -p crates/corelink-container/src/storage
    printf 'fn cited_handler() {}\n'   > crates/corelink-container/src/cited_handler.rs
    printf 'fn uncited_handler() {}\n' > crates/corelink-container/src/uncited_handler.rs
    printf 'fn region_lookup() {}\n'   > crates/corelink-container/src/storage/region_map.rs

    mkdir -p docs/knowledge/compliance
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [x](/compliance/x.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"

    # concept grounds (cites) ONLY cited_handler.rs.
    cat > docs/knowledge/compliance/x.md <<EOF
---
type: "ComplianceControl"
title: "Seed grounding cross-check (hermetic)"
description: "cites cited_handler.rs only; the manifest also seeds uncited_handler.rs."
source_files:
  - "crates/corelink-container/src/cited_handler.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Seed grounding cross-check (hermetic)

Lead paragraph: the concept grounds only the cited handler.

# How it works
- the grounded handler (\`crates/corelink-container/src/cited_handler.rs:1\`).

# Invariants
- the grounded handler stays present (\`crates/corelink-container/src/cited_handler.rs:1\`).

# Citations
1. \`crates/corelink-container/src/cited_handler.rs:1\` — the ONLY grounded file.
EOF
    # manifest seeds BOTH handlers + excludes the storage file (focus the test on
    # the cited-vs-uncited discrimination). uncited_handler is self-certifying.
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes:
  - surface: "crates/corelink-container/src/storage/region_map.rs"
    reason: "fixture: data-table, excluded so the assertion focuses on the cited-vs-uncited seed discrimination."
candidates:
  - id: "compliance/x"
    type: "ComplianceControl"
    status: "active"
    seed_from:
      - "crates/corelink-container/src/cited_handler.rs"
      - "crates/corelink-container/src/uncited_handler.rs"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.sc_out" 2>&1 || true
  )

  # POSITIVE: [C10b] fires naming the UNCITED (self-certifying) file.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.sc_out" 2>/dev/null \
     && grep -q 'uncited_handler.rs' "$tmp/.sc_out" 2>/dev/null; then
    ok "C10b seed-grounding: uncited seed is self-certifying -> fires [C10b] naming uncited_handler.rs"
  else
    miss "C10b seed-grounding positive (no [C10b] naming uncited_handler.rs): $(tail -1 "$tmp/.sc_out" 2>/dev/null)" "C10b-selfcert-pos"
  fi
  # NEGATIVE (selectivity): the GROUNDED (cited) seed is covered -> NOT named.
  # Anchor the grep on the `/cited_handler.rs` path boundary so it does NOT
  # substring-match `uncited_handler.rs` (which legitimately IS flagged).
  total=$((total + 1))
  if ! grep -q '/cited_handler.rs' "$tmp/.sc_out" 2>/dev/null; then
    ok "C10b seed-grounding: grounded (cited) seed IS covered -> not flagged (cross-check is selective)"
  else
    miss "C10b seed-grounding negative: grounded cited_handler.rs was wrongly flagged" "C10b-selfcert-neg"
  fi
  rm -rf "$tmp"
}

# --- C10b storage-subdir RECURSIVE container-src enumeration --------------------
# A handler dropped in crates/corelink-container/src/storage/ (a subdir) must be
# enumerated by the recursive container-src walk. Hermetic: the file is seeded by
# NO concept and has NO exclude -> it MUST surface as a [C10b] silent gap naming
# the storage/ path. Proves the recursion into src/storage/ (the d1_http/r2_kv/
# r2_s3/region_map subtree the lead's rglob fix closed) is gated, not skipped.
assert_storage_subdir_enum() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src/storage
    printf 'fn cited_handler() {}\n'   > crates/corelink-container/src/cited_handler.rs
    printf 'fn d1_over_http() {}\n'     > crates/corelink-container/src/storage/d1_http.rs
    mkdir -p docs/knowledge/storage
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [s](/storage/s.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    cat > docs/knowledge/storage/s.md <<EOF
---
type: "StorageComponent"
title: "Storage enum (hermetic)"
description: "grounds cited_handler.rs only; the storage subdir file is unseeded."
source_files:
  - "crates/corelink-container/src/cited_handler.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Storage enum (hermetic)

Lead paragraph.

# How it works
- the grounded file (\`crates/corelink-container/src/cited_handler.rs:1\`).

# Invariants
- it stays present (\`crates/corelink-container/src/cited_handler.rs:1\`).

# Citations
1. \`crates/corelink-container/src/cited_handler.rs:1\` — grounded anchor.
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "storage/s"
    type: "StorageComponent"
    status: "active"
    seed_from:
      - "crates/corelink-container/src/cited_handler.rs"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.st_out" 2>&1 || true
  )
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.st_out" 2>/dev/null \
     && grep -q 'storage/d1_http.rs' "$tmp/.st_out" 2>/dev/null; then
    ok "C10b storage-subdir: recursive container-src walk enumerates src/storage/d1_http.rs -> [C10b]"
  else
    miss "C10b storage-subdir enumeration (no [C10b] naming storage/d1_http.rs): $(tail -1 "$tmp/.st_out" 2>/dev/null)" "C10b-storage-enum"
  fi
  rm -rf "$tmp"
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

# --- C5b orphaned-checkpoint REPAIR exemption ---------------------------------
# When the PREVIOUS checkpoint_sha (the base-ref version) is not an ancestor of
# the base ref, it is an ORPHANED pointer (a pre-merge branch tip git rewrote at
# merge; e.g. #677's G4 commit). Repointing it to the real main-history landing
# SHA is a mandatory C4 repair with byte-identical source — demanding a body
# edit would force the exact phantom edit C5b rejects. So: SHA bump w/o body edit
# MUST NOT fire [C5b] when the prev checkpoint is orphaned. The regular phantom
# case above (prev IS an on-base ancestor) still fires — this is the narrow carve.
assert_c5b_orphan_exempt() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  # A 40-hex that is NOT a resolvable commit in the throwaway repo -> git
  # `merge-base --is-ancestor` errors (128) -> treated as "not an ancestor"
  # (an orphan), exactly as a shallow CI clone lacks the real orphan object.
  local orphan="dead0000dead0000dead0000dead0000dead0000"
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
    # Commit A (the base-ref version) pins an ORPHANED checkpoint_sha.
    cat > docs/knowledge/auth/x.md <<EOF
---
type: "AuthMechanism"
title: "Orphan-checkpoint repair (git harness)"
description: "prev checkpoint is orphaned; body identical across the repair."
source_files:
  - "src.txt"
checkpoint_sha: "$orphan"
provenance: "AUTHORED"
---

# Orphan-checkpoint repair (git harness)

Lead paragraph held byte-identical across the checkpoint repair.

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

    # PR commit B: repoint the ORPHAN -> a real, reachable SHA (sha_a). Body
    # byte-identical. Because prev (orphan) is not an ancestor of base-ref,
    # [C5b] must be EXEMPT.
    sed -i.bak "s/$orphan/$sha_a/" docs/knowledge/auth/x.md && rm -f docs/knowledge/auth/x.md.bak
    git add -A
    git commit -q -m B

    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$sha_a" > "$tmp/.orphan_out" 2>&1 || true
  )
  total=$((total + 1))
  if grep -q '^\[C5b\]' "$tmp/.orphan_out" 2>/dev/null; then
    miss "C5b orphan-repair exemption: fired [C5b] on an orphaned-prev repair" "C5b-orphan"
  else
    ok "C5b git-harness: orphaned-prev checkpoint repair does NOT fire [C5b] (exemption)"
  fi
  rm -rf "$tmp"
}

# --- C4 SQUASH-ORPHAN tolerance + preserved freshness -------------------------
# A concept's checkpoint_sha is ORPHANED when its PR is squash/rebase-merged: git
# rewrites the pre-merge branch tip, so the SHA the PR wrote names a commit `main`
# cannot reach (and a fetch-depth:0 CI clone never fetched it). This is NOT drift —
# the squash landing preserves the cited source byte-for-byte. The old C4 hard-
# failed "checkpoint_sha not found in git history" on EVERY downstream PR until a
# manual #688/#690 repoint. This harness proves the durable fix:
#   (1) an orphaned 40-hex checkpoint with INTACT cited content must NOT fire [C4]
#       and the bundle must PASS (the false-positive is gone); and
#   (2) FRESHNESS CONTROL — with the SAME orphaned checkpoint, if the cited content
#       DRIFTS relative to the base ref, [C5] MUST still fire. The squash-orphan
#       tolerance must not blunt the anti-drift guarantee.
assert_c4_squash_orphan_tolerant() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  # A well-formed 40-hex that is NOT a resolvable commit in the throwaway repo ->
  # unreachable, exactly as a fetch-depth:0 CI clone lacks the squashed pre-merge tip.
  local orphan="beef0000beef0000beef0000beef0000beef0000"
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
    # Concept pins an ORPHANED checkpoint_sha; cited source (src.txt:1) is intact.
    cat > docs/knowledge/auth/x.md <<EOF
---
type: "AuthMechanism"
title: "Squash-orphan checkpoint (git harness)"
description: "checkpoint is orphaned but the cited source is intact."
source_files:
  - "src.txt"
checkpoint_sha: "$orphan"
provenance: "AUTHORED"
---

# Squash-orphan checkpoint (git harness)

Lead paragraph grounding the anchor line the concept rests on.

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

    # Downstream PR commit B: an UNRELATED change (does not touch src.txt). The
    # concept keeps its orphaned checkpoint — the recurring trap every later PR hit.
    printf 'note\n' > other.txt
    git add -A
    git commit -q -m B

    # base ref = sha_a (the fork point); cited src.txt:1 is intact there.
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$sha_a" > "$tmp/.intact_out" 2>&1 || true

    # FRESHNESS CONTROL commit C: DRIFT src.txt:1 without reconciling the concept
    # (checkpoint stays orphaned). C5 must still catch it against the base ref.
    printf 'ALPHA-DRIFTED\nbeta\ngamma\n' > src.txt
    git add -A
    git commit -q -m C
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$sha_a" > "$tmp/.drift_out" 2>&1 || true
  )

  # (1) orphan tolerated: no [C4] failure AND the intact-content run PASSED.
  total=$((total + 1))
  if ! grep -q '^\[C4\]' "$tmp/.intact_out" 2>/dev/null \
     && grep -q 'OKF-CoreLink profile valid' "$tmp/.intact_out" 2>/dev/null; then
    ok "C4 git-harness: squash-orphaned checkpoint w/ intact content does NOT fire [C4] (tolerated + passes)"
  else
    miss "C4 squash-orphan tolerance broken: $(tail -1 "$tmp/.intact_out" 2>/dev/null)" "C4-orphan-tol"
  fi

  # (2) freshness preserved: drift under the SAME orphaned checkpoint fires [C5].
  total=$((total + 1))
  if grep -q '^\[C5\]' "$tmp/.drift_out" 2>/dev/null; then
    ok "C4 git-harness: drift under an orphaned checkpoint STILL fires [C5] (freshness preserved)"
  else
    miss "C4 squash-orphan freshness broken (no [C5] on drift): $(tail -1 "$tmp/.drift_out" 2>/dev/null)" "C4-orphan-fresh"
  fi
  rm -rf "$tmp"
}

# --- C4 orphan with NO reachable base anchor -> FAIL CLOSED --------------------
# The squash-orphan tolerance re-anchors C5 freshness to the base ref. If the base
# ref is UNRESOLVABLE (or shares no common ancestor with HEAD), `base_rev_for_c5`
# is None and there is NO reachable anchor — freshness is UNVERIFIABLE. A gate whose
# job is to be unbypassable must FAIL CLOSED there, never silently skip C5. This
# harness drives an orphaned concept with an unresolvable --base-ref and asserts the
# gate FAILS with a freshness-unverifiable [C5]. (Latent in CI, which always
# resolves origin/<base_ref>; guards against the fail-open footgun regardless.)
assert_c4_orphan_no_base_fail_closed() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  local orphan="beef0000beef0000beef0000beef0000beef0000"
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
    # Orphaned checkpoint, cited content intact — would be TOLERATED if a base
    # anchor existed. Here the base ref is unresolvable, so it must fail closed.
    cat > docs/knowledge/auth/x.md <<EOF
---
type: "AuthMechanism"
title: "Orphan, no base anchor (git harness)"
description: "orphaned checkpoint with no resolvable base ref -> fail closed."
source_files:
  - "src.txt"
checkpoint_sha: "$orphan"
provenance: "AUTHORED"
---

# Orphan, no base anchor (git harness)

Lead paragraph grounding the anchor line the concept rests on.

# How it works
- the source anchor line (\`src.txt:1\`).

# Invariants
- the anchor stays present (\`src.txt:1\`).

# Citations
1. \`src.txt:1\` — the anchor.
EOF
    git add -A
    git commit -q -m A

    # An UNRESOLVABLE base ref -> resolve_base() None -> merge_base() None ->
    # base_rev_for_c5 None. Orphaned checkpoint + no anchor MUST fail closed.
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" \
      --base-ref "no-such-base-ref-zzzz" > "$tmp/.nobase_out" 2>&1 || true
  )
  total=$((total + 1))
  if grep -q '^\[C5\]' "$tmp/.nobase_out" 2>/dev/null \
     && grep -qi 'unverifiable' "$tmp/.nobase_out" 2>/dev/null; then
    ok "C4 git-harness: orphaned checkpoint w/ NO reachable base anchor FAILS CLOSED ([C5] unverifiable)"
  else
    miss "C4 fail-open: orphan + unresolvable base did NOT fail closed: $(tail -1 "$tmp/.nobase_out" 2>/dev/null)" "C4-orphan-failclosed"
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

# --- C5 reports EVERY drifted range, not just the first -----------------------
# C5 used to `break` after the FIRST drifted range per (concept, file), so its
# report was a LOWER BOUND: an author who fixed exactly what the gate printed
# could re-run and be handed the NEXT one, and a final `grep` was the only way to
# learn the real set. A single concept cites THREE disjoint ranges of one file
# (lines 2, 5 and 8); commit B rewrites all three. All three STALE lines MUST be
# reported in ONE run. The dedup half is proven too: the concept cites `:2` twice
# (once under `# How it works`, once under `# Citations`) and must still produce
# exactly ONE offender line for it.
assert_c5_all_ranges() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    printf 'l1\nl2-orig\nl3\nl4\nl5-orig\nl6\nl7\nl8-orig\nl9\n' > multi.txt
    git add multi.txt
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
- [multi](/ops/multi.md)
EOF
    cat > docs/knowledge/ops/multi.md <<EOF
---
type: "Runbook"
title: "Multi-range drift (hermetic)"
description: "cites three disjoint ranges of one file; all three drift at once."
source_files:
  - "multi.txt"
checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# Multi-range drift (hermetic)

Lead paragraph for the multi-range reporting case.

# How it works
- the first anchor (\`multi.txt:2\`).
- the second anchor (\`multi.txt:5\`).
- the third anchor (\`multi.txt:8\`).

# Invariants
- all three anchors are reconciled (\`multi.txt:2\`).

# Citations
1. \`multi.txt:2\` — first anchor (cited twice on purpose: dedup control).
2. \`multi.txt:5\` — second anchor.
3. \`multi.txt:8\` — third anchor.
EOF
    git add -A
    git commit -q -m A

    # commit B: rewrite ALL THREE cited lines in one change.
    printf 'l1\nl2-CHANGED\nl3\nl4\nl5-CHANGED\nl6\nl7\nl8-CHANGED\nl9\n' > multi.txt
    git add -A
    git commit -q -m B
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.multi_out" 2>&1 || true
  )

  # every drifted range is named in ONE run.
  total=$((total + 1))
  local n2 n5 n8
  n2="$(grep -c 'STALE: cited content `multi.txt:2-2`' "$tmp/.multi_out" 2>/dev/null || true)"
  n5="$(grep -c 'STALE: cited content `multi.txt:5-5`' "$tmp/.multi_out" 2>/dev/null || true)"
  n8="$(grep -c 'STALE: cited content `multi.txt:8-8`' "$tmp/.multi_out" 2>/dev/null || true)"
  if [ "$n2" -ge 1 ] && [ "$n5" -ge 1 ] && [ "$n8" -ge 1 ]; then
    ok "C5 all-ranges: three drifted ranges of one file are ALL reported in one run"
  else
    miss "C5 all-ranges (got :2=$n2 :5=$n5 :8=$n8, want >=1 each)" "C5-allranges"
  fi

  # dedup control: `multi.txt:2` is cited TWICE -> exactly ONE offender line.
  total=$((total + 1))
  if [ "$n2" -eq 1 ]; then
    ok "C5 all-ranges: a range cited twice is reported ONCE (dedup)"
  else
    miss "C5 all-ranges dedup (multi.txt:2 reported $n2 times, want 1)" "C5-allranges-dedup"
  fi
  rm -rf "$tmp"
}

# --- BLOB ANCHOR (§2.2 source_blobs): C5 keeps ALL its teeth ------------------
# The blob anchor replaces the commit id as C5's baseline. Its whole value is
# that a blob id survives rebase/squash/cherry-pick — so the ONE thing that must
# be proven is that nothing was traded away for that: the two things C5 exists to
# catch must both still fire when the baseline is a blob.
#   (a) in-range content EDIT under a blob anchor -> [C5]
#   (b) pure POSITION-SHIFT under a blob anchor (the cite slides off its authored
#       content with the cited lines never appearing in a diff hunk) -> [C5]
#   (c) negative control: untouched content under a blob anchor -> NO [C5]
# All three share one repo so the anchors are directly comparable.
assert_blob_anchor_teeth() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    printf 'head1\nEDIT-TARGET\ntail1\n'                > edit.txt
    printf 'head1\nhead2\nSHIFT-A\nSHIFT-B\ntail1\n'    > shift.txt
    printf 'stable1\nstable2\nstable3\n'                > stable.txt
    git add -A
    git commit -q -m base
    sha_a="$(git rev-parse HEAD)"
    blob_edit="$(git rev-parse HEAD:edit.txt)"
    blob_shift="$(git rev-parse HEAD:shift.txt)"
    blob_stable="$(git rev-parse HEAD:stable.txt)"

    mkdir -p docs/knowledge/ops
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [edit](/ops/edit.md)
- [shift](/ops/shift.md)
- [stable](/ops/stable.md)
EOF
    concept() {  # $1=slug $2=file $3=blob $4=cite-range
      cat > "docs/knowledge/ops/$1.md" <<EOF
---
type: "Runbook"
title: "Blob anchor — $1"
description: "blob-addressed concept citing $2:$4."
source_files:
  - "$2"
source_blobs:
  - "$2@$3"
checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# Blob anchor — $1

Lead paragraph for the blob-anchored $1 case.

# How it works
- the blob-anchored citation (\`$2:$4\`).

# Invariants
- the blob-anchored citation is reconciled (\`$2:$4\`).

# Citations
1. \`$2:$4\` — the anchor under test.
EOF
    }
    concept edit   edit.txt   "$blob_edit"   "2"
    concept shift  shift.txt  "$blob_shift"  "3-4"
    concept stable stable.txt "$blob_stable" "2"
    git add -A
    git commit -q -m A

    # commit B: (a) edit line 2 of edit.txt IN RANGE;
    #           (b) insert TWO lines ABOVE the cited 3-4 of shift.txt (pure shift);
    #           (c) leave stable.txt alone.
    printf 'head1\nEDIT-TARGET-CHANGED\ntail1\n'                        > edit.txt
    printf 'INS-1\nINS-2\nhead1\nhead2\nSHIFT-A\nSHIFT-B\ntail1\n'      > shift.txt
    git add -A
    git commit -q -m B
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.teeth_out" 2>&1 || true

    # prove (b) really is the diff-hunk blind spot: the changed HEAD-side lines
    # are {1,2}, disjoint from the cited range [3,4].
    git diff --unified=0 "$sha_a" HEAD -- shift.txt | grep '^@@' > "$tmp/.shift_hunks" 2>&1 || true
  )

  total=$((total + 1))
  if grep -q 'STALE: cited content `edit.txt:2-2`' "$tmp/.teeth_out" 2>/dev/null \
     && grep -q 'blob anchor' "$tmp/.teeth_out" 2>/dev/null; then
    ok "blob anchor: in-range content edit STILL fires [C5] (named as a blob anchor)"
  else
    miss "blob anchor in-range edit not caught: $(tail -1 "$tmp/.teeth_out" 2>/dev/null)" "blob-edit"
  fi

  total=$((total + 1))
  if grep -q 'STALE: cited content `shift.txt:3-4`' "$tmp/.teeth_out" 2>/dev/null \
     && grep -q '+1,2 @@' "$tmp/.shift_hunks" 2>/dev/null; then
    ok "blob anchor: pure POSITION-SHIFT STILL fires [C5] (hunk {1,2} disjoint from cited [3,4])"
  else
    miss "blob anchor position-shift not caught: $(tail -1 "$tmp/.teeth_out" 2>/dev/null)" "blob-shift"
  fi

  total=$((total + 1))
  if grep -q 'stable.txt' "$tmp/.teeth_out" 2>/dev/null; then
    miss "blob anchor negative control fired on untouched stable.txt" "blob-stable"
  else
    ok "blob anchor: untouched content does NOT fire [C5] (negative control)"
  fi
  rm -rf "$tmp"
}

# --- BLOB ANCHOR: the recurring re-anchor TAX, deleted ------------------------
# The tax this WP removes. A PR legitimately changes a source AND re-authors the
# concept against the new content, pinning checkpoint_sha at its own pre-merge
# tip. A rebase/squash rewrites that tip -> the commit is unreachable -> C4 warns
# and C5 silently re-anchors to the BASE REF, which still holds the OLD content
# -> every citation the branch legitimately moved reads STALE, and the author
# re-anchors again, and the next rebase undoes it again.
# Two concepts, same repo, same file, same cited line, same orphaned checkpoint:
# the legacy one MUST fire the false [C5]; the blob-addressed one MUST NOT.
assert_blob_anchor_survives_rewrite() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  local orphan="beef0000beef0000beef0000beef0000beef0000"
  (
    set -e
    cd "$tmp"
    git init -q -b main
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # BASE REF content (what a rebase re-anchors to).
    printf 'v1-cited-line\nfiller\n' > code.txt
    mkdir -p docs/knowledge/ops
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [legacy](/ops/legacy.md)
- [blobbed](/ops/blobbed.md)
EOF
    git add -A
    git commit -q -m base
    base_sha="$(git rev-parse HEAD)"

    # The PR: legitimately rewrites the cited line AND re-authors both concepts
    # against the NEW content. checkpoint_sha = the pre-merge tip a rebase kills.
    printf 'v2-cited-line\nfiller\n' > code.txt
    blob_v2="$(git hash-object code.txt)"
    concept() {  # $1=slug  $2=extra frontmatter lines
      cat > "docs/knowledge/ops/$1.md" <<EOF
---
type: "Runbook"
title: "Rebase survival — $1"
description: "cites code.txt:1, re-authored against v2."
source_files:
  - "code.txt"
$2checkpoint_sha: "$orphan"
provenance: "AUTHORED"
---

# Rebase survival — $1

Lead paragraph for the rebase-survival case.

# How it works
- the re-authored citation (\`code.txt:1\`).

# Invariants
- the re-authored citation is reconciled (\`code.txt:1\`).

# Citations
1. \`code.txt:1\` — v2 content, authored this PR.
EOF
    }
    concept legacy  ""
    concept blobbed "source_blobs:
  - \"code.txt@$blob_v2\"
"
    git add -A
    git commit -q -m "PR: rewrite code.txt + re-author both concepts"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$base_sha" > "$tmp/.rewrite_out" 2>&1 || true
  )

  # the tax, reproduced: the LEGACY concept reads STALE though nothing is stale.
  total=$((total + 1))
  if grep -q 'ops/legacy.md: STALE' "$tmp/.rewrite_out" 2>/dev/null \
     && grep -q 'base-ref anchor' "$tmp/.rewrite_out" 2>/dev/null; then
    ok "rebase tax reproduced: the COMMIT-anchored concept falsely reads STALE off the base ref"
  else
    miss "rebase tax not reproduced (legacy concept did not fire): $(tail -1 "$tmp/.rewrite_out" 2>/dev/null)" "blob-tax-repro"
  fi

  # the tax, deleted: the BLOB-anchored concept is clean under the same rewrite.
  total=$((total + 1))
  if grep -q 'ops/blobbed.md' "$tmp/.rewrite_out" 2>/dev/null; then
    miss "blob-anchored concept ALSO went stale under the rewrite: $(grep 'blobbed' "$tmp/.rewrite_out" | head -1)" "blob-tax-gone"
  else
    ok "rebase tax deleted: the BLOB-anchored concept is untouched by the orphaned checkpoint"
  fi
  rm -rf "$tmp"
}

# --- BLOB ANCHOR: the laundering residual, CLOSED -----------------------------
# validate_okf.py used to carry a ~40-line comment accepting this: a forged,
# well-formed, UNREACHABLE checkpoint_sha is indistinguishable from a genuine
# squash-orphan, so C5 re-anchors to the base ref — which ALREADY CONTAINS the
# landed drift — and the drifted citation reads fresh. Reproduced here on the
# legacy path (positive control, so the fixture proves the hole is real and not
# folklore), then shown CLOSED on the blob path: a forged blob id is not an
# orphan, it is an absent object, and it REDs [C4b] with no fallback.
# Both attackers also make a cosmetic body edit, which is all C5b requires.
assert_blob_launder_closed() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  local forged="beef0000beef0000beef0000beef0000beef0000"
  (
    set -e
    cd "$tmp"
    git init -q -b main
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    printf 'CITED-ORIGINAL\nfiller\n' > code.txt
    mkdir -p docs/knowledge/ops
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [legacy](/ops/legacy.md)
- [blobbed](/ops/blobbed.md)
EOF
    git add -A
    git commit -q -m base
    sha_a="$(git rev-parse HEAD)"
    blob_v1="$(git rev-parse HEAD:code.txt)"
    concept() {  # $1=slug $2=ckpt $3=extra-frontmatter $4=lead-paragraph
      cat > "docs/knowledge/ops/$1.md" <<EOF
---
type: "Runbook"
title: "Laundering — $1"
description: "cites code.txt:1 authored at v1."
source_files:
  - "code.txt"
$3checkpoint_sha: "$2"
provenance: "AUTHORED"
---

# Laundering — $1

$4

# How it works
- the citation under attack (\`code.txt:1\`).

# Invariants
- the citation is reconciled (\`code.txt:1\`).

# Citations
1. \`code.txt:1\` — authored against CITED-ORIGINAL.
EOF
    }
    concept legacy  "$sha_a" "" "Lead paragraph."
    concept blobbed "$sha_a" "source_blobs:
  - \"code.txt@$blob_v1\"
" "Lead paragraph."
    git add -A
    git commit -q -m "concepts authored at v1"

    # The DRIFT lands on the base branch. Both concepts are now genuinely stale.
    printf 'CITED-DRIFTED-ON-MAIN\nfiller\n' > code.txt
    git add -A
    git commit -q -m "drift lands on main"
    base_sha="$(git rev-parse HEAD)"
    git checkout -q -b pr
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$base_sha" > "$tmp/.honest_out" 2>&1 || true

    # THE ATTACK: forge an unreachable anchor instead of reconciling, plus a
    # cosmetic body edit (all C5b asks for).
    concept legacy  "$forged" "" "Lead paragraph, lightly reworded."
    concept blobbed "$sha_a" "source_blobs:
  - \"code.txt@$forged\"
" "Lead paragraph, lightly reworded."
    git add -A
    git commit -q -m "launder"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$base_sha" > "$tmp/.attack_out" 2>&1 || true

    # THE UN-MIGRATION dodge: drop the blob anchor to fall back onto the legacy
    # carve-out. The C4c ratchet must refuse it.
    concept blobbed "$forged" "" "Lead paragraph, reworded again."
    git add -A
    git commit -q -m "un-migrate"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" --base-ref "$base_sha" > "$tmp/.unmigrate_out" 2>&1 || true
  )

  # honest control: BOTH concepts are stale before the attack.
  total=$((total + 1))
  if grep -q 'ops/legacy.md: STALE' "$tmp/.honest_out" 2>/dev/null \
     && grep -q 'ops/blobbed.md: STALE' "$tmp/.honest_out" 2>/dev/null; then
    ok "laundering control: real drift fires [C5] on BOTH the commit- and blob-anchored concept"
  else
    miss "laundering control (expected both STALE): $(tail -1 "$tmp/.honest_out" 2>/dev/null)" "launder-ctl"
  fi

  # positive control: the hole is REAL on the legacy path (forged SHA -> silent).
  total=$((total + 1))
  if grep -q 'ops/legacy.md' "$tmp/.attack_out" 2>/dev/null; then
    miss "legacy laundering unexpectedly caught — the residual this WP closes is not reproducible" "launder-legacy"
  else
    ok "laundering residual reproduced: a forged unreachable checkpoint_sha silences [C5] on the LEGACY path"
  fi

  # the closure: the SAME forgery on the blob path REDs [C4b], no fallback.
  total=$((total + 1))
  if grep -q '^\[C4b\]' "$tmp/.attack_out" 2>/dev/null \
     && grep -q 'ops/blobbed.md: `source_blobs` anchor `code.txt@' "$tmp/.attack_out" 2>/dev/null; then
    ok "laundering CLOSED: a forged blob anchor is an absent object -> [C4b], never a base-ref re-anchor"
  else
    miss "forged blob anchor did not RED [C4b]: $(tail -1 "$tmp/.attack_out" 2>/dev/null)" "launder-blob"
  fi

  # the closure is not merely MOVED: un-migrating to reach the carve-out REDs.
  total=$((total + 1))
  if grep -q '^\[C4c\]' "$tmp/.unmigrate_out" 2>/dev/null \
     && grep -q 'ops/blobbed.md' "$tmp/.unmigrate_out" 2>/dev/null; then
    ok "laundering not MOVED: dropping the blob anchor to regain the carve-out fires [C4c] (ratchet)"
  else
    miss "un-migration dodge not caught by [C4c]: $(tail -1 "$tmp/.unmigrate_out" 2>/dev/null)" "launder-ratchet"
  fi
  rm -rf "$tmp"
}

# --- C4b: the source_blobs entry itself is validated --------------------------
# A shape check with no tolerance: malformed entry, an anchor for a path that is
# not a declared source, a duplicate anchor, and a COMMIT id pasted where a blob
# id belongs (the likeliest honest mistake — it resolves as an object, so only a
# type check rejects it).
assert_c4b_entry_shapes() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    printf 'anchor-line\nfiller\n' > code.txt
    printf 'other\n' > other.txt
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
    git add -A
    git commit -q -m base
    sha_a="$(git rev-parse HEAD)"
    blob="$(git rev-parse HEAD:code.txt)"
    concept() {  # $1 = the source_blobs block body
      cat > docs/knowledge/ops/x.md <<EOF
---
type: "Runbook"
title: "C4b entry shapes"
description: "exercises source_blobs entry validation."
source_files:
  - "code.txt"
source_blobs:
$1checkpoint_sha: "$sha_a"
provenance: "AUTHORED"
---

# C4b entry shapes

Lead paragraph for the entry-shape case.

# How it works
- the anchor (\`code.txt:1\`).

# Invariants
- the anchor holds (\`code.txt:1\`).

# Citations
1. \`code.txt:1\` — the anchor.
EOF
    }
    # (1) malformed: no @sha
    concept "  - \"code.txt\"
"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.malformed" 2>&1 || true
    # (2) anchors a path not in source_files
    concept "  - \"other.txt@$blob\"
"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.undeclared" 2>&1 || true
    # (3) duplicate anchor for the same path
    concept "  - \"code.txt@$blob\"
  - \"code.txt@$blob\"
"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.dupe" 2>&1 || true
    # (4) a COMMIT id where a blob id belongs (resolves as an object, wrong type)
    concept "  - \"code.txt@$sha_a\"
"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.commitid" 2>&1 || true
    # (5) negative control: a correct anchor passes cleanly
    concept "  - \"code.txt@$blob\"
"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.good" 2>&1 || true
  )
  local case
  for case in malformed undeclared dupe commitid; do
    total=$((total + 1))
    if grep -q '^\[C4b\]' "$tmp/.$case" 2>/dev/null; then
      ok "C4b: $case source_blobs entry fires [C4b]"
    else
      miss "C4b $case not caught: $(tail -1 "$tmp/.$case" 2>/dev/null)" "C4b-$case"
    fi
  done
  total=$((total + 1))
  if grep -q 'OKF-CoreLink profile valid' "$tmp/.good" 2>/dev/null; then
    ok "C4b: a well-formed, resolvable blob anchor passes (negative control)"
  else
    miss "C4b negative control failed: $(tail -1 "$tmp/.good" 2>/dev/null)" "C4b-good"
  fi
  rm -rf "$tmp"
}

# --- C4b REACHABILITY: presence in the clone is a TIMING artifact --------------
# The false-green this closes, reproduced end-to-end. A concept authored mid-PR
# anchors an INTERMEDIATE commit's blob; a later commit on the same branch
# supersedes it, so the squash-merge lands the branch TIP's blob and the anchored
# one never reaches the mainline. Its only ref is the PR head, which GitHub
# auto-DELETES at merge — but the `push:main` run clones seconds after the merge
# and still resolves the object, so a presence-only C4b passes; every later clone
# lacks it and the identical check REDs. Here the abandoned branch is deleted
# while its loose object lingers (exactly the merge-time clone), so a
# presence-only check would PASS and only the reachability half can fire.
#
# Positive control in the same repo: an OLDER blob that IS on HEAD's history must
# still pass — the anchor names authoring-time content, and deciding whether the
# CITED LINES have since drifted is C5's job, not C4b's. Without this control the
# check could collapse into "the anchor must equal HEAD", which makes C5 vacuous.
assert_c4b_blob_must_be_reachable() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    printf 'anchor-line\nfiller\n' > code.txt
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
    git add -A
    git commit -q -m base
    local_branch="$(git rev-parse --abbrev-ref HEAD)"
    blob_v1="$(git rev-parse HEAD:code.txt)"

    # An ABANDONED intermediate commit: committed on a side branch, never merged,
    # branch deleted. The loose blob survives in this clone (the merge-time race)
    # but no commit reachable from HEAD ever carried it.
    git checkout -q -b abandoned
    printf 'anchor-line\nfiller\nsuperseded-only-on-the-dead-branch\n' > code.txt
    git add code.txt
    git commit -q -m abandoned
    blob_dead="$(git rev-parse HEAD:code.txt)"
    git checkout -q "$local_branch"
    git branch -q -D abandoned

    # What actually LANDED: a second commit on the mainline. HEAD's blob differs
    # from blob_v1, so blob_v1 exercises the history walk, not the fast path.
    printf 'anchor-line\nfiller\nlanded\n' > code.txt
    git add code.txt
    git commit -q -m landed
    sha_head="$(git rev-parse HEAD)"

    concept() {  # $1 = the anchored blob id
      cat > docs/knowledge/ops/x.md <<EOF
---
type: "Runbook"
title: "C4b reachability"
description: "exercises the source_blobs reachability half of C4b."
source_files:
  - "code.txt"
source_blobs:
  - "code.txt@$1"
checkpoint_sha: "$sha_head"
provenance: "AUTHORED"
---

# C4b reachability

Lead paragraph for the reachability case.

# How it works
- the anchor (\`code.txt:1\`).

# Invariants
- the anchor holds (\`code.txt:1\`).

# Citations
1. \`code.txt:1\` — the anchor.
EOF
    }
    # (1) present-but-unreachable: the abandoned intermediate blob.
    concept "$blob_dead"
    git cat-file -t "$blob_dead" > "$tmp/.dead_type" 2>&1 || true
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.unreachable" 2>&1 || true
    # (2) older-but-reachable: a real earlier version on HEAD's own history.
    concept "$blob_v1"
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.older" 2>&1 || true
  )
  # The premise of the whole case: the object really IS in the clone, so a
  # presence-only C4b could not have fired. If this ever stops holding the test
  # below would pass for the WRONG reason.
  total=$((total + 1))
  if grep -qx 'blob' "$tmp/.dead_type" 2>/dev/null; then
    ok "C4b reachability: premise holds — the abandoned blob IS present in the clone"
  else
    miss "C4b reachability premise broken: abandoned object is not a present blob" "C4b-reach-premise"
  fi
  total=$((total + 1))
  if grep -q '^\[C4b\]' "$tmp/.unreachable" 2>/dev/null \
     && grep -q 'reachable from HEAD' "$tmp/.unreachable" 2>/dev/null; then
    ok "C4b: a present-but-UNREACHABLE blob anchor fires [C4b] (merge-time false-green closed)"
  else
    miss "C4b unreachable anchor not caught: $(tail -1 "$tmp/.unreachable" 2>/dev/null)" "C4b-unreachable"
  fi
  total=$((total + 1))
  if grep -q 'OKF-CoreLink profile valid' "$tmp/.older" 2>/dev/null; then
    ok "C4b: an OLDER blob still on HEAD's history passes (age is C5's business, not C4b's)"
  else
    miss "C4b over-strict — a reachable older anchor failed: $(tail -1 "$tmp/.older" 2>/dev/null)" "C4b-older"
  fi
  rm -rf "$tmp"
}

# --- gate v5 #2: module-parent grounding REMOVED -------------------------------
# A concept that cites the module-ROOT file `routes.rs` (the `mod routes;`
# declaration site) but NOT the handler `routes/sub_handler.rs` must NO LONGER
# auto-cover that handler. The handler is in the concept's seed_from but is never
# cited -> with the v5 removal of relation (d) it is self-certifying -> [C10b]
# MUST fire naming sub_handler.rs. Negative control: a handler the concept DOES
# cite stays covered (not flagged). Proves the module-parent self-cert hole closed.
assert_module_parent_removed() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src/routes
    printf 'pub mod sub_handler;\npub mod cited_handler;\n' > crates/corelink-container/src/routes.rs
    printf 'fn sub() {}\n'    > crates/corelink-container/src/routes/sub_handler.rs
    printf 'fn cited() {}\n'  > crates/corelink-container/src/routes/cited_handler.rs
    mkdir -p docs/knowledge/planes
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [p](/planes/p.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # concept cites the module ROOT routes.rs + the cited_handler, NOT sub_handler.
    cat > docs/knowledge/planes/p.md <<EOF
---
type: "Plane"
title: "Module-parent removal (hermetic)"
description: "cites routes.rs (module root) + cited_handler.rs; sub_handler.rs is only seeded."
source_files:
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/routes/cited_handler.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Module-parent removal (hermetic)

Lead paragraph: the concept grounds the module root and one handler, not the sibling.

# How it works
- the module root (\`crates/corelink-container/src/routes.rs:1\`).
- the cited handler (\`crates/corelink-container/src/routes/cited_handler.rs:1\`).

# Invariants
- the module root holds (\`crates/corelink-container/src/routes.rs:1\`).

# Citations
1. \`crates/corelink-container/src/routes.rs:1\` — module root.
2. \`crates/corelink-container/src/routes/cited_handler.rs:1\` — the cited handler.
EOF
    # manifest seeds routes.rs + BOTH handlers. sub_handler is named but uncited;
    # the v5 removal of the X.rs-module-parent relation means routes.rs no longer
    # auto-grounds it -> it must surface as a silent gap.
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "planes/p"
    type: "Plane"
    status: "active"
    seed_from:
      - "crates/corelink-container/src/routes.rs"
      - "crates/corelink-container/src/routes/cited_handler.rs"
      - "crates/corelink-container/src/routes/sub_handler.rs"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.mp_out" 2>&1 || true
  )
  # POSITIVE: sub_handler.rs (module-parent only, uncited) is NOT covered -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.mp_out" 2>/dev/null \
     && grep -q 'routes/sub_handler.rs' "$tmp/.mp_out" 2>/dev/null; then
    ok "module-parent removed: a seeded-but-uncited routes/ handler under a cited routes.rs -> [C10b] naming sub_handler.rs"
  else
    miss "module-parent removal positive (no [C10b] naming sub_handler.rs): $(tail -1 "$tmp/.mp_out" 2>/dev/null)" "module-parent-pos"
  fi
  # NEGATIVE: the genuinely-cited handler is still covered (not flagged).
  total=$((total + 1))
  if ! grep -q '/cited_handler.rs' "$tmp/.mp_out" 2>/dev/null; then
    ok "module-parent removed: a genuinely-cited routes/ handler stays covered (selective)"
  else
    miss "module-parent removal negative: cited_handler.rs was wrongly flagged" "module-parent-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v5 #6: glob is SEGMENT-AWARE (a single `*` does not cross `/`) --------
# A real handler at `crates/.../routes/tests_helpers/realhandler.rs` is enumerated
# by the recursive routes walk. The manifest carries the existing broad-looking
# exclude `crates/corelink-container/src/routes/**/tests_*.rs`. Under the OLD
# fnmatch matcher `*` crossed `/`, so `**/tests_*.rs`-style patterns could swallow
# a path with `tests_` ANYWHERE. We prove the SEGMENT-aware matcher does NOT
# exclude a real handler whose DIRECTORY merely starts with `tests_`: the file is
# seeded by no concept and matches no segment-correct exclude -> [C10b] MUST fire
# naming it. Negative control: an actual `tests_foo.rs` inline-test module under a
# route subdir IS excluded by `routes/**/tests_*.rs` (segment-correct) -> NOT flagged.
assert_glob_segment_aware() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src/routes/tests_helpers
    mkdir -p crates/corelink-container/src/routes/audit_export
    printf 'fn anchor() {}\n'   > crates/corelink-container/src/routes/anchor.rs
    # a REAL handler whose DIRECTORY name starts with tests_ — must NOT be swallowed.
    printf 'fn real() {}\n'     > crates/corelink-container/src/routes/tests_helpers/realhandler.rs
    # a genuine inline-test module file — MUST be excluded by routes/**/tests_*.rs.
    printf 'mod t {}\n'         > crates/corelink-container/src/routes/audit_export/tests_proptest.rs
    mkdir -p docs/knowledge/planes
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [g](/planes/g.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    cat > docs/knowledge/planes/g.md <<EOF
---
type: "Plane"
title: "Glob segment-aware (hermetic)"
description: "grounds the anchor handler only."
source_files:
  - "crates/corelink-container/src/routes/anchor.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Glob segment-aware (hermetic)

Lead paragraph.

# How it works
- the anchor handler (\`crates/corelink-container/src/routes/anchor.rs:1\`).

# Invariants
- the anchor handler holds (\`crates/corelink-container/src/routes/anchor.rs:1\`).

# Citations
1. \`crates/corelink-container/src/routes/anchor.rs:1\` — grounded anchor.
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes:
  - surface: "crates/corelink-container/src/routes/**/tests_*.rs"
    reason: "fixture: inline test modules under a route subdir — test-only."
candidates:
  - id: "planes/g"
    type: "Plane"
    status: "active"
    seed_from:
      - "crates/corelink-container/src/routes/anchor.rs"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.gl_out" 2>&1 || true
  )
  # POSITIVE: the real handler under a tests_-prefixed DIR is NOT excluded -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.gl_out" 2>/dev/null \
     && grep -q 'tests_helpers/realhandler.rs' "$tmp/.gl_out" 2>/dev/null; then
    ok "glob segment-aware: a real handler under a tests_-prefixed DIR is NOT swallowed -> [C10b] names realhandler.rs"
  else
    miss "glob segment-aware positive (no [C10b] naming tests_helpers/realhandler.rs): $(tail -1 "$tmp/.gl_out" 2>/dev/null)" "glob-seg-pos"
  fi
  # NEGATIVE: a genuine tests_*.rs inline-test file IS excluded (segment-correct).
  total=$((total + 1))
  if ! grep -q 'audit_export/tests_proptest.rs' "$tmp/.gl_out" 2>/dev/null; then
    ok "glob segment-aware: a genuine tests_*.rs inline-test module stays excluded (segment-correct match)"
  else
    miss "glob segment-aware negative: tests_proptest.rs was wrongly flagged (exclude failed to match)" "glob-seg-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v6 #1: cluster-directory adoption does NOT auto-cover a container file --
# THE class fix. A `type: CrateCluster` concept seeds the WHOLE `crates/corelink-
# container` directory (the real `crates/container-platform` shape) and cites that
# crate dir, so clause (b) of `_concept_grounds` makes the directory a grounded
# seed. Before v6 the FILE branch of `is_covered` then auto-covered EVERY .rs under
# the crate via `rel.startswith(grounded_dir + '/')` — so a brand-new
# `routes/poison.rs` stayed GREEN (the recursive anti-shadow walk was dead). v6
# makes `crates/corelink-container/src/**` FILE-GRANULAR-STRICT: directory/cluster
# adoption no longer auto-covers an individual file there. We prove BOTH:
#  POSITIVE — a new uncited handler (routes/poison.rs) REDs [C10b] naming it;
#  NEGATIVE — the genuinely-cited handler under the SAME crate stays covered.
assert_cluster_adoption_strict() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src/routes
    printf 'fn cited() {}\n'   > crates/corelink-container/src/routes/cited_handler.rs
    # a NEW handler dropped beside the cited one — no per-file cite anywhere.
    printf 'fn poison() {}\n'  > crates/corelink-container/src/routes/poison.rs
    mkdir -p docs/knowledge/crates
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [c](/crates/c.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # a CrateCluster that ADOPTS the whole crate dir + cites ONE handler file.
    cat > docs/knowledge/crates/c.md <<EOF
---
type: "CrateCluster"
title: "Container cluster adoption (hermetic)"
description: "adopts the whole crate dir; cites only cited_handler.rs."
source_files:
  - "crates/corelink-container"
  - "crates/corelink-container/src/routes/cited_handler.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Container cluster adoption (hermetic)

Lead paragraph: the cluster adopts the crate dir and cites one handler.

# How it works
- the adopted crate dir (\`crates/corelink-container:1\` is NOT a file cite; use the handler) and the cited handler (\`crates/corelink-container/src/routes/cited_handler.rs:1\`).

# Invariants
- the cited handler holds (\`crates/corelink-container/src/routes/cited_handler.rs:1\`).

# Citations
1. \`crates/corelink-container/src/routes/cited_handler.rs:1\` — the one grounded handler.
EOF
    # manifest: the cluster seeds the whole crate dir (adoption). poison.rs is NOT
    # individually seeded and NOT excluded -> under v6 file-granular-strict it must
    # surface as a silent gap (directory adoption does not reach it).
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes:
  - surface: "crates/corelink-container/src/routes/cited_handler.rs"
    reason: "fixture: focus the assertion on poison.rs; this one is also genuinely cited."
candidates:
  - id: "crates/c"
    type: "CrateCluster"
    status: "active"
    seed_from:
      - "crates/corelink-container"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.ca_out" 2>&1 || true
  )
  # POSITIVE: the new uncited handler is NOT covered by the whole-crate adoption -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.ca_out" 2>/dev/null \
     && grep -q 'routes/poison.rs' "$tmp/.ca_out" 2>/dev/null; then
    ok "cluster-adoption strict: a new uncited container handler under a whole-crate-adopted dir -> [C10b] names poison.rs"
  else
    miss "cluster-adoption strict positive (no [C10b] naming routes/poison.rs): $(tail -1 "$tmp/.ca_out" 2>/dev/null)" "cluster-adopt-pos"
  fi
  # NEGATIVE (selectivity): the cited handler stays covered (here via its exclude) -> not a poison-style gap.
  total=$((total + 1))
  if ! grep -q 'routes/cited_handler.rs' "$tmp/.ca_out" 2>/dev/null; then
    ok "cluster-adoption strict: the genuinely-covered handler is not flagged (selective)"
  else
    miss "cluster-adoption strict negative: cited_handler.rs was wrongly flagged" "cluster-adopt-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v6 #2: testing exemption requires `testing/` dir AND `type: TestStrategy` -
# A SECURITY invariant filed under docs/knowledge/testing/ but grounded SOLELY on a
# test path must STILL fire [C6c] — the old placement-only exemption let it through.
# POSITIVE: a `type: SecurityControl` concept placed at testing/sneaky.md with a
# test-only Invariants cite fires [C6c]. NEGATIVE: a genuine `type: TestStrategy`
# concept at testing/legit.md with the same test-only cite is exempt (no [C6c]).
assert_testing_exemption_typed() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/x/tests
    printf 'fn anchor() {}\n'        > crates/x/anchor.rs
    printf '#[test] fn t() {}\n'     > crates/x/tests/iso_tests.rs
    mkdir -p docs/knowledge/testing
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # the BYPASS attempt: a SecurityControl placed under testing/, grounded ONLY on a test.
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [sneaky](/testing/sneaky.md)
- [legit](/testing/legit.md)
EOF
    cat > docs/knowledge/testing/sneaky.md <<EOF
---
type: "SecurityControl"
title: "Placement-bypass attempt (hermetic)"
description: "a security invariant filed under testing/ grounded only on a test."
source_files:
  - "crates/x/tests/iso_tests.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Placement-bypass attempt (hermetic)

Lead paragraph.

# How it works
- the test that 'enforces' it (\`crates/x/tests/iso_tests.rs:1\`).

# Invariants
- tenant isolation holds (\`crates/x/tests/iso_tests.rs:1\`).
EOF
    # the LEGIT control: a real TestStrategy under testing/ MAY ground on a test.
    cat > docs/knowledge/testing/legit.md <<EOF
---
type: "TestStrategy"
title: "Genuine test-harness concept (hermetic)"
description: "subject IS the harness; grounding on a test is correct."
source_files:
  - "crates/x/tests/iso_tests.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Genuine test-harness concept (hermetic)

Lead paragraph.

# How it works
- the harness test (\`crates/x/tests/iso_tests.rs:1\`).

# Invariants
- the harness asserts isolation (\`crates/x/tests/iso_tests.rs:1\`).
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest "$NONE" > "$tmp/.te_out" 2>&1 || true
  )
  # POSITIVE: the SecurityControl-under-testing/ bypass STILL fires [C6c] naming it.
  total=$((total + 1))
  if grep -q '^\[C6c\]' "$tmp/.te_out" 2>/dev/null \
     && grep -q 'testing/sneaky.md' "$tmp/.te_out" 2>/dev/null; then
    ok "testing exemption typed: a SecurityControl placed under testing/ with a test-only cite STILL fires [C6c]"
  else
    miss "testing exemption typed positive (no [C6c] naming testing/sneaky.md): $(tail -1 "$tmp/.te_out" 2>/dev/null)" "testing-typed-pos"
  fi
  # NEGATIVE: the genuine TestStrategy concept is exempt -> NOT flagged for [C6c].
  total=$((total + 1))
  if ! grep -q 'testing/legit.md' "$tmp/.te_out" 2>/dev/null; then
    ok "testing exemption typed: a genuine type:TestStrategy concept under testing/ stays exempt (no C6c)"
  else
    miss "testing exemption typed negative: legit TestStrategy concept was wrongly flagged [C6c]" "testing-typed-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v6 #3: worker/src enumeration is RECURSIVE -----------------------------
# A new file under a worker/src SUBDIR (other than lib/) must be enumerated. Before
# v6 only worker/src/*.ts + the hardcoded worker/src/lib/*.ts were walked, so a new
# worker/src/<subdir>/*.ts escaped. Hermetic: a file at worker/src/handlers/new.ts
# is seeded by NO concept and NOT excluded -> it MUST surface as a [C10b] gap.
assert_worker_src_recursive() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p worker/src/handlers
    printf 'export const anchor = 1;\n' > worker/src/index.ts
    # a NEW edge file in a worker/src SUBDIR other than lib/ — must be enumerated.
    printf 'export const h = 1;\n'      > worker/src/handlers/new.ts
    mkdir -p docs/knowledge/planes
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [w](/planes/w.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    cat > docs/knowledge/planes/w.md <<EOF
---
type: "Plane"
title: "Worker edge recursive (hermetic)"
description: "grounds the index entrypoint only."
source_files:
  - "worker/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Worker edge recursive (hermetic)

Lead paragraph.

# How it works
- the edge entrypoint (\`worker/src/index.ts:1\`).

# Invariants
- the entrypoint holds (\`worker/src/index.ts:1\`).

# Citations
1. \`worker/src/index.ts:1\` — grounded anchor.
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "planes/w"
    type: "Plane"
    status: "active"
    seed_from:
      - "worker/src/index.ts"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.ws_out" 2>&1 || true
  )
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.ws_out" 2>/dev/null \
     && grep -q 'worker/src/handlers/new.ts' "$tmp/.ws_out" 2>/dev/null; then
    ok "worker/src recursive: a new worker/src/<subdir>/*.ts is enumerated -> [C10b] names handlers/new.ts"
  else
    miss "worker/src recursive (no [C10b] naming worker/src/handlers/new.ts): $(tail -1 "$tmp/.ws_out" 2>/dev/null)" "worker-recursive"
  fi
  rm -rf "$tmp"
}

# --- gate v7 #1: strict-tree exclude-glob escape closed ------------------------
# A REAL request-reachable handler dropped under a `tests/` dir INSIDE a strict
# tree (crates/corelink-container/src/**) was swallowed by the broad `**/tests/**`
# manifest exclude with ZERO edits — the exclude globs ran AFTER the strict guard,
# so the strict guard suppressed dir/cluster ADOPTION but not the bulk exclude.
# v7: inside a strict tree a BROAD pattern exclude may exclude a file ONLY when
# that file is GENUINELY a test/config by its own BASENAME. We prove BOTH:
#  POSITIVE — routes/tests/poison.rs (a real handler name) REDs [C10b] naming it;
#  NEGATIVE — a genuine inline test (mod_tests.rs) under the SAME tests/ dir stays
#             excluded by `**/tests/**` (no [C10b] naming it).
assert_strict_tree_exclude_escape() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src/routes/tests
    # a REAL handler name dropped under a tests/ dir (NOT a test by its basename).
    printf 'pub async fn poison_handler() { /* reachable */ }\n' \
      > crates/corelink-container/src/routes/tests/poison.rs
    # a GENUINE inline-test module (basename is a test) under the SAME tests/ dir.
    printf '#[test]\nfn it_works() {}\n' \
      > crates/corelink-container/src/routes/tests/mod_tests.rs
    mkdir -p docs/knowledge
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
EOF
    git add -A
    git commit -q -m base
    # manifest: the broad `**/tests/**` bulk exclude is the ONLY thing that could
    # cover either file. v7 lets it cover mod_tests.rs (genuine test basename) but
    # NOT poison.rs (real handler basename -> needs an explicit per-file entry).
    cat > manifest.yaml <<EOF
profile_version: "0.1"
candidates: []
excludes:
  - surface: "**/tests/**"
    reason: "Test directories — test-only, not a load-bearing subsystem."
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.se_out" 2>&1 || true
  )
  # POSITIVE: the real handler name under tests/ is NOT auto-excluded -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.se_out" 2>/dev/null \
     && grep -q 'routes/tests/poison.rs' "$tmp/.se_out" 2>/dev/null; then
    ok "strict-tree exclude-escape: a real handler under a tests/ dir is NOT swallowed by **/tests/** -> [C10b] names poison.rs"
  else
    miss "strict-tree exclude-escape positive (no [C10b] naming routes/tests/poison.rs): $(tail -1 "$tmp/.se_out" 2>/dev/null)" "se-pos"
  fi
  # NEGATIVE (selectivity): the genuine inline test stays excluded by the glob.
  total=$((total + 1))
  if ! grep -q 'routes/tests/mod_tests.rs' "$tmp/.se_out" 2>/dev/null; then
    ok "strict-tree exclude-escape: a genuine inline test (mod_tests.rs) under tests/ stays excluded (selective)"
  else
    miss "strict-tree exclude-escape negative: mod_tests.rs was wrongly flagged" "se-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v7 #2: a strict-tree COVERING cite must be a CODE line ----------------
# In a strict tree a file counted as covered when a concept listed it in
# source_files + cited it under `# Citations` — but the cite could point at a
# NON-grounding line (a `//`/`//!` doc-comment, e.g. `:1-2`) while the executed
# handler body stayed uncited. v7: a strict-tree file whose ONLY covering cites
# resolve to comment/blank lines is NOT covered. We prove BOTH:
#  POSITIVE — poison.rs cited solely at its `:1-2` doc-comment header REDs [C10b];
#  NEGATIVE — the SAME file cited at its CODE line (:3) stays covered (no gap).
assert_cite_must_be_code_line() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src/routes/poisonx docs/knowledge/compliance
    cat > crates/corelink-container/src/routes/poisonx/poison.rs <<'RS'
//! poison module header doc-comment
//! second doc-comment line
pub async fn exfiltrate() { /* uncited executed body */ }
RS
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [x](/compliance/x.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # the concept declares + cites the file ONLY at its :1-2 doc-comment header.
    cat > docs/knowledge/compliance/x.md <<EOF
---
type: Concept
title: "Poison (hermetic)"
description: "covers poison only at its doc-comment header"
source_files:
  - "crates/corelink-container/src/routes/poisonx/poison.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# How it works
- the module header (\`crates/corelink-container/src/routes/poisonx/poison.rs:1-2\`).

# Citations
1. \`crates/corelink-container/src/routes/poisonx/poison.rs:1-2\` — the doc-comment header only.
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "compliance/x"
    type: "Concept"
    status: "active"
    seed_from:
      - "crates/corelink-container/src/routes/poisonx/poison.rs"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.cl_out" 2>&1 || true
    # NEGATIVE control: repoint BOTH cites to the CODE line (:3) -> covered.
    sed -i.bak 's/poison.rs:1-2/poison.rs:3/g' docs/knowledge/compliance/x.md
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.cl_ok" 2>&1 || true
  )
  # POSITIVE: doc-comment-only cite does not ground the strict file -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.cl_out" 2>/dev/null \
     && grep -q 'routes/poisonx/poison.rs' "$tmp/.cl_out" 2>/dev/null; then
    ok "strict-tree code-line cite: a doc-comment-only (:1-2) citation does NOT cover -> [C10b] names poison.rs"
  else
    miss "strict-tree code-line cite positive (no [C10b] naming routes/poisonx/poison.rs): $(tail -1 "$tmp/.cl_out" 2>/dev/null)" "cl-pos"
  fi
  # NEGATIVE (selectivity): the CODE-line (:3) citation stays covered (valid run).
  total=$((total + 1))
  if grep -q 'OKF-CoreLink profile valid' "$tmp/.cl_ok" 2>/dev/null; then
    ok "strict-tree code-line cite: the same file cited at its CODE line (:3) stays covered (selective)"
  else
    miss "strict-tree code-line cite negative: code-line (:3) citation was NOT accepted: $(tail -1 "$tmp/.cl_ok" 2>/dev/null)" "cl-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v8 #1: app enumeration is DYNAMIC (not a hardcoded 3-app allowlist) ----
# `_is_file_granular_strict` classifies EVERY `apps/*/src/**` as strict, but the
# surface walk used to enumerate only ("signup-worker","cas-worker","analytics-
# worker") — MISALIGNED, so a NEW app's handler was classified strict yet never
# enumerated and could never surface as a [C10b] gap (it shipped GREEN). v8
# enumerates ALL `apps/*/src/**/*.{ts,rs}` dynamically. Hermetic: a brand-new
# `apps/runner-worker/src/poison.ts` seeded by NO concept and NOT excluded MUST
# surface as a [C10b] silent gap naming it. NEGATIVE control: a `.ts` under an app
# whose dir IS excluded (apps/admin-ui-style) stays covered (not flagged).
assert_new_app_enumerated() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # a BRAND-NEW app (not in the old hardcoded allowlist) with a handler.
    mkdir -p apps/runner-worker/src
    printf 'export async function poison() { /* reachable backdoor */ }\n' \
      > apps/runner-worker/src/poison.ts
    # a non-handler app whose WHOLE dir is wholesale-excluded (admin-ui class).
    mkdir -p apps/some-ui/src
    printf 'export const page = 1;\n' > apps/some-ui/src/page.ts
    mkdir -p docs/knowledge
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
EOF
    git add -A
    git commit -q -m base
    cat > manifest.yaml <<EOF
profile_version: "0.1"
candidates: []
excludes:
  - surface: "apps/some-ui"
    reason: "fixture: presentation-only UI app, wholesale-excluded (admin-ui class) — covered at app-level only, not a request-reachable enforcer."
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.na_out" 2>&1 || true
  )
  # POSITIVE: the new app's uncited handler is enumerated -> fires naming it.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.na_out" 2>/dev/null \
     && grep -q 'apps/runner-worker/src/poison.ts' "$tmp/.na_out" 2>/dev/null; then
    ok "dynamic app enum: a NEW app's handler (runner-worker/src/poison.ts) is enumerated -> [C10b] names it"
  else
    miss "dynamic app enum positive (no [C10b] naming apps/runner-worker/src/poison.ts): $(tail -1 "$tmp/.na_out" 2>/dev/null)" "new-app-pos"
  fi
  # NEGATIVE (selectivity): the wholesale-excluded UI app's file is NOT flagged.
  total=$((total + 1))
  if ! grep -q 'apps/some-ui/src/page.ts' "$tmp/.na_out" 2>/dev/null; then
    ok "dynamic app enum: a wholesale-excluded UI app's file stays covered (selective)"
  else
    miss "dynamic app enum negative: apps/some-ui/src/page.ts was wrongly flagged" "new-app-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v8 #2: a strict-tree covering cite must be a SUBSTANTIVE code line ------
# `_line_is_code` accepted ANY non-comment line — including pure BOILERPLATE that
# substantiates nothing (`use crate::auth;`, `mod x;`, a bare `}`, `});`). A
# backdoor handler cited at its `use` line therefore shipped GREEN. v8 requires the
# covering cite to land on a SUBSTANTIVE line (real statement/expression or a named
# fn signature), rejecting import/module-wiring/brace scaffolding. We prove THREE:
#  POSITIVE-A — poison.rs cited solely at its `use` line REDs [C10b];
#  POSITIVE-B — the SAME file cited solely at a bare-brace `}` line REDs [C10b];
#  NEGATIVE   — the same file cited at its GENUINE enforcer (a real statement) line
#               stays covered (no gap) — proving the bar is raised, not broken.
assert_boilerplate_line_cite_rejected() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src/routes/poisony docs/knowledge/compliance
    # line1: use (boilerplate); line2: blank; line3: bare-brace block;
    # line5: the GENUINE enforcer statement; line6: closing brace.
    cat > crates/corelink-container/src/routes/poisony/poison.rs <<'RS'
use crate::auth::PatVerifier;
pub async fn handle() {
    {
        let _scaffold = ();
        return deny_unless_authorized(&caller);
    }
}
RS
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [x](/compliance/x.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # the concept declares + cites the file ONLY at its line-1 \`use\` boilerplate.
    cat > docs/knowledge/compliance/x.md <<EOF
---
type: Concept
title: "Boilerplate-line cite (hermetic)"
description: "covers poison only at its use-line boilerplate"
source_files:
  - "crates/corelink-container/src/routes/poisony/poison.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# How it works
- the import (\`crates/corelink-container/src/routes/poisony/poison.rs:1\`).

# Citations
1. \`crates/corelink-container/src/routes/poisony/poison.rs:1\` — the use line only.
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "compliance/x"
    type: "Concept"
    status: "active"
    seed_from:
      - "crates/corelink-container/src/routes/poisony/poison.rs"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.bp_use" 2>&1 || true
    # POSITIVE-B: repoint BOTH cites to the bare-brace line (:3) -> still REDs.
    sed -i.bak 's/poison.rs:1/poison.rs:3/g' docs/knowledge/compliance/x.md && rm -f docs/knowledge/compliance/x.md.bak
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.bp_brace" 2>&1 || true
    # NEGATIVE: repoint BOTH cites to the GENUINE enforcer statement (:5) -> covered.
    sed -i.bak 's/poison.rs:3/poison.rs:5/g' docs/knowledge/compliance/x.md && rm -f docs/knowledge/compliance/x.md.bak
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.bp_ok" 2>&1 || true
  )
  # POSITIVE-A: use-line-only cite does NOT cover the strict file -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.bp_use" 2>/dev/null \
     && grep -q 'routes/poisony/poison.rs' "$tmp/.bp_use" 2>/dev/null; then
    ok "boilerplate-line cite: a \`use\`-line-only citation does NOT cover -> [C10b] names poison.rs"
  else
    miss "boilerplate-line cite use-line positive (no [C10b] naming routes/poisony/poison.rs): $(tail -1 "$tmp/.bp_use" 2>/dev/null)" "bp-use-pos"
  fi
  # POSITIVE-B: bare-brace-line cite also does NOT cover -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.bp_brace" 2>/dev/null \
     && grep -q 'routes/poisony/poison.rs' "$tmp/.bp_brace" 2>/dev/null; then
    ok "boilerplate-line cite: a bare-brace (\`{\`) line citation does NOT cover -> [C10b] names poison.rs"
  else
    miss "boilerplate-line cite brace-line positive (no [C10b] naming routes/poisony/poison.rs): $(tail -1 "$tmp/.bp_brace" 2>/dev/null)" "bp-brace-pos"
  fi
  # NEGATIVE (selectivity): the GENUINE enforcer statement (:5) stays covered.
  total=$((total + 1))
  if grep -q 'OKF-CoreLink profile valid' "$tmp/.bp_ok" 2>/dev/null; then
    ok "boilerplate-line cite: the same file cited at its GENUINE enforcer statement (:5) stays covered (raised bar, not broken)"
  else
    miss "boilerplate-line cite negative: genuine enforcer-line (:5) citation was NOT accepted: $(tail -1 "$tmp/.bp_ok" 2>/dev/null)" "bp-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v9: strict-classifier vs surface-walk EXTENSION set unified -----------
# `_is_file_granular_strict` classifies EVERY file under worker/src/** and
# apps/*/src/** as strict (file-granular-required) REGARDLESS of extension, but the
# C10b surface WALK used to glob only `*.ts` (worker) and `*.ts`/`*.rs` (apps). So a
# real request-reachable Cloudflare-Worker backdoor with ANY OTHER executable
# extension — .mts/.mjs/.cts/.cjs/.tsx/.jsx/.js (all wrangler-`main`-eligible) —
# was classified strict yet NEVER enumerated and shipped GREEN. v9 enumerates the
# FULL executable matrix via the shared `_is_exec_source` predicate so the two sides
# AGREE. We prove THREE:
#  POSITIVE-A — worker/src/poison.mts (an ES-module edge handler) REDs [C10b];
#  POSITIVE-B — worker/src/widget.tsx (a JSX edge handler) REDs [C10b];
#  POSITIVE-C — a NEW app's apps/runner-worker/src/poison.mjs REDs [C10b];
#  NEGATIVE   — a genuine .ts edge file that IS concept-cited stays covered
#               (no gap) — proving the widened net stays SELECTIVE.
assert_exec_ext_set_unified() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # worker edge plane: a genuine cited .ts entrypoint + two NON-.ts backdoors.
    mkdir -p worker/src
    printf 'export const anchor = 1;\n'                                  > worker/src/index.ts
    printf 'export default { async fetch() { /* backdoor */ } };\n'      > worker/src/poison.mts
    printf 'export const Widget = () => null; /* jsx backdoor */\n'      > worker/src/widget.tsx
    # a BRAND-NEW app with an ES-module (.mjs) request-reachable handler.
    mkdir -p apps/runner-worker/src
    printf 'export async function poison() { /* reachable backdoor */ }\n' \
      > apps/runner-worker/src/poison.mjs
    mkdir -p docs/knowledge/planes
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [w](/planes/w.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    cat > docs/knowledge/planes/w.md <<EOF
---
type: "Plane"
title: "Worker edge ext (hermetic)"
description: "grounds the index entrypoint only."
source_files:
  - "worker/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Worker edge ext (hermetic)

Lead paragraph.

# How it works
- the edge entrypoint (\`worker/src/index.ts:1\`).

# Invariants
- the entrypoint holds (\`worker/src/index.ts:1\`).

# Citations
1. \`worker/src/index.ts:1\` — grounded anchor.
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "planes/w"
    type: "Plane"
    status: "active"
    seed_from:
      - "worker/src/index.ts"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.xx_out" 2>&1 || true
  )
  # POSITIVE-A: the .mts worker handler is enumerated -> fires naming it.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.xx_out" 2>/dev/null \
     && grep -q 'worker/src/poison.mts' "$tmp/.xx_out" 2>/dev/null; then
    ok "exec-ext unified: a .mts worker handler is enumerated -> [C10b] names poison.mts"
  else
    miss "exec-ext unified (no [C10b] naming worker/src/poison.mts): $(tail -1 "$tmp/.xx_out" 2>/dev/null)" "ext-mts-pos"
  fi
  # POSITIVE-B: the .tsx worker handler is enumerated -> fires naming it.
  total=$((total + 1))
  if grep -q 'worker/src/widget.tsx' "$tmp/.xx_out" 2>/dev/null; then
    ok "exec-ext unified: a .tsx worker handler is enumerated -> [C10b] names widget.tsx"
  else
    miss "exec-ext unified (no [C10b] naming worker/src/widget.tsx): $(tail -1 "$tmp/.xx_out" 2>/dev/null)" "ext-tsx-pos"
  fi
  # POSITIVE-C: the NEW app's .mjs handler is enumerated -> fires naming it.
  total=$((total + 1))
  if grep -q 'apps/runner-worker/src/poison.mjs' "$tmp/.xx_out" 2>/dev/null; then
    ok "exec-ext unified: a NEW app's .mjs handler is enumerated -> [C10b] names poison.mjs"
  else
    miss "exec-ext unified (no [C10b] naming apps/runner-worker/src/poison.mjs): $(tail -1 "$tmp/.xx_out" 2>/dev/null)" "ext-mjs-pos"
  fi
  # NEGATIVE (selectivity): the genuine concept-cited .ts entrypoint is NOT flagged.
  total=$((total + 1))
  if ! grep -q 'worker/src/index.ts' "$tmp/.xx_out" 2>/dev/null; then
    ok "exec-ext unified: a genuine concept-cited .ts entrypoint stays covered (selective)"
  else
    miss "exec-ext unified negative: covered worker/src/index.ts was wrongly flagged" "ext-ts-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v10 #1: Rust enumeration is CASE-INSENSITIVE -------------------------
# The container/routes surface used a case-SENSITIVE `rglob("*.rs")` while
# `_is_exec_source` lowercases the suffix — so `crates/corelink-container/src/
# Poison.RS` (pulled via `#[path]`) was classified strict yet NEVER enumerated and
# shipped GREEN. v10 enumerates via the SAME `_is_exec_source` predicate
# (case-insensitive). Hermetic: a `src/Poison.RS` seeded by NO concept and NOT
# excluded MUST surface as a [C10b] gap naming it.
assert_rust_case_insensitive() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    mkdir -p crates/corelink-container/src
    # an upper-case-extension Rust file (a `#[path]`-pulled module) — invisible to
    # a case-sensitive `*.rs` glob, classified strict by the lowercasing classifier.
    printf 'pub async fn poison() { deny_unless_authorized(); }\n' \
      > crates/corelink-container/src/Poison.RS
    mkdir -p docs/knowledge
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
EOF
    git add -A
    git commit -q -m base
    cat > manifest.yaml <<EOF
profile_version: "0.1"
candidates: []
excludes: []
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.ci_out" 2>&1 || true
  )
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.ci_out" 2>/dev/null \
     && grep -q 'crates/corelink-container/src/Poison.RS' "$tmp/.ci_out" 2>/dev/null; then
    ok "rust case-insensitive: a Poison.RS (upper-case ext) is enumerated -> [C10b] names it"
  else
    miss "rust case-insensitive (no [C10b] naming src/Poison.RS): $(tail -1 "$tmp/.ci_out" 2>/dev/null)" "rust-case-pos"
  fi
  rm -rf "$tmp"
}

# --- gate v10 #2: wrangler `main` OUTSIDE src/ is enumerated -------------------
# The apps surface walk + the strict classifier were hardcoded to `apps/*/src/**`,
# but a CF Worker's wrangler `main` can be ANYWHERE (app-root, a non-src dir, build
# output). An app whose deploy entrypoint lives outside src/ had its real
# request-reachable handler NEVER enumerated -> shipped GREEN. v10 parses each app's
# wrangler `main` and ALSO enumerates that entrypoint. We prove TWO:
#  POSITIVE — apps/sneaky-worker/index.ts (main at APP-ROOT, outside src/) REDs;
#  NEGATIVE — an app whose wrangler main IS its concept-cited src/index.ts stays
#             covered (no gap) — proving the widened net stays SELECTIVE.
assert_wrangler_main_outside_src() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # APP A: wrangler main lives at APP-ROOT (outside src/) — the backdoor surface.
    mkdir -p apps/sneaky-worker
    printf 'export default { async fetch() { /* reachable backdoor */ } };\n' \
      > apps/sneaky-worker/index.ts
    cat > apps/sneaky-worker/wrangler.toml <<'TOML'
name = "sneaky-worker"
main = "index.ts"
TOML
    # APP B: a genuine app whose wrangler main is its concept-cited src/index.ts.
    # Line 2 is the SUBSTANTIVE enforcer statement the concept cites (line 1 is a
    # boilerplate `export default {` opener, rejected by the v8 substantive rule).
    mkdir -p apps/honest-worker/src
    printf 'export default {\n  async fetch() { return deny_unless_authorized(); },\n};\n' \
      > apps/honest-worker/src/index.ts
    cat > apps/honest-worker/wrangler.toml <<'TOML'
name = "honest-worker"
main = "src/index.ts"
TOML
    mkdir -p docs/knowledge/planes
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [h](/planes/h.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    cat > docs/knowledge/planes/h.md <<EOF
---
type: "Plane"
title: "Honest worker (hermetic)"
description: "grounds the honest app entrypoint only."
source_files:
  - "apps/honest-worker/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Honest worker (hermetic)

Lead paragraph.

# How it works
- the entrypoint (\`apps/honest-worker/src/index.ts:2\`).

# Invariants
- it holds (\`apps/honest-worker/src/index.ts:2\`).

# Citations
1. \`apps/honest-worker/src/index.ts:2\` — grounded anchor (the fetch enforcer).
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "planes/h"
    type: "Plane"
    status: "active"
    seed_from:
      - "apps/honest-worker/src/index.ts"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.wm_out" 2>&1 || true
  )
  # POSITIVE: the app-root main (outside src/) is enumerated -> fires naming it.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.wm_out" 2>/dev/null \
     && grep -q 'apps/sneaky-worker/index.ts' "$tmp/.wm_out" 2>/dev/null; then
    ok "wrangler-main outside src: an app-root main (apps/sneaky-worker/index.ts) is enumerated -> [C10b] names it"
  else
    miss "wrangler-main outside src positive (no [C10b] naming apps/sneaky-worker/index.ts): $(tail -1 "$tmp/.wm_out" 2>/dev/null)" "wmain-pos"
  fi
  # NEGATIVE (selectivity): the concept-cited honest src/index.ts is NOT flagged.
  total=$((total + 1))
  if ! grep -q 'apps/honest-worker/src/index.ts' "$tmp/.wm_out" 2>/dev/null; then
    ok "wrangler-main outside src: a concept-cited src/index.ts main stays covered (selective)"
  else
    miss "wrangler-main outside src negative: covered apps/honest-worker/src/index.ts was wrongly flagged" "wmain-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v11: an [env.*] main OVERRIDE outside src/ is enumerated -------------
# A wrangler config may declare a top-level `main` (decoy) AND a per-environment
# override — `[env.prod]\nmain = "build/worker-prod.mjs"`. `wrangler deploy --env
# prod` deploys the ENV main, not the top-level one. The old parser used .search()
# (FIRST match only) so it saw ONLY the top-level decoy → the REAL prod entrypoint
# shipped GREEN with zero coverage. v11 parses EVERY main (findall) — top-level AND
# every [env.<name>] override. We prove TWO:
#  POSITIVE — apps/envmain-worker's [env.prod] main `build/worker-prod.mjs` (outside
#             src/, uncited) REDs [C10b] naming it, EVEN THOUGH the top-level
#             src/index.ts decoy is concept-cited (the decoy no longer masks it);
#  NEGATIVE — that concept-cited top-level src/index.ts main stays covered (the
#             widened net stays selective).
assert_env_main_override_outside_src() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # The app: a CONCEPT-CITED top-level src/index.ts main (the decoy) PLUS an
    # [env.prod] override main living OUTSIDE src/ (the real prod deploy surface,
    # uncited) — the v11 hole.
    mkdir -p apps/envmain-worker/src apps/envmain-worker/build
    printf 'export default {\n  async fetch() { return deny_unless_authorized(); },\n};\n' \
      > apps/envmain-worker/src/index.ts
    printf 'export default { async fetch() { /* prod-only backdoor */ } };\n' \
      > apps/envmain-worker/build/worker-prod.mjs
    cat > apps/envmain-worker/wrangler.toml <<'TOML'
name = "envmain-worker"
main = "src/index.ts"

[env.prod]
main = "build/worker-prod.mjs"
TOML
    mkdir -p docs/knowledge/planes
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [h](/planes/h.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # The concept grounds ONLY the top-level src/index.ts main (line 2 = the
    # substantive enforcer). The [env.prod] override is left uncited on purpose.
    cat > docs/knowledge/planes/h.md <<EOF
---
type: "Plane"
title: "Envmain worker (hermetic)"
description: "grounds the top-level app entrypoint only."
source_files:
  - "apps/envmain-worker/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Envmain worker (hermetic)

Lead paragraph.

# How it works
- the entrypoint (\`apps/envmain-worker/src/index.ts:2\`).

# Invariants
- it holds (\`apps/envmain-worker/src/index.ts:2\`).

# Citations
1. \`apps/envmain-worker/src/index.ts:2\` — grounded anchor (the fetch enforcer).
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "planes/h"
    type: "Plane"
    status: "active"
    seed_from:
      - "apps/envmain-worker/src/index.ts"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.em_out" 2>&1 || true
  )
  # POSITIVE: the [env.prod] override main (outside src/, uncited) is enumerated
  # -> fires naming it, even though the top-level decoy IS concept-cited.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.em_out" 2>/dev/null \
     && grep -q 'apps/envmain-worker/build/worker-prod.mjs' "$tmp/.em_out" 2>/dev/null; then
    ok "env-main override outside src: an [env.prod] main (build/worker-prod.mjs) is enumerated -> [C10b] names it"
  else
    miss "env-main override outside src positive (no [C10b] naming apps/envmain-worker/build/worker-prod.mjs): $(tail -1 "$tmp/.em_out" 2>/dev/null)" "envmain-pos"
  fi
  # NEGATIVE (selectivity): the concept-cited top-level src/index.ts is NOT flagged.
  total=$((total + 1))
  if ! grep -q 'apps/envmain-worker/src/index.ts' "$tmp/.em_out" 2>/dev/null; then
    ok "env-main override outside src: the concept-cited top-level src/index.ts main stays covered (selective)"
  else
    miss "env-main override outside src negative: covered apps/envmain-worker/src/index.ts was wrongly flagged" "envmain-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v12: ALT-NAMED + DUAL-CONFIG wrangler mains are unioned --------------
# Two same-root LOW holes (both: the gate's stated contract is "every wrangler
# main", but config resolution diverged from wrangler's actual selection):
#  1. ALT-NAMED config — `wrangler deploy --config wrangler.prod.toml` (or the
#     convention wrangler.<env>.toml) deploys a main declared in a NON-canonical-
#     named config. The old loop checked ONLY wrangler.toml/.jsonc/.json, so a main
#     declared solely in wrangler.prod.toml was NEVER read → shipped GREEN uncovered.
#     v12 globs every wrangler*.{toml,jsonc,json}.
#  2. DUAL-CONFIG precedence — the old loop was first-file-wins (returned on the
#     first existing canonical name, .toml before .jsonc), but modern wrangler
#     prefers .jsonc/.json. A decoy wrangler.toml (cited src/index.ts) over a
#     wrangler.jsonc (real, uncited build/*.mjs) hid the real entrypoint. v12 unions
#     the mains across ALL config files — no first-file-win.
# We prove THREE:
#  POSITIVE-A — apps/altcfg-worker's wrangler.prod.toml main `dist/prod.mjs` (outside
#               src/, declared ONLY in the alt-named config, uncited) REDs [C10b];
#  POSITIVE-B — apps/dualcfg-worker's wrangler.jsonc main `build/real.mjs` (the real
#               entrypoint, uncited) REDs [C10b] EVEN THOUGH a wrangler.toml decoy
#               cites src/index.ts (no first-file-win);
#  NEGATIVE   — that concept-cited top-level src/index.ts main stays covered.
assert_alt_and_dual_wrangler_config() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # APP A: a main declared ONLY in an alt-named wrangler.prod.toml, outside src/.
    mkdir -p apps/altcfg-worker/src apps/altcfg-worker/dist
    printf 'export default {\n  async fetch() { return ok(); },\n};\n' \
      > apps/altcfg-worker/src/index.ts
    printf 'export default { async fetch() { /* prod-only backdoor */ } };\n' \
      > apps/altcfg-worker/dist/prod.mjs
    # NO canonical wrangler.toml — the entrypoint lives ONLY in the alt-named config.
    cat > apps/altcfg-worker/wrangler.prod.toml <<'TOML'
name = "altcfg-worker"
main = "dist/prod.mjs"
TOML
    # APP B: a wrangler.toml decoy (cites src/index.ts) PLUS a wrangler.jsonc whose
    # real (uncited) main lives outside src/ — the dual-config precedence hole.
    mkdir -p apps/dualcfg-worker/src apps/dualcfg-worker/build
    printf 'export default {\n  async fetch() { return deny_unless_authorized(); },\n};\n' \
      > apps/dualcfg-worker/src/index.ts
    printf 'export default { async fetch() { /* the real, hidden entrypoint */ } };\n' \
      > apps/dualcfg-worker/build/real.mjs
    cat > apps/dualcfg-worker/wrangler.toml <<'TOML'
name = "dualcfg-worker"
main = "src/index.ts"
TOML
    cat > apps/dualcfg-worker/wrangler.jsonc <<'JSONC'
{
  // modern wrangler prefers .jsonc over .toml
  "name": "dualcfg-worker",
  "main": "build/real.mjs"
}
JSONC
    mkdir -p docs/knowledge/planes
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [a](/planes/a.md)
- [b](/planes/b.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # Concepts ground ONLY the cited src/index.ts mains; the alt-named & jsonc
    # mains (outside src/) are left uncited on purpose.
    cat > docs/knowledge/planes/a.md <<EOF
---
type: "Plane"
title: "Altcfg worker (hermetic)"
description: "grounds the src entrypoint only."
source_files:
  - "apps/altcfg-worker/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Altcfg worker (hermetic)

Lead paragraph.

# How it works
- the entrypoint (\`apps/altcfg-worker/src/index.ts:2\`).

# Invariants
- it holds (\`apps/altcfg-worker/src/index.ts:2\`).

# Citations
1. \`apps/altcfg-worker/src/index.ts:2\` — grounded anchor (the fetch enforcer).
EOF
    cat > docs/knowledge/planes/b.md <<EOF
---
type: "Plane"
title: "Dualcfg worker (hermetic)"
description: "grounds the top-level toml entrypoint only."
source_files:
  - "apps/dualcfg-worker/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Dualcfg worker (hermetic)

Lead paragraph.

# How it works
- the entrypoint (\`apps/dualcfg-worker/src/index.ts:2\`).

# Invariants
- it holds (\`apps/dualcfg-worker/src/index.ts:2\`).

# Citations
1. \`apps/dualcfg-worker/src/index.ts:2\` — grounded anchor (the fetch enforcer).
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "planes/a"
    type: "Plane"
    status: "active"
    seed_from:
      - "apps/altcfg-worker/src/index.ts"
  - id: "planes/b"
    type: "Plane"
    status: "active"
    seed_from:
      - "apps/dualcfg-worker/src/index.ts"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.av_out" 2>&1 || true
  )
  # POSITIVE-A: the alt-named wrangler.prod.toml main (outside src/, uncited) fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.av_out" 2>/dev/null \
     && grep -q 'apps/altcfg-worker/dist/prod.mjs' "$tmp/.av_out" 2>/dev/null; then
    ok "alt-named wrangler config: a main in wrangler.prod.toml (dist/prod.mjs) is enumerated -> [C10b] names it"
  else
    miss "alt-named wrangler config positive (no [C10b] naming apps/altcfg-worker/dist/prod.mjs): $(tail -1 "$tmp/.av_out" 2>/dev/null)" "altcfg-pos"
  fi
  # POSITIVE-B: the dual-config .jsonc real main is enumerated despite the .toml decoy.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.av_out" 2>/dev/null \
     && grep -q 'apps/dualcfg-worker/build/real.mjs' "$tmp/.av_out" 2>/dev/null; then
    ok "dual-config wrangler (no first-file-win): the .jsonc main (build/real.mjs) is enumerated despite a .toml decoy -> [C10b] names it"
  else
    miss "dual-config wrangler positive (no [C10b] naming apps/dualcfg-worker/build/real.mjs): $(tail -1 "$tmp/.av_out" 2>/dev/null)" "dualcfg-pos"
  fi
  # NEGATIVE (selectivity): the concept-cited top-level src/index.ts mains are NOT flagged.
  total=$((total + 1))
  if ! grep -qE 'apps/(altcfg|dualcfg)-worker/src/index.ts' "$tmp/.av_out" 2>/dev/null; then
    ok "alt/dual wrangler config: the concept-cited top-level src/index.ts mains stay covered (selective)"
  else
    miss "alt/dual wrangler config negative: a covered src/index.ts main was wrongly flagged" "altdual-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v13: a CRATE-HOSTED (or root) wrangler main outside src/ is enumerated -
# v10/v11/v12 closed the wrangler-main class FOR APPS ONLY — the enumeration globbed
# `apps/*/wrangler*` exclusively. A wrangler config that lives OUTSIDE apps/ (in a
# `crates/<x>` dir, or the repo root) declaring a `main` OUTSIDE `*/src/**` (e.g. a
# wasm POC's `build/worker/shim.mjs`) was NEVER enumerated as a strict surface: the
# crate is dir-covered by a CrateCluster seed, so the build-output entrypoint rode
# coarse cluster ADOPTION and shipped GREEN with zero coverage (PoC-proven via
# crates/corelink-clerk-cf/build/worker/shim.mjs). v13 runs the SAME _wrangler_mains
# enumeration over apps/ + crates/* + the repo ROOT. We prove TWO:
#  POSITIVE — crates/poc-cf's wrangler main `build/worker/shim.mjs` (outside src/,
#             uncited), under a whole-crate-adopted CrateCluster seed, REDs [C10b];
#  NEGATIVE — a crate whose wrangler main IS its concept-cited src/index.ts stays
#             covered (the widened net stays SELECTIVE; src/-internal mains unaffected).
assert_crate_wrangler_main_outside_src() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # CRATE A: a crate-hosted wrangler whose main is BUILD OUTPUT outside src/ — the
    # backdoor surface that rode whole-crate cluster adoption. The shim file exists.
    mkdir -p crates/poc-cf/build/worker crates/poc-cf/src
    printf 'fn anchor() {}\n' > crates/poc-cf/src/lib.rs
    printf 'export default { async fetch() { /* reachable backdoor */ } };\n' \
      > crates/poc-cf/build/worker/shim.mjs
    cat > crates/poc-cf/wrangler.toml <<'TOML'
name = "poc-cf"
main = "build/worker/shim.mjs"
TOML
    # CRATE B: a genuine crate whose wrangler main is its concept-cited src/index.ts.
    # Line 2 is the SUBSTANTIVE enforcer the concept cites (line 1 is boilerplate).
    mkdir -p crates/honest-cf/src
    printf 'export default {\n  async fetch() { return deny_unless_authorized(); },\n};\n' \
      > crates/honest-cf/src/index.ts
    cat > crates/honest-cf/wrangler.toml <<'TOML'
name = "honest-cf"
main = "src/index.ts"
TOML
    mkdir -p docs/knowledge/crates
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [a](/crates/a.md)
- [b](/crates/b.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # CrateCluster A: ADOPTS the whole crates/poc-cf dir (so the build-output main is
    # dir-covered by adoption — exactly the corelink-clerk-cf shape) + cites src/lib.rs.
    cat > docs/knowledge/crates/a.md <<EOF
---
type: "CrateCluster"
title: "POC CF cluster (hermetic)"
description: "adopts the whole crates/poc-cf dir; cites src/lib.rs, NOT the build main."
source_files:
  - "crates/poc-cf"
  - "crates/poc-cf/src/lib.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# POC CF cluster (hermetic)

Lead paragraph: the cluster adopts the crate dir and cites the lib anchor.

# How it works
- the adopted crate (the cited anchor is \`crates/poc-cf/src/lib.rs:1\`).

# Invariants
- the lib anchor holds (\`crates/poc-cf/src/lib.rs:1\`).

# Citations
1. \`crates/poc-cf/src/lib.rs:1\` — the one grounded file (NOT the build-output main).
EOF
    cat > docs/knowledge/crates/b.md <<EOF
---
type: "CrateCluster"
title: "Honest CF cluster (hermetic)"
description: "grounds the honest crate entrypoint (its wrangler main IS src/index.ts)."
source_files:
  - "crates/honest-cf/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Honest CF cluster (hermetic)

Lead paragraph.

# How it works
- the entrypoint (\`crates/honest-cf/src/index.ts:2\`).

# Invariants
- it holds (\`crates/honest-cf/src/index.ts:2\`).

# Citations
1. \`crates/honest-cf/src/index.ts:2\` — grounded anchor (the fetch enforcer).
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "crates/a"
    type: "CrateCluster"
    status: "active"
    seed_from:
      - "crates/poc-cf"
  - id: "crates/b"
    type: "CrateCluster"
    status: "active"
    seed_from:
      - "crates/honest-cf/src/index.ts"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.cwm_out" 2>&1 || true
  )
  # POSITIVE: the crate-hosted build-output main (outside src/), under whole-crate
  # adoption, is enumerated strict -> fires naming it. (THE v13 hole.)
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.cwm_out" 2>/dev/null \
     && grep -q 'crates/poc-cf/build/worker/shim.mjs' "$tmp/.cwm_out" 2>/dev/null; then
    ok "crate wrangler-main outside src: a crate-hosted build-output main (crates/poc-cf/build/worker/shim.mjs) is enumerated despite whole-crate adoption -> [C10b] names it"
  else
    miss "crate wrangler-main outside src positive (no [C10b] naming crates/poc-cf/build/worker/shim.mjs): $(tail -1 "$tmp/.cwm_out" 2>/dev/null)" "crate-wmain-pos"
  fi
  # NEGATIVE (selectivity): a crate whose main IS its concept-cited src/index.ts
  # stays covered — a main INSIDE the owning crate's src/** is unaffected.
  total=$((total + 1))
  if ! grep -q 'crates/honest-cf/src/index.ts' "$tmp/.cwm_out" 2>/dev/null; then
    ok "crate wrangler-main outside src: a concept-cited crate src/index.ts main stays covered (selective; src/-internal mains unaffected)"
  else
    miss "crate wrangler-main outside src negative: covered crates/honest-cf/src/index.ts was wrongly flagged" "crate-wmain-neg"
  fi
  rm -rf "$tmp"
}

# --- gate v14: config LOCATION can no longer hide a wrangler main -------------
# v13 enumerated wrangler mains over a FIXED 3-class LOCATION set — the repo ROOT +
# ONE level into apps/* + ONE level into crates/* — yet claimed "EVERY directory
# that hosts a wrangler config." A config whose LOCATION fell OUTSIDE that set
# shipped GREEN with only coarse crate-dir/cluster adoption. Three PoC-proven
# location bypasses (all reverted):
#   * crate-NESTED      — crates/<x>/cf/wrangler.toml (one-level-only misses cf/);
#   * sibling top-level  — services/edge/wrangler.toml (only root/apps/crates seen);
#   * worker/ alt config — worker/wrangler.staging.toml (worker/ not in the set).
# v14 RECURSIVELY rglobs wrangler*.{toml,jsonc,json} over the surface root (pruning
# build-output/vendor dirs), so config LOCATION can no longer hide a deploy
# entrypoint. We prove FOUR:
#  POSITIVE (crate-NESTED)    — crates/poc-cf/cf's main poison_nested.mjs (outside
#                               src/, riding whole-crate adoption) REDs [C10b];
#  POSITIVE (sibling top-dir) — services/edge's main poison_edge.mjs (a dir outside
#                               root/apps/crates entirely) REDs [C10b];
#  NEGATIVE (build-output)    — a config UNDER a pruned build/ dir is NOT walked, so
#                               its main is not scanned (we never scan generated
#                               bundles — same spirit as the other gates);
#  NEGATIVE (selectivity)     — a crate-nested config whose main IS a concept-cited
#                               src/index.ts stays covered (the widened net stays
#                               selective; src/-internal mains unaffected).
assert_wrangler_config_location_recursive() {
  local tmp; tmp="$(mktemp -d 2>/dev/null || mktemp -d -t okf)"
  local absval="$REPO_ROOT/$VAL"
  (
    set -e
    cd "$tmp"
    git init -q
    git config user.email t@t.io
    git config user.name tester
    git config commit.gpgsign false
    # BYPASS 1 — crate-NESTED config: crates/poc-cf/cf/wrangler.toml. The one-level
    # crates/* walk never descended into cf/, so its main rode whole-crate adoption.
    # The main is relative to the config's OWN dir and lives outside any src/.
    mkdir -p crates/poc-cf/cf crates/poc-cf/src
    printf 'fn anchor() {}\n' > crates/poc-cf/src/lib.rs
    printf 'export default { async fetch() { /* reachable backdoor */ } };\n' \
      > crates/poc-cf/cf/poison_nested.mjs
    cat > crates/poc-cf/cf/wrangler.toml <<'TOML'
name = "poc-cf-nested"
main = "poison_nested.mjs"
TOML
    # BYPASS 2 — sibling TOP-LEVEL dir: services/edge/wrangler.toml. Only root/apps/
    # crates were ever considered, so a config in a brand-new top-level dir was
    # invisible. (No concept/exclude grounds it -> must RED.)
    mkdir -p services/edge
    printf 'export default { async fetch() { /* reachable backdoor */ } };\n' \
      > services/edge/poison_edge.mjs
    cat > services/edge/wrangler.toml <<'TOML'
name = "edge"
main = "poison_edge.mjs"
TOML
    # NEGATIVE 1 — a config UNDER a pruned build/ dir: must NOT be walked (generated
    # bundles are not authored surface). If it WERE walked, build/gen/main.mjs would
    # RED — so a clean run proves the prune works.
    mkdir -p crates/poc-cf/build/gen
    printf 'export default { async fetch() {} };\n' \
      > crates/poc-cf/build/gen/main.mjs
    cat > crates/poc-cf/build/gen/wrangler.toml <<'TOML'
name = "generated-bundle"
main = "main.mjs"
TOML
    # NEGATIVE 2 — a crate-nested config whose main IS its concept-cited src/index.ts.
    # Line 2 is the SUBSTANTIVE enforcer the concept cites (line 1 is boilerplate).
    mkdir -p crates/honest-cf/deploy crates/honest-cf/src
    printf 'export default {\n  async fetch() { return deny_unless_authorized(); },\n};\n' \
      > crates/honest-cf/src/index.ts
    cat > crates/honest-cf/deploy/wrangler.toml <<'TOML'
name = "honest-cf"
main = "../src/index.ts"
TOML
    mkdir -p docs/knowledge/crates
    cat > docs/knowledge/index.md <<EOF
---
type: Index
okf_version: '0.1'
profile_version: '0.1'
---
# index
- [a](/crates/a.md)
- [b](/crates/b.md)
EOF
    git add -A
    git commit -q -m base
    sha0="$(git rev-parse HEAD)"
    # CrateCluster A: ADOPTS the whole crates/poc-cf dir (so the nested main is
    # dir-covered by adoption) + cites src/lib.rs — NOT the nested deploy main.
    cat > docs/knowledge/crates/a.md <<EOF
---
type: "CrateCluster"
title: "POC CF cluster (hermetic)"
description: "adopts the whole crates/poc-cf dir; cites src/lib.rs, NOT the nested main."
source_files:
  - "crates/poc-cf"
  - "crates/poc-cf/src/lib.rs"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# POC CF cluster (hermetic)

Lead paragraph: the cluster adopts the crate dir and cites the lib anchor.

# How it works
- the adopted crate (the cited anchor is \`crates/poc-cf/src/lib.rs:1\`).

# Invariants
- the lib anchor holds (\`crates/poc-cf/src/lib.rs:1\`).

# Citations
1. \`crates/poc-cf/src/lib.rs:1\` — the one grounded file (NOT the nested deploy main).
EOF
    cat > docs/knowledge/crates/b.md <<EOF
---
type: "CrateCluster"
title: "Honest CF cluster (hermetic)"
description: "grounds the honest crate entrypoint (its nested wrangler main IS src/index.ts)."
source_files:
  - "crates/honest-cf/src/index.ts"
checkpoint_sha: "$sha0"
provenance: "AUTHORED"
---

# Honest CF cluster (hermetic)

Lead paragraph.

# How it works
- the entrypoint (\`crates/honest-cf/src/index.ts:2\`).

# Invariants
- it holds (\`crates/honest-cf/src/index.ts:2\`).

# Citations
1. \`crates/honest-cf/src/index.ts:2\` — grounded anchor (the fetch enforcer).
EOF
    cat > manifest.yaml <<EOF
profile_version: "0.1"
excludes: []
candidates:
  - id: "crates/a"
    type: "CrateCluster"
    status: "active"
    seed_from:
      - "crates/poc-cf"
  - id: "crates/b"
    type: "CrateCluster"
    status: "active"
    seed_from:
      - "crates/honest-cf/src/index.ts"
EOF
    git add -A
    git commit -q -m A
    python3 "$absval" --bundle docs/knowledge --manifest manifest.yaml --surface-root . > "$tmp/.loc_out" 2>&1 || true
  )
  # POSITIVE (crate-NESTED): crates/poc-cf/cf/poison_nested.mjs, in a dir one level
  # deeper than the v13 walk ever descended, is enumerated strict -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.loc_out" 2>/dev/null \
     && grep -q 'crates/poc-cf/cf/poison_nested.mjs' "$tmp/.loc_out" 2>/dev/null; then
    ok "wrangler config LOCATION recursive: a crate-NESTED main (crates/poc-cf/cf/poison_nested.mjs) is enumerated despite whole-crate adoption -> [C10b] names it"
  else
    miss "wrangler config LOCATION crate-nested positive (no [C10b] naming crates/poc-cf/cf/poison_nested.mjs): $(tail -1 "$tmp/.loc_out" 2>/dev/null)" "loc-nested-pos"
  fi
  # POSITIVE (sibling top-level): services/edge/poison_edge.mjs, in a top-level dir
  # outside root/apps/crates entirely, is enumerated -> fires.
  total=$((total + 1))
  if grep -q '^\[C10b\]' "$tmp/.loc_out" 2>/dev/null \
     && grep -q 'services/edge/poison_edge.mjs' "$tmp/.loc_out" 2>/dev/null; then
    ok "wrangler config LOCATION recursive: a sibling top-level main (services/edge/poison_edge.mjs) is enumerated -> [C10b] names it"
  else
    miss "wrangler config LOCATION sibling-top-level positive (no [C10b] naming services/edge/poison_edge.mjs): $(tail -1 "$tmp/.loc_out" 2>/dev/null)" "loc-sibling-pos"
  fi
  # NEGATIVE (build-output prune): a config UNDER build/ is NOT walked, so its main
  # is never scanned (we never scan generated bundles).
  total=$((total + 1))
  if ! grep -q 'crates/poc-cf/build/gen/main.mjs' "$tmp/.loc_out" 2>/dev/null; then
    ok "wrangler config LOCATION recursive: a config under a pruned build/ dir is NOT walked (generated bundles not scanned)"
  else
    miss "wrangler config LOCATION build-output prune: crates/poc-cf/build/gen/main.mjs was wrongly scanned" "loc-build-prune"
  fi
  # NEGATIVE (selectivity): a crate-nested config whose main IS its concept-cited
  # src/index.ts stays covered — src/-internal mains unaffected.
  total=$((total + 1))
  if ! grep -q 'crates/honest-cf/src/index.ts' "$tmp/.loc_out" 2>/dev/null; then
    ok "wrangler config LOCATION recursive: a concept-cited src/index.ts main (declared by a nested config) stays covered (selective)"
  else
    miss "wrangler config LOCATION selectivity: covered crates/honest-cf/src/index.ts was wrongly flagged" "loc-selective-neg"
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
assert_c5_all_ranges
assert_blob_anchor_teeth
assert_blob_anchor_survives_rewrite
assert_blob_launder_closed
assert_c4b_entry_shapes
assert_c4b_blob_must_be_reachable
assert_c5b
assert_c5b_orphan_exempt
assert_c4_squash_orphan_tolerant
assert_c4_orphan_no_base_fail_closed
assert_bad C6  C6  --bundle "$FIX/bad/C6"  --manifest "$NONE"
assert_bad C6b C6b --bundle "$FIX/bad/C6b" --manifest "$NONE"
assert_bad C6c C6c --bundle "$FIX/bad/C6c" --manifest "$NONE"
assert_bad C7  C7  --bundle "$FIX/bad/C7"  --manifest "$NONE"
assert_bad C8  C8  --bundle "$FIX/bad/C8"  --manifest "$NONE"
assert_bad C9  C9  --bundle "$FIX/bad/C9"  --manifest "$NONE"
# Manifest checks need their fixture manifest; isolate C10 from C10b via surface root.
assert_bad C10  C10  --bundle "$FIX/bad/C10"  --manifest "$FIX/bad/C10/manifest.yaml"  --surface-root "$FIX/bad/C10/empty-surface"
assert_bad C10b C10b --bundle "$FIX/bad/C10b" --manifest "$FIX/bad/C10b/manifest.yaml" --surface-root "$FIX/bad/C10b/surface"
# fix #3 (seed_from self-certification grounding cross-check) + the lead's
# recursive container-src/storage-subdir enumeration — hermetic git harnesses.
assert_seed_selfcert
assert_storage_subdir_enum
# gate v5: #2 module-parent grounding removed + #6 glob is segment-aware.
assert_module_parent_removed
assert_glob_segment_aware
# gate v6: #1 cluster-directory adoption no longer auto-covers a container file +
# #2 testing exemption requires testing/ dir AND type:TestStrategy + #3 worker/src recursive.
assert_cluster_adoption_strict
assert_testing_exemption_typed
assert_worker_src_recursive
# gate v7: #1 strict-tree exclude-glob escape closed + #2 a strict-tree covering
# cite must resolve to a real CODE line (not a doc-comment header).
assert_strict_tree_exclude_escape
assert_cite_must_be_code_line
# gate v8: #1 app enumeration is DYNAMIC (a new app's handler is enumerated) +
# #2 a strict-tree covering cite must be a SUBSTANTIVE line (use/mod/brace REDs).
assert_new_app_enumerated
assert_boilerplate_line_cite_rejected
# gate v9: the strict-classifier and the surface walk use ONE shared executable-
# extension set — a .mts/.tsx worker handler + a .mjs new-app handler now RED [C10b]
# (was GREEN: classified strict but never enumerated because the walk globbed .ts only).
assert_exec_ext_set_unified
# gate v10: #1 the Rust enumeration is CASE-INSENSITIVE (a Poison.RS upper-case ext
# is enumerated, was GREEN under the case-sensitive *.rs glob) + #2 an app's
# wrangler `main` OUTSIDE src/ (the real deploy surface) is enumerated.
assert_rust_case_insensitive
assert_wrangler_main_outside_src
# gate v11: a wrangler [env.<name>] main OVERRIDE (TOML) / "env":{…} (JSONC) is now
# parsed via findall (not search) — an env-override entrypoint outside src/ that a
# top-level decoy used to mask is enumerated and REDs [C10b] (PoC: apps/poc-envmain).
assert_env_main_override_outside_src
# gate v12: wrangler config resolution now globs ALL wrangler*.{toml,jsonc,json} and
# UNIONs every declared main (no first-file-win) — an ALT-NAMED wrangler.prod.toml
# main outside src/ REDs, and a DUAL-CONFIG .jsonc real main REDs despite a .toml decoy.
assert_alt_and_dual_wrangler_config
# gate v13: the wrangler-main enumeration now runs over apps/ + crates/* + the repo
# ROOT (not apps/ only) — a CRATE-HOSTED wrangler main outside src/ (build output,
# riding whole-crate cluster adoption) REDs [C10b], closing the same wrangler-main
# class the apps-only v10/v11/v12 fix missed (PoC: crates/corelink-clerk-cf shim).
assert_crate_wrangler_main_outside_src
# gate v14: the wrangler-config walk is now a RECURSIVE rglob over the surface root
# (build-output/vendor dirs pruned), not a fixed root+apps/*+crates/* LOCATION set —
# a crate-NESTED (crates/<x>/cf/) and a sibling top-level (services/edge/) wrangler
# main outside src/ both RED [C10b], while a config under a pruned build/ dir is not
# scanned and a concept-cited src/index.ts main stays covered (selective).
assert_wrangler_config_location_recursive

echo "---------------------------------------"
if [ ${#misses[@]} -eq 0 ]; then
  echo "✅ $pass/$total fixtures behaved as specified"
  exit 0
else
  echo "⛔ ${#misses[@]} fixture(s) misbehaved: ${misses[*]}"
  echo "   ($pass/$total passed)"
  exit 1
fi
