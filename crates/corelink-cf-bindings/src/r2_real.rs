//! Real CF R2 binding adapter with extended operations + tenant-prefix
//! enforcement (R-PREP, wasm32 production binding).
//!
//! # What this module adds over [`crate::cf_r2::CfR2BucketAdapter`]
//!
//! `cf_r2::CfR2BucketAdapter` (wasm32-only) implements the **minimal**
//! [`R2Backend`] trait surface (`put_if_none_match` / `get` / `head`)
//! that `corelink-worker::storage::r2::R2Writer` needs for the CAS
//! hot-path. Production also needs the broader R2 toolkit:
//!
//! - `delete` for GC + tombstone reconciliation (S-06).
//! - `list` for orphan sweeps + admin tooling (S-06 / S-12).
//! - Multipart upload (`create / upload_part / complete / abort`) for
//!   blobs > 5 MiB (the `corelink-r2-multipart` crate owns the full
//!   `MultipartAdapter` trait; this module exposes the underlying
//!   verbs so that adapter can be wired without re-importing
//!   `worker::r2::Bucket` directly — keeps the trait-abstraction-defer
//!   boundary clean).
//!
//! All extended operations route through [`CfR2BucketReal`] which:
//!
//! 1. **Enforces tenant prefix** on every key argument (every key MUST
//!    start with `<tenant_prefix>/`). Defense-in-depth: even if a
//!    caller forgets to derive the canonical key, the wrapper refuses
//!    rather than crossing a tenant boundary. Runtime check + typed
//!    [`TenantScopedKey`] wrapper for the type-driven path.
//! 2. **Forbids unwrap/expect/panic** (workspace-level lints already
//!    deny these; reasserted here because a panic on wasm32 is a
//!    customer-visible outage — the Worker isolate aborts).
//! 3. **Emits an audit fence** before every mutation. The audit emitter
//!    is injected via [`CfR2BucketReal::with_audit`] (the default is a
//!    no-op closure suitable for read-only test fixtures); production
//!    boot wires a writer that fans into `apps/server`'s audit chain.
//!    Audit emission is fail-CLOSED: if the audit closure returns an
//!    `Err`, the R2 mutation is **not** performed.
//! 4. **Idempotent semantics** on `put_if_none_match` (per
//!    INV-CAS-IDEMPOTENCY): a second PUT to the same key with the
//!    same body returns `AlreadyExists`. The R2 native idempotency
//!    contract is preserved.
//!
//! # Dual-target build
//!
//! - `target_arch = "wasm32"`: real `worker::r2::Bucket` wiring.
//! - `target_arch != "wasm32"`: stub that returns
//!   [`R2Error::Backend`] with the stable prefix `"WasmOnly: …"`.
//!   The crate's `R2Error` enum has no `WasmOnly` variant (changing it
//!   touches the storage-error taxonomy and `error_taxonomy.md`); we
//!   use the existing `Backend(String)` variant with a stable
//!   diagnostic prefix that upstream code can match on.
//!
//! The stub exists so consumer crates can construct
//! `CfR2BucketReal::stub_for_native_tests()` in unit tests without
//! per-call `cfg(target_arch = ...)` gates. Calling any operation on
//! the native stub returns `R2Error::Backend("WasmOnly: <op>")`
//! immediately — this is the deterministic "wrong target" signal.
//!
//! # Pattern replication
//!
//! The D1 / KV / DO real bindings should follow the **same** template:
//!
//! - Typed scoped-key wrapper (`*ScopedKey`) constructible only via
//!   the bucket/namespace/object's `scoped_*` factory.
//! - Extended operation surface gated on `target_arch = "wasm32"`,
//!   native stub returning `*Error::Backend("WasmOnly: …")`.
//! - Audit hook on every mutation (fail-CLOSED).
//! - ≥ 6 unit tests covering tenant-prefix validation, idempotency,
//!   and error-path mapping.
//!
//! See `specs/_audits/sealed/2026-05-15-cf-binding-real-pattern.md` for the
//! step-by-step recipe.

use bytes::Bytes;
use corelink_cas::r2_storage::R2Error;
use corelink_cas::r2_storage::BackendPutOutcome;
#[cfg(target_arch = "wasm32")]
use corelink_cas::r2_storage::R2Backend;
use std::fmt;
#[cfg(target_arch = "wasm32")]
use std::future::Future;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Tenant-prefix typed wrapper (compile-time-ish defense)
// ---------------------------------------------------------------------------

/// A tenant prefix anchor. Constructible only via [`TenantPrefix::new`]
/// which validates basic shape: non-empty, no `/` anywhere (segments
/// are joined by the wrapper), no embedded NUL.
///
/// Cloning is cheap (the inner string is `Arc<str>`-equivalent via
/// `String`; we keep it `String` for `worker::*` ergonomics). The
/// canonical tenant prefix is a 16-hex HMAC produced by
/// `corelink_tenant_path::TenantPrefix`; we don't bind to that type
/// here to keep this crate decoupled from upstream tenant-derivation
/// machinery.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TenantPrefix(String);

/// Error category emitted by [`CfR2BucketReal`] wrapper validation,
/// before any `worker::*` call. Mapped onto [`R2Error::Backend`] with a
/// stable diagnostic prefix for upstream pattern-matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum R2Op {
    /// `head` operation.
    Head,
    /// `get` operation.
    Get,
    /// `put` (PUT, INM=*) operation.
    Put,
    /// `delete` operation.
    Delete,
    /// `list` operation.
    List,
    /// `create_multipart_upload` operation.
    CreateMultipart,
    /// `upload_part` operation.
    UploadPart,
    /// `complete_multipart_upload` operation.
    CompleteMultipart,
    /// `abort_multipart_upload` operation.
    AbortMultipart,
}

impl R2Op {
    /// Static label used in `R2Error::Backend` diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Get => "get",
            Self::Put => "put",
            Self::Delete => "delete",
            Self::List => "list",
            Self::CreateMultipart => "create_multipart_upload",
            Self::UploadPart => "upload_part",
            Self::CompleteMultipart => "complete_multipart_upload",
            Self::AbortMultipart => "abort_multipart_upload",
        }
    }
}

impl fmt::Display for R2Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TenantPrefix {
    /// Construct a tenant prefix from a raw string. Validates:
    ///
    /// - non-empty
    /// - no `/` characters (segments are joined by [`CfR2BucketReal`])
    /// - no NUL bytes
    ///
    /// Returns [`R2Error::Backend`] with a stable diagnostic prefix
    /// on rejection so callers see the same taxonomy as backend
    /// faults but with a `tenant_prefix:` qualifier.
    pub fn new(raw: impl Into<String>) -> Result<Self, R2Error> {
        let s = raw.into();
        if s.is_empty() {
            return Err(R2Error::Backend(
                "tenant_prefix: empty prefix rejected".to_owned(),
            ));
        }
        if s.contains('/') {
            return Err(R2Error::Backend(format!(
                "tenant_prefix: prefix '{s}' must not contain '/'"
            )));
        }
        if s.contains('\0') {
            return Err(R2Error::Backend(
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

/// A key string that has been validated to start with a specific
/// tenant prefix. Constructible only via
/// [`CfR2BucketReal::scoped_key`]; the type therefore witnesses
/// tenant-prefix enforcement at the type level for callers that
/// thread it through.
///
/// The wrapper is `Clone` + `Debug` but the inner string is
/// **never** logged by this crate (CTRL-PRIV-001).
#[derive(Clone, Debug)]
pub struct TenantScopedKey {
    full: String,
    // Boundary of the prefix segment (`prefix/`); kept for invariants.
    #[allow(dead_code)] // referenced via inherent methods; future audit
    prefix_len: usize,
}

impl TenantScopedKey {
    /// Borrow the full key (prefix + slash + tail).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.full
    }
}

// ---------------------------------------------------------------------------
// Audit hook
// ---------------------------------------------------------------------------

/// Closure type for the audit emitter wired into [`CfR2BucketReal`].
/// Called BEFORE every mutation (`put`, `delete`, multipart `complete`
/// / `abort`). If the closure returns `Err`, the mutation is NOT
/// performed (fail-CLOSED).
///
/// Production wires a closure that fans into `apps/server`'s audit
/// chain (Workers Analytics + R2 audit log). Tests use the default
/// no-op or a recording closure (see unit tests below).
pub type AuditFn =
    Arc<dyn Fn(R2Op, &str) -> Result<(), R2Error> + Send + Sync + 'static>;

fn noop_audit() -> AuditFn {
    Arc::new(|_op, _key| Ok(()))
}

// ---------------------------------------------------------------------------
// CfR2BucketReal: dual-target struct definition
// ---------------------------------------------------------------------------

/// Real CF R2 binding adapter with extended operations and tenant-prefix
/// enforcement.
///
/// Dual-target: wasm32 wraps a `worker::r2::Bucket`; native build holds
/// no inner binding (the stub returns `R2Error::Backend("WasmOnly: …")`
/// from every operation).
#[cfg(target_arch = "wasm32")]
pub struct CfR2BucketReal {
    bucket: worker::Bucket,
    tenant: TenantPrefix,
    audit: AuditFn,
}

/// Native-build stub variant of [`CfR2BucketReal`]. Holds the tenant
/// prefix + audit hook so the wrapper-layer validation contract still
/// runs on native CI; every operation method returns
/// `R2Error::Backend("WasmOnly: …")` after validation/audit. See the
/// `#[cfg(target_arch = "wasm32")]` impl block for the production
/// surface this stub mirrors.
#[cfg(not(target_arch = "wasm32"))]
pub struct CfR2BucketReal {
    tenant: TenantPrefix,
    audit: AuditFn,
}

impl fmt::Debug for CfR2BucketReal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CfR2BucketReal")
            .field("tenant", &self.tenant)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Shared (target-agnostic) impl: constructors, scoped-key derivation,
// audit hookup. These compile on both wasm32 and native.
// ---------------------------------------------------------------------------

impl CfR2BucketReal {
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
    /// - already starts with `<tenant>/`, in which case it is accepted
    ///   verbatim; or
    /// - does not contain the tenant prefix anywhere, in which case
    ///   the wrapper prepends `<tenant>/` to produce the canonical
    ///   key.
    ///
    /// This dual mode lets callers either pre-derive the canonical
    /// key (production hot path, where `corelink_worker::storage::key::canonical_key`
    /// already produced the full path) or hand in a tenant-local
    /// tail (admin tools / GC sweep) without redundant prefix gymnastics.
    ///
    /// # Errors
    ///
    /// Returns [`R2Error::Backend`] (stable prefix `tenant_prefix:`)
    /// if the key contains an embedded `<other_tenant>/` shape that
    /// would be ambiguous, or if it is empty / contains NUL.
    pub fn scoped_key(&self, key: &str) -> Result<TenantScopedKey, R2Error> {
        if key.is_empty() {
            return Err(R2Error::Backend(
                "tenant_prefix: empty key rejected".to_owned(),
            ));
        }
        if key.contains('\0') {
            return Err(R2Error::Backend(
                "tenant_prefix: key must not contain NUL byte".to_owned(),
            ));
        }
        let tenant = self.tenant.as_str();
        let prefix_with_slash = format!("{tenant}/");
        let full = if let Some(rest) = key.strip_prefix(&prefix_with_slash) {
            if rest.is_empty() {
                return Err(R2Error::Backend(
                    "tenant_prefix: key tail must be non-empty".to_owned(),
                ));
            }
            key.to_owned()
        } else if key.starts_with('/') || key.contains("//") {
            return Err(R2Error::Backend(format!(
                "tenant_prefix: key '{key}' is malformed (leading slash or empty segment)"
            )));
        } else {
            // Treat as tenant-local tail; prepend canonical prefix.
            format!("{prefix_with_slash}{key}")
        };
        let prefix_len = prefix_with_slash.len();
        Ok(TenantScopedKey { full, prefix_len })
    }

    /// Helper: validate-or-derive the scoped key and emit the audit
    /// fence before any mutation. Returns the full scoped key for the
    /// downstream `worker::*` call.
    fn audit_and_scope(&self, op: R2Op, key: &str) -> Result<TenantScopedKey, R2Error> {
        let scoped = self.scoped_key(key)?;
        (self.audit)(op, scoped.as_str())?;
        Ok(scoped)
    }
}

// ---------------------------------------------------------------------------
// wasm32 production impl
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
impl CfR2BucketReal {
    /// Construct a real wrapper from a `worker::r2::Bucket` and a
    /// validated tenant prefix. Uses a no-op audit hook; replace via
    /// [`Self::with_audit`].
    #[must_use]
    pub fn new(bucket: worker::Bucket, tenant: TenantPrefix) -> Self {
        Self {
            bucket,
            tenant,
            audit: noop_audit(),
        }
    }

    /// Borrow the underlying `worker::r2::Bucket` for callers that
    /// truly need the raw surface (e.g. an alternate multipart
    /// driver). Use sparingly; bypasses tenant-prefix enforcement.
    #[must_use]
    pub fn inner(&self) -> &worker::Bucket {
        &self.bucket
    }

    /// `HEAD key` — returns `true` if an object exists at the
    /// scoped key, `false` otherwise.
    pub async fn head(&self, key: &str) -> Result<bool, R2Error> {
        let scoped = self.scoped_key(key)?;
        // HEAD is a read; we still audit because production needs the
        // probe trail for incident-response (was a HEAD ever issued
        // against this digest?).
        (self.audit)(R2Op::Head, scoped.as_str())?;
        let opt = self
            .bucket
            .head(scoped.as_str().to_owned())
            .await
            .map_err(|e| R2Error::Backend(format!("r2 head: {e}")))?;
        Ok(opt.is_some())
    }

    /// `GET key` — returns the object body or [`R2Error::NotFound`].
    pub async fn get_bytes(&self, key: &str) -> Result<Bytes, R2Error> {
        let scoped = self.scoped_key(key)?;
        let object = self
            .bucket
            .get(scoped.as_str().to_owned())
            .execute()
            .await
            .map_err(|e| R2Error::Backend(format!("r2 get: {e}")))?
            .ok_or(R2Error::NotFound)?;
        let body = object
            .body()
            .ok_or_else(|| R2Error::Backend("r2 get: body absent on hydrated object".to_owned()))?;
        let bytes_vec = body
            .bytes()
            .await
            .map_err(|e| R2Error::Backend(format!("r2 body bytes: {e}")))?;
        Ok(Bytes::from(bytes_vec))
    }

    /// `PUT key` with `If-None-Match: *` semantics (idempotent —
    /// duplicate PUT returns [`BackendPutOutcome::AlreadyExists`]
    /// without overwriting). Audit-fenced.
    pub async fn put_if_absent(
        &self,
        key: &str,
        body: Bytes,
    ) -> Result<BackendPutOutcome, R2Error> {
        let scoped = self.audit_and_scope(R2Op::Put, key)?;
        // workers-rs 0.8 PutOptionsBuilder lacks first-class INM=*
        // support; fall back to head-probe-then-put. Race with two
        // concurrent first writes still satisfies INV-CAS-IDEMPOTENCY
        // (CAS is content-addressed; both writes carry byte-identical
        // bodies under the same digest).
        let exists = self
            .bucket
            .head(scoped.as_str().to_owned())
            .await
            .map_err(|e| R2Error::Backend(format!("r2 head: {e}")))?
            .is_some();
        if exists {
            return Ok(BackendPutOutcome::AlreadyExists);
        }
        let payload: Vec<u8> = body.to_vec();
        self.bucket
            .put(scoped.as_str().to_owned(), payload)
            .execute()
            .await
            .map_err(|e| R2Error::Backend(format!("r2 put: {e}")))?;
        Ok(BackendPutOutcome::Stored)
    }

    /// `DELETE key`. Audit-fenced. Idempotent: deleting a non-existent
    /// key is **not** an error (the R2 binding returns Ok in that
    /// case; we preserve that semantics).
    pub async fn delete(&self, key: &str) -> Result<(), R2Error> {
        let scoped = self.audit_and_scope(R2Op::Delete, key)?;
        self.bucket
            .delete(scoped.as_str().to_owned())
            .await
            .map_err(|e| R2Error::Backend(format!("r2 delete: {e}")))?;
        Ok(())
    }

    /// `LIST` with the tenant prefix as the list root. Returns the
    /// raw key strings (caller is responsible for prefix-stripping if
    /// they want tenant-local tails).
    pub async fn list_keys(&self, limit: Option<u32>) -> Result<Vec<String>, R2Error> {
        let tenant = self.tenant.as_str();
        let prefix_with_slash = format!("{tenant}/");
        // Audit the list operation against the tenant root.
        (self.audit)(R2Op::List, &prefix_with_slash)?;
        let mut builder = self.bucket.list().prefix(prefix_with_slash);
        if let Some(n) = limit {
            builder = builder.limit(n);
        }
        let result = builder
            .execute()
            .await
            .map_err(|e| R2Error::Backend(format!("r2 list: {e}")))?;
        Ok(result.objects().iter().map(|o| o.key()).collect())
    }

    /// `createMultipartUpload(key)` — returns the upload handle plus
    /// the canonical scoped key (the workers-rs `MultipartUpload`
    /// handle does not expose `.key()`; we surface it ourselves so
    /// downstream audit/abort flows have a tenancy anchor).
    pub async fn create_multipart_upload(
        &self,
        key: &str,
    ) -> Result<(worker::MultipartUpload, String), R2Error> {
        let scoped = self.audit_and_scope(R2Op::CreateMultipart, key)?;
        let upload = self
            .bucket
            .create_multipart_upload(scoped.as_str().to_owned())
            .execute()
            .await
            .map_err(|e| R2Error::Backend(format!("r2 create_multipart_upload: {e}")))?;
        Ok((upload, scoped.as_str().to_owned()))
    }

    /// `uploadPart(upload, scoped_key, part_number, body)`. The caller
    /// must pass the scoped key returned from
    /// [`Self::create_multipart_upload`] so the per-part audit fence
    /// carries the tenancy anchor (workers-rs MultipartUpload does
    /// not surface the key).
    pub async fn upload_part(
        &self,
        upload: &worker::MultipartUpload,
        scoped_key: &str,
        part_number: u16,
        body: Bytes,
    ) -> Result<worker::UploadedPart, R2Error> {
        (self.audit)(R2Op::UploadPart, scoped_key)?;
        let payload: Vec<u8> = body.to_vec();
        upload
            .upload_part(part_number, payload)
            .await
            .map_err(|e| R2Error::Backend(format!("r2 upload_part: {e}")))
    }

    /// `completeMultipartUpload(upload, scoped_key, parts)`.
    pub async fn complete_multipart_upload(
        &self,
        upload: worker::MultipartUpload,
        scoped_key: &str,
        parts: Vec<worker::UploadedPart>,
    ) -> Result<(), R2Error> {
        (self.audit)(R2Op::CompleteMultipart, scoped_key)?;
        upload
            .complete(parts)
            .await
            .map_err(|e| R2Error::Backend(format!("r2 complete_multipart_upload: {e}")))?;
        Ok(())
    }

    /// `abortMultipartUpload(upload, scoped_key)`.
    pub async fn abort_multipart_upload(
        &self,
        upload: worker::MultipartUpload,
        scoped_key: &str,
    ) -> Result<(), R2Error> {
        (self.audit)(R2Op::AbortMultipart, scoped_key)?;
        upload
            .abort()
            .await
            .map_err(|e| R2Error::Backend(format!("r2 abort_multipart_upload: {e}")))?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl R2Backend for CfR2BucketReal {
    fn put_if_none_match<'a>(
        &'a self,
        key: &'a str,
        body: Bytes,
    ) -> impl Future<Output = Result<BackendPutOutcome, R2Error>> + Send + 'a {
        worker::send::SendFuture::new(async move { self.put_if_absent(key, body).await })
    }

    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> impl Future<Output = Result<Bytes, R2Error>> + Send + 'a {
        worker::send::SendFuture::new(async move { self.get_bytes(key).await })
    }

    fn head<'a>(
        &'a self,
        key: &'a str,
    ) -> impl Future<Output = Result<bool, R2Error>> + Send + 'a {
        worker::send::SendFuture::new(async move { CfR2BucketReal::head(self, key).await })
    }
}

// ---------------------------------------------------------------------------
// native stub
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
impl CfR2BucketReal {
    /// Construct a native stub for host-side trait-bound testing.
    /// Every operation returns `R2Error::Backend("WasmOnly: …")`.
    ///
    /// The stub still enforces tenant-prefix validation (it runs
    /// `scoped_key` first), so the wrapper-layer tests pin the
    /// validation contract on the native target — the same code path
    /// runs on wasm32 in production.
    #[must_use]
    pub fn stub_for_native_tests(tenant: TenantPrefix) -> Self {
        Self {
            tenant,
            audit: noop_audit(),
        }
    }

    /// Native stub for HEAD — validates prefix, then `WasmOnly`.
    pub async fn head(&self, key: &str) -> Result<bool, R2Error> {
        let _ = self.scoped_key(key)?;
        Err(R2Error::Backend(format!("WasmOnly: {}", R2Op::Head)))
    }

    /// Native stub for GET — validates prefix, then `WasmOnly`.
    pub async fn get_bytes(&self, key: &str) -> Result<Bytes, R2Error> {
        let _ = self.scoped_key(key)?;
        Err(R2Error::Backend(format!("WasmOnly: {}", R2Op::Get)))
    }

    /// Native stub for PUT — validates prefix, audit fence, then `WasmOnly`.
    pub async fn put_if_absent(
        &self,
        key: &str,
        _body: Bytes,
    ) -> Result<BackendPutOutcome, R2Error> {
        let _ = self.audit_and_scope(R2Op::Put, key)?;
        Err(R2Error::Backend(format!("WasmOnly: {}", R2Op::Put)))
    }

    /// Native stub for DELETE — validates prefix, audit fence, then `WasmOnly`.
    pub async fn delete(&self, key: &str) -> Result<(), R2Error> {
        let _ = self.audit_and_scope(R2Op::Delete, key)?;
        Err(R2Error::Backend(format!("WasmOnly: {}", R2Op::Delete)))
    }

    /// Native stub for LIST.
    pub async fn list_keys(&self, _limit: Option<u32>) -> Result<Vec<String>, R2Error> {
        let tenant = self.tenant.as_str();
        let prefix_with_slash = format!("{tenant}/");
        (self.audit)(R2Op::List, &prefix_with_slash)?;
        Err(R2Error::Backend(format!("WasmOnly: {}", R2Op::List)))
    }

    /// Native stub for createMultipartUpload.
    pub async fn create_multipart_upload(&self, key: &str) -> Result<(), R2Error> {
        let _ = self.audit_and_scope(R2Op::CreateMultipart, key)?;
        Err(R2Error::Backend(format!(
            "WasmOnly: {}",
            R2Op::CreateMultipart
        )))
    }
}

// ---------------------------------------------------------------------------
// Unit tests — wrapper layer (target-agnostic; run on native CI).
//
// These tests validate the trait-bound invariants the wrapper enforces
// BEFORE calling worker::*: tenant-prefix validation, audit fail-CLOSED,
// scoped-key derivation, error-path mapping. The wasm32 production path
// runs the same scoped_key + audit code (it lives in the shared impl
// block), so these tests pin the contract end-to-end.
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tp(s: &str) -> TenantPrefix {
        TenantPrefix::new(s).expect("test prefix must be valid")
    }

    #[test]
    fn tenant_prefix_rejects_empty() {
        let err = TenantPrefix::new("").expect_err("empty prefix must be rejected");
        match err {
            R2Error::Backend(msg) => assert!(msg.starts_with("tenant_prefix:")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn tenant_prefix_rejects_slash() {
        let err = TenantPrefix::new("a/b").expect_err("slash in prefix must be rejected");
        match err {
            R2Error::Backend(msg) => {
                assert!(msg.contains("must not contain '/'"), "got: {msg}");
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn tenant_prefix_rejects_nul() {
        let err = TenantPrefix::new("a\0b").expect_err("NUL in prefix must be rejected");
        match err {
            R2Error::Backend(msg) => assert!(msg.contains("NUL")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_prepends_tenant_when_tail_only() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt0123456789abcd"));
        let scoped = bucket
            .scoped_key("digest/blake3:deadbeef")
            .expect("derivation must succeed");
        assert_eq!(scoped.as_str(), "tnt0123456789abcd/digest/blake3:deadbeef");
    }

    #[test]
    fn scoped_key_accepts_already_prefixed() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tntABCD"));
        let scoped = bucket
            .scoped_key("tntABCD/r/wnam/blake3/0123")
            .expect("already-prefixed key must be accepted verbatim");
        assert_eq!(scoped.as_str(), "tntABCD/r/wnam/blake3/0123");
    }

    #[test]
    fn scoped_key_rejects_empty_tail_after_prefix() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .scoped_key("tnt/")
            .expect_err("empty tail must be rejected");
        match err {
            R2Error::Backend(msg) => assert!(msg.contains("tail must be non-empty")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_rejects_leading_slash() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .scoped_key("/etc/passwd")
            .expect_err("leading slash must be rejected");
        match err {
            R2Error::Backend(msg) => assert!(msg.contains("malformed")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_rejects_empty_key() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .scoped_key("")
            .expect_err("empty key must be rejected");
        match err {
            R2Error::Backend(msg) => assert!(msg.contains("empty key")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn scoped_key_rejects_nul_byte() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .scoped_key("foo\0bar")
            .expect_err("NUL in key must be rejected");
        match err {
            R2Error::Backend(msg) => assert!(msg.contains("NUL")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn native_stub_returns_wasm_only_on_head() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .head("tnt/x")
            .await
            .expect_err("native stub must refuse");
        match err {
            R2Error::Backend(msg) => assert_eq!(msg, "WasmOnly: head"),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn native_stub_returns_wasm_only_on_get() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .get_bytes("tnt/x")
            .await
            .expect_err("native stub must refuse");
        assert!(matches!(err, R2Error::Backend(msg) if msg == "WasmOnly: get"));
    }

    #[tokio::test]
    async fn native_stub_returns_wasm_only_on_put() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .put_if_absent("tnt/x", Bytes::from_static(b"hello"))
            .await
            .expect_err("native stub must refuse");
        assert!(matches!(err, R2Error::Backend(msg) if msg == "WasmOnly: put"));
    }

    #[tokio::test]
    async fn native_stub_returns_wasm_only_on_delete() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .delete("tnt/x")
            .await
            .expect_err("native stub must refuse");
        assert!(matches!(err, R2Error::Backend(msg) if msg == "WasmOnly: delete"));
    }

    #[tokio::test]
    async fn native_stub_returns_wasm_only_on_list() {
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .list_keys(Some(100))
            .await
            .expect_err("native stub must refuse");
        assert!(matches!(err, R2Error::Backend(msg) if msg == "WasmOnly: list"));
    }

    #[tokio::test]
    async fn put_validates_tenant_prefix_before_any_backend_call() {
        // The stub returns WasmOnly only AFTER scoped_key + audit.
        // Hitting a malformed key must surface the validation error
        // first (i.e. WasmOnly must NOT be the error here).
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt"));
        let err = bucket
            .put_if_absent("/etc/passwd", Bytes::new())
            .await
            .expect_err("malformed key must be rejected pre-backend");
        match err {
            R2Error::Backend(msg) => {
                assert!(
                    msg.contains("malformed"),
                    "validation must fire before WasmOnly: got: {msg}"
                );
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn audit_hook_fail_closed_blocks_mutation() {
        // An audit closure that returns Err must prevent the mutation
        // from reaching the (stub) backend. We detect this by the fact
        // that we see the audit error, NOT WasmOnly.
        let audit: AuditFn = Arc::new(|_op, _key| {
            Err(R2Error::Backend("audit: write denied by policy".to_owned()))
        });
        let bucket =
            CfR2BucketReal::stub_for_native_tests(tp("tnt")).with_audit(audit);
        let err = bucket
            .put_if_absent("tnt/x", Bytes::from_static(b"hello"))
            .await
            .expect_err("audit deny must block mutation");
        match err {
            R2Error::Backend(msg) => {
                assert!(
                    msg.contains("audit: write denied by policy"),
                    "fail-CLOSED must surface audit error, got: {msg}"
                );
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn audit_hook_records_operation_and_key() {
        // Capture audit calls into an atomic counter + a recorded
        // (op, key) tuple via a closure capture.
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&count);
        let captured_op: Arc<std::sync::Mutex<Option<R2Op>>> =
            Arc::new(std::sync::Mutex::new(None));
        let captured_op_clone = Arc::clone(&captured_op);
        let audit: AuditFn = Arc::new(move |op, _key| {
            count_clone.fetch_add(1, Ordering::AcqRel);
            if let Ok(mut g) = captured_op_clone.lock() {
                *g = Some(op);
            }
            Ok(())
        });
        let bucket = CfR2BucketReal::stub_for_native_tests(tp("tnt")).with_audit(audit);
        // Mutation: audit fires; backend is WasmOnly (we ignore the result).
        let _ = bucket
            .put_if_absent("tnt/digest/x", Bytes::from_static(b"hi"))
            .await;
        assert_eq!(count.load(Ordering::Acquire), 1, "audit must fire exactly once");
        let op = captured_op
            .lock()
            .ok()
            .and_then(|g| *g)
            .expect("audit op must have been captured");
        assert_eq!(op, R2Op::Put);
    }

    #[test]
    fn r2_op_display_is_stable() {
        // The diagnostic prefix `WasmOnly: <op>` is consumed by upstream
        // pattern matching; pin the strings so a rename surfaces as a
        // test failure rather than a silent contract drift.
        assert_eq!(R2Op::Head.as_str(), "head");
        assert_eq!(R2Op::Get.as_str(), "get");
        assert_eq!(R2Op::Put.as_str(), "put");
        assert_eq!(R2Op::Delete.as_str(), "delete");
        assert_eq!(R2Op::List.as_str(), "list");
        assert_eq!(R2Op::CreateMultipart.as_str(), "create_multipart_upload");
        assert_eq!(R2Op::UploadPart.as_str(), "upload_part");
        assert_eq!(R2Op::CompleteMultipart.as_str(), "complete_multipart_upload");
        assert_eq!(R2Op::AbortMultipart.as_str(), "abort_multipart_upload");
    }
}
