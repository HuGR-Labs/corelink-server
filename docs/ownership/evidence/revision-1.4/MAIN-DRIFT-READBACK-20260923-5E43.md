# Main drift readback — 2026-09-23

This readback compares the campaign source snapshot `50a5ab3a` with the
currently fetched `origin/main=5e4339c50907afe6be682d231d836120fc55fd28`.
It is evidence of drift, not an approval or a promotion of campaign pins.

## Result

- Cargo manifests and `Cargo.lock` do not differ in this delta.
- The five pilot package trees are unchanged except for `corelink-server`
  (`crates/corelink-container`) and its DSR source paths.
- The changed server paths are `src/main.rs`, `src/main_runtime.rs`,
  `src/main_tests.rs`, `src/routes/dsr/adapter_d1.rs`, and
  `src/routes/dsr/adapter_d1/classification.rs`.
- The remaining delta is workflow, backlog, capacity, operator-document,
  verifier-script and verifier-test material outside the pilot package trees.

## Decision

The campaign keeps the immutable documentary snapshot `50a5ab3a`. Existing
artifact approvals remain scoped to their recorded source pins; no claim in
the registry is silently upgraded to `origin/main`. `corelink-server` and its
DSR relations are stale for any current-main claim and require a later
source readback before a future freeze or integration. No issue publication,
deployment, runtime reachability or production behavior is inferred here.

## Reproduction

```text
git diff --stat 50a5ab3a..origin/main -- Cargo.toml Cargo.lock crates/corelink-container crates/corelink-billing crates/corelink-cf-bindings tests/e2e-billing-flow tools/sdks
git diff --name-only 50a5ab3a..origin/main
```
