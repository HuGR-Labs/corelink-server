//! `corelink-openapi` — OpenAPI 3.1 contract surface for the CoreLink
//! non-REAPI HTTP API.
//!
//! # What this crate ships
//!
//! The canonical contract document lives at the workspace root as
//! [`openapi/corelink-v1.yaml`](../../../openapi/corelink-v1.yaml).
//! A sibling JSON encoding (`corelink-v1.json`) is generated from the
//! YAML by `scripts/openapi_sync.py` and validated in CI (see
//! `.github/workflows/openapi-validate.yml`).
//!
//! This crate ships:
//!
//! 1. [`SPEC_VERSION`] — the contract version string baked at compile
//!    time.
//! 2. [`SPEC_YAML`] — the canonical YAML payload embedded via
//!    `include_str!` so runtime consumers (admin UI, docs site SSR,
//!    SDK generators) can resolve the contract without a filesystem
//!    lookup.
//! 3. [`SPEC_JSON`] — the JSON encoding likewise embedded.
//! 4. [`paths`] — a typed mirror of the canonical path constants so
//!    handler crates can avoid hard-coding the string literals (and
//!    drift between crates and the spec is caught by the integration
//!    test in `tests/spec_roundtrip.rs`).
//!
//! # Why hand-authored YAML rather than `utoipa`
//!
//! The customer / privacy / admin handlers live across ~10 separate
//! crates that target both native (apps/server) and wasm32
//! (cf-bindings) — `utoipa`'s macro surface attaches to handler
//! function signatures, which are only present in the wasm-only CF
//! Worker entry crates (deferred to PRR ship gate). Hand-authored
//! YAML with a roundtrip test is the canonical pattern that survives
//! the trait-abstraction-defer charter without coupling the contract
//! to the not-yet-wired binding surface.
//!
//! When the CF Worker entry crates land at PRR, this crate is the
//! natural home for the matching `utoipa::OpenApi` derives — the YAML
//! becomes the regression oracle that the macro output must match.

#![forbid(unsafe_code)]

/// Embedded canonical OpenAPI 3.1 YAML document.
pub const SPEC_YAML: &str = include_str!("../../../openapi/corelink-v1.yaml");

/// Embedded canonical OpenAPI 3.1 JSON document (generated from
/// [`SPEC_YAML`] by `scripts/openapi_sync.py`).
pub const SPEC_JSON: &str = include_str!("../../../openapi/corelink-v1.json");

/// Contract major-version string. Matches the `info.version` field of
/// the canonical spec.
pub const SPEC_VERSION: &str = "v1";

/// Canonical route constants. Keep in sync with `openapi/corelink-v1.yaml`
/// (the spec_roundtrip test asserts that every value below appears as a
/// path key in the parsed spec).
pub mod paths {
    /// Customer signup orchestration.
    pub const SIGNUP: &str = "/v1/signup";
    /// DPA click-through acceptance.
    pub const DPA_ACCEPT: &str = "/v1/dpa/accept";
    /// DPA re-acceptance (30d grace window).
    pub const DPA_RE_ACCEPT: &str = "/v1/dpa/re-accept";
    /// Tier selection + Stripe Checkout handoff.
    pub const TIER_SELECT: &str = "/v1/onboarding/tier-select";
    /// PAT collection root (POST issue / GET list).
    pub const PATS: &str = "/v1/pats";
    /// PAT item (DELETE revoke).
    pub const PATS_ITEM: &str = "/v1/pats/{pat_id}";
    /// Stripe webhook ingestion.
    pub const STRIPE_WEBHOOK: &str = "/v1/billing/stripe-webhook";
    /// DSR submit (6 actions).
    pub const DSR_SUBMIT: &str = "/v1/privacy/dsr/{action}";
    /// DSR status poll.
    pub const DSR_STATUS: &str = "/v1/privacy/dsr/{request_id}/status";
    /// Consent grant / revoke per canonical purpose.
    pub const CONSENT_ITEM: &str = "/v1/consent/{purpose}";
    /// Consent history list.
    pub const CONSENT_HISTORY: &str = "/v1/consent";
    /// Public stateless grant verify.
    pub const CONSENT_VERIFY: &str = "/v1/consent/verify";
    /// Public stateless revocation verify.
    pub const CONSENT_REVOCATION_VERIFY: &str = "/v1/consent/revocation/verify";
    /// Admin op submission (dual-approval).
    pub const ADMIN_OPS: &str = "/v1/admin/ops";
    /// Admin op detail.
    pub const ADMIN_OPS_ITEM: &str = "/v1/admin/ops/{op_id}";
    /// Admin op approve.
    pub const ADMIN_OPS_APPROVE: &str = "/v1/admin/ops/{op_id}/approve";
    /// Admin op reject.
    pub const ADMIN_OPS_REJECT: &str = "/v1/admin/ops/{op_id}/reject";
    /// Audit-event search.
    pub const ADMIN_AUDIT_EVENTS: &str = "/v1/admin/audit/events";
    /// Tenant search.
    pub const ADMIN_TENANTS: &str = "/v1/admin/tenants";
    /// Enterprise sales inquiry.
    pub const ENTERPRISE_INQUIRE: &str = "/v1/enterprise/inquire";
    /// Current-user profile.
    pub const USERS_ME: &str = "/v1/users/me";
    /// Canonical data-category list.
    pub const DATA_CATEGORIES: &str = "/v1/data-categories";
    /// Liveness probe.
    pub const HEALTH: &str = "/api/health";
    /// Browser CSP-report intake.
    pub const CSP_REPORT: &str = "/api/csp-report";

    // -------------------------------------------------------------
    // SERVER-ONLY paths reconciled 2026-05-27 — see
    // specs/_audits/2026-05-27-openapi-reconciliation-seal.md for
    // the per-route classification.
    // -------------------------------------------------------------

    /// Pilot-programme signup token redemption (axum-mounted today).
    pub const SIGNUP_PILOT: &str = "/v1/signup/pilot/{token}";
    /// Admin-plane read (typed resource fetch).
    pub const ADMIN_READ: &str = "/v1/admin/read/{resource}";
    /// Admin-plane mutate (dual-approval-gated; wire-up demo path).
    pub const ADMIN_MUTATE: &str = "/v1/admin/mutate";
    /// Pilot tenant list (axum-mounted; replaces shell script).
    pub const ADMIN_PILOTS: &str = "/v1/admin/pilots";
    /// Grant pilot tier (NEW → ACTIVE state transition).
    pub const ADMIN_PILOTS_GRANT_TIER: &str = "/v1/admin/pilots/{tenant_id}/grant-tier";
    /// Run the 24h pilot activation check-in.
    pub const ADMIN_PILOTS_CHECKIN: &str = "/v1/admin/pilots/{tenant_id}/checkin";
    /// Bulk audit-chain export from R2 (per-tenant window).
    pub const AUDIT_EXPORT: &str = "/v1/audit/export";
    /// Audit-event count aggregates over a window.
    pub const AUDIT_ANALYTICS_EVENT_COUNT: &str = "/v1/audit/analytics/event-count";
    /// Audit-event timeline bucketed at a chosen granularity.
    pub const AUDIT_ANALYTICS_TIMELINE: &str = "/v1/audit/analytics/timeline";
    /// CAS read (REAPI HTTP fake — container surface).
    pub const CAS_READ_TENANT: &str = "/v1/cas/{tenant}/{hash}";
    /// CAS read (REAPI binary — `corelink-reapi`).
    pub const CAS_READ_DIGEST: &str = "/v1/cas/{digest}";
    /// Action Cache lookup + update (REAPI HTTP fake).
    pub const AC_LOOKUP: &str = "/v1/ac/{tenant}/{action_digest}";

    /// Every canonical path constant. Used by the spec_roundtrip test
    /// to assert no constant drifts away from the YAML.
    pub const ALL: &[&str] = &[
        SIGNUP,
        DPA_ACCEPT,
        DPA_RE_ACCEPT,
        TIER_SELECT,
        PATS,
        PATS_ITEM,
        STRIPE_WEBHOOK,
        DSR_SUBMIT,
        DSR_STATUS,
        CONSENT_ITEM,
        CONSENT_HISTORY,
        CONSENT_VERIFY,
        CONSENT_REVOCATION_VERIFY,
        ADMIN_OPS,
        ADMIN_OPS_ITEM,
        ADMIN_OPS_APPROVE,
        ADMIN_OPS_REJECT,
        ADMIN_AUDIT_EVENTS,
        ADMIN_TENANTS,
        ENTERPRISE_INQUIRE,
        USERS_ME,
        DATA_CATEGORIES,
        HEALTH,
        CSP_REPORT,
        // SERVER-ONLY (reconciled 2026-05-27)
        SIGNUP_PILOT,
        ADMIN_READ,
        ADMIN_MUTATE,
        ADMIN_PILOTS,
        ADMIN_PILOTS_GRANT_TIER,
        ADMIN_PILOTS_CHECKIN,
        AUDIT_EXPORT,
        AUDIT_ANALYTICS_EVENT_COUNT,
        AUDIT_ANALYTICS_TIMELINE,
        CAS_READ_TENANT,
        CAS_READ_DIGEST,
        AC_LOOKUP,
    ];
}

/// Parse the embedded JSON spec into a [`serde_json::Value`]. Cheap
/// (one allocation tree per call); intended for ad-hoc tooling, not
/// hot paths.
///
/// # Errors
///
/// Returns the underlying [`serde_json::Error`] when the embedded JSON
/// fails to parse — this should be impossible at runtime because the
/// CI lint gate fails the build whenever the YAML/JSON pair drifts,
/// but the fallible signature lets the integration test assert it.
pub fn parse_json() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(SPEC_JSON)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "test module: assertion failures via expect() are the intended panic path"
)]
mod tests {
    use super::*;

    #[test]
    fn json_parses() {
        let v = parse_json().expect("embedded JSON must parse");
        assert!(v.is_object());
        let openapi = v
            .get("openapi")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        assert!(
            openapi.starts_with("3.1"),
            "expected OpenAPI 3.1.x, got {openapi}",
        );
    }

    #[test]
    fn every_path_constant_is_present_in_spec() {
        let v = parse_json().expect("embedded JSON must parse");
        let paths = v
            .get("paths")
            .and_then(serde_json::Value::as_object)
            .expect("paths object");
        for p in paths::ALL {
            assert!(
                paths.contains_key(*p),
                "path constant {p} missing from spec — drift between corelink-openapi::paths and openapi/corelink-v1.yaml",
            );
        }
    }

    #[test]
    fn spec_version_matches_info_version() {
        let v = parse_json().expect("embedded JSON must parse");
        let info_version = v
            .get("info")
            .and_then(|i| i.get("version"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        assert_eq!(info_version, SPEC_VERSION);
    }
}
