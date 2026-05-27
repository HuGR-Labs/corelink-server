---
id: "AUDIT-2026-05-16-STRIPE-WASM32-GATE-LIFT"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "stripe", "wasm32", "r-prep", "wave-19", "trait-abstraction-defer", "billing", "pattern"]
---

# Stripe wasm32 Gate Lift — `corelink-stripe-real` Per-Module Cfg Pattern

> **Audit Date:** 2026-05-16 · **Branch:** `wt/r-prep-stripe-wasm32-gate-lift` · **Lane:** R-PREP (release-prep) · **Wave:** 19
> **Reviewer:** Gustavo Schneiter
> **Crate touched:** `crates/corelink-stripe-real`
> **Crate unblocked:** `crates/corelink-billing-stripe-materializer` (wave-18 SEAL)
> **Cross-ref:** [`specs/_audits/sealed/2026-05-15-cf-binding-real-pattern.md`](2026-05-15-cf-binding-real-pattern.md)
> **WI closure:** `specs/04_sprints/S10/work_items/WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md` § change-log v1.5.0
> **Disposition:** Lifts the crate-root `#![cfg(not(target_arch = "wasm32"))]` so the wasm32-safe surface (`webhook` / `webhook_dispatch` / `error` / `retry` / `dlq` / `portal`) compiles on `wasm32-unknown-unknown`. The native-only HTTPS client (`client.rs`, depends on `reqwest::blocking`) is per-module gated to non-wasm32 targets. Mirrors the `corelink-cf-bindings` r2/d1/kv/do_real per-module gate pattern.

---

## 1. Context

Wave 15 (`corelink-stripe-real::webhook_dispatch`) shipped the canonical Stripe webhook trait pipeline (classification + idempotency + audit + SLI primitives). Wave 18 (`corelink-billing-stripe-materializer`) layered the D1-backed production materializers on top of those traits.

The crate was originally created with a **crate-root** `#![cfg(not(target_arch = "wasm32"))]` (wave-15 baseline; see lib.rs commit prior to wave-19). Rationale at the time: `client.rs` wraps `reqwest::blocking::Client`, which depends on `tokio` runtime + native TLS — neither runnable on `wasm32-unknown-unknown`. Gating the **whole crate** was a one-line fix but had a hidden cost: every downstream crate that imported any type from `corelink_stripe_real` (e.g. `webhook_dispatch::AuditEmitter`, `webhook_dispatch::StateMaterializer`, `error::WebhookVerifyError`) failed to wasm32-build because the entire module tree was empty under that cfg.

Wave 18's `corelink-billing-stripe-materializer` is itself wasm32-buildable (CF Worker target via `cf-billing-real` feature). Its `src/audit.rs`, `src/handler.rs`, `src/idempotency.rs`, `src/d1.rs` all import from `corelink_stripe_real::webhook_dispatch`. The wave-15 crate-root gate therefore manifested as:

```
error[E0432]: unresolved import `corelink_stripe_real::webhook_dispatch`
  --> crates/corelink-billing-stripe-materializer/src/audit.rs:19:27
   |
19 | use corelink_stripe_real::webhook_dispatch::{AuditEmitter, AuditRecord};
   |                           ^^^^^^^^^^^^^^^^ could not find `webhook_dispatch` in `corelink_stripe_real`
```

…blocking `cargo build --target wasm32-unknown-unknown -p corelink-billing-stripe-materializer`.

The fix is per-module gating: keep `client.rs` (the actual native-only module) gated, expose everything else.

## 2. Decision

**Approach (A) — per-module gate.** Selected over approach (B) — extract `corelink-stripe-types`. Rationale:

| Criterion | (A) per-module gate | (B) split crate |
| --- | --- | --- |
| Moving parts | 1 crate, 2 edits (lib.rs root cfg + Cargo.toml dep arm) | 2 crates, full dep wiring, 8 module re-exports moved |
| Risk to wave-15 SEALed crate | Surgical (only crate-root + reqwest dep gate change) | Restructures the crate Cargo + lib boundary |
| Backward compat | All existing `corelink_stripe_real::*` paths preserved | Forces caller migration on every import path |
| Pattern parity | Matches `corelink-cf-bindings` r2/d1/kv/do_real (4 precedents) | Diverges from existing R-PREP norm |
| Reversibility | Trivial revert | Hard revert (crate split is permanent) |

(A) wins on every axis.

## 3. Implementation

### 3.1 `crates/corelink-stripe-real/src/lib.rs`

Before:

```rust
#![cfg(not(target_arch = "wasm32"))]
#![forbid(unsafe_code)]
…
pub mod client;
pub mod dlq;
…
pub use client::{StripeRealClient, StripeRealClientBuilder};
…
```

After:

```rust
#![forbid(unsafe_code)]
…
#[cfg(not(target_arch = "wasm32"))]
pub mod client;
pub mod dlq;
…
#[cfg(not(target_arch = "wasm32"))]
pub use client::{StripeRealClient, StripeRealClientBuilder};
…
```

The crate-root cfg is **deleted**. `client` (the only module that references `reqwest`) and its `pub use` re-export are gated. All other modules — `dlq`, `error`, `portal`, `retry`, `webhook`, `webhook_dispatch` — compile on both native and wasm32.

### 3.2 `crates/corelink-stripe-real/Cargo.toml`

Before:

```toml
[dependencies]
…
reqwest = { workspace = true, features = ["json", "rustls-tls", "blocking"], default-features = false }
…
```

After:

```toml
[dependencies]
…
# (reqwest removed from native-shared block)
…

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { workspace = true, features = ["json", "rustls-tls", "blocking"], default-features = false }
```

This prevents Cargo from even resolving `reqwest`'s wasm32 graph — which would otherwise pull in `web-sys`, `js-sys`, `wasm-bindgen-futures` for a crate that no longer uses any of those on wasm32. Slimmer wasm32 build; faster compile.

### 3.3 What did NOT change

- Native-side surface: `StripeRealClient`, `StripeRealClientBuilder`, `STRIPE_API_BASE`, `CustomerObject`, `SubscriptionObject`, `BillingPortalSession`, `CheckoutSessionObject`, `parse_retry_after` — all unchanged.
- Webhook signature verify (`subtle::ConstantTimeEq`) — untouched.
- Audit fail-CLOSED contract (`webhook_dispatch::AuditEmitter`, `portal::PortalAuditSink`) — untouched.
- `#[non_exhaustive]` on every public type — preserved.
- Idempotency primitives (`IdempotencyKey`, `IdempotencyStore`, `BLAKE3-256` token derivation) — untouched.
- Retry policy (`RetryPolicy`, `next_sleep_ms`, `is_retryable_status`) — untouched.

## 4. Pattern

The same per-module cfg-gate template now covers two crates with native-leaning real impls:

| Crate | Module layout | Native-only modules | wasm32-safe modules |
| --- | --- | --- | --- |
| `corelink-cf-bindings` | per-module cfg | (n/a — all `*_real.rs` are `wasm32`-only with native stubs) | (the trait-stub layer) |
| `corelink-stripe-real` | per-module cfg | `client` (reqwest::blocking) | `webhook`, `webhook_dispatch`, `error`, `retry`, `dlq`, `portal` |

The lib root carries `#![forbid(unsafe_code)]` + `#![deny(missing_docs)]` + `#![deny(missing_debug_implementations)]` but **does not** carry `#![cfg(target_arch = "wasm32")]` or its negation.

### 4.1 Replication recipe

For any crate that ships a wasm32-native split surface:

1. Identify the **native-only** modules (those importing `reqwest`, `tokio::*`, `std::process`, `std::os::unix`, native FFI, etc.).
2. Identify the **wasm32-safe** modules (pure logic + trait surface + types).
3. Gate **only** the native-only modules with `#[cfg(not(target_arch = "wasm32"))]` (module decl + `pub use` re-exports).
4. Move the corresponding **native-only Cargo deps** into `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`.
5. Do **not** add a `#![cfg(...)]` at the crate root.

This preserves crate-level forbid lints + doc lints uniformly while letting downstream wasm32 callers see the type-level surface.

### 4.2 When to choose split-crate instead

Split into `<crate>-types` only if:

- The native module count exceeds the wasm32-safe count (split provides a clearer named boundary), OR
- The native modules pull a non-trivial macro/build-script chain that even a target-conditional dep arm can't isolate (e.g. proc-macro side-effects), OR
- Multiple downstream crates need to depend on the type surface without the heavyweight native dep graph.

None of those apply to `corelink-stripe-real`. The native arm is one module (`client.rs`, 620 lines) and one dep (`reqwest`). Per-module gating wins.

## 5. Charter compliance

| Constraint | Status |
| --- | --- |
| `#![forbid(unsafe_code)]` | preserved (crate root) |
| No `unwrap` / `expect` / `panic` outside test code | preserved (no semantic change) |
| `#[non_exhaustive]` on every public type | preserved |
| Audit fail-CLOSED contract | preserved (no audit emit path touched) |
| `subtle::ConstantTimeEq` for signature compare | preserved (no `webhook.rs` change) |
| Per-instance `Arc<Mutex<…>>` F-001 closure | preserved (`InMemoryIdempotencyStore` untouched) |
| DCO sign-off + Co-Authored-By | included in commit |

## 6. Test evidence

| Gate | Command | Result |
| --- | --- | --- |
| wasm32 build (stripe-real) | `cargo build --target wasm32-unknown-unknown -p corelink-stripe-real` | green |
| wasm32 build (materializer) | `cargo build --target wasm32-unknown-unknown -p corelink-billing-stripe-materializer` | green |
| Native build | `cargo build -p corelink-stripe-real` | green |
| Native unit tests | `cargo test -p corelink-stripe-real --lib` | 51 passed; 0 failed |
| Native integration tests | `cargo test -p corelink-stripe-real` | 12 passed (webhook_e2e); 0 failed |
| Workspace clippy | `cargo clippy --workspace --all-targets -- -D warnings` | green |
| validate_specs.py | `python3 tools/validators/validate_specs.py` | green (no regressions; baseline drift unchanged) |
| validate_references.py | `python3 tools/validators/validate_references.py` | green |

## 7. Tests net-new

Zero. This is a build-system fix; the underlying semantics are unchanged. The existing 63 native tests (51 lib + 12 integration) cover the wasm32-safe surface end-to-end; on wasm32 the same code paths compile (verified via `cargo build`) but production callers route through the in-memory fake per the `trait-abstraction-defer` charter — there is no wasm32 runtime to exercise here.

A future follow-up may add a wasm32 smoke test driven from `corelink-billing-stripe-materializer`'s wasm32 production binders (`cf-billing-real` feature). That belongs to the materializer's runtime-binder PR, not to this gate-lift.

## 8. Caveats

- **`portal.rs` + `webhook_dispatch.rs` use `std::time::SystemTime::now()`** — ~~OPEN~~ **CLOSED 2026-05-16 (wave-20)**. Replaced by a target-conditional [`Clock`] trait abstraction (`crates/corelink-stripe-real/src/clock.rs`): native injects `SystemClock` (wraps `SystemTime::now()`), wasm32 injects `WasmWorkerClock` (reads `js_sys::Date::now()` — never panics on `wasm32-unknown-unknown`), tests inject `InMemoryFakeClock` (deterministic). `StripeRealClientBuilder::with_clock` + `InMemoryPortalSessionCreator::with_clock` setters added. All `SystemTime::now()` call sites in the wasm32-safe surface (`portal.rs`, `webhook_dispatch.rs::SystemClock`) eliminated. Net-new tests: +4 lib (3 in `clock.rs`, 1 in `portal.rs::clock_injection_used_for_idempotency_keys`) + 1 doctest. Closure branch: `wt/r-prep-stripe-wasm32-clock-trait`.
- **`corelink-billing-stripe-materializer::handler::SystemMatClock`** — ~~OPEN (wave-20 follow-on)~~ **CLOSED 2026-05-16 (wave-22)** — see §9.
- **`client.rs` re-exports**: the `pub use client::{StripeRealClient, StripeRealClientBuilder}` re-export is cfg-gated to mirror the module gate. Callers using `corelink_stripe_real::StripeRealClient` on wasm32 will get a clean `unresolved import` (intended; they should be using the trait or the in-memory fake).
- **Dev-dep `tokio`**: unchanged. Used only in test code (`#[tokio::test]` arms) which never runs on wasm32.

## 9. MatClock follow-on closure (wave-22, 2026-05-16)

The wave-20 SEAL note flagged that `corelink-billing-stripe-materializer::handler::SystemMatClock` was intentionally left out of scope — it owned a separate `MatClock` trait (private to `handler.rs`) whose `SystemMatClock` impl called `std::time::SystemTime::now()` directly. On `wasm32-unknown-unknown` this would panic at runtime in the Cloudflare Worker before the first webhook materializes. Wave-22 closes that follow-on by mirroring the wave-20 pattern end-to-end.

### 9.1 What changed

- New module `crates/corelink-billing-stripe-materializer/src/clock.rs` exposing the [`MatClock`] trait + three impls:
  - `SystemMatClock` — `cfg(not(target_arch = "wasm32"))`; wraps `SystemTime::now()`.
  - `WasmWorkerMatClock` — `cfg(target_arch = "wasm32")`; reads `js_sys::Date::now()` with `is_finite()` + `< 0.0` clamping; never panics.
  - `InMemoryFakeMatClock` — always available; `at_unix_ms` / `at_unix_seconds` / `advance(Duration)` / `set_unix_ms` for deterministic tests.
- Trait surface: canonical `fn now(&self) -> SystemTime`; convenience `fn now_ms(&self) -> u64` with a default impl that derives from `now()` (saturates to `0` pre-epoch, clamps to `u64::MAX` on overflow). The handler keeps calling `clock.now_ms()` — no audit `ts_ms` semantics shifted.
- Factory `default_mat_clock() -> Arc<dyn MatClock + Send + Sync>` — native picks `SystemMatClock`, wasm32 picks `WasmWorkerMatClock`. `D1SubscriptionStateHandler::new` now wires through this factory (was hardcoded `Arc::new(SystemMatClock)` in handler.rs, fatal on wasm32).
- `handler.rs` no longer declares its own `MatClock` trait / `SystemMatClock` / `FixedMatClock`. The handler imports `MatClock` from `crate::clock` and the internal test fixture uses `InMemoryFakeMatClock::at_unix_ms(1_700_000_000_000)` instead of `FixedMatClock(1_700_000_000_000)`.
- `Cargo.toml`: added `[target.'cfg(target_arch = "wasm32")'.dependencies] js-sys = { workspace = true }` (mirrors the wave-20 stripe-real arm). Native + `cf-billing-real` arms unchanged.
- `src/lib.rs`: re-exports `MatClock`, `InMemoryFakeMatClock`, `default_mat_clock`, and the cfg-gated `SystemMatClock` (native) / `WasmWorkerMatClock` (wasm32). The handler's old `pub struct FixedMatClock` (only re-exposed for cross-crate tests, but nothing actually used it) is gone; downstream test crates should use `InMemoryFakeMatClock::at_unix_ms` instead.

### 9.2 `SystemTime::now` call sites replaced

| Site | Before | After |
| --- | --- | --- |
| `handler.rs::SystemMatClock::now_ms` | `std::time::SystemTime::now().duration_since(UNIX_EPOCH)...` | trait impl moved to `clock.rs::SystemMatClock::now() -> SystemTime` (native cfg-gated); the duration math is the default `MatClock::now_ms` derived from `now()` |

One direct `SystemTime::now()` call site eliminated from the wasm32-safe surface (`handler.rs`). The new `clock.rs::SystemMatClock` is `cfg(not(target_arch = "wasm32"))`-gated, so the wasm32 crate graph no longer contains any reachable `SystemTime::now()` call.

### 9.3 Tests net-new (+3)

All three live in `src/clock.rs::tests`:

1. `fake_clock_returns_injected_time_and_advances` — pins at `1_700_000_000` unix-s, checks `now_ms()` + `now() -> SystemTime`, then `advance(2_500ms)` + `set_unix_ms` round-trips.
2. `system_clock_returns_non_decreasing_ms` — `cfg(not(target_arch = "wasm32"))`-gated; spin-loops until the OS clock advances, then asserts `b >= a`.
3. `default_mat_clock_returns_target_appropriate_impl` — calls `default_mat_clock().now_ms()` + `.now()` on whatever the current target is. On native this exercises `SystemMatClock`; on wasm32 this exercises `WasmWorkerMatClock` (the wasm32-safe build path — `cargo build --target wasm32-unknown-unknown` proves the closure compiles).

Plus 1 doctest on the `clock` module header showing `InMemoryFakeMatClock` deterministic-time idiom.

### 9.4 Quality-gate evidence

| Gate | Command | Result |
| --- | --- | --- |
| Native build | `cargo build -p corelink-billing-stripe-materializer` | green |
| wasm32 build | `cargo build --target wasm32-unknown-unknown -p corelink-billing-stripe-materializer` | green |
| Native tests | `cargo test -p corelink-billing-stripe-materializer` | 21 lib + 8 integration + 1 doctest; all passed (was 18 lib pre-wave-22) |
| Workspace clippy | `cargo clippy --workspace --all-targets -- -D warnings` | green |
| validate_specs.py | `python3 scripts/validate_specs.py` | green |
| validate_references.py | `python3 scripts/validate_references.py` | green |

### 9.5 Charter compliance

| Constraint | Status |
| --- | --- |
| `#![forbid(unsafe_code)]` | preserved (crate root) |
| No `unwrap` / `expect` / `panic` / `indexing_slicing` in `src/` | preserved (mutex-poison path returns inner deterministically per wave-20 idiom) |
| `mod_module_files = "deny"` | preserved (`clock.rs` is a sibling module, not a `clock/mod.rs`) |
| Audit fail-CLOSED ordering | preserved (no audit emit path semantics touched; only the clock impl behind `ts_ms` changed) |
| DCO sign-off + Co-Authored-By | included in commit |

## 10. Sign-off

| Role | Status |
| --- | --- |
| Owner / Final Approver (Gustavo) | SEALED 2026-05-16 |
| Architect | SEALED 2026-05-16 (per-module gate pattern matches cf-binding precedent) |
| Wave-22 MatClock closure (Gustavo) | SEALED 2026-05-16 |

---

**End audit doc.**
