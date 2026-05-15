//! `e2e-tenant-isolation` — R3-8 adversarial end-to-end harness pinning
//! `INV-TENANT-ISOLATION` (canonical cross-tenant security invariant).
//!
//! # Scope
//!
//! This crate is **NOT a production component** — it is a test-only
//! harness that wires:
//!
//! - `corelink-tenant-path::derive_prefix` (real HMAC-SHA256 prefix derivation,
//!   layer 5 of `auth_model.md §8.1`),
//! - `corelink-byok::EnvelopeEncryptor` (real AAD-bound envelope encryption,
//!   layer 4 — encryption_context binds `{tenant_id, blob_hash}`),
//! - `corelink-audit::InMemoryEmitter` (real CloudEvents envelope; captures
//!   every audit event so the fail-CLOSED ordering invariant can be asserted),
//!
//! behind in-memory fakes for the cross-cutting subsystems (CAS / R2,
//! D1, PAT store, idempotency ledger, quota tracker, rate limiter,
//! Stripe webhook ledger). Each fake faithfully implements the
//! production tenant-isolation contract; the fakes are deliberately
//! small so the adversarial scenarios in `tests/adversarial.rs` can
//! read like a security spec rather than a mocking ceremony.
//!
//! # Invariant pinned (INV-TENANT-ISOLATION)
//!
//! The 12 scenarios in `tests/adversarial.rs` together pin the
//! cross-tenant isolation invariant across the six canonical layers:
//!
//! | Layer | Mechanism | Scenario coverage |
//! |---|---|---|
//! | 1. Authn | JWT / PAT sig | Scenario 7 (D1 row spoofing — backend uses JWT tenant_id, not body), Scenario 12 (PAT scoped to tenant) |
//! | 2. Authz | per-tenant ACL | Scenarios 1, 2, 6, 12 (cross-tenant denied + AUTHZ event) |
//! | 3. Quota | per-tenant atomic | Scenarios 9, 10 (quota / rate-limit crosstalk) |
//! | 4. Crypto | BYOK AAD binding | Scenarios 4, 5 (DEK wrap wrong AAD, envelope tamper) |
//! | 5. Prefix | HMAC-derived R2 path | Scenarios 1, 2, 3 (CAS read/write/list) |
//! | 6. Audit | fail-CLOSED emit-before-reject | every scenario asserts `emitter.snapshot().last()` carries the matching deny event PRIOR to the rejection return path |
//!
//! Plus Scenario 8 (idempotency collision) and Scenario 11 (Stripe
//! webhook replay) which pin cross-tenant non-fungibility of
//! single-use tokens / event ids.
//!
//! # Charter constraints honoured
//!
//! - `#![forbid(unsafe_code)]` crate-wide.
//! - No `unwrap` / `expect` / `panic` / `[i]` indexing in library code
//!   (all clippy lints are `deny`).
//! - No `tokio` in `src/` — async runtime is `[dev-dependencies]` only,
//!   pulled in for `#[tokio::test]` async assertions in `tests/`.
//! - Tenant id comparison helpers use `subtle::ConstantTimeEq`
//!   (defence-in-depth — fake stores look up by tenant id and a
//!   timing oracle could leak the existence of a sibling tenant under
//!   a real network stack).
//! - Audit emission happens BEFORE every rejection return — assertions
//!   in `tests/adversarial.rs` verify the emitter snapshot contains
//!   the deny event whose `tenant_id` matches the *requester* (not the
//!   victim) so an attacker probing cross-tenant resources cannot
//!   suppress audit by triggering a rejection path.
//!
//! # Quickstart
//!
//! ```no_run
//! use e2e_tenant_isolation::TenantCtx;
//! # fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let tenant_a = TenantCtx::tenant_a()?;
//! let tenant_b = TenantCtx::tenant_b()?;
//! assert_ne!(tenant_a.prefix().as_str(), tenant_b.prefix().as_str());
//! # Ok(()) }
//! ```

#![forbid(unsafe_code)]

pub mod fakes;
pub mod tenants;

pub use fakes::{
    AuditAttempt, AuditCapture, CasStore, DenyKind, IdempotencyStore, PatStore, QuotaStore,
    RateLimiter, StripeWebhookLedger,
};
pub use tenants::TenantCtx;
