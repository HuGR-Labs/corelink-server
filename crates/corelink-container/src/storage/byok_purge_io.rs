//! Concrete bounded I/O driver for durable BYOK live-object purge claims.
//!
//! A claimed row is never declared verified until the exact R2 key has been
//! deleted, a subsequent HEAD proves absence, and (for Mode B) the exact
//! allocation-qualified envelope has been reclaimed. Individual object
//! failures are written back as retryable outcomes and never poison the rest
//! of the claimed page.

use std::{sync::Arc, time::Duration};

use corelink_handler_cas::DigestAlgo;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{
    byok_backfill::{generation_qualified_suffix, BackfillSurface},
    byok_cas::{allocation_envelope_blob_key, ByokEnvelopeStore, D1ByokEnvelopeStore},
    byok_generation_catalog::{
        generation_qualified_digest, ByokObjectKind, ByokPurgePlan, ByokPurgeReason,
        ByokPurgeReconciler, D1ByokGenerationCatalog,
        COMMON_PURGE_INVALID_IDENTITY_QUARANTINE_AFTER,
    },
    d1_http::D1HttpClient,
    env_or,
    r2_s3::R2S3Client,
    StorageEnv,
};

const MAX_PURGE_PAGE: u32 = 1;
// One page claim plus the five slow-path operations (DELETE, HEAD, absence
// checkpoint, envelope reclaim, final completion) consume at most 90 seconds,
// strictly below the D1 claim's 120-second lease.
const MAX_OPERATION_TIMEOUT: Duration = Duration::from_secs(15);
const STALE_CENSUS_TIMEOUT: Duration = Duration::from_millis(500);
const DEFAULT_CAS_BUCKET: &str = "corelink-cas-prod";
const DEFAULT_AC_BUCKET: &str = "corelink-ac-iad";
const MAX_PERSISTED_ERROR_BYTES: usize = 512;

/// Bounded work and wall-clock limits for one reconciler page.
#[derive(Clone, Copy, Debug)]
pub struct ByokPurgeIoConfig {
    /// Maximum rows claimed in one page (hard-capped at 128).
    pub page_limit: u32,
    /// Outer deadline for each R2, envelope, or D1 completion operation.
    pub operation_timeout: Duration,
}

impl Default for ByokPurgeIoConfig {
    fn default() -> Self {
        Self {
            page_limit: 1,
            operation_timeout: Duration::from_secs(15),
        }
    }
}

impl ByokPurgeIoConfig {
    fn validate(self) -> Result<Self, String> {
        if self.page_limit == 0 || self.page_limit > MAX_PURGE_PAGE {
            return Err(format!(
                "BYOK purge page_limit must be between 1 and {MAX_PURGE_PAGE}"
            ));
        }
        if self.operation_timeout.is_zero() || self.operation_timeout > MAX_OPERATION_TIMEOUT {
            return Err(format!(
                "BYOK purge operation_timeout must be positive and at most {} seconds",
                MAX_OPERATION_TIMEOUT.as_secs()
            ));
        }
        Ok(self)
    }
}

/// Aggregate result of one bounded claim page. Item failures are deliberately
/// reported instead of aborting the page, preserving poison-item isolation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByokPurgePageReport {
    /// Rows returned by the durable claim operation.
    pub claimed: u32,
    /// Rows fully verified, including Mode-B envelope reclaim where required.
    pub verified: u32,
    /// Rows returned to a retryable ledger state.
    pub retryable: u32,
    /// Rows terminally quarantined for a bounded invalid-identity fault.
    pub quarantined: u32,
    /// Rows whose exact claim expired or was lost before completion persisted.
    pub completion_conflicts: u32,
}

/// Production purge driver over separate CAS/AC buckets and the D1 envelope
/// store. The durable reconciler remains the sole claim/state authority.
#[derive(Debug)]
pub struct D1R2ByokPurgeDriver {
    reconciler: Arc<dyn ByokPurgeReconciler>,
    cas: Arc<R2S3Client>,
    ac: Arc<R2S3Client>,
    envelopes: Arc<dyn ByokEnvelopeStore>,
    tdk: TenantDerivationKey,
    cas_region: String,
    ac_region: String,
    config: ByokPurgeIoConfig,
}

impl D1R2ByokPurgeDriver {
    /// Build the concrete production driver from the same bucket environment
    /// used by live CAS/AC handlers.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid bounds, incomplete storage configuration,
    /// or failure to construct either R2 client.
    pub async fn from_env(
        d1: Arc<D1HttpClient>,
        config: ByokPurgeIoConfig,
    ) -> Result<Self, String> {
        let config = config.validate()?;
        let storage = StorageEnv::from_env()
            .ok_or_else(|| "R2/D1 storage environment is incomplete".to_owned())?;
        let cas_bucket = env_or("R2_CAS_BUCKET", DEFAULT_CAS_BUCKET);
        let ac_bucket = env_or("R2_AC_BUCKET", DEFAULT_AC_BUCKET);
        let cas_region = env_or("R2_CAS_REGION", "iad");
        let ac_region = env_or("R2_AC_REGION", "iad");
        let tdk = load_tdk_from_env()?;
        let cas = Arc::new(R2S3Client::new(&storage, cas_bucket).await?);
        let ac = Arc::new(R2S3Client::new(&storage, ac_bucket).await?);
        let reconciler: Arc<dyn ByokPurgeReconciler> =
            Arc::new(D1ByokGenerationCatalog::new(Arc::clone(&d1)));
        let envelopes: Arc<dyn ByokEnvelopeStore> = Arc::new(D1ByokEnvelopeStore::new(d1));
        Ok(Self {
            reconciler,
            cas,
            ac,
            envelopes,
            tdk,
            cas_region,
            ac_region,
            config,
        })
    }

    /// Compose the driver from concrete production-compatible collaborators.
    /// This seam allows a supervisor to share already-constructed clients.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured bounds are invalid.
    #[allow(
        clippy::too_many_arguments,
        reason = "the purge driver requires explicit independent storage and identity collaborators"
    )]
    pub fn new(
        reconciler: Arc<dyn ByokPurgeReconciler>,
        cas: Arc<R2S3Client>,
        ac: Arc<R2S3Client>,
        envelopes: Arc<dyn ByokEnvelopeStore>,
        tdk: TenantDerivationKey,
        cas_region: impl Into<String>,
        ac_region: impl Into<String>,
        config: ByokPurgeIoConfig,
    ) -> Result<Self, String> {
        Ok(Self {
            reconciler,
            cas,
            ac,
            envelopes,
            tdk,
            cas_region: cas_region.into(),
            ac_region: ac_region.into(),
            config: config.validate()?,
        })
    }

    /// Claim and process at most one bounded page. Failure of one physical
    /// object does not prevent later claims in the page from being attempted.
    ///
    /// # Errors
    ///
    /// Returns an error only when the bounded page claim itself fails or times
    /// out. Census failures are non-fatal and retry on the next supervisor
    /// tick; per-object errors are captured in the returned report.
    pub async fn run_page(&self) -> Result<ByokPurgePageReport, String> {
        match tokio::time::timeout(
            STALE_CENSUS_TIMEOUT,
            self.reconciler.reconcile_stale_page(self.config.page_limit),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "BYOK background stale census deferred")
            }
            Err(_) => tracing::warn!("BYOK background stale census timed out"),
        }
        let plans = tokio::time::timeout(
            self.config.operation_timeout,
            self.reconciler.claim_purge_page(self.config.page_limit),
        )
        .await
        .map_err(|_| "BYOK purge claim page timed out".to_owned())??;
        let mut report = ByokPurgePageReport {
            claimed: u32::try_from(plans.len()).unwrap_or(MAX_PURGE_PAGE),
            ..ByokPurgePageReport::default()
        };
        // Claims share a finite D1 lease, so process the bounded page
        // concurrently. Sequential processing could let later claims expire
        // behind one slow R2 object even though every individual call is timed.
        let outcomes =
            futures::future::join_all(plans.iter().map(|plan| self.process_claim(plan))).await;
        for outcome in outcomes {
            match outcome {
                ClaimOutcome::Verified => report.verified += 1,
                ClaimOutcome::Retryable => report.retryable += 1,
                ClaimOutcome::Quarantined => report.quarantined += 1,
                ClaimOutcome::CompletionConflict => report.completion_conflicts += 1,
            }
        }
        Ok(report)
    }

    async fn process_claim(&self, plan: &ByokPurgePlan) -> ClaimOutcome {
        if let Err(error) = self.validate_physical_identity(plan) {
            // This is a durable ledger/identity defect, not an R2 outage.
            // Never issue DELETE for an unvalidated key; after the bounded
            // identity threshold the exact row is quarantined so it cannot
            // monopolize the one-item work page forever.
            return self.record_invalid_identity(plan, &error).await;
        }
        if !plan.r2_absent {
            let r2 = match plan.kind {
                ByokObjectKind::Cas => &self.cas,
                ByokObjectKind::Ac => &self.ac,
            };
            let delete_error = self
                .bounded(r2.delete(&plan.object.physical_key), "R2 DELETE")
                .await
                .err();
            // HEAD is mandatory even when DELETE returned an error: R2 may
            // have committed the idempotent delete while its response was
            // lost. Only the post-delete presence verdict is authoritative.
            match self
                .bounded(r2.head_size(&plan.object.physical_key), "R2 HEAD")
                .await
            {
                Ok(None) => {}
                Ok(Some(_)) => {
                    let error = delete_error
                        .as_deref()
                        .unwrap_or("R2 HEAD remained present after DELETE");
                    return self.record_retry(plan, false, error).await;
                }
                Err(head_error) => {
                    let error = delete_error
                        .map(|delete_error| format!("{delete_error}; {head_error}"))
                        .unwrap_or(head_error);
                    return self.record_retry(plan, false, &error).await;
                }
            }
            // Persist the absence checkpoint before touching key material. A
            // crash after this point resumes directly at envelope reclaim.
            if self.finish(plan, true, None, false, None).await.is_err() {
                return ClaimOutcome::CompletionConflict;
            }
        }

        if let Err(error) = self.reclaim_envelope(plan).await {
            return self.record_retry(plan, true, &error).await;
        }
        match self.finish(plan, true, None, true, None).await {
            Ok(()) => ClaimOutcome::Verified,
            Err(()) => ClaimOutcome::CompletionConflict,
        }
    }

    async fn reclaim_envelope(&self, plan: &ByokPurgePlan) -> Result<(), String> {
        match plan.crypto_mode.as_str() {
            "plaintext" if plan.object.generation == 0 => Ok(()),
            "convergent" => Ok(()),
            "random" => {
                let (surface, digest) = envelope_identity(plan.kind, &plan.object.logical_key)?;
                let key =
                    allocation_envelope_blob_key(surface, digest, &plan.object.allocation_id)?;
                self.bounded(
                    self.envelopes.delete_envelope(&plan.tenant_id, &key),
                    "Mode-B envelope reclaim",
                )
                .await
            }
            other => Err(format!(
                "unsupported positive-generation purge crypto mode {other:?}"
            )),
        }
    }

    fn validate_physical_identity(&self, plan: &ByokPurgePlan) -> Result<(), String> {
        let tenant = Uuid::parse_str(&plan.tenant_id)
            .map_err(|_| "BYOK purge tenant_id must be a canonical UUID".to_owned())?;
        if tenant.to_string() != plan.tenant_id {
            return Err("BYOK purge tenant_id must use canonical UUID spelling".to_owned());
        }
        let prefix = derive_prefix(&self.tdk, tenant).to_string();
        let region = match plan.kind {
            ByokObjectKind::Cas => self.cas_region.as_str(),
            ByokObjectKind::Ac => self.ac_region.as_str(),
        };
        if !is_path_segment(region) {
            return Err("BYOK purge region is not a canonical path segment".to_owned());
        }
        let (_surface, digest) = envelope_identity(plan.kind, &plan.object.logical_key)?;
        let expected = if plan.object.generation == 0 {
            if plan.reason != ByokPurgeReason::ActivationSource
                || !plan.object.allocation_id.is_empty()
                || plan.crypto_mode != "plaintext"
            {
                return Err(
                    "generation-zero purge is not an activation plaintext source".to_owned(),
                );
            }
            match plan.kind {
                ByokObjectKind::Cas => R2S3Client::blob_key(
                    region,
                    &prefix,
                    digest,
                    cas_digest_algorithm(&plan.object.logical_key)?,
                ),
                ByokObjectKind::Ac => {
                    R2S3Client::blob_key(region, &prefix, digest, DigestAlgo::Blake3)
                }
            }
        } else {
            if !is_path_segment(&plan.object.allocation_id) {
                return Err("positive-generation purge has invalid allocation_id".to_owned());
            }
            let backfill_surface = match plan.kind {
                ByokObjectKind::Cas => BackfillSurface::Cas,
                ByokObjectKind::Ac => BackfillSurface::Ac,
            };
            let backfill = format!(
                "{region}/{prefix}/{}",
                generation_qualified_suffix(
                    backfill_surface,
                    plan.object.generation,
                    &plan.object.logical_key,
                )
            );
            if plan.object.physical_key == backfill {
                return Ok(());
            }
            let terminal_digest = plan
                .object
                .physical_key
                .rsplit('/')
                .next()
                .filter(|value| is_canonical_digest(value))
                .ok_or_else(|| {
                    "BYOK purge physical key lacks canonical terminal digest".to_owned()
                })?;
            let live_suffix = generation_qualified_digest(
                plan.object.generation,
                &plan.object.allocation_id,
                terminal_digest,
            )?;
            let live = match plan.kind {
                ByokObjectKind::Cas => R2S3Client::blob_key(
                    region,
                    &prefix,
                    &live_suffix,
                    cas_digest_algorithm(&plan.object.logical_key)?,
                ),
                ByokObjectKind::Ac => {
                    R2S3Client::blob_key(region, &prefix, &live_suffix, DigestAlgo::Blake3)
                }
            };
            if plan.object.physical_key == live {
                return Ok(());
            }
            return Err(
                "BYOK purge physical key does not match its exact tenant identity".to_owned(),
            );
        };
        if plan.object.physical_key != expected {
            return Err(
                "BYOK purge physical key does not match its exact tenant identity".to_owned(),
            );
        }
        Ok(())
    }

    async fn record_retry(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: &str,
    ) -> ClaimOutcome {
        let error = bounded_error(error);
        match self
            .finish(plan, head_absent, Some(&error), false, None)
            .await
        {
            Ok(()) => ClaimOutcome::Retryable,
            Err(()) => ClaimOutcome::CompletionConflict,
        }
    }

    async fn record_invalid_identity(&self, plan: &ByokPurgePlan, error: &str) -> ClaimOutcome {
        let error = bounded_error(error);
        match self
            .finish(plan, plan.r2_absent, Some(&error), false, Some(&error))
            .await
        {
            Ok(()) if plan.attempts >= COMMON_PURGE_INVALID_IDENTITY_QUARANTINE_AFTER => {
                ClaimOutcome::Quarantined
            }
            Ok(()) => ClaimOutcome::Retryable,
            Err(()) => ClaimOutcome::CompletionConflict,
        }
    }

    async fn finish(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: Option<&str>,
        envelope_reclaimed: bool,
        invalid_identity_reason: Option<&str>,
    ) -> Result<(), ()> {
        match tokio::time::timeout(
            self.config.operation_timeout,
            self.reconciler.finish_claimed_purge_attempt(
                plan,
                head_absent,
                error,
                envelope_reclaimed,
                invalid_identity_reason,
            ),
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(()),
        }
    }

    async fn bounded<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, String>>,
        label: &str,
    ) -> Result<T, String> {
        tokio::time::timeout(self.config.operation_timeout, operation)
            .await
            .map_err(|_| format!("{label} timed out"))?
            .map_err(|error| format!("{label}: {error}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaimOutcome {
    Verified,
    Retryable,
    Quarantined,
    CompletionConflict,
}

fn envelope_identity(
    kind: ByokObjectKind,
    logical_key: &str,
) -> Result<(&'static str, &str), String> {
    match kind {
        ByokObjectKind::Ac if is_canonical_digest(logical_key) => Ok(("ac", logical_key)),
        ByokObjectKind::Ac => Err("AC purge logical key must be lowercase 64-hex".to_owned()),
        ByokObjectKind::Cas => {
            let (algorithm, digest) = logical_key
                .split_once(':')
                .ok_or_else(|| "CAS purge logical key must be algorithm-qualified".to_owned())?;
            if !matches!(algorithm, "blake3" | "sha256") || !is_canonical_digest(digest) {
                return Err("CAS purge logical key has an invalid algorithm/digest".to_owned());
            }
            Ok(("cas", digest))
        }
    }
}

fn is_canonical_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn is_path_segment(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && !value
            .chars()
            .any(|character| matches!(character, '/' | '\0'))
}

fn cas_digest_algorithm(logical_key: &str) -> Result<DigestAlgo, String> {
    match logical_key.split_once(':').map(|(algorithm, _)| algorithm) {
        Some("blake3") => Ok(DigestAlgo::Blake3),
        Some("sha256") => Ok(DigestAlgo::Sha256),
        _ => Err("CAS purge logical key has an invalid algorithm".to_owned()),
    }
}

fn load_tdk_from_env() -> Result<TenantDerivationKey, String> {
    let raw = std::env::var("R2_TDK_HEX")
        .map_err(|_| "R2_TDK_HEX is required for BYOK purge path validation".to_owned())?;
    let raw = raw.trim();
    if raw.len() != 64 {
        return Err("R2_TDK_HEX must contain exactly 64 hex characters".to_owned());
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(raw, bytes.as_mut())
        .map_err(|_| "R2_TDK_HEX is not valid hex".to_owned())?;
    Ok(TenantDerivationKey::from_bytes(bytes))
}

fn bounded_error(error: &str) -> String {
    if error.len() <= MAX_PERSISTED_ERROR_BYTES {
        return error.to_owned();
    }
    let mut end = MAX_PERSISTED_ERROR_BYTES;
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_b_reclaim_uses_raw_digest_and_exact_allocation() {
        let digest = "a".repeat(64);
        let logical_key = format!("sha256:{digest}");
        let (surface, parsed_digest) =
            envelope_identity(ByokObjectKind::Cas, &logical_key).expect("CAS identity");
        assert_eq!(surface, "cas");
        assert_eq!(parsed_digest, digest);
        assert_eq!(
            allocation_envelope_blob_key(surface, parsed_digest, "allocation"),
            Ok(format!("cas:{digest}:allocation:allocation"))
        );

        assert_eq!(
            envelope_identity(ByokObjectKind::Ac, &digest),
            Ok(("ac", digest.as_str()))
        );
    }

    #[test]
    fn malformed_cas_identity_and_unbounded_config_are_rejected() {
        assert!(envelope_identity(ByokObjectKind::Cas, "abc").is_err());
        assert!(ByokPurgeIoConfig {
            page_limit: MAX_PURGE_PAGE + 1,
            ..ByokPurgeIoConfig::default()
        }
        .validate()
        .is_err());
    }
}
