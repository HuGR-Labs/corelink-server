### Changed

- **PR #1485's bounded money-path effect-probe wait is explicitly retired on
  the D03 base.** Commit `3f2a1b1484a6d4520faa35b9db9e67d52eb7f20c` changed the
  historical `crates/corelink-container/tests/money_path_internal_auth.rs`
  integration test to wait up to one second for a local refusing-proxy
  classification. That test binary is not present at the exact D03 base
  `ed0cd972a7453ccbd9783d258f2e46d204085ef8`; its executable replacement is
  `crates/corelink-container/tests/money_path_auth_wiring.rs`, which preserves
  `build_state_from_env` plus `router().oneshot` wiring and verifies zero D1 /
  Stripe effects for invalid auth, exactly one D1 attempt for each positive
  control, and a bounded one-second observation window. The current
  tier-select, DPA, and worker auth regressions remain covered by their local
  tests. No production latency or closure is inferred. The replacement is a
  real Rust integration test; the exact bundle gate is:

  ```sh
  cargo test -p corelink-server --test money_path_auth_wiring
  ```

  No source-marker or Python parser is treated as proof that this behavior
  executes.
