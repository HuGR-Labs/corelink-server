//! Authenticated tenant-scoped ingress boundary for future REAPI gRPC services.
//!
//! This module is deliberately not mounted as a public service. The route
//! factory returns these dependencies only alongside the same decorated CAS
//! and ActionCache handler trait objects used by the existing data plane. A
//! later service mount must wait for the Worker/DO/container transport proof
//! tracked by #2176.

use std::sync::Arc;

use async_trait::async_trait;
use corelink_handler_ac::{
    AcHandlerError, AcLookupHandler, AcLookupRequest, AcLookupResponse, AcUpdateHandler,
    AcUpdateRequest, AcUpdateResponse,
};
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler,
    CasWriteRequest, CasWriteResponse, DigestAlgo,
};
use tonic::{Code, Status};

#[path = "reapi_ingress/validation.rs"]
mod validation;
pub use validation::{
    sha256_digest, validate_blob_resource_name, validate_digest, validate_instance_name,
    ValidatedBlobResourceName,
};
#[path = "reapi_ingress/admission.rs"]
mod admission;
pub use admission::QuotaConcurrencyAdmission;
#[cfg(test)]
#[path = "reapi_ingress/tests.rs"]
mod tests;

use crate::adapter_pat::{PatVerifier, VerifyError};

/// Maximum concurrent ingress RPCs admitted by one container process.
pub const REAPI_INGRESS_CONCURRENCY_LIMIT: usize = 64;

/// Access required by a REAPI operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Read CAS or ActionCache data.
    Read,
    /// Mutate CAS or ActionCache data.
    Write,
}

/// Authenticated cache principal. It contains no bearer material and cannot be
/// constructed by service modules; only [`ReapiIngress::authorize`] creates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedTenant {
    tenant_id: String,
    can_write: bool,
}

impl AuthorizedTenant {
    /// Tenant identity proven by the D1-backed PAT verifier.
    #[must_use]
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Whether the verified PAT grants cache write access.
    #[must_use]
    pub const fn can_write(&self) -> bool {
        self.can_write
    }
}

/// Admission decision. Exhaustion is a client-visible capacity denial;
/// unavailability means the admission authority could not establish safety.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionFailure {
    /// A quota or concurrency ceiling has been reached.
    Exhausted,
    /// Admission state is unavailable or corrupt.
    Unavailable,
}

/// A lease held until a gRPC operation finishes.
#[derive(Debug)]
pub struct AdmissionLease {
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl AdmissionLease {
    fn new(permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        Self {
            _permit: Some(permit),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_only() -> Self {
        Self { _permit: None }
    }
}

/// Quota and concurrency admission seam shared by every REAPI service.
#[async_trait]
pub trait IngressAdmission: Send + Sync + std::fmt::Debug {
    /// Admit one authenticated RPC and return a lease held through completion.
    async fn admit(&self, tenant_id: &str) -> Result<AdmissionLease, AdmissionFailure>;
}

/// Authentication result before gRPC status mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthenticationFailure {
    /// Missing, malformed, invalid, revoked, or scope-less PAT.
    Invalid,
    /// Authoritative verifier backend failed or shed the request.
    Unavailable,
}

/// Minimal testable boundary around the authoritative PAT verifier.
#[async_trait]
pub trait IngressAuthenticator: Send + Sync + std::fmt::Debug {
    /// Verify a bearer PAT and return its tenant and cache-write capability.
    async fn authenticate(&self, bearer: &str) -> Result<(String, bool), AuthenticationFailure>;
}

/// Production adapter around the shared D1-backed `adapter_pat::PatVerifier`.
#[derive(Debug)]
pub struct PatIngressAuthenticator {
    verifier: Arc<PatVerifier>,
}

impl PatIngressAuthenticator {
    /// Bind ingress to the same authoritative PAT verifier used by adapters.
    #[must_use]
    pub fn new(verifier: Arc<PatVerifier>) -> Self {
        Self { verifier }
    }
}

#[async_trait]
impl IngressAuthenticator for PatIngressAuthenticator {
    async fn authenticate(&self, bearer: &str) -> Result<(String, bool), AuthenticationFailure> {
        self.verifier
            .verify_capability(bearer)
            .await
            .map_err(|error| match error {
                VerifyError::InvalidPat => AuthenticationFailure::Invalid,
                VerifyError::Backend(_) => AuthenticationFailure::Unavailable,
            })
    }
}

/// One tenant-scoped, quota- and concurrency-admitted REAPI request.
#[derive(Debug)]
pub struct AdmittedIngress {
    principal: AuthorizedTenant,
    access: Access,
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    ac_lookup: Arc<dyn AcLookupHandler>,
    ac_update: Arc<dyn AcUpdateHandler>,
    cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver>,
    _lease: AdmissionLease,
}

impl AdmittedIngress {
    /// Authenticated tenant for handler requests.
    #[must_use]
    pub fn tenant(&self) -> &AuthorizedTenant {
        &self.principal
    }

    /// Read one tenant-scoped CAS blob through the shared decorated handler.
    pub fn cas_read(&self, hash: &str, size_bytes: i64) -> Result<CasReadResponse, Status> {
        validate_digest(hash, size_bytes)?;
        let max_bytes = u64::try_from(size_bytes)
            .map_err(|_| Status::new(Code::InvalidArgument, "invalid CAS digest size"))?;
        let tenant = self.principal.tenant_id.clone();
        let request = CasReadRequest::new(
            tenant.clone(),
            hash,
            tenant.clone(),
            tenant,
            current_unix_ms(),
        )
        .with_algo(DigestAlgo::Sha256)
        .with_max_bytes(max_bytes);
        let response = self.cas_read.read(request).map_err(cas_error_status)?;
        let actual_size = i64::try_from(response.bytes.len())
            .map_err(|_| Status::new(Code::DataLoss, "CAS response exceeds digest size range"))?;
        if actual_size != size_bytes {
            return Err(Status::new(
                Code::DataLoss,
                "CAS response size does not match digest",
            ));
        }
        Ok(response)
    }

    /// Store one tenant-scoped CAS blob through the shared decorated handler.
    pub async fn cas_write(
        &self,
        hash: &str,
        size_bytes: i64,
        bytes: Vec<u8>,
    ) -> Result<CasWriteResponse, Status> {
        self.require_write_access()?;
        validate_digest(hash, size_bytes)?;
        let actual_size = i64::try_from(bytes.len())
            .map_err(|_| Status::new(Code::ResourceExhausted, "CAS payload is too large"))?;
        if actual_size != size_bytes || sha256_digest(&bytes) != hash {
            return Err(Status::new(
                Code::InvalidArgument,
                "CAS digest does not match payload",
            ));
        }
        let tenant = self.principal.tenant_id.clone();
        let storage_cap = self
            .cap_resolver
            .resolve_storage_cap(&tenant)
            .await
            .ok_or_else(|| Status::new(Code::Unavailable, "storage quota authority unavailable"))?;
        let request = CasWriteRequest::new(
            tenant.clone(),
            hash,
            bytes,
            tenant.clone(),
            tenant,
            current_unix_ms(),
        )
        .with_storage_quota_bytes(Some(storage_cap))
        .with_algo(DigestAlgo::Sha256);
        self.cas_write.write(request).map_err(cas_error_status)
    }

    /// Look up a tenant-scoped ActionCache entry through the shared handler.
    pub fn ac_lookup(
        &self,
        action_hash: &str,
        action_size_bytes: i64,
    ) -> Result<AcLookupResponse, Status> {
        validate_digest(action_hash, action_size_bytes)?;
        let tenant = self.principal.tenant_id.clone();
        let request = AcLookupRequest::new(
            tenant.clone(),
            action_hash,
            tenant.clone(),
            tenant,
            current_unix_ms(),
        );
        self.ac_lookup.lookup(request).map_err(ac_error_status)
    }

    /// Update a tenant-scoped ActionCache entry through the shared handler.
    pub async fn ac_update(
        &self,
        action_hash: &str,
        action_size_bytes: i64,
        result_payload: Vec<u8>,
    ) -> Result<AcUpdateResponse, Status> {
        self.require_write_access()?;
        validate_digest(action_hash, action_size_bytes)?;
        let tenant = self.principal.tenant_id.clone();
        let storage_cap = self
            .cap_resolver
            .resolve_storage_cap(&tenant)
            .await
            .ok_or_else(|| Status::new(Code::Unavailable, "storage quota authority unavailable"))?;
        let request = AcUpdateRequest::new(
            tenant.clone(),
            action_hash,
            result_payload,
            tenant.clone(),
            tenant,
            current_unix_ms(),
        )
        .with_storage_quota_bytes(Some(storage_cap));
        self.ac_update.update(request).map_err(ac_error_status)
    }

    fn require_write_access(&self) -> Result<(), Status> {
        if self.access == Access::Write && self.principal.can_write {
            Ok(())
        } else {
            Err(Status::new(
                Code::PermissionDenied,
                "cache write scope required",
            ))
        }
    }
}

fn current_unix_ms() -> u64 {
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX));
    u64::try_from(unix_ms).unwrap_or(u64::MAX)
}

fn cas_error_status(error: CasHandlerError) -> Status {
    match error {
        CasHandlerError::NotFound { .. } => Status::new(Code::NotFound, "CAS object not found"),
        CasHandlerError::HashMismatch { .. } => Status::new(Code::DataLoss, "CAS digest mismatch"),
        CasHandlerError::CrossTenantDenied { .. } => {
            Status::new(Code::PermissionDenied, "tenant access denied")
        }
        CasHandlerError::ObjectTooLarge { .. } => Status::new(
            Code::ResourceExhausted,
            "CAS object exceeds the serving limit",
        ),
        CasHandlerError::AuditFailed(_) | CasHandlerError::Internal(_) => {
            Status::new(Code::Unavailable, "CAS handler unavailable")
        }
        _ => Status::new(Code::Unavailable, "CAS handler unavailable"),
    }
}

fn ac_error_status(error: AcHandlerError) -> Status {
    match error {
        AcHandlerError::Miss { .. } => Status::new(Code::NotFound, "ActionCache entry not found"),
        AcHandlerError::CrossTenantDenied { .. } => {
            Status::new(Code::PermissionDenied, "tenant access denied")
        }
        AcHandlerError::DivergentBody { .. } => {
            Status::new(Code::FailedPrecondition, "ActionCache entry is immutable")
        }
        AcHandlerError::AuditFailed(_) | AcHandlerError::Internal(_) => {
            Status::new(Code::Unavailable, "ActionCache handler unavailable")
        }
        _ => Status::new(Code::Unavailable, "ActionCache handler unavailable"),
    }
}

/// Typed shared ingress dependencies built from decorated production handlers.
#[derive(Clone)]
pub struct ReapiIngress {
    authenticator: Arc<dyn IngressAuthenticator>,
    admission: Arc<dyn IngressAdmission>,
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    ac_lookup: Arc<dyn AcLookupHandler>,
    ac_update: Arc<dyn AcUpdateHandler>,
    cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver>,
    #[cfg(test)]
    admission_calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl std::fmt::Debug for ReapiIngress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReapiIngress")
            .field("authenticator", &self.authenticator)
            .field("admission", &self.admission)
            .field("cas_read", &self.cas_read)
            .field("cas_write", &self.cas_write)
            .field("ac_lookup", &self.ac_lookup)
            .field("ac_update", &self.ac_update)
            .field("cap_resolver", &self.cap_resolver)
            .finish()
    }
}

impl ReapiIngress {
    /// Construct a bundle from the shared production-decorated handler objects.
    /// The route factory is the intended caller; this constructor takes the
    /// exact trait objects already consumed by its native routes.
    #[must_use]
    pub(crate) fn from_shared_handlers(
        verifier: Arc<PatVerifier>,
        admission: Arc<dyn IngressAdmission>,
        cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver>,
        cas_read: Arc<dyn CasReadHandler>,
        cas_write: Arc<dyn CasWriteHandler>,
        ac_lookup: Arc<dyn AcLookupHandler>,
        ac_update: Arc<dyn AcUpdateHandler>,
    ) -> Self {
        Self {
            authenticator: Arc::new(PatIngressAuthenticator::new(verifier)),
            admission,
            cas_read,
            cas_write,
            ac_lookup,
            ac_update,
            cap_resolver,
            #[cfg(test)]
            admission_calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Build an ingress from explicit test doubles in sibling crate tests.
    ///
    /// This seam is unavailable to production code: route construction must
    /// continue through [`Self::from_shared_handlers`].
    #[cfg(test)]
    #[must_use]
    pub(crate) fn from_test_components(
        authenticator: Arc<dyn IngressAuthenticator>,
        admission: Arc<dyn IngressAdmission>,
        cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver>,
        cas_read: Arc<dyn CasReadHandler>,
        cas_write: Arc<dyn CasWriteHandler>,
        ac_lookup: Arc<dyn AcLookupHandler>,
        ac_update: Arc<dyn AcUpdateHandler>,
    ) -> Self {
        Self {
            authenticator,
            admission,
            cas_read,
            cas_write,
            ac_lookup,
            ac_update,
            cap_resolver,
            admission_calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Construct an ingress identity and reserve admission before storage work.
    /// The bearer is borrowed only for verification and is never copied into
    /// the returned request context.
    pub async fn authorize(
        &self,
        metadata: &tonic::metadata::MetadataMap,
        instance_name: &str,
        access: Access,
    ) -> Result<AdmittedIngress, Status> {
        let bearer = bearer_from_metadata(metadata)?;
        let (tenant_id, can_write) = self
            .authenticator
            .authenticate(bearer)
            .await
            .map_err(authentication_status)?;

        if crate::auth_tenant::is_reserved_sentinel(&tenant_id) {
            return Err(Status::new(Code::Unauthenticated, "invalid credentials"));
        }
        validate_instance_name(instance_name, &tenant_id)?;
        if access == Access::Write && !can_write {
            return Err(Status::new(
                Code::PermissionDenied,
                "cache write scope required",
            ));
        }

        let lease = self
            .admission
            .admit(&tenant_id)
            .await
            .map_err(admission_status)?;
        Ok(AdmittedIngress {
            principal: AuthorizedTenant {
                tenant_id,
                can_write,
            },
            access,
            cas_read: self.cas_read.clone(),
            cas_write: self.cas_write.clone(),
            ac_lookup: self.ac_lookup.clone(),
            ac_update: self.ac_update.clone(),
            cap_resolver: self.cap_resolver.clone(),
            _lease: lease,
        })
    }
}

pub(super) fn bearer_from_metadata(
    metadata: &tonic::metadata::MetadataMap,
) -> Result<&str, Status> {
    let mut values = metadata.get_all("authorization").iter();
    let value = values
        .next()
        .and_then(|header| header.to_str().ok())
        .ok_or_else(|| Status::new(Code::Unauthenticated, "authentication required"))?;
    if values.next().is_some() {
        return Err(Status::new(
            Code::Unauthenticated,
            "authentication required",
        ));
    }
    let (_, bearer) = value
        .split_once(' ')
        .filter(|(scheme, token)| {
            scheme.eq_ignore_ascii_case("bearer")
                && !token.is_empty()
                && !token.bytes().any(|byte| byte.is_ascii_whitespace())
        })
        .ok_or_else(|| Status::new(Code::Unauthenticated, "authentication required"))?;
    Ok(bearer)
}

fn authentication_status(error: AuthenticationFailure) -> Status {
    match error {
        AuthenticationFailure::Invalid => Status::new(Code::Unauthenticated, "invalid credentials"),
        AuthenticationFailure::Unavailable => {
            Status::new(Code::Unavailable, "authentication service unavailable")
        }
    }
}

fn admission_status(error: AdmissionFailure) -> Status {
    match error {
        AdmissionFailure::Exhausted => {
            Status::new(Code::ResourceExhausted, "cache admission limit reached")
        }
        AdmissionFailure::Unavailable => {
            Status::new(Code::Unavailable, "cache admission service unavailable")
        }
    }
}
