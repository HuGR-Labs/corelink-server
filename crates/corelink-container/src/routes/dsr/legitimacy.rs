//! Real D1-backed [`DsrLegitimacyStore`] over the canonical
//! `dsr_requested` table (`migrations/d1/0069_dsr_requested.sql`) —
//! rt-nuclear #18/#19 GDPR mass-erase authz gate.
//!
//! The pure `corelink-privacy-erasure-worker` crate ships only the
//! in-memory legitimacy stores; this is the durable transport. A
//! legitimate erasure ALWAYS has a `dsr_requested` row matching
//! `(dsr_id, tenant_id)` (written by the Clerk `user.deleted` path with a
//! D1-authenticated `tenant_id`); a forged body-asserted `(dsr_id,
//! tenant_id)` does not — so binding the erase to this row defeats the
//! shared-internal-key mass-erase primitive.
//!
//! Fail-CLOSED: the trait surface returns `Err(_)` on a D1 fault, which
//! the orchestrator maps to a `Rejected` decision (erasure is
//! irreversible — ambiguous legitimacy must DENY).

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use corelink_privacy_erasure_worker::legitimacy::{DsrLegitimacyError, DsrLegitimacyStore};

use super::d1util::d1_query_blocking;
use crate::storage::d1_http::D1HttpClient;

/// Durable legitimacy store backed by the D1 `dsr_requested` table.
///
/// Shared by BOTH erase legs: the DSR mass-erase orchestrator (this module)
/// and the per-blob CAS-erase route (`crate::routes::cas_erase`). Single
/// source of truth for "what makes an erasure legitimate" so the two legs
/// can never drift apart (r34 #8/#9).
pub(crate) struct D1DsrLegitimacyStore {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1DsrLegitimacyStore {
    // Never surface the inner client's Debug — it holds the CF API token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1DsrLegitimacyStore")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl D1DsrLegitimacyStore {
    /// Construct over a shared [`D1HttpClient`].
    pub(crate) fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Build from env (`StorageEnv`). Returns `None` when the D1 env is
    /// absent or the client cannot be built, so callers fail-CLOSED (do not
    /// mount the erase route). Used by the per-blob CAS-erase route
    /// (`crate::routes::cas_erase::build_state_from_env`).
    #[must_use]
    pub(crate) fn from_env() -> Option<Self> {
        let env = crate::storage::StorageEnv::from_env()?;
        match D1HttpClient::new(&env) {
            Ok(c) => Some(Self::new(Arc::new(c))),
            Err(e) => {
                tracing::warn!(error = %e, "cas_erase: D1 client build failed (legitimacy)");
                None
            }
        }
    }
}

impl DsrLegitimacyStore for D1DsrLegitimacyStore {
    fn is_requested(&self, dsr_id: Uuid, tenant_id: Uuid) -> Result<bool, DsrLegitimacyError> {
        // A legitimate erasure has a `dsr_requested` row for THIS tenant
        // with a live status. Bind on BOTH dsr_id AND tenant_id so a
        // forged tenant_id (same dsr_id) cannot satisfy the gate.
        let sql = "SELECT 1 AS one FROM dsr_requested \
             WHERE dsr_id = ?1 AND tenant_id = ?2 \
               AND status IN ('requested', 'verified') LIMIT 1";
        let rows = d1_query_blocking(
            &self.d1,
            sql,
            vec![json!(dsr_id.to_string()), json!(tenant_id.to_string())],
        )
        .map_err(DsrLegitimacyError::Backend)?;
        Ok(!rows.is_empty())
    }
}
