//! `e2e-pilot-onboarding` — Wave-23 R-PREP pilot tenant onboarding
//! end-to-end harness.
//!
//! # Pipeline
//!
//! Per the corelink autonomous execution charter, the GA cutover
//! depends on a fully-exercised pilot tenant journey. This crate is
//! **NOT a production component** — it is a test-only harness that
//! composes a deterministic in-memory pipeline so the full customer
//! journey
//!
//! ```text
//! 1. tenant signup (POST /v1/signup → Stripe checkout → webhook →
//!    tenant provisioned in D1)
//!   → 2. first CAS upload (BatchUpdateBlobs with 100 blobs; verify R2
//!         chunks + tenant prefix)
//!     → 3. audit export round-trip (24h window; verify NDJSON + chain
//!           integrity + manifest)
//!       → 4. DSR erasure (POST /v1/dsr/erasure → 7-day window →
//!             verify all CAS + audit cleanup)
//!         → 5. tenant offboarding (subscription cancel → 30-day grace
//!               → final deletion + erasure verification)
//! ```
//!
//! can be exercised in a single `cargo test -p e2e-pilot-onboarding`
//! run without any network IO.
//!
//! # Why self-contained
//!
//! This crate deliberately does NOT depend on the production
//! `corelink-signup`, `corelink-billing-stripe`, `corelink-dsr`,
//! `corelink-audit-chain`, `corelink-r2-multipart`, etc. The pilot
//! onboarding journey is a *contract* across those subsystems — the
//! integration crates (`e2e-signup-flow`, `e2e-billing-flow`,
//! `e2e-dsr`) already pin each subsystem's internals. This harness
//! pins only the cross-subsystem **journey shape** so the 5 tests
//! survive arbitrary API churn inside any one production crate. Per
//! the `charter-trait-abstraction-defer` clause, that is the right
//! seam for a journey-level test.
//!
//! # Charter constraints honoured
//!
//! - `#![forbid(unsafe_code)]` at the crate root.
//! - `#[deny(missing_docs)]` and `#[deny(missing_debug_implementations)]`.
//! - No `unwrap()` / `expect()` / `panic!()` / `indexing_slicing` in
//!   non-test code (every helper returns a typed
//!   [`PilotHarnessError`]).
//! - All identifiers (tenant id, subscription id, audit row hashes,
//!   blob digests) are deterministic functions of the tenant slug so
//!   the harness is byte-reproducible across runs.
//! - Audit-emit ordering is fail-CLOSED at every state mutation: the
//!   harness records the audit row BEFORE flipping the corresponding
//!   ledger state and the journey tests assert that ordering.
//! - No `tokio` in src; no network IO under any branch.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod harness;

pub use harness::{
    blob_digest_hex, canonical_blob_payload, canonical_blob_payloads, canonical_pilot_tenant,
    AuditChainError, AuditEventKind, AuditExportError, AuditExportManifest, AuditRecord,
    BatchUploadReceipt, CasError, DsrErasureError, DsrErasureReceipt, OffboardingError,
    OffboardingReceipt, PilotHarness, PilotHarnessError, PilotTenant, SignupError, SignupReceipt,
    SubscriptionState, TenantLifecycleState, CANONICAL_BLOB_COUNT, DSR_ERASURE_WINDOW_MS,
    FIXED_NOW_MS, OFFBOARDING_GRACE_MS, ONE_DAY_MS,
};
