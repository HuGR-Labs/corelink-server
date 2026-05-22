//! D1 auth schema + RLS WITH CHECK enforcement — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-auth-schema`. The
//! actual implementation lives in `crates/corelink-auth-schema/`
//! (Stage 1 Stream B sub-step B.3 Option-A aggregator pattern).
//!
//! Charter: `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` (CRITICAL) — RLS WITH
//! CHECK on every txn is preserved by reference (the absorbed crate's
//! `tenant_isolation` module enforces it; not touched by this
//! aggregator).

pub use corelink_auth_schema::*;
