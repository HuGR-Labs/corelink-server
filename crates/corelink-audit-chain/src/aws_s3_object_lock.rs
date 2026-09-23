//! Native Amazon S3 Object Lock adapter for the portable archive contract.
//!
//! This module accepts already-constructed AWS SDK clients so credential
//! selection and rotation remain outside the adapter. Production callers must
//! use a short-lived, least-privilege writer client. A separate probe client
//! is required to demonstrate that S3 itself rejects a version delete; an
//! application writer client must never receive delete permission.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use aws_sdk_s3::error::ProvideErrorMetadata as _;
use aws_sdk_s3::primitives::{ByteStream, DateTime};
use aws_sdk_s3::types::{
    BucketVersioningStatus, ObjectLockEnabled, ObjectLockLegalHoldStatus, ObjectLockMode,
    ObjectLockRetentionMode,
};
use aws_sdk_s3::Client;
use aws_types::request_id::RequestId as _;

use crate::object_lock_archive::{
    ArchiveAuditReceipt, ArchiveResidency, DeleteAttempt, ImmutableArchivePut,
    ImmutableArchiveWriteReceipt, LegalHold, ObjectLockArchiveAdapter, ObjectLockArchiveError,
    ObjectLockBackendFamily, ObjectLockBackendIdentity, ObjectLockCapabilities,
    ObjectLockCapabilityReport, ObjectLockRetentionReadback, OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION,
};

/// Amazon S3 implementation bound to one bucket, region, and approved
/// residency declaration.
pub struct AwsS3ObjectLockAdapter {
    writer: Client,
    delete_probe: Option<Client>,
    delete_probe_target: Option<DeleteProbeTarget>,
    bucket: String,
    expected_bucket_owner: String,
    region: String,
    residency: ArchiveResidency,
    target_label: String,
    evidence_reference: String,
    version_ids: Arc<Mutex<BTreeMap<String, String>>>,
}

/// Reference to the independently verified delete permission for one exact
/// probe identity and object version.
#[derive(Clone, PartialEq, Eq)]
pub struct DeleteProbeAuthorization {
    identity_reference: String,
    object_key: String,
    version_id: String,
    evidence_reference: String,
}

impl DeleteProbeAuthorization {
    /// Bind current external IAM authorization evidence to the probe target.
    pub fn new(
        identity_reference: impl Into<String>,
        object_key: impl Into<String>,
        version_id: impl Into<String>,
        evidence_reference: impl Into<String>,
    ) -> Result<Self, ObjectLockArchiveError> {
        let identity_reference = identity_reference.into();
        let object_key = object_key.into();
        let version_id = version_id.into();
        let evidence_reference = evidence_reference.into();
        if identity_reference.trim().is_empty()
            || object_key.trim().is_empty()
            || version_id.trim().is_empty()
            || evidence_reference.trim().is_empty()
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "delete probe requires an authorization evidence reference".to_string(),
            ));
        }
        Ok(Self {
            identity_reference,
            object_key,
            version_id,
            evidence_reference,
        })
    }

    fn covers(&self, object_key: &str, version_id: &str) -> bool {
        !self.identity_reference.trim().is_empty()
            && !self.evidence_reference.trim().is_empty()
            && self.object_key == object_key
            && self.version_id == version_id
    }
}

/// Exact already-locked synthetic object version used during capability
/// negotiation. A missing, mismatched, or unverified target fails closed.
#[derive(Clone, PartialEq, Eq)]
pub struct DeleteProbeTarget {
    object_key: String,
    version_id: String,
    authorization: DeleteProbeAuthorization,
}

impl DeleteProbeTarget {
    /// Bind the deletion probe to one exact object version.
    pub fn new(
        object_key: impl Into<String>,
        version_id: impl Into<String>,
        authorization: DeleteProbeAuthorization,
    ) -> Result<Self, ObjectLockArchiveError> {
        let object_key = object_key.into();
        let version_id = version_id.into();
        AwsS3ObjectLockAdapter::validate_object_key(&object_key)?;
        if version_id.trim().is_empty()
            || version_id.trim() != version_id.as_str()
            || !authorization.covers(&object_key, &version_id)
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "delete probe requires permission evidence for the exact object version"
                    .to_string(),
            ));
        }
        Ok(Self {
            object_key,
            version_id,
            authorization,
        })
    }
}

impl core::fmt::Debug for AwsS3ObjectLockAdapter {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AwsS3ObjectLockAdapter")
            .field("writer", &"[AWS SDK client redacted]")
            .field("delete_probe", &self.delete_probe.is_some())
            .field("bucket", &"[configured target]")
            .field("expected_bucket_owner", &"[REDACTED]")
            .field("region", &self.region)
            .field("residency", &self.residency)
            .field("target_label", &self.target_label)
            .field("evidence_reference", &self.evidence_reference)
            .finish()
    }
}

impl AwsS3ObjectLockAdapter {
    fn residency_is_complete(residency: &ArchiveResidency) -> bool {
        !residency.jurisdiction.trim().is_empty()
            && !residency.region.trim().is_empty()
            && !residency.location_class.trim().is_empty()
    }

    /// Construct an adapter for one pre-provisioned bucket.
    ///
    /// The caller supplies the expected owner from protected configuration.
    /// `delete_probe` must use a separate, short-lived identity with delete
    /// permission scoped to synthetic probe objects. The writer client must
    /// not be able to delete or release legal holds.
    #[must_use]
    pub fn new(
        writer: Client,
        delete_probe: Option<Client>,
        delete_probe_target: Option<DeleteProbeTarget>,
        bucket: impl Into<String>,
        expected_bucket_owner: impl Into<String>,
        region: impl Into<String>,
        residency: ArchiveResidency,
        target_label: impl Into<String>,
        evidence_reference: impl Into<String>,
    ) -> Self {
        Self {
            writer,
            delete_probe,
            delete_probe_target,
            bucket: bucket.into(),
            expected_bucket_owner: expected_bucket_owner.into(),
            region: region.into(),
            residency,
            target_label: target_label.into(),
            evidence_reference: evidence_reference.into(),
            version_ids: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn validate_configuration(&self) -> Result<(), ObjectLockArchiveError> {
        if self.bucket.trim().is_empty()
            || self.expected_bucket_owner.trim().is_empty()
            || self.region.trim().is_empty()
            || self.target_label.trim().is_empty()
            || self.evidence_reference.trim().is_empty()
            || !Self::residency_is_complete(&self.residency)
            || self.residency.region != self.region
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "AWS target identity or residency configuration was incomplete or inconsistent"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn block_on<T>(
        operation: &str,
        future: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, ObjectLockArchiveError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| {
            ObjectLockArchiveError::Backend(format!(
                "AWS S3 {operation} requires the native Tokio multi-thread runtime"
            ))
        })?;
        if handle.runtime_flavor() != tokio::runtime::RuntimeFlavor::MultiThread {
            return Err(ObjectLockArchiveError::Backend(format!(
                "AWS S3 {operation} requires the native Tokio multi-thread runtime"
            )));
        }
        tokio::task::block_in_place(|| handle.block_on(future))
            .map_err(ObjectLockArchiveError::Backend)
    }

    fn object_version(&self, object_key: &str) -> Result<Option<String>, ObjectLockArchiveError> {
        self.version_ids
            .lock()
            .map(|versions| Self::version_for_key(&versions, object_key))
            .map_err(|_| {
                ObjectLockArchiveError::Backend(
                    "AWS S3 Object Lock version map was unavailable".to_string(),
                )
            })
    }

    fn version_for_key(versions: &BTreeMap<String, String>, object_key: &str) -> Option<String> {
        versions.get(object_key).cloned()
    }

    fn validate_object_key(object_key: &str) -> Result<(), ObjectLockArchiveError> {
        if !object_key.starts_with("audit/")
            || object_key.len() <= "audit/".len()
            || object_key.starts_with('/')
            || object_key.split('/').any(|segment| segment == "..")
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "AWS Object Lock archive keys must be beneath audit/".to_string(),
            ));
        }
        Ok(())
    }

    fn delete_probe_target(&self) -> Result<&DeleteProbeTarget, ObjectLockArchiveError> {
        Self::validate_delete_probe_configuration(
            self.delete_probe.is_some(),
            self.delete_probe_target.as_ref(),
        )
    }

    fn validate_delete_probe_configuration(
        has_probe_identity: bool,
        target: Option<&DeleteProbeTarget>,
    ) -> Result<&DeleteProbeTarget, ObjectLockArchiveError> {
        if !has_probe_identity {
            return Err(ObjectLockArchiveError::RequiredCapabilityMissing(vec![
                "delete_denial",
            ]));
        }
        target
            .ok_or_else(|| ObjectLockArchiveError::RequiredCapabilityMissing(vec!["delete_denial"]))
    }

    fn is_structured_access_denied(code: Option<&str>) -> bool {
        // Only a structured S3 service error qualifies. Do not infer denial
        // from a present client or match unstable provider message text.
        code == Some("AccessDenied")
    }

    fn delete_exact_version(
        &self,
        object_key: &str,
        version_id: &str,
    ) -> Result<DeleteAttempt, ObjectLockArchiveError> {
        Self::validate_object_key(object_key)?;
        if version_id.trim().is_empty() {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "delete probe requires an exact object version".to_string(),
            ));
        }
        let probe = self.delete_probe.as_ref().ok_or_else(|| {
            ObjectLockArchiveError::RequiredCapabilityMissing(vec!["delete_denial"])
        })?;
        let probe = probe.clone();
        let bucket = self.bucket.clone();
        let expected_owner = self.expected_bucket_owner.clone();
        let key = object_key.to_string();
        let version_id = version_id.to_string();
        let access_denied = Self::block_on("delete denial probe", async move {
            match probe
                .delete_object()
                .bucket(&bucket)
                .key(&key)
                .version_id(version_id)
                .expected_bucket_owner(&expected_owner)
                .send()
                .await
            {
                Ok(_) => Ok(false),
                Err(error) => {
                    let structured_access_denied =
                        error.as_service_error().is_some_and(|service_error| {
                            Self::is_structured_access_denied(service_error.code())
                        });
                    if structured_access_denied {
                        Ok(true)
                    } else {
                        Err(
                            "delete probe failed without a structured AccessDenied response"
                                .to_string(),
                        )
                    }
                }
            }
        })?;
        if access_denied {
            Ok(DeleteAttempt::Denied {
                object_key: object_key.to_string(),
                reason: "s3_structured_access_denied_for_exact_version".to_string(),
            })
        } else {
            Ok(DeleteAttempt::Deleted)
        }
    }
}

impl ObjectLockArchiveAdapter for AwsS3ObjectLockAdapter {
    fn negotiate_capabilities(&self) -> Result<ObjectLockCapabilityReport, ObjectLockArchiveError> {
        self.validate_configuration()?;
        let writer = self.writer.clone();
        let bucket = self.bucket.clone();
        let expected_owner = self.expected_bucket_owner.clone();
        let expected_region = self.region.clone();
        let mut capabilities = Self::block_on("capability negotiation", async move {
            let lock = writer
                .get_object_lock_configuration()
                .bucket(&bucket)
                .expected_bucket_owner(&expected_owner)
                .send()
                .await
                .map_err(|_| "GetObjectLockConfiguration failed".to_string())?;
            let lock = lock.object_lock_configuration().ok_or_else(|| {
                "GetObjectLockConfiguration returned no configuration".to_string()
            })?;
            let enabled = lock.object_lock_enabled() == Some(&ObjectLockEnabled::Enabled);
            let default_retention = lock
                .rule()
                .and_then(|rule| rule.default_retention())
                .is_some_and(|retention| {
                    retention.mode() == Some(&ObjectLockRetentionMode::Compliance)
                        && (retention.days().is_some_and(|days| days > 0)
                            || retention.years().is_some_and(|years| years > 0))
                });
            if !enabled || !default_retention {
                return Err(
                    "bucket Object Lock was not enabled with Compliance default retention"
                        .to_string(),
                );
            }

            let versioning = writer
                .get_bucket_versioning()
                .bucket(&bucket)
                .expected_bucket_owner(&expected_owner)
                .send()
                .await
                .map_err(|_| "GetBucketVersioning failed".to_string())?;
            if versioning.status() != Some(&BucketVersioningStatus::Enabled) {
                return Err("bucket versioning was not enabled".to_string());
            }

            let location = writer
                .get_bucket_location()
                .bucket(&bucket)
                .expected_bucket_owner(&expected_owner)
                .send()
                .await
                .map_err(|_| "GetBucketLocation failed".to_string())?;
            let actual_region = location
                .location_constraint()
                .map(|constraint| constraint.as_str())
                .unwrap_or("us-east-1");
            if actual_region != expected_region {
                return Err("S3 bucket region did not match the configured residency".to_string());
            }
            Ok(ObjectLockCapabilities {
                immutable_put: true,
                retain_until_readback: true,
                legal_hold_readback: true,
                delete_denial: false,
                residency_metadata: true,
                audit_receipt: true,
            })
        })?;

        let delete_probe_target = self.delete_probe_target()?;
        match self.delete_exact_version(
            &delete_probe_target.object_key,
            &delete_probe_target.version_id,
        )? {
            DeleteAttempt::Denied { object_key, .. }
                if object_key == delete_probe_target.object_key =>
            {
                capabilities.delete_denial = true;
            }
            DeleteAttempt::Denied { .. } => {
                return Err(ObjectLockArchiveError::ReadbackMismatch(
                    "delete denial covered a different synthetic object".to_string(),
                ));
            }
            DeleteAttempt::Deleted => {
                return Err(ObjectLockArchiveError::DeleteWasAllowed(
                    delete_probe_target.object_key.clone(),
                ));
            }
        }
        Ok(ObjectLockCapabilityReport {
            contract_version: OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION,
            backend: ObjectLockBackendIdentity::new(
                ObjectLockBackendFamily::S3,
                "aws-s3",
                self.target_label.clone(),
            ),
            capabilities,
            evidence_reference: format!(
                "{}; delete probe identity={} authorization={}",
                self.evidence_reference,
                delete_probe_target.authorization.identity_reference,
                delete_probe_target.authorization.evidence_reference,
            ),
        })
    }

    fn put_immutable(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<ImmutableArchiveWriteReceipt, ObjectLockArchiveError> {
        Self::validate_object_key(&request.object_key)?;
        let retain_until = i64::try_from(request.retention.retain_until_unix_ms).map_err(|_| {
            ObjectLockArchiveError::ReadbackMismatch(
                "requested retain-until timestamp was outside the AWS range".to_string(),
            )
        })?;
        let retain_until = DateTime::from_millis(retain_until);
        let writer = self.writer.clone();
        let bucket = self.bucket.clone();
        let expected_owner = self.expected_bucket_owner.clone();
        let object_key = request.object_key.clone();
        let body = request.body.clone();
        let legal_hold = request.retention.legal_hold;
        let output = Self::block_on("immutable put", async move {
            let mut put = writer
                .put_object()
                .bucket(&bucket)
                .key(&object_key)
                .expected_bucket_owner(&expected_owner)
                .body(ByteStream::from(body))
                .object_lock_mode(ObjectLockMode::Compliance)
                .object_lock_retain_until_date(retain_until)
                .server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::Aes256);
            if legal_hold == LegalHold::On {
                put = put.object_lock_legal_hold_status(ObjectLockLegalHoldStatus::On);
            }
            put.send()
                .await
                .map_err(|_| "PutObject with Compliance retention failed".to_string())
        })?;
        // Capture the adapter's local observation time for the successful response.
        let observed_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                ObjectLockArchiveError::Backend(
                    "system clock was before the Unix epoch".to_string(),
                )
            })?
            .as_millis();
        let observed_at_unix_ms = u64::try_from(observed_at_unix_ms).map_err(|_| {
            ObjectLockArchiveError::Backend(
                "local observation timestamp exceeded u64 range".to_string(),
            )
        })?;
        let version_id = output.version_id().ok_or_else(|| {
            ObjectLockArchiveError::Backend(
                "S3 PutObject returned no version ID for the locked object".to_string(),
            )
        })?;
        let request_id = output.request_id().ok_or_else(|| {
            ObjectLockArchiveError::Backend(
                "S3 PutObject returned no provider request ID".to_string(),
            )
        })?;
        self.version_ids
            .lock()
            .map_err(|_| {
                ObjectLockArchiveError::Backend(
                    "AWS S3 Object Lock version map was unavailable".to_string(),
                )
            })?
            .insert(request.object_key.clone(), version_id.to_string());
        Ok(ImmutableArchiveWriteReceipt {
            object_key: request.object_key.clone(),
            audit_receipt: ArchiveAuditReceipt {
                receipt_id: request_id.to_string(),
                object_key: request.object_key.clone(),
                observed_at_unix_ms,
            },
        })
    }

    fn read_retention(
        &self,
        object_key: &str,
    ) -> Result<ObjectLockRetentionReadback, ObjectLockArchiveError> {
        Self::validate_object_key(object_key)?;
        let writer = self.writer.clone();
        let bucket = self.bucket.clone();
        let expected_owner = self.expected_bucket_owner.clone();
        let key = object_key.to_string();
        let version_id = self.object_version(object_key)?;
        let residency = self.residency.clone();
        Ok(Self::block_on("retention readback", async move {
            let mut retention_request = writer
                .get_object_retention()
                .bucket(&bucket)
                .key(&key)
                .expected_bucket_owner(&expected_owner);
            if let Some(version_id) = version_id.as_deref() {
                retention_request = retention_request.version_id(version_id);
            }
            let retention = retention_request
                .send()
                .await
                .map_err(|_| "GetObjectRetention failed".to_string())?
                .retention()
                .cloned()
                .ok_or_else(|| "GetObjectRetention returned no state".to_string())?;
            let mode = retention
                .mode()
                .ok_or_else(|| "GetObjectRetention returned no mode".to_string())?;
            if mode != &ObjectLockRetentionMode::Compliance {
                return Err("S3 object retention was not in Compliance mode".to_string());
            }
            let retain_until_unix_ms = retention
                .retain_until_date()
                .ok_or_else(|| "GetObjectRetention returned no expiry".to_string())?
                .clone()
                .to_millis()
                .map_err(|_| "S3 retain-until timestamp was out of range".to_string())?;
            let retain_until_unix_ms = u64::try_from(retain_until_unix_ms)
                .map_err(|_| "S3 retain-until timestamp was negative".to_string())?;

            let mut legal_hold_request = writer
                .get_object_legal_hold()
                .bucket(&bucket)
                .key(&key)
                .expected_bucket_owner(&expected_owner);
            if let Some(version_id) = version_id.as_deref() {
                legal_hold_request = legal_hold_request.version_id(version_id);
            }
            let legal_hold_response = legal_hold_request
                .send()
                .await
                .map_err(|_| "GetObjectLegalHold failed".to_string())?;
            let legal_hold = legal_hold_response
                .legal_hold()
                .and_then(|state| state.status())
                .cloned()
                .ok_or_else(|| "GetObjectLegalHold returned no state".to_string())?;
            let legal_hold = match legal_hold {
                ObjectLockLegalHoldStatus::On => LegalHold::On,
                ObjectLockLegalHoldStatus::Off => LegalHold::Off,
                _ => return Err("GetObjectLegalHold returned an unknown state".to_string()),
            };

            let location = writer
                .get_bucket_location()
                .bucket(&bucket)
                .expected_bucket_owner(&expected_owner)
                .send()
                .await
                .map_err(|_| "GetBucketLocation failed".to_string())?;
            let actual_region = location
                .location_constraint()
                .map(|constraint| constraint.as_str())
                .unwrap_or("us-east-1");
            if actual_region != residency.region {
                return Err("S3 bucket region changed from negotiated residency".to_string());
            }
            Ok(ObjectLockRetentionReadback {
                object_key: key,
                retention: crate::object_lock_archive::ImmutableRetention {
                    retain_until_unix_ms,
                    legal_hold,
                },
                residency,
            })
        })?)
    }

    fn attempt_delete(&self, object_key: &str) -> Result<DeleteAttempt, ObjectLockArchiveError> {
        Self::validate_object_key(object_key)?;
        let version_id = self.object_version(object_key)?.ok_or_else(|| {
            ObjectLockArchiveError::Backend(
                "delete denial probe requires a version from this adapter's immutable put"
                    .to_string(),
            )
        })?;
        self.delete_exact_version(object_key, &version_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_key_is_confined_to_audit_prefix() {
        assert!(AwsS3ObjectLockAdapter::validate_object_key("audit/epoch/line.ndjson").is_ok());
        assert!(AwsS3ObjectLockAdapter::validate_object_key("audit/").is_err());
        assert!(AwsS3ObjectLockAdapter::validate_object_key("../audit/line.ndjson").is_err());
        assert!(AwsS3ObjectLockAdapter::validate_object_key("audit/../other").is_err());
    }

    #[test]
    fn residency_requires_all_fields() {
        assert!(AwsS3ObjectLockAdapter::residency_is_complete(
            &ArchiveResidency::new("US", "us-east-1", "s3-general-purpose")
        ));
        assert!(!AwsS3ObjectLockAdapter::residency_is_complete(
            &ArchiveResidency::new("US", "us-east-1", " ")
        ));
    }

    #[test]
    fn delete_probe_requires_current_authorization_evidence_for_exact_version() {
        let authorization = DeleteProbeAuthorization::new(
            "synthetic-probe-role",
            "audit/probe",
            "version-1",
            "approved permission evidence",
        )
        .expect("authorization evidence is complete");
        assert!(DeleteProbeAuthorization::new(" ", "audit/probe", "version-1", "evidence",).is_err());
        assert!(DeleteProbeTarget::new("audit/probe", "version-1", authorization.clone(),).is_ok());
        assert!(DeleteProbeTarget::new("audit/probe", " ", authorization.clone()).is_err());
        assert!(DeleteProbeTarget::new("audit/probe", " version-1", authorization.clone(),).is_err());
        assert!(DeleteProbeTarget::new("audit/../other", "version-1", authorization.clone(),).is_err());
        assert!(DeleteProbeTarget::new("audit/probe", "version-2", authorization.clone()).is_err());

        let target = DeleteProbeTarget::new("audit/probe", "version-1", authorization)
            .expect("authorized exact probe target");
        assert!(
            AwsS3ObjectLockAdapter::validate_delete_probe_configuration(false, Some(&target))
                .is_err()
        );
        assert!(AwsS3ObjectLockAdapter::validate_delete_probe_configuration(true, None).is_err());

        let versions = BTreeMap::from([("audit/probe".to_string(), "version-1".to_string())]);
        assert_eq!(
            AwsS3ObjectLockAdapter::version_for_key(&versions, "audit/probe").as_deref(),
            Some("version-1")
        );
        assert_eq!(
            AwsS3ObjectLockAdapter::version_for_key(&versions, "audit/other"),
            None
        );
    }

    #[test]
    fn delete_denial_requires_structured_access_denied() {
        assert!(AwsS3ObjectLockAdapter::is_structured_access_denied(Some(
            "AccessDenied"
        )));
        assert!(!AwsS3ObjectLockAdapter::is_structured_access_denied(None));
        assert!(!AwsS3ObjectLockAdapter::is_structured_access_denied(Some(
            "NoSuchVersion"
        )));
    }
}
