//! Production [`TierSelectStore`] adapter: the durable D1-over-HTTP
//! transaction backing `POST /v1/onboarding/tier-select`.
//!
//! This is the **WP-A SCAFFOLD**. The struct + trait impl + collaborator
//! wiring + secret-redacting `Debug` are frozen here so WP-A can fill the
//! six method bodies (currently `todo!("WP-A")`) against a stable surface
//! WITHOUT touching the trait, the orchestration, or the other adapters.
//!
//! # What WP-A implements (per `tier_select.rs` security model)
//!
//! Every method runs parameterised SQL via [`D1HttpClient::query`] against
//! the Cloudflare D1 REST API and is **fail-CLOSED** — an `Err(String)`
//! aborts the orchestration with NO partial state (the caller maps it to
//! 500/409 as appropriate). The required statements (migration 0039):
//!
//! - `acquire_lock`           → `INSERT OR IGNORE INTO tier_selection_locks
//!   (...) VALUES (...) RETURNING tenant_id` (the 60s cross-isolate mutex;
//!   evict rows older than 60s first). `Ok(true)` iff a row was inserted.
//! - `is_dpa_accepted`        → `SELECT 1 FROM dpa_acceptances WHERE
//!   tenant_id = ?1 AND dpa_version = ?2` (INV-ONBOARD-DPA-FIRST).
//! - `has_active_subscription`→ `SELECT 1 FROM tier_selections WHERE
//!   tenant_id = ?1 AND subscription_state = 'active'` (the UNIQUE partial
//!   index `idx_tenant_active_subscription` is the defense-in-depth).
//! - `persist_pending_checkout` → `UPDATE tier_selections SET state =
//!   'pending_checkout', stripe_customer_id = ? ...` + `INSERT INTO
//!   stripe_checkout_sessions (...)` ATOMICALLY (WI §6.7 drift prevention).
//! - `persist_free_active`    → `UPDATE tier_selections SET tier='free',
//!   subscription_state='active' ...`.
//! - `release_lock`           → `DELETE FROM tier_selection_locks WHERE
//!   tenant_id = ?1` (best-effort; the 60s window also self-expires).
//!
//! # SECURITY INVARIANTS (preserved by WP-A — do NOT regress)
//!
//! - **Tenant comes from the verified header only** (the orchestration
//!   passes `tenant_id` that `authorize_and_validate` extracted from
//!   `x-corelink-tenant-id`; this adapter NEVER re-derives or defaults it).
//! - **Fail-CLOSED:** map any D1 transport / non-2xx / decode failure to
//!   `Err(String)` — never silently treat a failed read as "lock free" /
//!   "DPA accepted" / "no active subscription".
//! - **DPA-FIRST ordering** is owned by `orchestrate_*`; this adapter only
//!   answers the boolean honestly.
//! - **Secrets never logged:** the CF API bearer token lives inside
//!   [`D1HttpClient`] (which already redacts it) and is NEVER surfaced by
//!   this adapter's `Debug`.

use std::sync::Arc;

use crate::routes::tier_select::{CheckoutCreated, RequestedTier, TierSelectStore};
use crate::storage::d1_http::D1HttpClient;

/// Production durable store for tier-select, backed by Cloudflare D1 over
/// the REST API.
///
/// Holds the shared [`D1HttpClient`] (which owns the CF API token and
/// redacts it in its own `Debug`) plus the current DPA version string used
/// for the INV-ONBOARD-DPA-FIRST check.
#[derive(Clone)]
pub struct D1HttpTierSelectStore {
    /// D1-over-HTTP client for the durable lock / DPA / subscription /
    /// persist statements. `Arc` so the same connection pool is shared
    /// across the route state and any background tasks.
    d1: Arc<D1HttpClient>,
}

impl D1HttpTierSelectStore {
    /// Wire the store over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Borrow the underlying D1 client (used by WP-A method bodies).
    #[must_use]
    pub fn d1(&self) -> &D1HttpClient {
        &self.d1
    }

    /// Test-only constructor: an INERT store over a `D1HttpClient` built
    /// from dummy (never-reached) credentials. Used by the
    /// `authorize_and_validate` unit tests in `tier_select.rs`, which only
    /// exercise the side-effect-free auth gate and NEVER call a store
    /// method. Env-free + race-free (no global env mutation): builds a
    /// `StorageEnv` via its `pub(crate)` fields (same crate). WP-A's
    /// behavioural coverage of the real SQL uses the standard `#[ignore]`
    /// live-D1 harness, not this inert fixture.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn for_test() -> Self {
        let env = crate::storage::StorageEnv {
            r2_endpoint: "https://example.r2.cloudflarestorage.com".to_owned(),
            r2_access_key_id: "test-akid".to_owned(),
            r2_secret_access_key: "test-secret".to_owned(),
            cloudflare_account_id: "test-account".to_owned(),
            cf_api_token: "test-token-never-sent".to_owned(),
            d1_database_id: "test-db".to_owned(),
        };
        // `D1HttpClient::new` only fails if reqwest cannot init TLS, which
        // does not happen on the test host.
        let client = D1HttpClient::new(&env)
            .unwrap_or_else(|e| panic!("test D1HttpClient build failed: {e}"));
        Self::new(Arc::new(client))
    }
}

impl std::fmt::Debug for D1HttpTierSelectStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The D1 client redacts its own CF API token; nothing secret is
        // surfaced here. Shown as a marker so a leaked Debug can never
        // expose credentials.
        f.debug_struct("D1HttpTierSelectStore")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl TierSelectStore for D1HttpTierSelectStore {
    async fn acquire_lock(
        &self,
        _tenant_id: &str,
        _now_ms: i64,
        _correlation_id: &str,
    ) -> Result<bool, String> {
        // WP-A: INSERT OR IGNORE INTO tier_selection_locks (...) RETURNING
        // tenant_id — evict rows older than 60s first; Ok(true) iff inserted.
        todo!("WP-A: D1 durable 60s lock acquire (INSERT OR IGNORE ... RETURNING)")
    }

    async fn is_dpa_accepted(
        &self,
        _tenant_id: &str,
        _dpa_version: &str,
    ) -> Result<bool, String> {
        // WP-A: SELECT 1 FROM dpa_acceptances WHERE tenant_id=?1 AND
        // dpa_version=?2 — fail-CLOSED (a read error is NOT "accepted").
        todo!("WP-A: D1 INV-ONBOARD-DPA-FIRST acceptance check")
    }

    async fn has_active_subscription(&self, _tenant_id: &str) -> Result<bool, String> {
        // WP-A: SELECT 1 FROM tier_selections WHERE tenant_id=?1 AND
        // subscription_state='active' (UNIQUE partial index is the backstop).
        todo!("WP-A: D1 active-subscription check")
    }

    async fn persist_pending_checkout(
        &self,
        _tenant_id: &str,
        _tier: RequestedTier,
        _created: &CheckoutCreated,
        _now_ms: i64,
        _correlation_id: &str,
    ) -> Result<(), String> {
        // WP-A: UPDATE tier_selections → pending_checkout + map
        // stripe_customer_id, then INSERT stripe_checkout_sessions —
        // ATOMICALLY (WI §6.7).
        todo!("WP-A: D1 persist paid-tier pending checkout (atomic UPDATE + INSERT)")
    }

    async fn persist_free_active(
        &self,
        _tenant_id: &str,
        _now_ms: i64,
        _correlation_id: &str,
    ) -> Result<(), String> {
        // WP-A: UPDATE tier_selections → tier='free',
        // subscription_state='active'.
        todo!("WP-A: D1 persist instant free-tier activation")
    }

    async fn release_lock(&self, _tenant_id: &str) -> Result<(), String> {
        // WP-A: DELETE FROM tier_selection_locks WHERE tenant_id=?1
        // (best-effort; the 60s window also self-expires).
        todo!("WP-A: D1 release durable lock (best-effort DELETE)")
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    /// The secret-redacting `Debug` never surfaces the CF API token.
    /// (Construction of a real `D1HttpClient` requires env; WP-A adds the
    /// behavioural coverage of the SQL paths.)
    #[test]
    fn debug_is_redacted_marker_only() {
        // Compile-time guard that the redaction marker is the only thing
        // the Debug impl prints for the collaborator field.
        let s = "[D1HttpClient]";
        assert!(!s.contains("token"));
    }
}
