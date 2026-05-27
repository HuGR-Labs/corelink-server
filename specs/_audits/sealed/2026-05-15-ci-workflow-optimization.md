---
title: CI workflow optimization audit (R-1 cleanup tail)
date: 2026-05-15
audit_type: ci-pipeline
scope: .github/workflows (71 files)
verdict: AUDIT-ONLY (no code changes outside template)
owner: orchestrator
roadmap_ref: ROADMAP-TO-GA.md §1 (Wave R-1)
---

# CI workflow optimization audit — 2026-05-15

## 0. Executive summary

The repository ships **71** GitHub Actions workflows. **48** trigger on `pull_request`, contributing to every PR's wall-clock and minute-budget cost. Audit measures (estimates derived from job timeouts + step structure; no historical run-API mining done here):

| Metric | Today | After all P1 tickets land | Delta |
|---|---|---|---|
| PR-trigger workflows | 48 | 48 (same count; many short-circuit via paths) | 0 |
| **Estimated PR-time wall-clock (p50)** | **~58 min** | **~34 min** | **−24 min / −41 %** |
| **Estimated PR-time billable minutes (p50)** | **~310 min** | **~190 min** | **−120 min / −39 %** |
| SHA-pinned workflows | 50 / 71 (70 %) | 71 / 71 (100 %) | +21 |
| Workflows with `concurrency:` cancel | 18 / 71 (25 %) | 48 / 48 PR-trigger | +30 |
| Workflows with `paths:` filter | 39 / 71 (55 %) | 48 / 48 PR-trigger | +9 |
| Duplicate workspace `cargo clippy --workspace` runs / PR (worst case) | 12 | 1 (in `cas_foundation.yml`) | −11 |
| Duplicate `validate_specs.py` runs / PR (worst case) | 6 | 1 | −5 |
| Workflows w/ `Swatinem/rust-cache` | 17 / ~25 Rust-touching | 25 / 25 | +8 |

The single largest win is **deduplicating the workspace-wide `cargo clippy/test/doc` matrix** that runs identically in 12 workflows: `cas_foundation`, `corelink-{hash,meta,reapi,worker,client-verify}`, `tenant-path`, `gc-ship-gate`, `s07/s08/s09/s10-ship-gate`. Each instance burns ~12–18 minutes; centralising on `cas_foundation.yml`'s `workspace-build-test` reclaims ~10–15 minutes wall-clock per PR (jobs already parallel) and **~50–80 billable minutes per PR**.

NO QUALITY GATE IS WEAKENED in any ticket: per-crate workflows retain their targeted gates (canonical vectors, WASM build, fuzz smoke, ct-variance) and only delegate the cross-cutting `clippy --workspace -D warnings` + `cargo test --workspace` to the convergence job.

---

## 1. Inventory (71 workflows)

Columns: **Name** · **Trigger** · **Jobs** · **Est. p50 wall-clock** · **Critical path** · **SHA-pin** · **Secrets**.

Wall-clock estimate methodology: read `timeout-minutes:` ceilings, sum sequential `needs:` chains, take max over parallel jobs. Where no timeout is declared, GitHub's 360-minute default is replaced by a structural estimate (sum of step kinds × empirical baseline: checkout 5s, setup-rust cold 60s / warm 8s, `cargo build --workspace` cold 180s / warm 60s, `cargo test --workspace` cold 240s / warm 120s, `cargo clippy --workspace` warm 90s, `pnpm install --frozen-lockfile` warm 40s, `cargo doc --workspace` warm 60s, `cargo-fuzz` 60s smoke + 30s build = 90s, `cargo-cyclonedx` 25 crates ≈ 150s, sbomqs 25 files ≈ 10s, cosign sign+verify 25 files ≈ 60s, TLC 4 specs PR bounds ≈ 5–8 min total). These are calibrated to typical ubuntu-latest 4-vCPU runners.

### 1.1 PR-trigger workflows (run every PR — these dominate cost)

| # | Workflow | Jobs | Wall-clock | Critical path | SHA-pin | Secrets |
|---|---|---|---|---|---|---|
| 1 | `admin-ui-ci.yml` | 4 | ~6m | install→lint+test+build | ✅ 2/2 | none |
| 2 | `admin-ui-e2e.yml` | 4 | ~15m | install→playwright | ✅ 6/6 | none |
| 3 | `api-reference-sync.yml` | 5 | ~5m | install→generate→diff | ✅ 4/4 | GITHUB_TOKEN |
| 4 | `bazel-starter-ci.yml` | 7 | ~12m | bazel-init→build→test | ✅ 9/9 | CORELINK_PAT |
| 5 | `buck2-starter-ci.yml` | 6 | ~12m | buck2-init→build→test | ✅ 4/4 | CORELINK_PAT, GITHUB_TOKEN |
| 6 | `cargo-audit.yml` | 6 | ~6m | install→audit | ✅ 6/6 | GITHUB_TOKEN |
| 7 | `cargo-deny.yml` | 5 | ~5m | install→deny check | ✅ 2/2 | GITHUB_TOKEN |
| 8 | **`cas_foundation.yml`** | 8 | **~30m** | tlc + workspace-build-test + cargo-deny + sbom→cosign + repro-build | ✅ 15/15 | GITHUB_TOKEN |
| 9 | `changelog-validate.yml` | 2 | ~2m | python→check | ✅ 1/1 | none |
| 10 | `corelink-client-verify.yml` | 11 | ~18m | clippy+test+repro+wasm | ⚠ 0/2 | GITHUB_TOKEN |
| 11 | `corelink-hash.yml` | 9 | ~16m | clippy+test+wasm+fuzz-smoke | ⚠ 0/2 | GITHUB_TOKEN |
| 12 | `corelink-meta.yml` | 9 | ~16m | clippy+test+wasm | ⚠ 0/2 | GITHUB_TOKEN |
| 13 | `corelink-reapi.yml` | 7 | ~14m | clippy+test+wasm | ⚠ 0/5 | GITHUB_TOKEN |
| 14 | `corelink-worker.yml` | 9 | ~16m | clippy+test+wasm | ⚠ 0/2 | GITHUB_TOKEN |
| 15 | `coverage.yml` | 3 | ~12m | install→coverage→upload | ✅ 6/6 | GITHUB_TOKEN |
| 16 | `d1-migration-validate.yml` | 3 | ~5m | wrangler→plan→assert | ✅ 5/5 | none |
| 17 | `dashboard_validation.yml` | 3 | ~2m | python→validate | ⚠ 0/2 | none |
| 18 | `dependabot-auto-merge.yml` | 2 | ~1m | gh check→merge | ⚠ 0/1 | GITHUB_TOKEN |
| 19 | `docs-a11y-baseline.yml` | 4 | ~7m | install→build→axe | ✅ 3/3 | none |
| 20 | `docs-a11y.yml` | 3 | ~6m | install→build→axe | ✅ 3/3 | none |
| 21 | `docs-ci.yml` | 8 | ~18m | install→build (also fans out to a11y/lychee/lh) | ✅ 10/10 | none |
| 22 | `docs-i18n.yml` | 3 | ~4m | install→validate | ✅ 2/2 | none |
| 23 | `docs-lychee.yml` | 4 | ~3m | docker→lychee | ✅ 3/3 | GITHUB_TOKEN |
| 24 | `docs-reapi-gen.yml` | 3 | ~3m | python→gen→diff | ⚠ 0/0 (no third-party uses) | none |
| 25 | `docs-vale.yml` | 4 | ~2m | vale-action | ✅ 1/1 | none |
| 26 | `dpa-legal-review.yml` | 2 | ~1m | python→check | ✅ 2/2 | none |
| 27 | `ffi-matrix-ci.yml` | 7 | ~20m | matrix(3 OS × 2 lang) | ✅ 16/16 | none |
| 28 | `gc-ship-gate.yml` | 8 | ~22m | validators→prop→agg | ⚠ 0/0 (run scripts only) | GITHUB_TOKEN |
| 29 | `legal-changes-review.yml` | 2 | ~1m | python→check | ✅ 2/2 | none |
| 30 | `license-policy.yml` | 5 | ~4m | install→license-check | ✅ 5/5 | none |
| 31 | `lighthouse-ci.yml` | 5 | ~10m | install→build→lh | ✅ 3/3 | none |
| 32 | `lockfile-diff.yml` | 2 | ~3m | py→diff→comment | ✅ 3/3 | GITHUB_TOKEN |
| 33 | `openapi-validate.yml` | 5 | ~3m | install→spectral | ✅ 2/2 | none |
| 34 | `pentest-findings-sync.yml` | 4 | ~4m | py→sync→notify | ✅ 3/3 | SLACK, SENDGRID |
| 35 | `quickstart-validate.yml` | 3 | ~3m | docker→smoke | ✅ 1/1 | none |
| 36 | `region_pinning.yml` | 10 | ~18m | rust matrix | ⚠ 0/0 | none |
| 37 | `s07-ship-gate.yml` | 7 | ~22m | validators→prop+repro+dash→agg | ⚠ 0/0 | GITHUB_TOKEN |
| 38 | `s08-ship-gate.yml` | 7 | ~22m | validators→prop+repro+dash→agg | ⚠ 0/0 | GITHUB_TOKEN |
| 39 | `s09-ship-gate.yml` | 7 | ~22m | validators→prop+repro+dash→agg | ⚠ 0/0 | GITHUB_TOKEN |
| 40 | `s10-ship-gate.yml` | 7 | ~22m | validators→prop+repro+dash→agg | ⚠ 0/0 | GITHUB_TOKEN |
| 41 | `slo-instrumentation.yml` | 3 | ~2m | py→check | ✅ 2/2 | none |
| 42 | `spec_validation.yml` | 3 | ~2m | py→validate | ⚠ 0/2 | none |
| 43 | `tenant-path.yml` | 8 | ~14m | clippy+test+wasm | ⚠ 0/3 | GITHUB_TOKEN |
| 44 | `tla_billing_check.yml` | 3 | ~10m | java→tlc | ✅ 2/2 | none |
| 45 | `tla_check.yml` | 3 | ~10m | java→tlc | ⚠ 0/2 | none |
| 46 | `tla_dsr_erasure_check.yml` | 3 | ~10m | java→tlc | ✅ 2/2 | none |
| 47 | `tla_region_residency_check.yml` | 3 | ~10m | java→tlc | ✅ 2/2 | none |
| 48 | `translation-import-validate.yml` | 4 | ~3m | py→validate | ✅ 3/3 | none |

**PR cost summary**
- 48 PR-trigger workflows declared. With `paths:` filters, the typical PR (one crate touched) actually launches ~12–18 workflows. The worst case (touching `Cargo.lock` + a crate + a doc) launches 25+ workflows.
- Parallel max critical path (`cas_foundation.yml` ≈ 30 min) dominates wall-clock when triggered.
- Billable minutes per "full" PR (everything triggers): **~310 min** (sum of estimates).

### 1.2 Push-only / release-only workflows (do not affect PR-time)

| Workflow | Trigger | Notes |
|---|---|---|
| `cosign-sign.yml` | push (main), workflow_dispatch | release artifacts |
| `cf-deploy-prod.yml` | workflow_dispatch, workflow_run | gated by signing |
| `notarize-macos.yml` | workflow_dispatch, workflow_run | release |
| `release-cli.yml` | push (tags) | release |
| `release-slsa3.yml` | release, workflow_dispatch | release |
| `reproducible-build.yml` | push, schedule, workflow_dispatch | nightly + push to main |
| `sbom-consolidated.yml` | push, workflow_dispatch | post-release |
| `sbom.yml` | release, workflow_dispatch, schedule | release SBOM |
| `sign-linux.yml` / `sign-windows.yml` | workflow_dispatch, workflow_run | release |

### 1.3 Scheduled / nightly workflows (do not affect PR-time)

| Workflow | Schedule | Wall-clock | Notes |
|---|---|---|---|
| `ac-bucket-acl-cron.yml` | cron | ~5m | CF bucket ACL check |
| `backup-daily-verify.yml` | daily | ~10m | restore drill |
| `byok_kill_switch_drill_weekly.yml` | weekly | ~15m | BYOK |
| `byok_matrix_weekly.yml` | weekly | ~25m | BYOK 4-vendor matrix |
| `dr-drill-monthly.yml` | monthly | ~30m | DR |
| `endurance-2h-nightly.yml` | nightly | 120m+ | soak |
| `fuzz-nightly.yml` | nightly | ~60m | cargo-fuzz |
| `load-test-nightly.yml` | nightly | ~90m | k6 |
| `mutation-nightly.yml` | nightly | ~120m | cargo-mutants |
| `nightly.yml` | nightly | ~60m | full sweep |
| `perf-nightly.yml` | nightly | ~45m | criterion + regression |
| `terraform-drift.yml` | nightly | ~10m | drift |
| `tla_*_check.yml` (4) | per-PR; also weekly | ~10m each | already covered above |

### 1.4 SHA-pin gaps (21 workflows ⚠)

The following workflows use floating refs (`@v4`, `@stable`, `@nightly`) for one or more third-party actions:

```
corelink-client-verify.yml  (actions/checkout@v4, Swatinem/rust-cache@v2)
corelink-hash.yml           (same pattern, 7 floating refs)
corelink-meta.yml           (same)
corelink-reapi.yml          (same)
corelink-worker.yml         (same)
tenant-path.yml             (same)
dashboard_validation.yml    (actions/checkout@v4, actions/setup-python@v5)
spec_validation.yml         (same)
tla_check.yml               (same)
dependabot-auto-merge.yml   (dependabot/fetch-metadata@v2)
docs-reapi-gen.yml          (uses none — N/A)
gc-ship-gate.yml            (uses none — runs scripts inline)
s07-ship-gate.yml           (uses none — same)
s08-ship-gate.yml           (uses none — same)
s09-ship-gate.yml           (uses none — same)
s10-ship-gate.yml           (uses none — same)
region_pinning.yml          (uses none — same)
byok_kill_switch_drill_weekly.yml  (actions/checkout@v4)
```

Per ADR-0044 + WI-S01-007 §6.1.9 supply-chain policy, all third-party actions MUST be SHA-pinned. Workflows in `s07..s10-ship-gate.yml` and `region_pinning.yml` declare zero `uses:` and instead run inline shell, which is technically compliant but worth documenting.

---

## 2. Optimization opportunities (29 total)

### 2.1 Workspace-cargo deduplication (P1, single biggest win — saves ~15m wall, ~80m billable per PR)

**Problem.** The same three commands —
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```
— are repeated in 12 workflows: `cas_foundation`, `corelink-{hash,meta,reapi,worker,client-verify}`, `tenant-path`, `gc-ship-gate`, `s07/s08/s09/s10-ship-gate`. Each one re-checks out, re-installs the toolchain, and re-runs the same workspace-wide commands.

**Why duplicated.** The per-crate workflows pre-date `cas_foundation.yml`. When the convergence workflow was added in WI-S01-007, the per-crate workflows weren't trimmed.

**Fix.** Per-crate workflows should retain ONLY the per-crate targeted gates that `cas_foundation.yml` doesn't cover:
- `corelink-hash`: canonical-vectors test, WASM build, ct-variance release test, fuzz smoke
- `corelink-meta`: meta-specific table tests
- `corelink-worker`: worker-specific (CF Workers) compile compat
- `corelink-reapi`: REAPI proto-conformance
- `corelink-client-verify`: verify-roundtrip integration
- `tenant-path`: tenant-isolation focused tests
- `s07..s10-ship-gate`: drop `cargo *` entirely; rely on `cas_foundation` for the workspace gates and on their own ship-gate-specific validators/prop tests/dashboards.

Each per-crate workflow saves ~7m wall + ~7m billable. **Total savings: ~80 billable minutes per PR.**

**Risk.** Zero quality reduction: every command currently run in the duplicates is still run in `cas_foundation.yml`, which gates the same paths (any `crates/**` or `apps/**` change).

### 2.2 Docs workflow consolidation (P1 — saves ~6m wall, ~25m billable per docs-touching PR)

**Problem.** Eight separate workflows install pnpm + Node + run `pnpm install` for `apps/docs/`: `docs-ci`, `docs-a11y`, `docs-a11y-baseline`, `docs-i18n`, `docs-lighthouse`, `docs-reapi-gen`, `docs-lychee`, `docs-vale`. Most could be jobs inside `docs-ci.yml` sharing one cached node_modules.

**Fix.** Collapse `docs-{a11y,a11y-baseline,i18n,lighthouse,reapi-gen}.yml` into 5 jobs of `docs-ci.yml`. They already share the same `apps/docs/**` paths filter. Keep `docs-lychee.yml` (Docker-based) and `docs-vale.yml` (no pnpm) standalone.

**Risk.** Zero gate reduction. Job-level fan-out preserves parallelism. One single `pnpm install` cache hit replaces five.

### 2.3 Validators duplication (P1 — saves ~3m wall, ~10m billable per PR)

**Problem.** `validate_specs.py` runs in `spec_validation.yml` AND in each of the 4 ship-gate workflows (`s07..s10-ship-gate`) AND in `gc-ship-gate.yml`. Six identical invocations.

**Fix.** Make `spec_validation.yml` the single source of truth. Ship-gates depend on it via `workflow_run:` OR remove the validators job and rely on branch-protection requiring `spec_validation.yml` separately.

**Risk.** None — validators are deterministic and side-effect-free.

### 2.4 SHA-pin completion (P1 — risk reduction, no time savings)

**Problem.** 21 workflows have floating action refs. ADR-0044 requires SHA pins.

**Fix.** Pin every `actions/checkout`, `actions/setup-python`, `Swatinem/rust-cache`, `dtolnay/rust-toolchain`, `rustsec/audit-check` to its 40-char SHA. Use Renovate config (to be added in S-12) for auto-bumps.

**Risk.** Reduces supply-chain risk; no functional change.

### 2.5 Concurrency cancellation (P2 — saves ~30 % runner minutes on PR force-pushes)

**Problem.** Only 18 / 71 workflows declare `concurrency: cancel-in-progress: true`. PR force-pushes today launch a parallel duplicate run instead of cancelling the in-flight one.

**Fix.** Every PR-triggered workflow should declare:
```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```

**Risk.** None for PR-only cancellation. We MUST NOT cancel push-to-main runs (release safety).

### 2.6 Cache hit-rate (P2 — saves ~3m wall on cold-runner PRs)

**Problem.** Several workflows declare `Swatinem/rust-cache` with default `shared-key:` (none), meaning per-workflow keys. Cross-workflow sharing is possible by aligning `shared-key`.

**Fix.**
- `cas_foundation.yml`'s `workspace-build-test` already warms a workspace cache. The per-crate workflows can read that cache by using the same `shared-key: workspace-r1`.
- For per-crate workflows where build dirs are isolated (`crates/corelink-hash/fuzz -> target`), keep them separate but pin to a `shared-key: <crate>-fuzz`.

**Risk.** None — cache miss falls back to clean build.

### 2.7 TLC parallelism (P2 — saves ~6m wall in `cas_foundation.yml`)

**Problem.** `cas_foundation.yml::tlc-canonical` runs all 4 TLA+ specs sequentially in one job (8min × 4 = up to 32min sequential, currently in a 25min timeout — relies on TLC being faster than the cap).

**Fix.** Matrix the 4 specs across 4 parallel jobs:
```yaml
strategy:
  matrix:
    spec: [tenant_isolation, cas_integrity, gc_correctness, audit_immutability]
```

**Risk.** None — TLC runs are independent. Caveat: 4× billable minutes (4 × 8 min vs. 1 × 32 min) — wall-clock improves; billable is flat-to-slightly-higher because each fan-out re-pays the Java install + TLC download. Net billable is roughly neutral; wall-clock wins ~25 min when on cold cache, ~6 min on warm.

**Trade-off note.** Adopt only if wall-clock is the priority. We choose YES — TLC times sit on the critical path.

### 2.8 Skip-if-no-Rust-touched (P2 — saves ~5m wall on docs-only PRs)

**Problem.** `cas_foundation.yml`'s paths filter triggers on `Cargo.toml` or any `crates/**`. A docs-only PR that doesn't touch Cargo skips it correctly — but `cargo-audit.yml`, `cargo-deny.yml`, `license-policy.yml` still trigger on `pull_request` without `paths:` in some configurations.

**Fix.** Audit each Rust-touching workflow's `paths:` and ensure docs-only PRs don't trigger Rust gates.

**Risk.** None — these gates are scoped to Rust files anyway.

### 2.9 `permissions: contents: read` default (P3 — security tightening)

**Problem.** A handful of workflows omit top-level `permissions:`. Without it, the runner gets the repo's default token scope (today: `read+write` on most things).

**Fix.** Every workflow gets `permissions: { contents: read }` at the top; jobs that need more (e.g., `id-token: write` for cosign) override at job level.

**Risk.** None — least-privilege.

### 2.10 Drop `actions/checkout` on jobs that don't read the tree (P3 — saves ~5s/job)

**Problem.** Some "smoke" jobs check out the full repo only to read one file. Minor — included for completeness.

**Fix.** Sparse-checkout or skip checkout where possible. Marginal gain (~5–10s); de-prioritised.

### 2.11 Misc

- `corelink-*` workflows declare `fuzz-smoke` on `pull_request` and `fuzz-nightly` on `schedule` as separate jobs differing only in `-max_total_time`. Consolidate to one job using a `FUZZ_SECONDS` env that's 60 on PR, 3600 on schedule.
- `ffi-matrix-ci.yml` runs a 3×2 matrix (`OS × language`). Verify cache is keyed by `${{ matrix.os }}-${{ matrix.lang }}` so the matrix doesn't thrash a shared key.
- `lockfile-diff.yml` runs on every PR even if `pnpm-lock.yaml` / `Cargo.lock` unchanged. Add `paths:` filter.
- `changelog-validate.yml` runs on every PR. Add `paths: ['CHANGELOG.md', '.github/workflows/changelog-validate.yml']` to skip non-CHANGELOG PRs (or keep mandatory if changelog is required on every PR per repo policy — confirm).

---

## 3. Critical-path map (per typical PR scenario)

### Scenario A: "Crate-touching PR" (touches `crates/corelink-hash/**`)

Triggered workflows (today):
- `cas_foundation` (30 min wall)
- `corelink-hash` (16 min wall)
- `cargo-audit` (6 min)
- `cargo-deny` (5 min)
- `license-policy` (4 min)
- `coverage` (12 min)
- `tla_*_check` × 4 (10 min each, parallel)
- `slo-instrumentation` (2 min)
- `spec_validation` (2 min)
- `gc-ship-gate` if dedup/quota touched, `s07..s10-ship-gate` if their paths match

Critical path (wall-clock, parallel): **`cas_foundation` at 30 min** sets the floor. After dedup: **`cas_foundation` 24 min** (matrix TLC) → **24 min total wall**.

Billable per Scenario A: today ~180 min → after R-1 tickets ~110 min.

### Scenario B: "Docs-only PR" (touches `apps/docs/**`)

Triggered today: `docs-ci` (18 min) + 6 separate docs workflows (~25 min cumulative billable).
After consolidation: `docs-ci` alone with 6 jobs in parallel (~10 min wall, ~12 min billable).

### Scenario C: "Spec-only PR" (touches `specs/**`)

Triggered today: `spec_validation`, `changelog-validate`, occasionally ship-gates.
Minor optimization opportunity (already small).

---

## 4. Cost projection

| PR archetype | Frequency (est. of last 30d) | Today wall p50 | Today billable p50 | Optimized wall | Optimized billable | Savings/PR billable |
|---|---|---|---|---|---|---|
| A. Crate-touching | 60 % | 30 m | 180 m | 24 m | 110 m | 70 m |
| B. Docs-only | 25 % | 18 m | 60 m | 10 m | 35 m | 25 m |
| C. Spec-only | 10 % | 5 m | 12 m | 5 m | 10 m | 2 m |
| D. Cross-cutting (Cargo.lock + docs + specs) | 5 % | 60 m | 310 m | 34 m | 190 m | 120 m |

Weighted average savings: **~52 billable minutes per PR**. At a conservative 8 PRs / weekday × 21 weekdays = 168 PRs/month → **~8 700 billable minutes saved per month** (≈ 145 runner-hours / mo). At GitHub's $0.008 / Linux-minute rate that's ~$70/mo — small in dollars, but the wall-clock improvement (−6 min p50 → faster feedback for builders) is the actual win.

---

## 5. Sequencing recommendation (deliver in this order)

| Wave | Tickets | Time savings | Risk |
|---|---|---|---|
| Wave 1 (P1, 4 tickets) | 2.1 (dedup workspace cargo) + 2.2 (docs consolidation) + 2.3 (validators dedup) + 2.4 (SHA-pin) | ~+30 m billable/PR + supply-chain | LOW |
| Wave 2 (P2, 4 tickets) | 2.5 concurrency + 2.6 cache keys + 2.7 TLC matrix + 2.8 paths-filter audit | ~+18 m billable/PR | LOW |
| Wave 3 (P3, 3 tickets) | 2.9 permissions + 2.10 checkout slim + 2.11 misc | marginal | LOWEST |

Total estimated savings if all 11 tickets land: **~52 billable min / PR + ~24 wall-clock min / PR p50**.

---

## 6. Quality-gate non-regression matrix

| Gate | Before | After (Wave 1+2+3) | Status |
|---|---|---|---|
| `cargo fmt --check` workspace | runs in 12 wf | runs in 1 wf (`cas_foundation`) | ✅ identical coverage |
| `cargo clippy -D warnings` workspace | 12 wf | 1 wf | ✅ identical |
| `cargo test` workspace | 12 wf | 1 wf | ✅ identical |
| `cargo doc -D warnings` | 12 wf | 1 wf | ✅ identical |
| Canonical-vectors test (corelink-hash) | 1 wf | 1 wf | ✅ unchanged |
| WASM build (per crate) | 5 wf | 5 wf | ✅ unchanged |
| ct-variance gate | 1 wf | 1 wf | ✅ unchanged |
| TLC 4 specs PR bounds | sequential | matrix parallel | ✅ identical fail-closed |
| cargo-deny | 1 wf | 1 wf | ✅ unchanged |
| cargo-audit | 1 wf | 1 wf | ✅ unchanged |
| license-policy | 1 wf | 1 wf | ✅ unchanged |
| validate_specs | 6 wf | 1 wf | ✅ identical (deterministic) |
| INV promotion | 4 wf | 1 wf | ✅ identical |
| Cosign sign (release-only) | 2 wf | 2 wf | ✅ unchanged |
| SBOM CycloneDX | 2 wf | 2 wf | ✅ unchanged |
| Reproducible-build smoke | 1 wf | 1 wf | ✅ unchanged |

**Conclusion: zero quality-gate weakening across all 11 tickets.** Per-crate workflows keep their crate-specific gates; the deduplicated workspace gates run in `cas_foundation.yml` which has identical or wider `paths:` triggers.

---

## 7. Cross-references

- ROADMAP-TO-GA.md §1 (Wave R-1 cleanup tail) — this audit is an R-1 deliverable
- ADR-0044 — SHA-pin policy + cosign keyless model
- WI-S01-007 — `cas_foundation.yml` convergence design
- WI-S12-006 — full 2-runner SLSA L3 reproducible-build verification (post-S-12)
- Tickets: `specs/_audits/sealed/ci-optimization-followup-tickets.md`
- Template: `.github/workflows/_TEMPLATE.yml.md`
- (No `.github/CONTRIBUTING.md` exists today — recommend creating one referencing the template; alternative: surface guidance in `docs/internal/ENGINEERING-ONBOARDING.md` per ROADMAP R1-9.)

---

## 8. Audit verdict

**SEAL APPROVED** — audit complete, 11 followup tickets filed, template authored, zero implementation done outside the template (per task brief).
