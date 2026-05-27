# SEAL — `corelink-clerk::fakes` feature-gate (charter audit §L2.1 / §L2.7)

- **Date**: 2026-05-27
- **Owner**: CLERK-FAKES-GATE agent (worktree `agent-a58bd676ec33976b6`)
- **Source escalation**: `specs/_audits/2026-05-27-charter-strict-audit-post-w36.md` §L2.1 ("test code must not leak to prod") and §L2.7
- **Decision**: branch B of the §3 decision tree — add a `test-utils` feature gate; consumers opt in via `[dev-dependencies]`.

## 1. Problem

`crates/corelink-clerk/src/lib.rs:119` declared `pub mod fakes;` with no
`cfg` gate. Although the `fakes::test_keys` submodule (RSA keygen for
signed-JWT crafting) was already gated behind `#[cfg(feature = "test-utils")]`
(L313 of `fakes.rs`), the surrounding fakes (`InMemoryKvCache`,
`StaticJwksFetcher`, `ScriptedJwksFetcher`, slot/clock helpers) were
unconditionally compiled into **every** consumer of `corelink-clerk` —
including production builds of `corelink-worker` and the CF Worker
binding crates. This violates the charter principle that test fakes must
not be linkable from non-test profiles.

## 2. Consumer survey

`rg "corelink_clerk::fakes" --include "*.rs"` across the workspace:

| Path                                                       | Profile        | Notes |
|------------------------------------------------------------|----------------|-------|
| `crates/corelink-clerk/src/lib.rs` (rustdoc only)          | doc            | links in module docstrings — safe |
| `crates/corelink-clerk/tests/*.rs` (4 files)               | test           | clerk's own integration tests |
| `crates/corelink-clerk/examples/*.rs` (3 files)            | example        | demo apps |
| `crates/corelink-worker/tests/auth_middleware_smoke.rs`    | dev-dependency | only worker consumer; already pulls `test-utils` |
| `crates/corelink-clerk-cf/src/lib.rs` (rustdoc only)       | doc            | safe |
| `crates/corelink-worker/src/**` (rustdoc only)             | doc            | safe |

**No production consumer.** Branch B applies (the worker dev-deps block
at `crates/corelink-worker/Cargo.toml:120` already enables
`features = ["jwt-adapter", "test-utils"]`).

## 3. Change set

### 3.1 `crates/corelink-clerk/src/lib.rs`

```diff
- pub mod fakes;
+ // `fakes` is a test-only surface (InMemoryKvCache, StaticJwksFetcher,
+ // TestRsaKey). Gated behind `test-utils` so production builds (default
+ // features) never compile it — closes §L2.1/§L2.7 escalation in
+ // `specs/_audits/2026-05-27-charter-strict-audit-post-w36.md`.
+ #[cfg(any(test, feature = "test-utils"))]
+ pub mod fakes;
```

`cfg(test)` is retained because the clerk crate's **own** unit tests
(`src/**/#[cfg(test)] mod tests`) consume `crate::fakes::*` without
needing the feature toggled at the `cargo test -p corelink-clerk` entry
point. `cfg(test)` only activates for the crate-being-tested, so
external consumers continue to need the `test-utils` feature; this is
purely an ergonomic shortcut for in-crate tests and does not widen the
production surface.

### 3.2 `crates/corelink-clerk/Cargo.toml`

Examples gained `required-features = ["jwt-adapter", "test-utils"]`. The
clerk crate's `[features] test-utils = ["dep:rsa", "dep:rand"]` block
already existed (originally added for the `test_keys` submodule). No
new features or deps were introduced.

```diff
 [[example]]
 name = "basic"
 path = "examples/basic.rs"
+# Uses `fakes::{InMemoryKvCache, StaticJwksFetcher}` — gated behind
+# `test-utils` (post-§L2.1/§L2.7 charter audit hygiene).
+required-features = ["jwt-adapter", "test-utils"]
 ...
```

Identical `required-features` lines added to the `multi_issuer` and
`rotation` examples.

### 3.3 Consumer crates — no change required

`corelink-worker`'s `[dev-dependencies]` block at L120 already lists
`features = ["jwt-adapter", "test-utils"]`, so
`tests/auth_middleware_smoke.rs` continues to compile.

The clerk crate's own `[dev-dependencies]` at L90 likewise enables
`["jwt-adapter", "test-utils", "http-fetcher"]`.

## 4. Acceptance — all green

| Gate                                                                  | Result | Tail |
|-----------------------------------------------------------------------|--------|------|
| `cargo build -p corelink-clerk` (default features only)               | GREEN  | Finished dev profile in 1m 29s |
| `cargo build -p corelink-clerk --features test-utils`                 | GREEN  | Finished dev profile in 39.28s |
| `cargo test -p corelink-clerk --no-run`                               | GREEN  | 6 test executables linked |
| `cargo clippy -p corelink-clerk --tests -- -D warnings`               | GREEN  | Finished dev profile in 1m 21s |
| `cargo build -p corelink-worker`                                      | GREEN  | Finished dev profile in 47.41s |
| `cargo test -p corelink-worker --no-run`                              | GREEN  | 8+ test executables linked |
| `cargo build -p corelink-clerk --examples --features test-utils`      | GREEN  | Finished in 1.23s (cached) |

Cold default-feature build of `corelink-clerk` finishes without
compiling `rsa`, `rand`, or the `fakes` module — symbol scan of the
resulting rlib shows zero `fakes::*` items (only rustdoc string
references remain in `.rmeta` metadata, which is expected and inert).

## 5. Non-goals / out of scope

- No semantic test changes; only the gating cfg moved.
- No new public API in `corelink-clerk`.
- The `fakes` module's internal cfg-gates (e.g. `#[cfg(feature = "test-utils")]` around `test_keys`) are preserved as-is — the outer
  module gate subsumes them but the inner gates remain as a belt-and-suspenders
  signal that those submodules were always test-only.

## 6. References

- Escalation: `specs/_audits/2026-05-27-charter-strict-audit-post-w36.md` §L2.1, §L2.7
- Modified: `crates/corelink-clerk/src/lib.rs`, `crates/corelink-clerk/Cargo.toml`
- Worktree: `.claude/worktrees/agent-a58bd676ec33976b6` (branch `worktree-agent-a58bd676ec33976b6`)
