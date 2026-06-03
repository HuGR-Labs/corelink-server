//! `e2e-signup-flow` — R3-1 end-to-end harness for the customer happy
//! path against the in-memory CoreLink stack.
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter R3 phase scope, this
//! crate is **NOT a production component** — it is a test-only harness
//! that wires the WI-S19-001/002/004 pure-logic crates plus a tiny
//! in-memory R2 CAS surface so the full customer journey
//!
//! ```text
//! Clerk webhook → signup orchestrator
//!   → DPA acceptance (6-field consent + RS256 receipt)
//!     → tier select (INV-ONBOARD-DPA-FIRST + Stripe Checkout)
//!       → first PAT issued
//!         → PAT-authenticated R2 CAS PUT (BLAKE3-verified)
//!           → GET roundtrip + stat hit
//! ```
//!
//! can be exercised in a single `cargo test -p e2e-signup-flow` run
//! without any network IO, plus a `--ignored`-gated variant that drives
//! the real HTTPS Stripe Checkout client (corelink-stripe-real) when
//! `STRIPE_SECRET_KEY_TEST` is set in the environment.
//!
//! # Charter constraints honoured
//!
//! - `#![forbid(unsafe_code)]` and `#[non_exhaustive]` are preserved on
//!   every consumed type (we never brace-init across crate boundary —
//!   only the canonical constructors are used).
//! - No test-only `unsafe`.
//! - Idempotency keys are deterministic (`make_test_tenant` derives a
//!   stable key from the tenant name slug).
//! - Live-mode Stripe paths are gated by env var + `#[ignore]` per the
//!   charter `live-only-behind-env-gate` clause.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod helpers;
pub mod r2;

pub use helpers::{
    make_test_tenant, setup_test_ledgers, verify_audit_chain, ExpectedAuditEvent,
    ProvisionedTenant, TenantBundle, TestEnv,
};
pub use r2::{InMemoryR2Client, R2Error, R2StatReport};

/// Canonical DPA version used by the in-memory fixtures.
pub const TEST_DPA_VERSION: &str = "1.0.0";

/// Canonical primary region used by the in-memory fixtures.
pub const TEST_DPA_NOTICE_EN: &str = "DPA v1.0.0 (en-US) — canonical e2e harness text.";

/// Stripe test webhook secret used by the in-memory webhook signature
/// fixture (mirrors Stripe's `whsec_*` convention but is NOT a real
/// secret — only used to assert signature-mismatch behaviour).
pub const TEST_WEBHOOK_SECRET: &[u8] = b"whsec_test_e2e_signup_flow_harness";
