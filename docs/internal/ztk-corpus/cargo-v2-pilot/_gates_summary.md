# Atomicity + dedup gates — consumer run

- Corpus in: 15
- Atomicity violations: 1 NEEDS_SPLIT + 0 FRAGMENT
- Dedup violations: 0
- Survivors: 14

## NEEDS_SPLIT
- `corelink-rust-safety-conventions` — The capture bundles general crate-hardening rules with an unrelated PAT secret-handling rule, which are independent units that each stand alone.
  - proposed: `corelink-adapter-crate-hardening` — CoreLink adapter crates must mark every pub type `#[non_exhaustive]`, set `#![forbid(unsafe_code)]`, and contain zero `u
  - proposed: `corelink-pat-secret-handling` — CoreLink PATs must be stored as `SecretString` and compared using `subtle::ConstantTimeEq`.

## FRAGMENT

## DUPLICATES