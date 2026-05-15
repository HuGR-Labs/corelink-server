---
id: "AUTHOR-PRE-PR-CHECKLIST"
type: "process"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["author", "pre-pr", "self-check", "techlead-mirror", "ga-gate"]
---

# Author Pre-PR Self-Checklist

**Purpose.** Run these 30 checks **locally**, before you open the PR. Every row is a
gate a reviewer (or `/techlead`) WILL check; catching it pre-PR saves a review
round-trip.

**How to use.**

1. After your last commit, work this list top-to-bottom in your worktree.
2. Tick each row in the *PR body template* (it embeds this list).
3. For each `FAIL`: apply the *how to fix* in-place. Re-commit (not amend, NEW commit).
4. Only after every row is green: `gh pr create`.

**Cross-links.**

- `docs/internal/CODE-REVIEW-CHECKLIST.md` — reviewer side; same gates from the other seat.
- `docs/internal/TECHLEAD-CHECKLIST.md` — orchestrator-side L0-L10.
- `docs/internal/ENGINEERING-ONBOARDING.md` §Day 4-5 — "first PR flow" links here.
- `.claude/skills/techlead/SKILL.md` §4 — anti-pattern catalog (read once; these are the failure modes this checklist exists to prevent).

**Time budget.** ~15-20 min for a typical PR. ~30 min for a substantial new crate. Worth it: each round-trip is ~24h elapsed.

---

## Sanity (L0) — 5 rows

| # | Check | How to fix if FAIL |
|---|---|---|
| 1 | Branch name follows convention: `wt/<wave>-<wi>-<slug>` (orchestrator) or `<handle>/<short-desc>` (human). | `git branch -m <new-name>`; force-push only if branch isn't yet shared. |
| 2 | `git status --short` is empty (no uncommitted/untracked junk). | `git add` what's needed; commit, or `.gitignore` the rest. Do NOT leave WIP. |
| 3 | Commit subjects use convention: `feat(<wi>): ...` / `fix(<wave>): ...` / `refactor: ...` / `docs: ...` / `test: ...` / `chore: ...`. Body explains the WHY. | `git commit --amend` (only if branch unshared) or NEW commit with reword. |
| 4 | No secrets / `.env` / `*.pem` / private keys staged. | `git rm --cached <file>` + add to `.gitignore`. If pushed: rotate the actual secret + history-rewrite (`git filter-repo`); never just delete. |
| 5 | PR title drafted in `<type>(<scope>): <subject>` form, < 70 chars, body explains *why* not just *what*. | Edit before `gh pr create`. Title is the audit signal. |

---

## Build & Lint (L1) — 6 rows

| # | Check | How to fix if FAIL |
|---|---|---|
| 6 | `cargo build --workspace` exits 0. | Read the first error; fix root cause. Do NOT add `#[allow]`. If disk-pressure: `cargo clean` + retry. |
| 7 | `cargo clippy --workspace --tests -- -D warnings` exits 0 (Rust 1.91 strict). | Fix each warning. 1.91-class lints (`uninlined_format_args`, `default_constructed_unit_structs`, `duplicated_attributes`) usually have a clippy-suggested rewrite. |
| 8 | `cargo test --workspace --no-run` exits 0 (tests *compile*). | Same approach as #6/#7. Compile-time test failures are the cheapest to fix; do it now. |
| 9 | No function-level `#[allow(clippy::...)]` anywhere in `src/`. Crate-level `#![allow]` in test modules only. | Remove the `#[allow]`; fix the underlying lint. If lint is truly false-positive: open an issue + leave a doc-comment with the rationale (do not silently allow). |
| 10 | No `--no-verify` / hook-skip in any commit on this branch. | `git log` → identify; re-commit normally. Pre-commit hooks exist for a reason. |
| 11 | If you added a new crate: `[workspace] members` in root `Cargo.toml` includes it. | Edit `Cargo.toml`; commit the change. |

---

## Charter Constraints (L2) — 9 rows

These are the SOTA invariants. Reviewers will HARD-REJECT on violations.

| # | Check | How to fix if FAIL |
|---|---|---|
| 12 | All new `pub enum` / `pub struct` have `#[non_exhaustive]`. | Add the attribute. Document any exception (rare) in the type's rustdoc. |
| 13 | Zero new `unsafe` outside FFI crates (`corelink-py`, `corelink-go`, `corelink-wasm`, `corelink-clerk-cf`). | Remove the `unsafe`. If genuinely needed: file an ADR, get architect sign-off, retry. |
| 14 | No `use tokio` in `src/` (non-test, non-`main.rs`). | Refactor to executor-agnostic. Use `futures::executor` traits or pass executor in. |
| 15 | No `prop_assert!(matches!(..., Variant { .. }))`. | Replace with struct destructure + explicit field asserts: `let Variant { field, .. } = value else { panic!() }; prop_assert_eq!(field, expected);`. |
| 16 | `PROPTEST_CASES` reads env via the `proptest_cases(N)` helper from `corelink-test-utils`, NOT bare integer literals. | `use corelink_test_utils::proptest_cases; ProptestConfig::with_cases(proptest_cases(1000))`. |
| 17 | Audit order is **lookup → emit_audit → mutate_state**. Mutating before audit emit is a HARD violation. | Re-order. If state is borrowed mut before lookup: split into clone-key → lookup → emit → mutate. |
| 18 | Zero `unwrap()` / `expect(` / `panic!` / `unimplemented!()` / `todo!()` in `src/` outside `#[cfg(test)]`. | Replace with `?` + proper error variant. Add the variant to the crate's `Error` enum (with `#[non_exhaustive]`). |
| 19 | New non-FFI crates have `#![forbid(unsafe_code)]` at `lib.rs` top. | Add the attribute. CI will reject if not present. |
| 20 | No raw secrets in `tracing::*` / `log::*` / `println!` / `eprintln!` / `dbg!`. | Wrap secrets in a newtype with custom `Debug` that prints `***`. Or pass only a fingerprint (`hash_hex(&secret)[..8]`). |

---

## Security & Privacy (L3) — 5 rows

| # | Check | How to fix if FAIL |
|---|---|---|
| 21 | Every new HTTP / gRPC endpoint validates auth (Bearer PAT or Clerk session) BEFORE handler logic. | Wrap the route in the auth middleware OR add the explicit `verify_token(&req)?` call as the first line of the handler. |
| 22 | Every state mutation emits an audit event. No PII in payloads (use `tenant_id` / hashes / `correlation_id`; never raw email/phone/name). | Wrap the mutation in `audit.emit(EvtMutation { tenant_id, correlation_id, ... })?` before the mutation. Hash any PII you must reference. |
| 23 | HMAC / signature verify uses `subtle::ConstantTimeEq`, NOT `==`. | `use subtle::ConstantTimeEq; if expected.ct_eq(&got).into() { ... }`. |
| 24 | New GitHub Actions `uses:` are SHA-pinned (40-char hex) with `# vX.Y.Z` comment. | `uses: actions/checkout@<40-hex> # v4.2.2`. Look the SHA up on GitHub releases page. |
| 25 | If touching BYOK / crypto: AAD includes `tenant_id` (or its HMAC). | Add `aad: AssociatedData::tenant(tenant_id)` to the encrypt call. Verify decrypt path checks AAD too. |

---

## Tests (L4) — 3 rows

| # | Check | How to fix if FAIL |
|---|---|---|
| 26 | ≥ 1 property test per non-trivial new invariant. Adversarial test for each public happy-path function. | Add `proptest! { fn prop_invariant_<...>(...) { ... } }` and `#[test] fn adversarial_<...>() { ... }`. Use `corelink-test-utils::proptest_cases(N)`. |
| 27 | No test that *only* checks `.is_ok()` / `.is_err()` on substantial logic. Destructure + assert fields. | `let Ok(Resp { value, signature, .. }) = result else { panic!() }; assert_eq!(value, expected);`. |
| 28 | Live-network tests gated `#[ignore]` AND `#[cfg(feature = "live-integration")]`. | Add both attributes. Default `cargo test` should never hit the wire. |

---

## Docs & Spec (L5 + L6) — 2 rows

| # | Check | How to fix if FAIL |
|---|---|---|
| 29 | If you added a new crate: crate-level `//!` rustdoc + `///` on every `pub` item. If architectural decision: ADR filed at `specs/03_architecture/adrs/ADR-S*-<keyword>.md`. If spec contract scope changed: changelog row added. New spec docs have full frontmatter. | Add the docs. ADR template: `specs/03_architecture/adrs/_ADR-TEMPLATE.md`. Frontmatter template: copy from any existing spec doc. |
| 30 | `python3 scripts/validate_specs.py` exits 0, OR failure count ≤ pre-branch baseline (NEVER goes up). `python3 scripts/check_migrations_additive.py` exits 0. Migration numbers don't collide with `main`. | Run both locally; fix any new failures. If `validate_specs` count rose: fix the cause in this same PR — never punt as "pre-existing" (AP-5 anti-pattern). Migration collision: renumber to next-free under `migrations/d1/`. |

---

## Final gate

After all 30 rows green:

```bash
# Sanity re-run (60s)
cargo build --workspace 2>&1 | tail -3
cargo clippy --workspace --tests -- -D warnings 2>&1 | tail -3
cargo test --workspace --no-run 2>&1 | tail -3
python3 scripts/validate_specs.py 2>&1 | tail -3

# Branch is up-to-date with origin/main
git fetch origin
git log --oneline origin/main..HEAD | head -10   # your commits
git merge-base --is-ancestor origin/main HEAD && echo "OK: branch has main"

# Open the PR
gh pr create   # template auto-loaded
```

If any of the above fails: **do not open the PR**. Fix locally, push, re-run. A PR opened red costs the reviewer one round-trip + your context-switch.

---

## Anti-patterns to refuse on yourself

These are documented in `.claude/skills/techlead/SKILL.md` §4. The shortlist:

1. **AP-1 (test count lying)** — verify your reported count: `cargo test --workspace --no-run 2>&1 | grep -E "test [a-z_]+" | wc -l`. Put the actual number in the PR body.
2. **AP-2 (uncommitted SEAL)** — `git status --short` must be empty before you mark the WI as done.
3. **AP-4 (charter shortcut)** — never relax L2 to ship faster. Fix it.
4. **AP-5 (validate_specs drift)** — never let the failure count rise. If it would, this PR isn't ready.
5. **AP-6 (Cargo.lock --theirs blind)** — on conflict, `--ours` + `cargo update -w`. Verify security pins after.
6. **AP-7 ("I'll fix later")** — there is no later. Fix it in this PR.

---

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7) | Initial 30-row author-side pre-PR self-checklist mirroring L0-L6 of `/techlead`. Each row has check + how-to-fix. Anti-pattern self-refusal shortlist. GA-Gate "every PR reviewed" prerequisite. |
