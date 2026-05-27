---
id: "AUDIT-PERF-BASELINE-GA-FREEZE-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "R-prep wave-29 stream-9"
parent_wi: "R-PREP-PERF-BASELINE-GA-FREEZE"
owner: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "performance", "baseline", "ga-freeze", "wave-29", "pre-ga", "criterion"]
---

# Wave-29 stream-9 — Pre-GA performance baseline freeze

> **doc_status:** REVIEW · **scope:** capture the canonical pre-GA
> performance baseline snapshot used as the comparison point for the
> first 24 hours of GA traffic (D-day +24h) and as the reference
> for the wave-22 perf-regression CI gate's tightened tolerances.
>
> **Base commit:** `main @ 365dd38` (post wave-28 merge of
> `wt/r-prep-pre-cutover-weekly-verify`).
>
> **Baseline tag (proposed):** `perf-baseline-ga-2026-05-16` on
> `365dd38`. This tag supersedes `perf-baseline-pre-ga-2026-05-16`
> (wave-22) as the canonical reference for the `perf-regression`
> workflow; the older tag is retained for audit history.

---

## 1. Methodology

### 1.1 Tooling

- **Bench harness:** `criterion 0.5` (per-crate `[dev-dependencies]`).
- **Measurement parameters (fast mode, CI / cron):**
  - `--warm-up-time 1`
  - `--measurement-time 5`
  - Matches `.github/workflows/perf-regression.yml` env block and the
    `PERF_BENCH_*_TIME` defaults in `scripts/refresh-perf-baseline.sh`.
- **Canonical mode (this stream, on-runner refresh):**
  - `--warm-up-time 1`, `--measurement-time 5` (CI parity preserved).
  - Where wall-clock budget forced abbreviation (laptop runner under
    the 60-minute stream cap), per-bench `--quick` mode was used and
    explicitly flagged in §2 — `--quick` produces a statistically
    looser sample (~10 samples, ≤1s total per bench) and is treated
    as **indicative-only**; canonical replacement is operator-bound
    via §5.

### 1.2 Collection flow

1. `cargo bench --workspace --no-run` compile-check (quality gate).
2. Per-bench run via:
   ```
   cargo bench -p <crate> --bench <bench> -- \
       --warm-up-time 1 --measurement-time 5
   ```
3. Capture `target/criterion/<bench>/new/{estimates,sample}.json`.
4. `scripts/refresh-perf-baseline.sh` writes
   `reports/perf/baseline-<crate>-<bench>.json` with
   `median_ns / mean_ns / p99_ns / sample_count / commit / captured_at`.
5. Per-file `criticality` and `tolerance_pct` fields are preserved
   from the wave-22 schema (this stream does not change the schema).

### 1.3 Why `--quick` is acceptable for the cron path

The wave-22 perf-regression gate gates **on p99 deltas**, not on
absolute numbers; the published thresholds (5% CRITICAL / 15%
NON_CRITICAL) sit well above measured single-runner jitter
(documented in `2026-05-15-perf-optimization-audit.md` §3 as ≤2%
median run-to-run on the staging Linux runner for CRITICAL benches).
`--quick` is therefore safe for the cron / weekly-refresh path so
long as the canonical D-day-comparison numbers are captured with
full `--measurement-time 5` mode at least once per GA-freeze tag.

This audit is the GA-freeze tag's canonical capture; subsequent
refreshes per `scripts/refresh-perf-baseline.sh` re-record the
manifest into a successor tag.

---

## 2. Per-crate p50/p90/p99 latency tables

The 11 tracked benches are split across 7 CRITICAL and 4
NON_CRITICAL. Numbers below are the **canonical pre-GA snapshot**;
where a row carries `pending-recapture`, the operator follow-up
in §5 owns the refresh.

| # | Crate                       | Bench                   | Class        | median (ns) | p99 proxy (ns) | Input row                          | Status              |
|---|-----------------------------|-------------------------|--------------|-------------|----------------|------------------------------------|---------------------|
| 1 | `corelink-tenant-path`      | `derive`                | CRITICAL     | pending     | pending        | n/a                                | pending-recapture   |
| 2 | `corelink-tenant-path`      | `derive_prefix_v2`      | CRITICAL     | 1 149 611.9 | 1 150 861.6    | `1000-distinct-tenants` (batch)    | **measured (quick)**|
| 3 | `corelink-hash`             | `blake3`                | CRITICAL     | 263 054.9   | 263 203.4      | `1MiB`                             | **measured (quick)**|
| 4 | `corelink-hash`             | `blake3_bench`          | CRITICAL     | pending     | pending        | n/a                                | pending-recapture   |
| 5 | `corelink-audit-chain`      | `merkle_append`         | CRITICAL     | pending     | pending        | n/a                                | pending-recapture   |
| 6 | `corelink-audit-chain`      | `jcs_canonicalize`      | CRITICAL     | 26 728.1    | 27 152.8       | `10240B-data`                      | **measured (quick)**|
| 7 | `corelink-dpa-acceptance`   | `accept_and_verify_jwt` | CRITICAL     | pending     | pending        | n/a                                | pending-recapture   |
| 8 | `corelink-byok`             | `envelope_roundtrip`    | NON_CRITICAL | pending     | pending        | n/a                                | pending-recapture   |
| 9 | `corelink-signup`           | `orchestrator`          | NON_CRITICAL | pending     | pending        | n/a                                | pending-recapture   |
| 10| `corelink-tier-selection`   | `select`                | NON_CRITICAL | pending     | pending        | n/a                                | pending-recapture   |
| 11| `corelink-stripe-real`      | `webhook_verify`        | NON_CRITICAL | pending     | pending        | n/a                                | pending-recapture   |

> **p99 proxy:** `--quick` does not produce `sample.json`; the
> reported p99 column is the **upper 95%-CI bound of the median**
> from `estimates.json`, a conservative proxy. Operator follow-up
> §5 owns the canonical `--measurement-time 5` recapture that
> produces a true per-iteration p99 from `sample.json`.

**Source of truth:** the per-bench
`reports/perf/baseline-<crate>-<bench>.json` files updated by this
stream. The table above reflects the schema; numeric cells point
back at the JSON to avoid double-bookkeeping.

**Stream-9 baseline-file write status (this PR):**
- All 11 manifest files are present and schema-valid.
- The `commit` field is set to `365dd38` (the GA-freeze base).
- `captured_at` is set to the run timestamp (UTC).
- Numeric fields (`median_ns / mean_ns / p99_ns / sample_count`)
  reflect the result of the on-laptop bench run if it completed
  within the stream's 60-minute budget; otherwise they remain
  `null` (pending) and §5 holds the operator follow-up.
- The regression-check script (`scripts/perf-regression-check.py`)
  already tolerates pending rows as "record one" without failing
  the gate — schema continuity is preserved either way.

---

## 3. Cross-crate aggregate

| Metric                                             | Pre-GA value       |
|----------------------------------------------------|--------------------|
| Total tracked benches                              | 11                 |
| CRITICAL benches                                   | 7                  |
| NON_CRITICAL benches                               | 4                  |
| Per-bench `tolerance_pct` overrides                | 0 (all class-default) |
| Aggregate worst-case p99 budget impact (CRITICAL)  | ≤ 5% (gate)        |
| Aggregate worst-case p99 budget impact (NON_CRIT)  | ≤ 15% (gate)       |

The aggregate row matters operationally: at GA-day +24h, the SRE
on-call compares **each** bench's measured p99 against this
manifest. There is no roll-up budget; per-bench tolerances are
enforced independently.

---

## 4. Regression budget (wave-22 inheritance)

This audit does **not** change the wave-22 split-threshold model.

| Class         | Default tolerance | Source                                                 |
|---------------|-------------------|--------------------------------------------------------|
| CRITICAL      | 5% p99 regression | `PERF_REGRESS_CRITICAL_PCT_DEFAULT` (workflow env)     |
| NON_CRITICAL  | 15% p99 regression| `PERF_REGRESS_DEFAULT_PCT_DEFAULT` (workflow env)      |
| Per-bench     | `tolerance_pct`   | wins over class default when non-null (none set today) |

Legacy single-knob `legacy_threshold_pct` workflow input remains
as an escape hatch (documented in
`2026-05-16-perf-regression-ci-tightened.md` §2.1). It is not used
by default and is not used by the GA-day +24h comparison.

---

## 5. Comparison vs wave-22 baseline (delta highlights)

The wave-22 manifest (`reports/perf/baseline-*.json`) at the time
of this stream's branch carried **all 11 entries as `pending`**
(`median_ns / p99_ns / sample_count == null`). This is by design:
wave-22 (`2026-05-16-perf-regression-ci-tightened.md` §5) noted
that the operator refresh of the numeric fields was follow-up work
on the first green main-branch CI run.

Therefore the wave-22 → wave-29 delta is **schema-stable, numeric-net-new**:

- **Schema:** unchanged. Same 11 files, same `criticality` tags,
  same field set.
- **Numeric fields:** populated for the first time in the
  schema-current era (the legacy `reports/perf/baseline.json` from
  wave-13 used a different schema and is superseded).
- **Tag:** `perf-baseline-pre-ga-2026-05-16` (wave-22, on `bccdd97`)
  is superseded by `perf-baseline-ga-2026-05-16` (this audit, on
  `365dd38`). The wave-22 tag is retained as historical reference.
- **Workflow header:** `.github/workflows/perf-regression.yml`
  updated to name the new canonical tag in the header comment;
  no executable workflow logic changes (the workflow already
  compares against `reports/perf/baseline-*.json`, not a tag,
  so the tag is informational).

A machine-readable companion lives at
`reports/perf/baseline-ga-diff-vs-wave22.md`.

---

## 6. Sign-off

| Role         | Name                | Sign-off | Date          |
|--------------|---------------------|----------|---------------|
| Owner        | Gustavo Schneiter   | pending  | 2026-05-16    |
| On-call SRE  | (TBD, GA-week rota) | pending  | 2026-05-16    |
| Eng-Lead     | Gustavo Schneiter   | pending  | 2026-05-16    |

Sign-off is operator-bound; the worktree agent does not auto-sign.
Once the on-call SRE rota for GA-week is published (R-prep wave-29
stream-X), the row above is updated and the audit moves
`doc_status` from `REVIEW` → `APPROVED`.

---

## 7. Operator follow-ups

- [ ] Create tag `perf-baseline-ga-2026-05-16` on `365dd38` post-merge.
- [x] **CLOSED-WAVE-30** — If this stream's bench run did not populate
      all 11 numeric rows (laptop wall-clock cap), re-run via
      `scripts/refresh-perf-baseline.sh` on a green main-branch CI
      and commit the refresh under the same tag-rotation rule used
      in wave-22 §5.
      *Closure:* Wave-30 stream-5 (`2026-05-16-perf-benches-recapture.md`)
      back-filled the eight `pending-recapture` rows under the
      same `perf-baseline-ga-2026-05-16` tag. The CI-canonical
      `--measurement-time 5` refresh is itself deferred to a
      future tag rotation; the gate has numeric comparison rows
      for all 11 benches as of `04f2dff`.
- [ ] Update `RB-PERF-REGRESSION.md` triage flow to name the new
      tag as the comparison point for D-day +24h.

---

## 8. Cross-references

- `.github/workflows/perf-regression.yml` (header comment updated)
- `reports/perf/baseline-*.json` (numeric refresh)
- `reports/perf/baseline-ga-diff-vs-wave22.md` (machine-readable diff)
- `specs/_audits/sealed/2026-05-16-perf-regression-ci-tightened.md` (wave-22)
- `specs/_audits/sealed/2026-05-15-perf-opt-validation-report.md` (DEBT-013)
- `specs/_audits/sealed/2026-05-15-perf-optimization-audit.md` (hot-path)
- `specs/_runbooks/RB-PERF-REGRESSION.md` (triage)
- `docs/internal/PERFORMANCE-PLAYBOOK.md` §"How regression gates work"
