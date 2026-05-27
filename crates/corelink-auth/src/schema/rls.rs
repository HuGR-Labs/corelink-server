//! Documentation-only module describing the canonical RLS lifecycle
//! that the application layer **must** follow when querying any
//! tenant-scoped auth table.
//!
//! # Mandatory pattern
//!
//! `SET LOCAL` is **transaction-scoped** in Postgres — it resets at the
//! end of the wrapping transaction. The CF Workers + sqlx pool path
//! therefore looks like:
//!
//! ```ignore
//! let mut tx = pool.begin().await?;
//! sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
//!     .bind(tenant_id.to_string())
//!     .execute(&mut *tx)
//!     .await?;
//!
//! // … queries here run with RLS scoped to `tenant_id` …
//!
//! tx.commit().await?;
//! ```
//!
//! Forgetting the `SET LOCAL` step does **not** silently expose
//! cross-tenant rows: the policies created by `migrations/002_auth_tables.sql`
//! evaluate `current_setting('app.current_tenant', true)::uuid` against
//! `NULL`, which is `false` for `=`, so the row set is empty.
//!
//! # Why no helper macro is exported
//!
//! The application surface (CF Worker handler crate) is owned by
//! WI-S03-007 forward. Exporting a Rust macro from this crate today
//! would couple it to a specific async runtime / pool type before the
//! consumer crate exists. Instead, the canonical pattern is documented
//! here verbatim and consumers re-export their own thin wrapper at
//! the call site.

/// Opaque marker emitted by tests to assert the RLS context-setting
/// invariant. Production callers do **not** import this type — it
/// exists so the simulator's property tests can guard against
/// regressions where a query path forgets to bind `tenant_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TenantContextRequired;
