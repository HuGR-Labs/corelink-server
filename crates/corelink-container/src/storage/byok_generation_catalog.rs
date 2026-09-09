//! Durable logical-to-physical publication catalog for BYOK objects.
//!
//! R2 cannot enforce a D1 lease token.  Writers therefore stage bytes under a
//! generation-qualified, immutable physical key and publish that key only in a
//! D1 transaction which proves that the originating data intent is still live
//! and belongs to the current tenant gate epoch.  A stale PUT can leave an
//! orphan, but it can never become addressable by a reader.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde_json::json;

use super::d1_http::{D1BatchStatement, D1HttpClient, D1Row};
use crate::byok_transition_fence::{D1ByokFence, DataIntent, DataOperation};
use crate::customer_d1::{ByokCryptoMode, ByokMode, ByokState, TenantByokConfig};

const DATA_INTENT_LEASE: Duration = Duration::from_secs(120);
const PURGE_CLAIM_LEASE_MS: i64 = 120_000;
const STALE_ALLOCATION_GRACE_MS: i64 = 120_000;
const PURGE_DISPATCH_TAIL_MS: i64 = 5_000;
/// A malformed physical identity is permanent until an operator repairs the
/// durable row.  Three bounded observations are enough to survive a single
/// stale replica/claim handoff while preventing queue starvation.
pub(crate) const COMMON_PURGE_INVALID_IDENTITY_QUARANTINE_AFTER: i64 = 3;
const PURGE_RECONCILE_PERIOD: Duration = Duration::from_secs(60);
const STALE_CENSUS_TIMEOUT: Duration = Duration::from_millis(500);
// R2's operation timeout is 60s; require another 5s of scheduler/network
// safety before dispatch so the durable intent cannot expire mid-PUT.
const R2_PUT_DISPATCH_HEADROOM_MS: i64 = 65_000;
// The census accepts either a terminal data-plane write intent or an aborted
// backfill run whose transition fence is terminal/expired. It never uses age
// alone to classify a still-authorized PUT as a loser.
const STALE_ALLOCATION_OWNER: &str = "(EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.token=g.intent_token AND i.tenant_id=g.tenant_id AND i.operation='write' AND (i.outcome IN ('completed','expired') OR (i.outcome='active' AND i.expires_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000)))) OR EXISTS (SELECT 1 FROM byok_backfill_run r JOIN byok_transition_fence f ON f.tenant_id=r.tenant_id AND f.token=r.intent_token AND f.epoch=r.transition_epoch WHERE r.tenant_id=g.tenant_id AND r.run_id=g.backfill_run_id AND r.intent_token=g.intent_token AND r.target_generation=g.generation AND r.phase='aborted' AND (f.outcome='aborted' OR f.outcome='expired' OR f.expires_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000))))";

// Resolve the allocation owner's pinned config version against the current
// row plus immutable history. GROUP/HAVING is a deliberate fail-closed
// ambiguity check: zero or multiple owner/config matches cannot be selected.
const STALE_ALLOCATION_SELECTION: &str = "SELECT x.rowid,x.crypto_mode FROM (SELECT g.rowid,g.allocated_at_ms,c.crypto_mode FROM byok_logical_object_generation g JOIN byok_data_intent i ON i.tenant_id=g.tenant_id AND i.token=g.intent_token AND i.operation='write' JOIN (SELECT tenant_id,config_version,crypto_mode FROM tenant_byok_config UNION ALL SELECT tenant_id,config_version,crypto_mode FROM tenant_byok_config_history) c ON c.tenant_id=g.tenant_id AND c.config_version=i.observed_config_version WHERE g.outcome IN ('allocated','abandoned') AND g.allocated_at_ms<=?1 AND (i.outcome IN ('completed','expired') OR (i.outcome='active' AND i.expires_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000))) AND NOT EXISTS (SELECT 1 FROM byok_logical_object_publication q WHERE q.tenant_id=g.tenant_id AND q.object_kind=g.object_kind AND q.logical_key=g.logical_key AND q.generation=g.generation AND q.allocation_id=g.allocation_id AND q.physical_key=g.physical_key) AND (g.outcome='allocated' OR NOT EXISTS (SELECT 1 FROM byok_object_purge_item p JOIN byok_object_purge_cause pc ON pc.purge_id=p.purge_id AND pc.cause_kind='publish_loser' AND pc.cause_id=g.allocation_id WHERE p.tenant_id=g.tenant_id AND p.object_kind=g.object_kind AND p.logical_key=g.logical_key AND p.generation=g.generation AND p.allocation_id=g.allocation_id AND p.physical_key=g.physical_key)) AND NOT EXISTS (SELECT 1 FROM byok_object_purge_item p WHERE p.tenant_id=g.tenant_id AND p.physical_key=g.physical_key AND (p.object_kind<>g.object_kind OR p.logical_key<>g.logical_key OR p.generation<>g.generation OR p.allocation_id IS NOT g.allocation_id)) UNION ALL SELECT g.rowid,g.allocated_at_ms,c.crypto_mode FROM byok_logical_object_generation g JOIN byok_backfill_run r ON r.tenant_id=g.tenant_id AND r.run_id=g.backfill_run_id AND r.intent_token=g.intent_token AND r.target_generation=g.generation JOIN byok_transition_fence f ON f.tenant_id=r.tenant_id AND f.token=r.intent_token AND f.epoch=r.transition_epoch JOIN (SELECT tenant_id,config_version,crypto_mode FROM tenant_byok_config UNION ALL SELECT tenant_id,config_version,crypto_mode FROM tenant_byok_config_history) c ON c.tenant_id=g.tenant_id AND c.config_version=f.observed_config_version WHERE g.outcome IN ('allocated','abandoned') AND g.allocated_at_ms<=?1 AND r.phase='aborted' AND (f.outcome IN ('aborted','expired') OR f.expires_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000)) AND NOT EXISTS (SELECT 1 FROM byok_logical_object_publication q WHERE q.tenant_id=g.tenant_id AND q.object_kind=g.object_kind AND q.logical_key=g.logical_key AND q.generation=g.generation AND q.allocation_id=g.allocation_id AND q.physical_key=g.physical_key) AND (g.outcome='allocated' OR NOT EXISTS (SELECT 1 FROM byok_object_purge_item p JOIN byok_object_purge_cause pc ON pc.purge_id=p.purge_id AND pc.cause_kind='publish_loser' AND pc.cause_id=g.allocation_id WHERE p.tenant_id=g.tenant_id AND p.object_kind=g.object_kind AND p.logical_key=g.logical_key AND p.generation=g.generation AND p.allocation_id=g.allocation_id AND p.physical_key=g.physical_key)) AND NOT EXISTS (SELECT 1 FROM byok_object_purge_item p WHERE p.tenant_id=g.tenant_id AND p.physical_key=g.physical_key AND (p.object_kind<>g.object_kind OR p.logical_key<>g.logical_key OR p.generation<>g.generation OR p.allocation_id IS NOT g.allocation_id))) x GROUP BY x.rowid HAVING COUNT(*)=1 ORDER BY MIN(x.allocated_at_ms),x.rowid LIMIT ?2";

const PUBLISH_CATALOG_SQL: &str = "INSERT INTO byok_logical_object_publication (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,published_at_ms) SELECT ?1,?2,?3,?4,?5,?6,?7,?8,(CAST(strftime('%s','now') AS INTEGER)*1000) WHERE EXISTS (SELECT 1 FROM byok_logical_object_generation x WHERE x.tenant_id=?1 AND x.object_kind=?2 AND x.logical_key=?3 AND x.generation=?4 AND x.allocation_id=?5 AND x.physical_key=?6 AND x.intent_token=?9 AND x.gate_epoch=?7 AND x.size_bytes=?8 AND x.outcome='allocated') AND EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.tenant_id=?1 AND i.token=?9 AND i.observed_gate_epoch=?7 AND i.observed_generation=?4 AND i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) AND i.outcome='active') AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?1 AND g.gate_epoch=?7 AND g.current_generation=?4) ON CONFLICT(tenant_id,object_kind,logical_key) DO UPDATE SET generation=excluded.generation,allocation_id=excluded.allocation_id,physical_key=excluded.physical_key,gate_epoch=excluded.gate_epoch,size_bytes=excluded.size_bytes,published_at_ms=excluded.published_at_ms WHERE byok_logical_object_publication.generation < excluded.generation RETURNING allocation_id";

/// Catalog keyspace.  CAS includes both digest algorithms in one logical kind;
/// the algorithm-qualified R2 key remains part of `physical_key`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByokObjectKind {
    /// Content-addressed blob.
    Cas,
    /// Mutable action-cache entry.
    Ac,
}

impl ByokObjectKind {
    #[must_use]
    /// Stable value persisted in D1.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cas => "cas",
            Self::Ac => "ac",
        }
    }
}

/// Authoritative published location for one logical CAS/AC identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedObject {
    /// Caller-visible object identity.
    pub logical_key: String,
    /// Encryption generation containing this publication.
    pub generation: i64,
    /// Complete R2 object key, including region, tenant prefix and generation.
    pub physical_key: String,
    /// Exact staged allocation selected by the publication.
    pub allocation_id: String,
    /// Tenant gate epoch which authorized publication.
    pub gate_epoch: i64,
    /// Plaintext size exposed by logical LIST.
    pub size_bytes: u64,
    /// D1 publication time in Unix milliseconds.
    pub published_at_ms: i64,
}

/// Why an exact physical object must be removed from R2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByokPurgeReason {
    /// An activation source retained for purge after its owner became terminal.
    ActivationSource,
    /// A customer delete removed the only logical publication.
    LiveDelete,
    /// A staged PUT lost the publication race or its capability expired.
    PublishLoser,
}

impl ByokPurgeReason {
    /// Stable value persisted in the shared purge ledger.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ActivationSource => "activation_source",
            Self::LiveDelete => "live_delete",
            Self::PublishLoser => "publish_loser",
        }
    }
}

/// Exact durable deletion plan. The ledger row is created before the
/// publication pointer is removed or a stale staged PUT is reported lost.
#[derive(Clone, PartialEq, Eq)]
pub struct ByokPurgePlan {
    /// D1 idempotency key for the purge row.
    pub purge_id: String,
    /// Tenant which owns the exact physical object.
    pub tenant_id: String,
    /// R2 bucket namespace containing the exact physical object.
    pub kind: ByokObjectKind,
    /// Exact published/staged object identity.
    pub object: PublishedObject,
    /// Mode used by the object's envelope (`plaintext` for generation zero).
    pub crypto_mode: String,
    /// The race/delete which created this obligation.
    pub reason: ByokPurgeReason,
    /// Claim token proving this replica owns the current attempt.
    claim_token: Option<String>,
    /// Monotonic claim epoch.
    claim_epoch: i64,
    /// Durable attempt number assigned by the claim transaction.
    pub(crate) attempts: i64,
    /// D1-authored claim expiry.
    claim_expires_at_ms: Option<i64>,
    /// A prior attempt already proved R2 absence; skip DELETE/HEAD and resume
    /// at envelope reclaim.
    pub r2_absent: bool,
}

impl core::fmt::Debug for ByokPurgePlan {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ByokPurgePlan")
            .field("purge_id", &self.purge_id)
            .field("tenant_id", &self.tenant_id)
            .field("kind", &self.kind)
            .field("object", &self.object)
            .field("crypto_mode", &self.crypto_mode)
            .field("reason", &self.reason)
            .field(
                "claim_token",
                &self.claim_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("claim_epoch", &self.claim_epoch)
            .field("attempts", &self.attempts)
            .field("claim_expires_at_ms", &self.claim_expires_at_ms)
            .field("r2_absent", &self.r2_absent)
            .finish()
    }
}

/// Candidate publication created before the R2 PUT is dispatched.
#[derive(Clone, PartialEq, Eq)]
pub struct StagedObject {
    /// Tenant owner.
    pub tenant_id: String,
    /// CAS or AC namespace.
    pub kind: ByokObjectKind,
    /// Caller-visible identity.
    pub logical_key: String,
    /// Encryption generation.
    pub generation: i64,
    /// Public, random write identifier. It is not the data-intent capability.
    pub allocation_id: String,
    /// Complete immutable R2 key.
    pub physical_key: String,
    intent_token: String,
    /// Tenant gate epoch observed by the intent.
    pub gate_epoch: i64,
    /// Plaintext object size.
    pub size_bytes: u64,
    /// Crypto mode bound into the durable purge row.
    pub crypto_mode: String,
}

impl core::fmt::Debug for StagedObject {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("StagedObject")
            .field("tenant_id", &self.tenant_id)
            .field("kind", &self.kind)
            .field("logical_key", &self.logical_key)
            .field("generation", &self.generation)
            .field("allocation_id", &self.allocation_id)
            .field("physical_key", &self.physical_key)
            .field("intent_token", &"[REDACTED]")
            .field("gate_epoch", &self.gate_epoch)
            .field("size_bytes", &self.size_bytes)
            .field("crypto_mode", &self.crypto_mode)
            .finish()
    }
}

/// Persistence seam for generation allocation and logical publication.
#[async_trait]
pub trait ByokGenerationCatalog: Send + Sync + core::fmt::Debug {
    /// Resolve an exact logical identity at one encryption generation.
    async fn resolve(
        &self,
        tenant_id: &str,
        kind: ByokObjectKind,
        logical_key: &str,
        generation: i64,
    ) -> Result<Option<PublishedObject>, String>;

    /// Allocate an immutable physical candidate under a live intent.
    async fn allocate(&self, staged: &StagedObject) -> Result<(), String>;

    /// Publish iff the exact data intent is live and its epoch/generation is
    /// still current.  `Ok(false)` means the PUT lost the transition race and
    /// is deliberately left unreachable for the orphan reaper.
    async fn publish(&self, staged: &StagedObject) -> Result<bool, String>;

    /// Remove addressability before deleting bytes.  The exact live intent and
    /// epoch are checked in the same transaction as the pointer deletion.
    #[allow(
        clippy::too_many_arguments,
        reason = "the durable purge identity must remain explicit and allocation-qualified"
    )]
    async fn tombstone(
        &self,
        tenant_id: &str,
        kind: ByokObjectKind,
        logical_key: &str,
        generation: i64,
        intent_token: &str,
        gate_epoch: i64,
        crypto_mode: &str,
    ) -> Result<Option<ByokPurgePlan>, String>;

    /// Mark a durable plan as being attempted. The ledger remains durable if
    /// this call or the subsequent R2 request fails.
    async fn begin_purge_attempt(&self, plan: &mut ByokPurgePlan) -> Result<(), String>;

    /// Record the post-delete HEAD verdict. Only an absent HEAD advances a
    /// row to `verified`; every ambiguous/error path remains retryable.
    async fn finish_purge_attempt(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: Option<&str>,
        envelope_reclaimed: bool,
    ) -> Result<(), String>;

    /// Enumerate published logical identities, never raw R2 keys.
    async fn list(
        &self,
        tenant_id: &str,
        kind: ByokObjectKind,
        generation: i64,
        limit: u32,
        after_logical_key: Option<&str>,
    ) -> Result<Vec<PublishedObject>, String>;
}

/// Typed seam for the periodic non-activation purge worker. The runtime census
/// hook deliberately does not claim work without an R2 worker. The worker owns
/// R2 DELETE/HEAD and Mode-B envelope I/O; it must process every claimed plan
/// and call the exact-claim completion API, while this seam owns D1 leasing.
#[async_trait]
pub trait ByokPurgeReconciler: Send + Sync + core::fmt::Debug {
    /// Discover one bounded page of terminal allocations before claiming.
    async fn reconcile_stale_page(&self, limit: u32) -> Result<(), String>;

    /// Claim a bounded page of live-delete/publish-loser plans for processing.
    async fn claim_purge_page(&self, limit: u32) -> Result<Vec<ByokPurgePlan>, String>;

    /// Finish only the exact still-live claim represented by `plan`.
    async fn finish_claimed_purge_attempt(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: Option<&str>,
        envelope_reclaimed: bool,
        invalid_identity_reason: Option<&str>,
    ) -> Result<(), String>;

    /// Classify only allocations whose exact owner is terminal/expired.
    async fn reconcile_stale_allocations(
        &self,
        older_than_ms: i64,
        limit: u32,
    ) -> Result<(), String>;
}

/// Data-plane boundary consumed by R2 handlers. Implementations own both the
/// tenant-wide intent and catalog, preventing either concern from being wired
/// without the other in production.
#[async_trait]
pub trait ByokRuntimeGate: Send + Sync + core::fmt::Debug {
    /// Acquire tenant-wide data authority.
    async fn acquire_data(
        &self,
        tenant_id: &str,
        operation: DataOperation,
    ) -> Result<RuntimeDataIntent, String>;
    /// Release exact data authority.
    async fn release_data(&self, intent: &RuntimeDataIntent) -> Result<(), String>;
    /// Revalidate a read capability immediately before returning information.
    async fn validate_for_return(&self, intent: &RuntimeDataIntent) -> Result<(), String>;
    /// Revalidate immediately before PUT dispatch. Production additionally
    /// requires enough lease headroom for the R2 operation deadline.
    async fn validate_for_storage_dispatch(
        &self,
        intent: &RuntimeDataIntent,
    ) -> Result<(), String> {
        self.validate_for_return(intent).await
    }
    /// Resolve a logical object under the intent's authoritative generation.
    async fn resolve_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        logical_key: &str,
    ) -> Result<Option<PublishedObject>, String>;

    /// Begin the durable R2 deletion attempt for one exact plan.
    async fn begin_purge_attempt(&self, plan: &mut ByokPurgePlan) -> Result<(), String>;

    /// Persist the post-delete HEAD verdict for one exact plan.
    async fn finish_purge_attempt(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: Option<&str>,
        envelope_reclaimed: bool,
    ) -> Result<(), String>;
    /// Stage one unique immutable physical object.
    async fn allocate_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        logical_key: &str,
        allocation_id: &str,
        physical_key: &str,
        size_bytes: u64,
    ) -> Result<StagedObject, String>;
    /// Publish a staged object iff its originating capability remains live.
    async fn publish_catalog(
        &self,
        intent: &RuntimeDataIntent,
        staged: &StagedObject,
    ) -> Result<bool, String>;
    /// Remove logical addressability before deleting R2 bytes.
    async fn tombstone_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        logical_key: &str,
    ) -> Result<Option<ByokPurgePlan>, String>;
    /// List logical publications in stable key order.
    async fn list_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        limit: u32,
        after_logical_key: Option<&str>,
    ) -> Result<Vec<PublishedObject>, String>;
}

/// One live intent plus the exact config/key identity read while transitions
/// are excluded. This bypasses the process config cache.
#[derive(Debug)]
pub struct RuntimeDataIntent {
    intent: DataIntent,
    /// Exact tenant config; absent/inactive tenants may have no row.
    pub config: Option<TenantByokConfig>,
    /// Exact wrapped-TCS version for active/partial crypto.
    pub tcs_version: Option<i64>,
}

impl RuntimeDataIntent {
    /// Underlying opaque fence capability.
    #[must_use]
    pub fn capability(&self) -> &DataIntent {
        &self.intent
    }

    /// Current addressable encryption generation. Inactive/absent tenants use
    /// only the fenced legacy raw-key path and never write catalog generation 0.
    #[must_use]
    pub fn catalog_generation(&self) -> Option<i64> {
        let active = self
            .config
            .as_ref()
            .is_some_and(|config| matches!(config.state, ByokState::Active | ByokState::Partial));
        active
            .then_some(self.intent.snapshot.current_generation)
            .filter(|generation| *generation > 0)
    }

    /// Crypto mode to persist alongside an exact object identity.
    #[must_use]
    pub fn catalog_crypto_mode(&self) -> Option<&'static str> {
        self.config.as_ref().and_then(|config| {
            matches!(config.state, ByokState::Active | ByokState::Partial)
                .then_some(config.crypto_mode.as_str())
        })
    }
}

/// Owns a data intent through the final storage/crypto return boundary and
/// releases it on every early-return path.
#[derive(Debug)]
pub struct ByokDataGuard {
    gate: Arc<dyn ByokRuntimeGate>,
    intent: Option<RuntimeDataIntent>,
}

impl ByokDataGuard {
    /// Acquire a guard which releases its capability on every exit path.
    pub async fn acquire(
        gate: Arc<dyn ByokRuntimeGate>,
        tenant: &str,
        operation: DataOperation,
    ) -> Result<Self, String> {
        let intent = gate.acquire_data(tenant, operation).await?;
        Ok(Self {
            gate,
            intent: Some(intent),
        })
    }

    /// Borrow the authoritative operation snapshot.
    pub fn intent(&self) -> Result<&RuntimeDataIntent, String> {
        self.intent
            .as_ref()
            .ok_or_else(|| "BYOK guard intent already released".to_owned())
    }

    /// Revalidate immediately before a read-like success or miss is returned.
    pub async fn validate_for_return(&self) -> Result<(), String> {
        self.gate.validate_for_return(self.intent()?).await
    }

    /// Revalidate authority and R2-operation lease headroom immediately before
    /// a catalog-backed PUT is dispatched.
    pub async fn validate_for_storage_dispatch(&self) -> Result<(), String> {
        self.gate
            .validate_for_storage_dispatch(self.intent()?)
            .await
    }

    /// Validate if requested and release before the caller can observe a
    /// successful result. Release failure is therefore fail-closed.
    pub async fn finish(&mut self, validate: bool) -> Result<(), String> {
        let intent = self.intent()?;
        if validate {
            self.gate.validate_for_return(intent).await?;
        }
        self.gate.release_data(intent).await?;
        self.intent = None;
        Ok(())
    }

    #[must_use]
    /// Borrow the catalog/gate boundary bound to this capability.
    pub fn gate(&self) -> &Arc<dyn ByokRuntimeGate> {
        &self.gate
    }
}

impl Drop for ByokDataGuard {
    fn drop(&mut self) {
        let Some(intent) = self.intent.take() else {
            return;
        };
        let gate = Arc::clone(&self.gate);
        let release = || tokio::runtime::Handle::current().block_on(gate.release_data(&intent));
        let result = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(release)
        } else {
            Err("BYOK data guard dropped outside a Tokio runtime".to_owned())
        };
        if let Err(error) = result {
            tracing::error!(error = %error, tenant = %intent.capability().tenant_id(), "BYOK data intent release failed; expiry will recover");
        }
    }
}

fn parse_authoritative_config(
    row: Option<&D1Row>,
) -> Result<(Option<TenantByokConfig>, Option<i64>), String> {
    let row =
        row.ok_or_else(|| "BYOK authoritative config read rejected stale intent".to_owned())?;
    let Some(tenant_id) = row.get("tenant_id").and_then(serde_json::Value::as_str) else {
        return Ok((None, None));
    };
    let text = |name: &str| {
        row.get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("BYOK authoritative config missing {name}"))
    };
    let config = TenantByokConfig {
        tenant_id: tenant_id.to_owned(),
        mode: text("mode")?
            .parse::<ByokMode>()
            .map_err(|error| error.to_string())?,
        crypto_mode: text("crypto_mode")?
            .parse::<ByokCryptoMode>()
            .map_err(|error| error.to_string())?,
        cmk_provider: row
            .get("cmk_provider")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        cmk_key_id: row
            .get("cmk_key_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        cmk_region: row
            .get("cmk_region")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        state: text("state")?
            .parse::<ByokState>()
            .map_err(|error| error.to_string())?,
    };
    Ok((
        Some(config),
        row.get("tcs_version").and_then(serde_json::Value::as_i64),
    ))
}

/// Production composition over one D1 client. All authority checks use D1's
/// clock; `validate_for_return` also rejects a snapshot changed while a read
/// was in flight.
#[derive(Debug)]
pub struct D1ByokRuntimeGate {
    d1: Arc<D1HttpClient>,
    fence: D1ByokFence,
    catalog: D1ByokGenerationCatalog,
    last_purge_reconcile: Mutex<Instant>,
}

/// Writer-first rollout probe. Before migrations 0118/0121 exist, legacy
/// operation is allowed only when D1 authoritatively proves there is no
/// active/partial configuration. An ambiguous probe or any active tenant
/// fails closed.
pub async fn runtime_gate_after_rollout_probe(
    d1: Arc<D1HttpClient>,
) -> Result<Option<Arc<dyn ByokRuntimeGate>>, String> {
    let rows = d1.query(
        "SELECT (SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('byok_0118_rollout_guard','byok_tenant_gate','byok_data_intent','byok_transition_fence','byok_backfill_run','byok_logical_object_generation','byok_logical_object_publication','byok_object_purge_item','byok_object_purge_cause'))=9 AS fence_ready, EXISTS(SELECT 1 FROM tenant_byok_config WHERE state IN ('active','partial')) AS encryption_live",
        &[],
    ).await?;
    let row = rows
        .first()
        .ok_or_else(|| "BYOK rollout probe returned no row".to_owned())?;
    let flag = |name: &str| {
        row.get(name)
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| format!("BYOK rollout probe missing {name}"))
    };
    match (flag("fence_ready")?, flag("encryption_live")?) {
        (1, _) => Ok(Some(Arc::new(D1ByokRuntimeGate::new(d1)))),
        (0, 0) => Ok(None),
        (0, 1) => Err(
            "required BYOK catalog/purge migrations absent while active/partial BYOK tenants exist"
                .to_owned(),
        ),
        _ => Err("BYOK rollout probe returned invalid flags".to_owned()),
    }
}

impl D1ByokRuntimeGate {
    /// Compose the fence and catalog over the same D1 database.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self {
            d1: Arc::clone(&d1),
            fence: D1ByokFence::new(Arc::clone(&d1)),
            catalog: D1ByokGenerationCatalog::new(d1),
            last_purge_reconcile: Mutex::new(Instant::now() - PURGE_RECONCILE_PERIOD),
        }
    }

    fn stale_census_due(&self) -> bool {
        let Ok(mut last) = self.last_purge_reconcile.lock() else {
            return false;
        };
        if last.elapsed() < PURGE_RECONCILE_PERIOD {
            return false;
        }
        *last = Instant::now();
        true
    }

    async fn reconcile_stale_allocations_if_due(&self) {
        if !self.stale_census_due() {
            return;
        }
        let cutoff_rows = match self
            .d1
            .query(
                "SELECT (CAST(strftime('%s','now') AS INTEGER)*1000)-?1 AS cutoff",
                &[json!(STALE_ALLOCATION_GRACE_MS)],
            )
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(error = %error, "BYOK stale allocation census clock unavailable");
                return;
            }
        };
        let Some(cutoff) = cutoff_rows
            .first()
            .and_then(|row| row.get("cutoff"))
            .and_then(serde_json::Value::as_i64)
        else {
            tracing::warn!("BYOK stale allocation census clock returned no cutoff");
            return;
        };
        if let Err(error) = self.catalog.reconcile_stale_allocations(cutoff, 128).await {
            tracing::warn!(error = %error, "BYOK stale allocation census deferred to next interval");
        }
    }
}

#[async_trait]
impl ByokRuntimeGate for D1ByokRuntimeGate {
    async fn acquire_data(
        &self,
        tenant_id: &str,
        operation: DataOperation,
    ) -> Result<RuntimeDataIntent, String> {
        if tokio::time::timeout(
            STALE_CENSUS_TIMEOUT,
            self.reconcile_stale_allocations_if_due(),
        )
        .await
        .is_err()
        {
            tracing::warn!("BYOK stale allocation census timed out; data acquisition continues");
        }
        let intent = self
            .fence
            .acquire_data_intent(tenant_id, operation, DATA_INTENT_LEASE)
            .await
            .map_err(|error| error.to_string())?;
        let rows = self.d1.query(
            "SELECT c.tenant_id,c.mode,c.crypto_mode,c.cmk_provider,c.cmk_key_id,c.cmk_region,c.state,s.tcs_version FROM byok_data_intent i JOIN byok_tenant_gate g ON g.tenant_id=i.tenant_id JOIN tenant t ON t.tenant_id=i.tenant_id LEFT JOIN tenant_byok_config c ON c.tenant_id=i.tenant_id LEFT JOIN tenant_byok_secret s ON s.tenant_id=i.tenant_id WHERE i.tenant_id=?1 AND i.token=?2 AND i.outcome='active' AND i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) AND i.observed_gate_epoch=g.gate_epoch AND i.observed_generation=g.current_generation AND ((i.observed_config_version IS NULL AND c.config_version IS NULL) OR i.observed_config_version=c.config_version) AND i.observed_config_state=COALESCE(c.state,'absent') AND i.observed_byok_status=t.byok_status LIMIT 1",
            &[json!(intent.tenant_id()), json!(intent.token())],
        ).await;
        let parsed = rows.and_then(|rows| parse_authoritative_config(rows.first()));
        match parsed {
            Ok((config, tcs_version)) => Ok(RuntimeDataIntent {
                intent,
                config,
                tcs_version,
            }),
            Err(error) => {
                let _ = self.fence.release_data_intent(&intent).await;
                Err(error)
            }
        }
    }

    async fn release_data(&self, intent: &RuntimeDataIntent) -> Result<(), String> {
        self.fence
            .release_data_intent(intent.capability())
            .await
            .map_err(|error| error.to_string())
    }

    async fn validate_for_return(&self, intent: &RuntimeDataIntent) -> Result<(), String> {
        let rows = self.d1.query(
            "SELECT 1 AS valid FROM byok_data_intent i JOIN byok_tenant_gate g ON g.tenant_id=i.tenant_id JOIN tenant t ON t.tenant_id=i.tenant_id LEFT JOIN tenant_byok_config c ON c.tenant_id=i.tenant_id WHERE i.tenant_id=?1 AND i.token=?2 AND i.outcome='active' AND i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) AND i.observed_gate_epoch=g.gate_epoch AND i.observed_generation=g.current_generation AND ((i.observed_config_version IS NULL AND c.config_version IS NULL) OR i.observed_config_version=c.config_version) AND i.observed_config_state=COALESCE(c.state,'absent') AND i.observed_byok_status=t.byok_status LIMIT 1",
            &[json!(intent.capability().tenant_id()), json!(intent.capability().token())],
        ).await?;
        if rows.is_empty() {
            Err("BYOK data intent is no longer authoritative".to_owned())
        } else {
            Ok(())
        }
    }

    async fn validate_for_storage_dispatch(
        &self,
        intent: &RuntimeDataIntent,
    ) -> Result<(), String> {
        let rows = self.d1.query(
            "SELECT 1 AS valid FROM byok_data_intent i JOIN byok_tenant_gate g ON g.tenant_id=i.tenant_id JOIN tenant t ON t.tenant_id=i.tenant_id LEFT JOIN tenant_byok_config c ON c.tenant_id=i.tenant_id WHERE i.tenant_id=?1 AND i.token=?2 AND i.outcome='active' AND i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000)+?3 AND i.observed_gate_epoch=g.gate_epoch AND i.observed_generation=g.current_generation AND ((i.observed_config_version IS NULL AND c.config_version IS NULL) OR i.observed_config_version=c.config_version) AND i.observed_config_state=COALESCE(c.state,'absent') AND i.observed_byok_status=t.byok_status LIMIT 1",
            &[json!(intent.capability().tenant_id()), json!(intent.capability().token()), json!(R2_PUT_DISPATCH_HEADROOM_MS)],
        ).await?;
        if rows.is_empty() {
            Err("BYOK data intent lacks authority or R2 PUT lease headroom".to_owned())
        } else {
            Ok(())
        }
    }

    async fn resolve_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        logical_key: &str,
    ) -> Result<Option<PublishedObject>, String> {
        let generation = intent
            .catalog_generation()
            .ok_or_else(|| "catalog unavailable for inactive generation".to_owned())?;
        self.catalog
            .resolve(
                intent.capability().tenant_id(),
                kind,
                logical_key,
                generation,
            )
            .await
    }

    async fn allocate_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        logical_key: &str,
        allocation_id: &str,
        physical_key: &str,
        size_bytes: u64,
    ) -> Result<StagedObject, String> {
        let generation = intent
            .catalog_generation()
            .ok_or_else(|| "catalog allocation unavailable for inactive generation".to_owned())?;
        let crypto_mode = intent
            .catalog_crypto_mode()
            .ok_or_else(|| "catalog crypto mode unavailable for inactive generation".to_owned())?;
        let intent = intent.capability();
        let staged = StagedObject {
            tenant_id: intent.tenant_id().to_owned(),
            kind,
            logical_key: logical_key.to_owned(),
            generation,
            allocation_id: allocation_id.to_owned(),
            physical_key: physical_key.to_owned(),
            intent_token: intent.token().to_owned(),
            gate_epoch: intent.snapshot.gate_epoch,
            size_bytes,
            crypto_mode: crypto_mode.to_owned(),
        };
        self.catalog.allocate(&staged).await?;
        Ok(staged)
    }

    async fn publish_catalog(
        &self,
        intent: &RuntimeDataIntent,
        staged: &StagedObject,
    ) -> Result<bool, String> {
        let generation = intent
            .catalog_generation()
            .ok_or_else(|| "catalog publication unavailable for inactive generation".to_owned())?;
        let intent = intent.capability();
        if staged.intent_token != intent.token()
            || staged.gate_epoch != intent.snapshot.gate_epoch
            || staged.generation != generation
        {
            let recovery = self.catalog.abandon_publish_loser(staged).await;
            if let Err(recovery_error) = recovery {
                return Err(format!(
                    "BYOK catalog publication capability mismatch; loser recovery: {recovery_error}"
                ));
            }
            return Err("BYOK catalog publication capability mismatch".to_owned());
        }
        self.catalog.publish(staged).await
    }

    async fn tombstone_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        logical_key: &str,
    ) -> Result<Option<ByokPurgePlan>, String> {
        let generation = intent
            .catalog_generation()
            .ok_or_else(|| "catalog tombstone unavailable for inactive generation".to_owned())?;
        let crypto_mode = intent
            .catalog_crypto_mode()
            .ok_or_else(|| "catalog crypto mode unavailable for inactive generation".to_owned())?;
        let intent = intent.capability();
        self.catalog
            .tombstone(
                intent.tenant_id(),
                kind,
                logical_key,
                generation,
                intent.token(),
                intent.snapshot.gate_epoch,
                crypto_mode,
            )
            .await
    }

    async fn begin_purge_attempt(&self, plan: &mut ByokPurgePlan) -> Result<(), String> {
        self.catalog.begin_purge_attempt(plan).await
    }

    async fn finish_purge_attempt(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: Option<&str>,
        envelope_reclaimed: bool,
    ) -> Result<(), String> {
        self.catalog
            .finish_purge_attempt(plan, head_absent, error, envelope_reclaimed)
            .await
    }

    async fn list_catalog(
        &self,
        intent: &RuntimeDataIntent,
        kind: ByokObjectKind,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Vec<PublishedObject>, String> {
        let generation = intent
            .catalog_generation()
            .ok_or_else(|| "catalog list unavailable for inactive generation".to_owned())?;
        let intent = intent.capability();
        self.catalog
            .list(intent.tenant_id(), kind, generation, limit, after)
            .await
    }
}

/// D1 implementation of the logical generation catalog.
#[derive(Clone, Debug)]
pub struct D1ByokGenerationCatalog {
    d1: Arc<D1HttpClient>,
}

impl D1ByokGenerationCatalog {
    /// Construct over the production D1 HTTP client.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    fn parse(row: &D1Row) -> Result<PublishedObject, String> {
        let text = |name: &str| {
            row.get(name)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("BYOK catalog row missing {name}"))
        };
        let integer = |name: &str| {
            row.get(name)
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| format!("BYOK catalog row missing {name}"))
        };
        Ok(PublishedObject {
            logical_key: text("logical_key")?,
            generation: integer("generation")?,
            physical_key: text("physical_key")?,
            allocation_id: text("allocation_id")?,
            gate_epoch: integer("gate_epoch")?,
            size_bytes: u64::try_from(integer("size_bytes")?)
                .map_err(|_| "BYOK catalog size_bytes is negative".to_owned())?,
            published_at_ms: integer("published_at_ms")?,
        })
    }

    async fn abandon_publish_loser(&self, staged: &StagedObject) -> Result<(), String> {
        self.d1
            .batch(vec![
                D1BatchStatement::new(
                    "UPDATE byok_logical_object_generation SET outcome='abandoned',completed_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000) WHERE tenant_id=?1 AND object_kind=?2 AND logical_key=?3 AND generation=?4 AND allocation_id=?5 AND physical_key=?6 AND intent_token=?7 AND gate_epoch=?8 AND outcome='allocated'",
                    vec![json!(staged.tenant_id),json!(staged.kind.as_str()),json!(staged.logical_key),json!(staged.generation),json!(staged.allocation_id),json!(staged.physical_key),json!(staged.intent_token),json!(staged.gate_epoch)],
                ),
                D1BatchStatement::new(
                    "INSERT INTO byok_object_purge_item (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id,physical_key,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) SELECT lower(hex(randomblob(16))),?1,NULL,?2,?3,?4,?5,?6,?7,'publish_loser','pending',(CAST(strftime('%s','now') AS INTEGER)*1000),(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_logical_object_generation x WHERE x.tenant_id=?1 AND x.object_kind=?2 AND x.logical_key=?3 AND x.generation=?4 AND x.allocation_id=?5 AND x.physical_key=?6 AND x.intent_token=?8 AND x.gate_epoch=?9 AND x.outcome='abandoned' ON CONFLICT(tenant_id,physical_key) DO NOTHING",
                    vec![json!(staged.tenant_id),json!(staged.kind.as_str()),json!(staged.logical_key),json!(staged.generation),json!(staged.allocation_id),json!(staged.physical_key),json!(staged.crypto_mode),json!(staged.intent_token),json!(staged.gate_epoch)],
                ),
                D1BatchStatement::new(
                    "INSERT OR IGNORE INTO byok_object_purge_cause (purge_id,cause_kind,cause_id,created_at_ms) SELECT purge_id,'publish_loser',allocation_id,(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_object_purge_item WHERE tenant_id=?1 AND object_kind=?2 AND logical_key=?3 AND generation=?4 AND allocation_id=?5 AND physical_key=?6",
                    vec![json!(staged.tenant_id),json!(staged.kind.as_str()),json!(staged.logical_key),json!(staged.generation),json!(staged.allocation_id),json!(staged.physical_key)],
                ),
            ])
            .await
            .map(|_| ())
            .map_err(|e| format!("BYOK publish-loser recovery transaction: {}", e.message))
    }

    /// Reconcile staged allocations whose publication callback was lost. This
    /// bounded census is safe to run after a D1 timeout: it only classifies an
    /// old allocation with no publication when its exact owning data intent is
    /// terminal or expired, then creates the exact loser purge row from the
    /// allocation identity. A failed publication therefore has a durable
    /// recovery path even when the immediate recovery batch was unavailable;
    /// a slow, still-authorized writer is left allocated for its callback.
    pub async fn reconcile_stale_allocations(
        &self,
        older_than_ms: i64,
        limit: u32,
    ) -> Result<(), String> {
        if older_than_ms < 0 || limit == 0 {
            return Err(
                "stale allocation census requires a non-negative cutoff and positive limit"
                    .to_owned(),
            );
        }
        let limit = i64::from(limit.min(1_000));
        let selected = STALE_ALLOCATION_SELECTION;
        let collisions = format!(
            "SELECT g.rowid FROM byok_logical_object_generation g WHERE g.outcome IN ('allocated','abandoned') AND g.allocated_at_ms<=?1 AND {STALE_ALLOCATION_OWNER} AND EXISTS (SELECT 1 FROM byok_object_purge_item p WHERE p.tenant_id=g.tenant_id AND p.physical_key=g.physical_key AND (p.object_kind<>g.object_kind OR p.logical_key<>g.logical_key OR p.generation<>g.generation OR p.allocation_id IS NOT g.allocation_id)) ORDER BY g.allocated_at_ms,g.rowid LIMIT ?2"
        );
        self.d1
            .batch(vec![
                D1BatchStatement::new(
                    format!("INSERT OR IGNORE INTO byok_purge_identity_quarantine (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,conflicting_purge_id,detected_at_ms) SELECT g.tenant_id,g.object_kind,g.logical_key,g.generation,g.allocation_id,g.physical_key,p.purge_id,(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_logical_object_generation g JOIN ({collisions}) s ON s.rowid=g.rowid JOIN byok_object_purge_item p ON p.tenant_id=g.tenant_id AND p.physical_key=g.physical_key AND (p.object_kind<>g.object_kind OR p.logical_key<>g.logical_key OR p.generation<>g.generation OR p.allocation_id IS NOT g.allocation_id)"),
                    vec![json!(older_than_ms), json!(limit)],
                ),
                D1BatchStatement::new(
                    format!("INSERT INTO byok_object_purge_item (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id,physical_key,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) SELECT lower(hex(randomblob(16))),g.tenant_id,NULL,g.object_kind,g.logical_key,g.generation,g.allocation_id,g.physical_key,s.crypto_mode,'publish_loser','pending',(CAST(strftime('%s','now') AS INTEGER)*1000),(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_logical_object_generation g JOIN ({selected}) s ON s.rowid=g.rowid ON CONFLICT(tenant_id,physical_key) DO NOTHING"),
                    vec![json!(older_than_ms), json!(limit)],
                ),
                D1BatchStatement::new(
                    format!("INSERT OR IGNORE INTO byok_object_purge_cause (purge_id,cause_kind,cause_id,created_at_ms) SELECT p.purge_id,'publish_loser',g.allocation_id,(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_logical_object_generation g JOIN ({selected}) s ON s.rowid=g.rowid JOIN byok_object_purge_item p ON p.tenant_id=g.tenant_id AND p.object_kind=g.object_kind AND p.logical_key=g.logical_key AND p.generation=g.generation AND p.allocation_id=g.allocation_id AND p.physical_key=g.physical_key"),
                    vec![json!(older_than_ms), json!(limit)],
                ),
                D1BatchStatement::new(
                    format!("UPDATE byok_logical_object_generation AS g SET outcome='abandoned',completed_at_ms=COALESCE(completed_at_ms,(CAST(strftime('%s','now') AS INTEGER)*1000)) WHERE g.rowid IN (SELECT rowid FROM ({selected})) AND EXISTS (SELECT 1 FROM byok_object_purge_item p JOIN byok_object_purge_cause c ON c.purge_id=p.purge_id AND c.cause_kind='publish_loser' AND c.cause_id=g.allocation_id WHERE p.tenant_id=g.tenant_id AND p.object_kind=g.object_kind AND p.logical_key=g.logical_key AND p.generation=g.generation AND p.allocation_id=g.allocation_id AND p.physical_key=g.physical_key)"),
                    vec![json!(older_than_ms), json!(limit)],
                ),
            ])
            .await
            .map(|_| ())
            .map_err(|e| format!("BYOK stale allocation census: {}", e.message))
    }

    /// Use the authoritative D1 clock for a bounded background census page.
    pub async fn reconcile_stale_page(&self, limit: u32) -> Result<(), String> {
        let rows = self
            .d1
            .query(
                "SELECT (CAST(strftime('%s','now') AS INTEGER)*1000)-?1 AS cutoff",
                &[json!(STALE_ALLOCATION_GRACE_MS)],
            )
            .await?;
        let cutoff = rows
            .first()
            .and_then(|row| row.get("cutoff"))
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| "BYOK stale allocation census clock returned no cutoff".to_owned())?;
        self.reconcile_stale_allocations(cutoff, limit).await
    }

    async fn claim_purge_page(&self, limit: u32) -> Result<Vec<ByokPurgePlan>, String> {
        if limit == 0 {
            return Err("purge reconciler requires a positive limit".to_owned());
        }
        let limit = i64::from(limit.min(128));
        let claim_sql = "UPDATE byok_object_purge_item SET state=CASE WHEN state IN ('pending','retry') THEN 'deleting' ELSE state END,claim_owner='data-plane',claim_token=lower(hex(randomblob(16))),claim_epoch=claim_epoch+1,claim_expires_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000)+?1,attempts=attempts+1,next_attempt_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000),last_error=NULL WHERE rowid IN (SELECT p.rowid FROM byok_object_purge_item p WHERE ((NOT EXISTS (SELECT 1 FROM byok_object_purge_cause pc WHERE pc.purge_id=p.purge_id AND pc.cause_kind='publish_loser' AND pc.cause_id=p.allocation_id) AND (p.reason='live_delete' OR (p.reason='activation_source' AND p.intent_id IS NOT NULL AND EXISTS (SELECT 1 FROM byok_activation_intent a WHERE a.intent_id=p.intent_id AND a.tenant_id=p.tenant_id AND a.phase='preempted')))) OR (EXISTS (SELECT 1 FROM byok_object_purge_cause pc WHERE pc.purge_id=p.purge_id AND pc.cause_kind='publish_loser' AND pc.cause_id=p.allocation_id) AND EXISTS (SELECT 1 FROM byok_logical_object_generation g WHERE g.tenant_id=p.tenant_id AND g.object_kind=p.object_kind AND g.logical_key=p.logical_key AND g.generation=p.generation AND g.allocation_id=p.allocation_id AND g.physical_key=p.physical_key AND g.outcome='abandoned' AND (EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.tenant_id=g.tenant_id AND i.token=g.intent_token AND i.operation='write' AND i.expires_at_ms+?3<=(CAST(strftime('%s','now') AS INTEGER)*1000)) OR EXISTS (SELECT 1 FROM byok_backfill_run r JOIN byok_transition_fence f ON f.tenant_id=r.tenant_id AND f.token=r.intent_token AND f.epoch=r.transition_epoch WHERE r.tenant_id=g.tenant_id AND r.run_id=g.backfill_run_id AND r.target_generation=g.generation AND f.expires_at_ms+?3<=(CAST(strftime('%s','now') AS INTEGER)*1000)) OR EXISTS (SELECT 1 FROM byok_activation_source_object s JOIN byok_activation_intent a ON a.intent_id=s.intent_id AND a.tenant_id=s.tenant_id WHERE s.intent_id=g.backfill_run_id AND s.tenant_id=g.tenant_id AND s.object_kind=g.object_kind AND s.logical_key=g.logical_key AND s.target_allocation_id=g.allocation_id AND s.target_physical_key=g.physical_key AND a.phase='preempted' AND s.target_write_expires_at_ms+?3<=(CAST(strftime('%s','now') AS INTEGER)*1000)))))) AND ((p.reason='activation_source' AND p.generation>=0) OR (p.generation>0)) AND ((p.state IN ('pending','retry') AND p.next_attempt_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000)) OR (p.state='r2_absent' AND p.claim_owner IS NULL AND p.claim_token IS NULL AND p.claim_expires_at_ms IS NULL AND p.next_attempt_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000)) OR (p.state IN ('deleting','r2_absent') AND p.claim_expires_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000))) ORDER BY p.next_attempt_at_ms,p.purge_id LIMIT ?2) AND (state IN ('pending','retry') OR (state IN ('deleting','r2_absent') AND (claim_expires_at_ms IS NULL OR claim_expires_at_ms<=(CAST(strftime('%s','now') AS INTEGER)*1000)))) RETURNING purge_id,tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,crypto_mode,reason,state,claim_token,claim_epoch,claim_expires_at_ms";
        let rows = self
            .d1
            .query(
                claim_sql,
                &[
                    json!(PURGE_CLAIM_LEASE_MS),
                    json!(limit),
                    json!(PURGE_DISPATCH_TAIL_MS),
                ],
            )
            .await?;
        let mut plans = rows
            .iter()
            .map(|row| {
                let text = |name: &str| {
                    row.get(name)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .ok_or_else(|| format!("BYOK purge row missing {name}"))
                };
                let integer = |name: &str| {
                    row.get(name)
                        .and_then(serde_json::Value::as_i64)
                        .ok_or_else(|| format!("BYOK purge row missing {name}"))
                };
                let kind = match text("object_kind")?.as_str() {
                    "cas" => ByokObjectKind::Cas,
                    "ac" => ByokObjectKind::Ac,
                    other => return Err(format!("BYOK purge row has invalid kind {other}")),
                };
                let reason = match text("reason")?.as_str() {
                    "activation_source" => ByokPurgeReason::ActivationSource,
                    "live_delete" => ByokPurgeReason::LiveDelete,
                    "publish_loser" => ByokPurgeReason::PublishLoser,
                    other => return Err(format!("BYOK purge row has invalid reason {other}")),
                };
                let generation = integer("generation")?;
                let allocation_id = row
                    .get("allocation_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let claim_token = text("claim_token")?;
                Ok(ByokPurgePlan {
                    purge_id: text("purge_id")?,
                    tenant_id: text("tenant_id")?,
                    kind,
                    object: PublishedObject {
                        logical_key: text("logical_key")?,
                        generation,
                        physical_key: text("physical_key")?,
                        allocation_id,
                        gate_epoch: 0,
                        size_bytes: 0,
                        published_at_ms: 0,
                    },
                    crypto_mode: text("crypto_mode")?,
                    reason,
                    claim_token: Some(claim_token),
                    claim_epoch: integer("claim_epoch")?,
                    claim_expires_at_ms: row
                        .get("claim_expires_at_ms")
                        .and_then(serde_json::Value::as_i64),
                    attempts: 0,
                    r2_absent: text("state")? == "r2_absent",
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for plan in &mut plans {
            let rows = self
                .d1
                .query(
                    "SELECT attempts FROM byok_object_purge_item WHERE purge_id=?1 AND tenant_id=?2 AND claim_token=?3 AND claim_epoch=?4",
                    &[
                        json!(plan.purge_id),
                        json!(plan.tenant_id),
                        json!(plan.claim_token),
                        json!(plan.claim_epoch),
                    ],
                )
                .await?;
            plan.attempts = rows
                .first()
                .and_then(|row| row.get("attempts"))
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| "BYOK purge claim missing attempts".to_owned())?;
        }
        Ok(plans)
    }

    async fn finish_invalid_identity_purge_attempt(
        &self,
        plan: &ByokPurgePlan,
        reason: &str,
    ) -> Result<(), String> {
        let claim = plan
            .claim_token
            .as_deref()
            .ok_or_else(|| "BYOK purge completion requires a claim token".to_owned())?;
        let reason = bounded_purge_error(reason);
        let rows = self
            .d1
            .query(
                "UPDATE byok_object_purge_item SET state=CASE WHEN attempts>=?6 THEN 'quarantined' ELSE CASE WHEN state='r2_absent' THEN 'r2_absent' ELSE 'retry' END END,next_attempt_at_ms=CASE WHEN attempts>=?6 THEN next_attempt_at_ms ELSE (CAST(strftime('%s','now') AS INTEGER)*1000) END,last_error=?7,quarantine_reason=CASE WHEN attempts>=?6 THEN ?7 ELSE NULL END,quarantined_at_ms=CASE WHEN attempts>=?6 THEN (CAST(strftime('%s','now') AS INTEGER)*1000) ELSE NULL END,verified_at_ms=CASE WHEN attempts>=?6 THEN (CAST(strftime('%s','now') AS INTEGER)*1000) ELSE NULL END,claim_epoch=claim_epoch+1,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL WHERE purge_id=?1 AND tenant_id=?2 AND physical_key=?3 AND state IN ('deleting','r2_absent') AND claim_owner='data-plane' AND claim_token=?4 AND claim_epoch=?5 AND claim_expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) RETURNING purge_id,state",
                &[
                    json!(plan.purge_id),
                    json!(plan.tenant_id),
                    json!(plan.object.physical_key),
                    json!(claim),
                    json!(plan.claim_epoch),
                    json!(COMMON_PURGE_INVALID_IDENTITY_QUARANTINE_AFTER),
                    json!(reason),
                ],
            )
            .await?;
        if rows.is_empty() {
            return Err("BYOK invalid-identity quarantine lost its exact claim".to_owned());
        }
        Ok(())
    }
}

#[async_trait]
impl ByokPurgeReconciler for D1ByokGenerationCatalog {
    async fn reconcile_stale_page(&self, limit: u32) -> Result<(), String> {
        self.reconcile_stale_page(limit).await
    }

    async fn claim_purge_page(&self, limit: u32) -> Result<Vec<ByokPurgePlan>, String> {
        self.claim_purge_page(limit).await
    }

    async fn finish_claimed_purge_attempt(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: Option<&str>,
        envelope_reclaimed: bool,
        invalid_identity_reason: Option<&str>,
    ) -> Result<(), String> {
        if let Some(reason) = invalid_identity_reason {
            return self
                .finish_invalid_identity_purge_attempt(plan, reason)
                .await;
        }
        ByokGenerationCatalog::finish_purge_attempt(
            self,
            plan,
            head_absent,
            error,
            envelope_reclaimed,
        )
        .await
    }

    async fn reconcile_stale_allocations(
        &self,
        older_than_ms: i64,
        limit: u32,
    ) -> Result<(), String> {
        self.reconcile_stale_allocations(older_than_ms, limit).await
    }
}

#[async_trait]
impl ByokGenerationCatalog for D1ByokGenerationCatalog {
    async fn resolve(
        &self,
        tenant_id: &str,
        kind: ByokObjectKind,
        logical_key: &str,
        generation: i64,
    ) -> Result<Option<PublishedObject>, String> {
        let rows = self.d1.query(
            "SELECT logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,published_at_ms FROM byok_logical_object_publication WHERE tenant_id=?1 AND object_kind=?2 AND logical_key=?3 AND generation=?4 LIMIT 1",
            &[json!(tenant_id), json!(kind.as_str()), json!(logical_key), json!(generation)],
        ).await?;
        rows.first().map(Self::parse).transpose()
    }

    async fn allocate(&self, staged: &StagedObject) -> Result<(), String> {
        let rows = self.d1.query(
            "INSERT INTO byok_logical_object_generation (tenant_id,object_kind,logical_key,generation,allocation_id,physical_key,intent_token,gate_epoch,outcome,size_bytes,allocated_at_ms) SELECT ?1,?2,?3,?4,?5,?6,?7,?8,'allocated',?9,(CAST(strftime('%s','now') AS INTEGER)*1000) WHERE EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.tenant_id=?1 AND i.token=?7 AND i.observed_gate_epoch=?8 AND i.observed_generation=?4 AND i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) AND i.outcome='active') AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?1 AND g.gate_epoch=?8 AND g.current_generation=?4) RETURNING allocation_id",
            &[json!(staged.tenant_id), json!(staged.kind.as_str()), json!(staged.logical_key), json!(staged.generation), json!(staged.allocation_id), json!(staged.physical_key), json!(staged.intent_token), json!(staged.gate_epoch), json!(staged.size_bytes)],
        ).await?;
        if rows.is_empty() {
            return Err("BYOK catalog allocation rejected stale intent/epoch".to_owned());
        }
        Ok(())
    }

    async fn publish(&self, staged: &StagedObject) -> Result<bool, String> {
        // The publication, allocation classification and loser ledger are one
        // D1 transaction. A stale capability therefore cannot leave an R2
        // object without a durable exact-key cleanup obligation.
        let result = match self.d1.batch(vec![
            D1BatchStatement::new(
                PUBLISH_CATALOG_SQL,
                vec![json!(staged.tenant_id),json!(staged.kind.as_str()),json!(staged.logical_key),json!(staged.generation),json!(staged.allocation_id),json!(staged.physical_key),json!(staged.gate_epoch),json!(staged.size_bytes),json!(staged.intent_token)],
            ),
            D1BatchStatement::new(
                "UPDATE byok_logical_object_generation SET outcome=CASE WHEN EXISTS (SELECT 1 FROM byok_logical_object_publication p WHERE p.tenant_id=?1 AND p.object_kind=?2 AND p.logical_key=?3 AND p.generation=?4 AND p.allocation_id=?5 AND p.physical_key=?6 AND p.gate_epoch=?8) THEN 'published' ELSE 'abandoned' END,completed_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000) WHERE tenant_id=?1 AND object_kind=?2 AND logical_key=?3 AND generation=?4 AND allocation_id=?5 AND physical_key=?6 AND intent_token=?7 AND gate_epoch=?8 AND outcome='allocated'",
                vec![json!(staged.tenant_id),json!(staged.kind.as_str()),json!(staged.logical_key),json!(staged.generation),json!(staged.allocation_id),json!(staged.physical_key),json!(staged.intent_token),json!(staged.gate_epoch)],
            ),
            D1BatchStatement::new(
                "INSERT INTO byok_object_purge_item (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id,physical_key,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) SELECT lower(hex(randomblob(16))),?1,NULL,?2,?3,?4,?5,?6,?7,'publish_loser','pending',(CAST(strftime('%s','now') AS INTEGER)*1000),(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_logical_object_generation x WHERE x.tenant_id=?1 AND x.object_kind=?2 AND x.logical_key=?3 AND x.generation=?4 AND x.allocation_id=?5 AND x.physical_key=?6 AND x.intent_token=?8 AND x.gate_epoch=?9 AND x.outcome='abandoned' ON CONFLICT(tenant_id,physical_key) DO NOTHING",
                vec![json!(staged.tenant_id),json!(staged.kind.as_str()),json!(staged.logical_key),json!(staged.generation),json!(staged.allocation_id),json!(staged.physical_key),json!(staged.crypto_mode),json!(staged.intent_token),json!(staged.gate_epoch)],
            ),
            D1BatchStatement::new(
                "INSERT OR IGNORE INTO byok_object_purge_cause (purge_id,cause_kind,cause_id,created_at_ms) SELECT purge_id,'publish_loser',allocation_id,(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_object_purge_item WHERE tenant_id=?1 AND object_kind=?2 AND logical_key=?3 AND generation=?4 AND allocation_id=?5 AND physical_key=?6 AND reason='publish_loser'",
                vec![json!(staged.tenant_id),json!(staged.kind.as_str()),json!(staged.logical_key),json!(staged.generation),json!(staged.allocation_id),json!(staged.physical_key)],
            ),
        ]).await {
            Ok(result) => result,
            Err(error) => {
                let recovery = self.abandon_publish_loser(staged).await;
                return Err(match recovery {
                    Ok(()) => format!("BYOK catalog publish transaction: {}; loser recorded", error.message),
                    Err(recovery_error) => format!("BYOK catalog publish transaction: {}; loser recovery: {recovery_error}; stale allocation census required", error.message),
                });
            }
        };
        Ok(result.first().is_some_and(|rows| !rows.is_empty()))
    }

    async fn tombstone(
        &self,
        tenant_id: &str,
        kind: ByokObjectKind,
        logical_key: &str,
        generation: i64,
        intent_token: &str,
        gate_epoch: i64,
        crypto_mode: &str,
    ) -> Result<Option<ByokPurgePlan>, String> {
        let result = self.d1.batch(vec![
            D1BatchStatement::new(
                "INSERT INTO byok_object_purge_item (purge_id,tenant_id,intent_id,object_kind,logical_key,generation,allocation_id,physical_key,crypto_mode,reason,state,next_attempt_at_ms,created_at_ms) SELECT lower(hex(randomblob(16))),p.tenant_id,NULL,p.object_kind,p.logical_key,p.generation,p.allocation_id,p.physical_key,?7,'live_delete','pending',(CAST(strftime('%s','now') AS INTEGER)*1000),(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_logical_object_publication p WHERE p.tenant_id=?1 AND p.object_kind=?2 AND p.logical_key=?3 AND p.generation=?4 AND p.gate_epoch=?6 AND EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.tenant_id=?1 AND i.token=?5 AND i.observed_gate_epoch=?6 AND i.observed_generation=?4 AND i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) AND i.outcome='active') AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?1 AND g.gate_epoch=?6 AND g.current_generation=?4) ON CONFLICT(tenant_id,physical_key) DO NOTHING",
                vec![json!(tenant_id),json!(kind.as_str()),json!(logical_key),json!(generation),json!(intent_token),json!(gate_epoch),json!(crypto_mode)],
            ),
            D1BatchStatement::new(
                "INSERT OR IGNORE INTO byok_object_purge_cause (purge_id,cause_kind,cause_id,created_at_ms) SELECT p.purge_id,'live_delete',p.logical_key,(CAST(strftime('%s','now') AS INTEGER)*1000) FROM byok_object_purge_item p JOIN byok_logical_object_publication o ON o.tenant_id=p.tenant_id AND o.object_kind=p.object_kind AND o.logical_key=p.logical_key AND o.generation=p.generation AND o.allocation_id=p.allocation_id AND o.physical_key=p.physical_key WHERE p.tenant_id=?1 AND p.object_kind=?2 AND p.logical_key=?3 AND p.generation=?4",
                vec![json!(tenant_id),json!(kind.as_str()),json!(logical_key),json!(generation)],
            ),
            D1BatchStatement::new(
                "DELETE FROM byok_logical_object_publication WHERE tenant_id=?1 AND object_kind=?2 AND logical_key=?3 AND generation=?4 AND gate_epoch=?6 AND EXISTS (SELECT 1 FROM byok_data_intent i WHERE i.tenant_id=?1 AND i.token=?5 AND i.observed_gate_epoch=?6 AND i.observed_generation=?4 AND i.expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) AND i.outcome='active') AND EXISTS (SELECT 1 FROM byok_tenant_gate g WHERE g.tenant_id=?1 AND g.gate_epoch=?6 AND g.current_generation=?4) RETURNING logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,published_at_ms",
                vec![json!(tenant_id),json!(kind.as_str()),json!(logical_key),json!(generation),json!(intent_token),json!(gate_epoch)],
            ),
            D1BatchStatement::new(
                "SELECT purge_id,crypto_mode FROM byok_object_purge_item WHERE tenant_id=?1 AND object_kind=?2 AND logical_key=?3 AND generation=?4 ORDER BY created_at_ms DESC LIMIT 1",
                vec![json!(tenant_id),json!(kind.as_str()),json!(logical_key),json!(generation)],
            ),
        ]).await.map_err(|e| format!("BYOK catalog tombstone transaction: {}", e.message))?;
        let Some(publication_row) = result.get(2).and_then(|rows| rows.first()) else {
            return Ok(None);
        };
        let object = Self::parse(publication_row)?;
        let purge_row = result
            .get(3)
            .and_then(|rows| rows.first())
            .ok_or_else(|| "BYOK live-delete purge row missing after tombstone".to_owned())?;
        let purge_id = purge_row
            .get("purge_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "BYOK live-delete purge row missing purge_id".to_owned())?
            .to_owned();
        let crypto_mode = purge_row
            .get("crypto_mode")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "BYOK live-delete purge row missing crypto_mode".to_owned())?
            .to_owned();
        Ok(Some(ByokPurgePlan {
            purge_id,
            tenant_id: tenant_id.to_owned(),
            kind,
            object,
            crypto_mode,
            reason: ByokPurgeReason::LiveDelete,
            claim_token: None,
            claim_epoch: 0,
            attempts: 0,
            claim_expires_at_ms: None,
            r2_absent: false,
        }))
    }

    async fn begin_purge_attempt(&self, plan: &mut ByokPurgePlan) -> Result<(), String> {
        let rows = self
            .d1
            .query(
                "UPDATE byok_object_purge_item SET state=CASE WHEN state IN ('pending','retry') THEN 'deleting' ELSE state END,claim_owner='data-plane',claim_token=lower(hex(randomblob(16))),claim_epoch=claim_epoch+1,claim_expires_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000)+?4,attempts=attempts+1,next_attempt_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000),last_error=NULL WHERE purge_id=?1 AND tenant_id=?2 AND physical_key=?3 AND (state IN ('pending','retry') OR (state IN ('deleting','r2_absent') AND (claim_expires_at_ms IS NULL OR claim_expires_at_ms <= (CAST(strftime('%s','now') AS INTEGER)*1000)))) RETURNING state,claim_token,claim_epoch,claim_expires_at_ms",
                &[json!(plan.purge_id), json!(plan.tenant_id), json!(plan.object.physical_key), json!(PURGE_CLAIM_LEASE_MS)],
            )
            .await?;
        let row = rows.first().ok_or_else(|| {
            "BYOK purge claim lost or already owned by another replica".to_owned()
        })?;
        let state = row
            .get("state")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "BYOK purge claim missing state".to_owned())?;
        plan.r2_absent = state == "r2_absent";
        plan.claim_token = Some(
            row.get("claim_token")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "BYOK purge claim missing token".to_owned())?
                .to_owned(),
        );
        plan.claim_epoch = row
            .get("claim_epoch")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| "BYOK purge claim missing epoch".to_owned())?;
        plan.claim_expires_at_ms = row
            .get("claim_expires_at_ms")
            .and_then(serde_json::Value::as_i64);
        Ok(())
    }

    async fn finish_purge_attempt(
        &self,
        plan: &ByokPurgePlan,
        head_absent: bool,
        error: Option<&str>,
        envelope_reclaimed: bool,
    ) -> Result<(), String> {
        let claim = plan
            .claim_token
            .as_deref()
            .ok_or_else(|| "BYOK purge completion requires a claim token".to_owned())?;
        let claim_epoch = json!(plan.claim_epoch);
        let rows = if head_absent && envelope_reclaimed {
            self.d1
                .query(
                    "UPDATE byok_object_purge_item SET state='verified',verified_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000),last_error=NULL,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL WHERE purge_id=?1 AND tenant_id=?2 AND physical_key=?3 AND state='r2_absent' AND claim_owner='data-plane' AND claim_token=?4 AND claim_epoch=?5 AND claim_expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) RETURNING purge_id",
                    &[json!(plan.purge_id),json!(plan.tenant_id),json!(plan.object.physical_key),json!(claim),claim_epoch],
                )
                .await?
        } else if head_absent && error.is_none() {
            self.d1
                .query(
                    "UPDATE byok_object_purge_item SET state='r2_absent',last_error=NULL WHERE purge_id=?1 AND tenant_id=?2 AND physical_key=?3 AND state IN ('deleting','r2_absent') AND claim_owner='data-plane' AND claim_token=?4 AND claim_epoch=?5 AND claim_expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) RETURNING purge_id",
                    &[json!(plan.purge_id),json!(plan.tenant_id),json!(plan.object.physical_key),json!(claim),claim_epoch],
                )
                .await?
        } else {
            self.d1
                .query(
                    "UPDATE byok_object_purge_item SET state=CASE WHEN state='r2_absent' THEN 'r2_absent' ELSE 'retry' END,next_attempt_at_ms=(CAST(strftime('%s','now') AS INTEGER)*1000),last_error=?6,claim_epoch=claim_epoch+1,claim_owner=NULL,claim_token=NULL,claim_expires_at_ms=NULL WHERE purge_id=?1 AND tenant_id=?2 AND physical_key=?3 AND state IN ('deleting','r2_absent') AND claim_owner='data-plane' AND claim_token=?4 AND claim_epoch=?5 AND claim_expires_at_ms>(CAST(strftime('%s','now') AS INTEGER)*1000) RETURNING purge_id",
                    &[json!(plan.purge_id),json!(plan.tenant_id),json!(plan.object.physical_key),json!(claim),claim_epoch,json!(error.unwrap_or("R2 purge attempt failed"))],
                )
                .await?
        };
        if rows.is_empty() {
            return Err("BYOK purge completion lost its exact claim".to_owned());
        }
        Ok(())
    }

    async fn list(
        &self,
        tenant_id: &str,
        kind: ByokObjectKind,
        generation: i64,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Vec<PublishedObject>, String> {
        let rows = self.d1.query(
            "SELECT logical_key,generation,allocation_id,physical_key,gate_epoch,size_bytes,published_at_ms FROM byok_logical_object_publication WHERE tenant_id=?1 AND object_kind=?2 AND generation=?3 AND logical_key>?4 ORDER BY logical_key LIMIT ?5",
            &[json!(tenant_id),json!(kind.as_str()),json!(generation),json!(after.unwrap_or("")),json!(limit.clamp(1,1000))],
        ).await?;
        rows.iter().map(Self::parse).collect()
    }
}

fn bounded_purge_error(error: &str) -> String {
    const MAX_PURGE_ERROR_BYTES: usize = 512;
    if error.len() <= MAX_PURGE_ERROR_BYTES {
        return error.to_owned();
    }
    let mut end = MAX_PURGE_ERROR_BYTES;
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].to_owned()
}

/// Add an unambiguous generation segment before a physical digest.  Generation
/// zero is reserved for the read-only legacy raw-key fallback and is never a
/// valid destination for a new gated write.
pub fn generation_qualified_digest(
    generation: i64,
    allocation_id: &str,
    digest: &str,
) -> Result<String, String> {
    if generation <= 0 {
        return Err("BYOK write generation must be positive".to_owned());
    }
    if allocation_id.is_empty() || allocation_id.contains('/') {
        return Err("BYOK allocation id must be non-empty and path-safe".to_owned());
    }
    Ok(format!("generation/{generation}/{allocation_id}/{digest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_allocations_in_one_generation_get_distinct_physical_keys() {
        let first = generation_qualified_digest(7, "write-a", "digest");
        let second = generation_qualified_digest(7, "write-b", "digest");
        assert_ne!(first, second);
        assert_eq!(first.as_deref(), Ok("generation/7/write-a/digest"));
        assert_eq!(second.as_deref(), Ok("generation/7/write-b/digest"));
    }

    #[test]
    fn generation_zero_and_path_escaping_allocation_ids_are_rejected() {
        assert!(generation_qualified_digest(0, "write-a", "digest").is_err());
        assert!(generation_qualified_digest(1, "../escape", "digest").is_err());
    }

    #[test]
    fn purge_reasons_match_durable_ledger_literals() {
        assert_eq!(
            ByokPurgeReason::ActivationSource.as_str(),
            "activation_source"
        );
        assert_eq!(ByokPurgeReason::LiveDelete.as_str(), "live_delete");
        assert_eq!(ByokPurgeReason::PublishLoser.as_str(), "publish_loser");
    }

    #[test]
    fn purge_plan_debug_redacts_replica_claim_token() {
        let plan = ByokPurgePlan {
            purge_id: "purge".to_owned(),
            tenant_id: "tenant".to_owned(),
            kind: ByokObjectKind::Cas,
            object: PublishedObject {
                logical_key: "blake3:digest".to_owned(),
                generation: 2,
                physical_key: "generation/2/allocation/digest".to_owned(),
                allocation_id: "allocation".to_owned(),
                gate_epoch: 4,
                size_bytes: 10,
                published_at_ms: 5,
            },
            crypto_mode: "random".to_owned(),
            reason: ByokPurgeReason::LiveDelete,
            claim_token: Some("do-not-log-this-claim".to_owned()),
            claim_epoch: 3,
            attempts: 1,
            claim_expires_at_ms: Some(120_000),
            r2_absent: false,
        };
        let rendered = format!("{plan:?}");
        assert!(!rendered.contains("do-not-log-this-claim"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn stale_census_requires_terminal_owner_capability() {
        assert!(STALE_ALLOCATION_OWNER.contains("byok_data_intent"));
        assert!(STALE_ALLOCATION_OWNER.contains("byok_backfill_run"));
        assert!(STALE_ALLOCATION_OWNER.contains("outcome IN ('completed','expired')"));
        assert!(STALE_ALLOCATION_OWNER.contains("phase='aborted'"));
        assert!(STALE_ALLOCATION_OWNER.contains("expires_at_ms<="));
    }

    #[test]
    fn staged_debug_redacts_live_intent_token() {
        let staged = StagedObject {
            tenant_id: "tenant".to_owned(),
            kind: ByokObjectKind::Ac,
            logical_key: "action".to_owned(),
            generation: 2,
            allocation_id: "allocation".to_owned(),
            physical_key: "physical".to_owned(),
            intent_token: "do-not-log-this-capability".to_owned(),
            gate_epoch: 4,
            size_bytes: 10,
            crypto_mode: "convergent".to_owned(),
        };
        let rendered = format!("{staged:?}");
        assert!(!rendered.contains("do-not-log-this-capability"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn publication_rejects_mutated_size_after_allocation() {
        use rusqlite::{params, Connection};

        let db = Connection::open_in_memory().expect("sqlite");
        db.execute_batch(
            "CREATE TABLE byok_logical_object_generation (tenant_id TEXT, object_kind TEXT, logical_key TEXT, generation INTEGER, allocation_id TEXT, physical_key TEXT, intent_token TEXT, gate_epoch INTEGER, size_bytes INTEGER, outcome TEXT); \
             CREATE TABLE byok_logical_object_publication (tenant_id TEXT, object_kind TEXT, logical_key TEXT, generation INTEGER, allocation_id TEXT, physical_key TEXT, gate_epoch INTEGER, size_bytes INTEGER, published_at_ms INTEGER, PRIMARY KEY (tenant_id,object_kind,logical_key)); \
             CREATE TABLE byok_data_intent (tenant_id TEXT, token TEXT, observed_gate_epoch INTEGER, observed_generation INTEGER, expires_at_ms INTEGER, outcome TEXT); \
             CREATE TABLE byok_tenant_gate (tenant_id TEXT, gate_epoch INTEGER, current_generation INTEGER); \
             INSERT INTO byok_logical_object_generation VALUES ('tenant','cas','blake3:digest',7,'allocation','physical','intent',3,10,'allocated'); \
             INSERT INTO byok_data_intent VALUES ('tenant','intent',3,7,9223372036854775807,'active'); \
             INSERT INTO byok_tenant_gate VALUES ('tenant',3,7);",
        )
        .expect("catalog fixture");

        let publish = |size_bytes: i64| {
            let mut statement = db.prepare(PUBLISH_CATALOG_SQL).expect("publish SQL");
            let mut rows = statement
                .query(params![
                    "tenant",
                    "cas",
                    "blake3:digest",
                    7_i64,
                    "allocation",
                    "physical",
                    3_i64,
                    size_bytes,
                    "intent"
                ])
                .expect("execute publication");
            rows.next().expect("read RETURNING row").is_some()
        };

        assert!(!publish(11), "mutated size must not publish");
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM byok_logical_object_publication",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("publication count"),
            0
        );
        assert!(publish(10), "exact allocated size must publish");
        assert_eq!(
            db.query_row(
                "SELECT size_bytes FROM byok_logical_object_publication",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("published size"),
            10
        );
    }
}
