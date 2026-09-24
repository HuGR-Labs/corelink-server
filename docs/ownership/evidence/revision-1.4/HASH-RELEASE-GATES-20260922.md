# `corelink-hash` release gates — 2026-09-22

Execution mode: `LOCAL_ISOLATED`; offline Cargo, no services or production
systems.

- Checkout: campaign branch `50d9cee92`; Rust source files are byte-identical
  to `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8`; manifest URL-only drift is
  recorded separately.
- Constant-time command:
  `cargo test --locked --offline --release -p corelink-hash --test prop_hash constant_time_variance -- --exact --nocapture`
  — **PASS**, relative delta `0.019%` (medians 18,785 ns / 18,782 ns).
- Performance command:
  `cargo test --locked --offline --release -p corelink-hash --test prop_hash perf_regression_5mib_under_50ms -- --exact --nocapture`
  — **PASS**, 10 × 5 MiB total `21.792027 ms`, per-call `2.18 ms`,
  approximately `2.24 GiB/s`.

These results are current local observations and do not prove production
latency, wasm timing, runtime reachability or peer approval. They supersede
neither the historical failed performance observation nor the requirement for
fresh cold review after the maintenance/reference byte changes.
