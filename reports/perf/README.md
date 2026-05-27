# reports/perf — Committed perf-regression baseline manifest

This directory holds the **committed** criterion baseline used by the
`perf-regression` CI gate. One JSON file per tracked `(crate, bench)`
pair. `perf-regression-check.py` compares each PR run's criterion
output against these files and fails the workflow on a per-bench
tolerance:

- **CRITICAL** benches gate at **>5% p99 regression** (wave-22 tightening).
- **NON_CRITICAL** benches gate at **>15% p99 regression**.
- A per-baseline `tolerance_pct` override wins over the class default.

The canonical pre-GA snapshot is tagged `perf-baseline-ga-2026-05-16`
(wave-29 stream-9, on `main @ 365dd38`); it supersedes the wave-22
`perf-baseline-pre-ga-2026-05-16` tag. See
`specs/_audits/sealed/2026-05-16-perf-baseline-ga-freeze.md`.

## Files

- `baseline-<crate>-<bench>.json` — one per tracked bench
- `README.md` — this file

## Schema (per file)

```json
{
  "bench_id":     "<criterion-relative-bench-id>",
  "crate":        "<cargo-crate-name>",
  "bench":        "<bench-target>",
  "captured_at":  "YYYY-MM-DDTHH:MM:SSZ",
  "commit":       "<short-git-sha or 'pending'>",
  "median_ns":    123.45,
  "mean_ns":      130.10,
  "p99_ns":       180.00,
  "sample_count": 100,
  "criticality":  "CRITICAL|NON_CRITICAL",
  "tolerance_pct": null,
  "notes":        "free-form"
}
```

`median_ns`, `mean_ns`, `p99_ns`, `sample_count` may be `null` to mark
a **pending** baseline (no measurement recorded yet). The regression
check tolerates pending baselines as "record one" — it never fails on
them.

## Tracked benches (wave-22 split-tolerance table)

| Crate                        | Bench target              | Class         | Tolerance | Hot path                                  |
|------------------------------|---------------------------|---------------|-----------|-------------------------------------------|
| `corelink-tenant-path`       | `derive`                  | CRITICAL      | 5%        | tenant prefix derivation (every request)  |
| `corelink-tenant-path`       | `derive_prefix_v2`        | CRITICAL      | 5%        | optimized derivation                      |
| `corelink-hash`              | `blake3_bench`            | CRITICAL      | 5%        | content addressing (CAS write)            |
| `corelink-hash`              | `blake3`                  | CRITICAL      | 5%        | content addressing (CAS read / chunk)     |
| `corelink-audit-chain`       | `merkle_append`           | CRITICAL      | 5%        | audit-chain append                        |
| `corelink-audit-chain`       | `jcs_canonicalize`        | CRITICAL      | 5%        | audit JSON canonicalization               |
| `corelink-dpa-acceptance`    | `accept_and_verify_jwt`   | CRITICAL      | 5%        | DPA accept + JWT verify (clerk path)      |
| `corelink-byok`              | `envelope_roundtrip`      | NON_CRITICAL  | 15%       | BYOK envelope encrypt/decrypt             |
| `corelink-signup`            | `orchestrator`            | NON_CRITICAL  | 15%       | signup flow orchestration                 |
| `corelink-tier-selection`    | `select`                  | NON_CRITICAL  | 15%       | tier selection (auth middleware-adjacent) |
| `corelink-stripe-real`       | `webhook_verify`          | NON_CRITICAL  | 15%       | Stripe webhook verification               |

## How to update

```bash
# Refresh baselines locally (operator workflow)
scripts/refresh-perf-baseline.sh

# Inspect deltas
git diff reports/perf/

# Commit with rationale
git add reports/perf
git commit -m "chore(perf): refresh baselines (rationale: WI-XXX, ADR-YYY)"
```

See `specs/_runbooks/RB-PERF-REGRESSION.md` for triage flow when CI
fails, and `docs/internal/PERFORMANCE-PLAYBOOK.md` §"How regression
gates work" for the underlying methodology.
