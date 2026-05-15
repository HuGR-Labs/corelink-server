<!--
  CoreLink PR template. Author fills the body; reviewer pastes the reviewer-checklist as a top-level review comment.

  Cross-refs:
  - docs/internal/AUTHOR-PRE-PR-CHECKLIST.md (author side, 30 rows)
  - docs/internal/CODE-REVIEW-CHECKLIST.md   (reviewer side, L0-L7)
  - docs/internal/TECHLEAD-CHECKLIST.md      (orchestrator L0-L10)
  - .claude/skills/techlead/SKILL.md         (full anti-pattern catalog)
-->

## Summary

<!-- 1-3 bullets: what changed, why. The reviewer reads this first. -->

-
-
-

## Linked work

- Work item / WI ID:
- Spec ref:
- Wave / sprint:
- Closes: # (issue, if any)

## Test plan

<!-- Concrete commands the reviewer can run. Include actual test counts (not estimates). -->

- [ ] `cargo build --workspace` — green
- [ ] `cargo clippy --workspace --tests -- -D warnings` — green
- [ ] `cargo test --workspace --no-run` — green; test count: <N>
- [ ] `cargo test --workspace` — green; pass count: <N>
- [ ] `python3 scripts/validate_specs.py` — failures count: <N> (baseline: <M>; must be ≤ M)
- [ ] Manual / e2e: <describe>

## Author Pre-PR Checklist

> Source: `docs/internal/AUTHOR-PRE-PR-CHECKLIST.md`. Tick every box BEFORE requesting review. Untickable rows → fix locally first.

### Sanity (L0)

- [ ] 1. Branch name follows convention (`wt/<wave>-<wi>-<slug>` or `<handle>/<short-desc>`).
- [ ] 2. `git status --short` clean post-final-commit.
- [ ] 3. Commit subjects follow `<type>(<scope>): <subject>`; bodies explain WHY.
- [ ] 4. No secrets / `.env` / `*.pem` / private keys committed.
- [ ] 5. PR title `<type>(<scope>): <subject>` < 70 chars; body covers WHY.

### Build & Lint (L1)

- [ ] 6. `cargo build --workspace` exits 0.
- [ ] 7. `cargo clippy --workspace --tests -- -D warnings` exits 0.
- [ ] 8. `cargo test --workspace --no-run` exits 0.
- [ ] 9. No new function-level `#[allow(clippy::...)]` in `src/`.
- [ ] 10. No `--no-verify` / hook-skip in any commit.
- [ ] 11. New crates added to root `Cargo.toml` `[workspace] members`.

### Charter (L2) — non-negotiable

- [ ] 12. `#[non_exhaustive]` on every new `pub enum` / `pub struct`.
- [ ] 13. Zero new `unsafe` outside FFI crates.
- [ ] 14. No `use tokio` in `src/` (non-test, non-`main.rs`).
- [ ] 15. No `prop_assert!(matches!(..., Variant { .. }))`.
- [ ] 16. `PROPTEST_CASES` via `proptest_cases(N)` helper.
- [ ] 17. Audit order: lookup → emit_audit → mutate_state.
- [ ] 18. Zero `unwrap()` / `expect(` / `panic!` / `unimplemented!()` / `todo!()` in `src/` outside `#[cfg(test)]`.
- [ ] 19. `#![forbid(unsafe_code)]` at top of every new non-FFI crate's `lib.rs`.
- [ ] 20. No raw secrets in `tracing::*` / `log::*` / `println!` / `eprintln!` / `dbg!`.

### Security & Privacy (L3)

- [ ] 21. New endpoints validate auth (Bearer PAT / Clerk session) BEFORE handler logic.
- [ ] 22. Every state mutation emits an audit event; no PII in payloads.
- [ ] 23. HMAC / signature verify uses `subtle::ConstantTimeEq`.
- [ ] 24. New GitHub Actions `uses:` SHA-pinned (40-hex + `# vX.Y.Z` comment).
- [ ] 25. BYOK / crypto changes include `tenant_id` (or HMAC) in AAD.

### Tests (L4)

- [ ] 26. ≥ 1 property test per non-trivial new invariant; adversarial test per public happy path.
- [ ] 27. No lone `.is_ok()` / `.is_err()` on substantial logic — destructure + assert fields.
- [ ] 28. Live-network tests gated `#[ignore]` AND `#[cfg(feature = "live-integration")]`.

### Docs & Spec (L5 + L6)

- [ ] 29. New crates have `//!` + `///` rustdoc; architectural changes have ADR; new spec docs have full frontmatter; spec-contract changelog row added if scope shifted.
- [ ] 30. `validate_specs.py` not regressed; `check_migrations_additive.py` exits 0; migration numbers don't collide with `main`.

## Risk

<!-- Answer briefly. Defaults to N/A only when truly inapplicable. -->

- **Customer-visible change?** yes / no — if yes: changelog updated, customer doc updated.
- **Schema / API contract change?** yes / no — if yes: migration story + backward-compat documented.
- **New crypto primitive / protocol?** yes / no — if yes: ADR filed.
- **Worst-case if shipped tomorrow with one undiscovered bug?** <one line>

## Reviewer notes

<!-- Optional. Anything the reviewer should look at first, gotchas, known follow-ups (with WI links). -->

-

---

<details>
<summary><strong>Reviewer Checklist (copy into a top-level review comment when reviewing)</strong></summary>

> Source: `docs/internal/CODE-REVIEW-CHECKLIST.md`. Walk L0-L7 in order; comment per row ID on any FAIL. HARD-REJECT triggers escalate to `/techlead` + tag the owner team.

### L0 — Sanity

- [ ] L0.1 PR title `<type>(<scope>): <subject>`.
- [ ] L0.2 Scope reasonable (< 2000 LoC, or clearly partitioned).
- [ ] L0.3 Branch name follows convention.
- [ ] L0.4 Commit subjects clean (no WIP / typo-spam / `--no-verify`).
- [ ] L0.5 Files claimed in PR body exist in the diff.
- [ ] L0.6 No secrets / `.env` / private keys in diff. (HARD-REJECT if violated.)
- [ ] L0.7 CI green at HEAD.

### L1 — Build & Lint

- [ ] L1.1 CI `cargo build` green.
- [ ] L1.2 CI `cargo clippy -D warnings` green (Rust 1.91).
- [ ] L1.3 CI `cargo test --no-run` green.
- [ ] L1.4 No new function-level `#[allow(clippy::...)]` in src. (HARD-REJECT.)
- [ ] L1.5 No `--no-verify` / hook-skip in commits. (HARD-REJECT.)
- [ ] L1.6 Workspace `Cargo.toml` `members` includes new crates.
- [ ] L1.7 `cargo-deny` green (license / advisory / bans).

### L2 — Charter Constraints

- [ ] L2.1 `#[non_exhaustive]` on new public types.
- [ ] L2.2 No new `unsafe` outside FFI. (HARD-REJECT.)
- [ ] L2.3 No `tokio` import in `src/`.
- [ ] L2.4 No `prop_assert!(matches!)`.
- [ ] L2.5 `PROPTEST_CASES` via runtime helper.
- [ ] L2.6 Audit order: lookup → emit → mutate. (HARD-REJECT.)
- [ ] L2.7 No `unwrap`/`expect`/`panic` outside test.
- [ ] L2.8 `#![forbid(unsafe_code)]` at new-crate root.
- [ ] L2.9 No secrets in logs. (HARD-REJECT.)

### L3 — Security & Privacy

- [ ] L3.1 Auth-before-handler on new endpoints.
- [ ] L3.2 Audit emit before mutation.
- [ ] L3.3 No PII in audit payloads.
- [ ] L3.4 `subtle::ConstantTimeEq` for HMAC. (HARD-REJECT.)
- [ ] L3.5 Replay window enforced on timestamp-bound signatures.
- [ ] L3.6 Idempotency keys hashed before storage.
- [ ] L3.7 AAD includes `tenant_id` on BYOK ops. (HARD-REJECT.)
- [ ] L3.8 GHA `uses:` SHA-pinned. (HARD-REJECT.)
- [ ] L3.9 Retention bounds ≥ 7y on regulated paths.
- [ ] L3.10 New customer endpoints in rate-limit catalog.

### L4 — Tests

- [ ] L4.1 ≥ 1 prop test per non-trivial invariant.
- [ ] L4.2 Adversarial naming on negative-path tests.
- [ ] L4.3 Deterministic seeds (no `thread_rng` in tests).
- [ ] L4.4 Live-network tests gated.
- [ ] L4.5 No lone `is_ok()` on substantial logic.
- [ ] L4.6 Async APIs exercised by runtime test.
- [ ] L4.7 Mutation kill rate not regressed.
- [ ] L4.8 No spurious `tokio` in sync-doable tests.

### L5 — Docs

- [ ] L5.1 Crate-level `//!` rustdoc on new crates.
- [ ] L5.2 `///` on every new `pub` item.
- [ ] L5.3 README updated if top-level concept changed.
- [ ] L5.4 ADR filed for architectural decisions.
- [ ] L5.5 Spec-contract changelog row when scope shifts.
- [ ] L5.6 Frontmatter on new spec docs.
- [ ] L5.7 TODOs reference WI / INV / spec §.
- [ ] L5.8 Customer-visible changes in `docs/customer/` + CHANGELOG.

### L6 — Spec Hygiene

- [ ] L6.1 `validate_specs.py` count ≤ baseline. (HARD-REJECT if up.)
- [ ] L6.2 `check_migrations_additive.py` clean. (HARD-REJECT.)
- [ ] L6.3 No new dangling cross-refs.
- [ ] L6.4 Migration numbering free (no collision). (HARD-REJECT.)
- [ ] L6.5 Registry IDs (INV/CTRL/FM/EVT/PAT/SLO) resolve.

### L7 — Merge Hygiene

- [ ] L7.1 Branch rebased / merged-from `main`.
- [ ] L7.2 `Cargo.lock` policy honored; security pins survived. (HARD-REJECT on regression.)
- [ ] L7.3 Workspace `members` unioned.
- [ ] L7.4 No conflict markers. (HARD-REJECT.)
- [ ] L7.5 Final commit message conforms.
- [ ] L7.6 No `--no-verify` on last commit. (HARD-REJECT.)
- [ ] L7.7 Test binary count ≥ baseline.

### Verdict

- [ ] `APPROVE` — all rows pass or advisory only.
- [ ] `REQUEST-CHANGES` — comments inline by row ID.
- [ ] `BLOCK` / HARD-REJECT — any HARD-REJECT trigger fired; tag AppSec / Architect / Privacy as relevant.
- [ ] `ESCALATE` — invoke `/techlead <branch>` and surface to final approver.

</details>
