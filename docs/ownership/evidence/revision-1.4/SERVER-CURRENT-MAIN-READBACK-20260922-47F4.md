# `corelink-server` current-main readback — 2026-09-22 (`47f4`)

Read-only comparison of `origin/main@47f4db7f32bba5ee346e6157203cd480a3abe78b`
against the prior server pin `96fd67793704c597c697d24b0baf96731da102ef`.

- `crates/corelink-container/src/**/*.rs`: 399 files at the current pin.
- `crates/corelink-container/tests/**/*.rs`: 14 files; the manifest still
  declares 13 integration-test targets plus the library and two binaries.
- The delta touching the server pilot is limited to Cargo storage-cap
  forwarding: `routes/cargo/part-00.rs`, `part-00-01.rs` and
  `tests-00-00.rs`.
- `cargo_gate` parses the Worker-authenticated
  `STORAGE_QUOTA_HEADER`, scopes `CARGO_STORAGE_QUOTA_CAP` around the
  downstream adapter, and `CargoMoatStore::put` reuses that scoped cap. Direct
  calls retain the resolver fallback; absent/invalid forwarded values remain
  fail-closed.
- The added tests cover resolver bypass and task-local propagation, but were
  read from source and not executed by this campaign.

This readback supersedes the prior server current-main evidence for source
claims. Runtime, deployment, Worker header stripping and D1/R2 reachability
remain unobserved.
