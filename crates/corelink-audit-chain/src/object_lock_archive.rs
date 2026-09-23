//! Provider-neutral contract for a storage-enforced immutable audit archive.
//!
//! This module supplies no provider credentials, provisioning, or network
//! implementation. A future S3 Object Lock or GCS retention adapter must prove
//! this contract for its exact provider account before Compliance retention can
//! use it. The existing R2 archive is a separate, non-WORM path.
//!
//! [`VerifiedObjectLockArchive`] is the only write surface supplied here. It
//! accepts an adapter only after capability negotiation and verifies retention,
//! legal hold, residency, and audit receipt after each write. There is no
//! secondary storage fallback.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Version of the immutable archive conformance contract.
pub const OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION: u32 = 1;

/// Storage API family used by one archive backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ObjectLockBackendFamily {
    /// An S3-compatible Object Lock implementation.
    S3,
    /// A Google Cloud Storage retention-policy implementation.
    Gcs,
    /// Another provider that must satisfy this same contract.
    Other,
}

/// Non-secret identity for one exact archive target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectLockBackendIdentity {
    /// Storage API family.
    pub family: ObjectLockBackendFamily,
    /// Provider or installation name.
    pub provider: String,
    /// Archive target name included in receipts.
    pub archive_target: String,
}

impl ObjectLockBackendIdentity {
    /// Construct a provider-neutral backend identity.
    #[must_use]
    pub fn new(
        family: ObjectLockBackendFamily,
        provider: impl Into<String>,
        archive_target: impl Into<String>,
    ) -> Self {
        Self {
            family,
            provider: provider.into(),
            archive_target: archive_target.into(),
        }
    }
}

/// Legal-hold state read back from a provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LegalHold {
    /// The provider reports an active legal hold.
    On,
    /// The provider reports no active legal hold.
    Off,
}

/// Retention state an immutable write must request and read back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImmutableRetention {
    /// UTC Unix timestamp after which deletion may become eligible.
    pub retain_until_unix_ms: u64,
    /// Legal-hold state that must be reported after the write.
    pub legal_hold: LegalHold,
}

/// Provider residency metadata for an archive target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveResidency {
    /// Customer or regulatory jurisdiction, such as `EU`.
    pub jurisdiction: String,
    /// Provider region or multi-region identifier.
    pub region: String,
    /// Provider location-class or placement description.
    pub location_class: String,
}

impl ArchiveResidency {
    /// Construct non-secret residency metadata.
    #[must_use]
    pub fn new(
        jurisdiction: impl Into<String>,
        region: impl Into<String>,
        location_class: impl Into<String>,
    ) -> Self {
        Self {
            jurisdiction: jurisdiction.into(),
            region: region.into(),
            location_class: location_class.into(),
        }
    }

    fn is_complete(&self) -> bool {
        !self.jurisdiction.trim().is_empty()
            && !self.region.trim().is_empty()
            && !self.location_class.trim().is_empty()
    }
}

/// Required capabilities for a Compliance immutable archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectLockCapabilities {
    /// The backend can create storage-enforced Compliance retention.
    pub immutable_put: bool,
    /// The backend can read back the exact retain-until instant.
    pub retain_until_readback: bool,
    /// The backend can read back legal-hold state.
    pub legal_hold_readback: bool,
    /// The backend can demonstrate pre-expiry delete denial.
    pub delete_denial: bool,
    /// The backend returns residency metadata.
    pub residency_metadata: bool,
    /// The backend returns a durable audit receipt.
    pub audit_receipt: bool,
}

impl ObjectLockCapabilities {
    /// Return the complete mandatory capability set.
    #[must_use]
    pub const fn required() -> Self {
        Self {
            immutable_put: true,
            retain_until_readback: true,
            legal_hold_readback: true,
            delete_denial: true,
            residency_metadata: true,
            audit_receipt: true,
        }
    }

    /// Return the capability names a report still lacks.
    #[must_use]
    pub fn missing(self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.immutable_put {
            missing.push("immutable_put");
        }
        if !self.retain_until_readback {
            missing.push("retain_until_readback");
        }
        if !self.legal_hold_readback {
            missing.push("legal_hold_readback");
        }
        if !self.delete_denial {
            missing.push("delete_denial");
        }
        if !self.residency_metadata {
            missing.push("residency_metadata");
        }
        if !self.audit_receipt {
            missing.push("audit_receipt");
        }
        missing
    }
}

/// Result of capability negotiation for one exact archive target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectLockCapabilityReport {
    /// Contract version evaluated by the adapter.
    pub contract_version: u32,
    /// Exact archive backend evaluated.
    pub backend: ObjectLockBackendIdentity,
    /// Capabilities proven for that target.
    pub capabilities: ObjectLockCapabilities,
    /// Non-secret evidence reference, such as a provider request identifier.
    pub evidence_reference: String,
}

/// Immutable object payload and required post-write state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableArchivePut {
    /// Authenticated tenant whose trusted routing selected this archive target.
    pub tenant_id: String,
    /// Provider-neutral object key.
    pub object_key: String,
    /// Bytes to archive.
    pub body: Vec<u8>,
    /// Required retention and legal-hold settings.
    pub retention: ImmutableRetention,
    /// Residency required for this archive write.
    pub expected_residency: ArchiveResidency,
}

/// Provider readback after an immutable write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectLockRetentionReadback {
    /// Exact provider object version read back from storage.
    pub object_version: String,
    /// Object key read back from the provider.
    pub object_key: String,
    /// Storage-enforced retention and hold state.
    pub retention: ImmutableRetention,
    /// Object residency metadata.
    pub residency: ArchiveResidency,
}

/// Receipt metadata for the provider immutable-write operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveAuditReceipt {
    /// Provider-issued or externally durable audit-event identifier.
    pub receipt_id: String,
    /// Exact provider bucket covered by the durable receipt.
    pub bucket: String,
    /// Exact provider object version covered by the durable receipt.
    pub object_version: String,
    /// Object key covered by the receipt.
    pub object_key: String,
    /// Local adapter clock observation immediately after write success.
    /// This is not a provider timestamp or a durable audit-event time.
    pub observed_at_unix_ms: u64,
}

/// Provider result after it accepts an immutable write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableArchiveWriteReceipt {
    /// Exact bucket receiving the immutable object.
    pub bucket: String,
    /// Object key accepted by the provider.
    pub object_key: String,
    /// Exact immutable version written by the provider.
    pub object_version: String,
    /// Provider audit receipt for the immutable write.
    pub audit_receipt: ArchiveAuditReceipt,
}

/// Result of a delete attempt before retention expiry.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeleteAttempt {
    /// The provider denied deletion due to retention or legal hold.
    Denied {
        /// Object whose deletion was denied.
        object_key: String,
        /// Provider denial reason or code, without credentials.
        reason: String,
    },
    /// The provider allowed deletion; this violates the contract.
    Deleted,
}

/// Error emitted by the adapter contract or verified wrapper.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ObjectLockArchiveError {
    /// Capability negotiation could not prove the contract.
    CapabilityNegotiationFailed(String),
    /// The capability report omitted mandatory capabilities.
    RequiredCapabilityMissing(Vec<&'static str>),
    /// The provider adapter rejected an operation.
    Backend(String),
    /// A provider response did not match the immutable-write request.
    ReadbackMismatch(String),
    /// The provider allowed a delete that Object Lock must deny.
    DeleteWasAllowed(String),
    /// The in-memory conformance fake encountered a poisoned mutex.
    TestFixture(String),
}

impl core::fmt::Display for ObjectLockArchiveError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CapabilityNegotiationFailed(reason) => {
                write!(formatter, "Object-Lock negotiation failed: {reason}")
            }
            Self::RequiredCapabilityMissing(missing) => {
                write!(formatter, "Object-Lock report is incomplete: {missing:?}")
            }
            Self::Backend(reason) => write!(formatter, "Object-Lock backend error: {reason}"),
            Self::ReadbackMismatch(reason) => {
                write!(formatter, "Object-Lock readback did not verify: {reason}")
            }
            Self::DeleteWasAllowed(key) => {
                write!(formatter, "Object-Lock backend allowed deletion of {key}")
            }
            Self::TestFixture(reason) => {
                write!(formatter, "Object-Lock test fixture error: {reason}")
            }
        }
    }
}

impl std::error::Error for ObjectLockArchiveError {}

/// Provider-neutral port for a storage-enforced immutable archive.
///
/// Implementors represent one exact provider target. Production callers must
/// construct [`VerifiedObjectLockArchive`] rather than calling this port
/// directly; the wrapper makes failed negotiation and readback errors visible
/// and supplies no fallback storage path.
pub trait ObjectLockArchiveAdapter: Send + Sync + core::fmt::Debug {
    /// Prove the required capability set for the exact target.
    fn negotiate_capabilities(&self) -> Result<ObjectLockCapabilityReport, ObjectLockArchiveError>;

    /// Put one object with storage-enforced immutable retention.
    fn put_immutable(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<ImmutableArchiveWriteReceipt, ObjectLockArchiveError>;

    /// Read provider retention, legal-hold, and residency state.
    fn read_retention(
        &self,
        object_key: &str,
    ) -> Result<ObjectLockRetentionReadback, ObjectLockArchiveError>;

    /// Attempt deletion to prove it is denied while retention or hold applies.
    fn attempt_delete(&self, object_key: &str) -> Result<DeleteAttempt, ObjectLockArchiveError>;

    /// Persist a failure observed while verifying a provider operation.
    /// Implementations with durable audit requirements must bind this record
    /// to the exact object version when one is known.
    fn record_failure(
        &self,
        _operation: &'static str,
        _object_key: &str,
        _object_version: Option<&str>,
        _failure_class: &'static str,
    ) -> Result<(), ObjectLockArchiveError> {
        Err(ObjectLockArchiveError::Backend(
            "archive adapter does not support durable failure audit".to_string(),
        ))
    }
}

/// Negotiated immutable archive surface with no fallback backend.
#[derive(Debug)]
pub struct VerifiedObjectLockArchive<A> {
    adapter: A,
    capability_report: ObjectLockCapabilityReport,
}

impl<A> VerifiedObjectLockArchive<A>
where
    A: ObjectLockArchiveAdapter,
{
    /// Negotiate every required capability before accepting immutable writes.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider cannot prove the complete contract.
    pub fn connect(adapter: A) -> Result<Self, ObjectLockArchiveError> {
        let capability_report = match adapter.negotiate_capabilities() {
            Ok(report) => report,
            Err(error) => {
                adapter.record_failure(
                    "capability_negotiation",
                    "",
                    None,
                    "provider_or_audit_failure",
                )?;
                return Err(error);
            }
        };
        if capability_report.contract_version != OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "contract version mismatch".to_string(),
            ));
        }
        if capability_report.backend.provider.trim().is_empty()
            || capability_report.backend.archive_target.trim().is_empty()
            || capability_report.evidence_reference.trim().is_empty()
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "backend identity or evidence reference was empty".to_string(),
            ));
        }
        let missing = capability_report.capabilities.missing();
        if !missing.is_empty() {
            return Err(ObjectLockArchiveError::RequiredCapabilityMissing(missing));
        }
        Ok(Self {
            adapter,
            capability_report,
        })
    }

    /// Return the evidence captured during successful negotiation.
    #[must_use]
    pub fn capability_report(&self) -> &ObjectLockCapabilityReport {
        &self.capability_report
    }

    /// Write one immutable object and verify the provider's readback.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider declines the write or readback differs
    /// from the required retention, legal-hold, residency, or audit receipt.
    pub fn put_immutable(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<VerifiedImmutableArchiveReceipt, ObjectLockArchiveError> {
        if request.tenant_id.trim().is_empty()
            || request.object_key.trim().is_empty()
            || request.body.is_empty()
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "immutable archive write requires a tenant, non-empty key, and body".to_string(),
            ));
        }
        if request.retention.retain_until_unix_ms == 0 || !request.expected_residency.is_complete()
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "immutable archive write requires retention and complete residency".to_string(),
            ));
        }
        let write_receipt = match self.adapter.put_immutable(request) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.adapter.record_failure(
                    "immutable_put",
                    &request.object_key,
                    None,
                    "provider_or_audit_failure",
                )?;
                return Err(error);
            }
        };
        if write_receipt.object_key != request.object_key
            || write_receipt.bucket.trim().is_empty()
            || write_receipt.audit_receipt.bucket != write_receipt.bucket
            || write_receipt.object_version.trim().is_empty()
            || write_receipt.audit_receipt.object_key != request.object_key
            || write_receipt.audit_receipt.object_version != write_receipt.object_version
            || write_receipt.audit_receipt.receipt_id.trim().is_empty()
            || write_receipt.audit_receipt.observed_at_unix_ms == 0
        {
            self.adapter.record_failure(
                "immutable_put_verification",
                &request.object_key,
                Some(&write_receipt.object_version),
                "write_receipt_mismatch",
            )?;
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "write receipt did not cover the requested object".to_string(),
            ));
        }
        let retention_readback = match self.adapter.read_retention(&request.object_key) {
            Ok(readback) => readback,
            Err(error) => {
                self.adapter.record_failure(
                    "retention_readback",
                    &request.object_key,
                    Some(&write_receipt.object_version),
                    "provider_or_audit_failure",
                )?;
                return Err(error);
            }
        };
        if retention_readback.object_key != request.object_key
            || retention_readback.object_version != write_receipt.object_version
            || retention_readback.object_version.trim().is_empty()
            || retention_readback.retention != request.retention
            || retention_readback.residency != request.expected_residency
            || !retention_readback.residency.is_complete()
        {
            self.adapter.record_failure(
                "retention_readback_verification",
                &request.object_key,
                Some(&write_receipt.object_version),
                "readback_mismatch",
            )?;
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "provider retention, legal hold, or residency did not match the request"
                    .to_string(),
            ));
        }
        Ok(VerifiedImmutableArchiveReceipt {
            backend: self.capability_report.backend.clone(),
            write_receipt,
            retention_readback,
        })
    }

    /// Prove that the provider denies deletion before retention expiry.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectLockArchiveError::DeleteWasAllowed`] when a backend
    /// permits deletion, making it unusable for immutable retention.
    pub fn assert_delete_denied(
        &self,
        object_key: &str,
    ) -> Result<DeleteDenialReceipt, ObjectLockArchiveError> {
        if object_key.trim().is_empty() {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "delete-denial proof requires a non-empty object key".to_string(),
            ));
        }
        let attempt = match self.adapter.attempt_delete(object_key) {
            Ok(attempt) => attempt,
            Err(error) => {
                self.adapter.record_failure(
                    "pre_expiry_delete_denial",
                    object_key,
                    None,
                    "provider_or_audit_failure",
                )?;
                return Err(error);
            }
        };
        match attempt {
            DeleteAttempt::Denied {
                object_key: denied_object_key,
                reason,
            } if denied_object_key == object_key && !reason.trim().is_empty() => {
                Ok(DeleteDenialReceipt {
                    object_key: denied_object_key,
                    reason,
                })
            }
            DeleteAttempt::Denied {
                object_key: denied_object_key,
                ..
            } if denied_object_key != object_key => {
                self.adapter.record_failure(
                    "pre_expiry_delete_denial",
                    object_key,
                    None,
                    "wrong_object_denied",
                )?;
                Err(ObjectLockArchiveError::ReadbackMismatch(
                    "delete denial covered a different object than requested".to_string(),
                ))
            }
            DeleteAttempt::Denied { .. } => {
                self.adapter.record_failure(
                    "pre_expiry_delete_denial",
                    object_key,
                    None,
                    "missing_provider_reason",
                )?;
                Err(ObjectLockArchiveError::ReadbackMismatch(
                    "delete denial did not include a provider reason".to_string(),
                ))
            }
            DeleteAttempt::Deleted => {
                self.adapter.record_failure(
                    "pre_expiry_delete_denial",
                    object_key,
                    None,
                    "delete_was_allowed",
                )?;
                Err(ObjectLockArchiveError::DeleteWasAllowed(
                    object_key.to_string(),
                ))
            }
        }
    }
}

/// Successful immutable-write evidence after provider readback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedImmutableArchiveReceipt {
    /// Backend negotiated before the write.
    pub backend: ObjectLockBackendIdentity,
    /// Provider receipt for the write operation.
    pub write_receipt: ImmutableArchiveWriteReceipt,
    /// Post-write retention, legal-hold, and residency readback.
    pub retention_readback: ObjectLockRetentionReadback,
}

/// Evidence that a provider denied deletion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteDenialReceipt {
    /// Object whose deletion was denied.
    pub object_key: String,
    /// Provider denial reason or code.
    pub reason: String,
}

/// Explicitly unavailable Object-Lock adapter for the current Cloudflare R2 path.
///
/// This type has no credentials and does not contact Cloudflare. It records the
/// documented capability result so verified construction fails before an
/// immutable put can run.
#[derive(Clone, Debug, Default)]
pub struct R2ObjectLockUnavailable {
    immutable_put_calls: Arc<Mutex<u64>>,
}

impl R2ObjectLockUnavailable {
    /// Construct an unavailable R2 adapter for capability-gate enforcement.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Count immutable-put attempts for conformance tests.
    #[must_use]
    pub fn immutable_put_calls(&self) -> u64 {
        match self.immutable_put_calls.lock() {
            Ok(calls) => *calls,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }
}

impl ObjectLockArchiveAdapter for R2ObjectLockUnavailable {
    fn negotiate_capabilities(&self) -> Result<ObjectLockCapabilityReport, ObjectLockArchiveError> {
        Ok(ObjectLockCapabilityReport {
            contract_version: OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION,
            backend: ObjectLockBackendIdentity::new(
                ObjectLockBackendFamily::S3,
                "cloudflare-r2",
                "current-r2-audit-archive",
            ),
            capabilities: ObjectLockCapabilities {
                immutable_put: false,
                retain_until_readback: false,
                legal_hold_readback: false,
                delete_denial: false,
                residency_metadata: true,
                audit_receipt: false,
            },
            evidence_reference: "Cloudflare R2 S3 compatibility table: Object Lock unsupported"
                .to_string(),
        })
    }

    fn put_immutable(
        &self,
        _request: &ImmutableArchivePut,
    ) -> Result<ImmutableArchiveWriteReceipt, ObjectLockArchiveError> {
        let mut calls = self.immutable_put_calls.lock().map_err(|_| {
            ObjectLockArchiveError::TestFixture("R2 unavailable adapter mutex poisoned".to_string())
        })?;
        *calls = calls.saturating_add(1);
        Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
            "Cloudflare R2 does not provide the Object-Lock contract".to_string(),
        ))
    }

    fn read_retention(
        &self,
        _object_key: &str,
    ) -> Result<ObjectLockRetentionReadback, ObjectLockArchiveError> {
        Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
            "Cloudflare R2 cannot read Object-Lock retention state".to_string(),
        ))
    }

    fn attempt_delete(&self, _object_key: &str) -> Result<DeleteAttempt, ObjectLockArchiveError> {
        Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
            "Cloudflare R2 cannot prove Object-Lock delete denial".to_string(),
        ))
    }
}

/// In-memory adapter used to exercise the portable contract in CI.
#[derive(Clone, Debug)]
pub struct InMemoryObjectLockArchive {
    identity: ObjectLockBackendIdentity,
    residency: ArchiveResidency,
    delete_is_denied: bool,
    objects: Arc<Mutex<BTreeMap<String, (ImmutableRetention, String)>>>,
    next_receipt: Arc<Mutex<u64>>,
}

impl InMemoryObjectLockArchive {
    /// Construct a conforming fake with the supplied backend identity and residency.
    #[must_use]
    pub fn new(identity: ObjectLockBackendIdentity, residency: ArchiveResidency) -> Self {
        Self {
            identity,
            residency,
            delete_is_denied: true,
            objects: Arc::new(Mutex::new(BTreeMap::new())),
            next_receipt: Arc::new(Mutex::new(1)),
        }
    }

    /// Construct a deliberately mutable fake for negative conformance tests.
    #[must_use]
    pub fn mutable_for_test(
        identity: ObjectLockBackendIdentity,
        residency: ArchiveResidency,
    ) -> Self {
        Self {
            delete_is_denied: false,
            ..Self::new(identity, residency)
        }
    }
}

impl ObjectLockArchiveAdapter for InMemoryObjectLockArchive {
    fn negotiate_capabilities(&self) -> Result<ObjectLockCapabilityReport, ObjectLockArchiveError> {
        let capabilities = if self.delete_is_denied {
            ObjectLockCapabilities::required()
        } else {
            ObjectLockCapabilities {
                delete_denial: false,
                ..ObjectLockCapabilities::required()
            }
        };
        Ok(ObjectLockCapabilityReport {
            contract_version: OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION,
            backend: self.identity.clone(),
            capabilities,
            evidence_reference: "in-memory conformance fixture".to_string(),
        })
    }

    fn put_immutable(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<ImmutableArchiveWriteReceipt, ObjectLockArchiveError> {
        let mut objects = self.objects.lock().map_err(|_| {
            ObjectLockArchiveError::TestFixture("in-memory object map mutex poisoned".to_string())
        })?;
        let mut next_receipt = self.next_receipt.lock().map_err(|_| {
            ObjectLockArchiveError::TestFixture(
                "in-memory receipt counter mutex poisoned".to_string(),
            )
        })?;
        let version_id = format!("in-memory-version-{}", *next_receipt);
        objects.insert(
            request.object_key.clone(),
            (request.retention, version_id.clone()),
        );
        let receipt_id = format!("in-memory-object-lock-{}", *next_receipt);
        *next_receipt = next_receipt.saturating_add(1);
        let observed_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                ObjectLockArchiveError::TestFixture(
                    "test clock was before the Unix epoch".to_string(),
                )
            })?
            .as_millis();
        let observed_at_unix_ms = u64::try_from(observed_at_unix_ms).map_err(|_| {
            ObjectLockArchiveError::TestFixture(
                "test observation timestamp exceeded u64 range".to_string(),
            )
        })?;
        Ok(ImmutableArchiveWriteReceipt {
            bucket: self.identity.archive_target.clone(),
            object_key: request.object_key.clone(),
            object_version: version_id.clone(),
            audit_receipt: ArchiveAuditReceipt {
                receipt_id,
                bucket: self.identity.archive_target.clone(),
                object_key: request.object_key.clone(),
                object_version: version_id,
                observed_at_unix_ms,
            },
        })
    }

    fn read_retention(
        &self,
        object_key: &str,
    ) -> Result<ObjectLockRetentionReadback, ObjectLockArchiveError> {
        let objects = self.objects.lock().map_err(|_| {
            ObjectLockArchiveError::TestFixture("in-memory object map mutex poisoned".to_string())
        })?;
        let (retention, object_version) = objects.get(object_key).cloned().ok_or_else(|| {
            ObjectLockArchiveError::Backend(
                "object does not exist in in-memory archive".to_string(),
            )
        })?;
        Ok(ObjectLockRetentionReadback {
            object_version,
            object_key: object_key.to_string(),
            retention,
            residency: self.residency.clone(),
        })
    }

    fn attempt_delete(&self, object_key: &str) -> Result<DeleteAttempt, ObjectLockArchiveError> {
        if self.delete_is_denied {
            return Ok(DeleteAttempt::Denied {
                object_key: object_key.to_string(),
                reason: "retention_or_legal_hold".to_string(),
            });
        }
        let mut objects = self.objects.lock().map_err(|_| {
            ObjectLockArchiveError::TestFixture("in-memory object map mutex poisoned".to_string())
        })?;
        objects.remove(object_key);
        Ok(DeleteAttempt::Deleted)
    }

    fn record_failure(
        &self,
        _operation: &'static str,
        _object_key: &str,
        _object_version: Option<&str>,
        _failure_class: &'static str,
    ) -> Result<(), ObjectLockArchiveError> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests use direct assertions for conformance examples"
)]
mod tests {
    use super::*;

    const LEGACY_TIMESTAMP_FIELD: &str = concat!("recorded_at", "_unix_ms");
    const LEGACY_PROVIDER_TIME_CLAIM: &str = concat!("Provider-recorded immutable-write ", "time.");

    fn timestamp_semantics_are_local(contract: &str, adapter: &str, docs: &str) -> bool {
        contract.contains("pub observed_at_unix_ms: u64")
            && contract.contains("Local adapter clock observation immediately after write success.")
            && !contract.contains(LEGACY_TIMESTAMP_FIELD)
            && !contract.contains(LEGACY_PROVIDER_TIME_CLAIM)
            && adapter.contains("let observed_at_unix_ms = SystemTime::now()")
            && adapter.contains("observed_at_unix_ms,")
            && !adapter.contains(LEGACY_TIMESTAMP_FIELD)
            && docs.contains("Its `observed_at_unix_ms` field is the")
            && docs.contains("adapter's local clock reading")
            && docs.contains("not an S3 event timestamp")
    }

    #[test]
    fn receipt_timestamp_semantics_reject_provider_recorded_mutations() {
        let contract = include_str!("object_lock_archive.rs");
        let adapter = include_str!("aws_s3_object_lock.rs");
        let docs =
            include_str!("../../../docs/operator/aws-s3-object-lock-archive-provisioning.md");
        assert!(timestamp_semantics_are_local(contract, adapter, docs));

        let provider_field_mutation =
            contract.replace("observed_at_unix_ms", LEGACY_TIMESTAMP_FIELD);
        assert!(!timestamp_semantics_are_local(
            &provider_field_mutation,
            adapter,
            docs,
        ));

        let provider_claim_mutation = contract.replace(
            "Local adapter clock observation immediately after write success.",
            LEGACY_PROVIDER_TIME_CLAIM,
        );
        assert!(!timestamp_semantics_are_local(
            &provider_claim_mutation,
            adapter,
            docs,
        ));

        let adapter_field_mutation = adapter.replace("observed_at_unix_ms", LEGACY_TIMESTAMP_FIELD);
        assert!(!timestamp_semantics_are_local(
            contract,
            &adapter_field_mutation,
            docs,
        ));

        let provider_docs_mutation = docs.replace(
            "adapter's local clock reading",
            "provider recorded immutable-write time",
        );
        assert!(!timestamp_semantics_are_local(
            contract,
            adapter,
            &provider_docs_mutation,
        ));
    }

    #[derive(Debug)]
    struct WrongObjectDeleteDenialAdapter {
        inner: InMemoryObjectLockArchive,
    }

    impl ObjectLockArchiveAdapter for WrongObjectDeleteDenialAdapter {
        fn negotiate_capabilities(
            &self,
        ) -> Result<ObjectLockCapabilityReport, ObjectLockArchiveError> {
            self.inner.negotiate_capabilities()
        }

        fn put_immutable(
            &self,
            request: &ImmutableArchivePut,
        ) -> Result<ImmutableArchiveWriteReceipt, ObjectLockArchiveError> {
            self.inner.put_immutable(request)
        }

        fn read_retention(
            &self,
            object_key: &str,
        ) -> Result<ObjectLockRetentionReadback, ObjectLockArchiveError> {
            self.inner.read_retention(object_key)
        }

        fn attempt_delete(
            &self,
            _object_key: &str,
        ) -> Result<DeleteAttempt, ObjectLockArchiveError> {
            Ok(DeleteAttempt::Denied {
                object_key: "audit/other-object.ndjson".to_string(),
                reason: "retention_or_legal_hold".to_string(),
            })
        }

        fn record_failure(
            &self,
            operation: &'static str,
            object_key: &str,
            object_version: Option<&str>,
            failure_class: &'static str,
        ) -> Result<(), ObjectLockArchiveError> {
            self.inner
                .record_failure(operation, object_key, object_version, failure_class)
        }
    }

    fn identity() -> ObjectLockBackendIdentity {
        ObjectLockBackendIdentity::new(ObjectLockBackendFamily::S3, "test-s3", "audit-worm-eu")
    }

    fn residency() -> ArchiveResidency {
        ArchiveResidency::new("EU", "eu-west-1", "single-region")
    }

    fn request() -> ImmutableArchivePut {
        ImmutableArchivePut {
            tenant_id: "tenant-a".to_string(),
            object_key: "audit/2026/09/22/tenant/00000001.ndjson".to_string(),
            body: b"sealed audit bytes".to_vec(),
            retention: ImmutableRetention {
                retain_until_unix_ms: 1_800_000_000_000,
                legal_hold: LegalHold::On,
            },
            expected_residency: residency(),
        }
    }

    #[test]
    fn conforming_fake_proves_the_full_archive_contract() {
        let archive = VerifiedObjectLockArchive::connect(InMemoryObjectLockArchive::new(
            identity(),
            residency(),
        ))
        .expect("complete fake negotiates");
        let request = request();
        let receipt = archive
            .put_immutable(&request)
            .expect("verified immutable put");
        assert_eq!(receipt.write_receipt.bucket, "audit-worm-eu");
        assert_eq!(
            receipt.write_receipt.audit_receipt.bucket,
            receipt.write_receipt.bucket
        );
        assert_eq!(receipt.write_receipt.object_key, request.object_key);
        assert_eq!(receipt.retention_readback.retention, request.retention);
        assert_eq!(
            receipt.retention_readback.residency,
            request.expected_residency
        );
        assert!(!receipt.write_receipt.audit_receipt.receipt_id.is_empty());
        let denial = archive
            .assert_delete_denied(&request.object_key)
            .expect("retained object delete must be denied");
        assert_eq!(denial.object_key, request.object_key);
    }

    #[test]
    fn r2_fails_negotiation_before_an_immutable_put_can_run() {
        let r2 = R2ObjectLockUnavailable::new();
        let error = VerifiedObjectLockArchive::connect(r2.clone())
            .expect_err("R2 is not Object-Lock capable");
        assert!(matches!(
            error,
            ObjectLockArchiveError::RequiredCapabilityMissing(_)
        ));
        assert_eq!(r2.immutable_put_calls(), 0);
    }

    #[test]
    fn mutable_backend_cannot_pass_as_a_worm_archive() {
        let error = VerifiedObjectLockArchive::connect(
            InMemoryObjectLockArchive::mutable_for_test(identity(), residency()),
        )
        .expect_err("mutable backend must fail negotiation before an immutable put");
        assert_eq!(
            error,
            ObjectLockArchiveError::RequiredCapabilityMissing(vec!["delete_denial"])
        );
    }

    #[test]
    fn delete_denial_must_cover_the_requested_object() {
        let archive = VerifiedObjectLockArchive::connect(WrongObjectDeleteDenialAdapter {
            inner: InMemoryObjectLockArchive::new(identity(), residency()),
        })
        .expect("complete fake negotiates");

        let error = archive
            .assert_delete_denied("audit/requested-object.ndjson")
            .expect_err("a denial for another object cannot prove retention");
        assert_eq!(
            error,
            ObjectLockArchiveError::ReadbackMismatch(
                "delete denial covered a different object than requested".to_string()
            )
        );
    }

    #[test]
    fn delete_denial_rejects_an_empty_object_key() {
        let archive = VerifiedObjectLockArchive::connect(InMemoryObjectLockArchive::new(
            identity(),
            residency(),
        ))
        .expect("complete fake negotiates");

        let error = archive
            .assert_delete_denied("  ")
            .expect_err("delete proof must identify an object");
        assert_eq!(
            error,
            ObjectLockArchiveError::ReadbackMismatch(
                "delete-denial proof requires a non-empty object key".to_string()
            )
        );
    }
}
