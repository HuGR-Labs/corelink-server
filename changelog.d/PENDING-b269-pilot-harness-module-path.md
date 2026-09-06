### Fixed

- **B-269 restores the pilot-onboarding harness split.** The parent harness now
  binds its extracted lifecycle implementation from the existing sibling file,
  so workspace tests and all-target Clippy compile the complete E2E crate.
