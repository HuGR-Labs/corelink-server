//! Native Amazon S3 Object Lock adapter for the portable archive contract.
//!
//! Construction accepts complete AWS SDK configurations, not opaque clients.
//! The adapter builds both its S3 delete probe and STS identity client from the
//! same protected configuration, then binds the observed STS principal to the
//! approved probe identity before treating a delete denial as Object Lock
//! evidence. Application writers never receive delete permission.

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
use aws_sdk_sts::Client as StsClient;
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
    delete_probe_sts: Option<StsClient>,
    delete_probe_target: Option<DeleteProbeTarget>,
    config: AwsS3ComplianceArchiveConfig,
    residency: ArchiveResidency,
    audit: Arc<dyn ComplianceArchiveAuditSink>,
    version_ids: Arc<Mutex<BTreeMap<String, String>>>,
}

/// Trusted, fail-closed configuration for one tenant's approved S3 target.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AwsS3ComplianceArchiveConfig {
    tenant_id: String,
    jurisdiction: String,
    account_id: String,
    bucket: String,
    region: String,
    target_label: String,
    writer_workload_identity: String,
    retention_days: u32,
    approval_reference: String,
    evidence_reference: String,
    cost_ceiling_usd_micros: u64,
    cost_owner: String,
    cleanup_owner: String,
}

impl AwsS3ComplianceArchiveConfig {
    /// Construct only a complete reviewed target configuration.
    /// Validate a complete configuration supplied by trusted server configuration.
    pub fn try_from_parts(
        parts: AwsS3ComplianceArchiveConfigParts,
    ) -> Result<Self, ObjectLockArchiveError> {
        let config = Self {
            tenant_id: parts.tenant_id,
            jurisdiction: parts.jurisdiction,
            account_id: parts.account_id,
            bucket: parts.bucket,
            region: parts.region,
            target_label: parts.target_label,
            writer_workload_identity: parts.writer_workload_identity,
            retention_days: parts.retention_days,
            approval_reference: parts.approval_reference,
            evidence_reference: parts.evidence_reference,
            cost_ceiling_usd_micros: parts.cost_ceiling_usd_micros,
            cost_owner: parts.cost_owner,
            cleanup_owner: parts.cleanup_owner,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ObjectLockArchiveError> {
        let complete = [
            &self.tenant_id,
            &self.jurisdiction,
            &self.account_id,
            &self.bucket,
            &self.region,
            &self.target_label,
            &self.writer_workload_identity,
            &self.approval_reference,
            &self.evidence_reference,
            &self.cost_owner,
            &self.cleanup_owner,
        ]
        .iter()
        .all(|value| !value.trim().is_empty());
        if !complete
            || self.account_id.len() != 12
            || !self.account_id.bytes().all(|byte| byte.is_ascii_digit())
            || self.retention_days == 0
            || self.cost_ceiling_usd_micros == 0
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed("AWS Compliance archive configuration was incomplete, malformed, or lacked an approved cost boundary".to_string()));
        }
        Ok(())
    }
}

/// Complete input to [`AwsS3ComplianceArchiveConfig::try_from_parts`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AwsS3ComplianceArchiveConfigParts {
    pub tenant_id: String,
    pub jurisdiction: String,
    pub account_id: String,
    pub bucket: String,
    pub region: String,
    pub target_label: String,
    pub writer_workload_identity: String,
    pub retention_days: u32,
    pub approval_reference: String,
    pub evidence_reference: String,
    pub cost_ceiling_usd_micros: u64,
    pub cost_owner: String,
    pub cleanup_owner: String,
}

/// Append-only durable audit port. A missing, failed, or unverifiable receipt
/// rejects the archive operation before it can be reported as successful.
pub trait ComplianceArchiveAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one tenant-safe provider operation record and return its durable receipt.
    fn append(&self, event: ComplianceArchiveAuditEvent) -> Result<String, ObjectLockArchiveError>;
    /// Verify the exact durable receipt before the provider operation succeeds.
    fn verify(
        &self,
        receipt_id: &str,
        event: &ComplianceArchiveAuditEvent,
    ) -> Result<(), ObjectLockArchiveError>;
}

/// Tenant-safe audit record for one Object Lock operation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComplianceArchiveAuditEvent {
    /// Stable operation name; never includes payload bytes or credentials.
    pub operation: &'static str,
    /// Authenticated tenant bound to the target configuration.
    pub tenant_id: String,
    /// Approved account and target labels, never secrets.
    pub account_id: String,
    /// Exact configured bucket bound to this operation and durable receipt.
    pub bucket: String,
    pub target_label: String,
    pub region: String,
    pub object_key: String,
    pub object_version: String,
    pub provider_request_id: String,
    pub evidence_reference: String,
    /// `success` or `failure`; failure classes are stable, non-sensitive labels.
    pub outcome: &'static str,
    pub failure_class: Option<&'static str>,
}

/// Exact already-locked synthetic object version used during protected
/// capability negotiation. The workflow must independently demonstrate the
/// listed principal's effective delete permission for this exact version.
#[derive(Clone, PartialEq, Eq)]
pub struct DeleteProbeTarget {
    object_key: String,
    version_id: String,
    expected_principal_arn: String,
    effective_permission_receipt: String,
}

impl DeleteProbeTarget {
    /// Bind the deletion probe to one exact object version.
    pub fn new(
        object_key: impl Into<String>,
        version_id: impl Into<String>,
        expected_principal_arn: impl Into<String>,
        effective_permission_receipt: impl Into<String>,
    ) -> Result<Self, ObjectLockArchiveError> {
        let object_key = object_key.into();
        let version_id = version_id.into();
        let expected_principal_arn = expected_principal_arn.into();
        let effective_permission_receipt = effective_permission_receipt.into();
        AwsS3ObjectLockAdapter::validate_object_key(&object_key)?;
        if version_id.trim().is_empty()
            || version_id.trim() != version_id.as_str()
            || expected_principal_arn.trim().is_empty()
            || effective_permission_receipt.trim().is_empty()
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "delete probe requires an exact object version, STS principal, and effective-permission receipt"
                    .to_string(),
            ));
        }
        Ok(Self {
            object_key,
            version_id,
            expected_principal_arn,
            effective_permission_receipt,
        })
    }
}

impl core::fmt::Debug for AwsS3ObjectLockAdapter {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AwsS3ObjectLockAdapter")
            .field("writer", &"[AWS SDK client redacted]")
            .field("delete_probe", &self.delete_probe.is_some())
            .field("delete_probe_sts", &self.delete_probe_sts.is_some())
            .field("bucket", &"[configured target]")
            .field("expected_bucket_owner", &"[REDACTED]")
            .field("region", &self.config.region)
            .field("residency", &self.residency)
            .field("target_label", &self.config.target_label)
            .field("evidence_reference", &self.config.evidence_reference)
            .finish()
    }
}

impl AwsS3ObjectLockAdapter {
    fn residency_is_complete(residency: &ArchiveResidency) -> bool {
        !residency.jurisdiction.trim().is_empty()
            && !residency.region.trim().is_empty()
            && !residency.location_class.trim().is_empty()
    }

    /// Construct an adapter from protected configurations for one pre-provisioned target.
    /// S3 and STS probe clients always share the exact same probe credentials.
    pub fn from_sdk_configs(
        writer_config: aws_types::SdkConfig,
        delete_probe_config: Option<aws_types::SdkConfig>,
        delete_probe_target: Option<DeleteProbeTarget>,
        config: AwsS3ComplianceArchiveConfig,
        residency: ArchiveResidency,
        audit: Arc<dyn ComplianceArchiveAuditSink>,
    ) -> Self {
        let delete_probe = delete_probe_config.as_ref().map(Client::new);
        let delete_probe_sts = delete_probe_config.as_ref().map(StsClient::new);
        Self {
            writer: Client::new(&writer_config),
            delete_probe,
            delete_probe_sts,
            delete_probe_target,
            config,
            residency,
            audit,
            version_ids: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn validate_configuration(&self) -> Result<(), ObjectLockArchiveError> {
        if self.config.validate().is_err()
            || !Self::residency_is_complete(&self.residency)
            || self.residency.region != self.config.region
            || self.residency.jurisdiction != self.config.jurisdiction
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "AWS target identity or residency configuration was incomplete or inconsistent"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn validate_tenant_request(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<(), ObjectLockArchiveError> {
        let prefix = format!("audit/{}/", self.config.tenant_id);
        if request.tenant_id != self.config.tenant_id
            || request.expected_residency != self.residency
            || !request.object_key.starts_with(&prefix)
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "tenant, residency, or archive key did not match the trusted AWS target"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn require_approved_retention(
        &self,
        retain_until_unix_ms: u64,
    ) -> Result<(), ObjectLockArchiveError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                ObjectLockArchiveError::Backend(
                    "system clock was before the Unix epoch".to_string(),
                )
            })?
            .as_millis();
        let now = u64::try_from(now).map_err(|_| {
            ObjectLockArchiveError::Backend("local time exceeded u64 range".to_string())
        })?;
        let minimum = u64::from(self.config.retention_days)
            .checked_mul(86_400_000)
            .and_then(|duration| now.checked_add(duration))
            .ok_or_else(|| {
                ObjectLockArchiveError::ReadbackMismatch(
                    "approved retention duration overflowed".to_string(),
                )
            })?;
        if retain_until_unix_ms < minimum {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "requested retention was shorter than the approved tenant policy".to_string(),
            ));
        }
        Ok(())
    }

    fn audit_event(
        &self,
        operation: &'static str,
        object_key: impl Into<String>,
        object_version: impl Into<String>,
        provider_request_id: impl Into<String>,
    ) -> ComplianceArchiveAuditEvent {
        ComplianceArchiveAuditEvent {
            operation,
            tenant_id: self.config.tenant_id.clone(),
            account_id: self.config.account_id.clone(),
            bucket: self.config.bucket.clone(),
            target_label: self.config.target_label.clone(),
            region: self.config.region.clone(),
            object_key: object_key.into(),
            object_version: object_version.into(),
            provider_request_id: provider_request_id.into(),
            evidence_reference: self.config.evidence_reference.clone(),
            outcome: "success",
            failure_class: None,
        }
    }

    fn persist_audit(
        &self,
        event: ComplianceArchiveAuditEvent,
    ) -> Result<String, ObjectLockArchiveError> {
        let receipt_id = self.audit.append(event.clone())?;
        if receipt_id.trim().is_empty() {
            return Err(ObjectLockArchiveError::Backend(
                "durable Object Lock audit returned an empty receipt".to_string(),
            ));
        }
        self.audit.verify(&receipt_id, &event)?;
        Ok(receipt_id)
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
            self.delete_probe.is_some() && self.delete_probe_sts.is_some(),
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
        let bucket = self.config.bucket.clone();
        let expected_owner = self.config.account_id.clone();
        let key = object_key.to_string();
        let version_id = version_id.to_string();
        let (access_denied, provider_request_id) =
            Self::block_on("delete denial probe", async move {
                match probe
                    .delete_object()
                    .bucket(&bucket)
                    .key(&key)
                    .version_id(version_id)
                    .expected_bucket_owner(&expected_owner)
                    .send()
                    .await
                {
                    Ok(_) => Ok((false, String::new())),
                    Err(error) => {
                        let structured_access_denied =
                            error.as_service_error().is_some_and(|service_error| {
                                Self::is_structured_access_denied(service_error.code())
                            });
                        if structured_access_denied {
                            let request_id = error.request_id().ok_or_else(|| {
                                "delete probe AccessDenied returned no provider request ID"
                                    .to_string()
                            })?;
                            Ok((true, request_id.to_string()))
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
                reason: format!(
                    "s3_structured_access_denied_for_exact_version:{provider_request_id}"
                ),
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
        let bucket = self.config.bucket.clone();
        let expected_owner = self.config.account_id.clone();
        let expected_region = self.config.region.clone();
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
        let probe_sts = self
            .delete_probe_sts
            .as_ref()
            .ok_or_else(|| {
                ObjectLockArchiveError::RequiredCapabilityMissing(vec!["delete_denial"])
            })?
            .clone();
        let expected_principal = delete_probe_target.expected_principal_arn.clone();
        Self::block_on("delete probe identity binding", async move {
            let observed = probe_sts
                .get_caller_identity()
                .send()
                .await
                .map_err(|_| "STS GetCallerIdentity for delete probe failed".to_string())?
                .arn()
                .ok_or_else(|| "STS GetCallerIdentity returned no ARN".to_string())?;
            if observed != expected_principal {
                return Err(
                    "delete probe credentials did not match the approved principal".to_string(),
                );
            }
            Ok(())
        })?;
        match self.delete_exact_version(
            &delete_probe_target.object_key,
            &delete_probe_target.version_id,
        )? {
            DeleteAttempt::Denied { object_key, reason }
                if object_key == delete_probe_target.object_key =>
            {
                let provider_request_id = reason.rsplit(':').next().unwrap_or_default();
                if provider_request_id.trim().is_empty() {
                    return Err(ObjectLockArchiveError::ReadbackMismatch(
                        "delete denial lacked a provider request reference".to_string(),
                    ));
                }
                self.persist_audit(self.audit_event(
                    "pre_expiry_delete_denial",
                    object_key,
                    delete_probe_target.version_id.clone(),
                    provider_request_id.to_string(),
                ))?;
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
                self.config.target_label.clone(),
            ),
            capabilities,
            evidence_reference: format!(
                "{}; delete probe principal={} effective-permission-receipt={}",
                self.config.evidence_reference,
                delete_probe_target.expected_principal_arn,
                delete_probe_target.effective_permission_receipt,
            ),
        })
    }

    fn put_immutable(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<ImmutableArchiveWriteReceipt, ObjectLockArchiveError> {
        Self::validate_object_key(&request.object_key)?;
        self.validate_tenant_request(request)?;
        self.require_approved_retention(request.retention.retain_until_unix_ms)?;
        let retain_until = i64::try_from(request.retention.retain_until_unix_ms).map_err(|_| {
            ObjectLockArchiveError::ReadbackMismatch(
                "requested retain-until timestamp was outside the AWS range".to_string(),
            )
        })?;
        let retain_until = DateTime::from_millis(retain_until);
        let writer = self.writer.clone();
        let bucket = self.config.bucket.clone();
        let expected_owner = self.config.account_id.clone();
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
        let durable_receipt_id = self.persist_audit(self.audit_event(
            "immutable_put",
            request.object_key.clone(),
            version_id.to_string(),
            request_id.to_string(),
        ))?;
        Ok(ImmutableArchiveWriteReceipt {
            bucket: self.config.bucket.clone(),
            object_key: request.object_key.clone(),
            object_version: version_id.to_string(),
            audit_receipt: ArchiveAuditReceipt {
                receipt_id: durable_receipt_id,
                bucket: self.config.bucket.clone(),
                object_key: request.object_key.clone(),
                object_version: version_id.to_string(),
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
        let bucket = self.config.bucket.clone();
        let expected_owner = self.config.account_id.clone();
        let key = object_key.to_string();
        let version_id = self.object_version(object_key)?.ok_or_else(|| {
            ObjectLockArchiveError::ReadbackMismatch(
                "retention readback requires the exact version returned by immutable put"
                    .to_string(),
            )
        })?;
        let residency = self.residency.clone();
        let (retention_readback, provider_request_id) =
            Self::block_on("retention readback", async move {
                let mut retention_request = writer
                    .get_object_retention()
                    .bucket(&bucket)
                    .key(&key)
                    .expected_bucket_owner(&expected_owner);
                retention_request = retention_request.version_id(&version_id);
                let retention_response = retention_request
                    .send()
                    .await
                    .map_err(|_| "GetObjectRetention failed".to_string())?;
                let provider_request_id = retention_response
                    .request_id()
                    .ok_or_else(|| {
                        "GetObjectRetention returned no provider request ID".to_string()
                    })?
                    .to_string();
                let retention = retention_response
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
                legal_hold_request = legal_hold_request.version_id(&version_id);
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
                let readback = ObjectLockRetentionReadback {
                    object_version: version_id,
                    object_key: key,
                    retention: crate::object_lock_archive::ImmutableRetention {
                        retain_until_unix_ms,
                        legal_hold,
                    },
                    residency,
                };
                Ok((readback, provider_request_id))
            })?;
        self.persist_audit(self.audit_event(
            "retention_and_legal_hold_readback",
            retention_readback.object_key.clone(),
            retention_readback.object_version.clone(),
            provider_request_id,
        ))?;
        Ok(retention_readback)
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

    fn record_failure(
        &self,
        operation: &'static str,
        object_key: &str,
        object_version: Option<&str>,
        failure_class: &'static str,
    ) -> Result<(), ObjectLockArchiveError> {
        let observed_version = match object_version {
            Some(version) => Some(version.to_string()),
            None => self.object_version(object_key)?,
        };
        let mut event = self.audit_event(
            operation,
            object_key,
            observed_version.unwrap_or_default(),
            "",
        );
        event.outcome = "failure";
        event.failure_class = Some(failure_class);
        self.persist_audit(event).map(|_| ())
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
        let target = DeleteProbeTarget::new(
            "audit/probe",
            "version-1",
            "arn:aws:iam::123456789012:role/delete-probe",
            "effective permission receipt",
        )
        .expect("authorized exact probe target");
        assert!(DeleteProbeTarget::new("audit/probe", " ", "arn", "receipt").is_err());
        assert!(DeleteProbeTarget::new("audit/probe", "version-1", " ", "receipt").is_err());
        assert!(DeleteProbeTarget::new("audit/probe", "version-1", "arn", " ").is_err());
        assert!(DeleteProbeTarget::new("audit/probe", " version-1", "arn", "receipt").is_err());
        assert!(DeleteProbeTarget::new("audit/../other", "version-1", "arn", "receipt").is_err());
        assert!(DeleteProbeTarget::new("audit/probe", "version-2", "arn", "receipt").is_ok());

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
