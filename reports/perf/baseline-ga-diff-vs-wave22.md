# Baseline diff — wave-22 (`perf-baseline-pre-ga-2026-05-16`) → wave-29 GA-freeze (`perf-baseline-ga-2026-05-16`)

> Companion to `specs/_audits/2026-05-16-perf-baseline-ga-freeze.md` §5.
> Mechanical diff intended for the GA-day +24h SRE check.

## Tag transition

| Field              | wave-22                                     | wave-29 GA-freeze                                  |
|--------------------|---------------------------------------------|----------------------------------------------------|
| Tag name           | `perf-baseline-pre-ga-2026-05-16`           | `perf-baseline-ga-2026-05-16`                      |
| Target commit      | `bccdd97` (main, wave-22 merge)             | `365dd38` (main, post wave-28 merge)               |
| Audit doc          | `2026-05-16-perf-regression-ci-tightened.md`| `2026-05-16-perf-baseline-ga-freeze.md`            |
| Owner              | Gustavo Schneiter                           | Gustavo Schneiter                                  |
| Schema             | wave-22 schema (criticality + tolerance_pct)| **unchanged** (same 11-file manifest)              |
| Numeric fields     | all `null` (pending operator refresh)       | populated where stream wall-clock allowed; §5 holds the rest |

## Per-bench delta

The wave-22 manifest persisted 11 schema-current files with
`median_ns / mean_ns / p99_ns / sample_count == null` and
`commit == "pending"`. Therefore the per-bench delta is
**schema-stable, numeric-net-new**: every populated row in the
wave-29 GA-freeze manifest is a new measurement, not a regression
against wave-22.

| #  | Crate                     | Bench                   | Class        | wave-22 p99 | wave-29 p99 (GA-freeze)         | Δ (p99) | Verdict             |
|----|---------------------------|-------------------------|--------------|-------------|---------------------------------|---------|---------------------|
| 1  | corelink-tenant-path      | derive                  | CRITICAL     | `null`      | pending                         | n/a     | pending-recapture   |
| 2  | corelink-tenant-path      | derive_prefix_v2        | CRITICAL     | `null`      | 1 150 861.6 ns (quick, proxy)   | n/a     | net-new (measured)  |
| 3  | corelink-hash             | blake3                  | CRITICAL     | `null`      | 263 203.4 ns (quick, proxy)     | n/a     | net-new (measured)  |
| 4  | corelink-hash             | blake3_bench            | CRITICAL     | `null`      | pending                         | n/a     | pending-recapture   |
| 5  | corelink-audit-chain      | merkle_append           | CRITICAL     | `null`      | pending                         | n/a     | pending-recapture   |
| 6  | corelink-audit-chain      | jcs_canonicalize        | CRITICAL     | `null`      | 27 152.8 ns (quick, proxy)      | n/a     | net-new (measured)  |
| 7  | corelink-dpa-acceptance   | accept_and_verify_jwt   | CRITICAL     | `null`      | pending                         | n/a     | pending-recapture   |
| 8  | corelink-byok             | envelope_roundtrip      | NON_CRITICAL | `null`      | pending                         | n/a     | pending-recapture   |
| 9  | corelink-signup           | orchestrator            | NON_CRITICAL | `null`      | pending                         | n/a     | pending-recapture   |
| 10 | corelink-tier-selection   | select                  | NON_CRITICAL | `null`      | pending                         | n/a     | pending-recapture   |
| 11 | corelink-stripe-real      | webhook_verify          | NON_CRITICAL | `null`      | pending                         | n/a     | pending-recapture   |

> The p99 column for wave-29 reports the upper 95%-CI bound of the
> median from `--quick` mode (proxy). Operator §5 owns the
> canonical p99 recapture via `--measurement-time 5`.

## Verdict

- **Regression vs wave-22:** none possible — wave-22 carried no
  numeric measurements to regress against. The wave-22 → wave-29
  transition is a **first numeric capture** of the wave-22 schema.
- **GA-day +24h comparison point:** the wave-29 manifest
  (commit `365dd38`, tag `perf-baseline-ga-2026-05-16`).
- **Gate thresholds:** unchanged from wave-22 (5% CRITICAL / 15%
  NON_CRITICAL p99; per-bench `tolerance_pct` override wins).

## Cross-references

- `specs/_audits/2026-05-16-perf-baseline-ga-freeze.md`
- `specs/_audits/2026-05-16-perf-regression-ci-tightened.md`
- `reports/perf/README.md`
- `.github/workflows/perf-regression.yml` (header comment updated to
  name the new tag)
