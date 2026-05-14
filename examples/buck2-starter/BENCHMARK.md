# Buck2 Remote Cache Benchmark — CoreLink Starter

> This file is auto-updated weekly by the CI benchmark job
> (`.github/workflows/buck2-starter-ci.yml` → `benchmark` job).
> First real numbers will appear after the initial CI run.

## Configuration

| Field | Value |
|---|---|
| Target | `:hello` |
| Iterations | 10 cold + 10 warm |
| Hit threshold | ≥ 80% |

## Results (pending first CI run)

| Metric | Cold (no cache) | Warm (remote cache) |
|---|---|---|
| Median | — | — |
| p95    | — | — |
| Cache hit ratio | — | — |

## Notes

- Cold runs: `buck2 clean` before each build; remote cache populated during warm phase.
- Cache hit ratio derived from Buck2 build report JSON (`cache_hits / total_actions`).
- Threshold ≥ 80% maps to WI-S15-003 AC §8 + sprint contract R-S15-8.
- Run locally: `./scripts/benchmark.sh`

## Parity vs Bazel starter (WI-S15-002)

See `docs/integrations/bazel-vs-buck2.md` for apples-to-apples comparison methodology.
