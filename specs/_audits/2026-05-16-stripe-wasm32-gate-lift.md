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
> **Cross-ref:** [`specs/_audits/2026-05-15-cf-binding-real-pattern.md`](2026-05-15-cf-binding-real-pattern.md)
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

- **`portal.rs` + `webhook_dispatch.rs` use `std::time::SystemTime::now()`**: compiles on `wasm32-unknown-unknown` but panics at runtime (`unknown-unknown` has no clock). This is acceptable because production wasm32 callers consume the in-memory fakes (no clock reads) or inject a `Clock` impl backed by `worker::Date::now()`. The native HTTPS path (`client.rs`) — where the real Stripe API talks — stays native-only. Documented in `lib.rs` rustdoc.
- **`client.rs` re-exports**: the `pub use client::{StripeRealClient, StripeRealClientBuilder}` re-export is cfg-gated to mirror the module gate. Callers using `corelink_stripe_real::StripeRealClient` on wasm32 will get a clean `unresolved import` (intended; they should be using the trait or the in-memory fake).
- **Dev-dep `tokio`**: unchanged. Used only in test code (`#[tokio::test]` arms) which never runs on wasm32.

## 9. Sign-off

| Role | Status |
| --- | --- |
| Owner / Final Approver (Gustavo) | SEALED 2026-05-16 |
| Architect | SEALED 2026-05-16 (per-module gate pattern matches cf-binding precedent) |

---

**End audit doc.**
