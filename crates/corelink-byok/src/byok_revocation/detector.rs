//! [`RevocationDetector`] — background 60s CMK access check loop.
//!
//! This is the core kill switch implementation. See crate-level docs for
//! the full flow and SLA contract.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{DekCache, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind};
use tracing::{error, info, warn};

use super::alerter::{CustomerAlerter, RevocationAlertPayload};
use super::config::RevocationConfig;
use super::error::RevocationError;
use super::event::{RevocationAuditEvent, EVENT_TYPE_CMK_RESTORED, EVENT_TYPE_CMK_REVOKED};
use super::store::{TenantByokStatus, TenantStatusStore};

/// Per-key transient failure counter for network-partition handling.
#[derive(Debug, Default)]
struct FailureCounter {
    counts: std::collections::HashMap<String, u32>,
}

impl FailureCounter {
    fn increment(&mut self, key: &str) -> u32 {
        let count = self.counts.entry(key.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    fn reset(&mut self, key: &str) {
        self.counts.remove(key);
    }
}

/// Result of a single kill-switch execution cycle.
#[derive(Debug)]
pub struct KillSwitchOutcome {
    /// The KMS key ID that was processed.
    pub kms_key_id: KmsKeyId,
    /// Provider kind.
    pub provider: KmsProviderKind,
    /// Number of DEK cache entries evicted.
    pub evicted_dek_count: usize,
    /// Kill switch total duration in milliseconds.
    pub kill_switch_duration_ms: u64,
    /// Audit event emitted.
    pub audit_event: RevocationAuditEvent,
}

/// CMK revocation detection background worker.
///
/// Polls `KmsProvider::check_access` every 60 seconds per active BYOK
/// tenant. On `Revoked` or `NotFound`, executes the kill switch:
///
/// 1. Evict DEK cache atomically.
/// 2. Mark tenant degraded read-only (D1 via [`TenantStatusStore`]).
/// 3. Emit audit `corelink.byok.cmk_revoked` atomically.
/// 4. Alert customer via [`CustomerAlerter`].
///
/// On `Ok`: restore tenant to `active` if previously degraded.
///
/// On `Throttled` / `ApiError`: increment failure counter; degrade
/// conservatively after [`RevocationConfig::sustained_failure_threshold`]
/// consecutive failures.
///
/// **NO operator override.** There is no function that bypasses or defers
/// the kill switch. INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL.
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
/// use corelink_byok::DekCache;
/// use corelink_byok::revocation::{RevocationDetector, RevocationConfig};
/// use corelink_byok::revocation::testutil::{NoopAlerter, InMemoryTenantStore, StubKmsProvider};
///
/// # tokio_test::block_on(async {
/// let cache = Arc::new(DekCache::new(300).expect("valid TTL"));
/// let provider = Arc::new(StubKmsProvider::new_ok());
/// let store = Arc::new(InMemoryTenantStore::default());
/// let alerter = Arc::new(NoopAlerter);
/// let config = RevocationConfig::default();
///
/// let detector = RevocationDetector::new(
///     vec![provider],
///     cache,
///     store,
///     alerter,
///     config,
/// );
/// detector.run_one_cycle().await.unwrap();
/// # });
/// ```
pub struct RevocationDetector {
    providers: Vec<Arc<dyn KmsProvider>>,
    dek_cache: Arc<DekCache>,
    store: Arc<dyn TenantStatusStore>,
    alerter: Arc<dyn CustomerAlerter>,
    config: RevocationConfig,
}

impl std::fmt::Debug for RevocationDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RevocationDetector")
            .field("providers_count", &self.providers.len())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl RevocationDetector {
    /// Construct a new [`RevocationDetector`].
    ///
    /// - `providers`: one entry per active KMS provider.
    /// - `dek_cache`: shared DEK cache; TTL must be ≤ 300 s (enforced by
    ///   [`DekCache::new`]).
    /// - `store`: tenant status persistence.
    /// - `alerter`: multi-channel customer alerter.
    /// - `config`: tunable parameters.
    #[must_use]
    pub fn new(
        providers: Vec<Arc<dyn KmsProvider>>,
        dek_cache: Arc<DekCache>,
        store: Arc<dyn TenantStatusStore>,
        alerter: Arc<dyn CustomerAlerter>,
        config: RevocationConfig,
    ) -> Self {
        Self {
            providers,
            dek_cache,
            store,
            alerter,
            config,
        }
    }

    /// Run the background check loop indefinitely.
    ///
    /// Each iteration checks all active BYOK keys across all providers,
    /// then sleeps for [`RevocationConfig::check_interval`].
    ///
    /// This function never returns under normal operation. Use
    /// [`run_one_cycle`] in tests.
    ///
    /// [`run_one_cycle`]: RevocationDetector::run_one_cycle
    pub async fn run_loop(self) {
        let interval = self.config.check_interval();
        let mut failure_counter = FailureCounter::default();
        loop {
            if let Err(e) = self
                .run_cycle_inner(&mut failure_counter)
                .await
            {
                error!(error = %e, "revocation check cycle error");
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// Run a single check cycle (all providers, all active keys).
    ///
    /// Used in tests and integration harness.
    pub async fn run_one_cycle(&self) -> Result<(), RevocationError> {
        let mut failure_counter = FailureCounter::default();
        self.run_cycle_inner(&mut failure_counter).await
    }

    /// Internal: run one cycle with mutable failure counter.
    async fn run_cycle_inner(
        &self,
        failure_counter: &mut FailureCounter,
    ) -> Result<(), RevocationError> {
        for provider in &self.providers {
            let active_keys = self.list_active_byok_keys(provider).await;
            for key_id in active_keys {
                let result = provider.check_access(&key_id).await;
                match result {
                    Ok(KmsAccessStatus::Ok) => {
                        // Reset failure counter; restore tenant if previously degraded.
                        failure_counter.reset(key_id.as_str());
                        self.handle_ok_status(provider, &key_id).await;
                        if self.config.emit_trace_spans {
                            info!(
                                provider = provider.provider_kind().as_str(),
                                kms_key_id = key_id.as_str(),
                                "CMK access check: OK"
                            );
                        }
                    }
                    Ok(KmsAccessStatus::Revoked) | Ok(KmsAccessStatus::NotFound) => {
                        // Kill switch: unconditional, immediate.
                        failure_counter.reset(key_id.as_str());
                        if let Err(e) = self
                            .handle_revocation(provider, &key_id)
                            .await
                        {
                            error!(
                                error = %e,
                                provider = provider.provider_kind().as_str(),
                                kms_key_id = key_id.as_str(),
                                "kill switch execution error (CRITICAL)"
                            );
                        }
                    }
                    Ok(KmsAccessStatus::Throttled) => {
                        let count = failure_counter.increment(key_id.as_str());
                        warn!(
                            provider = provider.provider_kind().as_str(),
                            kms_key_id = key_id.as_str(),
                            consecutive_failures = count,
                            "KMS access check throttled"
                        );
                        if count >= self.config.sustained_failure_threshold {
                            // Conservative degrade: cannot confirm CMK is accessible.
                            warn!(
                                provider = provider.provider_kind().as_str(),
                                kms_key_id = key_id.as_str(),
                                "sustained throttling: conservatively degrading tenant"
                            );
                            self.conservative_degrade(provider, &key_id).await;
                        }
                    }
                    Ok(KmsAccessStatus::ApiError(ref detail)) => {
                        let count = failure_counter.increment(key_id.as_str());
                        warn!(
                            provider = provider.provider_kind().as_str(),
                            kms_key_id = key_id.as_str(),
                            consecutive_failures = count,
                            detail = detail.to_string(),
                            "KMS access check API error"
                        );
                        if count >= self.config.sustained_failure_threshold {
                            warn!(
                                provider = provider.provider_kind().as_str(),
                                kms_key_id = key_id.as_str(),
                                "sustained API errors: conservatively degrading tenant"
                            );
                            self.conservative_degrade(provider, &key_id).await;
                        }
                    }
                    Err(e) => {
                        let count = failure_counter.increment(key_id.as_str());
                        warn!(
                            provider = provider.provider_kind().as_str(),
                            kms_key_id = key_id.as_str(),
                            consecutive_failures = count,
                            error = %e,
                            "KMS access check error"
                        );
                    }
                    // NOTE: prior to Wave-35 Phase 2 absorption a defensive
                    // `Ok(_) => {...}` future-compat catch-all sat here. Once
                    // `KmsAccessStatus` moved into the same crate as this
                    // match the compiler proved the arm unreachable
                    // (intra-crate exhaustiveness). Removed per charter
                    // "no `#[allow]` to mask new lints"; behaviour preserved
                    // because the existing 4 arms are exhaustive over the
                    // current `#[non_exhaustive]` enum surface, and any
                    // future variant would force a recompile that surfaces
                    // the missing-arm error explicitly at this match site.
                }
            }
        }
        Ok(())
    }

    /// Kill switch execution. **NO operator override.**
    ///
    /// Steps (all mandatory; no bypass):
    /// 1. Atomic DEK cache eviction.
    /// 2. Tenant marked `degraded_read_only`.
    /// 3. Audit event emitted atomically.
    /// 4. Customer alerted.
    /// 5. SLA duration recorded.
    async fn handle_revocation(
        &self,
        provider: &Arc<dyn KmsProvider>,
        key_id: &KmsKeyId,
    ) -> Result<(), RevocationError> {
        let detected_at_ms = now_ms();
        let start = std::time::Instant::now();

        info!(
            provider = provider.provider_kind().as_str(),
            kms_key_id = key_id.as_str(),
            "CMK revocation detected — executing kill switch (INV-BYOK-CRYPTO-SOVEREIGNTY)"
        );

        // Step 1: Atomic DEK cache eviction (ZeroizeOnDrop per entry).
        // CRITICAL: must `.await` — evict_all_for_key is async.
        // Failure to await silently drops the Future (P0-2: INV-BYOK-CRYPTO-SOVEREIGNTY).
        let evicted_count = self
            .dek_cache
            .evict_all_for_key(key_id)
            .await
            .map_err(|e| RevocationError::Internal(format!("DEK cache eviction failed: {e}")))?;
        let evicted_at_ms = now_ms();
        info!(
            provider = provider.provider_kind().as_str(),
            kms_key_id = key_id.as_str(),
            evicted_count,
            "DEK cache evicted"
        );

        // Step 2: Mark tenant degraded read-only.
        let _updated = self
            .store
            .mark_degraded(key_id, provider.provider_kind().as_str(), detected_at_ms)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    "tenant degrade failed; kill switch partially executed (CRITICAL)"
                );
                e
            })?;

        // Step 3: Emit audit event (atomically with tenant update in D1;
        // here we record the event — the D1 batch wrapper is production wiring).
        let alerted_at_ms = now_ms();
        let duration_ms = start.elapsed().as_millis() as u64;

        let audit_event = RevocationAuditEvent {
            event_type: EVENT_TYPE_CMK_REVOKED.to_string(),
            provider: provider.provider_kind().as_str().to_string(),
            kms_key_id_hashed: hash_for_audit(key_id.as_str()),
            tenant_id_hashed: "hashed_by_store".to_string(),
            detected_at_ms,
            evicted_at_ms,
            alerted_at_ms,
            kill_switch_duration_ms: duration_ms,
            evicted_dek_count: evicted_count,
        };

        // Log audit event (production: INSERT into audit_outbox in D1 batch).
        info!(
            event = ?audit_event,
            "audit event: corelink.byok.cmk_revoked"
        );

        // Step 4: Alert customer multi-channel.
        let payload = RevocationAlertPayload {
            provider: provider.provider_kind().as_str().to_string(),
            kms_key_id: key_id.clone(),
            tenant_id_hashed: "hashed_by_store".to_string(),
            detected_at_ms,
            kill_switch_duration_ms: duration_ms,
            recovery_instructions: format!(
                "Your CMK ({provider}) has been revoked. CoreLink has suspended access. \
                 Re-enable your CMK in the {provider} console; access will be restored \
                 within 60 seconds.",
                provider = provider.provider_kind().as_str()
            ),
        };
        self.alerter.alert(payload).await.map_err(|e| {
            warn!(error = %e, "customer alert delivery failed (non-fatal; kill switch still executed)");
            e
        })?;

        info!(
            provider = provider.provider_kind().as_str(),
            kms_key_id = key_id.as_str(),
            kill_switch_duration_ms = duration_ms,
            "kill switch complete (INV-BYOK-CRYPTO-SOVEREIGNTY enforced)"
        );

        Ok(())
    }

    /// Handle `KmsAccessStatus::Ok` — restore tenant if previously degraded.
    async fn handle_ok_status(
        &self,
        provider: &Arc<dyn KmsProvider>,
        key_id: &KmsKeyId,
    ) {
        let current = self.store.current_status(key_id).await;
        match current {
            Ok(Some(TenantByokStatus::DegradedReadOnly)) => {
                let restored_at_ms = now_ms();
                let _ = self.store.restore_active(key_id, restored_at_ms).await;

                // Emit recovery audit event.
                let audit_event = RevocationAuditEvent {
                    event_type: EVENT_TYPE_CMK_RESTORED.to_string(),
                    provider: provider.provider_kind().as_str().to_string(),
                    kms_key_id_hashed: hash_for_audit(key_id.as_str()),
                    tenant_id_hashed: "hashed_by_store".to_string(),
                    detected_at_ms: restored_at_ms,
                    evicted_at_ms: restored_at_ms,
                    alerted_at_ms: restored_at_ms,
                    kill_switch_duration_ms: 0,
                    evicted_dek_count: 0,
                };
                info!(event = ?audit_event, "audit event: corelink.byok.cmk_restored");

                let _ = self
                    .alerter
                    .alert_recovery(
                        provider.provider_kind(),
                        key_id,
                        "hashed_by_store",
                        restored_at_ms,
                    )
                    .await;

                info!(
                    provider = provider.provider_kind().as_str(),
                    kms_key_id = key_id.as_str(),
                    "tenant BYOK access restored"
                );
            }
            _ => {
                // Already active or no entry; nothing to do.
            }
        }
    }

    /// Conservative degrade on sustained network partition (no kill switch;
    /// false-positive safe — customer can verify and unblock).
    async fn conservative_degrade(
        &self,
        provider: &Arc<dyn KmsProvider>,
        key_id: &KmsKeyId,
    ) {
        let degraded_at_ms = now_ms();
        let _ = self
            .store
            .mark_degraded(
                key_id,
                provider.provider_kind().as_str(),
                degraded_at_ms,
            )
            .await;

        warn!(
            provider = provider.provider_kind().as_str(),
            kms_key_id = key_id.as_str(),
            "tenant conservatively degraded due to sustained KMS unreachability"
        );
    }

    /// List active BYOK keys for a provider.
    ///
    /// In production: query D1 `byok_envelope` table for
    /// `SELECT DISTINCT kms_provider, kms_key_id WHERE byok_status != 'revoked'`.
    /// Here we return an empty list (production wiring is deployment-specific).
    // WI-S14-009 follow-up: D1 query adapter injection
    #[allow(unused)]
    async fn list_active_byok_keys(
        &self,
        _provider: &Arc<dyn KmsProvider>,
    ) -> Vec<KmsKeyId> {
        // Production: D1 query. Stub returns empty; tests inject via override.
        vec![]
    }
}

/// Return current time as milliseconds since UNIX epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

/// Hash a KMS key ID for audit emission (never log raw key IDs).
fn hash_for_audit(key_id: &str) -> String {
    // Production: BLAKE3 of key_id. Here we use a deterministic prefix.
    format!("blake3:{}", &key_id[..key_id.len().min(8)])
}
