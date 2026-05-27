//! Real CF KV binding adapter with extended operations + tenant-prefix
//! enforcement (R-PREP, wasm32 production binding).
//!
//! Replicates the [`crate::r2_real`] template onto Cloudflare Workers KV
//! ([`worker::kv::KvStore`]). See
//! `specs/_audits/2026-05-15-cf-binding-real-pattern.md` for the
//! audit-fenced, tenant-prefix-enforced shape this module follows.
//!
//! # What this module adds over `crate::cf_kv::CfKvNamespaceAdapter` (wasm32-only)
//!
//! `cf_kv::CfKvNamespaceAdapter` (wasm32-only) implements the **minimal**
//! `KvBackend` trait surface (`get` / `put_with_ttl` / `delete`) used
//! by the negative-cache. Production also needs the broader KV toolkit:
//!
//! - `get_bytes` — raw byte read (already covered by trait `get`,
//!   re-exposed here for symmetry with the R2 wrapper API shape).
//! - `put_bytes` with explicit `Option<u64>` TTL (clamped to the 60s CF
//!   floor; TTL = `None` means "permanent until manually deleted").
//! - `list` (with prefix scoped to the tenant root) for admin sweeps +
//!   GC tooling.
//!
//! All extended operations route through [`CfKvNamespaceReal`] which:
//!
//! 1. **Enforces tenant prefix** on every key argument. Defense-in-
//!    depth: every key is wrapped via the typed [`TenantScopedKey`];
//!    if a caller hands in a tenant-local tail, the wrapper prepends
//!    `<tenant_prefix>:` (KV uses `:`-separated path segments by
//!    convention — distinct from the R2 `/` separator because KV keys
//!    have no path-tree semantics and a `:` collation surfaces nicely
//!    in the dashboard prefix browser).
//! 2. **Forbids unwrap/expect/panic** outside `#[cfg(test)]`.
//! 3. **Emits an audit fence** before every mutation (`put` / `delete`)
//!    and before list (production needs the probe trail for incident
//!    response). Audit emission is fail-CLOSED: if the audit closure
//!    returns an `Err`, the KV mutation is **not** performed.
//! 4. **Tenant-id ConstantTimeEq** — when verifying that a key already
//!    starts with this wrapper's prefix, the comparison uses
//!    [`subtle::ConstantTimeEq`] to neutralize timing side-channels on
//!    cross-tenant probes.
//! 5. **TTL clamp** — Cloudflare requires `expirationTtl >= 60s`; the
//!    wrapper clamps defensively (the same behavior
//!    `crate::cf_kv::CfKvNamespaceAdapter` applies on its `put_with_ttl`
//!    path).
//!
//! # Dual-target build
//!
//! - `target_arch = "wasm32"`: real `worker::kv::KvStore` wiring.
//! - `target_arch != "wasm32"`: stub that returns
//!   [`KvError::Backend`] with the stable prefix `"WasmOnly: …"`. The
//!   diagnostic prefix is contract-stable for upstream pattern matching.
//!
//! # Trait impl
//!
//! [`CfKvNamespaceReal`] still implements
//! [`corelink_worker::cache::kv::KvBackend`] so it is drop-in compatible
//! with the negative-cache wiring under the same trait abstraction.
//!
//! # Charter (HARD requirements)
//!
//! - `#![forbid(unsafe_code)]` at crate root.
//! - No `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.
//! - Audit fail-CLOSED on every mutation (`put_bytes` / `delete`).
//! - Tenant prefix enforced (typed wrapper + runtime check + CT cmp).
//! - `#[non_exhaustive]` on public enums.

use corelink_cas::cache::kv::{KvBackend, KvError};
use std::fmt;
#[cfg(target_arch = "wasm32")]
use std::future::Future;
use std::sync::Arc;
use subtle::ConstantTimeEq;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum KV TTL enforced by Cloudflare (60 seconds). PUTs with a
/// smaller TTL are clamped up rather than rejected — matches the
/// behavior of `crate::cf_kv::CfKvNamespaceAdapter::put_with_ttl`.
pub const CF_KV_MIN_TTL_SECS: u64 = 60;

// ---------------------------------------------------------------------------
// Tenant-prefix typed wrapper
// ---------------------------------------------------------------------------

/// A tenant prefix anchor for KV. Constructible only via
/// [`TenantPrefix::new`] which validates basic shape: non-empty, no `:`
/// anywhere (segments are joined by the wrapper using `:`), no embedded
/// NUL.
///
/// The canonical tenant prefix is a 16-hex HMAC produced by
/// `corelink_tenant_path::TenantPrefix`; this crate does not bind to
/// that type so it stays decoupled from upstream tenant derivation.
#[derive(Clone, Debug, Eq)]
pub struct TenantPrefix(String);

impl PartialEq for TenantPrefix {
    /// Constant-time equality on the underlying bytes — neutralizes a
    /// timing side-channel for cross-tenant probe attempts.
    fn eq(&self, other: &Self) -> bool {
        self.0.as_bytes().ct_eq(other.0.as_bytes()).into()
    }
}

impl std::hash::Hash for TenantPrefix {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl TenantPrefix {
    /// Construct a tenant prefix from a raw string. Validates:
    ///
    /// - non-empty
    /// - no `:` characters (segments are joined by [`CfKvNamespaceReal`])
    /// - no NUL bytes
    ///
    /// Returns [`KvError::Backend`] with a stable diagnostic prefix
    /// (`tenant_prefix:`) on rejection.
    ///
    /// # Errors
    ///
    /// Returns [`KvError::Backend`] if the input is empty, contains a
    /// `:` separator, or contains a NUL byte.
    pub fn new(raw: impl Into<String>) -> Result<Self, KvError> {
        let s = raw.into();
        if s.is_empty() {
            return Err(KvError::Backend(
                "tenant_prefix: empty prefix rejected".to_owned(),
            ));
        }
        if s.contains(':') {
            return Err(KvError::Backend(format!(
                "tenant_prefix: prefix '{s}' must not contain ':'"
            )));
        }
        if s.contains('\0') {
            return Err(KvError::Backend(
                "tenant_prefix: prefix must not contain NUL byte".to_owned(),
            ));
        }
        Ok(Self(s))
    }

    /// Borrow the prefix as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A KV key string that has been validated to start with a specific
/// tenant prefix. Constructible only via
/// [`CfKvNamespaceReal::scoped_key`]; the type therefore witnesses
/// tenant-prefix enforcement at the type level.
///
/// The wrapper is `Clone` + `Debug` but the inner string is **never**
/// logged by this crate (CTRL-PRIV-001).
#[derive(Clone, Debug)]
pub struct TenantScopedKey {
    full: String,
    #[allow(dead_code)] // future invariant audits
    prefix_len: usize,
}

impl TenantScopedKey {
    /// Borrow the full key (prefix + `:` + tail).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.full
    }
}

// ---------------------------------------------------------------------------
// KvOp — stable diagnostic labels
// ---------------------------------------------------------------------------

/// KV operation labels routed to the audit hook and to the `WasmOnly:`
/// diagnostic on the native stub.
///
/// Marked `#[non_exhaustive]` so future additions (e.g. `GetWithMetadata`)
/// do not break downstream pattern-matching consumers.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvOp {
    /// `get` (or `get_bytes`) read.
    Get,
    /// `put` (or `put_bytes`) write with optional TTL.
    Put,
    /// `delete` write.
    Delete,
    /// `list` keys (scoped to the tenant prefix).
    List,
}

impl KvOp {
    /// Static label used in `KvError::Backend` diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Put => "put",
            Self::Delete => "delete",
            Self::List => "list",
        }
    }
}

impl fmt::Display for KvOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Audit hook
// ---------------------------------------------------------------------------

/// Closure type for the audit emitter wired into [`CfKvNamespaceReal`].
/// Called BEFORE every mutation (`put_bytes`, `delete`) AND before
/// `list_keys` so production has the probe trail. Fail-CLOSED: if the
/// closure returns `Err`, the KV op is NOT performed.
///
/// The default is a no-op suitable for read-only test fixtures;
/// production wires a writer via [`CfKvNamespaceReal::with_audit`].
pub type AuditFn =
    Arc<dyn Fn(KvOp, &str) -> Result<(), KvError> + Send + Sync + 'static>;

fn noop_audit() -> AuditFn {
    Arc::new(|_op, _key| Ok(()))
}

// ---------------------------------------------------------------------------
// CfKvNamespaceReal — dual-target struct definition
// ---------------------------------------------------------------------------

/// Real CF KV binding adapter with extended operations and tenant-prefix
/// enforcement.
///
/// Dual-target: wasm32 wraps a [`worker::kv::KvStore`]; native build
/// holds no inner binding (the stub returns
/// `KvError::Backend("WasmOnly: …")` from every operation method after
/// running the same prefix/audit validation the production path runs).
#[cfg(target_arch = "wasm32")]
pub struct CfKvNamespaceReal {
    store: worker::kv::KvStore,
    tenant: TenantPrefix,
    audit: AuditFn,
}

/// Native-build stub variant of [`CfKvNamespaceReal`]. Holds the tenant
/// prefix + audit hook so the wrapper-layer validation contract still
/// runs on native CI; every operation method returns
/// `KvError::Backend("WasmOnly: …")` after validation/audit.
#[cfg(not(target_arch = "wasm32"))]
pub struct CfKvNamespaceReal {
    tenant: TenantPrefix,
    audit: AuditFn,
}

impl fmt::Debug for CfKvNamespaceReal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CfKvNamespaceReal")
            .field("tenant", &self.tenant)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Shared (target-agnostic) impl: constructors, scoped-key derivation,
// audit hookup. Compiles on both wasm32 and native.
// ---------------------------------------------------------------------------

impl CfKvNamespaceReal {
    /// The tenant prefix this wrapper enforces.
    #[must_use]
    pub fn tenant(&self) -> &TenantPrefix {
        &self.tenant
    }

    /// Replace the audit hook. Returns `self` for builder-style chains.
    #[must_use]
    pub fn with_audit(mut self, audit: AuditFn) -> Self {
        self.audit = audit;
        self
    }

    /// Derive a [`TenantScopedKey`] for a raw key. Validates that
    /// `key` either:
    ///
    /// - already starts with `<tenant>:`, accepted verbatim; or
    /// - does not contain a `:` segment that would collide with the
    ///   tenant prefix, in which case the wrapper prepends `<tenant>:`.
    ///
    /// The prefix-check on the "already-prefixed" branch uses
    /// [`subtle::ConstantTimeEq`] to neutralize timing side-channels.
    ///
    /// # Errors
    ///
    /// Returns [`KvError::Backend`] (stable diagnostic prefix
    /// `tenant_prefix:`) if the key is empty, contains a NUL byte, or
    /// is malformed (leading `:` or empty trailing tail).
    pub fn scoped_key(&self, key: &str) -> Result<TenantScopedKey, KvError> {
        if key.is_empty() {
            return Err(KvError::Backend(
                "tenant_prefix: empty key rejected".to_owned(),
            ));
        }
        if key.contains('\0') {
            return Err(KvError::Backend(
                "tenant_prefix: key must not contain NUL byte".to_owned(),
            ));
        }
        let tenant = self.tenant.as_str();
        let prefix_with_sep = format!("{tenant}:");
        // Constant-time membership check on the leading prefix segment.
        // We compare the first prefix_with_sep.len() bytes via subtle.
        let key_bytes = key.as_bytes();
        let pref_bytes = prefix_with_sep.as_bytes();
        let already_prefixed = if key_bytes.len() >= pref_bytes.len() {
            // SAFETY: simple slicing by index that we've already bounded.
            // We forbid clippy::indexing_slicing; use split_at for a panic-free path.
            let (head, _) = key_bytes.split_at(pref_bytes.len());
            head.ct_eq(pref_bytes).into()
        } else {
            false
        };
        let full = if already_prefixed {
            let (_, tail) = key_bytes.split_at(pref_bytes.len());
            if tail.is_empty() {
                return Err(KvError::Backend(
                    "tenant_prefix: key tail must be non-empty".to_owned(),
                ));
            }
            key.to_owned()
        } else if key.starts_with(':') {
            return Err(KvError::Backend(format!(
                "tenant_prefix: key '{key}' is malformed (leading ':' separator)"
            )));
        } else {
            // Treat as tenant-local tail; prepend canonical prefix.
            format!("{prefix_with_sep}{key}")
        };
        let prefix_len = prefix_with_sep.len();
        Ok(TenantScopedKey { full, prefix_len })
    }

    /// Helper: validate-or-derive the scoped key and emit the audit
    /// fence before any mutation/read.
    fn audit_and_scope(&self, op: KvOp, key: &str) -> Result<TenantScopedKey, KvError> {
        let scoped = self.scoped_key(key)?;
        (self.audit)(op, scoped.as_str())?;
        Ok(scoped)
    }

    /// Clamp a TTL to the Cloudflare floor (60s). `None` means
    /// "permanent" and is passed through unmodified.
    ///
    /// Used by the wasm32 [`Self::put_bytes`] write path AND by the
    /// host-side unit tests in `#[cfg(test)] mod tests`. On a pure
    /// native non-test build this helper is unused; the explicit
    /// `#[allow(dead_code)]` keeps the function discoverable in a
    /// `cargo doc` build without a noisy warning.
    #[allow(dead_code, reason = "wasm32+test-only; kept on the shared impl block for clarity")]
    fn clamp_ttl(ttl: Option<u64>) -> Option<u64> {
        ttl.map(|n| n.max(CF_KV_MIN_TTL_SECS))
    }
}

// ---------------------------------------------------------------------------
// wasm32 production impl
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
impl CfKvNamespaceReal {
    /// Construct a real wrapper from a `worker::kv::KvStore` and a
    /// validated tenant prefix. Uses a no-op audit hook; replace via
    /// [`Self::with_audit`].
    #[must_use]
    pub fn new(store: worker::kv::KvStore, tenant: TenantPrefix) -> Self {
        Self {
            store,
            tenant,
            audit: noop_audit(),
        }
    }

    /// Borrow the underlying `worker::kv::KvStore` for callers that
    /// truly need the raw surface. Use sparingly; bypasses
    /// tenant-prefix enforcement.
    #[must_use]
    pub fn inner(&self) -> &worker::kv::KvStore {
        &self.store
    }

    /// `GET key` returning the raw byte body (or `None` on miss /
    /// soft-eviction). Audit-fenced (read-trail).
    pub async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        let scoped = self.audit_and_scope(KvOp::Get, key)?;
        let bytes = self
            .store
            .get(scoped.as_str())
            .bytes()
            .await
            .map_err(|e| KvError::Backend(format!("kv get: {e}")))?;
        Ok(bytes)
    }

    /// `PUT key` with optional TTL (clamped to the 60s CF floor). TTL
    /// = `None` means "permanent until manually deleted". Audit-fenced.
    pub async fn put_bytes(
        &self,
        key: &str,
        value: &[u8],
        ttl_secs: Option<u64>,
    ) -> Result<(), KvError> {
        let scoped = self.audit_and_scope(KvOp::Put, key)?;
        let mut builder = self
            .store
            .put_bytes(scoped.as_str(), value)
            .map_err(|e| KvError::Backend(format!("kv put build: {e}")))?;
        if let Some(ttl) = Self::clamp_ttl(ttl_secs) {
            builder = builder.expiration_ttl(ttl);
        }
        builder
            .execute()
            .await
            .map_err(|e| KvError::Backend(format!("kv put execute: {e}")))?;
        Ok(())
    }

    /// `DELETE key`. Idempotent: deleting a non-existent key is not an
    /// error. Audit-fenced.
    pub async fn delete(&self, key: &str) -> Result<(), KvError> {
        let scoped = self.audit_and_scope(KvOp::Delete, key)?;
        self.store
            .delete(scoped.as_str())
            .await
            .map_err(|e| KvError::Backend(format!("kv delete: {e}")))?;
        Ok(())
    }

    /// `LIST` with the tenant prefix as the list root. Returns the raw
    /// key strings. The optional `limit` is forwarded to KV. Audit-
    /// fenced (probe trail).
    pub async fn list_keys(&self, limit: Option<u64>) -> Result<Vec<String>, KvError> {
        let tenant = self.tenant.as_str();
        let prefix_with_sep = format!("{tenant}:");
        (self.audit)(KvOp::List, &prefix_with_sep)?;
        let mut builder = self.store.list().prefix(prefix_with_sep);
        if let Some(n) = limit {
            builder = builder.limit(n);
        }
        let response = builder
            .execute()
            .await
            .map_err(|e| KvError::Backend(format!("kv list: {e}")))?;
        Ok(response.keys.into_iter().map(|k| k.name).collect())
    }
}

#[cfg(target_arch = "wasm32")]
impl KvBackend for CfKvNamespaceReal {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, KvError>> + Send + 'a {
        worker::send::SendFuture::new(async move { self.get_bytes(key).await })
    }

    fn put_with_ttl<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl_secs: u64,
    ) -> impl Future<Output = Result<(), KvError>> + Send + 'a {
        worker::send::SendFuture::new(async move {
            self.put_bytes(key, &value, Some(ttl_secs)).await
        })
    }

    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> impl Future<Output = Result<(), KvError>> + Send + 'a {
        worker::send::SendFuture::new(async move { CfKvNamespaceReal::delete(self, key).await })
    }
}

// ---------------------------------------------------------------------------
// native stub
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
impl CfKvNamespaceReal {
    /// Construct a native stub for host-side trait-bound testing.
    /// Every operation returns `KvError::Backend("WasmOnly: …")` after
    /// running the same `scoped_key` + `audit` validation the wasm32
    /// path runs, so wrapper-layer tests pin the contract on host CI.
    #[must_use]
    pub fn stub_for_native_tests(tenant: TenantPrefix) -> Self {
        Self {
            tenant,
            audit: noop_audit(),
        }
    }

    /// Native stub for GET — validates prefix, audit fence, then `WasmOnly`.
    pub async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        let _ = self.audit_and_scope(KvOp::Get, key)?;
        Err(KvError::Backend(format!("WasmOnly: {}", KvOp::Get)))
    }

    /// Native stub for PUT — validates prefix, audit fence, then `WasmOnly`.
    pub async fn put_bytes(
        &self,
        key: &str,
        _value: &[u8],
        _ttl_secs: Option<u64>,
    ) -> Result<(), KvError> {
        let _ = self.audit_and_scope(KvOp::Put, key)?;
        Err(KvError::Backend(format!("WasmOnly: {}", KvOp::Put)))
    }

    /// Native stub for DELETE — validates prefix, audit fence, then `WasmOnly`.
    pub async fn delete(&self, key: &str) -> Result<(), KvError> {
        let _ = self.audit_and_scope(KvOp::Delete, key)?;
        Err(KvError::Backend(format!("WasmOnly: {}", KvOp::Delete)))
    }

    /// Native stub for LIST — runs the audit fence, then `WasmOnly`.
    pub async fn list_keys(&self, _limit: Option<u64>) -> Result<Vec<String>, KvError> {
        let tenant = self.tenant.as_str();
        let prefix_with_sep = format!("{tenant}:");
        (self.audit)(KvOp::List, &prefix_with_sep)?;
        Err(KvError::Backend(format!("WasmOnly: {}", KvOp::List)))
    }
}

// Native-target `KvBackend` impl for the stub — so consumer crates can
// thread `CfKvNamespaceReal` through trait-bound generic code in native
// tests (and observe the `WasmOnly:` failure mode end-to-end).
#[cfg(not(target_arch = "wasm32"))]
impl KvBackend for CfKvNamespaceReal {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, KvError> {
        self.get_bytes(key).await
    }

    async fn put_with_ttl(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl_secs: u64,
    ) -> Result<(), KvError> {
        self.put_bytes(key, &value, Some(ttl_secs)).await
    }

    async fn delete(&self, key: &str) -> Result<(), KvError> {
        CfKvNamespaceReal::delete(self, key).await
    }
}

// ---------------------------------------------------------------------------
// Unit tests — wrapper layer (target-agnostic; run on native CI).
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

    fn tp(s: &str) -> TenantPrefix {
        TenantPrefix::new(s).expect("test prefix must be valid")
    }

    // ----- TenantPrefix validation -----

    #[test]
    fn tenant_prefix_rejects_empty() {
        let err = TenantPrefix::new("").expect_err("empty prefix must be rejected");
        match err {
            KvError::Backend(msg) => assert!(msg.starts_with("tenant_prefix:")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn tenant_prefix_rejects_colon() {
        let err = TenantPrefix::new("a:b").expect_err("colon in prefix must be rejected");
        match err {
            KvError::Backend(msg) => {
                assert!(msg.contains("must not contain ':'"), "got: {msg}");
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn tenant_prefix_rejects_nul() {
        let err = TenantPrefix::new("a\0b").expect_err("NUL in prefix must be rejected");
        match err {
            KvError::Backend(msg) => assert!(msg.contains("NUL")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn tenant_prefix_constant_time_equality() {
        // Equality is via ct_eq; verify it is value-correct (the
        // timing property is structural — exercised by code review,
        // not a benchmark assertion).
        let a = tp("tntABCD");
        let b = tp("tntABCD");
        let c = tp("tntABCE");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ----- scoped_key derivation -----

    #[test]
    fn scoped_key_prepends_tenant_when_tail_only() {
        let kv = CfKvNamespaceReal::stub_for_native_tests(tp("tnt0123456789abcd"));
        let scoped = kv
            .scoped_key("neg/blake3/deadbeef")
            .expect("derivation must succeed");
        assert_eq!(scoped.as_str(), "tnt0123456789abcd:neg/blake3/deadbeef");
    }

    #[test]
    fn scoped_key_accepts_already_prefixed() {
        let kv = CfKvNamespaceReal::stub_for_native_tests(tp("tntABCD"));
        let scoped = kv
            .scoped_key("tntABCD:neg/blake3/0123")
            .expect("already-prefixed key must be accepted verbatim");
        assert_eq!(scoped.as_str(), "tntABCD:neg/blake3/0123");
    }

    #[test]
    fn scoped_key_rejects_empty_tail_after_prefix() {
        let kv = CfKvNamespaceReal::stub_for_native_tests(tp("tnt"));
        let err = kv
            .scoped_key("tnt:")
            .expect_err("empty tail must be rejected");
        match err {
            KvError::Backend(msg) => assert!(msg.contains("tail must be non-empty")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_rejects_leading_colon() {
        let kv = CfKvNamespaceReal::stub_for_native_tests(tp("tnt"));
        let err = kv
            .scoped_key(":etc:passwd")
            .expect_err("leading colon must be rejected");
        match err {
            KvError::Backend(msg) => assert!(msg.contains("malformed")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_rejects_empty_key() {
        let kv = CfKvNamespaceReal::stub_for_native_tests(tp("tnt"));
        let err = kv.scoped_key("").expect_err("empty key must be rejected");
        match err {
            KvError::Backend(msg) => assert!(msg.contains("empty key")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_rejects_nul_byte() {
        let kv = CfKvNamespaceReal::stub_for_native_tests(tp("tnt"));
        let err = kv
            .scoped_key("foo\0bar")
            .expect_err("NUL in key must be rejected");
        match err {
            KvError::Backend(msg) => assert!(msg.contains("NUL")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_short_key_treated_as_tail() {
        // A key shorter than `prefix:` cannot match — must be treated
        // as a tenant-local tail (and prepended).
        let kv = CfKvNamespaceReal::stub_for_native_tests(tp("tntLONGPREFIX"));
        let scoped = kv
            .scoped_key("x")
            .expect("short key must be treated as tail");
        assert_eq!(scoped.as_str(), "tntLONGPREFIX:x");
    }

    // ----- KvOp diagnostic stability -----

    #[test]
    fn kv_op_display_is_stable() {
        assert_eq!(KvOp::Get.as_str(), "get");
        assert_eq!(KvOp::Put.as_str(), "put");
        assert_eq!(KvOp::Delete.as_str(), "delete");
        assert_eq!(KvOp::List.as_str(), "list");
    }

    // ----- TTL clamp -----

    #[test]
    fn clamp_ttl_floors_below_minimum() {
        assert_eq!(CfKvNamespaceReal::clamp_ttl(Some(30)), Some(CF_KV_MIN_TTL_SECS));
        assert_eq!(CfKvNamespaceReal::clamp_ttl(Some(0)), Some(CF_KV_MIN_TTL_SECS));
    }

    #[test]
    fn clamp_ttl_passes_through_above_minimum() {
        assert_eq!(CfKvNamespaceReal::clamp_ttl(Some(3600)), Some(3600));
        assert_eq!(CfKvNamespaceReal::clamp_ttl(Some(60)), Some(60));
    }

    #[test]
    fn clamp_ttl_none_is_passthrough() {
        // None = "permanent until manually deleted" — must remain None.
        assert_eq!(CfKvNamespaceReal::clamp_ttl(None), None);
    }
}
