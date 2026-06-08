//! Cache-scope enforcement for the container cache surfaces.
//!
//! The Worker resolves the PAT's D1 scope string (e.g. `cas:rw`, `cas:r`,
//! `admin`) and forwards it as a SERVER-TRUSTED header
//! [`SCOPE_HEADER`] (`x-corelink-scope`), stripping any client-supplied
//! value first. The container's job is to ENFORCE it on the cache routes:
//! until now the D1 scope was written but never checked, so every PAT was
//! effectively unscoped. This module supplies the checkable form.
//!
//! # Grammar
//!
//! The scope string is a list of capability tokens separated by whitespace
//! and/or commas (both tolerated, in any mix). Each token is trimmed; empty
//! tokens are ignored. A token grants a cache capability when one of its
//! segments matches:
//!
//! - **write** (CAS PUT, AC update, Turbo PUT) requires a `cas:rw` /
//!   `read-write` token (or a future `cas:w`).
//! - **read** (CAS GET, AC lookup, Turbo GET) requires `cas:rw` / `read-write`
//!   OR `cas:r` / `read-only` (and, since write implies read, a `cas:w` token
//!   also grants read).
//!
//! # Two equivalent spellings
//!
//! The D1 `pat.scope` column is constrained to `('read-write','read-only',
//! 'admin')` (see `migrations/d1/0037`), and the production provisioning path
//! (`signup-worker/clerk.ts`) writes `read-write` — so `read-write` /
//! `read-only` are the canonical STORED forms. The colon grammar (`cas:rw` /
//! `cas:r` / `cas:w`) is the equivalent internal spelling used by the cache
//! surfaces' own `x-corelink-scope` plumbing. This module accepts BOTH so the
//! stored value and the internal spelling map to the same capability — no
//! destructive schema migration is needed to align them.
//!
//! # Fail-CLOSED
//!
//! An empty or missing scope grants NOTHING (both [`requires_cache_read`]
//! and [`requires_cache_write`] return `false`). `admin` is treated as a
//! superset that DOES grant cache rw; admin *route* authorization is a
//! separate internal-auth gate (`admin.rs`/`admin_pilot.rs`), not this module.
//!
//! # Current traffic
//!
//! Prod PATs carry `admin` (legacy) or `read-write` (self-serve) — both pass
//! read+write, so enforcement is a NO-OP for live traffic. This module
//! ESTABLISHES the gate so `read-only` / tiered tokens work later.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

/// The SERVER-TRUSTED scope header the Worker sets (singular `scope`).
///
/// The Worker strips any client-supplied value and replaces it with the
/// PAT's D1 scope string before forwarding, so the container may trust it.
pub const SCOPE_HEADER: &str = "x-corelink-scope";

/// Split a scope string into its capability tokens.
///
/// Tokens are separated by ASCII whitespace and/or commas (either, in any
/// mix); each token is trimmed and empty tokens are dropped. Panic-free.
fn tokens(scope: &str) -> impl Iterator<Item = &str> {
    scope
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

/// True when `scope` grants the cache **read** capability.
///
/// Granted by `cas:rw` / `read-write`, `cas:r` / `read-only`, `cas:w` (write
/// implies read), or `admin` (a superset that includes cache access).
/// Fail-CLOSED: empty / missing / no matching token ⇒ `false`.
#[must_use]
pub fn requires_cache_read(scope: &str) -> bool {
    tokens(scope).any(|t| {
        matches!(
            t,
            "cas:rw" | "cas:r" | "cas:w" | "read-write" | "read-only" | "admin"
        )
    })
}

/// True when `scope` grants the cache **write** capability.
///
/// Granted by `cas:rw` / `read-write`, `cas:w`, or `admin` (a superset that
/// includes cache access). NOT granted by `read-only` / `cas:r`. Fail-CLOSED:
/// empty / missing / no matching token ⇒ `false`.
///
/// `admin` grants cache rw so that legacy admin-scoped customer PATs keep
/// working alongside the `read-write` self-serve tokens. Admin *route*
/// authorization remains a separate internal-auth gate (see
/// `admin.rs`/`admin_pilot.rs`); this only governs the cache surface.
#[must_use]
pub fn requires_cache_write(scope: &str) -> bool {
    tokens(scope).any(|t| matches!(t, "cas:rw" | "cas:w" | "read-write" | "admin"))
}

/// Read the trusted [`SCOPE_HEADER`] value off the request parts, trimmed.
///
/// Returns `""` when the header is absent or non-UTF-8 (⇒ fail-CLOSED at the
/// capability checks above).
fn scope_from_parts(parts: &Parts) -> &str {
    parts
        .headers
        .get(SCOPE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("")
}

/// Axum 0.7 extractor exposing the request's cache capabilities.
///
/// Construction never fails (it is infallible by design — the *capability*
/// is what gates the route, via [`CacheScope::can_read`] /
/// [`CacheScope::can_write`], so handlers stay in control of the 403 body).
/// A missing / empty / non-cache scope yields a `CacheScope` that grants
/// nothing (fail-CLOSED).
#[derive(Debug, Clone, Copy)]
pub struct CacheScope {
    can_read: bool,
    can_write: bool,
}

impl CacheScope {
    /// Build a [`CacheScope`] from a raw scope string.
    #[must_use]
    pub fn from_scope_str(scope: &str) -> Self {
        Self {
            can_read: requires_cache_read(scope),
            can_write: requires_cache_write(scope),
        }
    }

    /// Whether this scope grants the cache read capability.
    #[must_use]
    pub fn can_read(self) -> bool {
        self.can_read
    }

    /// Whether this scope grants the cache write capability.
    #[must_use]
    pub fn can_write(self) -> bool {
        self.can_write
    }
}

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for CacheScope {
    // Infallible: the capability is enforced at the route, not at extraction,
    // so a scope-less request still constructs a (grant-nothing) CacheScope.
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::from_scope_str(scope_from_parts(parts)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn cas_rw_grants_read_and_write() {
        assert!(requires_cache_read("cas:rw"));
        assert!(requires_cache_write("cas:rw"));
    }

    #[test]
    fn cas_r_grants_read_only() {
        assert!(requires_cache_read("cas:r"));
        assert!(!requires_cache_write("cas:r"));
    }

    #[test]
    fn cas_w_grants_write_and_read() {
        // Forward-design: a write-only token still implies read.
        assert!(requires_cache_write("cas:w"));
        assert!(requires_cache_read("cas:w"));
    }

    #[test]
    fn read_write_is_equivalent_to_cas_rw() {
        // `read-write` is the canonical STORED form (D1 pat.scope CHECK +
        // signup-worker provisioning). It must grant read AND write, exactly
        // like `cas:rw` — this is what unblocks real self-serve signups.
        assert!(requires_cache_read("read-write"));
        assert!(requires_cache_write("read-write"));
    }

    #[test]
    fn read_only_grants_read_not_write() {
        // `read-only` is the stored equivalent of `cas:r`.
        assert!(requires_cache_read("read-only"));
        assert!(!requires_cache_write("read-only"));
    }

    #[test]
    fn empty_scope_grants_nothing() {
        assert!(!requires_cache_read(""));
        assert!(!requires_cache_write(""));
    }

    #[test]
    fn whitespace_only_scope_grants_nothing() {
        assert!(!requires_cache_read("   \t "));
        assert!(!requires_cache_write("   \t "));
    }

    #[test]
    fn admin_grants_cache_as_superset() {
        // `admin` is a superset that includes cache rw — so PR-4 enforcement is
        // decoupled from the prod scope back-fill (admin->cas:rw). Admin *route*
        // authorization remains a separate internal-auth gate.
        assert!(requires_cache_read("admin"));
        assert!(requires_cache_write("admin"));
    }

    #[test]
    fn comma_separated_list_is_tolerated() {
        assert!(requires_cache_read("admin,cas:r"));
        assert!(requires_cache_write("admin,cas:r")); // admin in the list grants write
        assert!(!requires_cache_write("cas:r,nope")); // read-only list (no admin/rw) → no write
        assert!(requires_cache_write("admin,cas:rw"));
    }

    #[test]
    fn whitespace_separated_list_is_tolerated() {
        assert!(requires_cache_write("admin cas:rw"));
        assert!(requires_cache_read("admin cas:r"));
    }

    #[test]
    fn mixed_comma_and_whitespace_with_padding() {
        let scope = "  admin ,  cas:rw , billing:r ";
        assert!(requires_cache_read(scope));
        assert!(requires_cache_write(scope));
    }

    #[test]
    fn unknown_token_does_not_match_substring() {
        // `xcas:rw` / `cas:rwx` must NOT be treated as `cas:rw` — exact
        // token match only, not substring.
        assert!(!requires_cache_write("xcas:rw"));
        assert!(!requires_cache_read("xcas:rw"));
        assert!(!requires_cache_write("cas:rwx"));
    }

    #[test]
    fn cache_scope_struct_mirrors_free_functions() {
        let rw = CacheScope::from_scope_str("cas:rw");
        assert!(rw.can_read());
        assert!(rw.can_write());
        let r = CacheScope::from_scope_str("cas:r");
        assert!(r.can_read());
        assert!(!r.can_write());
        let none = CacheScope::from_scope_str("");
        assert!(!none.can_read());
        assert!(!none.can_write());
    }

    /// Build a `Parts` carrying the given `x-corelink-scope` header value.
    fn parts_with_scope(value: &str) -> Parts {
        axum::http::Request::builder()
            .header(SCOPE_HEADER, value)
            .body(axum::body::Body::empty())
            .unwrap()
            .into_parts()
            .0
    }

    /// Build a `Parts` with no `x-corelink-scope` header at all.
    fn parts_without_scope() -> Parts {
        axum::http::Request::builder()
            .body(axum::body::Body::empty())
            .unwrap()
            .into_parts()
            .0
    }

    #[tokio::test]
    async fn extractor_reads_rw_scope() {
        let mut parts = parts_with_scope("cas:rw");
        let scope = CacheScope::from_request_parts(&mut parts, &())
            .await
            .expect("infallible");
        assert!(scope.can_read());
        assert!(scope.can_write());
    }

    #[tokio::test]
    async fn extractor_reads_read_only_scope() {
        let mut parts = parts_with_scope("cas:r");
        let scope = CacheScope::from_request_parts(&mut parts, &())
            .await
            .expect("infallible");
        assert!(scope.can_read());
        assert!(!scope.can_write());
    }

    #[tokio::test]
    async fn extractor_missing_header_grants_nothing() {
        let mut parts = parts_without_scope();
        let scope = CacheScope::from_request_parts(&mut parts, &())
            .await
            .expect("infallible");
        assert!(!scope.can_read());
        assert!(!scope.can_write());
    }
}
