//! Typed protocol for crash-resumable BYOK activation.
//!
//! The target generation becomes catalog-visible in `Partial`, while every
//! data-plane operation remains fail-closed. Only verified absence of every
//! exact source physical object permits the final transition to `Active`.

use std::time::Duration;

use async_trait::async_trait;

use super::byok_backfill::{BackfillError, BackfillSurface};
use super::d1_http::D1HttpClient;

/// Fail-closed rollout probe for the completion marker written last by 0121.
pub async fn require_0121_capability(d1: &D1HttpClient) -> Result<(), BackfillError> {
    let rows = d1
        .query(
            "SELECT singleton,schema_version FROM byok_0121_capability \
             WHERE singleton=1 AND schema_version=2 LIMIT 1",
            &[],
        )
        .await
        .map_err(|error| BackfillError::Store(format!("0121 capability probe: {error}")))?;
    let valid = rows.first().is_some_and(|row| {
        row.get("singleton").and_then(serde_json::Value::as_i64) == Some(1)
            && row
                .get("schema_version")
                .and_then(serde_json::Value::as_i64)
                == Some(2)
    });
    if !valid {
        return Err(BackfillError::Store(
            "0121 activation schema is absent or incomplete".to_owned(),
        ));
    }
    Ok(())
}

/// Durable activation phase. Every scheduler call performs bounded work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationPhase {
    /// Source identities are discovered, decrypted and copied.
    Copy,
    /// Target catalog is published and tenant configuration is `partial`.
    PublishedPartial,
    /// Exact source physical objects are being deleted and verified absent.
    Purging,
    /// Every source purge has been verified and finalization may run.
    ReadyFinalize,
    /// Configuration is active; the pipeline is terminal.
    Committed,
    /// No target was published and the activation returned to inactive.
    Aborted,
    /// A higher-priority deactivate/shred transition revoked this activation.
    Preempted,
}

impl ActivationPhase {
    /// Whether no worker may advance this intent further.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Aborted | Self::Preempted)
    }
}

/// Durable KMS-detector suspension. The 0119 degrade transition pauses work
/// without discarding phase; restore resumes the exact phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationSuspension {
    /// The worker may advance the durable phase.
    Active,
    /// The detector has placed the tenant in degraded read-only mode.
    DegradedReadOnly,
}

/// Configuration and secret identity pinned for the entire activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedActivationPolicy {
    /// Immutable tenant configuration version.
    pub config_version: i64,
    /// Customer custody mode (`byok` or `hyok`).
    pub mode: String,
    /// Target crypto mode (`convergent` or `random`).
    pub crypto_mode: String,
    /// KMS provider pinned at prepare time.
    pub cmk_provider: Option<String>,
    /// Exact CMK resource identity.
    pub cmk_key_id: Option<String>,
    /// Exact KMS residency region.
    pub cmk_region: Option<String>,
    /// Wrapped TCS version pinned at prepare time.
    pub tcs_version: i64,
    /// BLAKE3 proof of the opaque wrapped TCS, when convergent mode uses one.
    pub wrapped_tcs_blake3: Option<String>,
    /// Non-secret digest over every pinned policy field, retained after shred.
    pub policy_blake3: String,
}

/// Exact source encryption identity needed before target encryption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceCryptoIdentity {
    /// Generation zero raw plaintext; there is no allocation envelope.
    Plaintext,
    /// Previously published encrypted generation.
    Encrypted {
        /// Published source generation.
        generation: i64,
        /// Exact published allocation; Mode B unwrap is allocation-qualified.
        allocation_id: String,
        /// Source crypto mode used to select convergent or envelope decryption.
        crypto_mode: String,
        /// Source configuration version used for audit and pin validation.
        config_version: i64,
        /// Source CMK identity used by the ciphertext envelope/context.
        cmk_key_id: String,
        /// Source KMS provider.
        cmk_provider: String,
        /// Source KMS residency region.
        cmk_region: String,
        /// Source wrapped-TCS version (also pinned for Mode B audit identity).
        tcs_version: i64,
    },
}

/// Immutable physical source discovered under the activation snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationSourceObject {
    /// CAS or AC namespace.
    pub surface: BackfillSurface,
    /// Full logical identity; CAS includes its digest algorithm namespace.
    pub logical_key: String,
    /// Complete tenant- and region-scoped source key.
    pub physical_key: String,
    /// Plaintext size published to clients.
    pub plaintext_size: u64,
    /// BLAKE3 of the exact bytes read from the immutable source object.
    /// Missing or mismatched proofs must stop activation before decryption.
    pub source_blake3: String,
    /// How the source must be decoded before target encryption.
    pub crypto: SourceCryptoIdentity,
}

/// Exact immutable physical object retained until absence is proven.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalPurgeItem {
    /// Stable idempotency key for the purge operation.
    pub purge_id: String,
    /// Tenant owning the physical namespace.
    pub tenant_id: String,
    /// CAS or AC namespace.
    pub surface: BackfillSurface,
    /// Logical object identity retained for audit only.
    pub logical_key: String,
    /// Physical generation; zero identifies a legacy raw object.
    pub generation: i64,
    /// Exact allocation/envelope identity for encrypted generations.
    pub allocation_id: Option<String>,
    /// Complete immutable R2 key to delete and HEAD-verify.
    pub physical_key: String,
}

/// Exact immutable key identity used to decrypt a published source generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceActivationIdentity {
    /// Immutable configuration-history version of the published source.
    pub config_version: i64,
    /// KMS provider that protects the source TCS/envelope.
    pub cmk_provider: String,
    /// Exact source CMK resource identifier.
    pub cmk_key_id: String,
    /// KMS region bound to the source ciphertext identity.
    pub cmk_region: String,
    /// Immutable wrapped-TCS history version used by the source generation.
    pub tcs_version: i64,
}

/// One durable activation intent returned to an internal worker.
#[derive(Clone, PartialEq, Eq)]
pub struct ActivationIntent {
    /// Stable public-safe activation identity.
    pub intent_id: String,
    /// Tenant being activated.
    pub tenant_id: String,
    /// Stable transition guard identity.
    pub guard_id: String,
    /// Idempotency digest of the complete activation request body.
    pub request_blake3: String,
    /// Exact internal worker capability, with redacted formatting.
    pub(crate) claim: Option<WorkerClaim>,
    /// Gate epoch pinned before copy.
    pub observed_gate_epoch: i64,
    /// Gate epoch created by atomic target publication.
    pub publication_gate_epoch: Option<i64>,
    /// Config version created by the same atomic `partial` publication.
    pub published_config_version: Option<i64>,
    /// Published source generation.
    pub source_generation: i64,
    /// Exact historical source identity; absent only for plaintext generation zero.
    pub source_identity: Option<SourceActivationIdentity>,
    /// Invisible target generation.
    pub target_generation: i64,
    /// Fully pinned activation policy.
    pub policy: PinnedActivationPolicy,
    /// Current durable phase.
    pub phase: ActivationPhase,
    /// Durable pause controlled by authorized 0119 degrade/restore actions.
    pub suspension: ActivationSuspension,
    /// Whether CAS keyset enumeration is exhausted.
    pub cas_complete: bool,
    /// Whether AC keyset enumeration is exhausted.
    pub ac_complete: bool,
    /// Absolute scheduler deadline in Unix milliseconds.
    pub deadline_at_ms: i64,
    /// D1-clock verdict returned by the adapter; caller clocks are never used.
    pub deadline_expired: bool,
    /// Monotonic D1 compare-and-swap version for lifecycle mutations.
    pub state_version: i64,
}

impl core::fmt::Debug for ActivationIntent {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ActivationIntent")
            .field("intent_id", &self.intent_id)
            .field("tenant_id", &self.tenant_id)
            .field("guard_id", &self.guard_id)
            .field("request_blake3", &self.request_blake3)
            .field("claim", &self.claim)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

/// Internal multi-replica worker lease. Capability bytes never cross a public
/// response/model boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkerClaim {
    owner: String,
    token: String,
    epoch: i64,
    expires_at_ms: i64,
}

impl WorkerClaim {
    /// Construct a claim read from the trusted 0121 adapter.
    pub(crate) fn new(owner: String, token: String, epoch: i64, expires_at_ms: i64) -> Self {
        Self {
            owner,
            token,
            epoch,
            expires_at_ms,
        }
    }

    /// Worker identity owning this claim.
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }

    /// Opaque capability used only in guarded internal D1 statements.
    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    /// Monotonic takeover epoch.
    pub(crate) const fn epoch(&self) -> i64 {
        self.epoch
    }

    /// D1-authored expiry for diagnostics; D1 revalidates its own clock.
    pub(crate) const fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }
}

impl core::fmt::Debug for WorkerClaim {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WorkerClaim")
            .field("owner", &self.owner)
            .field("token", &"[REDACTED]")
            .field("epoch", &self.epoch)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

/// Durable result of one store operation and its bounded item count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationStep {
    /// Refreshed authoritative intent.
    pub intent: ActivationIntent,
    /// Objects copied or purge items verified during this call.
    pub processed: usize,
}

/// Result of one finite engine tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationTick {
    /// Durable progress or a phase transition occurred.
    Advanced(ActivationStep),
    /// The claimed intent was already terminal.
    Terminal(ActivationIntent),
    /// No claimable intent currently exists.
    Idle,
}

/// Atomic persistence contract for the 0121 activation pipeline.
#[async_trait]
pub trait ByokActivationStore: Send + Sync {
    /// Claim one nonterminal intent or atomically take over an expired claim.
    async fn claim_next(
        &self,
        worker_id: &str,
        lease: Duration,
    ) -> Result<Option<ActivationIntent>, BackfillError>;

    /// Copy at most `limit` source objects and atomically ledger their exact
    /// source/target identities with the keyset cursor checkpoint.
    async fn copy_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError>;

    /// Atomically publish the complete target generation and set config to
    /// `partial`; all data-plane operations remain fail-closed in that state.
    async fn publish_partial(
        &self,
        intent: &ActivationIntent,
    ) -> Result<ActivationIntent, BackfillError>;

    /// Atomically create exact source purge rows and enter `purging`.
    async fn begin_purge(
        &self,
        intent: &ActivationIntent,
    ) -> Result<ActivationIntent, BackfillError>;

    /// Delete and HEAD-verify at most `limit` exact purge rows. A delete error
    /// retains the durable physical identity for retry.
    async fn purge_page(
        &self,
        intent: &ActivationIntent,
        limit: usize,
    ) -> Result<ActivationStep, BackfillError>;

    /// Prove no unverified source purge remains and enter `ready_finalize`.
    async fn ready_finalize(
        &self,
        intent: &ActivationIntent,
    ) -> Result<ActivationIntent, BackfillError>;

    /// Atomically set config active and commit guard/intent after exact purge.
    async fn finalize(&self, intent: &ActivationIntent) -> Result<ActivationIntent, BackfillError>;

    /// Abort an unpublished activation back to inactive, or preempt it under
    /// an authorized higher-priority deactivate/shred transition.
    async fn abort_or_preempt(
        &self,
        intent: &ActivationIntent,
        reason: &str,
        control_token: Option<&str>,
    ) -> Result<ActivationIntent, BackfillError>;

    /// Atomically mirror an authorized 0119 degrade/restore transition without
    /// changing the activation phase.
    async fn set_suspension(
        &self,
        intent: &ActivationIntent,
        suspension: ActivationSuspension,
        control_token: &str,
    ) -> Result<ActivationIntent, BackfillError>;
}

/// Finite scheduler-independent activation coordinator.
pub struct ByokActivationEngine<S> {
    store: S,
}

impl<S> core::fmt::Debug for ByokActivationEngine<S> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ByokActivationEngine")
            .finish_non_exhaustive()
    }
}

impl<S: ByokActivationStore> ByokActivationEngine<S> {
    /// Construct the typed engine around one durable store.
    #[must_use]
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Advance at most one bounded page or one atomic phase transition.
    pub async fn tick(
        &self,
        worker_id: &str,
        lease: Duration,
        max_items: usize,
    ) -> Result<ActivationTick, BackfillError> {
        if worker_id.trim().is_empty() || max_items == 0 || lease.is_zero() {
            return Err(BackfillError::InvalidRequest(
                "worker, positive lease and positive item budget are required".to_owned(),
            ));
        }
        let Some(intent) = self.store.claim_next(worker_id, lease).await? else {
            return Ok(ActivationTick::Idle);
        };
        validate_intent(&intent)?;
        if intent.phase.is_terminal() {
            return Ok(ActivationTick::Terminal(intent));
        }
        if intent.suspension == ActivationSuspension::DegradedReadOnly {
            return Ok(ActivationTick::Advanced(ActivationStep {
                intent,
                processed: 0,
            }));
        }
        let step = match intent.phase {
            ActivationPhase::Copy => {
                if should_abort_for_deadline(&intent) {
                    phase_step(
                        self.store
                            .abort_or_preempt(&intent, "activation deadline exceeded", None)
                            .await?,
                    )
                } else if intent.cas_complete && intent.ac_complete {
                    phase_step(self.store.publish_partial(&intent).await?)
                } else {
                    self.store.copy_page(&intent, max_items).await?
                }
            }
            ActivationPhase::PublishedPartial => phase_step(self.store.begin_purge(&intent).await?),
            ActivationPhase::Purging => {
                let step = self.store.purge_page(&intent, max_items).await?;
                if step.processed == 0 {
                    phase_step(self.store.ready_finalize(&step.intent).await?)
                } else {
                    step
                }
            }
            ActivationPhase::ReadyFinalize => phase_step(self.store.finalize(&intent).await?),
            ActivationPhase::Committed | ActivationPhase::Aborted | ActivationPhase::Preempted => {
                unreachable!("terminal handled above")
            }
        };
        validate_same_intent(&intent, &step.intent)?;
        Ok(if step.intent.phase.is_terminal() {
            ActivationTick::Terminal(step.intent)
        } else {
            ActivationTick::Advanced(step)
        })
    }
}

fn should_abort_for_deadline(intent: &ActivationIntent) -> bool {
    intent.phase == ActivationPhase::Copy && intent.deadline_expired
}

fn phase_step(intent: ActivationIntent) -> ActivationStep {
    ActivationStep {
        intent,
        processed: 0,
    }
}

fn validate_intent(intent: &ActivationIntent) -> Result<(), BackfillError> {
    if intent.intent_id.is_empty()
        || intent.tenant_id.is_empty()
        || intent.guard_id.is_empty()
        || intent.request_blake3.is_empty()
        || intent.observed_gate_epoch <= 0
        || intent.target_generation != intent.source_generation.saturating_add(1)
        || intent.target_generation <= 0
        || (intent.source_generation == 0) != intent.source_identity.is_none()
        || intent.source_identity.as_ref().is_some_and(|source| {
            source.config_version <= 0
                || source.tcs_version <= 0
                || source.cmk_provider.is_empty()
                || source.cmk_key_id.is_empty()
                || source.cmk_region.is_empty()
        })
        || intent.state_version <= 0
        || intent.policy.policy_blake3.is_empty()
        || (intent.phase != ActivationPhase::Preempted
            && (intent.policy.cmk_provider.is_none()
                || intent.policy.cmk_key_id.is_none()
                || intent.policy.cmk_region.is_none()))
        || (intent.phase.is_terminal() != intent.claim.is_none())
        || intent.claim.as_ref().is_some_and(|claim| {
            claim.owner().is_empty()
                || claim.token().is_empty()
                || claim.epoch() <= 0
                || claim.expires_at_ms() <= 0
        })
    {
        return Err(BackfillError::Store(
            "activation store returned invalid durable identity".to_owned(),
        ));
    }
    Ok(())
}

fn validate_same_intent(
    previous: &ActivationIntent,
    next: &ActivationIntent,
) -> Result<(), BackfillError> {
    validate_intent(next)?;
    if previous.intent_id != next.intent_id
        || previous.tenant_id != next.tenant_id
        || previous.guard_id != next.guard_id
        || previous.request_blake3 != next.request_blake3
        || previous.source_generation != next.source_generation
        || previous.source_identity != next.source_identity
        || previous.target_generation != next.target_generation
        || previous.observed_gate_epoch != next.observed_gate_epoch
        || !valid_publication_snapshot(previous, next)
        || !same_policy_or_preempted_scrub(&previous.policy, &next.policy, next.phase)
        || !valid_claim_transition(previous.claim.as_ref(), next.claim.as_ref(), next.phase)
        || next.state_version <= previous.state_version
        || !valid_phase_transition(previous.phase, next.phase)
    {
        return Err(BackfillError::Store(
            "activation store changed immutable snapshot identity".to_owned(),
        ));
    }
    Ok(())
}

fn valid_claim_transition(
    previous: Option<&WorkerClaim>,
    next: Option<&WorkerClaim>,
    phase: ActivationPhase,
) -> bool {
    match (previous, next) {
        (Some(old), Some(new)) => new.epoch() >= old.epoch(),
        (Some(_), None) => phase.is_terminal(),
        (None, None) => phase.is_terminal(),
        (None, Some(_)) => false,
    }
}

fn valid_publication_snapshot(previous: &ActivationIntent, next: &ActivationIntent) -> bool {
    if previous.publication_gate_epoch == next.publication_gate_epoch
        && previous.published_config_version == next.published_config_version
    {
        return true;
    }
    previous.publication_gate_epoch.is_none()
        && previous.published_config_version.is_none()
        && next.phase == ActivationPhase::PublishedPartial
        && next.publication_gate_epoch == Some(previous.observed_gate_epoch.saturating_add(1))
        && next.published_config_version == Some(previous.policy.config_version.saturating_add(1))
}

fn same_policy_or_preempted_scrub(
    previous: &PinnedActivationPolicy,
    next: &PinnedActivationPolicy,
    phase: ActivationPhase,
) -> bool {
    previous == next
        || (phase == ActivationPhase::Preempted
            && previous.config_version == next.config_version
            && previous.mode == next.mode
            && previous.crypto_mode == next.crypto_mode
            && previous.tcs_version == next.tcs_version
            && previous.wrapped_tcs_blake3 == next.wrapped_tcs_blake3
            && previous.policy_blake3 == next.policy_blake3
            && next.cmk_provider.is_none()
            && next.cmk_key_id.is_none()
            && next.cmk_region.is_none())
}

fn valid_phase_transition(previous: ActivationPhase, next: ActivationPhase) -> bool {
    match previous {
        ActivationPhase::Copy => matches!(
            next,
            ActivationPhase::Copy
                | ActivationPhase::PublishedPartial
                | ActivationPhase::Aborted
                | ActivationPhase::Preempted
        ),
        ActivationPhase::PublishedPartial => matches!(
            next,
            ActivationPhase::PublishedPartial
                | ActivationPhase::Purging
                | ActivationPhase::Preempted
        ),
        ActivationPhase::Purging => matches!(
            next,
            ActivationPhase::Purging | ActivationPhase::ReadyFinalize | ActivationPhase::Preempted
        ),
        ActivationPhase::ReadyFinalize => matches!(
            next,
            ActivationPhase::ReadyFinalize
                | ActivationPhase::Committed
                | ActivationPhase::Preempted
        ),
        ActivationPhase::Committed | ActivationPhase::Aborted | ActivationPhase::Preempted => {
            previous == next
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(phase: ActivationPhase, deadline_expired: bool) -> ActivationIntent {
        let terminal = phase.is_terminal();
        let published = matches!(
            phase,
            ActivationPhase::PublishedPartial
                | ActivationPhase::Purging
                | ActivationPhase::ReadyFinalize
                | ActivationPhase::Committed
        );
        ActivationIntent {
            intent_id: "intent-a".to_owned(),
            tenant_id: "tenant-a".to_owned(),
            guard_id: "guard-a".to_owned(),
            request_blake3: "a".repeat(64),
            claim: (!terminal)
                .then(|| WorkerClaim::new("worker-a".to_owned(), "claim-a".to_owned(), 1, 10)),
            observed_gate_epoch: 1,
            publication_gate_epoch: published.then_some(2),
            published_config_version: published.then_some(2),
            source_generation: 0,
            source_identity: None,
            target_generation: 1,
            policy: PinnedActivationPolicy {
                config_version: 1,
                mode: "byok".to_owned(),
                crypto_mode: "convergent".to_owned(),
                cmk_provider: Some("aws".to_owned()),
                cmk_key_id: Some("key-a".to_owned()),
                cmk_region: Some("us-east-1".to_owned()),
                tcs_version: 1,
                wrapped_tcs_blake3: Some("b".repeat(64)),
                policy_blake3: "c".repeat(64),
            },
            phase,
            suspension: ActivationSuspension::Active,
            cas_complete: published,
            ac_complete: published,
            deadline_at_ms: 10,
            deadline_expired,
            state_version: 1,
        }
    }

    #[test]
    fn phases_are_forward_only_and_post_publish_cannot_abort() {
        assert!(valid_phase_transition(
            ActivationPhase::Copy,
            ActivationPhase::Aborted
        ));
        assert!(!valid_phase_transition(
            ActivationPhase::PublishedPartial,
            ActivationPhase::Aborted
        ));
        assert!(!valid_phase_transition(
            ActivationPhase::Purging,
            ActivationPhase::Copy
        ));
    }

    #[test]
    fn terminal_intent_requires_claim_absence() {
        let terminal = intent(ActivationPhase::Aborted, true);
        assert!(validate_intent(&terminal).is_ok());
        let mut invalid = terminal;
        invalid.claim = Some(WorkerClaim::new(
            "worker-a".to_owned(),
            "claim-a".to_owned(),
            1,
            10,
        ));
        assert!(validate_intent(&invalid).is_err());
    }

    #[test]
    fn d1_deadline_aborts_only_unpublished_copy() {
        assert!(should_abort_for_deadline(&intent(
            ActivationPhase::Copy,
            true
        )));
        assert!(!should_abort_for_deadline(&intent(
            ActivationPhase::PublishedPartial,
            true
        )));
        assert!(!should_abort_for_deadline(&intent(
            ActivationPhase::Purging,
            true
        )));
    }
}
