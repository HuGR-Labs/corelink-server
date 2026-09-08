### Fixed

- **B-262 fixed the Neon shadow CI module-resolution failure.** `real.rs` now
  binds its production `native.rs` implementation explicitly as a sibling,
  with a fail-closed focal verifier and mutation suite protecting the path,
  native-only configuration, parent export, and implementation binding.
