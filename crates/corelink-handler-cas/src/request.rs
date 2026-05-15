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
}

impl CasWriteRequest {
    /// Construct a [`CasWriteRequest`] from its fields.
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
        }
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
