//! `e2e-dsr` — R3-3 end-to-end harness for the Data Subject Rights
//! customer journey against the in-memory CoreLink stack.
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter R3 phase scope, this
//! crate is **NOT a production component** — it is a test-only harness
//! that wires the WI-S11-001 `corelink-dsr` endpoint + the WI-S11-002
//! `corelink-privacy-erasure-worker` cross-backend orchestrator
//! (12-deep fanout: 8 effective + 4 pseudonymized) into a single
//! deterministic pipeline so the full customer journey
//!
//! ```text
//! tenant DSR submit (Access / Portability / Rectification /
//!                    Erasure / Restriction / Objection)
//!   → WebAuthn MFA step-up stub (destructive arms only)
//!     → JWT receipt issued + sla_deadline_ms returned
//!       → status endpoint (Pending)
//!         → erasure worker drains 12 backends (Started)
//!           → 24h verification sweep (VerifiedComplete / Partial)
//!             → BLAKE3-signed canonical report
//!               → R2 evidence-dsr signed URL stub (24h TTL)
//! ```
//!
//! can be exercised in a single `cargo test -p e2e-dsr` run without any
//! network IO. The harness also exposes the
//! [`policy::TenantPolicyLedger`] for the canonical Restriction +
//! Objection arms (no production crate ships these as own modules per
//! the WI-S11-001 split — Restriction/Objection are policy-only DSR
//! arms whose downstream effect is "future writes return 403").
//!
//! # Charter constraints honoured
//!
//! - `#![forbid(unsafe_code)]` and `#[non_exhaustive]` are preserved on
//!   every consumed type (only canonical constructors are used).
//! - No `tokio` in src.
//! - All identifiers are deterministic functions of the canonical
//!   tenant slug so the harness is reproducible across runs.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod helpers;
pub mod policy;
pub mod r2;

pub use helpers::{
    canonical_dsr_for, canonical_dsr_with_request_id, canonical_erasure_for,
    canonical_mfa_token, make_test_tenant, poll_dsr_status, seed_backends_for, setup_test_env,
    verify_audit_chain, ExpectedDsrAuditEvent, TenantBundle, TestDsrEnv,
};
pub use policy::{PolicyDecision, RestrictionFlag, TenantPolicyLedger};
pub use r2::{InMemoryR2EvidenceClient, R2EvidenceError, R2SignedUrl};

/// Canonical wall-clock pin (Unix epoch ms) for the deterministic
/// harness. Mirrors `e2e-signup-flow::setup_test_ledgers` clock so
/// cross-harness regression tests can share fixtures.
pub const TEST_NOW_MS: u64 = 1_700_000_000_000;

/// Canonical signed-URL TTL (24h per WI-S11-002 §1) used by the in-
/// memory R2 evidence-dsr stub.
pub const TEST_SIGNED_URL_TTL_MS: u64 = 24 * 3_600_000;
