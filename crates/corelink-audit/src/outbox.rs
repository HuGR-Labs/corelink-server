//! Production [`OutboxEmitter`] — the durable, fail-CLOSED [`Emitter`].
//!
//! ## Why this exists
//!
//! [`crate::emitter::InMemoryEmitter`] is a TEST sink: it captures events in a
//! `Vec` that evaporates with the process. The emitter-module docs describe the
//! production ladder (`OutboxEmitter` → `DirectSiemEmitter` → `MultiplexEmitter`)
//! as "deferred to S-03 wiring". This module lands the load-bearing rung —
//! `OutboxEmitter` — so auth-plane audit events actually PERSIST instead of
//! dropping (WI item 3).
//!
//! ## What it does
//!
//! [`OutboxEmitter::emit`]:
//!
//! 1. Canonicalizes the [`AuthEvent`] via [`compute_content_hash`] (RFC 8785 JCS
//!    → SHA-256, 64-hex) — the tamper-evident content digest the audit chain
//!    links on.
//! 2. Serializes the canonical CloudEvents 1.0 JSON line.
//! 3. Builds an [`AuditOutboxRow`] keyed by the CloudEvents `id` (UUIDv7) — the
//!    stable idempotency key a retry reuses (see [`AuthEvent::with_id`]).
//! 4. Appends it through the injected [`AuditOutboxWriter`] port and propagates
//!    ANY failure as [`EmitterError::Store`] — **fail-CLOSED**. There is no arm
//!    that swallows a failed append: the handler MUST translate the `Err` into a
//!    503-class response and roll back its primary mutation
//!    (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER, CRITICAL).
//!
//! ## The `AuditOutboxWriter` seam (why a port)
//!
//! The actual durable INSERT belongs in the SAME D1 batch as the handler's main
//! mutation, so the batch commits both rows or neither. That D1/`tokio` machinery
//! is platform-gated and lives in the binding layer (`corelink-container` /
//! `corelink-adapters-cloud`), which is where a real [`AuditOutboxWriter`] is
//! implemented over `D1HttpClient::batch`. Keeping the port sync + storage-free
//! here preserves the crate's `wasm32-unknown-unknown` cleanliness (matching the
//! [`Emitter`] trait rationale) and lets the whole canonicalize→row→append
//! pipeline be unit-tested against the in-memory fake below.
//!
//! **Remaining wiring (flagged partial):** the container-side
//! `AuditOutboxWriter` over the real D1 batch + the drain that ships
//! `audit_outbox` rows into the R2/`corelink-audit-chain` sink are the
//! deployment-layer follow-up; the algorithmic core (canonicalization, the
//! fail-CLOSED contract, the idempotency-keyed row shape) is complete + tested
//! here.

use std::sync::{Arc, Mutex};

use crate::emitter::{Emitter, EmitterError};
use crate::events::AuthEvent;
use crate::link_hash::compute_content_hash;

/// One canonical audit-outbox row, ready for a same-batch D1 INSERT.
///
/// The field set mirrors the `audit_outbox` shape the drain consumes: the
/// idempotency key (`event_id`), the tenant partition, the taxonomy slug + time
/// for cheap indexed queries, the tamper-evident `content_hash`, and the full
/// canonical CloudEvents line the chain sink re-canonicalizes and links.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditOutboxRow {
    /// CloudEvents `id` (UUIDv7) — the stable idempotency key. A retried emit
    /// reuses the same `id` (via [`AuthEvent::with_id`]) so the drain's
    /// `INSERT OR IGNORE` is exactly-once.
    pub event_id: String,
    /// Tenant partition (UUIDv7 hyphenated lowercase canonical text).
    pub tenant_id: String,
    /// Stable CloudEvents `type` slug (e.g. `auth.token.validated`).
    pub event_type: String,
    /// CloudEvents `time` — Unix milliseconds.
    pub time_unix_ms: i64,
    /// 64-hex lowercase SHA-256 over the RFC 8785 JCS-canonical event bytes.
    pub content_hash: String,
    /// The canonical CloudEvents 1.0 JSON (the NDJSON line body the chain sink
    /// persists to R2).
    pub cloudevent_json: String,
}

/// Port: append ONE canonical audit row durably, in the SAME transaction/batch
/// as the caller's primary mutation.
///
/// Implementors MUST persist all-or-nothing with the handler's mutation so a
/// committed op can never lack its audit row and vice-versa
/// (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). The production impl lives in the
/// container binding layer over `D1HttpClient::batch`; the in-memory
/// [`InMemoryAuditOutboxWriter`] + [`FailingAuditOutboxWriter`] fakes below
/// exercise the fail-CLOSED contract in unit tests.
pub trait AuditOutboxWriter: Send + Sync {
    /// Append `row`. `Ok(())` iff the row is durably committed (the D1 batch
    /// committed); `Err(reason)` on any backend failure.
    ///
    /// # Errors
    ///
    /// Returns a backend-specific reason string on any failure to persist. The
    /// [`OutboxEmitter`] maps it to [`EmitterError::Store`]; the caller MUST NOT
    /// swallow it.
    fn append(&self, row: AuditOutboxRow) -> Result<(), String>;
}

/// Production [`Emitter`]: canonicalizes each [`AuthEvent`] and appends it to the
/// durable audit outbox through an injected [`AuditOutboxWriter`]. Fail-CLOSED —
/// a failed append surfaces as [`EmitterError::Store`].
#[derive(Clone)]
pub struct OutboxEmitter<W>
where
    W: AuditOutboxWriter,
{
    writer: Arc<W>,
}

impl<W> core::fmt::Debug for OutboxEmitter<W>
where
    W: AuditOutboxWriter,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OutboxEmitter").finish_non_exhaustive()
    }
}

impl<W> OutboxEmitter<W>
where
    W: AuditOutboxWriter,
{
    /// Compose the emitter over a durable outbox writer.
    #[must_use]
    pub fn new(writer: Arc<W>) -> Self {
        Self { writer }
    }

    /// Build the canonical [`AuditOutboxRow`] for `event` (canonicalize +
    /// serialize). Extracted so the row shape is unit-testable without a writer.
    ///
    /// # Errors
    ///
    /// [`EmitterError::Store`] if JCS canonicalization or JSON serialization of
    /// the event fails (a malformed event that cannot be persisted must
    /// fail-CLOSED, never emit an unhashed row).
    pub fn build_row(event: &AuthEvent) -> Result<AuditOutboxRow, EmitterError> {
        let content_hash = compute_content_hash(event)
            .map_err(|e| EmitterError::Store(format!("audit content-hash failed: {e}")))?;
        let cloudevent_json = serde_json::to_string(event)
            .map_err(|e| EmitterError::Store(format!("audit event serialize failed: {e}")))?;
        Ok(AuditOutboxRow {
            event_id: event.id.to_string(),
            tenant_id: event.tenant_id.to_canonical_text(),
            event_type: event.event_type.as_str().to_string(),
            time_unix_ms: event.time_unix_ms,
            content_hash: content_hash.as_str().to_string(),
            cloudevent_json,
        })
    }
}

impl<W> Emitter for OutboxEmitter<W>
where
    W: AuditOutboxWriter,
{
    fn emit(&self, event: AuthEvent) -> Result<(), EmitterError> {
        let row = Self::build_row(&event)?;
        // Fail-CLOSED: any append failure is the operation's error. No `.ok()`,
        // no `let _ =` — the `?` propagates so the handler surfaces 503-class.
        self.writer
            .append(row)
            .map_err(EmitterError::Store)
    }
}

/// In-memory [`AuditOutboxWriter`] test/staging fake: captures every appended
/// row. Cloning shares the buffer (`Arc<Mutex<Vec<…>>>`).
#[derive(Clone, Debug, Default)]
pub struct InMemoryAuditOutboxWriter {
    rows: Arc<Mutex<Vec<AuditOutboxRow>>>,
}

impl InMemoryAuditOutboxWriter {
    /// Construct an empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every appended row.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuditOutboxRow> {
        match self.rows.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of rows appended.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.rows.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether no rows have been appended.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AuditOutboxWriter for InMemoryAuditOutboxWriter {
    fn append(&self, row: AuditOutboxRow) -> Result<(), String> {
        match self.rows.lock() {
            Ok(mut g) => {
                g.push(row);
                Ok(())
            }
            Err(_) => Err("in-memory audit outbox mutex poisoned".to_string()),
        }
    }
}

/// Always-failing [`AuditOutboxWriter`] for adversarial fail-CLOSED tests.
#[derive(Debug, Default)]
pub struct FailingAuditOutboxWriter;

impl FailingAuditOutboxWriter {
    /// Construct the always-failing writer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AuditOutboxWriter for FailingAuditOutboxWriter {
    fn append(&self, _row: AuditOutboxRow) -> Result<(), String> {
        Err("induced audit outbox append failure (test fixture)".to_string())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::events::{AuthEventData, AuthEventType, RegionTag, TokenKind};
    use crate::redact::PrincipalIdHash;
    use crate::retention::RetentionHint;
    use crate::{RequestId, TenantId};
    use uuid::Uuid;

    fn sample(id: Uuid, tenant: Uuid) -> AuthEvent {
        AuthEvent::with_id(
            id,
            AuthEventType::TokenValidated,
            "corelink://wnam/auth/middleware",
            TenantId::from_uuid(tenant),
            PrincipalIdHash::derive("user_x").expect("derive"),
            RegionTag::Wnam,
            RequestId::new("req_x"),
            RetentionHint::Team90d,
            1_700_000_000_000,
            AuthEventData::TokenValidated {
                token_kind: TokenKind::Pat,
                scope_bitset: 1,
            },
        )
    }

    #[test]
    fn emit_persists_canonical_row() {
        let writer = Arc::new(InMemoryAuditOutboxWriter::new());
        let emitter = OutboxEmitter::new(Arc::clone(&writer));
        let id = Uuid::now_v7();
        let tenant = Uuid::now_v7();
        emitter.emit(sample(id, tenant)).expect("emit persists");

        let rows = writer.snapshot();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.event_id, id.to_string());
        assert_eq!(row.tenant_id, tenant.to_string());
        assert_eq!(row.event_type, "auth.token.validated");
        assert_eq!(row.time_unix_ms, 1_700_000_000_000);
        // 64-hex SHA-256 digest.
        assert_eq!(row.content_hash.len(), 64);
        assert!(row.content_hash.chars().all(|c| c.is_ascii_hexdigit()));
        // The canonical line carries the CloudEvents type slug.
        assert!(row.cloudevent_json.contains("auth.token.validated"));
    }

    #[test]
    fn writer_failure_surfaces_fail_closed() {
        let emitter = OutboxEmitter::new(Arc::new(FailingAuditOutboxWriter::new()));
        let err = emitter
            .emit(sample(Uuid::now_v7(), Uuid::now_v7()))
            .expect_err("append failure must surface as an emit error");
        assert!(matches!(err, EmitterError::Store(_)), "got {err:?}");
    }

    #[test]
    fn content_hash_is_deterministic_for_identical_event() {
        let id = Uuid::now_v7();
        let tenant = Uuid::now_v7();
        let a = OutboxEmitter::<InMemoryAuditOutboxWriter>::build_row(&sample(id, tenant)).unwrap();
        let b = OutboxEmitter::<InMemoryAuditOutboxWriter>::build_row(&sample(id, tenant)).unwrap();
        // Identical events → identical canonical digest (RFC 8785 determinism)
        // → a retried emit is exactly-once on the idempotency key + hash.
        assert_eq!(a.content_hash, b.content_hash);
        assert_eq!(a.event_id, b.event_id);
    }

    #[test]
    fn distinct_events_differ_in_hash() {
        let tenant = Uuid::now_v7();
        let a = OutboxEmitter::<InMemoryAuditOutboxWriter>::build_row(&sample(
            Uuid::now_v7(),
            tenant,
        ))
        .unwrap();
        let b = OutboxEmitter::<InMemoryAuditOutboxWriter>::build_row(&sample(
            Uuid::now_v7(),
            tenant,
        ))
        .unwrap();
        // Different CloudEvents id ⇒ different canonical bytes ⇒ different hash.
        assert_ne!(a.content_hash, b.content_hash);
    }

    #[test]
    fn emitter_is_dyn_object_safe() {
        // The production wiring stores it as `Arc<dyn Emitter>`.
        let emitter: Arc<dyn Emitter> =
            Arc::new(OutboxEmitter::new(Arc::new(InMemoryAuditOutboxWriter::new())));
        emitter.emit(sample(Uuid::now_v7(), Uuid::now_v7())).unwrap();
    }
}
