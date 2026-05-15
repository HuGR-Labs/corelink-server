---
name: techlead
description: Orchestrator/tech-lead verification protocol. Invoke before merging any Sonnet agent's SEAL commit to main. Runs L0-L7 cold checks (sanity → build/lint → charter constraints → security → tests → docs → spec hygiene → merge hygiene). Output is a per-deliverable verdict: APPROVE / FIX-FIRST / REJECT.
---

# /techlead — per-deliverable verification

Invoke as `/techlead <branch-or-commit-or-worktree-path>` whenever an agent reports SEAL on a Sonnet deliverable, BEFORE merging to main.

Full checklist + rationale lives at `docs/internal/TECHLEAD-CHECKLIST.md`. This skill is the executable version.

## When to invoke

- Right after an agent reports SEAL (`feat(<wi>): SEALED — ...` commit)
- Before `git merge --no-ff wt/<branch>` to main
- During batched merges, repeat L0-L7 per branch + L9 once at end
- During sprint-close / wave-close before tagging `<wave>-sealed`

## When NOT to invoke

- Pure documentation deliverable with no code (skip L1-L4; verify L5+L6+L7 only)
- Merge specialist's report (assume merge specialist already ran the per-branch checks; just verify final L1+L6 on main)
- Audit/review-only branches (no code; skip L1-L4)

## Inputs

The skill takes a target. Resolve in order:
1. If `<arg>` is a path → treat as worktree dir
2. If `<arg>` is a commit hash → `git show <hash>` + cd to its branch's worktree
3. If `<arg>` is a branch name → `git worktree list` lookup
4. If `<arg>` is empty → use current pwd

## Execution

Walk the levels in order. STOP at first hard fail; do not waste cycles past it.

### L0 — Sanity (60s)

```bash
TARGET_DIR="<resolved>"
cd "$TARGET_DIR"
git status --short
git log --oneline -3
ls <claimed-files>   # spot-check agent's file list
```

Verdict gates:
- L0.1-L0.5 ALL pass → continue to L1
- ANY fail → `REJECT — STOP`. Report what failed; do NOT merge.

### L1 — Build + Lint (3-5 min)

```bash
cargo build --workspace 2>&1 | tail -3       # exit 0
cargo clippy --workspace --tests -- -D warnings 2>&1 | tail -3   # exit 0
cargo test --workspace --no-run 2>&1 | tail -3   # exit 0
grep -rn "#\[allow(clippy::" <new-code-paths> 2>&1 | grep -v "#!\[allow" | head  # function-level allows → flag
git log --pretty=format:"%s" main..HEAD | grep -i "no.verify"   # should be empty
grep "^members =" -A 50 Cargo.toml | grep "<new-crate>"   # workspace member declared
```

For pure-docs deliverables: skip; verify `python3 scripts/validate_specs.py 2>&1 | tail -3` instead.

### L2 — Charter (5 min)

```bash
NEW_CRATE_DIR="<from-L0>"
grep -rn "pub enum\|pub struct" "$NEW_CRATE_DIR/src/" | head      # then verify each has #[non_exhaustive]
grep -rn "unsafe " "$NEW_CRATE_DIR/src/" | head                    # only FFI boundary
grep -rn "use tokio" "$NEW_CRATE_DIR/src/" | grep -v "#\[cfg(test)" | head   # empty
grep -rn "prop_assert!(matches" "$NEW_CRATE_DIR/" | head           # empty
grep -rn "ProptestConfig::with_cases\|cases: " "$NEW_CRATE_DIR/tests/" | head  # all should be proptest_cases(N) fn, not int literal
grep -rn "unwrap()\|expect(\|panic!\|todo!\|unimplemented!" "$NEW_CRATE_DIR/src/" | grep -v "#\[cfg(test)" | head  # empty
grep -n "forbid(unsafe_code)" "$NEW_CRATE_DIR/src/lib.rs"          # must match
```

For audit-fail-CLOSED L2.6: open one audit-emitting function. Confirm visual order: `let x = lookup()?; audit.emit(...)?; mutate_state(x);` — never `mutate then emit`.

### L3 — Security (10 min)

```bash
# L3.3 — no PII in audit
grep -rn "audit.emit\|audit_emit\|emit_audit" "$NEW_CRATE_DIR/src/" | grep -iE "email|phone|tenant_name|name:|address:"   # empty
# L3.4 — signature verify uses constant-time compare
grep -rn "verify_signature\|verify_hmac" "$NEW_CRATE_DIR/src/" | head -3
# Open each and verify `subtle::ConstantTimeEq` used, not `==`
# L3.9 — SHA-pin workflows
grep -rn "uses: " "$NEW_CRATE_DIR/.github/workflows/" 2>&1 | grep -vE "[a-f0-9]{40}" | head   # should be empty (no floating tags)
```

### L4 — Test Quality (5 min spot-sample)

```bash
ls "$NEW_CRATE_DIR/tests/" "$NEW_CRATE_DIR/src/" | grep -E "test|prop|adversarial"
grep -rn "fn test_\|#\[test\]\|proptest!\|prop_" "$NEW_CRATE_DIR/" | wc -l   # count vs agent's claim
grep -rn "tokio::test\|async fn test_" "$NEW_CRATE_DIR/src/" | head   # tokio in tests only ok
```

Open 1-2 test files at random. Skim for:
- Asserts a property, not just `is_ok()`
- Negative-path coverage (Err variants checked)
- Idempotency / atomicity claims pinned

### L5 — Docs (3 min)

```bash
head -5 "$NEW_CRATE_DIR/src/lib.rs"                                # crate rustdoc //!
grep "missing_docs" "$NEW_CRATE_DIR/Cargo.toml"                    # deny set
ls specs/03_architecture/adrs/ | grep -i "<wi-or-feature>"         # ADR filed if architectural
grep "<wi-id>" specs/04_sprints/S*/_spec_contract.md | head        # changelog row added
```

### L6 — Spec Hygiene (2 min)

```bash
python3 scripts/validate_specs.py 2>&1 | tail -3                   # exit 0 or only pre-existing
python3 scripts/check_migrations_additive.py 2>&1 | tail -3        # exit 0
ls migrations/d1/ | tail -10                                       # no number collision
```

### L7 — Merge Hygiene (4 min during merge)

```bash
git fetch origin
git merge-base --is-ancestor main "<branch>"                       # branch up-to-date OR rebased plan
# After merge:
grep -rn "<<<<<<<\|>>>>>>>" --include="*.toml" --include="*.rs" --include="*.md" specs/ crates/ 2>/dev/null | head   # empty
cargo build --workspace 2>&1 | tail -3                             # exit 0
cargo test --workspace --no-run 2>&1 | tail -3                     # count >= baseline
```

### L8 — Decision Documentation (always; after merge)

- Commit message: `merge wt/<branch> (one-line summary) into main`
- If P0 surfaced: file `fix(<wave>): P0 remediation` follow-up
- Update `ROADMAP-TO-GA.md` change log if wave completes
- Tag major milestones
- Push every 3-5 merges

### L9 — Risk Assessment (5 min before any tag)

Ask + answer aloud:
1. What's mocked / trait-deferred? Production swap path stable?
2. What's human-action-bound? Tracked in roadmap?
3. Contract changed silently?
4. Customer-visible API/schema migration documented?
5. New crypto primitive without ADR?
6. New SLO needing Grafana/PD/runbook?
7. Worst-case if shipped tomorrow with one bug?

### L10 — Rolling (every ~5 merges)

```bash
git worktree list
df -h /                          # ≥ 40GB target
python3 scripts/validate_specs.py 2>&1 | tail -3   # baseline failures count, never goes up
git log --oneline origin/main..HEAD | wc -l    # if > 10 → push
```

## Output format

```
TECH-LEAD VERDICT — <branch> @ <commit>
=========================================
L0 Sanity        : PASS|FAIL — <evidence>
L1 Build/Lint    : PASS|FAIL — <evidence>
L2 Charter       : PASS|FAIL — <evidence>
L3 Security      : PASS|FAIL — <evidence>
L4 Tests         : PASS|FAIL — <evidence> (count: N)
L5 Docs          : PASS|FAIL — <evidence>
L6 Spec hygiene  : PASS|FAIL — <evidence>
L7 Merge ready   : PASS|FAIL|N/A — <evidence>

Verdict: APPROVE-FOR-MERGE | FIX-FIRST (P0s: ...) | REJECT (HARD-FAIL: ...)

Remediation if needed:
  ...

Recorded as: log entry in /tmp/techlead-log-<branch>.md (append, never overwrite)
```

## Anti-patterns to refuse

1. Agent reports "0 tests" — always cold-check `cargo test --no-run` first
2. Agent says "wait for build" — verify worktree, commit manually if work is present + green
3. Tag-before-round-2 — always run sprint-close adversarial review before `<wave>-sealed`
4. Skip Opus eyeball on charter constraints — L2 is non-negotiable
5. Let `validate_specs.py` failures linger "out-of-scope" — every R-cycle MUST close them

## Notes

- This skill is FAST when prior agent quality is high (~10 min total). Slow when fixing problems (~38 min).
- For batched merges (5+ branches): batch L0-L7 per branch, do L9 once at end before tag.
- Skill output appended to `/tmp/techlead-log-<branch>.md` so audit trail survives session boundaries.

## Calibration

Time the skill on first 3 invocations; calibrate the budget. Target: median 12 min per branch with normal quality, escalate to 30+ min only on hard-fail.
