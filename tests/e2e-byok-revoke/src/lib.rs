//! `e2e-byok-revoke` — R3-2 end-to-end harness for the WI-S14-006 CMK
//! revocation kill switch invariant (INV-BYOK-CRYPTO-SOVEREIGNTY).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter R3 phase scope, this
//! crate is **NOT a production component** — it is a test-only harness
//! that wires the `corelink-byok` + `corelink-byok-revocation` +
//! `corelink-customer-alerts` crates so the full revocation pipeline
//!
//! ```text
//! customer revokes CMK in BYOK provider
//!   → RevocationDetector polls `KmsProvider::check_access`
//!     → KmsAccessStatus::Revoked observed
//!       → DEK cache evicted atomically (ZeroizeOnDrop)
//!         → tenant `byok_status = 'degraded_read_only'`
//!           → audit event `corelink.byok.cmk_revoked` emitted
//!             → customer alert dispatched via `MultiChannelAlerter`
//! ```
//!
//! is exercised end-to-end in a single `cargo test -p e2e-byok-revoke`
//! run without any network IO. Live-provider variants are
//! `#[ignore]`-gated behind `AWS_TEST_KEY_ARN`, `GCP_TEST_KEY_RESOURCE`,
//! `AZURE_TEST_KEY_RESOURCE`, and `VAULT_TEST_KEY_NAME` environment
//! variables.
//!
//! # Charter constraints honoured
//!
//! - `#![forbid(unsafe_code)]` and `#[non_exhaustive]` are preserved on
//!   every consumed type (canonical constructors only — no brace-init
//!   across the crate boundary).
//! - No test-only `unsafe`.
//! - No `tokio` usage in `src/` runtime constructs (`tokio` only appears
//!   as a `#[tokio::test]` attribute target — runtimes are spun up by
//!   the integration tests themselves).
//! - Live-mode provider paths are gated by env var + `#[ignore]` per the
//!   charter `live-only-behind-env-gate` clause.
//!
//! # INV-BYOK-CRYPTO-SOVEREIGNTY
//!
//! The harness asserts the invariant in at least three distinct test
//! functions:
//!
//! 1. [`happy_revoke_flow`](../happy_revoke_flow/index.html) — kill
//!    switch fires within SLA and DEK cache is fully evicted post-revoke.
//! 2. [`adversarial_stampede`](../adversarial_stampede/index.html) —
//!    10 000 cache entries are evicted on revocation; zero plaintext
//!    remains.
//! 3. [`adversarial_race_condition`](../adversarial_race_condition/index.html)
//!    — concurrent encrypt during revocation fails-CLOSED (Err return,
//!    not garbage plaintext).
//! 4. [`prop_fail_closed`](../prop_fail_closed/index.html) — 1 000
//!    random revoke timings × encrypt/decrypt interleavings: fail-CLOSED
//!    holds.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod helpers;

pub use helpers::{
    setup_byok_env, AuditSink, BoundedKmsProvider, CustomerAlertSink, ExpectedRevocationEvent,
    KmsBehaviour, RevokeBundle, RevokeError,
};
