# reports/perf — Committed perf-regression baseline manifest

This directory holds the **committed** criterion baseline used by the
`perf-regression` CI gate. One JSON file per tracked `(crate, bench)`
pair. `perf-regression-check.py` compares each PR run's criterion
output against these files and fails the workflow on >10% p99
regression by default.

## Files

- `baseline-<crate>-<bench>.json` — one per tracked bench
- `README.md` — this file

## Schema (per file)

```json
{
  "bench_id":   "<criterion-relative-bench-id>",
  "crate":      "<cargo-crate-name>",
  "bench":      "<bench-target>",
  "captured_at":"YYYY-MM-DDTHH:MM:SSZ",
  "commit":     "<short-git-sha or 'pending'>",
  "median_ns":  123.45,
  "mean_ns":    130.10,
  "p99_ns":     180.00,
  "sample_count": 100,
  "notes":      "free-form"
}
```

`median_ns`, `mean_ns`, `p99_ns`, `sample_count` may be `null` to mark
a **pending** baseline (no measurement recorded yet). The regression
check tolerates pending baselines as "record one" — it never fails on
them.

## Tracked benches (initial set, locked-in by `scripts/refresh-perf-baseline.sh`)

| Crate                        | Bench target              | Hot path                                  |
|------------------------------|---------------------------|-------------------------------------------|
| `corelink-tenant-path`       | `derive`                  | tenant prefix derivation (every request)  |
| `corelink-tenant-path`       | `derive_prefix_v2`        | optimized derivation                      |
| `corelink-hash`              | `blake3_bench`            | content addressing (CAS write)            |
| `corelink-hash`              | `blake3`                  | content addressing (CAS read)             |
| `corelink-byok`              | `envelope_roundtrip`      | BYOK envelope encrypt/decrypt             |
| `corelink-audit-chain`       | `merkle_append`           | audit Merkle append                       |
| `corelink-audit-chain`       | `jcs_canonicalize`        | audit JSON canonicalization               |
| `corelink-signup`            | `orchestrator`            | signup flow orchestration                 |
| `corelink-tier-selection`    | `select`                  | tier selection (auth middleware-adjacent) |
| `corelink-dpa-acceptance`    | `accept_and_verify_jwt`   | DPA accept + JWT verify                   |
| `corelink-stripe-real`       | `webhook_verify`          | Stripe webhook verification               |

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
