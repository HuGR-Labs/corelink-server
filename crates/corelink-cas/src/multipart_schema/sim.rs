//! Host-side in-memory simulator of the WI-S05-004 multipart schema.
//!
//! The simulator is **not** a SQL parser — it is a hand-coded fake of
//! the three migration tables (`chunks`, `manifest_chunks`,
//! `multipart_sessions`) that enforces every load-bearing invariant the
//! migration relies on:
//!
//! - `chunks` PRIMARY KEY uniqueness on `(tenant_id, chunk_digest)` +
//!   idempotent `INSERT … ON CONFLICT DO UPDATE SET refcount =
//!   refcount + 1` (`INV-MULTIPART-IDEMPOTENT`).
//! - Every inline `chk_chunks_*` CHECK constraint:
//!   - `chk_chunks_digest_len`: `length(chunk_digest) = 64`.
//!   - `chk_chunks_tenant_prefix_len`: `length(tenant_prefix) = 16`.
//!   - `chk_chunks_path_key_id_positive`: `path_key_id >= 1`.
//!   - `chk_chunks_region`: `region IN ('sam','iad','lhr','nrt','syd')`.
//!   - `chk_chunks_size_bytes`: `1 <= size_bytes <= 4_194_304`.
//!   - `chk_chunks_refcount_non_negative`: `refcount >= 0`.
//!   - `chk_chunks_lifecycle`: `last_referenced_at >= created_at`.
//! - `manifest_chunks` PRIMARY KEY uniqueness on
//!   `(tenant_id, blob_digest, chunk_index)` + every inline
//!   `chk_manifest_*` CHECK:
//!   - `chk_manifest_blob_digest_len`: `length(blob_digest) = 64`.
//!   - `chk_manifest_chunk_digest_len`: `length(chunk_digest) = 64`.
//!   - `chk_manifest_chunk_index_bounded`: `0 <= chunk_index < 81920`.
//! - `multipart_sessions` PRIMARY KEY on `session_id` + every inline
//!   `chk_multipart_*` CHECK + the **partial UNIQUE INDEX**
//!   `uq_multipart_sessions_in_progress` on `(tenant_id,
//!   blob_digest_expected) WHERE state = 'in_progress'` (Lote 10.5bis
//!   P0 fix; spec contract §5.1).
//! - State graph `in_progress → completed | aborted` (handler-level
//!   monotone enforcement; reverse rejected per
//!   `INV-MULTIPART-STATE-MONOTONIC`).
//! - Tenant-isolation envelope: every read API takes `tenant_id` first,
//!   so a Tenant B query for a Tenant A row returns `Ok(None)` — Layer
//!   4 of the 5-layer defence at storage level.
//!
//! Pure side-effect-free Rust — no async, no I/O. Property tests in
//! `tests/prop_multipart_schema.rs` drive 10 000 iterations against
//! this surface; integration tests in `tests/idempotency_canonical.rs`
//! pin the Gherkin scenarios from WI-S05-004 §8.

use std::collections::BTreeMap;

use thiserror::Error;
use uuid::Uuid;

use crate::multipart_schema::region::MultipartRegion;

/// Canonical hex digest length (BLAKE3-256 hex) for chunk_digest.
pub const CHUNK_DIGEST_HEX_LEN: usize = 64;
/// Canonical hex digest length (BLAKE3-256 hex) for blob_digest /
/// blob_digest_expected.
pub const BLOB_DIGEST_HEX_LEN: usize = 64;
/// Canonical materialised tenant_prefix length (HMAC truncation; ADR-0035 H-3).
pub const TENANT_PREFIX_LEN: usize = 16;
/// Canonical max chunk size in bytes (FastCDC max bound; lower bound is 1
/// byte — the final partial chunk may be < 1 MiB per Lote 10.5bis P0 fix).
pub const CHUNK_SIZE_BYTES_MAX: i64 = 4_194_304;
/// Canonical max chunks per blob (160 GiB / 2 MiB exact = 81920 per Lote
/// 10.5-tris P1-SR5-001 cross-crate alignment with `corelink-chunker`'s
/// `MAX_CHUNKS_PER_BLOB`).
pub const MAX_CHUNKS_PER_BLOB: i64 = 81_920;
/// Maximum legal `chunk_index` value (`MAX_CHUNKS_PER_BLOB - 1`).
pub const CHUNK_INDEX_MAX: i64 = MAX_CHUNKS_PER_BLOB - 1;
/// Default multipart-session TTL (7 days in milliseconds) per S-05 spec
/// contract §5.2 R-S05-5 sweeper SLA.
pub const DEFAULT_SESSION_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// All structured failure modes the simulator can report. Names mirror
/// the SQLite check / unique violation semantic.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SimError {
    /// Attempted to insert a row whose PK / UNIQUE column already
    /// exists. Mirrors SQLite `UNIQUE constraint failed`.
    #[error("unique_violation: {0}")]
    UniqueViolation(&'static str),

    /// CHECK constraint failed. Mirrors SQLite
    /// `CHECK constraint failed: <name>`.
    #[error("check_violation: {0}")]
    CheckViolation(&'static str),

    /// Partial UNIQUE INDEX violation: a second `in_progress` session
    /// for the same `(tenant_id, blob_digest_expected)` was
    /// attempted. The handler maps this to "return existing
    /// session_id" (S3/R2 native multipart idempotency contract).
    #[error("multipart_in_progress_already_exists: existing session {0}")]
    InProgressSessionAlreadyExists(SessionId),

    /// Handler-level monotone enforcement: state transition
    /// `in_progress → in_progress` (already in_progress) /
    /// `completed → in_progress` / `aborted → *` rejected per
    /// `INV-MULTIPART-STATE-MONOTONIC`.
    #[error("invalid_state_transition: {from:?} -> {to:?}")]
    InvalidStateTransition {
        /// Previous state recorded in the row.
        from: MultipartSessionState,
        /// Attempted next state.
        to: MultipartSessionState,
    },

    /// Cross-tenant access attempt: the session_id exists but its
    /// `tenant_id` does not match the calling context. Handler must
    /// map to the same `Ok(None)` shape an unknown session_id would
    /// produce — but the simulator surfaces the distinction so tests
    /// can prove the row was NOT touched.
    #[error("cross_tenant_session: {session_id} belongs to a different tenant")]
    CrossTenantSession {
        /// The opaque session id the caller supplied.
        session_id: SessionId,
    },

    /// Session_id not found.
    #[error("session_not_found: {0}")]
    SessionNotFound(SessionId),
}

/// Materialised `chunks` row (mirror of the SQL row layout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunksRow {
    /// PK component 1 — UUIDv7 minted app-side (canonical text form).
    pub tenant_id: Uuid,
    /// PK component 2 — BLAKE3-256 hex of chunk bytes (length 64).
    pub chunk_digest: String,
    /// Materialised tenant prefix BLOB(16) — HMAC truncation per
    /// ADR-0035 H-3.
    pub tenant_prefix: [u8; TENANT_PREFIX_LEN],
    /// Path-derivation TDK version (forward-compat S-14).
    pub path_key_id: i64,
    /// Region scope.
    pub region: MultipartRegion,
    /// R2 object key
    /// (`chunk-<region>/<tenant_prefix_hex>/<chunk_digest>` —
    /// composed by handler; never client-supplied).
    pub r2_object_key: String,
    /// Chunk byte count (1..=4_194_304).
    pub size_bytes: i64,
    /// Reference count (0 = candidate for S-06 GC).
    pub refcount: i64,
    /// Lifecycle timestamps (unix ms).
    pub created_at: i64,
    /// Lifecycle timestamps (unix ms; updated by manifest_chunks
    /// INSERT or explicit refresh).
    pub last_referenced_at: i64,
    /// Source PAT id (audit cross-check).
    pub created_by_pat_id: Option<String>,
    /// Request id correlation.
    pub created_by_request_id: Option<String>,
}

/// Argument bundle for [`MultipartSchema::upsert_chunk`].
#[derive(Clone, Debug)]
pub struct ChunkUpsertRequest {
    /// PK component 1.
    pub tenant_id: Uuid,
    /// PK component 2 — 64-char hex digest.
    pub chunk_digest: String,
    /// Materialised tenant prefix (handler computes via
    /// `corelink_tenant_path::derive_prefix(tdk, tenant_id)` once at
    /// INSERT time per ADR-0035 H-3).
    pub tenant_prefix: [u8; TENANT_PREFIX_LEN],
    /// Path-derivation TDK version.
    pub path_key_id: i64,
    /// Region scope.
    pub region: MultipartRegion,
    /// R2 object key (server-composed; never client-supplied).
    pub r2_object_key: String,
    /// Chunk byte count.
    pub size_bytes: i64,
    /// Unix epoch ms — handler clock (sets created_at on INSERT,
    /// last_referenced_at always).
    pub now_ms: i64,
    /// Source PAT id (audit cross-check).
    pub created_by_pat_id: Option<String>,
    /// Request id correlation.
    pub created_by_request_id: Option<String>,
}

/// Outcome of [`MultipartSchema::upsert_chunk`].
///
/// Mirrors the production
/// `INSERT … ON CONFLICT (tenant_id, chunk_digest) DO UPDATE SET
/// refcount = refcount + 1, last_referenced_at = ?` semantic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChunkUpsertOutcome {
    /// First-time INSERT (no prior `(tenant_id, chunk_digest)` row);
    /// `refcount = 1`.
    Inserted,
    /// Row pre-existed with matching `tenant_id + chunk_digest`;
    /// refcount incremented + `last_referenced_at` refreshed. Every
    /// other column is preserved (chunk_digest is content-addressed —
    /// the bytes the digest hashes are byte-identical so size_bytes /
    /// region / r2_object_key MUST match the existing row; if the
    /// caller supplies a different region the simulator rejects with
    /// [`SimError::CheckViolation`] — see
    /// [`MultipartSchema::upsert_chunk`] doc comment).
    Idempotent {
        /// Refcount AFTER the increment.
        new_refcount: i64,
    },
}

/// Materialised `manifest_chunks` row (mirror of the SQL row layout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestChunkRow {
    /// PK component 1.
    pub tenant_id: Uuid,
    /// PK component 2 — BLAKE3-256 hex of canonical blob root.
    pub blob_digest: String,
    /// PK component 3 — 0-indexed chunk order.
    pub chunk_index: i64,
    /// Logical FK to `chunks.chunk_digest`.
    pub chunk_digest: String,
}

/// Argument bundle for [`MultipartSchema::insert_manifest_chunk`].
#[derive(Clone, Debug)]
pub struct ManifestChunkInsertRequest {
    /// PK component 1.
    pub tenant_id: Uuid,
    /// PK component 2 — 64-char hex digest.
    pub blob_digest: String,
    /// PK component 3 — 0-indexed chunk order; `0..MAX_CHUNKS_PER_BLOB`.
    pub chunk_index: i64,
    /// Logical FK to `chunks.chunk_digest`.
    pub chunk_digest: String,
}

/// Outcome of [`MultipartSchema::insert_manifest_chunk`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ManifestChunkInsertOutcome {
    /// First-time INSERT.
    Inserted,
    /// Row pre-existed with the same `chunk_digest`; idempotent re-INSERT
    /// is a no-op (handler can re-issue the same manifest assembly without
    /// data drift).
    Idempotent,
}

/// Opaque session id. Production handlers mint this as a UUIDv7 / ULID
/// host-side; the simulator wraps a [`Uuid`] for type safety + cheap
/// equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub Uuid);

impl core::fmt::Display for SessionId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

/// Multipart session state — mirrors the SQL `state` enum domain
/// (`chk_multipart_state_domain`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MultipartSessionState {
    /// Active multipart upload; UploadPart accepted; Complete | Abort
    /// terminate.
    InProgress,
    /// CompleteMultipart succeeded; sealed manifest persisted; abort
    /// rejected per `INV-MULTIPART-FINALIZE-IRREVOCABLE`.
    Completed,
    /// Client cancelled OR sweeper aborted past TTL; finalize on
    /// Aborted rejected.
    Aborted,
}

impl MultipartSessionState {
    /// Canonical SQL literal value as it appears in the schema column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Aborted => "aborted",
        }
    }

    /// True if this state is a terminal (post-finalize) state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Aborted)
    }
}

/// Materialised `multipart_sessions` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipartSession {
    /// PK — opaque host-minted session id.
    pub session_id: SessionId,
    /// R2-issued upload_id; `None` until `record_r2_upload_id` is
    /// called post-init.
    pub r2_upload_id: Option<String>,
    /// Tenant scope (every method on the session enforces ctx ==
    /// row).
    pub tenant_id: Uuid,
    /// Materialised tenant prefix BLOB(16).
    pub tenant_prefix: [u8; TENANT_PREFIX_LEN],
    /// Path-derivation TDK version.
    pub path_key_id: i64,
    /// Expected blob digest (computed at SplitBlob start; verified at
    /// Complete).
    pub blob_digest_expected: String,
    /// Region scope.
    pub region: MultipartRegion,
    /// R2 bucket name (`corelink-chunk-<region>` |
    /// `corelink-manifest-<region>`).
    pub bucket: String,
    /// R2 object_key (server-composed via tenant_prefix; never
    /// client-supplied — `INV-MULTIPART-PATH-TENANT-SCOPED`).
    pub object_key: String,
    /// Lifecycle (unix ms).
    pub started_at: i64,
    /// Lifecycle (unix ms); updated per UploadPart.
    pub last_activity_at: i64,
    /// Lifecycle (unix ms); set on completed | aborted; NULL while
    /// in_progress.
    pub finalized_at_ms: Option<i64>,
    /// TTL: started_at + 7 days; sweeper aborts orphans past this.
    pub expires_at_ms: i64,
    /// State graph.
    pub state: MultipartSessionState,
    /// Source PAT id.
    pub created_by_pat_id: Option<String>,
    /// Request id correlation.
    pub created_by_request_id: String,
}

/// Argument bundle for [`MultipartSchema::initiate_session`].
#[derive(Clone, Debug)]
pub struct MultipartInitiateRequest {
    /// Host-minted session id (caller's responsibility — production
    /// uses UUIDv7).
    pub session_id: SessionId,
    /// Tenant scope.
    pub tenant_id: Uuid,
    /// Materialised tenant prefix.
    pub tenant_prefix: [u8; TENANT_PREFIX_LEN],
    /// Path-derivation TDK version.
    pub path_key_id: i64,
    /// Expected blob digest.
    pub blob_digest_expected: String,
    /// Region scope.
    pub region: MultipartRegion,
    /// R2 bucket name.
    pub bucket: String,
    /// R2 object_key (server-composed; never client-supplied).
    pub object_key: String,
    /// Unix epoch ms — handler clock.
    pub now_ms: i64,
    /// Optional explicit TTL in ms; `None` ⇒ default 7 days.
    pub ttl_ms: Option<i64>,
    /// Source PAT id.
    pub created_by_pat_id: Option<String>,
    /// Request id correlation.
    pub created_by_request_id: String,
}

/// Outcome of [`MultipartSchema::initiate_session`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultipartInitiateOutcome {
    /// First-time INSERT; new session created.
    Inserted,
    /// A pre-existing `in_progress` session for the same
    /// `(tenant_id, blob_digest_expected)` exists; handler returns the
    /// existing session_id (S3/R2 native multipart idempotency
    /// semantic). The simulator returns the existing session_id so
    /// callers can inspect / decide.
    AlreadyInProgress {
        /// The pre-existing session id (NOT the caller's
        /// session_id).
        existing: SessionId,
    },
}

/// Argument bundle for [`MultipartSchema::finalize_session`].
#[derive(Clone, Debug)]
pub struct MultipartFinalizeRequest {
    /// Session id.
    pub session_id: SessionId,
    /// Tenant scope (binding check).
    pub tenant_id: Uuid,
    /// Target terminal state.
    pub target_state: MultipartSessionState,
    /// Unix epoch ms — handler clock.
    pub now_ms: i64,
}

/// Outcome of [`MultipartSchema::finalize_session`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MultipartFinalizeOutcome {
    /// State transitioned `in_progress -> target_state` and
    /// `finalized_at_ms` was set.
    Finalized,
    /// Idempotent re-finalize on the same target state — no-op echo.
    IdempotentEcho,
}

/// In-memory simulator for the WI-S05-004 multipart schema.
///
/// Each `MultipartSchema` instance owns three independent BTreeMaps
/// keyed by canonical PK shape; F-001 closure preserved (no global
/// state — every shared collection is on the instance).
#[derive(Debug, Default)]
pub struct MultipartSchema {
    /// `chunks` table keyed by `(tenant_id, chunk_digest)`.
    chunks: BTreeMap<(Uuid, String), ChunksRow>,
    /// `manifest_chunks` table keyed by `(tenant_id, blob_digest, chunk_index)`.
    manifest_chunks: BTreeMap<(Uuid, String, i64), ManifestChunkRow>,
    /// `multipart_sessions` table keyed by `session_id`.
    sessions: BTreeMap<SessionId, MultipartSession>,
}

impl MultipartSchema {
    /// Construct an empty schema instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // -------------------------------------------------------------
    // chunks
    // -------------------------------------------------------------

    /// Apply the canonical CHECK constraint validation against
    /// `request`; returns `Ok(())` if every constraint is satisfied.
    fn validate_chunk_check_constraints(req: &ChunkUpsertRequest) -> Result<(), SimError> {
        if req.chunk_digest.len() != CHUNK_DIGEST_HEX_LEN {
            return Err(SimError::CheckViolation("chk_chunks_digest_len"));
        }
        if !is_lower_hex(&req.chunk_digest) {
            return Err(SimError::CheckViolation(
                "chk_chunks_digest_len: non-hex characters",
            ));
        }
        // tenant_prefix length is type-system enforced via [u8; 16].
        if req.path_key_id < 1 {
            return Err(SimError::CheckViolation("chk_chunks_path_key_id_positive"));
        }
        // region is `MultipartRegion` enum so the SQL CHECK is type-system enforced.
        if !(1..=CHUNK_SIZE_BYTES_MAX).contains(&req.size_bytes) {
            return Err(SimError::CheckViolation("chk_chunks_size_bytes"));
        }
        Ok(())
    }

    /// Upsert: INSERT new chunk row OR increment refcount + refresh
    /// `last_referenced_at` on idempotent re-insert.
    ///
    /// **Content-addressed semantic**: `chunk_digest` is the BLAKE3-256
    /// hex of the chunk bytes; if a row already exists for `(tenant_id,
    /// chunk_digest)` the bytes the caller is uploading MUST be
    /// identical to the bytes that produced the existing row's digest.
    /// The simulator does NOT see the bytes — it can only check the
    /// `size_bytes` / `region` / `r2_object_key` columns line up. A
    /// `region` mismatch (a chunk dedup hit across regions) is rejected
    /// with [`SimError::CheckViolation`] (`chk_chunks_region_immutable`):
    /// the chunk's R2 location is part of its identity for routing
    /// purposes, and a hash collision across regions would otherwise
    /// silently overwrite the routing column. `size_bytes` mismatch on
    /// the same digest is treated as a CHECK violation
    /// (`chk_chunks_size_immutable`) since it would imply BLAKE3
    /// collision (cripto-impossible) OR a handler bug computing the
    /// digest. Production handlers should never hit either path; the
    /// simulator surfaces them so adversarial property tests can prove
    /// the contract.
    pub fn upsert_chunk(
        &mut self,
        req: ChunkUpsertRequest,
    ) -> Result<ChunkUpsertOutcome, SimError> {
        Self::validate_chunk_check_constraints(&req)?;

        let key = (req.tenant_id, req.chunk_digest.clone());
        match self.chunks.get_mut(&key) {
            None => {
                let row = ChunksRow {
                    tenant_id: req.tenant_id,
                    chunk_digest: req.chunk_digest.clone(),
                    tenant_prefix: req.tenant_prefix,
                    path_key_id: req.path_key_id,
                    region: req.region,
                    r2_object_key: req.r2_object_key,
                    size_bytes: req.size_bytes,
                    refcount: 1,
                    created_at: req.now_ms,
                    last_referenced_at: req.now_ms,
                    created_by_pat_id: req.created_by_pat_id,
                    created_by_request_id: req.created_by_request_id,
                };
                // chk_chunks_lifecycle (post-write self-consistency).
                if row.last_referenced_at < row.created_at {
                    return Err(SimError::CheckViolation("chk_chunks_lifecycle"));
                }
                self.chunks.insert(key, row);
                Ok(ChunkUpsertOutcome::Inserted)
            }
            Some(existing) => {
                // Content-addressed identity invariants:
                if existing.region != req.region {
                    return Err(SimError::CheckViolation("chk_chunks_region_immutable"));
                }
                if existing.size_bytes != req.size_bytes {
                    return Err(SimError::CheckViolation("chk_chunks_size_immutable"));
                }
                // Refcount monotonic increment; clamp last_referenced_at to max(prev, now)
                // to absorb minor NTP skew (mirror of WI-S04-002 idempotent semantic).
                existing.refcount = existing.refcount.saturating_add(1);
                let new_last_ref = existing.last_referenced_at.max(req.now_ms);
                if new_last_ref < existing.created_at {
                    return Err(SimError::CheckViolation("chk_chunks_lifecycle"));
                }
                existing.last_referenced_at = new_last_ref;
                Ok(ChunkUpsertOutcome::Idempotent {
                    new_refcount: existing.refcount,
                })
            }
        }
    }

    /// Decrement refcount on a chunk (S-06 GC; manifest delete path).
    /// Returns the new refcount; `None` if the row is absent. Floors
    /// at 0 (never goes negative — `chk_chunks_refcount_non_negative`).
    pub fn decrement_chunk_refcount(
        &mut self,
        tenant_id: &Uuid,
        chunk_digest: &str,
    ) -> Option<i64> {
        let key = (*tenant_id, chunk_digest.to_string());
        let row = self.chunks.get_mut(&key)?;
        // Floor at 0; an i64::saturating_sub on an already-zero value
        // would happily go negative (saturating only fires near MIN).
        if row.refcount > 0 {
            row.refcount -= 1;
        }
        Some(row.refcount)
    }

    /// Read a chunk row by canonical PK shape.
    #[must_use]
    pub fn get_chunk(&self, tenant_id: &Uuid, chunk_digest: &str) -> Option<&ChunksRow> {
        self.chunks.get(&(*tenant_id, chunk_digest.to_string()))
    }

    /// Tenant-scoped chunk enumeration (`SELECT * FROM chunks WHERE
    /// tenant_id = ?`).
    #[must_use]
    pub fn list_chunks_for_tenant(&self, ctx_tenant: &Uuid) -> Vec<&ChunksRow> {
        self.chunks
            .iter()
            .filter_map(|((t, _), row)| (t == ctx_tenant).then_some(row))
            .collect()
    }

    /// Sweep candidate (refcount = 0) chunks for a tenant.
    /// Mirrors the partial index `idx_chunks_tenant_refcount_zero`.
    #[must_use]
    pub fn list_gc_candidates(&self, ctx_tenant: &Uuid) -> Vec<&ChunksRow> {
        self.chunks
            .iter()
            .filter_map(|((t, _), row)| (t == ctx_tenant && row.refcount == 0).then_some(row))
            .collect()
    }

    /// Total chunks row count (cardinality assertions in tests).
    #[must_use]
    pub fn chunks_count(&self) -> usize {
        self.chunks.len()
    }

    // -------------------------------------------------------------
    // manifest_chunks
    // -------------------------------------------------------------

    /// Apply the canonical CHECK constraint validation against
    /// `request`.
    fn validate_manifest_chunk_check_constraints(
        req: &ManifestChunkInsertRequest,
    ) -> Result<(), SimError> {
        if req.blob_digest.len() != BLOB_DIGEST_HEX_LEN {
            return Err(SimError::CheckViolation("chk_manifest_blob_digest_len"));
        }
        if !is_lower_hex(&req.blob_digest) {
            return Err(SimError::CheckViolation(
                "chk_manifest_blob_digest_len: non-hex characters",
            ));
        }
        if req.chunk_digest.len() != CHUNK_DIGEST_HEX_LEN {
            return Err(SimError::CheckViolation("chk_manifest_chunk_digest_len"));
        }
        if !is_lower_hex(&req.chunk_digest) {
            return Err(SimError::CheckViolation(
                "chk_manifest_chunk_digest_len: non-hex characters",
            ));
        }
        if !(0..MAX_CHUNKS_PER_BLOB).contains(&req.chunk_index) {
            return Err(SimError::CheckViolation("chk_manifest_chunk_index_bounded"));
        }
        Ok(())
    }

    /// Insert a manifest_chunks row. Idempotent on
    /// `(tenant_id, blob_digest, chunk_index)` if `chunk_digest`
    /// matches the existing row; UNIQUE violation otherwise (a
    /// different `chunk_digest` for the same PK = manifest mismatch
    /// = handler bug).
    pub fn insert_manifest_chunk(
        &mut self,
        req: ManifestChunkInsertRequest,
    ) -> Result<ManifestChunkInsertOutcome, SimError> {
        Self::validate_manifest_chunk_check_constraints(&req)?;
        let key = (req.tenant_id, req.blob_digest.clone(), req.chunk_index);
        match self.manifest_chunks.get(&key) {
            None => {
                let row = ManifestChunkRow {
                    tenant_id: req.tenant_id,
                    blob_digest: req.blob_digest,
                    chunk_index: req.chunk_index,
                    chunk_digest: req.chunk_digest,
                };
                self.manifest_chunks.insert(key, row);
                Ok(ManifestChunkInsertOutcome::Inserted)
            }
            Some(existing) => {
                if existing.chunk_digest == req.chunk_digest {
                    Ok(ManifestChunkInsertOutcome::Idempotent)
                } else {
                    Err(SimError::UniqueViolation(
                        "manifest_chunks: PK collision with mismatched chunk_digest",
                    ))
                }
            }
        }
    }

    /// Read a manifest_chunks row by canonical PK shape.
    #[must_use]
    pub fn get_manifest_chunk(
        &self,
        tenant_id: &Uuid,
        blob_digest: &str,
        chunk_index: i64,
    ) -> Option<&ManifestChunkRow> {
        self.manifest_chunks
            .get(&(*tenant_id, blob_digest.to_string(), chunk_index))
    }

    /// Manifest scan: every chunk_index for the given (tenant,
    /// blob_digest) ordered by `chunk_index`. The PK btree direction
    /// gives this for free.
    #[must_use]
    pub fn list_manifest_chunks(
        &self,
        tenant_id: &Uuid,
        blob_digest: &str,
    ) -> Vec<&ManifestChunkRow> {
        self.manifest_chunks
            .iter()
            .filter_map(|((t, b, _), row)| {
                (t == tenant_id && b.as_str() == blob_digest).then_some(row)
            })
            .collect()
    }

    /// Total manifest_chunks row count.
    #[must_use]
    pub fn manifest_chunks_count(&self) -> usize {
        self.manifest_chunks.len()
    }

    // -------------------------------------------------------------
    // multipart_sessions
    // -------------------------------------------------------------

    /// Apply the canonical CHECK constraint validation against an
    /// `initiate` request.
    fn validate_session_check_constraints(req: &MultipartInitiateRequest) -> Result<(), SimError> {
        if req.blob_digest_expected.len() != BLOB_DIGEST_HEX_LEN {
            return Err(SimError::CheckViolation("chk_multipart_blob_digest_len"));
        }
        if !is_lower_hex(&req.blob_digest_expected) {
            return Err(SimError::CheckViolation(
                "chk_multipart_blob_digest_len: non-hex characters",
            ));
        }
        if req.path_key_id < 1 {
            return Err(SimError::CheckViolation(
                "chk_multipart_path_key_id_positive",
            ));
        }
        // tenant_prefix length is type-system enforced via [u8; 16].
        // region is `MultipartRegion` enum.
        // state defaults to in_progress on initiate.
        if let Some(ttl) = req.ttl_ms {
            if ttl < 0 {
                return Err(SimError::CheckViolation("chk_multipart_lifecycle_expires"));
            }
        }
        Ok(())
    }

    /// Initiate a multipart session.
    ///
    /// Partial-UNIQUE semantic: if a row already exists with the same
    /// `(tenant_id, blob_digest_expected)` AND `state = 'in_progress'`,
    /// the SQL `uq_multipart_sessions_in_progress` partial UNIQUE
    /// rejects the insert. The simulator surfaces that as
    /// [`MultipartInitiateOutcome::AlreadyInProgress`] with the
    /// pre-existing session_id so the handler can return it (S3/R2
    /// native multipart idempotency contract).
    ///
    /// If the only matching rows are `completed` / `aborted`, those
    /// rows live as audit trail and DO NOT block a new in_progress
    /// session for the same blob (Lote 10.5bis P0 fix).
    ///
    /// `session_id` collision (caller-provided session_id reuse)
    /// surfaces as [`SimError::UniqueViolation`].
    pub fn initiate_session(
        &mut self,
        req: MultipartInitiateRequest,
    ) -> Result<MultipartInitiateOutcome, SimError> {
        Self::validate_session_check_constraints(&req)?;

        // Partial UNIQUE INDEX uq_multipart_sessions_in_progress check.
        if let Some(existing) =
            self.find_in_progress_session(&req.tenant_id, &req.blob_digest_expected)
        {
            return Ok(MultipartInitiateOutcome::AlreadyInProgress {
                existing: existing.session_id,
            });
        }

        // session_id PK uniqueness check.
        if self.sessions.contains_key(&req.session_id) {
            return Err(SimError::UniqueViolation(
                "multipart_sessions: session_id PK collision",
            ));
        }

        let ttl = req.ttl_ms.unwrap_or(DEFAULT_SESSION_TTL_MS);
        let session = MultipartSession {
            session_id: req.session_id,
            r2_upload_id: None,
            tenant_id: req.tenant_id,
            tenant_prefix: req.tenant_prefix,
            path_key_id: req.path_key_id,
            blob_digest_expected: req.blob_digest_expected,
            region: req.region,
            bucket: req.bucket,
            object_key: req.object_key,
            started_at: req.now_ms,
            last_activity_at: req.now_ms,
            finalized_at_ms: None,
            expires_at_ms: req.now_ms.saturating_add(ttl),
            state: MultipartSessionState::InProgress,
            created_by_pat_id: req.created_by_pat_id,
            created_by_request_id: req.created_by_request_id,
        };
        // chk_multipart_lifecycle_activity / _expires (post-write).
        if session.last_activity_at < session.started_at {
            return Err(SimError::CheckViolation("chk_multipart_lifecycle_activity"));
        }
        if session.expires_at_ms < session.started_at {
            return Err(SimError::CheckViolation("chk_multipart_lifecycle_expires"));
        }
        self.sessions.insert(session.session_id, session);
        Ok(MultipartInitiateOutcome::Inserted)
    }

    /// Record the R2-issued upload_id post-init. Returns the prior
    /// value if any. Cross-tenant guard fires if the calling tenant
    /// does not match the row.
    pub fn record_r2_upload_id(
        &mut self,
        tenant_id: &Uuid,
        session_id: &SessionId,
        r2_upload_id: String,
    ) -> Result<(), SimError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or(SimError::SessionNotFound(*session_id))?;
        if &session.tenant_id != tenant_id {
            return Err(SimError::CrossTenantSession {
                session_id: *session_id,
            });
        }
        if !matches!(session.state, MultipartSessionState::InProgress) {
            return Err(SimError::InvalidStateTransition {
                from: session.state,
                to: MultipartSessionState::InProgress,
            });
        }
        session.r2_upload_id = Some(r2_upload_id);
        Ok(())
    }

    /// Refresh `last_activity_at` on a session (called per UploadPart).
    pub fn touch_session(
        &mut self,
        tenant_id: &Uuid,
        session_id: &SessionId,
        now_ms: i64,
    ) -> Result<(), SimError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or(SimError::SessionNotFound(*session_id))?;
        if &session.tenant_id != tenant_id {
            return Err(SimError::CrossTenantSession {
                session_id: *session_id,
            });
        }
        if !matches!(session.state, MultipartSessionState::InProgress) {
            return Err(SimError::InvalidStateTransition {
                from: session.state,
                to: MultipartSessionState::InProgress,
            });
        }
        // Monotonic clamp: the column may NEVER roll back.
        let new_activity = session.last_activity_at.max(now_ms);
        if new_activity < session.started_at {
            return Err(SimError::CheckViolation("chk_multipart_lifecycle_activity"));
        }
        session.last_activity_at = new_activity;
        Ok(())
    }

    /// Finalize a session into the canonical terminal state
    /// (`Completed` | `Aborted`). Enforces the monotone state graph:
    ///
    /// - `InProgress -> Completed` ✓
    /// - `InProgress -> Aborted` ✓
    /// - `Completed -> Completed` ✓ (idempotent echo)
    /// - `Aborted -> Aborted` ✓ (idempotent echo)
    /// - any other transition ⇒ [`SimError::InvalidStateTransition`].
    ///
    /// Idempotent re-finalize on the same target is a no-op echo
    /// (`MultipartFinalizeOutcome::IdempotentEcho`).
    pub fn finalize_session(
        &mut self,
        req: MultipartFinalizeRequest,
    ) -> Result<MultipartFinalizeOutcome, SimError> {
        if !req.target_state.is_terminal() {
            return Err(SimError::InvalidStateTransition {
                from: MultipartSessionState::InProgress,
                to: req.target_state,
            });
        }
        let session = self
            .sessions
            .get_mut(&req.session_id)
            .ok_or(SimError::SessionNotFound(req.session_id))?;
        if session.tenant_id != req.tenant_id {
            return Err(SimError::CrossTenantSession {
                session_id: req.session_id,
            });
        }
        match (session.state, req.target_state) {
            (MultipartSessionState::InProgress, target) if target.is_terminal() => {
                session.state = target;
                let new_activity = session.last_activity_at.max(req.now_ms);
                session.last_activity_at = new_activity;
                let finalized_at = new_activity.max(session.started_at);
                session.finalized_at_ms = Some(finalized_at);
                Ok(MultipartFinalizeOutcome::Finalized)
            }
            (current, target) if current == target => {
                // Idempotent echo on the same terminal state.
                Ok(MultipartFinalizeOutcome::IdempotentEcho)
            }
            (from, to) => Err(SimError::InvalidStateTransition { from, to }),
        }
    }

    /// Read a session by id. Cross-tenant access returns
    /// `Err(CrossTenantSession)` so callers can prove the row was NOT
    /// touched (production handlers map to the same `Ok(None)` shape
    /// an unknown id would produce; the simulator surfaces the
    /// distinction for adversarial property tests).
    pub fn get_session(
        &self,
        tenant_id: &Uuid,
        session_id: &SessionId,
    ) -> Result<Option<&MultipartSession>, SimError> {
        match self.sessions.get(session_id) {
            None => Ok(None),
            Some(session) if &session.tenant_id == tenant_id => Ok(Some(session)),
            Some(_) => Err(SimError::CrossTenantSession {
                session_id: *session_id,
            }),
        }
    }

    /// Tenant-scoped session enumeration.
    #[must_use]
    pub fn list_sessions_for_tenant(&self, ctx_tenant: &Uuid) -> Vec<&MultipartSession> {
        self.sessions
            .values()
            .filter(|s| &s.tenant_id == ctx_tenant)
            .collect()
    }

    /// Orphan session enumeration — mirrors the partial index
    /// `idx_multipart_sessions_orphan_sweep`. Returns every
    /// `in_progress` session whose `last_activity_at < cutoff_ms`.
    #[must_use]
    pub fn list_orphan_sessions(&self, cutoff_ms: i64) -> Vec<&MultipartSession> {
        self.sessions
            .values()
            .filter(|s| {
                matches!(s.state, MultipartSessionState::InProgress)
                    && s.last_activity_at < cutoff_ms
            })
            .collect()
    }

    /// Total session row count.
    #[must_use]
    pub fn sessions_count(&self) -> usize {
        self.sessions.len()
    }

    /// Find the (single) in_progress session for a given (tenant_id,
    /// blob_digest_expected) — implements the partial-UNIQUE INDEX
    /// `uq_multipart_sessions_in_progress` lookup.
    fn find_in_progress_session(
        &self,
        tenant_id: &Uuid,
        blob_digest_expected: &str,
    ) -> Option<&MultipartSession> {
        self.sessions.values().find(|s| {
            &s.tenant_id == tenant_id
                && s.blob_digest_expected.as_str() == blob_digest_expected
                && matches!(s.state, MultipartSessionState::InProgress)
        })
    }

    // -------------------------------------------------------------
    // schema-level helpers
    // -------------------------------------------------------------

    /// Re-apply the migration. Idempotent (no-op).
    ///
    /// In production, `wrangler d1 migrations apply` re-running the
    /// SQL artifact is a no-op via `CREATE TABLE IF NOT EXISTS` /
    /// `CREATE INDEX IF NOT EXISTS` / `CREATE UNIQUE INDEX IF NOT
    /// EXISTS`. The simulator pins the same contract at the host-side
    /// level so property tests can assert "applying twice does not
    /// lose data".
    pub fn reapply_migration(&mut self) -> Result<(), SimError> {
        // Schema is the row layout; no DDL changes since the simulator
        // never had columns to add. Re-apply is a no-op; rows remain.
        Ok(())
    }
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
