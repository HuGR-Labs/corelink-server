# `corelink-server` source readback — 2026-09-23 (`5e4339c`)

Read-only comparison of the prior server artifact pin
`47f4db7f32bba5ee346e6157203cd480a3abe78b` with the requested main snapshot
`5e4339c50907afe6be682d231d836120fc55fd28`.

## Result

- The package manifest, targets, features and dependencies are unchanged in
  this source delta.
- The source inventory grows from 399 to 401 Rust files under
  `crates/corelink-container/src/`; the 13 integration targets and 14 test
  source files remain unchanged.
- `main_runtime.rs` extracts storage-backing state, the health handler and
  graceful-shutdown signal handling from `main.rs`. The composition root now
  records backing and wires the extracted handlers.
- `routes/dsr/adapter_d1/classification.rs` extracts classification buckets,
  the fail-closed completeness check and its count helper from
  `adapter_d1.rs`. The adapter continues to invoke that check before DSR work.
- `main_tests.rs` includes the extracted files and checks their module wiring.

The source diff is limited to `main.rs`, `main_runtime.rs`, `main_tests.rs`,
`routes/dsr/adapter_d1.rs` and `routes/dsr/adapter_d1/classification.rs`.
The current fetched `origin/main` advanced to
`b9b3ee8cba6ad6f43f73fe785fb99acb55192019`; its `crates/corelink-container`
tree has no diff from the requested `5e4339c` pin. This evidence therefore pins
package claims to the requested immutable snapshot without upgrading unrelated
campaign evidence.

## Limits

This is static source inspection. Tests were not executed. It does not establish
Cargo resolution for shipped targets, deployment, runtime reachability, D1
behavior, health probes in production or graceful shutdown in a live container.

## Reproduction

```text
git diff --name-status 47f4db7f32bba5ee346e6157203cd480a3abe78b..5e4339c50907afe6be682d231d836120fc55fd28 -- crates/corelink-container
git diff --stat 47f4db7f32bba5ee346e6157203cd480a3abe78b..5e4339c50907afe6be682d231d836120fc55fd28 -- crates/corelink-container
git diff --quiet 5e4339c50907afe6be682d231d836120fc55fd28..origin/main -- crates/corelink-container
git ls-tree -r --name-only 5e4339c50907afe6be682d231d836120fc55fd28 crates/corelink-container/src | rg '\\.rs$'
```
