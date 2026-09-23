# `corelink-hash` local Cargo test — 2026-09-22

Execution mode: `LOCAL_ISOLATED`. No network, secrets, services or production
systems were used.

- Command: `cargo test -p corelink-hash --locked --offline`
- Checkout: campaign branch `f96233680`, with Rust source files byte-identical
  to source pin `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8`; only the manifest
  repository URL differs, as recorded in the reanchor evidence.
- Result: exit 0.
- Results: library unit target 0 tests; `blob_store_contract` 2 passed;
  `mutation_kills` 7 passed; `prop_hash` 19 passed, 2 release-only tests
  ignored; doctest 1 passed.
- Total: **29 passed, 2 ignored, 0 failed**.

This is local test evidence for the current source-equivalent checkout. It does
not certify release timing gates, wasm, Cargo inverse consumers, runtime,
deployment, peer approval or production reachability. The release-only
performance/constant-time procedures remain open.
