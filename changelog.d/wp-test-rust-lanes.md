### Added

- **Rust tests now run for the crates a PR can break (WP-TEST).** Of 75 crates,
  only **9** had a `cargo test -p` in a lane that actually runs; 16 more looked
  covered but were named only in `nightly.yml` (zero successful runs) or
  `ffi-matrix-ci.yml` (never worked), and 50 — including `corelink-auth` and
  `corelink-pat`, which hold the auth primitives — were named nowhere.
  `rust-affected-tests.yml` selects by **reverse dependency closure**
  (`cargo tree --invert`) rather than "crates the diff touched", so touching
  `corelink-ratelimit` pulls `corelink-billing` without anyone maintaining a
  list, plus `corelink-auth`/`corelink-pat` unconditionally. Measured closures:
  largest is 13 of 75, so no ceiling is needed.
  `rust-deep-property.yml` runs the same suites at full `PROPTEST_CASES` and
  executes the `#[ignore]`d heavy tests via `--ignored`. No property leaves the
  suite between the lanes — only the iteration count changes.
