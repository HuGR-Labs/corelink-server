# Baseline diff — wave-22 (`perf-baseline-pre-ga-2026-05-16`) → wave-29 GA-freeze (`perf-baseline-ga-2026-05-16`)

> Companion to `specs/_audits/2026-05-16-perf-baseline-ga-freeze.md` §5
> and the wave-30 recapture in
> `specs/_audits/2026-05-16-perf-benches-recapture.md`.
> Mechanical diff intended for the GA-day +24h SRE check.

## Tag transition

| Field              | wave-22                                     | wave-29 GA-freeze (+ wave-30 recapture)            |
|--------------------|---------------------------------------------|----------------------------------------------------|
| Tag name           | `perf-baseline-pre-ga-2026-05-16`           | `perf-baseline-ga-2026-05-16`                      |
| Target commit      | `bccdd97` (main, wave-22 merge)             | `365dd38` (main, post wave-28 merge); recapture rebased onto `04f2dff` (post wave-29 merge) |
| Audit doc          | `2026-05-16-perf-regression-ci-tightened.md`| `2026-05-16-perf-baseline-ga-freeze.md` + `2026-05-16-perf-benches-recapture.md` (wave-30) |
| Owner              | Gustavo Schneiter                           | Gustavo Schneiter                                  |
| Schema             | wave-22 schema (criticality + tolerance_pct)| **unchanged** (same 11-file manifest)              |
| Numeric fields     | all `null` (pending operator refresh)       | **all 11 populated** as of wave-30 stream-5 recapture (3 by wave-29, 8 by wave-30) |

## Per-bench delta

The wave-22 manifest persisted 11 schema-current files with
`median_ns / mean_ns / p99_ns / sample_count == null` and
`commit == "pending"`. Therefore the per-bench delta is
**schema-stable, numeric-net-new**: every populated row in the
wave-29 + wave-30 GA-freeze manifest is a new measurement, not a
regression against wave-22.

| #  | Crate                     | Bench                   | Class        | wave-22 p99 | wave-29/30 median (ns)            | wave-29/30 p99 proxy (ns)         | Δ (p99) | Verdict             |
|----|---------------------------|-------------------------|--------------|-------------|-----------------------------------|-----------------------------------|---------|---------------------|
| 1  | corelink-tenant-path      | derive                  | CRITICAL     | `null`      | 1 666.5 (`derive_prefix`)         | 1 682.3                           | n/a     | net-new (wave-30)   |
| 2  | corelink-tenant-path      | derive_prefix_v2        | CRITICAL     | `null`      | 1 149 611.9 (`1000-distinct-tenants`) | 1 150 861.6                   | n/a     | net-new (wave-29)   |
| 3  | corelink-hash             | blake3                  | CRITICAL     | `null`      | 263 054.9 (`1MiB`)                | 263 203.4                         | n/a     | net-new (wave-29)   |
| 4  | corelink-hash             | blake3_bench            | CRITICAL     | `null`      | 523 770.0 (`1024 KiB`)            | 529 030.0                         | n/a     | net-new (wave-30)   |
| 5  | corelink-audit-chain      | merkle_append           | CRITICAL     | `null`      | 19 694.0 (`append_single`)        | 19 717.0                          | n/a     | net-new (wave-30)   |
| 6  | corelink-audit-chain      | jcs_canonicalize        | CRITICAL     | `null`      | 26 728.1 (`10240B-data`)          | 27 152.8                          | n/a     | net-new (wave-29)   |
| 7  | corelink-dpa-acceptance   | accept_and_verify_jwt   | CRITICAL     | `null`      | 46 630.0 (`verify_receipt_rs256`) | 46 799.0                          | n/a     | net-new (wave-30)   |
| 8  | corelink-byok             | envelope_roundtrip      | NON_CRITICAL | `null`      | 17 757.0 (`wrap_unwrap_roundtrip`)| 17 780.0                          | n/a     | net-new (wave-30)   |
| 9  | corelink-signup           | orchestrator            | NON_CRITICAL | `null`      | 41 626.0 (`provision_new`)        | 41 920.0                          | n/a     | net-new (wave-30)   |
| 10 | corelink-tier-selection   | select                  | NON_CRITICAL | `null`      | 10 272.0 (`select_starter`)       | 10 766.0                          | n/a     | net-new (wave-30)   |
| 11 | corelink-stripe-real      | webhook_verify          | NON_CRITICAL | `null`      | 26 599.0 (`verify/4096B`)         | 27 152.0                          | n/a     | net-new (wave-30)   |

> The p99 column reports the **upper 95%-CI bound of the median**
> from criterion `--quick` mode (proxy). Operator §5 owns the
> canonical p99 recapture via `--measurement-time 5` on CI.

## Wave-30 stream-5 recapture delta vs wave-29 stream-9

Wave-29 stream-9 measured rows 2, 3, 6 end-to-end (`--quick` mode)
and persisted rows 1, 4, 5, 7, 8, 9, 10, 11 as `pending` (laptop
wall-clock cap burned ~50 min of the 60-min budget on cold compile
of the first three CRITICAL benches). Wave-30 stream-5 picked up
the eight pending rows with a shared `CARGO_TARGET_DIR` cache
strategy and completed all eight in a single ~29-minute run window
(see audit `2026-05-16-perf-benches-recapture.md` §3).

| #  | Crate                     | Bench                   | wave-29 status        | wave-30 status        |
|----|---------------------------|-------------------------|-----------------------|-----------------------|
| 1  | corelink-tenant-path      | derive                  | pending-recapture     | **measured (quick)**  |
| 2  | corelink-tenant-path      | derive_prefix_v2        | **measured (quick)**  | unchanged             |
| 3  | corelink-hash             | blake3                  | **measured (quick)**  | unchanged             |
| 4  | corelink-hash             | blake3_bench            | pending-recapture     | **measured (quick)**  |
| 5  | corelink-audit-chain      | merkle_append           | pending-recapture     | **measured (quick)**  |
| 6  | corelink-audit-chain      | jcs_canonicalize        | **measured (quick)**  | unchanged             |
| 7  | corelink-dpa-acceptance   | accept_and_verify_jwt   | pending-recapture     | **measured (quick)**  |
| 8  | corelink-byok             | envelope_roundtrip      | pending-recapture     | **measured (quick)**  |
| 9  | corelink-signup           | orchestrator            | pending-recapture     | **measured (quick)**  |
| 10 | corelink-tier-selection   | select                  | pending-recapture     | **measured (quick)**  |
| 11 | corelink-stripe-real      | webhook_verify          | pending-recapture     | **measured (quick)**  |

## Verdict

- **Regression vs wave-22:** none possible — wave-22 carried no
  numeric measurements to regress against. The wave-22 → wave-29/30
  transition is a **first numeric capture** of the wave-22 schema.
- **GA-day +24h comparison point:** the wave-29+30 manifest
  (commits `365dd38` for rows 2/3/6 and `04f2dff` for rows
  1/4/5/7/8/9/10/11; same tag `perf-baseline-ga-2026-05-16`).
- **Sanity check vs class budgets (CRITICAL ≤ 5 % p99 / NON_CRIT
  ≤ 15 % p99):** every p99-proxy reading sits well below the
  per-class budget headroom relative to its own median — the
  `--quick`-mode CI band is tight (≤2% on the CRITICAL hot paths
  observed here). No anomalous outliers indicating a hot-path
  regression in either CRITICAL or NON_CRITICAL classes.
- **Gate thresholds:** unchanged from wave-22 (5% CRITICAL / 15%
  NON_CRITICAL p99; per-bench `tolerance_pct` override wins).

## Cross-references

- `specs/_audits/2026-05-16-perf-baseline-ga-freeze.md` (wave-29 stream-9)
- `specs/_audits/2026-05-16-perf-benches-recapture.md` (wave-30 stream-5)
- `specs/_audits/2026-05-16-perf-regression-ci-tightened.md` (wave-22)
- `reports/perf/README.md`
- `reports/perf/baseline-ga-2026-05-16-365dd38.json` (umbrella manifest)
- `.github/workflows/perf-regression.yml` (header comment names the new tag)
