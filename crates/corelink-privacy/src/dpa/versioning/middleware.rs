//! `PAT-DEGRADE-001` read-only middleware primitive.
//!
//! Pure function: `(tenant_state, method, path) -> GateDecision`.
//! Production wiring (axum / tower / CF Worker `fetch` handler)
//! drops in a thin adapter calling [`ReadOnlyDegradeGate::evaluate`].
//!
//! # Invariant
//!
//! For tenants with `is_grace_expired(now) == true`:
//! - **All write methods** (POST / PUT / PATCH / DELETE) on `/v1/*`
//!   are **denied** with `403 dpa_re_acceptance_required` ...
//! - **Except** `/v1/dpa/re-accept` (self-recovery escape hatch).
//! - **Reads** (GET / HEAD / OPTIONS) **always allowed** — consumer
//!   protection: data must remain accessible to the Controller.
//!
//! For tenants with `is_grace_expired(now) == false`: every request
//! is allowed.
//!
//! This invariant is pinned by `prop_read_only_enforcement`.

use super::schema::{TenantDpaState, UnixSeconds};

/// HTTP methods relevant to the gate.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    /// GET request — read.
    Get,
    /// HEAD request — read.
    Head,
    /// OPTIONS request — read (CORS preflight).
    Options,
    /// POST request — write.
    Post,
    /// PUT request — write.
    Put,
    /// PATCH request — write.
    Patch,
    /// DELETE request — write.
    Delete,
}

impl HttpMethod {
    /// True for HTTP methods that mutate server state.
    #[must_use]
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            Self::Post | Self::Put | Self::Patch | Self::Delete
        )
    }
}

/// Decision returned by the gate.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision {
    /// Request proceeds.
    Allow,
    /// Request denied — tenant must re-accept the latest DPA.
    /// Caller maps to `403 dpa_re_acceptance_required`.
    DenyReAcceptanceRequired,
}

/// Canonical re-acceptance path. Hard-coded canonical: changing this
/// changes the wire contract — guard with a regression test.
pub const RE_ACCEPT_PATH: &str = "/v1/dpa/re-accept";

/// Pure read-only gate.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReadOnlyDegradeGate;

impl ReadOnlyDegradeGate {
    /// Evaluate the gate for a single request.
    ///
    /// `path` is normalised by the caller (no trailing slash, no
    /// query string).
    #[must_use]
    pub fn evaluate(
        tenant: &TenantDpaState,
        method: HttpMethod,
        path: &str,
        now: UnixSeconds,
    ) -> GateDecision {
        // Non-`/v1/*` paths are out of scope (the gate is only mounted
        // on `/v1/*` in production routing). Default to allow so the
        // primitive is safe to misuse.
        if !path.starts_with("/v1/") && path != "/v1" {
            return GateDecision::Allow;
        }
        // Reads always allowed.
        if !method.is_write() {
            return GateDecision::Allow;
        }
        // Writes always allowed if grace has not expired.
        if !tenant.is_grace_expired(now) {
            return GateDecision::Allow;
        }
        // Self-recovery escape hatch.
        if path == RE_ACCEPT_PATH {
            return GateDecision::Allow;
        }
        GateDecision::DenyReAcceptanceRequired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::schema::GRACE_PERIOD_SECONDS;
    use super::super::version::SemverVersion;
    use uuid::Uuid;

    fn expired_tenant() -> TenantDpaState {
        TenantDpaState {
            tenant_id: Uuid::nil(),
            current_dpa_version: SemverVersion::new(1, 0, 0),
            grace_expires_at: Some(100),
            re_acceptance_pending: true,
        }
    }

    fn writable_tenant() -> TenantDpaState {
        TenantDpaState {
            tenant_id: Uuid::nil(),
            current_dpa_version: SemverVersion::new(2, 0, 0),
            grace_expires_at: Some(100 + GRACE_PERIOD_SECONDS),
            re_acceptance_pending: true,
        }
    }

    #[test]
    fn read_allowed_when_expired() {
        let d = ReadOnlyDegradeGate::evaluate(
            &expired_tenant(),
            HttpMethod::Get,
            "/v1/objects/abc",
            200,
        );
        assert_eq!(d, GateDecision::Allow);
    }

    #[test]
    fn write_denied_when_expired() {
        let d = ReadOnlyDegradeGate::evaluate(
            &expired_tenant(),
            HttpMethod::Put,
            "/v1/objects/abc",
            200,
        );
        assert_eq!(d, GateDecision::DenyReAcceptanceRequired);
    }

    #[test]
    fn re_accept_endpoint_always_allowed_even_when_expired() {
        let d = ReadOnlyDegradeGate::evaluate(
            &expired_tenant(),
            HttpMethod::Post,
            RE_ACCEPT_PATH,
            200,
        );
        assert_eq!(d, GateDecision::Allow);
    }

    #[test]
    fn write_allowed_within_grace() {
        let d = ReadOnlyDegradeGate::evaluate(
            &writable_tenant(),
            HttpMethod::Post,
            "/v1/objects/abc",
            200,
        );
        assert_eq!(d, GateDecision::Allow);
    }

    #[test]
    fn non_v1_path_always_allowed() {
        let d = ReadOnlyDegradeGate::evaluate(
            &expired_tenant(),
            HttpMethod::Post,
            "/internal/health",
            200,
        );
        assert_eq!(d, GateDecision::Allow);
    }
}
