---
id: "AUDIT-PERF-REGRESSION-CI-TIGHTENED-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "R-prep wave-22"
parent_wi: "R-PREP-PERF-REGRESSION-CI-TIGHTEN"
owner: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "performance", "ci", "regression-gate", "wave-22", "pre-ga", "criterion"]
---

# Wave-22 — Perf-regression CI gate tightened for pre-GA pilot workloads

> **doc_status:** REVIEW · **scope:** lift the regression-detection
> sensitivity of `.github/workflows/perf-regression.yml` (DEBT-013
> closure deliverable from wave-13) to match the pre-GA pilot tenant
> workload profile. Split the single 10% threshold into a CRITICAL /
> NON_CRITICAL pair (5% / 15%) keyed off per-bench `criticality`
> tags, with a per-bench `tolerance_pct` override. Capture
> flame-graph artefacts on regression (best-effort).
>
> **Baseline reference (tag, proposed):** `perf-baseline-pre-ga-2026-05-16`
> pointing at the canonical perf snapshot at `bccdd97` (main).

---

## 1. Motivation

DEBT-013 (closed PARTIAL 6/10 in wave-14; tracked in
`2026-05-15-perf-opt-validation-report.md`) shipped the perf-regression
gate with a uniform 10% p99 threshold. That threshold was deliberately
loose: it protected against gross regressions while DEBT-013
optimization batches were still landing and bench p99 noise was being
characterised.

Pre-GA pilot tenant workloads now demand a tighter floor. Wave-13/14
profiling (audit `2026-05-15-perf-optimization-audit.md` §3) showed:

- `tenant-path::derive_prefix` is on every request hot-path; a 5%
  p99 regression there compounds across the full request budget
  and is observable at the pilot SLO (p99 ≤ 350 ms for cached reads).
- BLAKE3 chunk-hash (`corelink-hash::blake3*`) is on every CAS
  read/write; the GA freeze profile in the perf playbook treats
  it as a "do-not-regress" path.
- JCS canonicalization + Merkle append (`corelink-audit-chain`) are
  serialised under the audit-chain critical section; the SLO budget
  for `audit-chain append` (DR-16) is unforgiving.
- JWT verify under DPA accept (`corelink-dpa-acceptance`) is the
  closest existing bench to the user-spec "clerk JWT verify" hot
  path; clerk-side `principal_id_clone` (corelink-clerk) is
  micro-scale (Arc clone) and lives outside the gate by design.

Non-critical benches (BYOK envelope roundtrip, signup orchestrator,
tier selection, Stripe webhook verify) sit off the per-request hot
path and tolerate higher absolute drift; a 15% gate is sufficient
to catch real regressions without becoming a noise channel.

---

## 2. Deliverables

### 2.1 Workflow tightening

File: `.github/workflows/perf-regression.yml`

- `workflow_dispatch` inputs split from a single `threshold_pct`
  into `critical_threshold_pct` (default `5`), `default_threshold_pct`
  (default `15`), and an optional `legacy_threshold_pct` escape hatch.
- Env defaults `PERF_REGRESS_CRITICAL_PCT_DEFAULT=5` and
  `PERF_REGRESS_DEFAULT_PCT_DEFAULT=15` declared at workflow scope.
- The "Perf-regression check" step now calls the script with the new
  flags; legacy mode is only activated when the operator passes a
  non-empty `legacy_threshold_pct`.
- New best-effort "Capture flame-graph on regression" step runs
  `cargo flamegraph` for `tenant-path::derive_prefix_v2` on failure
  (skipped with a notice if `cargo-flamegraph` is not present on
  the runner; never fails the workflow).
- New best-effort "Upload flame-graph artifact" step uploads the
  resulting SVG under `perf-flames-${run_id}` with `if-no-files-found:
  ignore` and 14-day retention.

### 2.2 Regression-check script

File: `scripts/perf-regression-check.py`

- New CLI flags `--critical-threshold-pct` (default 5),
  `--default-threshold-pct` (default 15). The pre-existing
  `--threshold-pct` is retained as a back-compat single-knob
  override (when set, it short-circuits both class thresholds
  and **also** suppresses per-bench `tolerance_pct` overrides).
- Per-bench resolution order (split mode):
  `baseline.tolerance_pct` (if non-null) → class default
  (CRITICAL=5% / NON_CRITICAL=15%).
- Unknown `criticality` values are warned and treated as
  `NON_CRITICAL` (fail-open against schema drift).
- Output column for `class` added; JSON report now records
  `criticality`, `effective_threshold_pct`, and the
  `threshold_mode` description.
- Smoke tested locally with synthetic baselines:
  CRITICAL +20% → fails at 5%; NON_CRITICAL +20% → fails at 15%;
  per-bench `tolerance_pct: 2.0` → fails at 2% in split mode;
  legacy `--threshold-pct 25` → passes the +20% NON_CRITICAL even
  with `tolerance_pct: 2.0` (as documented).

### 2.3 Baseline manifest

Directory: `reports/perf/`

- All 11 baseline JSON files updated with `criticality` and
  `tolerance_pct: null`.
- README updated with the new schema, the wave-22 tolerance table,
  and the canonical tag reference.

### 2.4 Baseline tag (proposed)

- Git tag `perf-baseline-pre-ga-2026-05-16` to point at this
  worktree's merge commit on `main` (post-merge of
  `wt/r-prep-perf-regression-ci-tighten`). The tag freezes the
  schema-current pre-GA snapshot; subsequent baseline refreshes
  via `scripts/refresh-perf-baseline.sh` produce successor tags
  with the same date-stamped naming (`perf-baseline-<phase>-YYYY-MM-DD`).
- Tag creation is operator-bound (the worktree agent does not push
  refs); ledger entry under §5.

---

## 3. Tolerance table

| Crate                        | Bench                    | Class         | Tolerance | Rationale                                       |
|------------------------------|--------------------------|---------------|-----------|-------------------------------------------------|
| `corelink-tenant-path`       | `derive`                 | CRITICAL      | 5%        | per-request prefix derivation                    |
| `corelink-tenant-path`       | `derive_prefix_v2`       | CRITICAL      | 5%        | optimized derivation (OPT-01 cache path)         |
| `corelink-hash`              | `blake3`                 | CRITICAL      | 5%        | CAS read / chunk-hash hot path                   |
| `corelink-hash`              | `blake3_bench`           | CRITICAL      | 5%        | CAS write hot path                               |
| `corelink-audit-chain`       | `merkle_append`          | CRITICAL      | 5%        | audit-chain append (DR-16 SLO bound)             |
| `corelink-audit-chain`       | `jcs_canonicalize`       | CRITICAL      | 5%        | audit canonicalization (DR-16 SLO bound)         |
| `corelink-dpa-acceptance`    | `accept_and_verify_jwt`  | CRITICAL      | 5%        | DPA accept + JWT verify (clerk-adjacent)         |
| `corelink-byok`              | `envelope_roundtrip`     | NON_CRITICAL  | 15%       | BYOK envelope (off the per-request hot path)     |
| `corelink-signup`            | `orchestrator`           | NON_CRITICAL  | 15%       | signup flow (called once per tenant lifecycle)   |
| `corelink-tier-selection`    | `select`                 | NON_CRITICAL  | 15%       | tier selection (cached; amortised per request)   |
| `corelink-stripe-real`       | `webhook_verify`         | NON_CRITICAL  | 15%       | Stripe webhook (out-of-band, low-volume)         |

Counts: **7 CRITICAL · 4 NON_CRITICAL · 11 total**.

---

## 4. Quality gates

- `cargo bench --workspace --no-run` — compile-check all benches.
- `cargo bench -p corelink-tenant-path --bench derive_prefix_v2 -- --quick`
  — verify at least one CRITICAL bench executes end-to-end.
- `actionlint .github/workflows/perf-regression.yml` — clean.
- `python3 scripts/validate_specs.py` + `validate_references.py` — green.
- Synthetic regression smoke test on the updated Python script
  (see §2.2): CRITICAL fail, NON_CRITICAL fail, per-bench override,
  legacy back-compat — all behaviours verified.

---

## 5. Operator follow-ups (out-of-band)

- [ ] Tag `perf-baseline-pre-ga-2026-05-16` on the merge commit of
      `wt/r-prep-perf-regression-ci-tighten` into `main`.
- [ ] On the next green main-branch CI run that records non-`null`
      `median_ns` / `p99_ns` values across all 11 baselines, refresh
      via `scripts/refresh-perf-baseline.sh` and re-tag as
      `perf-baseline-pre-ga-<date>` (the tag name is canonical;
      subsequent tags supersede).
- [ ] If `cargo-flamegraph` is desirable on every regression, gate
      install on a separate setup step and promote the flame-graph
      capture from `continue-on-error: true` to a required step.
      Tracking note: currently best-effort to keep the perf gate
      shippable on stock runners.

---

## 6. Non-goals

- This audit does **not** change baseline measurements; the existing
  `pending` baselines remain pending and are tolerated by the
  regression check until the operator refresh in §5 lands.
- It does **not** add new benches to the gate. The 11-bench manifest
  is unchanged from DEBT-013 closure.
- It does **not** alter the criterion measurement parameters
  (`PERF_BENCH_WARMUP_TIME=1`, `PERF_BENCH_MEASUREMENT_TIME=5`).
  Changing those requires an ADR per workflow header.

---

## 7. Cross-references

- `.github/workflows/perf-regression.yml` (this PR)
- `scripts/perf-regression-check.py` (this PR)
- `reports/perf/README.md` + `reports/perf/baseline-*.json` (this PR)
- `specs/_audits/sealed/perf-optimization-followup-tickets.md` (DEBT-013 scope)
- `specs/_audits/sealed/2026-05-15-perf-opt-validation-report.md` (DEBT-013 OPT-06 closure)
- `specs/_audits/sealed/2026-05-15-perf-optimization-audit.md` (hot-path analysis source)
- `specs/_audits/sealed/2026-05-15-ga-readiness-consolidation-wave-13-17.md` (wave ledger)
- `specs/_runbooks/RB-PERF-REGRESSION.md` (triage flow)
- `docs/internal/PERFORMANCE-PLAYBOOK.md` §"How regression gates work"
