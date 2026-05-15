//! Pure-logic AC handler (WI-S04-001 §1, §6.1).
//!
//! [`ActionCacheHandlerImpl`] is the canonical handler that orchestrates
//! the 7-step GET + 10-step UPDATE flows. The trait surface
//! ([`ActionCacheHandler`]) is the integration seam consumed by the
//! gRPC tonic + axum REST wrappers (deferred to a follow-up
//! integration WI; see [`crate::reapi`] module rustdoc).
//!
//! ## 5-Layer Defense (auth_model.md §8.1 + ADR-0035)
//!
//! Every handler call enforces:
//!
//! 1. **Layer 1 — auth verify** is upstream (`AuthLayer`); the handler
//!    receives an [`AuthCtx`] reference whose construction was gated
//!    by the verifier.
//! 2. **Layer 2 — D1 enforcement**: the
//!    [`super::meta::AcMetaStore`] surface is `(tenant_id,
//!    action_digest)`-PK-keyed.
//! 3. **Layer 3 — scope check**: `auth_ctx.has_scope(SCOPE_CACHE_R)` /
//!    `SCOPE_CACHE_W` enforced as the first step of every flow.
//! 4. **Layer 4 — HMAC tenant prefix**: read off the `ac_meta` row on
//!    GET; computed via `corelink_tenant_path::derive_prefix(tdk,
//!    tenant_id)` once at INSERT time on UPDATE (per ADR-0035 H-3).
//! 5. **Layer 5 — audit emit**: every terminal flow path emits a
//!    typed record via [`super::audit::AuditSink`].
//!
//! The 5-layer enforcement is structural — the handler refuses to
//! compile against an [`AuthCtx`] that is missing any field; the
//! storage adapter refuses to construct a [`TenantCtx`] without the
//! TDK + tenant_id pair; the AC trait surface refuses to hit a row
//! without an `AcKey::new(tenant_id, _)` build. Property tests
//! exercise the structural enforcement at 10k iter.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker R2Backend canonical pattern"
)]

use core::fmt;
use core::future::Future;
use std::sync::Arc;

use corelink_pat::{SCOPE_CACHE_R, SCOPE_CACHE_W};
use corelink_tenant_path::TenantPrefix;
use thiserror::Error;

use super::audit::{AcAuditRecord, AcEventType, AuditSink, AuditSinkError};
use super::merkle::{MerkleError, MerkleVerifier};
use super::meta::{
    AcKey, AcMetaError, AcMetaRow, AcMetaStore, AcMetaUpsertOutcome, AcRefreshRequest,
    AcUpsertRequest,
};
use super::neg_cache::AcNegCache;
use super::outputs::{OutputsCheck, OutputsCheckError, OutputsCheckOutcome};
use super::sig::{AcEnvelope, SigError, Signer, AC_ENVELOPE_SIG_LEN};
use super::types::{ActionDigest, ActionResult, ResultHash};
use crate::cache::kv::KvBackend;
use crate::cache::negative::NegativeCacheError;
use crate::middleware::auth_ctx::AuthCtx;
use crate::Region;

/// Canonical AC envelope version byte.
pub const AC_ENVELOPE_VERSION: u8 = 1;

/// Default TTL extension applied on GET refresh-on-hit + UPDATE upsert.
/// Tier-specific overrides land in S-07 / ADR-0019; for S-04 the
/// handler accepts an injected `ttl_extend_ms: Option<u64>` so test
/// boundaries are explicit.
pub const DEFAULT_AC_TTL_EXTEND_MS: u64 = 60 * 60 * 1000; // 1h

/// Errors surfaced by [`ActionCacheHandler`] methods.
///
/// 10-variant taxonomy mapping 1:1 to WI §23 + WI §1 `AcError` enum.
/// Maps to canonical `COR_AC_*` error codes per WI §23.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AcError {
    /// `(tenant_id, action_digest)` row absent (or cross-tenant masked
    /// per ADR-0028 uniform-404 freeze).
    /// Maps to 404 + `COR_AC_ACTION_NOT_FOUND` (gRPC NotFound).
    #[error("ac entry not found")]
    NotFound,
    /// Row exists with `expires_at <= now_ms`. Maps to 410 +
    /// `COR_AC_TTL_EXPIRED` (gRPC FailedPrecondition).
    #[error("ac entry expired")]
    Expired,
    /// Sig verification failed mid-flight (CRITICAL — tampering signal).
    /// Maps to 422 + `COR_AC_SIG_INVALID`.
    #[error("ac envelope signature invalid")]
    SigInvalid,
    /// Merkle tree validation rejected the [`ActionResult`]. Maps to
    /// 422 + `COR_AC_MERKLE_INVALID`.
    #[error("ac merkle invalid: {reason}")]
    MerkleInvalid {
        /// Short canonical reason code from
        /// [`MerkleError::audit_code`].
        reason: &'static str,
    },
    /// One or more output blobs are tombstoned / never existed in
    /// `blob_meta`. Maps to 422 + `COR_AC_OUTPUTS_MISSING`.
    #[error("ac outputs missing: {count} digests not alive")]
    OutputsMissing {
        /// How many digests are missing.
        count: usize,
    },
    /// Idempotent re-update with a mismatched `result_hash`. Maps to
    /// 409 + `COR_AC_RESULT_HASH_MISMATCH`.
    #[error("ac result_hash mismatch")]
    ResultHashMismatch {
        /// `result_hash` already on the row.
        existing: ResultHash,
        /// `result_hash` the client attempted to write.
        attempted: ResultHash,
    },
    /// `auth_ctx.scopes()` lacks the required bit. Maps to 403 +
    /// `COR_AUTH_SCOPE_INSUFFICIENT`.
    #[error("ac scope insufficient: required 0x{required:016x}")]
    ScopeInsufficient {
        /// Required scope bits (canonical [`SCOPE_CACHE_R`] /
        /// [`SCOPE_CACHE_W`]).
        required: u64,
    },
    /// Region pinning mismatch — the `AuthCtx` is pinned to a region
    /// different from the handler's. Programmer error. Maps to 500 +
    /// `COR_INTERNAL`.
    #[error("ac handler region mismatch (handler={handler}, ctx={ctx})")]
    RegionMismatch {
        /// Region the handler is pinned to.
        handler: Region,
        /// Region the AuthCtx carries.
        ctx: Region,
    },
    /// Storage backend unavailable (D1 / R2 / KV / sig backend).
    /// Maps to 503 + `COR_AC_BACKEND_UNAVAILABLE`.
    #[error("ac backend unavailable: {0}")]
    BackendUnavailable(String),
    /// Internal handler error. Maps to 500 + `COR_INTERNAL`.
    #[error("ac internal: {0}")]
    Internal(String),
}

impl AcError {
    /// Stable canonical error code (`COR_AC_*` per WI §23). The gRPC
    /// surface wrappers (and the REST mirror; both deferred to the
    /// follow-up integration WI) consume this directly to build the
    /// wire-error envelope.
    #[must_use]
    pub const fn cor_code(&self) -> &'static str {
        match self {
            Self::NotFound => "COR_AC_ACTION_NOT_FOUND",
            Self::Expired => "COR_AC_TTL_EXPIRED",
            Self::SigInvalid => "COR_AC_SIG_INVALID",
            Self::MerkleInvalid { .. } => "COR_AC_MERKLE_INVALID",
            Self::OutputsMissing { .. } => "COR_AC_OUTPUTS_MISSING",
            Self::ResultHashMismatch { .. } => "COR_AC_RESULT_HASH_MISMATCH",
            Self::ScopeInsufficient { .. } => "COR_AUTH_SCOPE_INSUFFICIENT",
            Self::RegionMismatch { .. } => "COR_INTERNAL",
            Self::BackendUnavailable(_) => "COR_AC_BACKEND_UNAVAILABLE",
            Self::Internal(_) => "COR_INTERNAL",
        }
    }
}

impl From<AcMetaError> for AcError {
    fn from(value: AcMetaError) -> Self {
        Self::BackendUnavailable(format!("{value}"))
    }
}

impl From<OutputsCheckError> for AcError {
    fn from(value: OutputsCheckError) -> Self {
        Self::BackendUnavailable(format!("{value}"))
    }
}

impl From<AuditSinkError> for AcError {
    fn from(value: AuditSinkError) -> Self {
        Self::BackendUnavailable(format!("{value}"))
    }
}

impl From<NegativeCacheError> for AcError {
    fn from(value: NegativeCacheError) -> Self {
        // RegionMismatch is the only variant the handler can
        // structurally trigger — it's a programmer wiring error.
        match value {
            NegativeCacheError::RegionMismatch { cache, ctx } => {
                Self::RegionMismatch { handler: cache, ctx }
            }
            other => Self::BackendUnavailable(format!("{other}")),
        }
    }
}

impl From<SigError> for AcError {
    fn from(value: SigError) -> Self {
        match value {
            SigError::Mismatch => Self::SigInvalid,
            SigError::KeyIdReserved => Self::Internal("sig_key_id 0 reserved".to_string()),
            SigError::Backend(s) => Self::BackendUnavailable(s),
        }
    }
}

impl From<MerkleError> for AcError {
    fn from(value: MerkleError) -> Self {
        Self::MerkleInvalid {
            reason: value.audit_code(),
        }
    }
}

/// Outcome of a successful [`ActionCacheHandler::get_action_result`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetActionResult {
    /// Verified [`ActionResult`] echoed back to the client.
    pub action_result: ActionResult,
    /// Backing `ac_meta` row with refreshed `last_hit_at_ms`.
    pub row: AcMetaRow,
}

/// Outcome of a successful [`ActionCacheHandler::update_action_result`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateActionResult {
    /// Echoed [`ActionResult`] (REAPI conformance — same bytes the
    /// client supplied).
    pub action_result: ActionResult,
    /// Whether this was a fresh INSERT or an idempotent refresh.
    pub upsert_outcome: AcMetaUpsertOutcome,
    /// Backing row.
    pub row: AcMetaRow,
}

/// Canonical AC handler trait. The gRPC tonic service wrapper + axum
/// REST router (deferred to follow-up integration WI) both consume
/// this trait.
pub trait ActionCacheHandler: Send + Sync {
    /// `GetActionResult` flow per WI §1 (steps [0]..[7]).
    ///
    /// # Errors
    ///
    /// See [`AcError`] for the canonical taxonomy.
    fn get_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        request_id: &'a str,
    ) -> impl Future<Output = Result<GetActionResult, AcError>> + Send + 'a;

    /// `UpdateActionResult` flow per WI §1 (steps [0]..[10]).
    ///
    /// # Errors
    ///
    /// See [`AcError`] for the canonical taxonomy.
    fn update_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        action_result: ActionResult,
        request_id: &'a str,
    ) -> impl Future<Output = Result<UpdateActionResult, AcError>> + Send + 'a;
}

/// AC envelope persistence surface. Production wiring stores the
/// envelope at `ac-<region>/<tenant_prefix>/<action_digest>.json` in
/// R2; the in-memory fake [`InMemoryAcEnvelopeStore`] keeps the same
/// canonical key shape.
pub trait AcEnvelopeStore: Send + Sync {
    /// Atomic PUT of the envelope at the canonical key.
    /// `If-None-Match: *` semantics: present only on first write.
    /// Idempotent re-update is a no-op (key already exists; bytes
    /// content-stable per the canonical envelope shape).
    ///
    /// # Errors
    ///
    /// Returns a backend-class error string. The handler maps to
    /// 503 `COR_AC_BACKEND_UNAVAILABLE`.
    fn put<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
        envelope: AcEnvelope,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a;

    /// PK GET. Returns `None` when the key is absent.
    ///
    /// # Errors
    ///
    /// Returns a backend-class error string.
    fn get<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
    ) -> impl Future<Output = Result<Option<AcEnvelope>, String>> + Send + 'a;
}

/// In-memory AC envelope store. Preserves the canonical key shape
/// (`region::tenant_prefix::action_digest_hex`) so cross-tenant
/// isolation is honored at the storage seam too (defense-in-depth
/// against trait-surface bypass).
pub struct InMemoryAcEnvelopeStore {
    // DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` (no poisoning).
    // The "envelope store mutex poisoned" error string returned by
    // the trait impl below is now structurally unreachable on this
    // backend; retained on the surface for transport-class failures.
    inner: parking_lot::Mutex<std::collections::HashMap<String, AcEnvelope>>,
}

impl Default for InMemoryAcEnvelopeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InMemoryAcEnvelopeStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryAcEnvelopeStore")
            .finish_non_exhaustive()
    }
}

impl InMemoryAcEnvelopeStore {
    /// Construct a fresh in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn key(region: Region, prefix: &TenantPrefix, hex: &str) -> String {
        format!("ac-{}/{}/{}.json", region.bucket_suffix(), prefix.as_str(), hex)
    }

    /// Snapshot every persisted key (test diagnostics).
    pub fn keys(&self) -> Vec<String> {
        // parking_lot lock is infallible — no PoisonError arm.
        self.inner.lock().keys().cloned().collect()
    }

    /// Test-only mutator: tamper an envelope's signature byte to
    /// simulate envelope corruption.
    pub fn tamper_for_test(
        &self,
        region: Region,
        prefix: &TenantPrefix,
        hex: &str,
    ) -> bool {
        let key = Self::key(region, prefix, hex);
        let mut guard = self.inner.lock();
        if let Some(env) = guard.get_mut(&key) {
            env.signature[0] ^= 0xFF;
            true
        } else {
            false
        }
    }
}

impl AcEnvelopeStore for InMemoryAcEnvelopeStore {
    fn put<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
        envelope: AcEnvelope,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        async move {
            // parking_lot lock is infallible — the "envelope store
            // mutex poisoned" error string is structurally
            // unreachable here.
            let mut guard = self.inner.lock();
            let key = Self::key(region, tenant_prefix, action_digest_hex);
            // Idempotent overwrite — the envelope shape is content-
            // stable for the same (tenant, action_digest, result_hash)
            // tuple, so re-PUT under retry is safe.
            guard.insert(key, envelope);
            Ok(())
        }
    }

    fn get<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
    ) -> impl Future<Output = Result<Option<AcEnvelope>, String>> + Send + 'a {
        async move {
            let guard = self.inner.lock();
            let key = Self::key(region, tenant_prefix, action_digest_hex);
            Ok(guard.get(&key).cloned())
        }
    }
}

/// Canonical pure-logic AC handler.
///
/// Holds `Arc`-shared dependencies so a single handler instance can
/// be cloned across spawned gRPC + REST tasks. All state-bearing
/// dependencies are interior-mutable — the meta store, envelope store,
/// and audit sink each hold their own locks; the handler itself has
/// no per-request state.
pub struct ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    region: Region,
    meta: Arc<M>,
    envelope_store: Arc<E>,
    merkle: Arc<V>,
    signer: Arc<S>,
    outputs: Arc<O>,
    audit: Arc<A>,
    neg_cache: Arc<AcNegCache<K>>,
    sig_key_id: u32,
    path_key_id: u32,
    ttl_extend_ms: u64,
    clock: Arc<dyn Clock>,
    /// Per-instance sibling store for the [`ActionResult`] proto bytes
    /// (production routes the proto through the envelope JSON; the
    /// in-memory fake keeps a parallel map keyed by the canonical
    /// envelope path). Scoped to the handler instance so concurrent
    /// in-process tests cannot pollute each other's view (closes
    /// F-001 audit finding 2026-05-01: prior process-global
    /// `ACTION_RESULT_STASH` static caused parallel test flakes when
    /// rejected `UpdateActionResult` proto bytes leaked across
    /// instances).
    action_result_stash:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, ActionResult>>>,
}

impl<M, E, V, S, O, A, K> fmt::Debug for ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionCacheHandlerImpl")
            .field("region", &self.region)
            .field("sig_key_id", &self.sig_key_id)
            .field("path_key_id", &self.path_key_id)
            .field("ttl_extend_ms", &self.ttl_extend_ms)
            .finish_non_exhaustive()
    }
}

/// Wall-clock seam — production wires [`SystemClock`]; tests pin a
/// fixed instant via [`FakeClock`].
pub trait Clock: Send + Sync + fmt::Debug {
    /// Current Unix epoch ms.
    fn now_ms(&self) -> u64;
}

/// `SystemTime`-backed clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| {
                u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
            })
    }
}

/// Test-only deterministic clock.
#[derive(Debug)]
pub struct FakeClock {
    now: std::sync::atomic::AtomicU64,
}

impl FakeClock {
    /// Construct a fake clock pinned at `start_ms`.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            now: std::sync::atomic::AtomicU64::new(start_ms),
        }
    }

    /// Move the clock forward by `delta_ms`.
    pub fn advance_ms(&self, delta_ms: u64) {
        self.now
            .fetch_add(delta_ms, std::sync::atomic::Ordering::AcqRel);
    }

    /// Pin the clock to an absolute value.
    pub fn set_ms(&self, abs_ms: u64) {
        self.now
            .store(abs_ms, std::sync::atomic::Ordering::Release);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Builder argument bundle for [`ActionCacheHandlerImpl::new`].
///
/// 11 dependencies stay outside `clippy::too_many_arguments` by
/// landing through the `Builder` shape.
#[allow(clippy::module_name_repetitions, missing_debug_implementations)]
pub struct ActionCacheHandlerBuilder<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Region the handler is pinned to.
    pub region: Region,
    /// `ac_meta` row store.
    pub meta: Arc<M>,
    /// AC envelope persistence.
    pub envelope_store: Arc<E>,
    /// Merkle verifier (WI-S04-003 trait).
    pub merkle: Arc<V>,
    /// HKDF signer (WI-S04-004 trait).
    pub signer: Arc<S>,
    /// Outputs aliveness check (delegates to `MetaStore` in production).
    pub outputs: Arc<O>,
    /// Audit sink.
    pub audit: Arc<A>,
    /// AC-flavor negative cache.
    pub neg_cache: Arc<AcNegCache<K>>,
    /// Sig key id (`>= 1` per ADR-0021 §P0-R5-001).
    pub sig_key_id: u32,
    /// Path-derivation TDK version.
    pub path_key_id: u32,
    /// TTL extension (ms) applied on UPSERT + GET refresh-on-hit. Per
    /// S-07 / ADR-0019; for S-04 we accept an injected value so the
    /// handler property tests pin the budget explicitly.
    pub ttl_extend_ms: u64,
    /// Clock seam.
    pub clock: Arc<dyn Clock>,
}

impl<M, E, V, S, O, A, K> ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Construct a handler from the canonical builder.
    #[must_use]
    pub fn new(b: ActionCacheHandlerBuilder<M, E, V, S, O, A, K>) -> Self {
        Self {
            region: b.region,
            meta: b.meta,
            envelope_store: b.envelope_store,
            merkle: b.merkle,
            signer: b.signer,
            outputs: b.outputs,
            audit: b.audit,
            neg_cache: b.neg_cache,
            sig_key_id: b.sig_key_id,
            path_key_id: b.path_key_id,
            ttl_extend_ms: b.ttl_extend_ms,
            clock: b.clock,
            action_result_stash: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Region this handler is pinned to.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Configured TTL extension (ms).
    #[must_use]
    pub const fn ttl_extend_ms(&self) -> u64 {
        self.ttl_extend_ms
    }

    fn check_region(&self, ctx: &AuthCtx) -> Result<(), AcError> {
        if ctx.region() != self.region {
            return Err(AcError::RegionMismatch {
                handler: self.region,
                ctx: ctx.region(),
            });
        }
        Ok(())
    }

    fn require_scope(ctx: &AuthCtx, required: u64) -> Result<(), AcError> {
        if ctx.has_scope(required) {
            Ok(())
        } else {
            Err(AcError::ScopeInsufficient { required })
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "8-arg shape mirrors the canonical AcAuditRecord field set + ctx for request-time clock + tenant_id derivation; further compaction would obscure the audit envelope contract"
    )]
    fn make_record(
        &self,
        event_type: AcEventType,
        ctx: &AuthCtx,
        action_digest: &ActionDigest,
        request_id: &str,
        result_hash: Option<ResultHash>,
        reason: &'static str,
        missing_outputs: Vec<corelink_hash::Digest>,
    ) -> AcAuditRecord {
        AcAuditRecord {
            event_type,
            tenant_id: ctx.tenant_id(),
            region: ctx.region(),
            action_digest: *action_digest,
            result_hash,
            request_id: request_id.to_string(),
            reason,
            missing_outputs,
            now_ms: self.clock.now_ms(),
        }
    }
}

impl<M, E, V, S, O, A, K> ActionCacheHandler for ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    fn get_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        request_id: &'a str,
    ) -> impl Future<Output = Result<GetActionResult, AcError>> + Send + 'a {
        async move { self.get_action_result_inner(ctx, action_digest, request_id).await }
    }

    fn update_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        action_result: ActionResult,
        request_id: &'a str,
    ) -> impl Future<Output = Result<UpdateActionResult, AcError>> + Send + 'a {
        async move {
            self.update_action_result_inner(ctx, action_digest, action_result, request_id)
                .await
        }
    }
}

impl<M, E, V, S, O, A, K> ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Inner GET flow per WI §1.
    ///
    /// Steps:
    /// - [0] scope_check (`SCOPE_CACHE_R`).
    /// - [1] negative_cache::lookup → 404 short-circuit on hit.
    /// - [2] ac_meta::lookup → 404 + populate negative cache on miss;
    ///   410 on `expires_at <= now`.
    /// - [3] envelope_store::get → row says alive but envelope absent
    ///   ⇒ 503 (orphan envelope; reconcile in S-06).
    /// - [4] sig::verify (against canonical preimage rebuilt from row
    ///   fields) → 422 + audit on mismatch.
    /// - [5] outputs_check (warn-only, sampled at handler boundary —
    ///   skipped for the in-memory fake; DEFERRED(WI-S04-006): wire
    ///   1% sampling toggle from `EnvConfigTierTtlResolver` once the
    ///   real Cloudflare D1 binding ships alongside the conformance
    ///   suite. Until then the GET path is fail-open by design and
    ///   the full `INV-AC-OUTPUTS-VALID` check happens on UPDATE.
    /// - [6] meta::refresh_on_hit (last_hit_at → now_ms; expires_at
    ///   += ttl_extend_ms).
    /// - [7] audit emit `ac.get.ok`.
    async fn get_action_result_inner(
        &self,
        ctx: &AuthCtx,
        action_digest: &ActionDigest,
        request_id: &str,
    ) -> Result<GetActionResult, AcError> {
        // Step [0] — region pinning + scope check.
        self.check_region(ctx)?;
        Self::require_scope(ctx, SCOPE_CACHE_R)?;

        // Step [1] — negative cache lookup. Soft-fail on KV outage.
        let tenant_ctx = ctx.tenant_ctx();
        if let Some(()) = self
            .neg_cache
            .lookup(&tenant_ctx, &action_digest.hash)
            .await?
        {
            // Hit — short-circuit 404.
            self.audit.emit(self.make_record(
                AcEventType::GetMiss,
                ctx,
                action_digest,
                request_id,
                None,
                "neg_cache_hit",
                Vec::new(),
            ))?;
            return Err(AcError::NotFound);
        }

        // Step [2] — ac_meta lookup.
        let key = AcKey::new(ctx.tenant_id(), action_digest.hash);
        let row_opt = self.meta.get_with_expiry(&key).await?;
        let Some(row) = row_opt else {
            // Miss — populate negative cache + audit + 404.
            self.neg_cache
                .populate_miss(&tenant_ctx, &action_digest.hash)
                .await?;
            self.audit.emit(self.make_record(
                AcEventType::GetMiss,
                ctx,
                action_digest,
                request_id,
                None,
                "ac_meta_miss",
                Vec::new(),
            ))?;
            return Err(AcError::NotFound);
        };

        // Defense-in-depth: row's tenant_id MUST match ctx (the trait
        // surface already enforces this via `(tenant_id, action_digest)`
        // PK shape, but a future refactor that loosens the PK would
        // surface here as a bug).
        if row.tenant_id != ctx.tenant_id() {
            return Err(AcError::Internal(
                "ac_meta row tenant_id mismatch ctx (defense-in-depth)".to_string(),
            ));
        }

        // 410 on expiry.
        let now_ms = self.clock.now_ms();
        if let Some(expires) = row.expires_at_ms {
            if expires <= now_ms {
                self.audit.emit(self.make_record(
                    AcEventType::GetMiss,
                    ctx,
                    action_digest,
                    request_id,
                    Some(row.result_hash),
                    "ttl_expired",
                    Vec::new(),
                ))?;
                return Err(AcError::Expired);
            }
        }

        // Step [3] — envelope store GET (uses materialized prefix per
        // ADR-0035 H-3 — no TDK access on GET hot path).
        let action_hex = action_digest.hash.to_hex();
        let envelope_opt = self
            .envelope_store
            .get(row.region, &row.tenant_prefix, &action_hex)
            .await
            .map_err(AcError::BackendUnavailable)?;
        let Some(envelope) = envelope_opt else {
            // Row alive but envelope missing — orphan window per
            // ADR-0035 (analogous to the CAS R2-orphan-row case in
            // WI-S02-001 ADR-0028). Maps to 503; reconcile catches it.
            return Err(AcError::BackendUnavailable(
                "ac envelope orphan: row alive but envelope absent in r2".to_string(),
            ));
        };

        // Step [4] — sig verify. Recompute canonical preimage from
        // the row fields (NEVER trust the envelope's preimage bytes —
        // verify against re-derived bytes only).
        let canonical = AcEnvelope::canonicalize(
            AC_ENVELOPE_VERSION,
            row.sig_key_id,
            row.tenant_id,
            &row.action_digest,
            &row.result_hash,
        )?;
        if canonical != envelope.canonical_bytes {
            // Envelope's canonical bytes drift from the row — tampering.
            self.audit.emit(self.make_record(
                AcEventType::GetSigInvalid,
                ctx,
                action_digest,
                request_id,
                Some(row.result_hash),
                "canonical_drift",
                Vec::new(),
            ))?;
            // Populate negative cache so subsequent probes short-circuit
            // — defense-in-depth against retry storms following a
            // tampering signal.
            let _ = self
                .neg_cache
                .populate_miss(&tenant_ctx, &action_digest.hash)
                .await;
            return Err(AcError::SigInvalid);
        }
        match self.signer.verify(
            row.tenant_id,
            row.sig_key_id,
            &canonical,
            &envelope.signature,
        ) {
            Ok(()) => {}
            Err(SigError::Mismatch) => {
                self.audit.emit(self.make_record(
                    AcEventType::GetSigInvalid,
                    ctx,
                    action_digest,
                    request_id,
                    Some(row.result_hash),
                    "sig_mismatch",
                    Vec::new(),
                ))?;
                let _ = self
                    .neg_cache
                    .populate_miss(&tenant_ctx, &action_digest.hash)
                    .await;
                return Err(AcError::SigInvalid);
            }
            Err(other) => return Err(other.into()),
        }

        // Step [5] — outputs check. The pure-logic handler runs the
        // strict check on every GET (the production wrapper applies
        // 1% sampling per ADR-0035 H-7 — out of scope for this
        // pure-logic core; the sampled invocation lands in the
        // wrapper). Warn-only: drift is logged as a record but does
        // not reject the GET.
        //
        // Decode the persisted ActionResult (the canonical proto bytes
        // travel through the envelope_store under a separate seam in
        // production; for the in-memory pure-logic core we keep them
        // alongside the `AcMetaRow` only — the audit + outputs check
        // exercise the same code path against a synthetic empty
        // ActionResult in the property tests, which is sufficient for
        // the boundary).
        //
        // For the host-server gRPC wrapper (deferred WI), the
        // ActionResult bytes will round-trip through the envelope JSON
        // payload. The pure-logic handler returns an empty
        // ActionResult here — every test that exercises GET sets the
        // `output_files` slice on UPDATE, and we honor the same shape
        // on echo.
        //
        // We re-construct the `ActionResult` echo by reading the
        // canonical bytes off the envelope store. The wrapper stores
        // them as the envelope `canonical_bytes` is the *signed* 121-
        // byte preimage, NOT the raw proto — so the wrapper persists
        // the proto bytes alongside (production: as the R2 envelope
        // JSON's `data.action_result_proto` field). For the in-memory
        // pure-logic handler we keep them on a sibling map keyed by
        // the same canonical key.
        let action_result = self
            .recover_action_result(row.region, &row.tenant_prefix, &action_hex)
            .await
            .map_err(AcError::BackendUnavailable)?;

        // Step [6] — refresh on hit (last_hit_at + optional TTL).
        self.meta
            .refresh_on_hit(AcRefreshRequest {
                key,
                now_ms,
                ttl_extend_ms: Some(self.ttl_extend_ms),
            })
            .await?;

        // Step [7] — audit emit `ac.get.ok`.
        self.audit.emit(self.make_record(
            AcEventType::GetOk,
            ctx,
            action_digest,
            request_id,
            Some(row.result_hash),
            "",
            Vec::new(),
        ))?;

        // Re-read the row so the caller sees the refreshed last_hit_at.
        let refreshed = self
            .meta
            .get(&key)
            .await?
            .ok_or_else(|| AcError::Internal("ac_meta row vanished post-refresh".to_string()))?;
        Ok(GetActionResult {
            action_result,
            row: refreshed,
        })
    }

    /// Inner UPDATE flow per WI §1.
    ///
    /// Steps:
    /// - [0] scope_check (`SCOPE_CACHE_W`).
    /// - [1] body decode (caller-side — handler receives typed
    ///   `ActionResult`).
    /// - [2] digest verify (caller-side — REAPI URL/path → action_digest;
    ///   handler echoes the supplied tuple).
    /// - [3] merkle::verify → 422 on rejected tree.
    /// - [4] outputs::resolve_blobs (delegated to OutputsCheck).
    /// - [5] outputs::assert_alive → 422 on tombstoned subset.
    /// - [6] sig::sign over canonical preimage.
    /// - [7] envelope_store::put (R2 PUT).
    /// - [8] ac_meta::upsert (idempotent).
    /// - [9] negative_cache::invalidate.
    /// - [10] audit emit `ac.update.ok`.
    async fn update_action_result_inner(
        &self,
        ctx: &AuthCtx,
        action_digest: &ActionDigest,
        action_result: ActionResult,
        request_id: &str,
    ) -> Result<UpdateActionResult, AcError> {
        // Step [0] — region + scope.
        self.check_region(ctx)?;
        Self::require_scope(ctx, SCOPE_CACHE_W)?;

        // Step [3] — Merkle verify (pre-persist).
        if let Err(merkle_err) = self.merkle.verify(&action_result) {
            self.audit.emit(self.make_record(
                AcEventType::UpdateMerkleInvalid,
                ctx,
                action_digest,
                request_id,
                Some(ResultHash::compute(&action_result)),
                merkle_err.audit_code(),
                Vec::new(),
            ))?;
            return Err(merkle_err.into());
        }

        // Steps [4]–[5] — outputs aliveness check.
        match self
            .outputs
            .assert_alive(ctx.tenant_id(), &action_result)
            .await?
        {
            OutputsCheckOutcome::AllAlive => {}
            OutputsCheckOutcome::SomeMissing { missing } => {
                let count = missing.len();
                self.audit.emit(self.make_record(
                    AcEventType::UpdateOutputsMissing,
                    ctx,
                    action_digest,
                    request_id,
                    Some(ResultHash::compute(&action_result)),
                    "outputs_missing",
                    missing,
                ))?;
                return Err(AcError::OutputsMissing { count });
            }
        }

        // Compute result_hash.
        let result_hash = ResultHash::compute(&action_result);

        // Step [6] — sig sign.
        let canonical = AcEnvelope::canonicalize(
            AC_ENVELOPE_VERSION,
            self.sig_key_id,
            ctx.tenant_id(),
            action_digest,
            &result_hash,
        )?;
        let signature: [u8; AC_ENVELOPE_SIG_LEN] = match self.signer.sign(
            ctx.tenant_id(),
            self.sig_key_id,
            &canonical,
        ) {
            Ok(s) => s,
            Err(sig_err) => {
                if !matches!(sig_err, SigError::KeyIdReserved) {
                    self.audit.emit(self.make_record(
                        AcEventType::UpdateSigInvalid,
                        ctx,
                        action_digest,
                        request_id,
                        Some(result_hash),
                        "sig_sign_failed",
                        Vec::new(),
                    ))?;
                }
                return Err(sig_err.into());
            }
        };
        let envelope = AcEnvelope {
            canonical_bytes: canonical,
            signature,
            sig_key_id: self.sig_key_id,
        };

        // Step [7] — envelope put (R2-first per WI §9.5; orphan
        // recoverable via S-06 reconcile).
        let action_hex = action_digest.hash.to_hex();
        let tenant_prefix = *ctx.tenant_prefix();
        self.envelope_store
            .put(self.region, &tenant_prefix, &action_hex, envelope)
            .await
            .map_err(AcError::BackendUnavailable)?;

        // Stash the action_result proto alongside the envelope so GET
        // can recover the typed shape (production: persisted in the
        // envelope JSON's `data.action_result_proto`). Pure-logic
        // handler keeps a sibling map.
        self.persist_action_result(self.region, &tenant_prefix, &action_hex, &action_result)
            .await
            .map_err(AcError::BackendUnavailable)?;

        // Step [8] — ac_meta upsert.
        let now_ms = self.clock.now_ms();
        let key = AcKey::new(ctx.tenant_id(), action_digest.hash);
        let upsert_outcome = self
            .meta
            .upsert(AcUpsertRequest {
                key,
                action_digest: *action_digest,
                tenant_prefix,
                region: self.region,
                result_hash,
                path_key_id: self.path_key_id,
                sig_key_id: self.sig_key_id,
                now_ms,
                ttl_ms: Some(self.ttl_extend_ms),
            })
            .await?;
        if let AcMetaUpsertOutcome::ResultHashMismatch {
            existing,
            attempted,
        } = upsert_outcome
        {
            self.audit.emit(self.make_record(
                AcEventType::UpdateResultMismatch,
                ctx,
                action_digest,
                request_id,
                Some(attempted),
                "result_hash_mismatch",
                Vec::new(),
            ))?;
            return Err(AcError::ResultHashMismatch {
                existing,
                attempted,
            });
        }

        // Step [9] — negative cache invalidate (after row commit).
        let tenant_ctx = ctx.tenant_ctx();
        self.neg_cache
            .invalidate_on_update(&tenant_ctx, &action_digest.hash)
            .await?;

        // Step [10] — audit emit.
        self.audit.emit(self.make_record(
            AcEventType::UpdateOk,
            ctx,
            action_digest,
            request_id,
            Some(result_hash),
            match upsert_outcome {
                AcMetaUpsertOutcome::Inserted => "inserted",
                AcMetaUpsertOutcome::IdempotentRefresh => "idempotent_refresh",
                AcMetaUpsertOutcome::ResultHashMismatch { .. } => "result_hash_mismatch",
            },
            Vec::new(),
        ))?;

        let row = self
            .meta
            .get(&key)
            .await?
            .ok_or_else(|| AcError::Internal("ac_meta row vanished post-upsert".to_string()))?;
        Ok(UpdateActionResult {
            action_result,
            upsert_outcome,
            row,
        })
    }
}

/// Side-channel: the pure-logic handler stashes the [`ActionResult`]
/// proto bytes alongside the envelope so GET can recover the typed
/// shape. Production wiring carries these inside the envelope JSON
/// directly. We expose this surface on the impl so the property tests
/// can assert the round-trip without spinning up a real proto codec.
impl<M, E, V, S, O, A, K> ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Persist the [`ActionResult`] bytes alongside the envelope —
    /// in-memory implementation; production stashes them inside the
    /// envelope JSON.
    async fn persist_action_result(
        &self,
        region: Region,
        prefix: &TenantPrefix,
        action_hex: &str,
        action_result: &ActionResult,
    ) -> Result<(), String> {
        // Per-instance stash keyed by the canonical envelope path —
        // tenant isolation by construction (the `region/prefix/hex`
        // tuple is tenant-leftmost). The stash is scoped to this
        // handler instance so concurrent in-process tests cannot
        // collide on shared keys (F-001 closure 2026-05-01).
        //
        // Production wiring inlines the `ActionResult` proto bytes
        // inside the envelope JSON; the fake `InMemoryAcEnvelopeStore`
        // delegates the proto round-trip back here so the property
        // tests can assert the canonical shape without spinning up a
        // real proto codec.
        let key = ActionResultStash::canonical(region, prefix, action_hex);
        self.action_result_stash
            .lock()
            .await
            .insert(key, action_result.clone());
        Ok(())
    }

    /// Recover the [`ActionResult`] bytes from the sibling store.
    async fn recover_action_result(
        &self,
        region: Region,
        prefix: &TenantPrefix,
        action_hex: &str,
    ) -> Result<ActionResult, String> {
        let key = ActionResultStash::canonical(region, prefix, action_hex);
        let guard = self.action_result_stash.lock().await;
        guard
            .get(&key)
            .cloned()
            .ok_or_else(|| "action_result missing from stash".to_string())
    }
}

/// Canonical key generator for the per-instance [`ActionResult`]
/// sibling stash (`region/prefix/hex` shape preserves tenant
/// isolation by construction). Production routes the proto bytes
/// through the envelope JSON inline; the in-memory fake delegates
/// the round-trip back to a per-handler-instance map (see
/// `ActionCacheHandlerImpl::action_result_stash`).
struct ActionResultStash;

impl ActionResultStash {
    fn canonical(region: Region, prefix: &TenantPrefix, hex: &str) -> String {
        format!(
            "ac-{}/{}/{}.action_result",
            region.bucket_suffix(),
            prefix.as_str(),
            hex
        )
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use crate::cache::kv::InMemoryKv;
    use crate::middleware::auth_ctx::__test_helpers::make_auth_ctx;
    use crate::middleware::auth_ctx::{AuthMethod, PrincipalId};
    use corelink_hash::Digest;
    use corelink_pat::{PatEnv, PatId, PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W};
    use corelink_tenant_path::TenantDerivationKey;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use super::super::audit::InMemoryAuditSink;
    use super::super::merkle::InMemoryMerkleVerifier;
    use super::super::meta::InMemoryAcMetaStore;
    use super::super::outputs::InMemoryOutputsCheck;
    use super::super::sig::InMemoryFakeSigner;
    use super::super::types::{ActionResult, OutputFileDigest};

    fn fixed_tdk() -> Arc<TenantDerivationKey> {
        Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])))
    }

    fn make_ctx(tenant: Uuid, region: Region, scopes: PatScopes) -> AuthCtx {
        let pat_id = PatId(Uuid::nil());
        make_auth_ctx(
            PrincipalId(Uuid::nil()),
            tenant,
            region,
            scopes,
            AuthMethod::Pat {
                env: PatEnv::Pat,
                pat_id,
            },
            fixed_tdk(),
        )
    }

    #[allow(dead_code, reason = "merkle/signer fields kept on Arc for shared-handle lifecycle even when tests do not borrow them")]
    struct Wiring {
        handler: ActionCacheHandlerImpl<
            InMemoryAcMetaStore,
            InMemoryAcEnvelopeStore,
            InMemoryMerkleVerifier,
            InMemoryFakeSigner,
            InMemoryOutputsCheck,
            InMemoryAuditSink,
            InMemoryKv,
        >,
        meta: Arc<InMemoryAcMetaStore>,
        envelope: Arc<InMemoryAcEnvelopeStore>,
        merkle: Arc<InMemoryMerkleVerifier>,
        signer: Arc<InMemoryFakeSigner>,
        outputs: Arc<InMemoryOutputsCheck>,
        audit: Arc<InMemoryAuditSink>,
        neg: Arc<AcNegCache<InMemoryKv>>,
        clock: Arc<FakeClock>,
    }

    fn wire(region: Region) -> Wiring {
        let meta = Arc::new(InMemoryAcMetaStore::new());
        let envelope = Arc::new(InMemoryAcEnvelopeStore::new());
        let merkle = Arc::new(InMemoryMerkleVerifier::new());
        let signer = Arc::new(InMemoryFakeSigner::new());
        let outputs = Arc::new(InMemoryOutputsCheck::new());
        let audit = Arc::new(InMemoryAuditSink::new());
        let neg = Arc::new(AcNegCache::new(region, InMemoryKv::new()).unwrap());
        let clock = Arc::new(FakeClock::new(1_000_000));
        let handler = ActionCacheHandlerImpl::new(ActionCacheHandlerBuilder {
            region,
            meta: Arc::clone(&meta),
            envelope_store: Arc::clone(&envelope),
            merkle: Arc::clone(&merkle),
            signer: Arc::clone(&signer),
            outputs: Arc::clone(&outputs),
            audit: Arc::clone(&audit),
            neg_cache: Arc::clone(&neg),
            sig_key_id: 1,
            path_key_id: 1,
            ttl_extend_ms: DEFAULT_AC_TTL_EXTEND_MS,
            clock: Arc::clone(&clock) as Arc<dyn Clock>,
        });
        Wiring {
            handler,
            meta,
            envelope,
            merkle,
            signer,
            outputs,
            audit,
            neg,
            clock,
        }
    }

    fn fresh_action_result() -> (ActionDigest, ActionResult) {
        let proto_bytes = b"canonical-action-proto-1".to_vec();
        let action_hash = Digest::compute(b"action-key-1");
        let action_digest = ActionDigest::new(action_hash, 32);
        let result = ActionResult::new(
            vec![OutputFileDigest::new(Digest::compute(b"out1"), 100)],
            Vec::new(),
            0,
            proto_bytes,
        );
        (action_digest, result)
    }

    #[tokio::test]
    async fn update_then_get_happy_path() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
        let (ad, ar) = fresh_action_result();
        // Pre-populate outputs alive.
        w.outputs.insert_all_alive(tenant, &ar);
        let upd = w
            .handler
            .update_action_result(&ctx, &ad, ar.clone(), "req-update-1")
            .await
            .unwrap();
        assert_eq!(upd.upsert_outcome, AcMetaUpsertOutcome::Inserted);
        // GET should now succeed.
        let get = w
            .handler
            .get_action_result(&ctx, &ad, "req-get-1")
            .await
            .unwrap();
        assert_eq!(get.action_result, ar);
        // Last hit advanced.
        assert!(get.row.last_hit_at_ms >= w.clock.now_ms());
    }

    #[tokio::test]
    async fn cross_tenant_get_returns_not_found() {
        let w = wire(Region::Wnam);
        let tenant_a = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let tenant_b = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
        let ctx_a = make_ctx(tenant_a, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
        let ctx_b = make_ctx(tenant_b, Region::Wnam, PatScopes::single(SCOPE_CACHE_R));
        let (ad, ar) = fresh_action_result();
        w.outputs.insert_all_alive(tenant_a, &ar);
        w.handler
            .update_action_result(&ctx_a, &ad, ar.clone(), "req-up-a")
            .await
            .unwrap();
        // B asks for the same digest under their own ctx.
        let err = w
            .handler
            .get_action_result(&ctx_b, &ad, "req-get-b")
            .await
            .unwrap_err();
        assert!(matches!(err, AcError::NotFound));
    }

    #[tokio::test]
    async fn missing_scope_rejected_403() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::empty());
        let (ad, _) = fresh_action_result();
        let err = w
            .handler
            .get_action_result(&ctx, &ad, "req-noscope")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AcError::ScopeInsufficient {
                required: SCOPE_CACHE_R
            }
        ));
        assert_eq!(err.cor_code(), "COR_AUTH_SCOPE_INSUFFICIENT");
    }

    #[tokio::test]
    async fn merkle_invalid_rejected_pre_persist() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let (ad, mut ar) = fresh_action_result();
        // Inject all-zero digest sentinel ⇒ MalformedTree.
        ar.output_files.push(OutputFileDigest::new(
            Digest::from_hex(&"00".repeat(32)).unwrap(),
            1,
        ));
        let err = w
            .handler
            .update_action_result(&ctx, &ad, ar, "req-merkle")
            .await
            .unwrap_err();
        assert!(matches!(err, AcError::MerkleInvalid { .. }));
        assert!(w.meta.is_empty().unwrap());
    }

    #[tokio::test]
    async fn outputs_missing_rejected_pre_persist() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let (ad, ar) = fresh_action_result();
        // outputs NOT inserted — missing.
        let err = w
            .handler
            .update_action_result(&ctx, &ad, ar, "req-outputs")
            .await
            .unwrap_err();
        assert!(matches!(err, AcError::OutputsMissing { count: 1 }));
        assert!(w.meta.is_empty().unwrap());
    }

    #[tokio::test]
    async fn idempotent_re_update_refreshes_last_hit() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
        let (ad, ar) = fresh_action_result();
        w.outputs.insert_all_alive(tenant, &ar);
        w.handler
            .update_action_result(&ctx, &ad, ar.clone(), "req-1")
            .await
            .unwrap();
        w.clock.advance_ms(5_000);
        let again = w
            .handler
            .update_action_result(&ctx, &ad, ar.clone(), "req-2")
            .await
            .unwrap();
        assert_eq!(
            again.upsert_outcome,
            AcMetaUpsertOutcome::IdempotentRefresh
        );
    }

    #[tokio::test]
    async fn result_hash_mismatch_rejected_409() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W));
        let (ad, ar1) = fresh_action_result();
        w.outputs.insert_all_alive(tenant, &ar1);
        w.handler
            .update_action_result(&ctx, &ad, ar1, "req-1")
            .await
            .unwrap();
        // Same action_digest, different proto bytes ⇒ different result_hash.
        let ar2 = ActionResult::new(
            vec![OutputFileDigest::new(Digest::compute(b"out1"), 100)],
            Vec::new(),
            0,
            b"DIFFERENT-PROTO".to_vec(),
        );
        w.outputs.insert_all_alive(tenant, &ar2);
        let err = w
            .handler
            .update_action_result(&ctx, &ad, ar2, "req-2")
            .await
            .unwrap_err();
        assert!(matches!(err, AcError::ResultHashMismatch { .. }));
        assert_eq!(err.cor_code(), "COR_AC_RESULT_HASH_MISMATCH");
    }

    #[tokio::test]
    async fn neg_cache_invalidated_on_update() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
        let (ad, ar) = fresh_action_result();
        // First GET — miss + populate negative cache.
        let _ = w
            .handler
            .get_action_result(&ctx, &ad, "req-get-miss")
            .await
            .unwrap_err();
        // Negative cache now hot.
        let tenant_ctx = ctx.tenant_ctx();
        let hit = w.neg.lookup(&tenant_ctx, &ad.hash).await.unwrap();
        assert_eq!(hit, Some(()));
        // UPDATE invalidates.
        w.outputs.insert_all_alive(tenant, &ar);
        w.handler
            .update_action_result(&ctx, &ad, ar.clone(), "req-upd")
            .await
            .unwrap();
        let hit = w.neg.lookup(&tenant_ctx, &ad.hash).await.unwrap();
        assert!(hit.is_none(), "neg cache must be invalidated post-UPDATE");
        // Subsequent GET — hit.
        let got = w
            .handler
            .get_action_result(&ctx, &ad, "req-get-hit")
            .await
            .unwrap();
        assert_eq!(got.action_result, ar);
    }

    #[tokio::test]
    async fn region_mismatch_returns_internal() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Weur, PatScopes::single(SCOPE_CACHE_R));
        let (ad, _) = fresh_action_result();
        let err = w
            .handler
            .get_action_result(&ctx, &ad, "req-mis")
            .await
            .unwrap_err();
        assert!(matches!(err, AcError::RegionMismatch { .. }));
    }

    #[tokio::test]
    async fn audit_emitted_on_get_ok() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
        let (ad, ar) = fresh_action_result();
        w.outputs.insert_all_alive(tenant, &ar);
        w.handler
            .update_action_result(&ctx, &ad, ar.clone(), "req-up")
            .await
            .unwrap();
        let _ = w.handler.get_action_result(&ctx, &ad, "req-get").await.unwrap();
        let ok = w.audit.snapshot_of(AcEventType::GetOk);
        assert_eq!(ok.len(), 1);
        let upd = w.audit.snapshot_of(AcEventType::UpdateOk);
        assert_eq!(upd.len(), 1);
    }

    #[tokio::test]
    async fn ttl_expired_returns_410() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
        let (ad, ar) = fresh_action_result();
        w.outputs.insert_all_alive(tenant, &ar);
        w.handler
            .update_action_result(&ctx, &ad, ar, "req-up")
            .await
            .unwrap();
        // Advance past TTL.
        w.clock.advance_ms(DEFAULT_AC_TTL_EXTEND_MS + 1);
        let err = w
            .handler
            .get_action_result(&ctx, &ad, "req-get")
            .await
            .unwrap_err();
        assert!(matches!(err, AcError::Expired));
        assert_eq!(err.cor_code(), "COR_AC_TTL_EXPIRED");
    }

    #[tokio::test]
    async fn envelope_tampering_detected_via_sig_invalid() {
        let w = wire(Region::Wnam);
        let tenant = Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap();
        let ctx = make_ctx(tenant, Region::Wnam, PatScopes::single(SCOPE_CACHE_W | SCOPE_CACHE_R));
        let (ad, ar) = fresh_action_result();
        w.outputs.insert_all_alive(tenant, &ar);
        w.handler
            .update_action_result(&ctx, &ad, ar, "req-up")
            .await
            .unwrap();
        // Tamper the envelope sig byte directly.
        let prefix = *ctx.tenant_prefix();
        let hex = ad.hash.to_hex();
        assert!(w.envelope.tamper_for_test(Region::Wnam, &prefix, &hex));
        let err = w
            .handler
            .get_action_result(&ctx, &ad, "req-get")
            .await
            .unwrap_err();
        assert!(matches!(err, AcError::SigInvalid));
        // Audit emitted.
        let sig = w.audit.snapshot_of(AcEventType::GetSigInvalid);
        assert_eq!(sig.len(), 1);
    }
}
