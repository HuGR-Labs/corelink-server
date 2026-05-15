---
id: "CODE-REVIEW-CHECKLIST"
type: "process"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["review", "checklist", "pr", "ga-gate", "techlead-mirror"]
---

# Code Review Checklist (Reviewer-side)

**Purpose.** Mirror of `/techlead` levels L0-L7 from the reviewer's seat. Every PR
into `main` must be reviewed against this checklist before approval. GA-Gate
criterion "every PR reviewed" depends on this artifact.

**How to use.**

1. Copy the *Reviewer Comment Template* (bottom of this doc) into the PR review.
2. Walk through L0-L7 in order. Tick each row.
3. For each FAIL: write a review comment referencing the row ID (e.g. "L2.6").
4. Render the verdict at the bottom: `APPROVE` / `REQUEST-CHANGES` / `BLOCK`.
5. Escalate to `/techlead` invocation if any *escalate-if* condition fires.

**Time budget.** ~20-30 min for medium PR. ~45 min for substantial new crate.
~10 min for pure-docs / typo / config-only PR.

**Cross-links.**

- `docs/internal/TECHLEAD-CHECKLIST.md` — orchestrator-side L0-L10 (this doc is the L0-L7 subset for human reviewers).
- `docs/internal/AUTHOR-PRE-PR-CHECKLIST.md` — author runs the same gates locally BEFORE opening PR.
- `docs/internal/ENGINEERING-ONBOARDING.md` — Week-2 onboarding includes "review 5 PRs"; use this doc.
- `.claude/skills/techlead/SKILL.md` — full L0-L10 with rationale + anti-pattern catalog.
- `ROADMAP-TO-GA.md` §GA-Gate — "every PR reviewed" criterion.

---

## L0 — Sanity (60 seconds, never skip)

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L0.1 | PR title follows convention: `<type>(<scope>): <subject>` where type ∈ `feat`/`fix`/`refactor`/`docs`/`test`/`chore`/`perf`/`security`/`merge`/`seal`. | Read PR title; eyeball. | Title is bare `WIP` / `fix` / empty subject — REQUEST-CHANGES. |
| L0.2 | PR scope reasonable: diff < ~2000 LoC OR clearly partitioned by commits. | `gh pr diff <n> --patch \| wc -l` | Diff > 5000 LoC AND not a single mechanical refactor — REQUEST split into smaller PRs. |
| L0.3 | Branch follows convention: `wt/<wave>-<wi>-<slug>` for orchestrator branches OR `<handle>/<slug>` for human-authored. | Read PR head ref. | Branch name unrecognizable — ask author to rename or document. |
| L0.4 | Commit message subjects clean: no `WIP`, no `fix typo`-spam, no `--no-verify` markers. | `gh pr view <n> --json commits --jq '.commits[].messageHeadline'` | Squash strategy unclear — confirm with author. |
| L0.5 | Files claimed in PR body actually exist in the diff. | `gh pr diff <n> --name-only` | Author mentions a file that's not in the diff — surface mismatch. |
| L0.6 | No accidentally committed secrets / `.env` / `*.pem` / `private-key`. | `gh pr diff <n> --name-only \| grep -iE 'env\|pem\|secret\|key$\|credentials'` | Any match — HARD REJECT until rotated and force-pushed-out (and rotate the actual secret). |
| L0.7 | CI is green at HEAD of PR branch (or rationale documented). | `gh pr checks <n>` | Red CI without comment — REQUEST author triage. |

**Verdict trigger.** ANY L0 fail → REQUEST-CHANGES. Do not proceed to L1+ until L0 is clean.

---

## L1 — Build & Lint (CI verification + local spot-check)

CI runs these; reviewer's job is to verify the green check **and** spot-check that no smuggled bypasses sneaked in.

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L1.1 | CI `cargo build --workspace` green. | `gh pr checks <n>` look for `build`. | Red — REQUEST author fix; do not approve. |
| L1.2 | CI `cargo clippy --workspace --tests -- -D warnings` green. Rust 1.91 lints honored. | `gh pr checks <n>` look for `clippy`. | Red, or yellow with author "I'll fix later" — REQUEST author fix in this PR. |
| L1.3 | CI `cargo test --workspace --no-run` green (tests compile). | `gh pr checks <n>` look for `test`. | Red — REQUEST fix. |
| L1.4 | No function-level `#[allow(clippy::...)]` smuggled in src/. Crate-level `#![allow]` in test modules only is acceptable. | `gh pr diff <n> \| grep -E '^\+\s*#\[allow\(clippy'` | Any match in non-test code — REQUEST author fix the underlying lint instead. |
| L1.5 | No `--no-verify` / hook-skip in commit history. | `gh pr view <n> --json commits --jq '.commits[].messageBody' \| grep -iE 'no.verify\|skip.hook'` | Any match — REQUEST author re-commit with hooks. |
| L1.6 | Workspace `Cargo.toml` `[workspace] members` includes any new crate. | `gh pr diff <n> -- Cargo.toml` | New crate dir added but `Cargo.toml` not updated — REQUEST fix. |
| L1.7 | No `cargo-deny` regressions: licenses, advisories, bans. | `gh pr checks <n>` look for `cargo-deny`. | Red — REQUEST author triage; CC AppSec if license category. |

**Verdict trigger.** L1.1-L1.3 red → REQUEST-CHANGES. L1.4 / L1.5 violations → HARD REQUEST-CHANGES (charter constraint).

---

## L2 — Charter Constraints (5-8 min, NEVER skip on code PRs)

This is the SOTA invariant set. Failures here = permanent quality regression. **Non-negotiable.**

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L2.1 | `#[non_exhaustive]` on every new public `enum` / `struct`. | `gh pr diff <n> \| grep -E '^\+\s*pub (enum\|struct)\s'` then verify each has the attribute. | New public type without `#[non_exhaustive]` and no rationale in commit — REQUEST add or document. |
| L2.2 | Zero `unsafe` outside FFI boundary modules (`corelink-py`, `corelink-go`, `corelink-wasm`, `corelink-clerk-cf`). | `gh pr diff <n> \| grep -E '^\+.*unsafe '` | Non-FFI new `unsafe` — HARD REJECT pending architect review + ADR. |
| L2.3 | No `tokio` import in `src/` (test modules + `main.rs` only). | `gh pr diff <n> \| grep -E '^\+\s*use tokio'` cross-reference paths. | Non-test `src/` adds `use tokio` — REQUEST refactor to executor-agnostic. |
| L2.4 | No `prop_assert!(matches!(..., Variant { .. }))` anti-pattern (S-08 P1-1 lesson). | `gh pr diff <n> \| grep -E '^\+.*prop_assert!\(matches'` | Any match — REQUEST replace with struct destructure + explicit field asserts. |
| L2.5 | `PROPTEST_CASES` reads env via runtime fn (`proptest_cases(N)` helper), NOT const literal. | `gh pr diff <n> \| grep -E '^\+.*(ProptestConfig::with_cases\|cases:)'` | Bare integer literal — REQUEST use helper. |
| L2.6 | Audit fail-CLOSED ordering: `lookup → emit_audit → mutate_state`. NEVER mutate before audit emit. | `gh pr diff <n> \| grep -E '^\+.*audit.*emit'` then open each callsite. | Mutate-before-emit visible — HARD REJECT (silent-state-change risk). |
| L2.7 | Zero `unwrap()` / `expect(` / `panic!` / `unimplemented!()` / `todo!()` outside `#[cfg(test)]`. | `gh pr diff <n> \| grep -E '^\+.*\.(unwrap\(\)\|expect\()'` and `\+.*(panic!\|todo!\|unimplemented!)` | Any match in src/ — REQUEST replace with `?` or explicit error path. |
| L2.8 | `#![forbid(unsafe_code)]` at the crate root of every new non-FFI crate. | `gh pr diff <n> -- 'crates/*/src/lib.rs'` head of new files. | Missing in new crate — REQUEST add. |
| L2.9 | Secrets never logged. Tokens / API keys / JWT material must be redacted (`***`, newtype with custom Debug). | `gh pr diff <n> \| grep -E '^\+.*(tracing::\|log::\|println!\|eprintln!\|dbg!)' \| grep -iE 'secret\|key\|token\|jwt\|password'` | Any match without redaction — HARD REJECT. |

**Verdict trigger.** ANY L2 fail → REQUEST-CHANGES. L2.2 / L2.6 / L2.9 → HARD-REJECT, escalate to `/techlead`.

---

## L3 — Security & Privacy (8-12 min)

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L3.1 | Every new HTTP / gRPC endpoint validates auth (Bearer PAT / Clerk session) BEFORE handler logic. | `gh pr diff <n> \| grep -E '^\+.*(\.route\(\|fn handle_\|fn route_\|tonic.*Service)'` then read each. | Endpoint added without auth check or with auth in wrong order — REQUEST fix + escalate to AppSec. |
| L3.2 | Every state mutation (insert/update/delete/persist/commit) emits an audit event. | `gh pr diff <n> \| grep -E '^\+.*fn (insert\|update\|delete\|create\|persist\|commit)_'` then trace to audit. | Silent mutation — REQUEST audit-emit wrap. |
| L3.3 | No PII in audit payloads (CTRL-PRIV-001). Use `tenant_id` / hashes / correlation_id, NEVER raw email/phone/name. | `gh pr diff <n> \| grep -E '^\+.*audit.*emit'` open each. | Raw PII fields — REQUEST hash-derive or remove + escalate to Privacy / DPO. |
| L3.4 | HMAC / signature verify uses constant-time compare (`subtle::ConstantTimeEq`), NOT `==` / `.eq()`. | `gh pr diff <n> \| grep -E '^\+.*(verify_signature\|verify_hmac\|verify_webhook)'` | Plain `==` — HARD REJECT (timing-attack risk). |
| L3.5 | Timestamp-bound signatures reject replay (≤ 5 min default tolerance). | `gh pr diff <n> \| grep -E '^\+.*(timestamp\|nonce\|replay)'` | No replay window — REQUEST add. |
| L3.6 | Idempotency keys hashed before D1 / persistence storage. | `gh pr diff <n> \| grep -E '^\+.*(idempotency_key\|Idempotency-Key)'` | Raw key persisted — REQUEST hash. |
| L3.7 | Cross-tenant AAD on BYOK / crypto ops (INV-BYOK-CRYPTO-SOVEREIGNTY). AAD must include `tenant_id` or HMAC thereof. | `gh pr diff <n> \| grep -E '^\+.*(encrypt\|wrap_dek\|encryption_context)'` | Missing AAD or tenant binding — HARD REJECT, escalate AppSec + Architect. |
| L3.8 | New GitHub Actions `uses:` are SHA-pinned (40-char hex + `# vX.Y.Z` comment). Floating `@vN` / `@main` is P0. | `gh pr diff <n> -- '.github/workflows/' \| grep -E '^\+.*uses:'` | Floating tag — HARD REJECT. |
| L3.9 | Retention bounds ≥ 7y where regulatory; no silent shortening. | `gh pr diff <n> \| grep -E '^\+.*(retention\|expires_at)'` | Numeric retention < 7y on regulated path — REQUEST review + escalate Privacy. |
| L3.10 | New endpoints added to rate-limit catalog if customer-facing. | `gh pr diff <n> -- 'crates/corelink-rate-limit/'` cross-check. | New customer endpoint without rate-limit row — REQUEST add. |

**Verdict trigger.** L3.4 / L3.7 / L3.8 fail → HARD-REJECT. Others → REQUEST-CHANGES.

---

## L4 — Test Quality (5-8 min, spot-sample)

Tests must catch real bugs, not just pass.

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L4.1 | ≥ 1 property test per non-trivial invariant in new code. | `gh pr diff <n> \| grep -E '^\+.*proptest!'` count. | Substantial new logic with zero prop tests — REQUEST add. |
| L4.2 | ≥ 1 adversarial test per public happy-path function. Names follow `adversarial_*` / `negative_*` / `replay_*` / `tampered_*`. | `gh pr diff <n> \| grep -E '^\+.*fn (adversarial_\|negative_\|replay_\|tampered_)'` | New public surface without adversarial test — REQUEST add. |
| L4.3 | Tests use deterministic seeds; randomness via `proptest` framework, not `rand::thread_rng()` ad-hoc. | `gh pr diff <n> \| grep -E '^\+.*(thread_rng\|rand::random)'` cross-reference. | Non-deterministic randomness in tests — REQUEST `ChaCha20Rng::from_seed`. |
| L4.4 | Live-network tests gated `#[ignore]` AND `#[cfg(feature = "live-integration")]`. | `gh pr diff <n> -- 'tests/'` look for unguarded network. | Real-API test runs by default — REQUEST gate. |
| L4.5 | No test that only asserts `.is_ok()` / `.is_err()` without also checking the value (S-08 P1-1). | `gh pr diff <n> \| grep -E '^\+.*assert!\(.*\.(is_ok\|is_err)'` open + read. | Lone `is_ok()` on substantial logic — REQUEST destructure + assert fields. |
| L4.6 | Async-API crates have ≥ 1 runtime-exercising test. | Look at new `pub async fn` and verify a test calls it. | Async API w/ zero exercises — REQUEST add. |
| L4.7 | Test count not regressed (CI mutation matrix should hold or grow). | `gh pr checks <n>` mutation row. | Mutation kill rate dropped — REQUEST author investigate gap. |
| L4.8 | No test imports `tokio::*` directly when a sync test would suffice (charter). | `gh pr diff <n> -- 'tests/' \| grep -E '^\+.*use tokio'` | Spurious tokio in test that's sync-doable — suggest refactor (non-blocking). |

**Verdict trigger.** L4.1 / L4.2 / L4.5 fail on substantial code → REQUEST-CHANGES. Others advisory unless adversarial gap is obvious.

---

## L5 — Documentation (3-5 min)

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L5.1 | Every new crate has crate-level rustdoc (`//!` at top of `lib.rs`) covering purpose, charter constraints honored, API summary. | `gh pr diff <n> -- 'crates/*/src/lib.rs'` head. | New crate without `//!` block — REQUEST add. |
| L5.2 | Every public type has `///` docstring (`missing_docs = "deny"` enforces; verify lint live). | `gh pr diff <n> \| grep -E '^\+\s*pub (fn\|struct\|enum\|trait\|const)'` ensure preceding `///`. | New `pub` without doc — REQUEST add. |
| L5.3 | README.md updated if new top-level concept / customer-visible feature. | `gh pr diff <n> -- README.md` | New feature, README untouched — REQUEST update. |
| L5.4 | ADR filed for any architectural decision (new protocol, primitive, schema). Path: `specs/03_architecture/adrs/ADR-S*-*.md`. | `gh pr diff <n> -- 'specs/03_architecture/adrs/'` | Architectural change without ADR — REQUEST file ADR. |
| L5.5 | Spec contract changelog row added when scope or contract shifts. | `gh pr diff <n> -- 'specs/04_sprints/*/_spec_contract.md'` | Contract drift without changelog — REQUEST add row. |
| L5.6 | Frontmatter on every new spec doc (`type`, `doc_status`, `audit_status`, `version`, `created`, `updated`, `owner`, `final_approver`, `reviewers`). | `gh pr diff <n> -- 'specs/' --name-only` then `head -1` each. | Missing frontmatter — REQUEST add (L6.1 validate_specs will catch but flag in review too). |
| L5.7 | Deferred items (`TODO` / `FIXME`) reference a spec § or tracking issue. | `gh pr diff <n> \| grep -E '^\+.*(TODO\|FIXME)'` | Bare TODO without ref — REQUEST tag with `WI-` or `INV-` ID. |
| L5.8 | Customer-visible API/CLI change documented in `docs/customer/` AND CHANGELOG. | `gh pr diff <n> -- 'docs/customer/'` and root CHANGELOG. | Missing customer doc — REQUEST add. |

**Verdict trigger.** L5.4 / L5.6 fail → REQUEST-CHANGES. Others advisory.

---

## L6 — Spec Hygiene (2-4 min)

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L6.1 | `python3 scripts/validate_specs.py` exits 0 OR failure count ≤ pre-PR baseline. **NEVER goes up.** | `gh pr checks <n>` look for `validate_specs`. | Count rose — HARD REJECT (AP-5 anti-pattern). |
| L6.2 | `python3 scripts/check_migrations_additive.py` exits 0. No DROP / RENAME / ALTER COLUMN dropping. | `gh pr checks <n>` look for `migrations`. | Destructive migration — HARD REJECT (escalate to Architect + DPA review). |
| L6.3 | `scripts/validate_references.py` (if present) — no new dangling cross-refs. | `gh pr checks <n>`. | New dangling ref — REQUEST author resolve. |
| L6.4 | Migration numbering: next-free; no collisions with sibling worktrees on `main`. | `ls migrations/d1/` then check PR. | Number collision — REQUEST renumber. |
| L6.5 | Canonical registry IDs (`INV-` / `CTRL-` / `FM-` / `EVT-` / `PAT-` / `SLO-`) cross-link to registry doc; spot-check 2-3. | `gh pr diff <n> \| grep -E '(INV-\|CTRL-\|FM-\|EVT-\|PAT-\|SLO-)'` resolve in spec corpus. | Unknown ID coined — REQUEST author register it. |

**Verdict trigger.** L6.1 / L6.2 / L6.4 fail → HARD-REJECT.

---

## L7 — Merge Hygiene (4-7 min, immediately before merge)

| ID | Rule | Check command | Escalate if |
|---|---|---|---|
| L7.1 | Branch is rebased / merged-from `main` (no stale base). | `gh pr view <n> --json mergeable` and `gh pr view <n> --json baseRefOid`. | Stale base + behind > 50 commits — REQUEST rebase. |
| L7.2 | `Cargo.lock` policy followed: take `--ours` + `cargo update -w` on conflict; verify pinned security versions (rustls 0.23+, pyo3 0.24+, lru 0.18+) survived. | `gh pr diff <n> -- Cargo.lock \| head -50` | Pinned versions reverted (R1-9 regressions) — HARD REJECT. |
| L7.3 | Workspace `Cargo.toml` `[workspace] members` list unioned; no entries lost. | `gh pr diff <n> -- Cargo.toml` | Members list shrunk — REQUEST author re-add. |
| L7.4 | No conflict markers remain (`<<<<<<<` / `>>>>>>>` / `=======`). | `gh pr diff <n> \| grep -E '<<<<<<<\|>>>>>>>'` | Any match — HARD REJECT. |
| L7.5 | Final commit message format: `feat(<wi>): ...` / `fix(<wave>): ...` / `merge wt/<branch> (...) into main`. Co-authored-by line where applicable. | `gh pr view <n> --json commits` last commit. | Non-conforming — REQUEST author fix. |
| L7.6 | Pre-commit hooks pass without `--no-verify`. | Already L1.5; reconfirm at merge time. | `--no-verify` in last commit — HARD REJECT. |
| L7.7 | Test binary count after merge ≥ pre-merge baseline. | Compare `cargo test --workspace --no-run` before/after. | Count dropped — REQUEST investigate (potential silent test deletion). |

**Verdict trigger.** L7.2 / L7.4 / L7.6 → HARD-REJECT and roll back if already merged.

---

## Verdict matrix

| Outcome | Trigger | Action |
|---|---|---|
| `APPROVE` | L0-L7 all PASS or only advisory notes. | Click Approve. Add the green-rendered template at the bottom. Optionally invoke `/techlead <branch>` for orchestrator-side L8/L9 if merging now. |
| `REQUEST-CHANGES` | Any non-HARD-REJECT FAIL. | Comment per-row; list remediation. Author re-pushes; re-review. |
| `BLOCK` / HARD-REJECT | Any HARD-REJECT trigger fired (L0.6, L1.4/L1.5, L2.2/L2.6/L2.9, L3.4/L3.7/L3.8, L6.1/L6.2/L6.4, L7.2/L7.4/L7.6). | Comment with the violation ID + remediation. Tag relevant owner team (AppSec / Architect / Privacy). Do NOT approve under any circumstance. |
| `ESCALATE` | Author disputes a HARD-REJECT, or violation requires policy waiver. | Invoke `/techlead` on the branch; surface to Gustavo / final approver. Document waiver in commit body if granted. |

---

## Reviewer comment template (copy into the PR review body)

```markdown
## Code Review — L0-L7

- [ ] **L0 Sanity** — PR title, scope, branch, commit history clean. No secrets in diff. CI green at HEAD.
- [ ] **L1 Build/Lint** — CI build / clippy / test-no-run green. No smuggled `#[allow]`. No `--no-verify`.
- [ ] **L2 Charter** — `#[non_exhaustive]`, no `unsafe`, no `tokio` in `src/`, no `prop_assert!(matches!)`, audit fail-CLOSED order, no `unwrap`/`expect`/`panic` outside test, `#![forbid(unsafe_code)]`, no secrets in logs.
- [ ] **L3 Security** — auth before handler, audit before mutate, no PII in audit, `ConstantTimeEq` for HMAC, replay window, idempotency hashed, AAD on BYOK, SHA-pinned GHA, retention bounds, rate-limit registered.
- [ ] **L4 Tests** — ≥ 1 prop test per invariant, adversarial naming, deterministic seeds, live-network gated, no lone `is_ok()`.
- [ ] **L5 Docs** — crate `//!`, public `///`, README updated, ADR if architectural, changelog row, frontmatter, TODOs ref'd.
- [ ] **L6 Spec** — `validate_specs.py` not regressed, additive migrations, no dangling refs, migration number free, registry IDs valid.
- [ ] **L7 Merge** — rebased, `Cargo.lock --ours + cargo update -w`, members unioned, no conflict markers, commit format clean.

**Verdict:** `APPROVE` / `REQUEST-CHANGES` / `BLOCK` / `ESCALATE`

**Comments:** <inline by row ID>
```

---

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7) | Initial L0-L7 reviewer-side checklist mirroring `/techlead` SKILL v2.0.0. 50+ rows across 8 levels, each with rule + check command + escalate-if. Reviewer comment template + verdict matrix. GA-Gate "every PR reviewed" criterion artifact. |
