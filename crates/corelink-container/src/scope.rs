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

/// Classification of a self-serve key-mint scope request.
///
/// `Admin` is the privileged class (never grantable self-serve); `ReadWrite`
/// carries a cache-write capability; `ReadOnly` is the least-privilege default
/// (incl. the canonical `["cache:read"]` and an empty request).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestedScopeClass {
    /// Least privilege — read-only cache access.
    ReadOnly,
    /// Carries a cache-write capability.
    ReadWrite,
    /// Privileged (admin/owner) — not grantable via self-serve key creation.
    Admin,
}

/// Classify a requested-scope list with the EXACT-token grammar (case-insensitive,
/// whitespace/comma-tokenized, trimmed) — the **single source of truth** shared by
/// the self-serve mint escalation gate (`routes::customer::mint_requests_write`)
/// and the D1 scope persister (`customer_d1::map_requested_scopes`).
///
/// Returns `Err(token)` for any token that is not a recognized read / write /
/// admin scope. **Fail-CLOSED:** unknown grammar can never silently map to a
/// privilege — closing the substring-vs-exact-token divergence (rt-nuclear #15)
/// where `"writes"` skipped the exact-token gate yet a `s.contains("write")`
/// persister granted `read-write`.
///
/// # Errors
///
/// `Err(unrecognized_token)` when any token is outside the read/write/admin
/// vocabulary, so the caller can reject the mint rather than guess a privilege.
pub fn classify_requested_scopes(requested: &[String]) -> Result<RequestedScopeClass, String> {
    let mut admin = false;
    let mut write = false;
    for raw in requested {
        for t in tokens(raw) {
            match t.to_ascii_lowercase().as_str() {
                "admin" | "owner" => admin = true,
                "cas:rw" | "cas:w" | "read-write" | "cache:write" | "cache:rw" | "write" => {
                    write = true;
                }
                "cas:r" | "read-only" | "cache:read" => {}
                other => return Err(other.to_owned()),
            }
        }
    }
    Ok(if admin {
        RequestedScopeClass::Admin
    } else if write {
        RequestedScopeClass::ReadWrite
    } else {
        RequestedScopeClass::ReadOnly
    })
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

impl<S: Send + Sync> FromRequestParts<S> for CacheScope {
    // Infallible: the capability is enforced at the route, not at extraction,
    // so a scope-less request still constructs a (grant-nothing) CacheScope.
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::from_scope_str(scope_from_parts(parts)))
    }
}

/// The SERVER-TRUSTED header the Worker sets to mark a **narrowed runner-job
/// PAT** (cf-multitenant WP5a→WP5b). Present-and-equal-to-`"1"` ⇒ this request
/// carries a per-job credential enforced fail-CLOSED at the container cache
/// gate: no DELETE on CAS/AC, and (if [`RUNNER_JOB_AC_KEY_ALLOW_HEADER`] pins a
/// key) AC writes only to that exact key.
///
/// The Worker strips any client-supplied copy and re-sets it, so the container
/// may trust it — exactly like [`SCOPE_HEADER`] and
/// [`crate::byte_accounting::STORAGE_QUOTA_HEADER`]. ABSENT ⇒ a normal PAT with
/// NO behavior change.
pub const RUNNER_JOB_HEADER: &str = "x-corelink-runner-job";

/// The SERVER-TRUSTED header the Worker sets alongside [`RUNNER_JOB_HEADER`] to
/// pin the ONE AC key a runner-job PAT may write. Value semantics
/// (parsed into [`RunnerJob::ac_key_allowed`]):
/// - `"*"` (the launch default) → NO key restriction (deny-DELETE only);
/// - a BLAKE3 hex digest → the runner-job PAT may write ONLY that exact AC key;
/// - absent / empty → no key pinned (deny-DELETE only).
pub const RUNNER_JOB_AC_KEY_ALLOW_HEADER: &str = "x-corelink-ac-key-allow";

/// The SERVER-TRUSTED header the Worker sets alongside [`RUNNER_JOB_HEADER`] to
/// mark a **create-only (deny-overwrite)** runner-job PAT — the anti AC-squat
/// fast-follow. Present-and-equal-to-`"1"` (trimmed) ⇒ this runner-job cred's AC
/// writes are FIRST-WRITER-WINS: it may CREATE a new `(tenant, action_digest)`
/// entry but may NOT OVERWRITE an existing one (⇒ 409). It is **key-AGNOSTIC**
/// (applies to every key — orthogonal to [`RUNNER_JOB_AC_KEY_ALLOW_HEADER`]'s
/// exact-key pin) and tenant-scoped (the tenant is the edge-injected id). This is
/// the AC analog of the existing deny-DELETE narrowing, at the SAME chokepoint.
///
/// The Worker strips any client-supplied copy and re-sets it ONLY for a genuine
/// runner-job cred (parallel to [`RUNNER_JOB_HEADER`] / [`SCOPE_HEADER`]), so the
/// container may trust it. Any other value / absent ⇒ OFF (fail-SAFE false), and
/// a non-runner-job is NEVER create-only (see [`RunnerJob::ac_create_only`]).
pub const RUNNER_JOB_AC_CREATE_ONLY_HEADER: &str = "x-corelink-ac-create-only";

/// Server-trusted marker + AC-key restriction for a narrowed runner-job PAT.
///
/// Construction is infallible: absent [`RUNNER_JOB_HEADER`] yields a
/// non-runner-job value that is a NO-OP at every gate (fail-SAFE — a normal PAT
/// is never accidentally narrowed). The narrowing is enforced at the route via
/// [`RunnerJob::is_runner_job`] + [`RunnerJob::ac_key_allowed`] +
/// [`RunnerJob::ac_create_only`].
#[derive(Debug, Clone)]
pub struct RunnerJob {
    is_runner_job: bool,
    ac_key_allow: Option<String>,
    /// Raw parse of [`RUNNER_JOB_AC_CREATE_ONLY_HEADER`] (`true` only on exact
    /// `"1"`). Gated on [`Self::is_runner_job`] at the [`Self::ac_create_only`]
    /// accessor so a stray header without the runner-job marker is a NO-OP.
    ac_create_only: bool,
}

impl RunnerJob {
    /// Build from the request parts' [`RUNNER_JOB_HEADER`] +
    /// [`RUNNER_JOB_AC_KEY_ALLOW_HEADER`].
    ///
    /// A request is a runner-job ONLY when the marker is present and equals
    /// exactly `"1"` (trimmed). Any other value — including a present-but-empty
    /// or garbage marker — is treated as NOT a runner-job: the narrowing is an
    /// additive Worker-set signal, so an unrecognized marker means "the Worker
    /// did not narrow this request", not "narrow it harder".
    #[must_use]
    fn from_headers(parts: &Parts) -> Self {
        let marker = parts
            .headers
            .get(RUNNER_JOB_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim);
        let is_runner_job = marker == Some("1");
        let ac_key_allow = parts
            .headers
            .get(RUNNER_JOB_AC_KEY_ALLOW_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        // Create-only (deny-overwrite): present-and-exactly-`"1"` (trimmed) ⇒ on;
        // any other value / absent ⇒ off. Parsed EXACTLY like the runner-job
        // marker (the `"1"` truth), then gated on `is_runner_job` at the accessor.
        let ac_create_only = parts
            .headers
            .get(RUNNER_JOB_AC_CREATE_ONLY_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            == Some("1");
        Self {
            is_runner_job,
            ac_key_allow,
            ac_create_only,
        }
    }

    /// Whether this request carries a narrowed runner-job PAT (deny-DELETE +
    /// exact-key enforcement apply). Fail-SAFE: `false` unless the marker was
    /// present and exactly `"1"`.
    #[must_use]
    pub fn is_runner_job(&self) -> bool {
        self.is_runner_job
    }

    /// Whether a runner-job PAT is allowed to write the given AC `key`.
    ///
    /// Returns `true` (no restriction) when this is NOT a runner-job, when no
    /// key is pinned, or when the pinned key is the `"*"` wildcard (the launch
    /// default). When a concrete key is pinned, only an EXACT match is allowed
    /// (fail-CLOSED). DELETE is denied separately via [`Self::is_runner_job`].
    #[must_use]
    pub fn ac_key_allowed(&self, key: &str) -> bool {
        if !self.is_runner_job {
            return true;
        }
        match self.ac_key_allow.as_deref() {
            None | Some("*") => true,
            Some(allowed) => allowed == key,
        }
    }

    /// Whether this request carries a **create-only (deny-overwrite)** runner-job
    /// PAT — AC writes are first-writer-wins: a CREATE of a new
    /// `(tenant, action_digest)` entry is allowed, but any OVERWRITE of an
    /// existing entry is rejected (the route returns 409). Key-AGNOSTIC (applies
    /// to every key, orthogonal to [`Self::ac_key_allowed`]).
    ///
    /// Fail-SAFE: `false` unless this is a genuine runner-job (marker == `"1"`)
    /// AND [`RUNNER_JOB_AC_CREATE_ONLY_HEADER`] is present and exactly `"1"`. A
    /// non-runner-job is NEVER create-only — a stray create-only header without
    /// the runner-job marker is ignored (the Worker strips client copies and sets
    /// it only for a real runner-job cred, so this is defense-in-depth).
    #[must_use]
    pub fn ac_create_only(&self) -> bool {
        self.is_runner_job && self.ac_create_only
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RunnerJob {
    // Infallible: the narrowing is enforced at the route (like `CacheScope`),
    // so a non-runner-job request still constructs a (NO-OP) `RunnerJob`.
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::from_headers(parts))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn classify_requested_scopes_is_exact_token_and_fail_closed() {
        use RequestedScopeClass::{Admin, ReadOnly, ReadWrite};
        let one = |s: &str| vec![s.to_owned()];
        // Canonical self-serve inputs.
        assert_eq!(classify_requested_scopes(&one("cache:read")), Ok(ReadOnly));
        assert_eq!(
            classify_requested_scopes(&one("cache:write")),
            Ok(ReadWrite)
        );
        assert_eq!(classify_requested_scopes(&one("cas:rw")), Ok(ReadWrite));
        assert_eq!(classify_requested_scopes(&one("read-write")), Ok(ReadWrite));
        assert_eq!(classify_requested_scopes(&[]), Ok(ReadOnly)); // least privilege
        assert_eq!(classify_requested_scopes(&one("admin")), Ok(Admin));
        assert_eq!(classify_requested_scopes(&one("owner")), Ok(Admin));
        // Case-insensitive.
        assert_eq!(
            classify_requested_scopes(&one("CACHE:WRITE")),
            Ok(ReadWrite)
        );
        // rt-nuclear #15 REGRESSION: substring-y tokens that the old persister
        // mapped to read-write via `s.contains("write")` must NOT silently become
        // a privilege — they are unrecognized ⇒ Err (fail-CLOSED), never ReadWrite.
        for evil in ["writes", "cache:write-x", "my-write", "rewrite", "writable"] {
            assert!(
                classify_requested_scopes(&one(evil)).is_err(),
                "{evil:?} must be rejected, not silently classified",
            );
        }
        // A read token mixed with an unknown token still fails closed.
        assert!(classify_requested_scopes(&["cache:read".into(), "writes".into()]).is_err());
    }

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

    // ---- RunnerJob (cf-multitenant WP5b) --------------------------------

    /// Build `Parts` carrying an optional runner-job marker + AC-key-allow.
    fn parts_with_runner_job(marker: Option<&str>, key_allow: Option<&str>) -> Parts {
        let mut b = axum::http::Request::builder();
        if let Some(m) = marker {
            b = b.header(RUNNER_JOB_HEADER, m);
        }
        if let Some(k) = key_allow {
            b = b.header(RUNNER_JOB_AC_KEY_ALLOW_HEADER, k);
        }
        b.body(axum::body::Body::empty()).unwrap().into_parts().0
    }

    async fn extract_runner_job(mut parts: Parts) -> RunnerJob {
        RunnerJob::from_request_parts(&mut parts, &())
            .await
            .expect("infallible")
    }

    #[tokio::test]
    async fn runner_job_absent_marker_is_not_narrowed() {
        let rj = extract_runner_job(parts_with_runner_job(None, None)).await;
        assert!(!rj.is_runner_job());
        // A non-runner-job never restricts an AC key.
        assert!(rj.ac_key_allowed("anykey"));
    }

    #[tokio::test]
    async fn runner_job_marker_one_is_narrowed() {
        let rj = extract_runner_job(parts_with_runner_job(Some("1"), None)).await;
        assert!(rj.is_runner_job());
    }

    #[tokio::test]
    async fn runner_job_marker_non_one_is_not_narrowed() {
        // Present-but-garbage / present-but-empty marker ⇒ NOT a runner-job
        // (the marker is an additive Worker-set signal; "1" is the ONLY truth).
        for bad in ["", "0", "true", "yes", " 2 ", "01", "1 1"] {
            let rj = extract_runner_job(parts_with_runner_job(Some(bad), None)).await;
            assert!(
                !rj.is_runner_job(),
                "marker {bad:?} must NOT narrow the request",
            );
        }
        // Trimmed "1" (surrounding whitespace) DOES narrow — header values are
        // trimmed like the scope/quota headers.
        let rj = extract_runner_job(parts_with_runner_job(Some("  1  "), None)).await;
        assert!(rj.is_runner_job());
    }

    #[tokio::test]
    async fn runner_job_exact_key_restriction() {
        let key_a = "a".repeat(64);
        let key_b = "b".repeat(64);
        let rj = extract_runner_job(parts_with_runner_job(Some("1"), Some(&key_a))).await;
        assert!(rj.is_runner_job());
        assert!(rj.ac_key_allowed(&key_a), "the pinned key is allowed");
        assert!(!rj.ac_key_allowed(&key_b), "a different key is denied");
    }

    #[tokio::test]
    async fn runner_job_wildcard_allows_any_key() {
        let rj = extract_runner_job(parts_with_runner_job(Some("1"), Some("*"))).await;
        assert!(rj.is_runner_job());
        assert!(rj.ac_key_allowed(&"a".repeat(64)));
        assert!(rj.ac_key_allowed(&"deadbeef".repeat(8)));
    }

    #[tokio::test]
    async fn runner_job_no_key_pinned_allows_any_key() {
        // Marker present, no ac-key-allow header ⇒ deny-DELETE only, no key pin.
        let rj = extract_runner_job(parts_with_runner_job(Some("1"), None)).await;
        assert!(rj.is_runner_job());
        assert!(rj.ac_key_allowed(&"c".repeat(64)));
        // A present-but-empty key-allow header is treated as no pin.
        let rj2 = extract_runner_job(parts_with_runner_job(Some("1"), Some("   "))).await;
        assert!(rj2.ac_key_allowed(&"c".repeat(64)));
    }

    #[tokio::test]
    async fn runner_job_key_pin_ignored_when_not_narrowed() {
        // A key-allow header WITHOUT the marker must not restrict anything —
        // only a genuine runner-job is narrowed.
        let rj = extract_runner_job(parts_with_runner_job(None, Some(&"a".repeat(64)))).await;
        assert!(!rj.is_runner_job());
        assert!(rj.ac_key_allowed(&"b".repeat(64)));
    }

    // ---- ac_create_only (deny-overwrite / anti AC-squat) ----------------------

    /// Build `Parts` carrying an optional runner-job marker + create-only header.
    fn parts_with_create_only(marker: Option<&str>, create_only: Option<&str>) -> Parts {
        let mut b = axum::http::Request::builder();
        if let Some(m) = marker {
            b = b.header(RUNNER_JOB_HEADER, m);
        }
        if let Some(c) = create_only {
            b = b.header(RUNNER_JOB_AC_CREATE_ONLY_HEADER, c);
        }
        b.body(axum::body::Body::empty()).unwrap().into_parts().0
    }

    #[tokio::test]
    async fn ac_create_only_true_only_on_exact_one_with_marker() {
        // runner-job marker "1" + create-only "1" ⇒ create-only ON.
        let rj = extract_runner_job(parts_with_create_only(Some("1"), Some("1"))).await;
        assert!(rj.is_runner_job());
        assert!(rj.ac_create_only());
        // Surrounding whitespace is trimmed (like the marker / scope headers).
        let rj = extract_runner_job(parts_with_create_only(Some("1"), Some("  1  "))).await;
        assert!(rj.ac_create_only());
    }

    #[tokio::test]
    async fn ac_create_only_fail_safe_false_on_non_one() {
        // Present-but-non-"1" create-only value ⇒ OFF (fail-SAFE): the "1" marker
        // is the ONLY truth, exactly like the runner-job marker.
        for bad in ["", "0", "true", "yes", " 2 ", "01", "1 1"] {
            let rj = extract_runner_job(parts_with_create_only(Some("1"), Some(bad))).await;
            assert!(
                !rj.ac_create_only(),
                "create-only value {bad:?} must be OFF (only exact \"1\" is on)",
            );
        }
        // Absent create-only header on a runner-job ⇒ OFF (unchanged behavior).
        let rj = extract_runner_job(parts_with_create_only(Some("1"), None)).await;
        assert!(rj.is_runner_job());
        assert!(!rj.ac_create_only());
    }

    #[tokio::test]
    async fn ac_create_only_false_when_not_runner_job() {
        // A create-only header WITHOUT the runner-job marker ⇒ NOT create-only
        // (defense-in-depth: only a genuine runner-job cred is create-only).
        let rj = extract_runner_job(parts_with_create_only(None, Some("1"))).await;
        assert!(!rj.is_runner_job());
        assert!(!rj.ac_create_only());
        // A non-"1" marker is not a runner-job, so create-only "1" is still OFF.
        let rj = extract_runner_job(parts_with_create_only(Some("0"), Some("1"))).await;
        assert!(!rj.is_runner_job());
        assert!(!rj.ac_create_only());
    }
}
