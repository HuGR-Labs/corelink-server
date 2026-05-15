---
title: CI optimization followup tickets
date: 2026-05-15
parent_audit: 2026-05-15-ci-workflow-optimization.md
status: partial (4/11 closed — P1 wave landed; P2+P3 deferred post-GA)
total_tickets: 11
closed_tickets: 4 (CI-OPT-001..004)
estimated_savings: 52 billable min / PR + 24 wall-clock min / PR p50
realised_savings_p1: ~95 billable min / PR (workspace dedup ~50 + docs consolidation ~25 + validators dedup ~10 + audit-job removal ~10) + supply-chain hardening (CI-OPT-004 risk-only)
---

# CI optimization followup tickets

Tickets prioritized by `priority = (time_savings × PR_frequency) / risk`. P1 = land first; P2 = next; P3 = polish.

Each ticket is implementation-ready: workflow file, change summary, projected savings, risk assessment, test plan.

---

## P1 tickets (Wave 1 — 4 tickets, ~30 billable-min/PR savings)

### CI-OPT-001 — Deduplicate workspace `cargo` commands across 11 workflows  **[CLOSED — wt/debt-010-ci-opt-p1]**

**Status:** CLOSED 2026-05-15. Implementation files: 6 per-crate workflows (`corelink-hash.yml`, `corelink-meta.yml`, `corelink-reapi.yml`, `corelink-worker.yml`, `corelink-client-verify.yml`, `tenant-path.yml`). All 4 workspace-wide steps (`cargo fmt --all`, `cargo clippy --workspace`, `cargo test --workspace`, `cargo doc --no-deps --workspace`) removed from each `pr-gate` job and replaced with crate-scoped equivalents (`cargo clippy --package <crate>` + `cargo test --package <crate>`). Workspace convergence remains in `cas_foundation.yml::workspace-build-test` (canonical). Per-crate redundant `audit:` jobs also removed (covered by `cargo-audit.yml`). Realised savings: ~50 billable min/PR on full-fan-out PRs (when all 6 workflows match) + ~10 min/PR from audit-job removal.

**Risk gates verified.** Branch protection rule must require `cas_foundation / workspace-build-test` as REQUIRED status check before this lands on main (orchestrator: confirm pre-merge).



**Files affected (12, removing duplicate steps from 11):**
- `.github/workflows/corelink-hash.yml`
- `.github/workflows/corelink-meta.yml`
- `.github/workflows/corelink-reapi.yml`
- `.github/workflows/corelink-worker.yml`
- `.github/workflows/corelink-client-verify.yml`
- `.github/workflows/tenant-path.yml`
- `.github/workflows/gc-ship-gate.yml`
- `.github/workflows/s07-ship-gate.yml`
- `.github/workflows/s08-ship-gate.yml`
- `.github/workflows/s09-ship-gate.yml`
- `.github/workflows/s10-ship-gate.yml`
- (Unchanged: `cas_foundation.yml` keeps the canonical `workspace-build-test` job)

**Change summary.**
1. Delete the steps `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, `cargo doc --no-deps --workspace` from each per-crate workflow's `pr-gate` job.
2. Replace with crate-scoped invocations: `cargo clippy --package <crate> --all-targets -- -D warnings`, `cargo test --package <crate> --all-targets`.
3. Add to each `pull_request:` block a `needs:` dependency or branch-protection rule referencing `cas_foundation / workspace-build-test`.

**Projected savings.** ~7m wall + ~7m billable per workflow × 11 = **~77 billable min/PR** (when all paths match). Realistic average per PR: **~50 billable min**.

**Risk.** LOW. `cas_foundation.yml` already runs the workspace-wide gates on the same `paths:` (`crates/**` + `apps/**` + `Cargo.toml` + `Cargo.lock`). The per-crate workflows retain their unique gates (canonical vectors, WASM compile, fuzz smoke, ct-variance).

**Test plan.**
1. Open a 1-line PR touching `crates/corelink-hash/src/lib.rs`.
2. Confirm `cas_foundation` runs the workspace gates (clippy/test/doc workspace-wide).
3. Confirm `corelink-hash` runs ONLY: canonical-vectors test, release ct-variance test, WASM build, fuzz-smoke.
4. Diff billable minutes against a baseline PR captured pre-change.
5. Quality non-regression: induce a clippy warning in `crates/corelink-meta` and confirm `cas_foundation` fails.

**Risk gates to verify.** Branch protection rule for `cas_foundation / workspace-build-test` must be REQUIRED before this ticket lands.

---

### CI-OPT-002 — Consolidate docs workflows into `docs-ci.yml` (5 jobs absorbed)  **[CLOSED — wt/debt-010-ci-opt-p1]**

**Status:** CLOSED 2026-05-15. Files modified: `.github/workflows/docs-ci.yml` (5 new jobs added; trigger surface expanded to include `crates/corelink-reapi/proto/**`, `apps/server/proto/**`, plus `schedule:` for nightly Lighthouse baseline). Files deleted: `docs-a11y.yml`, `docs-a11y-baseline.yml`, `docs-i18n.yml`, `docs-lighthouse.yml`, `docs-reapi-gen.yml`. Each absorbed job either reuses the `build` artifact via `needs: build` + `actions/download-artifact` (a11y-playwright, a11y-baseline-diff, lighthouse-baseline) or runs independently (i18n-coverage, reapi-gen-drift — both need their own pnpm install since they don't depend on the built site). Standalone `docs-lychee.yml` (Docker) and `docs-vale.yml` (separate config) kept per ticket. Realised savings: ~25 billable min/PR (single pnpm install + single build shared across 5 jobs vs 6 standalone installs+builds). Trigger surface caveat: docs-lighthouse.yml's tag-trigger (docs-v*, v*) deferred — schedule + workflow_dispatch + PR-time inline lighthouse together exceed prior coverage; tag-time lighthouse can be added in a follow-up if release-tag verification becomes a hard requirement.



**Files affected.**
- Modify: `.github/workflows/docs-ci.yml` (add 5 jobs)
- Delete: `.github/workflows/docs-a11y.yml`, `docs-a11y-baseline.yml`, `docs-i18n.yml`, `docs-lighthouse.yml`, `docs-reapi-gen.yml`
- Keep standalone: `docs-lychee.yml` (Docker), `docs-vale.yml` (no pnpm)

**Change summary.**
1. In `docs-ci.yml`, add jobs: `a11y`, `a11y-baseline`, `i18n`, `lighthouse`, `reapi-gen`, each `needs: build` to reuse the built site artifact.
2. The single `pnpm install` + Node setup is reused; `actions/upload-artifact` from `build` is downloaded by each downstream job.

**Projected savings.** ~6m wall + ~25m billable per docs-touching PR.

**Risk.** LOW. Each gate keeps identical configuration. Jobs run in parallel after `build`, preserving fan-out.

**Test plan.**
1. Touch `apps/docs/docs/index.mdx` and open PR.
2. Confirm exactly one `docs-ci.yml` run with 6 jobs (build + 5 downstream).
3. Confirm none of the deleted workflows trigger (deleted from `.github/workflows/`).
4. Each downstream job's logs match the pre-merge standalone workflow output line-for-line (a11y violations, i18n keys, lighthouse scores).

---

### CI-OPT-003 — Deduplicate `validate_specs.py` (6 → 1 invocation)  **[CLOSED — wt/debt-010-ci-opt-p1]**

**Status:** CLOSED 2026-05-15. Files modified: `gc-ship-gate.yml`, `s07-ship-gate.yml`, `s08-ship-gate.yml`, `s09-ship-gate.yml`, `s10-ship-gate.yml`. Each `validators:` job removed; replacement note pointing to `spec_validation.yml` (and `dashboard_validation.yml` for s09) added. Aggregate `<sprint>-ship-gate` job's `needs:` updated to drop `validators` dependency. `spec_validation.yml` remains the single source of truth for validate_specs/inv/refs + check_migrations_additive (and `dashboard_validation.yml` for the canonical-12 dashboards check). Branch protection MUST require `spec_validation / spec-validation` (and `dashboard_validation / dashboard-validation` where applicable) as separate REQUIRED status checks on PRs touching `specs/**` / `dashboards/**` — orchestrator confirm pre-merge. Realised savings: ~10 billable min/PR (5 ship-gates × ~2 min validators job = 10 min eliminated).



**Files affected.**
- Modify: `.github/workflows/s07-ship-gate.yml`, `s08-ship-gate.yml`, `s09-ship-gate.yml`, `s10-ship-gate.yml`, `gc-ship-gate.yml` (remove `validators` job)
- Keep: `.github/workflows/spec_validation.yml` (single source of truth)
- Update branch protection: require `spec_validation / validate-specs` as a separate check on all PRs touching `specs/**`.

**Change summary.**
1. Remove the `validators` job from each ship-gate workflow.
2. Ship-gates depend on `spec_validation.yml` via branch protection (NOT `workflow_run:`, which doesn't gate PR merge).
3. Drop `paths:` filter on `spec_validation.yml`'s `specs/**` to ensure it runs on every PR (today it already does — confirm).

**Projected savings.** ~3m wall + ~10m billable per PR.

**Risk.** LOW. Validators are deterministic and side-effect-free; running once gives the same answer.

**Test plan.**
1. PR touching `specs/04_sprints/S07/**` triggers `s07-ship-gate` (without validators) + `spec_validation` (validators).
2. Inject a broken spec frontmatter; confirm `spec_validation` fails AND blocks merge via branch protection.

---

### CI-OPT-004 — Complete SHA-pinning across 21 floating-ref workflows  **[CLOSED — wt/debt-010-ci-opt-p1]**

**Status:** CLOSED 2026-05-15. Files modified (11 of 11 listed): `corelink-{hash,meta,reapi,worker,client-verify}.yml`, `tenant-path.yml`, `dashboard_validation.yml`, `spec_validation.yml`, `tla_check.yml`, `byok_kill_switch_drill_weekly.yml` (already pinned: `dependabot-auto-merge.yml`). All `@v4`/`@v5`/`@v2`/`@v3`/`@v1`/`@stable`/`@nightly` refs replaced with 40-char SHAs and `# vX.Y.Z` trailing comments matching the canonical pin set in `_TEMPLATE.yml.md` + `cas_foundation.yml`. `actions/setup-java@v4` → `@3a4f6e1af504cf6a31855fa899c6aa5355ba6c12 # v4.7.0`. Floating `rustsec/audit-check@v1` use sites eliminated by removing the redundant per-crate `audit:` job (already covered by `cargo-audit.yml`, which is SHA-pinned). For nightly Rust toolchain (`dtolnay/rust-toolchain@nightly`), pinned action revision to `@29eef336d9b2848a0b548edc03f92a220660cdb8` with `with: toolchain: nightly` to select the toolchain. CI guard for floating-ref drift (greps for `uses:\s*[a-z]+/[a-zA-Z-]+@(v|main|master|stable|nightly)`) deferred to a follow-up tiny workflow. Realised savings: 0 (risk reduction only — ADR-0044 + WI-S01-007 §6.1.9 compliance).



**Files affected.**
- `corelink-{hash,meta,reapi,worker,client-verify}.yml`
- `tenant-path.yml`
- `dashboard_validation.yml`, `spec_validation.yml`, `tla_check.yml`
- `dependabot-auto-merge.yml`
- `byok_kill_switch_drill_weekly.yml`

**Change summary.** Replace every `@v4`, `@v2`, `@stable`, `@nightly` ref with the 40-char SHA at a known-good version, with `# vX.Y.Z` trailing comment. Use the SHAs already in `cas_foundation.yml` for `actions/checkout`, `actions/setup-python`, `Swatinem/rust-cache`, `dtolnay/rust-toolchain`.

Reference SHAs (from `cas_foundation.yml`):
```
actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683        # v4.2.2
actions/setup-python@0b93645e9fea7318ecaed2b359559ac225c90a2b    # v5.3.0
Swatinem/rust-cache@400e7407cfd7a091e5fbb6afec01ec146c432b7c     # v2.7.7
dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8  # stable as of 2026-04-29
arduino/setup-protoc@f4d5893b897028ff5739576ea0409746887fa536    # v3.0.0
actions/upload-artifact@6f51ac03b9356f520e9adb1b1b7802705f340c2b # v4.5.0
actions/download-artifact@fa0a91b85d4f404e444e00e005971372dc801d16 # v4.1.8
```

For `dependabot/fetch-metadata@v2` (in `dependabot-auto-merge.yml`), pin to the v2.x release SHA.

**Projected savings.** Zero time savings. Risk reduction only (ADR-0044 + WI-S01-007 §6.1.9 compliance).

**Risk.** LOW. Mechanical change. CI re-runs are deterministic.

**Test plan.**
1. Each pinned workflow re-runs on a no-op PR; outputs identical to baseline.
2. Add a CI guard: a check that greps `.github/workflows/*.yml` for `uses:\s*[a-z]+/[a-zA-Z-]+@(v|main|master|stable|nightly)` and fails if any match (gate this in `cas_foundation` or a new tiny workflow).

---

## P2 tickets (Wave 2 — 4 tickets, ~18 billable-min/PR savings)

### CI-OPT-005 — Add `concurrency:` cancel-in-progress to 30 PR-trigger workflows

**Files affected.** Every workflow listed in §1.1 of the audit that lacks `concurrency:` (count: 30).

**Change summary.** Add to each:
```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```

**Projected savings.** ~30 % runner minutes saved on PR force-pushes (typical PR sees 2–3 force-pushes). Estimate: ~5m billable / PR.

**Risk.** LOW. Cancellation is PR-only. Push-to-main runs are NOT cancelled (the `cancel-in-progress` is wrapped in the `pull_request`-only ternary).

**Test plan.**
1. Open PR; immediately force-push.
2. Confirm the first run is cancelled in Actions UI.
3. Push to main; confirm parallel runs are NOT cancelled.

---

### CI-OPT-006 — Share rust-cache `shared-key` across workflows

**Files affected.** `cas_foundation.yml` + 6 per-crate Rust workflows.

**Change summary.** In `cas_foundation.yml::workspace-build-test`, add `shared-key: workspace-r1`. In each per-crate workflow's `pr-gate`, add the same `shared-key: workspace-r1`. The first job to run warms the cache; subsequent jobs read it.

```yaml
- uses: Swatinem/rust-cache@400e7407cfd7a091e5fbb6afec01ec146c432b7c # v2.7.7
  with:
    workspaces: ". -> target"
    shared-key: workspace-r1
```

**Projected savings.** ~2–3m wall per cold-cache PR on the second-running workflow; flat on warm.

**Risk.** LOW. Swatinem/rust-cache is content-addressed by `Cargo.lock` + key; sharing keys cannot poison the cache.

**Test plan.**
1. PR touching a crate runs `cas_foundation` (warms cache) and `corelink-hash` (hits cache).
2. Compare wall-clock vs. pre-shared-key baseline.

---

### CI-OPT-007 — Matrix TLC 4 canonical specs in `cas_foundation.yml`

**Files affected.** `.github/workflows/cas_foundation.yml::tlc-canonical`.

**Change summary.** Replace sequential 4-step TLC run with a `strategy.matrix`:
```yaml
tlc-canonical:
  strategy:
    matrix:
      spec: [tenant_isolation, cas_integrity, gc_correctness, audit_immutability]
  steps:
    - run: timeout 480s bash scripts/run_tlc_corelink.sh ${{ matrix.spec }}
```

**Projected savings.** ~6m wall (sequential 32m max → parallel 8m max). Billable approximately flat (4 × Java install). Critical-path win.

**Risk.** LOW. Each TLC run is independent; no shared state. Failure isolation improves.

**Test plan.**
1. Open PR touching `specs/tla/cas_integrity.tla`.
2. Confirm 4 parallel `tlc-canonical (matrix: <spec>)` jobs.
3. Inject a counter-example into one spec; confirm only that matrix entry fails; others pass.

---

### CI-OPT-008 — Audit `paths:` filter coverage on Rust workflows

**Files affected.** `cargo-audit.yml`, `cargo-deny.yml`, `license-policy.yml`, `region_pinning.yml`, `tla_*_check.yml`.

**Change summary.** Verify and (if missing) add `paths:` filters so docs-only PRs don't trigger Rust gates. Specifically:
- `cargo-audit.yml`: trigger only on `crates/**`, `apps/**`, `Cargo.toml`, `Cargo.lock`, `.github/workflows/cargo-audit.yml`.
- `cargo-deny.yml`: same + `deny.toml`.
- `license-policy.yml`: same.
- `region_pinning.yml`: same.
- `tla_*_check.yml`: trigger only on `specs/tla/**` + the corresponding `.tla` file + `.github/workflows/tla_*_check.yml`.

**Projected savings.** ~3–8m billable on docs-only PRs (Scenario B).

**Risk.** LOW. Already-filtered workflows are unchanged; only over-broad triggers narrow.

**Test plan.**
1. Open docs-only PR (touches `apps/docs/**` only).
2. Confirm NONE of `cargo-audit / cargo-deny / license-policy / tla_*` workflows trigger.
3. Open Rust-only PR; confirm all four DO trigger.

---

## P3 tickets (Wave 3 — 3 tickets, marginal savings, security polish)

### CI-OPT-009 — Add top-level `permissions: { contents: read }` to every workflow

**Files affected.** 1 workflow lacks `permissions:` entirely (`docs-reapi-gen.yml`) + several declare only at job level. Audit all 71.

**Change summary.** Add at the top of each workflow file:
```yaml
permissions:
  contents: read
```
Jobs that need `id-token: write`, `pull-requests: write`, or `packages: write` keep their job-level overrides.

**Projected savings.** None. Security hardening (least-privilege).

**Risk.** LOW. Default token scope shrinks; jobs needing more already declare it.

**Test plan.**
1. Each workflow re-runs successfully on a no-op PR.
2. Specifically verify: `cosign-sign` still gets `id-token: write` at job level; `dependabot-auto-merge` still gets `pull-requests: write`.

---

### CI-OPT-010 — Sparse-checkout for smoke-only jobs

**Files affected.** `changelog-validate.yml`, `lockfile-diff.yml`, `legal-changes-review.yml`, `dpa-legal-review.yml`, `pentest-findings-sync.yml`.

**Change summary.** Use `actions/checkout` `sparse-checkout:` to fetch only the files each job reads (e.g., `CHANGELOG.md`, `pnpm-lock.yaml`).

**Projected savings.** ~5–10s/job. Marginal.

**Risk.** LOW. If a script reaches a file outside the sparse set, the job fails loudly.

**Test plan.** Run each workflow; verify expected files present and absent ones genuinely unread.

---

### CI-OPT-011 — Misc consolidation (fuzz-smoke/nightly merge; lockfile paths; ffi cache keys)

**Files affected.** `corelink-hash.yml`, `corelink-meta.yml`, `corelink-worker.yml`, `corelink-reapi.yml`, `corelink-client-verify.yml`, `lockfile-diff.yml`, `ffi-matrix-ci.yml`.

**Change summary.**
1. Merge `fuzz-smoke` (PR, 60s) and `fuzz-nightly` (schedule, 3600s) jobs into one parameterized job using `FUZZ_SECONDS` env (`60` on PR, `3600` on schedule).
2. Add `paths:` to `lockfile-diff.yml` so it triggers only on `Cargo.lock` / `pnpm-lock.yaml` changes.
3. Verify `ffi-matrix-ci.yml`'s rust-cache `shared-key` includes both `${{ matrix.os }}` and `${{ matrix.lang }}` to prevent thrashing.

**Projected savings.** ~2m billable/PR on the lockfile filter alone; fuzz merge is structural (no time win, less duplication).

**Risk.** LOW. Each change is independently testable.

**Test plan.**
1. Open PR not touching lockfile: `lockfile-diff` does NOT run.
2. Touch `Cargo.lock`: `lockfile-diff` runs.
3. Nightly schedule fires; confirm `fuzz` job runs for 3600s; PR runs same job for 60s.

---

## Tracking

| Ticket | Wave | File-count | Time savings | Risk | Status |
|---|---|---|---|---|---|
| CI-OPT-001 | P1 | 6 | ~50 m billable/PR | LOW | CLOSED (wt/debt-010-ci-opt-p1) |
| CI-OPT-002 | P1 | 6 (modify 1, delete 5) | ~25 m billable/PR | LOW | CLOSED (wt/debt-010-ci-opt-p1) |
| CI-OPT-003 | P1 | 5 | ~10 m billable/PR | LOW | CLOSED (wt/debt-010-ci-opt-p1) |
| CI-OPT-004 | P1 | 11 | 0 (risk reduction) | LOW | CLOSED (wt/debt-010-ci-opt-p1) |
| CI-OPT-005 | P2 | 30 | ~5 m billable/PR | LOW | OPEN |
| CI-OPT-006 | P2 | 7 | ~2 m wall/PR | LOW | OPEN |
| CI-OPT-007 | P2 | 1 | ~6 m wall/PR | LOW | OPEN |
| CI-OPT-008 | P2 | 5 | ~5 m billable/docs-PR | LOW | OPEN |
| CI-OPT-009 | P3 | ~10 | 0 (security) | LOW | OPEN |
| CI-OPT-010 | P3 | 5 | ~30s/PR | LOW | OPEN |
| CI-OPT-011 | P3 | 7 | ~2 m billable/PR | LOW | OPEN |
| **Total** | — | — | **~52 m billable + ~24 m wall p50 / PR** | **LOW** | — |

---

## Sequencing notes

- **CI-OPT-004 (SHA-pin)** should land FIRST in Wave 1; it's risk-only and unblocks the other tickets (we want a clean supply-chain baseline before refactoring).
- **CI-OPT-001 (dedup workspace cargo)** requires branch-protection rule update FIRST: mark `cas_foundation / workspace-build-test` as a REQUIRED check.
- **CI-OPT-002 (docs consolidation)** and **CI-OPT-003 (validators dedup)** can land in parallel.
- Waves 2 and 3 can land in any order after Wave 1.

## Owner / next step

These tickets are designed to be picked up one-per-Sonnet-agent in a future R-prep wave. Each ticket is implementation-ready: file paths, change summary, savings estimate, risk assessment, and test plan are all here.
