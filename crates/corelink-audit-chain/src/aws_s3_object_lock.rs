//! Optional native AWS S3 Object Lock Compliance adapter.
//!
//! Construction is deliberately explicit and has no environment parser. A
//! caller must supply a trusted, complete per-tenant target configuration, a
//! durable audit port, and separate delete-probe credentials. The crate does
//! not mount this adapter in a request route.

use std::sync::Arc;

use aws_sdk_s3::error::ProvideErrorMetadata as _;
use aws_sdk_s3::primitives::{ByteStream, DateTime};
use aws_sdk_s3::types::{
    BucketVersioningStatus, ObjectLockEnabled, ObjectLockLegalHoldStatus, ObjectLockMode,
    ObjectLockRetentionMode,
};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sts::Client as StsClient;
use aws_types::request_id::RequestId;

use crate::object_lock_archive::{
    ArchiveAuditReceipt, ArchiveResidency, DeleteAttempt, ImmutableArchivePut,
    ImmutableArchiveWriteReceipt, ImmutableRetention, LegalHold, ObjectLockArchiveAdapter,
    ObjectLockArchiveError, ObjectLockBackendFamily, ObjectLockBackendIdentity,
    ObjectLockCapabilities, ObjectLockCapabilityReport, ObjectLockRetentionReadback,
    OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION,
};

/// Complete, trusted target configuration for one tenant and one S3 bucket.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AwsS3ComplianceArchiveConfig {
    /// Authenticated tenant that may use this exact target.
    pub tenant_id: String,
    /// Approved AWS account ID for expected-bucket-owner fencing.
    pub account_id: String,
    /// Approved bucket. It is never selected by a request.
    pub bucket: String,
    /// Approved AWS region.
    pub region: String,
    /// Approved jurisdiction mapped to the region.
    pub jurisdiction: String,
    /// Non-secret target label carried into the contract receipt.
    pub target_label: String,
    /// Approved retention minimum in days.
    pub minimum_retention_days: u32,
    /// Named workload identity expected to write the target.
    pub writer_workload_identity: String,
    /// Written approval reference for the exact target.
    pub approval_reference: String,
    /// Durable audit evidence reference for the exact target.
    pub evidence_reference: String,
    /// Approved maximum cost in micro-USD for the target lifecycle.
    pub cost_ceiling_usd_micros: u64,
    /// Named owner of the approved cost boundary.
    pub cost_owner: String,
    /// Named owner of retention-expiry cleanup obligations.
    pub cleanup_owner: String,
}

impl AwsS3ComplianceArchiveConfig {
    /// Reject incomplete configuration before a client can issue provider I/O.
    pub fn validate(&self) -> Result<(), ObjectLockArchiveError> {
        let complete = [
            &self.tenant_id,
            &self.bucket,
            &self.region,
            &self.jurisdiction,
            &self.target_label,
            &self.writer_workload_identity,
            &self.approval_reference,
            &self.evidence_reference,
            &self.cost_owner,
            &self.cleanup_owner,
        ]
        .iter()
        .all(|part| !part.trim().is_empty());
        if !complete
            || self.account_id.len() != 12
            || !self.account_id.bytes().all(|byte| byte.is_ascii_digit())
            || self.minimum_retention_days == 0
            || self.cost_ceiling_usd_micros == 0
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "AWS Compliance archive configuration is incomplete or malformed".to_string(),
            ));
        }
        Ok(())
    }
}

/// Exact synthetic object and expected STS identity used for delete-denial proof.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeleteProbeTarget {
    /// Key of the already-retained synthetic object.
    pub object_key: String,
    /// Version of the already-retained synthetic object.
    pub object_version: String,
    /// The STS caller identity expected for the delete probe credentials.
    pub expected_principal_arn: String,
    /// Durable receipt proving the exact identity has effective delete authority.
    pub effective_permission_receipt: String,
}

impl DeleteProbeTarget {
    fn validate(&self) -> Result<(), ObjectLockArchiveError> {
        if self.object_key.trim().is_empty()
            || self.object_version.trim().is_empty()
            || self.expected_principal_arn.trim().is_empty()
            || self.effective_permission_receipt.trim().is_empty()
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "delete proof requires an exact version, STS identity, and effective-permission receipt"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

/// Phase of a durable Object Lock audit record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComplianceArchiveAuditPhase {
    /// Written and verified before an irreversible provider operation.
    Intent,
    /// Written and verified after an operation succeeds.
    Outcome,
    /// Written and verified when a provider operation fails.
    Failure,
}

/// Tenant-safe event persisted by the caller-provided append-only audit port.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComplianceArchiveAuditEvent {
    /// Capability negotiation, immutable put, readback, or deletion probe.
    pub operation: &'static str,
    /// Intent, successful outcome, or failure.
    pub phase: ComplianceArchiveAuditPhase,
    /// Authenticated tenant bound to the record.
    pub tenant_id: String,
    /// Provider account bound to the record.
    pub account_id: String,
    /// Bucket bound to the record.
    pub bucket: String,
    /// Region bound to the record.
    pub region: String,
    /// Object key bound to the record.
    pub object_key: String,
    /// Exact object version when it is known.
    pub object_version: String,
    /// Provider request reference when it is known.
    pub provider_request_id: String,
    /// Durable audit source reference for the target.
    pub evidence_reference: String,
    /// Redacted provider outcome or failure reason.
    pub detail: String,
}

/// Durable append-only audit port. In-memory logs cannot implement this port.
pub trait ComplianceArchiveAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist an event and return its durable record identifier.
    fn append(&self, event: ComplianceArchiveAuditEvent) -> Result<String, ObjectLockArchiveError>;
    /// Verify the precise record before an operation may be reported successful.
    fn verify(
        &self,
        receipt_id: &str,
        event: &ComplianceArchiveAuditEvent,
    ) -> Result<(), ObjectLockArchiveError>;
}

/// Native adapter with independent writer and delete-probe identities.
#[non_exhaustive]
pub struct AwsS3ObjectLockAdapter {
    writer: S3Client,
    delete_probe: S3Client,
    delete_probe_sts: StsClient,
    config: AwsS3ComplianceArchiveConfig,
    residency: ArchiveResidency,
    delete_target: DeleteProbeTarget,
    audit: Arc<dyn ComplianceArchiveAuditSink>,
}

impl core::fmt::Debug for AwsS3ObjectLockAdapter {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AwsS3ObjectLockAdapter")
            .field("writer", &"[redacted AWS SDK client]")
            .field("delete_probe", &"[redacted AWS SDK client]")
            .field("bucket", &"[trusted configuration]")
            .field("region", &self.config.region)
            .field("tenant_id", &self.config.tenant_id)
            .finish()
    }
}

impl AwsS3ObjectLockAdapter {
    /// Construct only from deployment-owned SDK configurations and trusted target data.
    pub fn new(
        writer_config: &aws_types::SdkConfig,
        delete_probe_config: &aws_types::SdkConfig,
        config: AwsS3ComplianceArchiveConfig,
        residency: ArchiveResidency,
        delete_target: DeleteProbeTarget,
        audit: Arc<dyn ComplianceArchiveAuditSink>,
    ) -> Result<Self, ObjectLockArchiveError> {
        config.validate()?;
        delete_target.validate()?;
        if residency.jurisdiction != config.jurisdiction
            || residency.region != config.region
            || residency.location_class.trim().is_empty()
        {
            return Err(ObjectLockArchiveError::CapabilityNegotiationFailed(
                "trusted target and residency configuration do not match".to_string(),
            ));
        }
        Ok(Self {
            writer: S3Client::new(writer_config),
            delete_probe: S3Client::new(delete_probe_config),
            delete_probe_sts: StsClient::new(delete_probe_config),
            config,
            residency,
            delete_target,
            audit,
        })
    }

    fn event(
        &self,
        operation: &'static str,
        phase: ComplianceArchiveAuditPhase,
        key: &str,
        version: &str,
        request_id: &str,
    ) -> ComplianceArchiveAuditEvent {
        ComplianceArchiveAuditEvent {
            operation,
            phase,
            tenant_id: self.config.tenant_id.clone(),
            account_id: self.config.account_id.clone(),
            bucket: self.config.bucket.clone(),
            region: self.config.region.clone(),
            object_key: key.to_string(),
            object_version: version.to_string(),
            provider_request_id: request_id.to_string(),
            evidence_reference: self.config.evidence_reference.clone(),
            detail: String::new(),
        }
    }

    fn persist(
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

    fn failure<T>(
        &self,
        operation: &'static str,
        key: &str,
        version: &str,
        reason: String,
    ) -> Result<T, ObjectLockArchiveError> {
        self.failure_with_request(
            operation,
            key,
            version,
            "provider-request-unavailable",
            reason,
        )
    }

    fn failure_with_request<T>(
        &self,
        operation: &'static str,
        key: &str,
        version: &str,
        request_id: &str,
        reason: String,
    ) -> Result<T, ObjectLockArchiveError> {
        let mut event = self.event(
            operation,
            ComplianceArchiveAuditPhase::Failure,
            key,
            version,
            request_id,
        );
        event.detail = reason.clone();
        self.persist(event)?;
        Err(ObjectLockArchiveError::Backend(reason))
    }

    fn block_on<T>(
        operation: &str,
        future: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, ObjectLockArchiveError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| {
            ObjectLockArchiveError::Backend(format!(
                "AWS S3 {operation} requires a native Tokio runtime"
            ))
        })?;
        tokio::task::block_in_place(|| handle.block_on(future))
            .map_err(ObjectLockArchiveError::Backend)
    }

    fn now_ms() -> Result<u64, ObjectLockArchiveError> {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                ObjectLockArchiveError::Backend("system clock preceded epoch".to_string())
            })?
            .as_millis();
        u64::try_from(millis).map_err(|_| {
            ObjectLockArchiveError::Backend("system clock exceeded u64 range".to_string())
        })
    }

    fn assert_request_binding(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<(), ObjectLockArchiveError> {
        let prefix = format!("audit/{}/", self.config.tenant_id);
        if request.tenant_id != self.config.tenant_id
            || request.expected_residency != self.residency
            || !request.object_key.starts_with(&prefix)
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "tenant, key, or residency did not match the trusted S3 target".to_string(),
            ));
        }
        if request.retention.legal_hold != LegalHold::On
            || request.retention.retain_until_unix_ms % 1_000 != 0
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "S3 Compliance writes require an active hold and a whole-second retain-until time"
                    .to_string(),
            ));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                ObjectLockArchiveError::Backend("system clock preceded epoch".to_string())
            })?
            .as_millis();
        let minimum =
            now.saturating_add(u128::from(self.config.minimum_retention_days) * 86_400_000);
        if u128::from(request.retention.retain_until_unix_ms) < minimum {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "requested retention is shorter than the approved tenant policy".to_string(),
            ));
        }
        Ok(())
    }

    fn exact_readback(
        &self,
        tenant_id: &str,
        key: &str,
        version: &str,
    ) -> Result<ObjectLockRetentionReadback, ObjectLockArchiveError> {
        if tenant_id != self.config.tenant_id || key.trim().is_empty() || version.trim().is_empty()
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "readback was not bound to the configured tenant, key, and version".to_string(),
            ));
        }
        self.persist(self.event(
            "read_retention",
            ComplianceArchiveAuditPhase::Intent,
            key,
            version,
            "",
        ))?;
        let writer = self.writer.clone();
        let bucket = self.config.bucket.clone();
        let owner = self.config.account_id.clone();
        let key_owned = key.to_string();
        let version_owned = version.to_string();
        let result = Self::block_on("retention readback", async move {
            let retention = writer
                .get_object_retention()
                .bucket(&bucket)
                .key(&key_owned)
                .version_id(&version_owned)
                .expected_bucket_owner(&owner)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let hold = writer
                .get_object_legal_hold()
                .bucket(&bucket)
                .key(&key_owned)
                .version_id(&version_owned)
                .expected_bucket_owner(&owner)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let state = retention
                .retention()
                .ok_or_else(|| "S3 returned no retention".to_string())?;
            if state.mode() != Some(&ObjectLockRetentionMode::Compliance) {
                return Err("S3 readback was not COMPLIANCE mode".to_string());
            }
            let until = state
                .retain_until_date()
                .ok_or_else(|| "S3 returned no retain-until date".to_string())?
                .to_millis()
                .map_err(|error| error.to_string())?;
            let until =
                u64::try_from(until).map_err(|_| "S3 retain-until was negative".to_string())?;
            let legal_hold = match hold.legal_hold().and_then(|state| state.status()) {
                Some(ObjectLockLegalHoldStatus::On) => LegalHold::On,
                Some(ObjectLockLegalHoldStatus::Off) => LegalHold::Off,
                _ => return Err("S3 returned no legal-hold state".to_string()),
            };
            let request_id = retention
                .request_id()
                .filter(|request_id| !request_id.is_empty())
                .ok_or_else(|| "S3 retention readback returned no request ID".to_string())?
                .to_string();
            Ok((until, legal_hold, request_id))
        });
        let (until, legal_hold, request_id) = match result {
            Ok(result) => result,
            Err(error) => return self.failure("read_retention", key, version, error.to_string()),
        };
        self.persist(self.event(
            "read_retention",
            ComplianceArchiveAuditPhase::Outcome,
            key,
            version,
            &request_id,
        ))?;
        Ok(ObjectLockRetentionReadback {
            tenant_id: tenant_id.to_string(),
            object_key: key.to_string(),
            object_version: version.to_string(),
            retention: ImmutableRetention {
                retain_until_unix_ms: until,
                legal_hold,
            },
            residency: self.residency.clone(),
        })
    }
}

impl ObjectLockArchiveAdapter for AwsS3ObjectLockAdapter {
    fn negotiate_capabilities(&self) -> Result<ObjectLockCapabilityReport, ObjectLockArchiveError> {
        self.config.validate()?;
        self.delete_target.validate()?;
        self.persist(self.event(
            "capability_negotiation",
            ComplianceArchiveAuditPhase::Intent,
            "",
            "",
            "",
        ))?;
        let writer = self.writer.clone();
        let bucket = self.config.bucket.clone();
        let owner = self.config.account_id.clone();
        let region = self.config.region.clone();
        let capability = Self::block_on("capability negotiation", async move {
            let lock = writer
                .get_object_lock_configuration()
                .bucket(&bucket)
                .expected_bucket_owner(&owner)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let request_id = lock
                .request_id()
                .filter(|request_id| !request_id.is_empty())
                .ok_or_else(|| "S3 Object Lock negotiation returned no request ID".to_string())?
                .to_string();
            let lock = lock
                .object_lock_configuration()
                .ok_or_else(|| "S3 returned no Object Lock configuration".to_string())?;
            let compliant = lock.object_lock_enabled() == Some(&ObjectLockEnabled::Enabled)
                && lock
                    .rule()
                    .and_then(|rule| rule.default_retention())
                    .is_some_and(|retention| {
                        retention.mode() == Some(&ObjectLockRetentionMode::Compliance)
                    });
            let versioning = writer
                .get_bucket_versioning()
                .bucket(&bucket)
                .expected_bucket_owner(&owner)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let location = writer
                .get_bucket_location()
                .bucket(&bucket)
                .expected_bucket_owner(&owner)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let location = location
                .location_constraint()
                .map(|item| item.as_str())
                .unwrap_or("us-east-1");
            if !compliant
                || versioning.status() != Some(&BucketVersioningStatus::Enabled)
                || location != region
            {
                return Err(
                    "S3 target did not prove Object Lock, versioning, and exact region".to_string(),
                );
            }
            Ok(request_id)
        });
        let capability_request_id = match capability {
            Ok(request_id) => request_id,
            Err(error) => return self.failure("capability_negotiation", "", "", error.to_string()),
        };
        let sts = self.delete_probe_sts.clone();
        let expected_principal = self.delete_target.expected_principal_arn.clone();
        let observed = Self::block_on("delete probe STS identity", async move {
            let identity = sts
                .get_caller_identity()
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let arn = identity
                .arn()
                .ok_or_else(|| "STS returned no caller ARN".to_string())?;
            if arn != expected_principal {
                return Err(
                    "delete probe credentials did not match the approved STS identity".to_string(),
                );
            }
            Ok(())
        });
        if let Err(error) = observed {
            return self.failure("capability_negotiation", "", "", error.to_string());
        }
        let delete = self.attempt_delete(
            &self.config.tenant_id,
            &self.delete_target.object_key,
            &self.delete_target.object_version,
        )?;
        if !matches!(delete, DeleteAttempt::Denied { .. }) {
            return Err(ObjectLockArchiveError::DeleteWasAllowed(
                self.delete_target.object_key.clone(),
            ));
        }
        self.persist(self.event(
            "capability_negotiation",
            ComplianceArchiveAuditPhase::Outcome,
            "",
            "",
            &capability_request_id,
        ))?;
        Ok(ObjectLockCapabilityReport {
            contract_version: OBJECT_LOCK_ARCHIVE_CONTRACT_VERSION,
            backend: ObjectLockBackendIdentity::new(
                ObjectLockBackendFamily::S3,
                "aws-s3",
                self.config.target_label.clone(),
            ),
            capabilities: ObjectLockCapabilities::required(),
            evidence_reference: self.config.evidence_reference.clone(),
        })
    }

    fn put_immutable(
        &self,
        request: &ImmutableArchivePut,
    ) -> Result<ImmutableArchiveWriteReceipt, ObjectLockArchiveError> {
        self.assert_request_binding(request)?;
        self.persist(self.event(
            "immutable_put",
            ComplianceArchiveAuditPhase::Intent,
            &request.object_key,
            "",
            "",
        ))?;
        let writer = self.writer.clone();
        let bucket = self.config.bucket.clone();
        let owner = self.config.account_id.clone();
        let key = request.object_key.clone();
        let body = request.body.clone();
        let retain_until = i64::try_from(request.retention.retain_until_unix_ms).map_err(|_| {
            ObjectLockArchiveError::ReadbackMismatch(
                "retain-until exceeded S3 date range".to_string(),
            )
        })?;
        let result = Self::block_on("immutable put", async move {
            let output = writer
                .put_object()
                .bucket(&bucket)
                .key(&key)
                .expected_bucket_owner(&owner)
                .body(ByteStream::from(body))
                .object_lock_mode(ObjectLockMode::Compliance)
                .object_lock_retain_until_date(DateTime::from_millis(retain_until))
                .object_lock_legal_hold_status(ObjectLockLegalHoldStatus::On)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let version = output
                .version_id()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "S3 immutable put returned no version ID".to_string())?
                .to_string();
            let request_id = output
                .request_id()
                .filter(|request_id| !request_id.is_empty())
                .ok_or_else(|| "S3 immutable put returned no request ID".to_string())?
                .to_string();
            Ok((version, request_id))
        });
        let (version, request_id) = match result {
            Ok(value) => value,
            Err(error) => {
                return self.failure("immutable_put", &request.object_key, "", error.to_string())
            }
        };
        let outcome = self.persist(self.event(
            "immutable_put",
            ComplianceArchiveAuditPhase::Outcome,
            &request.object_key,
            &version,
            &request_id,
        ));
        let receipt_id =
            match outcome {
                Ok(receipt_id) => receipt_id,
                Err(error) => return self.failure_with_request(
                    "immutable_put_outcome_persistence",
                    &request.object_key,
                    &version,
                    &request_id,
                    format!(
                        "S3 PutObject succeeded but durable outcome persistence failed: {error}"
                    ),
                ),
            };
        Ok(ImmutableArchiveWriteReceipt {
            tenant_id: request.tenant_id.clone(),
            object_key: request.object_key.clone(),
            object_version: version.clone(),
            audit_receipt: ArchiveAuditReceipt {
                receipt_id,
                tenant_id: request.tenant_id.clone(),
                object_key: request.object_key.clone(),
                object_version: version,
                provider_account: self.config.account_id.clone(),
                bucket: self.config.bucket.clone(),
                region: self.config.region.clone(),
                provider_request_id: request_id,
                recorded_at_unix_ms: Self::now_ms()?,
            },
        })
    }

    fn read_retention(
        &self,
        tenant_id: &str,
        object_key: &str,
        object_version: &str,
    ) -> Result<ObjectLockRetentionReadback, ObjectLockArchiveError> {
        self.exact_readback(tenant_id, object_key, object_version)
    }

    fn attempt_delete(
        &self,
        tenant_id: &str,
        object_key: &str,
        object_version: &str,
    ) -> Result<DeleteAttempt, ObjectLockArchiveError> {
        if tenant_id != self.config.tenant_id
            || object_key != self.delete_target.object_key
            || object_version != self.delete_target.object_version
        {
            return Err(ObjectLockArchiveError::ReadbackMismatch(
                "delete probe must use its configured exact tenant, key, and version".to_string(),
            ));
        }
        self.persist(self.event(
            "pre_expiry_delete_denial",
            ComplianceArchiveAuditPhase::Intent,
            object_key,
            object_version,
            "",
        ))?;
        let client = self.delete_probe.clone();
        let bucket = self.config.bucket.clone();
        let owner = self.config.account_id.clone();
        let key = object_key.to_string();
        let version = object_version.to_string();
        let result = Self::block_on("delete denial probe", async move {
            match client
                .delete_object()
                .bucket(&bucket)
                .key(&key)
                .version_id(&version)
                .expected_bucket_owner(&owner)
                .send()
                .await
            {
                Ok(_) => Ok(None),
                Err(error) => {
                    let denied = error
                        .as_service_error()
                        .is_some_and(|service| service.code() == Some("AccessDenied"));
                    if !denied {
                        return Err(error.to_string());
                    }
                    let request_id = error
                        .request_id()
                        .filter(|request_id| !request_id.is_empty())
                        .ok_or_else(|| "S3 delete denial returned no request ID".to_string())?
                        .to_string();
                    Ok(Some(request_id))
                }
            }
        });
        let result = match result {
            Ok(value) => value,
            Err(error) => {
                return self.failure(
                    "pre_expiry_delete_denial",
                    object_key,
                    object_version,
                    error.to_string(),
                )
            }
        };
        let request_id = match result {
            Some(id) => id,
            None => {
                return self.failure(
                    "pre_expiry_delete_denial",
                    object_key,
                    object_version,
                    "S3 allowed deletion of the exact retained version".to_string(),
                )
            }
        };
        self.persist(self.event(
            "pre_expiry_delete_denial",
            ComplianceArchiveAuditPhase::Outcome,
            object_key,
            object_version,
            &request_id,
        ))?;
        Ok(DeleteAttempt::Denied {
            tenant_id: tenant_id.to_string(),
            object_key: object_key.to_string(),
            object_version: object_version.to_string(),
            reason: format!("s3_access_denied:{request_id}"),
        })
    }
}
