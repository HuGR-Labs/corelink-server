# Current `main` drift readback — 2026-09-23 (`b9b3ee8c`)

The campaign re-read `origin/main`; the current remote tip is
`b9b3ee8cba6ad6f43f73fe785fb99acb55192019` (`fix(capacity): validate
per-deployment memory quota (#2207)`). The immutable campaign snapshot remains
`50a5ab3a0e3e90bff56beb18fade52f50e3ff005`; this readback does not rewrite
historical evidence.

There is no Cargo.toml or Cargo.lock delta from the snapshot in this comparison.
The server source changes since the snapshot remain the five documented paths:
`main.rs`, new `main_runtime.rs`, `main_tests.rs`,
`routes/dsr/adapter_d1.rs`, and new `classification.rs`. The server tree is
unchanged between `5e4339c5` and `b9b3ee8c`, so the server reanchor at
`b9b3ee8c` remains valid for the source facts it claims. No runtime,
deployment, or GitHub write is inferred.

Decision: retain the immutable campaign snapshot for registry and publication
preparation; treat `b9b3ee8c` as the current-main observation for future
re-review and do not silently reuse a stale-main approval.
