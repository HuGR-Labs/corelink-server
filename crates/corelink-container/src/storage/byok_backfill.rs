//! Tenant-wide BYOK backfill orchestration.
//!
//! This module deliberately contains no D1 or R2 SQL/HTTP details.  The
//! [`ByokBackfillStore`] boundary is the typed contract implemented by the
//! production transition-fence adapter.  Its transaction guarantees make a
//! cross-store migration safe:
//!
//! 1. acquire one tenant-exclusive transition token;
//! 2. enumerate the immutable legacy/current-generation CAS and AC views;
//! 3. write ciphertext to a deterministic, generation-qualified physical key;
//! 4. atomically publish its target-generation catalog row and checkpoint;
//! 5. switch `current_generation` and `state = active` in one guarded commit.
//!
//! A crash between 3 and 4 leaves an unreachable staged object.  Retrying
//! writes the same key and then publishes it.  A crash after 4 resumes after
//! the durable cursor.  Until 5, the data plane remains blocked and cannot
//! resolve target-generation rows.  Old or delayed writers can only mutate an
//! old generation and therefore remain unaddressable after publication.

use async_trait::async_trait;
use std::time::Duration;
use zeroize::Zeroizing;

use super::byok_activation::SourceCryptoIdentity;
use crate::byok_transition_fence::MAX_BYOK_FENCE_LEASE;

/// CAS and AC are separate source/catalog namespaces and both must complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackfillSurface {
    /// Content-addressable storage objects.
    Cas,
    /// Action-cache result objects.
    Ac,
}

impl BackfillSurface {
    /// Stable physical/catalog label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cas => "cas",
            Self::Ac => "ac",
        }
    }
}

/// Durable phase of one tenant's migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackfillPhase {
    /// The transition is exclusive and target objects are being staged.
    Copying,
    /// Both source namespaces have been completely checkpointed.
    ReadyToCommit,
    /// The guarded generation switch completed.
    Committed,
    /// The transition was released without publishing its target generation.
    Aborted,
}

/// Durable run state returned by `begin`/`resume`.
#[derive(Clone, PartialEq, Eq)]
pub struct BackfillRun {
    /// Tenant whose object namespace is being migrated.
    pub tenant_id: String,
    /// Stable identity across bounded transition-lease takeovers.
    pub run_id: String,
    /// Current opaque transition capability; never expose outside internals.
    pub run_token: String,
    /// Monotonic transition-fence epoch owning the current lease.
    pub transition_epoch: i64,
    /// Tenant gate epoch pinned when the run began.
    pub gate_epoch: i64,
    /// Previously addressable encryption generation.
    pub source_generation: i64,
    /// Invisible generation receiving ciphertext during copy.
    pub target_generation: i64,
    /// Current durable lifecycle phase.
    pub phase: BackfillPhase,
    /// Opaque durable cursor for the CAS source enumeration.
    pub cas_cursor: Option<String>,
    /// Opaque durable cursor for the AC source enumeration.
    pub ac_cursor: Option<String>,
    /// Whether CAS enumeration has proved exhaustion.
    pub cas_complete: bool,
    /// Whether AC enumeration has proved exhaustion.
    pub ac_complete: bool,
}

impl core::fmt::Debug for BackfillRun {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BackfillRun")
            .field("tenant_id", &self.tenant_id)
            .field("run_id", &self.run_id)
            .field("run_token", &"[REDACTED]")
            .field("transition_epoch", &self.transition_epoch)
            .field("gate_epoch", &self.gate_epoch)
            .field("source_generation", &self.source_generation)
            .field("target_generation", &self.target_generation)
            .field("phase", &self.phase)
            .field("cas_cursor", &self.cas_cursor)
            .field("ac_cursor", &self.ac_cursor)
            .field("cas_complete", &self.cas_complete)
            .field("ac_complete", &self.ac_complete)
            .finish()
    }
}

impl BackfillRun {
    fn cursor(&self, surface: BackfillSurface) -> Option<&str> {
        match surface {
            BackfillSurface::Cas => self.cas_cursor.as_deref(),
            BackfillSurface::Ac => self.ac_cursor.as_deref(),
        }
    }

    fn complete(&self, surface: BackfillSurface) -> bool {
        match surface {
            BackfillSurface::Cas => self.cas_complete,
            BackfillSurface::Ac => self.ac_complete,
        }
    }

    fn apply_checkpoint(&mut self, checkpoint: &BackfillCheckpoint) {
        match checkpoint.surface {
            BackfillSurface::Cas => {
                self.cas_cursor.clone_from(&checkpoint.next_cursor);
                self.cas_complete = checkpoint.surface_complete;
            }
            BackfillSurface::Ac => {
                self.ac_cursor.clone_from(&checkpoint.next_cursor);
                self.ac_complete = checkpoint.surface_complete;
            }
        }
        if self.cas_complete && self.ac_complete {
            self.phase = BackfillPhase::ReadyToCommit;
        }
    }
}

/// One source object from the stable, transition-exclusive enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillSourceObject {
    /// CAS or AC namespace containing this source.
    pub surface: BackfillSurface,
    /// The client-visible digest/action key.
    pub logical_key: String,
    /// Raw legacy key or old generation-qualified physical key.
    pub source_physical_key: String,
    /// Exact source-generation decoder identity. Generation zero must carry
    /// [`SourceCryptoIdentity::Plaintext`]; rotations must carry the complete
    /// published envelope identity and allocation id.
    pub source_crypto: Option<SourceCryptoIdentity>,
    /// BLAKE3 of the exact bytes read from the source object, when supplied by
    /// the authoritative catalog. Rotated sources must provide this proof.
    pub source_blake3: Option<String>,
    /// Catalog plaintext size, when supplied by the authoritative catalog.
    pub plaintext_size: Option<u64>,
}

/// A stable page. The cursor is adapter-opaque (for R2 it is the S3
/// continuation token); `None` proves the surface was exhausted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillPage {
    /// Source objects included in this bounded page.
    pub objects: Vec<BackfillSourceObject>,
    /// Opaque adapter cursor, or `None` when the surface is exhausted.
    pub next_cursor: Option<String>,
}

/// Ciphertext staged at an invisible target-generation key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedBackfillObject {
    /// Deterministic idempotency identity; contains no plaintext or token.
    pub allocation_id: String,
    /// CAS or AC target namespace.
    pub surface: BackfillSurface,
    /// Algorithm-qualified CAS identity or bare AC action digest.
    pub logical_key: String,
    /// Invisible encryption generation receiving this object.
    pub target_generation: i64,
    /// Complete region- and tenant-scoped immutable R2 key.
    pub target_physical_key: String,
    /// Plaintext byte length published to the logical catalog.
    pub plaintext_len: u64,
    /// Encrypted on-disk representation to stage.
    pub ciphertext: Vec<u8>,
}

/// Metadata confirmed by the target R2 PUT. This is the only staged material
/// carried into D1; ciphertext never crosses the catalog transaction boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillAllocation {
    /// Stable allocation identity copied from the staged request.
    pub allocation_id: String,
    /// CAS or AC namespace.
    pub surface: BackfillSurface,
    /// Logical object identity.
    pub logical_key: String,
    /// Target encryption generation.
    pub target_generation: i64,
    /// Complete immutable R2 key confirmed by the PUT.
    pub target_physical_key: String,
    /// Plaintext byte length copied from the source object.
    pub plaintext_len: u64,
    /// Ciphertext byte length.
    pub ciphertext_len: u64,
    /// Lowercase BLAKE3 digest of the exact staged ciphertext.
    pub ciphertext_blake3: String,
}

/// One atomic logical-catalog publication plus durable cursor advancement.
///
/// The adapter MUST make the target-generation catalog upsert, counters, and
/// cursor/surface-complete update one D1 transaction guarded by the exact
/// `(tenant_id, run_token, gate_epoch, target_generation)` tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillCheckpoint {
    /// Tenant owning the run.
    pub tenant_id: String,
    /// Current opaque transition capability.
    pub run_token: String,
    /// Current transition epoch.
    pub transition_epoch: i64,
    /// Gate epoch pinned by the run.
    pub gate_epoch: i64,
    /// Invisible target generation.
    pub target_generation: i64,
    /// Surface whose cursor is advancing.
    pub surface: BackfillSurface,
    /// Optional staged allocation published atomically with this checkpoint.
    pub allocation: Option<BackfillAllocation>,
    /// Opaque next-page cursor; `None` may mean exhaustion.
    pub next_cursor: Option<String>,
    /// Explicit proof that enumeration reached the end of this surface.
    pub surface_complete: bool,
}

/// Start parameters. The store acquires the transition and generates its
/// high-entropy token; no caller-provided token is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginBackfill {
    /// Tenant to migrate.
    pub tenant_id: String,
    /// Exact gate epoch expected by the caller.
    pub expected_gate_epoch: i64,
    /// Exact published generation expected by the caller.
    pub expected_source_generation: i64,
    /// Bounded transition lease requested from D1.
    pub lease: Duration,
}

/// Typed failure. Every variant is fail-closed: none permits activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackfillError {
    /// Caller input violated a typed precondition.
    InvalidRequest(String),
    /// Durable state or a capability no longer permits the operation.
    Conflict(String),
    /// An enumerated source changed or became ambiguous.
    SourceChanged(String),
    /// TCS/KMS/encryption failed closed.
    Crypto(String),
    /// D1/R2 persistence or response decoding failed.
    Store(String),
}

impl core::fmt::Display for BackfillError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidRequest(value) => write!(f, "invalid BYOK backfill request: {value}"),
            Self::Conflict(value) => write!(f, "BYOK backfill conflict: {value}"),
            Self::SourceChanged(value) => write!(f, "BYOK backfill source changed: {value}"),
            Self::Crypto(value) => write!(f, "BYOK backfill crypto failure: {value}"),
            Self::Store(value) => write!(f, "BYOK backfill store failure: {value}"),
        }
    }
}

impl std::error::Error for BackfillError {}

/// Typed persistence/object-store contract for a production adapter.
#[async_trait]
pub trait ByokBackfillStore: Send + Sync {
    /// Atomically acquire transition exclusivity and create a partial run.
    async fn begin(&self, request: &BeginBackfill) -> Result<BackfillRun, BackfillError>;

    /// Resume the stable run. The store may retain a still-live token or
    /// atomically replace an expired exact token with a new transition fence.
    async fn resume(
        &self,
        tenant_id: &str,
        run_id: &str,
        previous_token: &str,
        lease: Duration,
    ) -> Result<BackfillRun, BackfillError>;

    /// Renew exclusivity before I/O. An expired/lost token must fail closed.
    async fn renew(&self, run: &BackfillRun, lease: Duration) -> Result<(), BackfillError>;

    /// Enumerate raw legacy plus the pinned source generation, never staging.
    async fn enumerate(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        after: Option<&str>,
        limit: usize,
    ) -> Result<BackfillPage, BackfillError>;

    /// Read source bytes by the enumerated physical key.
    async fn read_source(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
    ) -> Result<Vec<u8>, BackfillError>;

    /// Build the complete tenant/region-scoped target R2 key. The returned key
    /// must place [`generation_qualified_suffix`] below the same secret-derived
    /// tenant namespace used by the live data plane.
    async fn target_physical_key(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        logical_key: &str,
    ) -> Result<String, BackfillError>;

    /// Idempotently PUT ciphertext. Existing unequal bytes must fail closed.
    async fn stage(
        &self,
        run: &BackfillRun,
        staged: &StagedBackfillObject,
    ) -> Result<BackfillAllocation, BackfillError>;

    /// Atomically publish a target catalog row and advance the checkpoint.
    async fn checkpoint(
        &self,
        run: &BackfillRun,
        checkpoint: &BackfillCheckpoint,
    ) -> Result<(), BackfillError>;

    /// Atomically verify completeness, switch generation, mark active, and
    /// release the exact transition token.
    async fn commit_generation(&self, run: &BackfillRun) -> Result<(), BackfillError>;

    /// Mark the run aborted and release only its exact transition token.
    async fn abort(&self, run: &BackfillRun, reason: &str) -> Result<(), BackfillError>;
}

/// Encryption boundary. Production implementations resolve the pinned BYOK
/// policy/TCS under the same transition; tests can use deterministic crypto.
#[async_trait]
pub trait ByokBackfillEncryptor: Send + Sync {
    /// Resolve the exact target policy/TCS snapshot and return the hardened
    /// terminal storage digest used by the live data plane.
    async fn target_hardened_digest(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        logical_key: &str,
        config_version: i64,
        tcs_version: i64,
    ) -> Result<String, BackfillError>;

    /// Resolve the immutable source policy/TCS identity and return the
    /// hardened terminal storage digest used by its published physical key.
    async fn source_hardened_digest(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
    ) -> Result<String, BackfillError>;

    /// Decode an enumerated source before target encryption. The default is
    /// intentionally narrow: only generation-zero raw plaintext is accepted;
    /// rotated ciphertext must be handled by a production decoder.
    async fn decrypt_source(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
        stored: &[u8],
    ) -> Result<Vec<u8>, BackfillError> {
        if run.source_generation != 0 {
            return Err(BackfillError::Crypto(
                "rotated source requires an exact crypto identity and decoder".to_owned(),
            ));
        }
        if !matches!(
            source.source_crypto.as_ref(),
            Some(SourceCryptoIdentity::Plaintext)
        ) {
            return Err(BackfillError::SourceChanged(
                "generation-zero source is missing its plaintext identity".to_owned(),
            ));
        }
        Ok(stored.to_vec())
    }

    /// Encrypt idempotently for `allocation_id`. Mode B implementations MUST
    /// reserve/reload the same wrapped DEK and nonce before returning bytes;
    /// generating fresh random material on retry would make crash recovery
    /// conflict with the already staged target key.
    async fn encrypt(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
        allocation_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, BackfillError>;
}

/// Crash-resumable coordinator. Durable truth always lives behind `store`.
pub struct ByokBackfillEngine<S, E> {
    store: S,
    encryptor: E,
}

impl<S, E> core::fmt::Debug for ByokBackfillEngine<S, E> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ByokBackfillEngine")
            .finish_non_exhaustive()
    }
}

impl<S, E> ByokBackfillEngine<S, E>
where
    S: ByokBackfillStore,
    E: ByokBackfillEncryptor,
{
    /// Construct an engine from its durable store and encryption boundary.
    #[must_use]
    pub const fn new(store: S, encryptor: E) -> Self {
        Self { store, encryptor }
    }

    /// Begin an exclusive migration after validating non-ambiguous inputs.
    pub async fn begin(&self, request: &BeginBackfill) -> Result<BackfillRun, BackfillError> {
        if request.tenant_id.trim().is_empty() {
            return Err(BackfillError::InvalidRequest(
                "tenant_id must be non-empty".to_owned(),
            ));
        }
        validate_lease(request.lease)?;
        let run = self.store.begin(request).await?;
        validate_run(&run, &request.tenant_id, None)?;
        if run.gate_epoch != request.expected_gate_epoch
            || run.source_generation != request.expected_source_generation
        {
            return Err(BackfillError::Store(
                "store acquired a transition for a different gate snapshot".to_owned(),
            ));
        }
        Ok(run)
    }

    /// Resume the exact durable run; callers never reconstruct cursors locally.
    pub async fn resume(
        &self,
        tenant_id: &str,
        run_id: &str,
        previous_token: &str,
        lease: Duration,
    ) -> Result<BackfillRun, BackfillError> {
        validate_lease(lease)?;
        let run = self
            .store
            .resume(tenant_id, run_id, previous_token, lease)
            .await?;
        validate_run(&run, tenant_id, None)?;
        if run.run_id != run_id {
            return Err(BackfillError::Store(
                "store resumed a different stable run identity".to_owned(),
            ));
        }
        Ok(run)
    }

    /// Process at most one bounded page and durably checkpoint every object.
    /// A page can be retried after any crash without exposing partial results.
    pub async fn copy_page(
        &self,
        run: &mut BackfillRun,
        surface: BackfillSurface,
        limit: usize,
        lease: Duration,
    ) -> Result<usize, BackfillError> {
        if limit == 0 {
            return Err(BackfillError::InvalidRequest(
                "page limit must be greater than zero".to_owned(),
            ));
        }
        validate_lease(lease)?;
        if run.phase != BackfillPhase::Copying {
            return Err(BackfillError::Conflict(
                "copy requires a copying run".to_owned(),
            ));
        }
        if run.complete(surface) {
            return Ok(0);
        }
        self.store.renew(run, lease).await?;
        let page = self
            .store
            .enumerate(run, surface, run.cursor(surface), limit)
            .await?;
        if page.objects.len() > limit {
            return Err(BackfillError::Store(
                "adapter returned more objects than the requested page limit".to_owned(),
            ));
        }
        for source in &page.objects {
            if source.surface != surface {
                return Err(BackfillError::Store(
                    "adapter returned an object from the wrong surface".to_owned(),
                ));
            }
            // Legacy plaintext must not remain in an allocator-owned Vec after
            // this iteration; ciphertext is the only durable output.
            let stored = Zeroizing::new(self.store.read_source(run, source).await?);
            let plaintext =
                Zeroizing::new(self.encryptor.decrypt_source(run, source, &stored).await?);
            let allocation_id = allocation_id(run, surface, &source.logical_key);
            let ciphertext = self
                .encryptor
                .encrypt(run, source, &allocation_id, &plaintext)
                .await?;
            let target_physical_key = self
                .store
                .target_physical_key(run, surface, &source.logical_key)
                .await?;
            let staged = StagedBackfillObject {
                allocation_id,
                surface,
                logical_key: source.logical_key.clone(),
                target_generation: run.target_generation,
                target_physical_key,
                plaintext_len: u64::try_from(plaintext.len()).map_err(|_| {
                    BackfillError::Store(
                        "plaintext length does not fit catalog metadata".to_owned(),
                    )
                })?,
                ciphertext,
            };
            let allocation = self.store.stage(run, &staged).await?;
            validate_allocation(&staged, &allocation)?;
            let checkpoint = BackfillCheckpoint {
                tenant_id: run.tenant_id.clone(),
                run_token: run.run_token.clone(),
                transition_epoch: run.transition_epoch,
                gate_epoch: run.gate_epoch,
                target_generation: run.target_generation,
                surface,
                allocation: Some(allocation),
                // Do not advance an opaque page cursor mid-page. A crash here
                // replays the page, and allocation/catalog idempotency absorbs
                // objects already published before the crash.
                next_cursor: run.cursor(surface).map(str::to_owned),
                surface_complete: false,
            };
            self.store.checkpoint(run, &checkpoint).await?;
            run.apply_checkpoint(&checkpoint);
        }
        let surface_complete = page.next_cursor.is_none();
        let checkpoint = BackfillCheckpoint {
            tenant_id: run.tenant_id.clone(),
            run_token: run.run_token.clone(),
            transition_epoch: run.transition_epoch,
            gate_epoch: run.gate_epoch,
            target_generation: run.target_generation,
            surface,
            allocation: None,
            next_cursor: page.next_cursor,
            surface_complete,
        };
        self.store.checkpoint(run, &checkpoint).await?;
        run.apply_checkpoint(&checkpoint);
        Ok(page.objects.len())
    }

    /// Publish only a fully checkpointed target generation.
    pub async fn commit(&self, run: &mut BackfillRun) -> Result<(), BackfillError> {
        if run.phase != BackfillPhase::ReadyToCommit || !run.cas_complete || !run.ac_complete {
            return Err(BackfillError::Conflict(
                "CAS and AC must both be complete before activation".to_owned(),
            ));
        }
        self.store.commit_generation(run).await?;
        run.phase = BackfillPhase::Committed;
        Ok(())
    }

    /// Abort without changing `current_generation`; staged bytes stay invisible.
    pub async fn abort(&self, run: &mut BackfillRun, reason: &str) -> Result<(), BackfillError> {
        if reason.trim().is_empty() {
            return Err(BackfillError::InvalidRequest(
                "abort reason must be non-empty".to_owned(),
            ));
        }
        if matches!(run.phase, BackfillPhase::Committed | BackfillPhase::Aborted) {
            return Err(BackfillError::Conflict(
                "terminal backfill run cannot be aborted".to_owned(),
            ));
        }
        self.store.abort(run, reason).await?;
        run.phase = BackfillPhase::Aborted;
        Ok(())
    }
}

fn validate_run(
    run: &BackfillRun,
    tenant_id: &str,
    run_token: Option<&str>,
) -> Result<(), BackfillError> {
    if run.tenant_id != tenant_id
        || run.run_id.trim().is_empty()
        || run.run_token.trim().is_empty()
        || run_token.is_some_and(|expected| run.run_token != expected)
    {
        return Err(BackfillError::Store(
            "adapter returned a run for a different tenant or token".to_owned(),
        ));
    }
    if run.target_generation != run.source_generation.saturating_add(1)
        || run.target_generation <= 0
        || run.transition_epoch <= 0
        || run.gate_epoch <= 0
    {
        return Err(BackfillError::Store(
            "target generation must be exactly source generation + 1".to_owned(),
        ));
    }
    Ok(())
}

fn validate_lease(lease: Duration) -> Result<(), BackfillError> {
    if lease < Duration::from_secs(1) || lease > MAX_BYOK_FENCE_LEASE {
        return Err(BackfillError::InvalidRequest(format!(
            "lease must be in 1s..={MAX_BYOK_FENCE_LEASE:?}"
        )));
    }
    Ok(())
}

fn allocation_id(run: &BackfillRun, surface: BackfillSurface, logical_key: &str) -> String {
    let identity = format!(
        "{}\0{}\0{}\0{}\0{}",
        run.tenant_id,
        run.run_id,
        run.target_generation,
        surface.as_str(),
        logical_key
    );
    hex::encode(blake3::hash(identity.as_bytes()).as_bytes())
}

fn validate_allocation(
    staged: &StagedBackfillObject,
    allocation: &BackfillAllocation,
) -> Result<(), BackfillError> {
    let expected_len = u64::try_from(staged.ciphertext.len()).map_err(|_| {
        BackfillError::Store("ciphertext length does not fit catalog metadata".to_owned())
    })?;
    let expected_hash = hex::encode(blake3::hash(&staged.ciphertext).as_bytes());
    if allocation.allocation_id != staged.allocation_id
        || allocation.surface != staged.surface
        || allocation.logical_key != staged.logical_key
        || allocation.target_generation != staged.target_generation
        || allocation.target_physical_key != staged.target_physical_key
        || allocation.plaintext_len != staged.plaintext_len
        || allocation.ciphertext_len != expected_len
        || allocation.ciphertext_blake3 != expected_hash
    {
        return Err(BackfillError::Store(
            "stage receipt does not match ciphertext PUT".to_owned(),
        ));
    }
    Ok(())
}

/// Deterministic physical key below the already tenant-scoped R2 prefix.
/// Hex encoding makes arbitrary logical identifiers path-segment safe.
#[must_use]
pub fn generation_qualified_suffix(
    surface: BackfillSurface,
    generation: i64,
    logical_key: &str,
) -> String {
    format!(
        "byok/{}/g{generation:020}/{}",
        surface.as_str(),
        hex::encode(logical_key.as_bytes())
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests;
