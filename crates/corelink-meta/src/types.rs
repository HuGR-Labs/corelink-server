//! Canonical metadata types — keyed lookups, row payloads, audit envelopes.

use core::fmt;

use corelink_hash::Digest;
use uuid::Uuid;

/// Newtype over `Uuid` for canonical D1 `tenant_id` storage.
///
/// `tenant_id` is stored in D1 as canonical UUIDv7 text form
/// (`data_model.md §2.1` — `01938af0-abcd-7123-8456-..`). This newtype enforces
/// "the canonical text form is the only thing that ever leaves Rust into a
/// SQL parameter" and gives a single place to plug a future text-form
/// validator without churning every call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(Uuid);

impl TenantId {
    /// Wrap a `Uuid` into the canonical newtype.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the inner `Uuid`.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Render the UUIDv7 in canonical hyphenated lowercase text form.
    ///
    /// This is the exact byte sequence written to D1 `tenant_id` columns
    /// and to `audit_outbox.tenant_id`.
    #[must_use]
    pub fn to_canonical_text(&self) -> String {
        // `Uuid::Display` uses canonical hyphenated lowercase — by spec.
        format!("{}", self.0)
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<Uuid> for TenantId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// Composite primary key for `blob_meta`: `(tenant_id, digest)`.
///
/// Constructing a [`BlobMetaKey`] is the only way to address a row.
/// `digest` arrives via the [`corelink_hash::Digest`] type whose
/// constructors verify length + alphabet — so the key never carries
/// untrusted hex bytes into a SQL parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlobMetaKey {
    tenant: TenantId,
    digest: Digest,
}

impl BlobMetaKey {
    /// Construct a key from the two canonical parts.
    #[must_use]
    pub fn new<T: Into<TenantId>>(tenant: T, digest: Digest) -> Self {
        Self {
            tenant: tenant.into(),
            digest,
        }
    }

    /// Borrow the tenant component.
    #[must_use]
    pub const fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Borrow the digest component.
    #[must_use]
    pub const fn digest(&self) -> &Digest {
        &self.digest
    }

    /// Canonical text form of `digest` for D1 storage: `'algo:hex'`.
    ///
    /// `algo` is hard-coded to `blake3` here because S-01 only stores
    /// BLAKE3 digests; future multi-algo support (S-12 hash agility) will
    /// add a parameter. `hex` is the 64-char lowercase BLAKE3-256 output.
    #[must_use]
    pub fn digest_canonical_text(&self) -> String {
        format!("blake3:{}", self.digest.to_hex())
    }
}

/// A row read back from `blob_meta`. Returned by [`crate::MetaStore::get`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlobMetaRow {
    /// Composite key — exposed for round-tripping ergonomics.
    pub key: BlobMetaKey,
    /// Canonical body size in bytes; always `> 0` per CHECK constraint.
    pub size_bytes: u64,
    /// Refcount; always `>= 0` per CHECK constraint. `0` ⇒ deletable by GC.
    pub refcount: u64,
    /// Unix epoch millisecond timestamp the row was first inserted.
    pub created_at_ms: u64,
    /// Unix epoch millisecond timestamp of the last refcount increment OR
    /// successful read-after-write (S-02 read path).
    pub last_accessed_at_ms: u64,
    /// `Some(ms)` once the row has been soft-deleted; `None` for alive rows.
    /// A tombstoned row is excluded from `idx_blob_meta_tenant_alive` and
    /// becomes a GC candidate via `idx_blob_meta_gc_candidates`.
    pub deleted_at_ms: Option<u64>,
}

impl BlobMetaRow {
    /// Convenience: `true` iff the row is alive (`deleted_at IS NULL`).
    #[must_use]
    pub const fn is_alive(&self) -> bool {
        self.deleted_at_ms.is_none()
    }
}

/// Outcome of a [`crate::MetaStore::commit_put`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InsertOutcome {
    /// First-time insert; a fresh row was created with `refcount = 1`.
    /// The accompanying `audit_outbox` event was inserted in the same
    /// atomic batch (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
    Inserted,
    /// `INSERT OR IGNORE` no-op: the row already existed. Per
    /// INV-CAS-IMMUTABILITY the existing row's `digest`/`size_bytes` are
    /// guaranteed identical (CAS guarantees content equivalence). The
    /// caller's idempotency-keyed audit event was still de-duplicated via
    /// `(request_id, event_type)` UNIQUE so no double-emission occurs.
    AlreadyExists,
}

/// Outcome of a [`crate::MetaStore::commit_decrement`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecrementOutcome {
    /// Refcount decremented but stays `> 0`. Row remains alive.
    Decremented {
        /// New refcount after the decrement.
        new_refcount: u64,
    },
    /// Refcount reached zero. The row is now eligible for GC sweep
    /// (S-06); it is *not* tombstoned automatically — the caller decides
    /// whether to call [`crate::MetaStore::commit_soft_delete`] or wait
    /// for the grace period.
    ReachedZero,
}

/// Idempotency key for audit events. Wraps a free-form client-provided
/// string (the REAPI handler propagates the gRPC `x-request-id` header).
///
/// `request_id` is part of the `(request_id, event_type)` UNIQUE constraint
/// in `audit_outbox`. Wrapping it in a newtype prevents accidental swaps
/// with `event_type` at call sites and gives a single place to enforce
/// length / charset bounds in a future hardening pass.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// Wrap a string into a [`RequestId`].
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for RequestId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for RequestId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Audit-outbox envelope written atomically alongside every metadata mutation.
///
/// The fields map 1:1 to the `audit_outbox` columns (id / request_id /
/// event_type / payload_json). The drain worker (S-09) consumes these rows
/// post-COMMIT, sealing them into the audit chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    /// Stable UUIDv7 identifier for this audit row. Caller-provided so the
    /// outbox row's PK is deterministic per (commit, attempt) pair — useful
    /// for at-least-once retry semantics in WI-S01-005.
    pub id: Uuid,
    /// Idempotency key. Combines with `event_type` to enforce
    /// "same retry → same effect".
    pub request_id: RequestId,
    /// CloudEvents `type` attribute (`corelink.cas.put_completed`,
    /// `corelink.cas.refcount_decremented`, …).
    pub event_type: AuditEventType,
    /// CloudEvents 1.0 envelope serialized as JSON. We do not parse it
    /// inside this crate — the audit chain (S-09) is the schema authority;
    /// the meta layer only persists and de-duplicates.
    pub payload_json: String,
}

/// Canonical `event_type` enum. WI-S01-004 narrative §1 + §2 names two
/// blob-related events; we expose them as a small typed enum so the REAPI
/// handler cannot accidentally typo a string label and break the audit
/// chain consumer (S-09 dispatches on `event_type`).
///
/// Variants beyond the S-01 trio (`put_completed` / `refcount_decremented` /
/// `soft_deleted`) live in their own crates (S-04 AC, S-06 GC, S-09 chain).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AuditEventType {
    /// First-time blob written.
    CasPutCompleted,
    /// Refcount decremented (may or may not have reached zero).
    CasRefcountDecremented,
    /// Blob soft-deleted (tombstoned). GC sweep eligible after grace period.
    CasSoftDeleted,
}

impl AuditEventType {
    /// Stable canonical CloudEvents `type` string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CasPutCompleted => "corelink.cas.put_completed",
            Self::CasRefcountDecremented => "corelink.cas.refcount_decremented",
            Self::CasSoftDeleted => "corelink.cas.soft_deleted",
        }
    }
}

impl fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AuditEvent {
    /// Convenience constructor for a `corelink.cas.put_completed` event.
    #[must_use]
    pub fn cas_put_completed(
        id: Uuid,
        request_id: impl Into<RequestId>,
        payload_json: impl Into<String>,
    ) -> Self {
        Self {
            id,
            request_id: request_id.into(),
            event_type: AuditEventType::CasPutCompleted,
            payload_json: payload_json.into(),
        }
    }

    /// Convenience constructor for a `corelink.cas.refcount_decremented`
    /// event.
    #[must_use]
    pub fn cas_refcount_decremented(
        id: Uuid,
        request_id: impl Into<RequestId>,
        payload_json: impl Into<String>,
    ) -> Self {
        Self {
            id,
            request_id: request_id.into(),
            event_type: AuditEventType::CasRefcountDecremented,
            payload_json: payload_json.into(),
        }
    }

    /// Convenience constructor for a `corelink.cas.soft_deleted` event.
    #[must_use]
    pub fn cas_soft_deleted(
        id: Uuid,
        request_id: impl Into<RequestId>,
        payload_json: impl Into<String>,
    ) -> Self {
        Self {
            id,
            request_id: request_id.into(),
            event_type: AuditEventType::CasSoftDeleted,
            payload_json: payload_json.into(),
        }
    }
}
