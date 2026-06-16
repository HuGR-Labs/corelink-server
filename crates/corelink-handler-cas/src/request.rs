//! Request / response envelopes for the CAS handler surface.

/// Read request — `GET /v1/cas/{tenant}/{hash}` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasReadRequest {
    /// Tenant from URL path.
    pub tenant: String,
    /// Lower-case hex content hash from URL path (canonical
    /// 64-char hex per `corelink_hash::CanonicalHash`).
    pub hash: String,
    /// Caller principal (already-authenticated upstream).
    pub principal: String,
    /// Caller's authenticated tenant (must equal `tenant` or the
    /// handler returns `CrossTenantDenied`).
    pub caller_tenant: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl CasReadRequest {
    /// Construct a [`CasReadRequest`] from its fields. Provided
    /// because the struct is `#[non_exhaustive]` per charter, which
    /// prevents struct-literal construction from outside the crate.
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        hash: impl Into<String>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            hash: hash.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            at_unix_ms,
        }
    }
}

/// Read response — bytes + the hash they were stored under.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasReadResponse {
    /// Bytes returned to the caller (already verified by the handler).
    pub bytes: Vec<u8>,
    /// Hash of the returned bytes (lowercase hex). Equal to the
    /// requested hash on the happy path.
    pub content_hash: String,
}

impl CasReadResponse {
    /// Construct a [`CasReadResponse`] from its fields.
    ///
    /// Provided because the struct is `#[non_exhaustive]`, which
    /// prevents struct-literal construction from outside this crate.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>, content_hash: impl Into<String>) -> Self {
        Self {
            bytes: bytes.into(),
            content_hash: content_hash.into(),
        }
    }
}

/// Write request — `PUT /v1/cas/{tenant}/{hash}` canonical shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasWriteRequest {
    /// Tenant from URL path.
    pub tenant: String,
    /// Lower-case hex content hash the caller claims for `bytes`.
    pub claimed_hash: String,
    /// Bytes to write.
    pub bytes: Vec<u8>,
    /// Caller principal (already-authenticated upstream).
    pub principal: String,
    /// Caller's authenticated tenant (must equal `tenant`).
    pub caller_tenant: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
    /// The tenant's **resolved per-tier storage cap in bytes**, as established
    /// by the quota-resolution authority (the Worker, via the server-trusted
    /// `x-corelink-storage-quota-bytes` header) and threaded down to the byte-
    /// accounting reservation. Semantics (the fresh-row seed source):
    ///
    /// - `Some(n)` with `n > 0` — a real finite cap. A fresh
    ///   `tenant_storage_state` row is seeded with `bytes_quota = n` (NOT the
    ///   legacy hard-coded `0`, which would mean "unlimited").
    /// - `Some(0)` — the tenant is on a **genuinely unlimited** tier
    ///   (enterprise/team); a fresh row is seeded with the `0` sentinel
    ///   deliberately.
    /// - `None` — the cap is **indeterminate** for this request (e.g. the
    ///   header was absent on a non-PAT-resolved surface). A fresh row then
    ///   FAILS CLOSED — it is NEVER created uncapped. An *existing* row keeps
    ///   its already-seeded cap (the reservation's UPDATE branch is unaffected).
    ///
    /// Defaults to `None` (set explicitly via [`Self::with_storage_quota_bytes`]
    /// by the route handler that reads the Worker header), so the 30+ existing
    /// `::new` call sites compile unchanged and inherit the fail-closed default.
    pub storage_quota_bytes: Option<i64>,
}

impl CasWriteRequest {
    /// Construct a [`CasWriteRequest`] from its fields.
    ///
    /// `storage_quota_bytes` defaults to `None` (indeterminate cap → fail-closed
    /// on a fresh row); set the resolved cap with
    /// [`Self::with_storage_quota_bytes`].
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        claimed_hash: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            claimed_hash: claimed_hash.into(),
            bytes: bytes.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            at_unix_ms,
            storage_quota_bytes: None,
        }
    }

    /// Attach the resolved per-tier storage cap (bytes) used to seed a fresh
    /// `tenant_storage_state` row in the byte-accounting reservation. See
    /// [`Self::storage_quota_bytes`] for the `Some(n)` / `Some(0)` / `None`
    /// semantics.
    #[must_use]
    pub fn with_storage_quota_bytes(mut self, cap: Option<i64>) -> Self {
        self.storage_quota_bytes = cap;
        self
    }
}

/// Delete request — `DELETE /v1/cas/{tenant}/{hash}` canonical shape.
///
/// DELETE is **idempotent**: deleting a present blob and deleting an
/// absent one both succeed (the route maps a successful delete to
/// HTTP 204 No Content in either case). The handler still enforces the
/// cross-tenant gate + emits the audit row exactly like the write path.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasDeleteRequest {
    /// Tenant from URL path.
    pub tenant: String,
    /// Lower-case hex content hash from URL path.
    pub hash: String,
    /// Caller principal (already-authenticated upstream).
    pub principal: String,
    /// Caller's authenticated tenant (must equal `tenant`).
    pub caller_tenant: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl CasDeleteRequest {
    /// Construct a [`CasDeleteRequest`] from its fields.
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        hash: impl Into<String>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            hash: hash.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            at_unix_ms,
        }
    }
}

/// Delete response — idempotent acknowledgement.
///
/// `existed` reports whether the blob was present before the delete (for
/// diagnostics / audit detail). The HTTP route returns 204 No Content
/// regardless of `existed` — DELETE is idempotent by contract.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasDeleteResponse {
    /// True if the blob existed (and was removed); false if it was
    /// already absent (idempotent no-op).
    pub existed: bool,
    /// Bytes reclaimed by the delete (the size of the removed blob), `0`
    /// when the blob was absent or its size could not be determined. The
    /// storage byte-accounting decorator releases exactly this many bytes
    /// from `tenant_storage_state.bytes_used` so a delete frees the tenant's
    /// headroom (red-team finding #1 / cluster-C — deletes that never
    /// decrement leak the cap forever).
    pub reclaimed_bytes: u64,
}

impl CasDeleteResponse {
    /// Construct a [`CasDeleteResponse`] reporting only existence (reclaimed
    /// size unknown ⇒ `0`). Kept for callers/tests that do not surface a
    /// size; prefer [`Self::with_reclaimed`] on the durable path so deletes
    /// release bytes.
    #[must_use]
    pub fn new(existed: bool) -> Self {
        Self {
            existed,
            reclaimed_bytes: 0,
        }
    }

    /// Construct a [`CasDeleteResponse`] carrying the reclaimed byte size.
    #[must_use]
    pub fn with_reclaimed(existed: bool, reclaimed_bytes: u64) -> Self {
        Self {
            existed,
            reclaimed_bytes,
        }
    }
}

/// List request — `GET /v1/cas/{tenant}` paginated blob enumeration.
///
/// `limit` is clamped to `1..=1000` by the route (default 200). `cursor`
/// is the opaque continuation token returned by a previous page (`None`
/// for the first page).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasListRequest {
    /// Tenant from URL path.
    pub tenant: String,
    /// Caller principal (already-authenticated upstream).
    pub principal: String,
    /// Caller's authenticated tenant (must equal `tenant`).
    pub caller_tenant: String,
    /// Max entries to return this page (route-clamped to `1..=1000`).
    pub limit: u32,
    /// Opaque pagination cursor from a prior page (`None` ⇒ first page).
    pub cursor: Option<String>,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl CasListRequest {
    /// Construct a [`CasListRequest`] from its fields.
    #[must_use]
    pub fn new(
        tenant: impl Into<String>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        limit: u32,
        cursor: Option<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            limit,
            cursor,
            at_unix_ms,
        }
    }
}

/// One enumerated CAS blob (the per-entry shape of the D-8 list body).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasBlobEntry {
    /// Lower-case hex content hash (the blob's CAS key, tenant-prefix
    /// stripped — never the raw storage key).
    pub hash: String,
    /// Object size in bytes.
    pub size: u64,
    /// RFC-3339 creation timestamp (the storage object's last-modified).
    pub created_at: String,
}

impl CasBlobEntry {
    /// Construct a [`CasBlobEntry`] from its fields.
    #[must_use]
    pub fn new(hash: impl Into<String>, size: u64, created_at: impl Into<String>) -> Self {
        Self {
            hash: hash.into(),
            size,
            created_at: created_at.into(),
        }
    }
}

/// List response — one page of blobs + an opaque continuation cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasListResponse {
    /// Blobs on this page (already tenant-scoped + hash-stripped).
    pub blobs: Vec<CasBlobEntry>,
    /// Opaque cursor for the next page, or `None` when exhausted.
    pub next_cursor: Option<String>,
}

impl CasListResponse {
    /// Construct a [`CasListResponse`] from its fields.
    #[must_use]
    pub fn new(blobs: Vec<CasBlobEntry>, next_cursor: Option<String>) -> Self {
        Self { blobs, next_cursor }
    }
}

/// Write response — durable content hash + persistence outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CasWriteResponse {
    /// Hash actually stored (equal to the claimed hash on success).
    pub content_hash: String,
    /// True if the write was a fresh insert; false if the object
    /// already existed (idempotent re-write).
    pub durable: bool,
}

impl CasWriteResponse {
    /// Construct a [`CasWriteResponse`] from its fields.
    ///
    /// Provided because the struct is `#[non_exhaustive]`, which
    /// prevents struct-literal construction from outside this crate.
    #[must_use]
    pub fn new(content_hash: impl Into<String>, durable: bool) -> Self {
        Self {
            content_hash: content_hash.into(),
            durable,
        }
    }
}
