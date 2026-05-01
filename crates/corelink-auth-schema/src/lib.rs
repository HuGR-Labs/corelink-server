//! `corelink-auth-schema` — Neon Postgres auth domain schema (WI-S03-005).
//!
//! # What this crate ships
//!
//! 1. The canonical SQL artifact `migrations/002_auth_tables.sql`, embedded
//!    via [`MIGRATION_002_AUTH_TABLES`] so production code can pass the
//!    DDL to `psql` / `sqlx::migrate!` / a Worker startup hook without
//!    re-reading the file from disk.
//! 2. A **host-side in-memory schema simulator** ([`AuthSchema`]) that
//!    enforces the load-bearing invariants of the migration:
//!    UNIQUE constraints on `pat.token_hash`, `pat.token_id`,
//!    `user_account.email_hash`, `user_account.clerk_user_id`,
//!    `tenant.slug`, the `(pat_id, revoked_at)` revocation idempotency
//!    pair, and the cross-tenant isolation envelope enforced by RLS in
//!    production. The simulator is purposefully ORM-free — its sole
//!    consumers are property tests in this crate.
//! 3. The [`rls`] helper module documenting the canonical
//!    `with_tenant_ctx` transactional pattern so the application layer
//!    cannot forget to call `set_config('app.current_tenant', …, true)`
//!    before touching `pat` / `tenant` / `membership` / `revocation_log`.
//!
//! # Why simulator instead of live Postgres tests
//!
//! S-03 lands without Neon credentials wired into CI (no remote +
//! Cloudflare Workers + Neon staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The simulator covers
//! the algorithmic invariants that a schema bug would expose
//! (cross-tenant key collisions, UNIQUE-constraint regressions,
//! revocation idempotency); the live-Postgres roundtrip tests
//! (`tests/encrypted_columns.rs`, `tests/rls_policies.sql`) are wired
//! to the migration string and will run when the staging-Neon URL
//! lands. WI-S03-005 §10 records this split.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - The simulator is **not** a SQL parser. It implements a hand-coded
//!   subset corresponding to the seven auth tables and reports
//!   structured [`SimError`] errors when an invariant fires.

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (Postgres 16+ / Neon).
///
/// The exact bytes ship to production via the migration runner. The
/// simulator does not parse this string; the algorithmic invariants are
/// re-implemented directly so test failures are easy to triage.
pub const MIGRATION_002_AUTH_TABLES: &str =
    include_str!("../../../migrations/002_auth_tables.sql");

pub mod email_hash;
pub mod pseudonymize;
pub mod rls;
pub mod sim;

pub use email_hash::{compute_email_hash, derive_email_hash_key, EmailHashKey, EMAIL_HASH_LEN};
pub use pseudonymize::{pseudonymize_account_id, pseudonymize_user_id};
pub use sim::{
    AccountRow, AuthSchema, MembershipRow, PatRow, RevocationLogRow, RevocationReason, Role,
    SimError, TenantRow, TenantTier, UserAccountRow,
};

/// Returns the canonical schema version recorded by migration 002.
///
/// The application startup compatibility check compares
/// `MAX(schema_version.version)` against this constant.
pub const fn schema_version() -> u32 {
    2
}
