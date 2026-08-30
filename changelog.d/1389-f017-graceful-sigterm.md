### Added

- **F-017 — graceful SIGTERM shutdown on the container data-plane server.**
  `crates/corelink-container/src/main.rs` wires
  `axum::serve(...).with_graceful_shutdown(shutdown_signal())`, so a Cloudflare
  containers rollout — which delivers SIGTERM — stops accepting new connections
  and lets in-flight data-plane requests finish instead of being aborted
  mid-flight. `shutdown_signal()` awaits SIGTERM (rollout) or SIGINT (local dev
  and test), with a `ctrl_c` fallback on non-Unix. The complementary halves, the
  wrangler `containers-rollout` drain policy and the `/_health/container`
  `storage==r2` readiness assertion, are tracked separately (TODO in-file).
