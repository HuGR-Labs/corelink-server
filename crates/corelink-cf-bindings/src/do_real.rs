//! Real CF Durable Object binding adapter with tenant-scoped naming +
//! audit fence on every stub fetch (R-PREP-CF-DO-REAL, wasm32
//! production binding + native stub).
//!
//! # What this module adds over [`crate::cf_do::CfDurableObjectAdapter`]
//!
//! `cf_do::CfDurableObjectAdapter` (wasm32-only) is a thin shim around
//! `worker::ObjectNamespace::id_from_name` + `id_from_string` +
//! `get_stub`. It is the minimal surface needed by the rollout
//! controller (S-13) hot path. Production also needs:
//!
//! - **Tenant-scoped naming.** Every DO instance name MUST be derived
//!   from a tenant prefix in the canonical form
//!   `tenant:<tenant_id>:<purpose>` (e.g. `tenant:tnt0123:dedup-counter`).
//!   This wrapper enforces the shape via a typed
//!   [`TenantScopedName`] wrapper. A caller cannot accidentally route
//!   a stub fetch for tenant A through tenant B's DO instance.
//! - **Audit fence on every fetch.** A DO `stub.fetch_*` is a mutation
//!   by default (the actor's `state.storage` is durable). Production
//!   needs an audit trail of every cross-instance RPC so incident
//!   response can answer "who poked the rollout DO for tenant T at
//!   wall-clock W?". Fail-CLOSED: if the audit closure returns `Err`,
//!   the fetch is **not** performed.
//! - **Constant-time tenant-id comparison** (CTRL-PRIV-001
//!   defense-in-depth) — the prefix-segment compare on incoming names
//!   uses [`subtle::ConstantTimeEq`] so a timing side channel cannot
//!   distinguish "wrong prefix at byte 0" from "wrong prefix at byte
//!   15". The canonical CoreLink tenant id is a 16-hex HMAC (collision
//!   probability ≈ 2^-96 per pair); the constant-time path is
//!   defense-in-depth for the membership check, not the HMAC itself.
//!
//! # Dual-target build
//!
//! - `target_arch = "wasm32"` → wraps `worker::ObjectNamespace` and
//!   issues real `id_from_name` / `id_from_string` / `get_stub` /
//!   `stub.fetch_with_str` / `stub.fetch_with_request` calls.
//! - `target_arch != "wasm32"` → host stub. Every method validates
//!   the scoped name + emits the audit fence first, then returns
//!   [`DoError::Backend`] with the stable prefix `"WasmOnly: …"`. The
//!   wrapper-layer contract therefore runs on native CI before the
//!   wasm32 build.
//!
//! For unit tests that need to exercise the full fetch round-trip
//! without the wasm32 toolchain, callers pass a [`FakeDoRouter`] into
//! [`CfDurableObjectReal::with_fake_router`] (native build only). The
//! router routes scoped names to handler closures that produce
//! [`FakeFetchResponse`]s — see the `tests/do_real.rs` integration
//! tests for the canonical wiring.
//!
//! # Pattern replication
//!
//! This is the third application of the pattern documented in
//! `specs/_audits/2026-05-15-cf-binding-real-pattern.md` (R2 / D1 / KV
//! are the siblings). All four bindings share:
//!
//! - Typed tenant-scoped key/name wrapper.
//! - Audit hook fail-CLOSED on every mutation (DO: every fetch).
//! - Dual-target wasm32 + native stub.
//! - `R2Op` / `DoOp` enum of operation labels with `Display` impl.
//! - `#![forbid(unsafe_code)]` (crate root); no `unwrap` / `expect` /
//!   `panic` outside `#[cfg(test)]`.

// `worker::durable::Stub` is wasm32-only (wraps `js-sys::JsValue`); the
// type imports here are also wasm32-only. The native build of this
// module provides parallel stubs that satisfy the same wrapper-layer
// contract without linking the worker types.

use std::fmt;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error surface
// ---------------------------------------------------------------------------

/// Canonical error code (mirrors `corelink-worker::storage::error`
/// taxonomy semantics — DO errors map to `COR_INTERNAL` on the
/// server-side fault path, and to `COR_SERVICE_DEGRADED` on transient
/// CF binding hiccups; the chooser sits upstream).
pub const COR_DO_INTERNAL: &str = "COR_DO_INTERNAL";

/// Stable diagnostic prefix used by [`DoError::Backend`] on the native
/// stub path. Upstream code MAY match on `"WasmOnly:"` to detect the
/// "wrong target" signal during host-only test runs.
pub const WASM_ONLY_PREFIX: &str = "WasmOnly: ";

/// Stable diagnostic prefix used by [`DoError::Backend`] on
/// scoped-name validation failures. Upstream code MAY match on
/// `"tenant_scope:"` to distinguish prefix violations from CF binding
/// faults.
pub const TENANT_SCOPE_PREFIX: &str = "tenant_scope: ";

/// Stable diagnostic prefix used by [`DoError::Backend`] when the
/// audit closure returns `Err` (fail-CLOSED). Upstream code MAY match
/// on `"audit:"` to surface policy denials with a dedicated UX.
pub const AUDIT_DENY_PREFIX: &str = "audit: ";

/// Durable Object adapter error surface. `#[non_exhaustive]` so adding
/// new variants is a minor version bump (the public API stays
/// forward-compatible).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DoError {
    /// A generic backend fault (e.g. CF binding rejected the name,
    /// network hiccup, audit policy denied). Carries a stable
    /// diagnostic prefix:
    ///
    /// - `"tenant_scope: …"` — scoped-name validation failed.
    /// - `"audit: …"`        — audit closure denied the fetch.
    /// - `"WasmOnly: …"`     — native stub refusal.
    /// - `"do_fetch: …"`     — `stub.fetch_*` returned an error
    ///   (wasm32 path).
    /// - `"do_resolve: …"`   — `id_from_*` / `get_stub` failed
    ///   (wasm32 path).
    #[error("durable object backend error ({COR_DO_INTERNAL}): {0}")]
    Backend(String),
}

// ---------------------------------------------------------------------------
// Operation labels
// ---------------------------------------------------------------------------

/// Canonical operation labels for the audit hook. Stable strings —
/// upstream code MAY pattern-match on the `Display` output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DoOp {
    /// `stub.fetch_with_str(url)`.
    FetchStr,
    /// `stub.fetch_with_request(req)`.
    FetchRequest,
    /// `id_from_name(name)` + `get_stub()` (no fetch issued yet — the
    /// audit fence on resolve catches "did we hand out a stub at all?"
    /// for the incident-response trail).
    Resolve,
}

impl DoOp {
    /// Static label used in `DoError::Backend` diagnostics and in the
    /// audit hook signature.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::FetchStr => "fetch_with_str",
            Self::FetchRequest => "fetch_with_request",
            Self::Resolve => "resolve",
        }
    }
}

impl fmt::Display for DoOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Tenant prefix + tenant-scoped name
// ---------------------------------------------------------------------------

/// A validated tenant prefix anchor for DO instance naming.
///
/// Construction validates basic shape (non-empty, no whitespace, no
/// embedded NUL, no `:` separators since `:` is reserved as the
/// segment delimiter in the canonical `tenant:<id>:<purpose>` shape).
/// Cloning is cheap.
///
/// The canonical CoreLink tenant prefix is a 16-hex HMAC produced by
/// `corelink_tenant_path::TenantPrefix`; we don't bind to that type
/// here to keep this crate decoupled from upstream tenant-derivation
/// machinery (same decoupling stance as `r2_real::TenantPrefix`).
#[derive(Clone, Debug, Eq)]
pub struct DoTenantPrefix(String);

impl DoTenantPrefix {
    /// Construct a tenant prefix from a raw string. Validates:
    ///
    /// - non-empty;
    /// - no `:` character (reserved separator in scoped names);
    /// - no `/` character (defense against double-encoding when a key
    ///   is shared with the R2 binding);
    /// - no embedded NUL;
    /// - no whitespace (defense against accidental name corruption).
    ///
    /// Returns [`DoError::Backend`] with the stable diagnostic prefix
    /// `"tenant_scope: "` on rejection.
    pub fn new(raw: impl Into<String>) -> Result<Self, DoError> {
        let s = raw.into();
        if s.is_empty() {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}empty prefix rejected"
            )));
        }
        if s.contains(':') {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}prefix '{s}' must not contain ':'"
            )));
        }
        if s.contains('/') {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}prefix '{s}' must not contain '/'"
            )));
        }
        if s.contains('\0') {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}prefix must not contain NUL byte"
            )));
        }
        if s.chars().any(char::is_whitespace) {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}prefix '{s}' must not contain whitespace"
            )));
        }
        Ok(Self(s))
    }

    /// Borrow the prefix as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Constant-time equality on the tenant-id byte string. This is the
/// `PartialEq` implementation `DoTenantPrefix` uses for routing checks.
impl PartialEq for DoTenantPrefix {
    fn eq(&self, other: &Self) -> bool {
        // `subtle::ConstantTimeEq` returns a `Choice` (u8 0/1) that we
        // convert into a bool via `.into()`. Bytes of unequal length
        // short-circuit to false explicitly so we don't leak the
        // length difference via the ct_eq path (which requires equal
        // lengths to be meaningful).
        let a = self.0.as_bytes();
        let b = other.0.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        a.ct_eq(b).into()
    }
}

impl std::hash::Hash for DoTenantPrefix {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// A DO instance name that has been validated to start with a specific
/// tenant prefix in the canonical `tenant:<tenant_id>:<purpose>` shape.
/// Constructible only via [`CfDurableObjectReal::scoped_name`].
///
/// The wrapper is `Clone + Debug` but the inner string is **never**
/// logged by this crate outside the audit hook (CTRL-PRIV-001).
#[derive(Clone, Debug)]
pub struct TenantScopedName {
    full: String,
    /// Length of `tenant:<id>:` (i.e. the boundary of the tenant
    /// segment plus the second `:`). Kept for invariant audits.
    #[allow(dead_code)]
    prefix_len: usize,
}

impl TenantScopedName {
    /// Borrow the full canonical name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.full
    }
}

// ---------------------------------------------------------------------------
// Audit hook
// ---------------------------------------------------------------------------

/// Closure type for the audit emitter wired into
/// [`CfDurableObjectReal`]. Called BEFORE every stub fetch (or stub
/// resolution). If the closure returns `Err`, the fetch (or resolve)
/// is NOT performed (fail-CLOSED).
///
/// Production wires a closure that fans into `apps/server`'s audit
/// chain. Tests use the default no-op or a recording closure (see the
/// `tests/do_real.rs` integration tests).
pub type AuditFn =
    Arc<dyn Fn(DoOp, &str) -> Result<(), DoError> + Send + Sync + 'static>;

fn noop_audit() -> AuditFn {
    Arc::new(|_op, _name| Ok(()))
}

// ---------------------------------------------------------------------------
// FakeDoRouter (native-only) — test-side in-memory routing.
// ---------------------------------------------------------------------------

/// Minimal response shape returned by a [`FakeDoRouter`] handler.
///
/// Modelled on the subset of `worker::Response` that callers actually
/// inspect (status + body). Native tests assert on these fields after
/// a round-trip through [`CfDurableObjectReal::fetch_with_str`] or
/// [`CfDurableObjectReal::fetch_with_request`].
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct FakeFetchResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body. Empty by default.
    pub body: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
impl FakeFetchResponse {
    /// Construct a 200 OK response with the given body.
    #[must_use]
    pub fn ok(body: Vec<u8>) -> Self {
        Self { status: 200, body }
    }

    /// Construct an empty response with the given status.
    #[must_use]
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
        }
    }
}

/// Native-only handler closure type. Routes a stub fetch keyed by the
/// (scoped name, url) tuple to a response.
///
/// Returning `Err` simulates a CF binding fault — the wrapper maps it
/// onto [`DoError::Backend("do_fetch: …")`].
#[cfg(not(target_arch = "wasm32"))]
pub type FakeFetchHandler = Arc<
    dyn Fn(&str, &str) -> Result<FakeFetchResponse, DoError> + Send + Sync + 'static,
>;

/// Native-only in-memory router that stands in for the `worker::*`
/// stub-fetch path in unit tests.
///
/// Construct via [`FakeDoRouter::new`], register handlers via
/// [`FakeDoRouter::on`], then pass into
/// [`CfDurableObjectReal::with_fake_router`].
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct FakeDoRouter {
    handler: FakeFetchHandler,
}

#[cfg(not(target_arch = "wasm32"))]
impl fmt::Debug for FakeDoRouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeDoRouter").finish_non_exhaustive()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl FakeDoRouter {
    /// Construct a router with a single handler closure that owns the
    /// `(scoped_name, url) -> response` dispatch.
    #[must_use]
    pub fn new(handler: FakeFetchHandler) -> Self {
        Self { handler }
    }

    /// Construct a router that returns a fixed `FakeFetchResponse` for
    /// every (name, url) pair. Convenience for tests that only need
    /// to assert the round-trip.
    #[must_use]
    pub fn constant(response: FakeFetchResponse) -> Self {
        Self {
            handler: Arc::new(move |_name, _url| Ok(response.clone())),
        }
    }

    /// Construct a router that always returns `Err(DoError::Backend
    /// ("do_fetch: <reason>"))`. Convenience for tests that exercise
    /// the binding-fault path.
    #[must_use]
    pub fn always_fail(reason: impl Into<String>) -> Self {
        let r = reason.into();
        Self {
            handler: Arc::new(move |_name, _url| {
                Err(DoError::Backend(format!("do_fetch: {r}")))
            }),
        }
    }

    /// Build a router that recognises a single (name, url) tuple.
    /// Any other fetch returns 404. Useful for "happy path" tests.
    #[must_use]
    pub fn on(
        expected_name: impl Into<String>,
        expected_url: impl Into<String>,
        response: FakeFetchResponse,
    ) -> Self {
        let n = expected_name.into();
        let u = expected_url.into();
        Self {
            handler: Arc::new(move |name, url| {
                if name == n && url == u {
                    Ok(response.clone())
                } else {
                    Ok(FakeFetchResponse::empty(404))
                }
            }),
        }
    }

    /// Invoke the routed handler. Called by [`CfDurableObjectReal`]
    /// internals on the native build only.
    pub fn dispatch(
        &self,
        scoped_name: &str,
        url: &str,
    ) -> Result<FakeFetchResponse, DoError> {
        (self.handler)(scoped_name, url)
    }
}

// ---------------------------------------------------------------------------
// CfDurableObjectReal: dual-target struct definition
// ---------------------------------------------------------------------------

/// Production Cloudflare Durable Object binding adapter with
/// tenant-scoped naming + audit fence.
///
/// Dual-target: wasm32 wraps a `worker::ObjectNamespace`; native build
/// holds no CF binding (the stub returns
/// `DoError::Backend("WasmOnly: …")` after validation+audit) or, if
/// constructed via [`Self::with_fake_router`], routes through a
/// [`FakeDoRouter`] for unit tests.
///
/// See `specs/_audits/2026-05-15-cf-binding-real-pattern.md` for the
/// replication pattern.
#[cfg(target_arch = "wasm32")]
pub struct CfDurableObjectReal {
    namespace: worker::ObjectNamespace,
    tenant: DoTenantPrefix,
    audit: AuditFn,
}

/// Native-build stub variant of [`CfDurableObjectReal`]. Holds the
/// tenant prefix + audit hook so the wrapper-layer validation contract
/// still runs on native CI. If a [`FakeDoRouter`] is wired via
/// [`Self::with_fake_router`], fetches route through it; otherwise
/// every operation returns `DoError::Backend("WasmOnly: …")` after
/// validation/audit.
#[cfg(not(target_arch = "wasm32"))]
pub struct CfDurableObjectReal {
    tenant: DoTenantPrefix,
    audit: AuditFn,
    fake_router: Option<FakeDoRouter>,
}

impl fmt::Debug for CfDurableObjectReal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CfDurableObjectReal")
            .field("tenant", &self.tenant)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Shared (target-agnostic) impl: constructors, scoped-name derivation,
// audit hookup, tenancy comparison. Compiles on both wasm32 and native.
// ---------------------------------------------------------------------------

impl CfDurableObjectReal {
    /// The tenant prefix this wrapper enforces.
    #[must_use]
    pub fn tenant(&self) -> &DoTenantPrefix {
        &self.tenant
    }

    /// Replace the audit hook. Returns `self` for builder-style chains.
    #[must_use]
    pub fn with_audit(mut self, audit: AuditFn) -> Self {
        self.audit = audit;
        self
    }

    /// Derive a [`TenantScopedName`] for a raw name. Validates that
    /// `name` either:
    ///
    /// - already starts with `tenant:<tenant_id>:`, in which case it
    ///   is accepted verbatim (the tail after the second `:` is
    ///   validated as non-empty); or
    /// - does not contain the `tenant:` ceremony anywhere, in which
    ///   case the wrapper prepends `tenant:<tenant_id>:` to produce
    ///   the canonical name (admin tools / startup wiring).
    ///
    /// The tenant-id segment compare uses
    /// [`subtle::ConstantTimeEq`] (defense-in-depth against a timing
    /// channel that could distinguish prefixes byte-by-byte).
    ///
    /// # Errors
    ///
    /// Returns [`DoError::Backend`] with the stable prefix
    /// `"tenant_scope: "` if the name is empty, contains NUL, has the
    /// `tenant:` ceremony with a mismatched tenant-id, or has an
    /// empty purpose tail.
    pub fn scoped_name(&self, name: &str) -> Result<TenantScopedName, DoError> {
        if name.is_empty() {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}empty name rejected"
            )));
        }
        if name.contains('\0') {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}name must not contain NUL byte"
            )));
        }
        if name.chars().any(char::is_whitespace) {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}name '{name}' must not contain whitespace"
            )));
        }
        let tenant_id = self.tenant.as_str();
        let canonical_prefix = format!("tenant:{tenant_id}:");

        if let Some(rest) = name.strip_prefix("tenant:") {
            // Caller used the ceremony; verify tenant-id segment
            // matches in constant time, then validate the tail.
            let Some(colon_idx) = rest.find(':') else {
                return Err(DoError::Backend(format!(
                    "{TENANT_SCOPE_PREFIX}name '{name}' is missing purpose segment"
                )));
            };
            let supplied_tid = rest.get(..colon_idx).unwrap_or("");
            // Constant-time tenant-id comparison.
            let supplied_bytes = supplied_tid.as_bytes();
            let expected_bytes = tenant_id.as_bytes();
            let matches = supplied_bytes.len() == expected_bytes.len()
                && bool::from(supplied_bytes.ct_eq(expected_bytes));
            if !matches {
                return Err(DoError::Backend(format!(
                    "{TENANT_SCOPE_PREFIX}name '{name}' tenant-id segment does not match adapter tenant"
                )));
            }
            // Tail: everything after `tenant:<tid>:`.
            let tail = rest.get((colon_idx + 1)..).unwrap_or("");
            if tail.is_empty() {
                return Err(DoError::Backend(format!(
                    "{TENANT_SCOPE_PREFIX}name '{name}' has empty purpose segment"
                )));
            }
            // Reject nested `:` followed by empty segment edge cases:
            // a purpose like `dedup::counter` is fine (`:` allowed in
            // purpose), but `tenant:tid:` is the empty-tail case
            // already caught above.
            return Ok(TenantScopedName {
                full: name.to_owned(),
                prefix_len: canonical_prefix.len(),
            });
        }

        // No ceremony: caller passed a bare purpose. Reject leading
        // `:` and bare `tenant` (without the trailing colon) to keep
        // the canonical form unambiguous.
        if name.starts_with(':') {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}name '{name}' is malformed (leading ':')"
            )));
        }
        // Forbid raw "tenant:<other>:..." where prefix-strip didn't
        // happen — `strip_prefix("tenant:")` above would have matched
        // that case. Other names that mention "tenant" embedded in
        // the purpose (e.g. `multi-tenant-counter`) are accepted as
        // tails because they don't start with "tenant:".
        let full = format!("{canonical_prefix}{name}");
        Ok(TenantScopedName {
            full,
            prefix_len: canonical_prefix.len(),
        })
    }

    /// Helper: validate-or-derive the scoped name and emit the audit
    /// fence before any backend call. Returns the full scoped name for
    /// the downstream `worker::*` / `FakeDoRouter` invocation.
    fn audit_and_scope(
        &self,
        op: DoOp,
        name: &str,
    ) -> Result<TenantScopedName, DoError> {
        let scoped = self.scoped_name(name)?;
        (self.audit)(op, scoped.as_str())?;
        Ok(scoped)
    }
}

// ---------------------------------------------------------------------------
// wasm32 production impl
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
impl CfDurableObjectReal {
    /// Construct a real wrapper from a `worker::ObjectNamespace` and a
    /// validated tenant prefix. Uses a no-op audit hook; replace via
    /// [`Self::with_audit`].
    #[must_use]
    pub fn new(namespace: worker::ObjectNamespace, tenant: DoTenantPrefix) -> Self {
        Self {
            namespace,
            tenant,
            audit: noop_audit(),
        }
    }

    /// Borrow the underlying `worker::ObjectNamespace` for callers that
    /// truly need the raw surface (e.g. an alternate routing layer).
    /// Use sparingly; bypasses tenant-scope enforcement.
    #[must_use]
    pub fn inner(&self) -> &worker::ObjectNamespace {
        &self.namespace
    }

    /// Resolve a tenant-scoped name to a stub. Equivalent to
    /// `namespace.id_from_name(name).get_stub()`. Audit-fenced.
    ///
    /// # Errors
    ///
    /// Returns [`DoError::Backend`] if the scoped-name validation
    /// fails (`tenant_scope:` prefix), the audit hook denies the
    /// resolve (`audit:` prefix), or the CF binding rejects the name
    /// (`do_resolve:` prefix).
    pub fn stub_by_name(&self, name: &str) -> Result<worker::Stub, DoError> {
        let scoped = self.audit_and_scope(DoOp::Resolve, name)?;
        let id = self
            .namespace
            .id_from_name(scoped.as_str())
            .map_err(|e| DoError::Backend(format!("do_resolve: id_from_name: {e}")))?;
        id.get_stub()
            .map_err(|e| DoError::Backend(format!("do_resolve: get_stub: {e}")))
    }

    /// Resolve a hex-encoded ObjectId to a stub. The hex id MUST have
    /// been produced by the same namespace previously (e.g. read out
    /// of a D1 row). Audit-fenced using `hex:<id>` as the audit key
    /// — the canonical scoped-name shape does not apply because the
    /// id is opaque, but the audit trail still records the access.
    ///
    /// # Errors
    ///
    /// Returns [`DoError::Backend`] if `hex_id` is empty, contains
    /// non-hex characters (rejected with `tenant_scope:` prefix), the
    /// audit hook denies the resolve, or the CF binding rejects the
    /// id.
    pub fn stub_by_hex_id(&self, hex_id: &str) -> Result<worker::Stub, DoError> {
        if hex_id.is_empty() {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}empty hex id rejected"
            )));
        }
        if hex_id.len() != 64 {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}hex id must be 64 chars (got {})",
                hex_id.len()
            )));
        }
        if !hex_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}hex id must be ascii-hex"
            )));
        }
        let audit_key = format!("hex:{hex_id}");
        (self.audit)(DoOp::Resolve, &audit_key)?;
        let id = self
            .namespace
            .id_from_string(hex_id)
            .map_err(|e| DoError::Backend(format!("do_resolve: id_from_string: {e}")))?;
        id.get_stub()
            .map_err(|e| DoError::Backend(format!("do_resolve: get_stub: {e}")))
    }

    /// Resolve `name` to a stub and issue `stub.fetch_with_str(url)`.
    /// Audit-fenced (the audit hook receives `DoOp::FetchStr` and the
    /// scoped name as the audit key). Fail-CLOSED.
    ///
    /// # Errors
    ///
    /// Returns [`DoError::Backend`] if validation/audit fails, the
    /// stub resolution fails, or the fetch returns an error.
    pub async fn fetch_with_str(
        &self,
        name: &str,
        url: &str,
    ) -> Result<worker::Response, DoError> {
        let scoped = self.audit_and_scope(DoOp::FetchStr, name)?;
        let id = self
            .namespace
            .id_from_name(scoped.as_str())
            .map_err(|e| DoError::Backend(format!("do_resolve: id_from_name: {e}")))?;
        let stub = id
            .get_stub()
            .map_err(|e| DoError::Backend(format!("do_resolve: get_stub: {e}")))?;
        stub.fetch_with_str(url)
            .await
            .map_err(|e| DoError::Backend(format!("do_fetch: fetch_with_str: {e}")))
    }

    /// Resolve `name` to a stub and issue
    /// `stub.fetch_with_request(req)`. Audit-fenced. Fail-CLOSED.
    ///
    /// # Errors
    ///
    /// Returns [`DoError::Backend`] if validation/audit fails, the
    /// stub resolution fails, or the fetch returns an error.
    pub async fn fetch_with_request(
        &self,
        name: &str,
        req: worker::Request,
    ) -> Result<worker::Response, DoError> {
        let scoped = self.audit_and_scope(DoOp::FetchRequest, name)?;
        let id = self
            .namespace
            .id_from_name(scoped.as_str())
            .map_err(|e| DoError::Backend(format!("do_resolve: id_from_name: {e}")))?;
        let stub = id
            .get_stub()
            .map_err(|e| DoError::Backend(format!("do_resolve: get_stub: {e}")))?;
        stub.fetch_with_request(req)
            .await
            .map_err(|e| DoError::Backend(format!("do_fetch: fetch_with_request: {e}")))
    }
}

// ---------------------------------------------------------------------------
// native stub
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
impl CfDurableObjectReal {
    /// Construct a native stub for host-side trait-bound testing.
    /// Every operation runs validation+audit and then returns
    /// `DoError::Backend("WasmOnly: …")`.
    ///
    /// The stub still enforces tenant-scope validation (it runs
    /// `scoped_name` first), so the wrapper-layer tests pin the
    /// validation contract on the native target — the same code path
    /// runs on wasm32 in production.
    #[must_use]
    pub fn stub_for_native_tests(tenant: DoTenantPrefix) -> Self {
        Self {
            tenant,
            audit: noop_audit(),
            fake_router: None,
        }
    }

    /// Construct a native stub backed by an in-memory
    /// [`FakeDoRouter`]. Fetches route through the router so unit
    /// tests can exercise the full round-trip (scope → audit →
    /// stub-fetch → response) without the wasm32 toolchain.
    #[must_use]
    pub fn with_fake_router(
        tenant: DoTenantPrefix,
        router: FakeDoRouter,
    ) -> Self {
        Self {
            tenant,
            audit: noop_audit(),
            fake_router: Some(router),
        }
    }

    /// Borrow the wired [`FakeDoRouter`] if one was set via
    /// [`Self::with_fake_router`].
    #[must_use]
    pub fn fake_router(&self) -> Option<&FakeDoRouter> {
        self.fake_router.as_ref()
    }

    /// Native stub for `stub_by_name`. Validates+audits, then refuses
    /// (no router routing for the resolve-only path — the meaningful
    /// surface is `fetch_*`).
    pub fn stub_by_name(&self, name: &str) -> Result<(), DoError> {
        let _ = self.audit_and_scope(DoOp::Resolve, name)?;
        Err(DoError::Backend(format!(
            "{WASM_ONLY_PREFIX}{}",
            DoOp::Resolve
        )))
    }

    /// Native stub for `stub_by_hex_id`. Validates the hex shape,
    /// audits, then refuses.
    pub fn stub_by_hex_id(&self, hex_id: &str) -> Result<(), DoError> {
        if hex_id.is_empty() {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}empty hex id rejected"
            )));
        }
        if hex_id.len() != 64 {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}hex id must be 64 chars (got {})",
                hex_id.len()
            )));
        }
        if !hex_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DoError::Backend(format!(
                "{TENANT_SCOPE_PREFIX}hex id must be ascii-hex"
            )));
        }
        let audit_key = format!("hex:{hex_id}");
        (self.audit)(DoOp::Resolve, &audit_key)?;
        Err(DoError::Backend(format!(
            "{WASM_ONLY_PREFIX}{}",
            DoOp::Resolve
        )))
    }

    /// Native stub for `fetch_with_str`. Validates+audits. If a
    /// [`FakeDoRouter`] is wired, the fetch routes through it and the
    /// fake response is returned. Otherwise returns
    /// `DoError::Backend("WasmOnly: fetch_with_str")`.
    pub async fn fetch_with_str(
        &self,
        name: &str,
        url: &str,
    ) -> Result<FakeFetchResponse, DoError> {
        let scoped = self.audit_and_scope(DoOp::FetchStr, name)?;
        match self.fake_router.as_ref() {
            Some(router) => router.dispatch(scoped.as_str(), url),
            None => Err(DoError::Backend(format!(
                "{WASM_ONLY_PREFIX}{}",
                DoOp::FetchStr
            ))),
        }
    }

    /// Native stub for `fetch_with_request`. Mirrors
    /// [`Self::fetch_with_str`] but takes a `(method, url, body)`
    /// triple so callers don't need to construct a `worker::Request`
    /// on the host (no wasm32 plumbing required for tests).
    pub async fn fetch_with_request(
        &self,
        name: &str,
        method: &str,
        url: &str,
        _body: &[u8],
    ) -> Result<FakeFetchResponse, DoError> {
        let scoped = self.audit_and_scope(DoOp::FetchRequest, name)?;
        // Encode the method into the routed url for fake dispatch so
        // tests can assert on method-discrimination.
        let routed_url = format!("{method} {url}");
        match self.fake_router.as_ref() {
            Some(router) => router.dispatch(scoped.as_str(), &routed_url),
            None => Err(DoError::Backend(format!(
                "{WASM_ONLY_PREFIX}{}",
                DoOp::FetchRequest
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests — wrapper layer (target-agnostic; run on native CI).
//
// These tests pin the contract that runs on BOTH targets (the shared
// impl block above). The integration test suite under tests/do_real.rs
// covers the fetch round-trip via FakeDoRouter.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: panics on assertion failure are the canonical signal"
)]
mod tests {
    use super::*;

    fn tp(s: &str) -> DoTenantPrefix {
        DoTenantPrefix::new(s).expect("test prefix must be valid")
    }

    #[test]
    fn tenant_prefix_rejects_empty() {
        let err = DoTenantPrefix::new("").expect_err("empty prefix");
        match err {
            DoError::Backend(msg) => assert!(msg.starts_with(TENANT_SCOPE_PREFIX)),
        }
    }

    #[test]
    fn tenant_prefix_rejects_colon() {
        let err = DoTenantPrefix::new("a:b").expect_err("colon in prefix");
        match err {
            DoError::Backend(msg) => assert!(msg.contains("must not contain ':'")),
        }
    }

    #[test]
    fn tenant_prefix_rejects_slash() {
        let err = DoTenantPrefix::new("a/b").expect_err("slash in prefix");
        match err {
            DoError::Backend(msg) => assert!(msg.contains("must not contain '/'")),
        }
    }

    #[test]
    fn tenant_prefix_equality_is_constant_time_path() {
        // Same content → equal.
        let a = tp("tnt0123456789abcd");
        let b = tp("tnt0123456789abcd");
        assert_eq!(a, b);
        // Different content → not equal.
        let c = tp("tntABCDEF01234567");
        assert_ne!(a, c);
        // Different length → not equal (length check short-circuits
        // BEFORE ct_eq, by design).
        let d = tp("short");
        assert_ne!(a, d);
    }

    #[test]
    fn do_op_display_is_stable() {
        assert_eq!(DoOp::FetchStr.as_str(), "fetch_with_str");
        assert_eq!(DoOp::FetchRequest.as_str(), "fetch_with_request");
        assert_eq!(DoOp::Resolve.as_str(), "resolve");
    }
}
