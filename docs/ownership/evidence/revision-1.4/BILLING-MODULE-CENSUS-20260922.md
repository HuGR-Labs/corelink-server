# `corelink-billing` module census — 2026-09-22

This read-only census tests the H-profile trigger against the immutable pilot
source pin, rather than treating a file count as a semantic-module claim.

- Source pin: `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`
- Command basis: enumerate tracked `crates/corelink-billing/src/**/*.rs` at
  that Git object, exclude `*_tests.rs`, then count Rust `mod` declarations
  excluding inline `tests`, `cas_tests` and `check_tests` modules.
- Non-test Rust files: **47**.
- Source-visible implementation module declarations: **49**.
- Top-level declarations in `src/lib.rs`: 11.
- Nested declaration groups: abuse 7; quota 3 + core 7 + cas 7 + fsm 5;
  replay 6; stripe 3.

The 49 declarations are a source-backed implementation-surface count and are
well above the H trigger of more than 20 semantic implementation modules. The
count is not a runtime/reachability claim and does not approve the billing
artifacts; a fresh independent reviewer must still decide whether this
classification satisfies the frozen standard.

## Semantic-module predicate

For calibration, a declaration counts as a semantic implementation module when
its module path resolves to a tracked non-test Rust source file at the source
pin, and that file contains implementation items rather than only an inline
test module. This excludes `cas_tests.rs`, `check_tests.rs` and inline
`tests`/`cas_tests`/`check_tests` declarations. The 47 qualifying source paths
are:

```text
abuse.rs; abuse/audit.rs; abuse/config.rs; abuse/error.rs; abuse/features.rs;
abuse/metrics.rs; abuse/score.rs; abuse/scorer.rs; aggregator.rs; emit.rs;
lib.rs; quota.rs; quota/cas.rs; quota/cas/audit.rs; quota/cas/cas.rs;
quota/cas/config.rs; quota/cas/error.rs; quota/cas/metrics.rs;
quota/cas/retry_after.rs; quota/cas/state.rs; quota/core.rs;
quota/core/audit.rs; quota/core/check.rs; quota/core/config.rs;
quota/core/error.rs; quota/core/metrics.rs; quota/core/reservation.rs;
quota/core/retry_after.rs; quota/fsm.rs; quota/fsm/audit.rs;
quota/fsm/error.rs; quota/fsm/event.rs; quota/fsm/fsm.rs; quota/fsm/store.rs;
rate_headers.rs; ratelimit.rs; reconcile.rs; replay.rs; replay/archive.rs;
replay/audit.rs; replay/engine.rs; replay/error.rs; replay/event.rs;
replay/idempotency.rs; stripe.rs; stripe_materializer.rs; tier.rs
```

The predicate is falsifiable from the pinned Git tree with `git ls-tree` and
source inspection. It establishes semantic implementation surface, not Cargo
resolution, runtime reachability or executed behavior.
