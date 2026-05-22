//! Single-shot audit emitters for the streaming `ByteStream::Read` +
//! HTTP read paths + deterministic audit-id derivation.
//!
//! Extracted from the monolithic `handler.rs` per Wave 33 Stream A2.1c
//! file-size discipline. The slot-aware `*_at_slot` batch variants live
//! in the sibling [`super::audit_emit_batch`] module.
//!
//! ## Audit-id derivation
//!
//! Both the non-slot `deterministic_audit_id` and the slot-aware
//! `deterministic_audit_id_at_slot` (in [`super::audit_emit_batch`]) live
//! here because the slot variant prepends the non-slot domain and the two
//! functions share the BLAKE3-derived UUIDv7 stamping discipline.

use corelink_hash::Digest;
use corelink_worker::{Region, TenantCtx as StorageTenantCtx};
use uuid::Uuid;

use crate::read::MissReason;

use super::helpers::region_str;

/// Mint a deterministic UUIDv7-shaped id from
/// `(client_request_id, tenant_uuid, digest)`. Two retries with the same
/// triple produce the same id, which makes the `audit_outbox.id` PK
/// stable across retries.
///
/// Tenant is included in the input so that two distinct tenants reusing
/// the SAME `(client_request_id, digest)` pair cannot collide on the
/// `audit_outbox.id` PK — closing the cross-tenant collision risk codex
/// round-2 surfaced as a High issue. The same shape is used by
/// `audit_request_id_for_blob` so the PK + `(request_id, event_type)`
/// dedup key are consistently tenant-scoped.
///
/// Implementation: BLAKE3-derived; we lift the first 16 bytes and stamp
/// the UUIDv7 version + variant nibbles so the resulting bytes parse as a
/// well-formed UUIDv7 (`version=7`, `variant=10b`). The "timestamp"
/// portion is content-derived, not wall-clock; the real ingest timestamp
/// lives in `audit_outbox.enqueued_at` (`request.now_ms`). UUIDv7's own
/// timestamp prefix is a sortability hint for D1 indexing, and a
/// content-derived "stamp" preserves sort stability per request without
/// varying with wall-clock between retries.
pub(super) fn deterministic_audit_id(
    client_request_id: &str,
    tenant: Uuid,
    digest: &Digest,
) -> Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"corelink-reapi-audit-id-v2\0");
    hasher.update(client_request_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(tenant.as_bytes());
    hasher.update(b"\0");
    hasher.update(digest.to_hex().as_bytes());
    let out = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&out.as_bytes()[..16]);
    // UUIDv7 markings: version 0b0111 in byte 6 high nibble; variant 0b10
    // in byte 8 high two bits.
    bytes[6] = (bytes[6] & 0x0F) | 0x70;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Public re-export of the read-completed audit emitter (module-private
/// `emit_read_completed_audit_post_stream`) for the HTTP read handler
/// in [`crate::http_read`]. Same shape; the cross-module helper keeps
/// audit envelopes byte-identical between gRPC and HTTP transports
/// (codex round-1 P1(b) parity fix).
#[allow(
    clippy::too_many_arguments,
    reason = "shared cross-module audit emitter; bundling args adds a struct noise without reuse"
)]
pub fn emit_read_completed_audit_pub(
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    size_bytes: u64,
    payload_len: u64,
    bytes_sent: u64,
    total_chunks: usize,
) {
    emit_read_completed_audit_post_stream(
        tenant_id,
        principal_id,
        region,
        client_request_id,
        digest,
        size_bytes,
        payload_len,
        bytes_sent,
        total_chunks,
    );
}

/// Public re-export of the read-miss audit emitter (module-private
/// `emit_read_miss_audit`) for the HTTP read handler in
/// [`crate::http_read`]. Same shape; ensures HTTP and gRPC 404 paths
/// emit byte-identical envelopes (codex round-1 P1(b) parity fix).
pub fn emit_read_miss_audit_pub(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    reason: MissReason,
) {
    emit_read_miss_audit(
        storage_ctx,
        principal_id,
        region,
        client_request_id,
        digest,
        reason,
    );
}

/// Emit the canonical `corelink.cas.read_completed` audit envelope on
/// a successful FULL stream completion.
///
/// `bytes_sent` is the actually-delivered byte count (post
/// `read_offset`/`read_limit`); when `bytes_sent < payload_len` we
/// emit a partial-read marker (codex P2(a) fix — partial reads no
/// longer audit as full reads).
#[allow(
    clippy::too_many_arguments,
    reason = "audit emitter aggregates many forensic fields; bundling adds noise without reuse"
)]
pub(super) fn emit_read_completed_audit_post_stream(
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    size_bytes: u64,
    payload_len: u64,
    bytes_sent: u64,
    total_chunks: usize,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id = deterministic_audit_id(client_request_id, tenant_id, digest);
    // Codex round-2 P3 fix: the envelope's `data.size_bytes` field is
    // the canonical blob_meta-recorded body length per the audit
    // contract (matches the write-side `read_completed` semantics in
    // CTRL-AUDIT-003). The actually-delivered byte count travels
    // alongside as a structured tracing field (`bytes_sent`) and into
    // the audit chain (S-09) via a separate column once the
    // out-of-band event surface ships.
    let envelope = crate::audit::AuditEnvelopeBuilder::read_completed(
        crate::audit::AuditPrincipal {
            tenant_id,
            principal_id,
            region: region_str(region),
        },
        client_request_id,
        canonical,
        size_bytes,
        crate::audit::AuditTime { envelope_id },
    )
    .build();
    if let Ok(json) = envelope.to_json() {
        tracing::info!(
            target: "corelink.audit",
            event_type = crate::audit::REAPI_READ_COMPLETED,
            tenant = %tenant_id,
            principal = %principal_id,
            region = %region_str(region),
            digest = %digest.to_hex(),
            size_bytes = size_bytes,
            payload_len = payload_len,
            bytes_sent = bytes_sent,
            total_chunks = total_chunks,
            envelope = %json,
            "CAS read completed"
        );
    }
}

/// Emit a per-MissReason audit envelope on a 404 path. Per ADR-0028
/// the disambiguation lives here, never on the wire.
///
/// Codex round-1 P2(b) + round-4 P2 fix: each [`MissReason`] now emits
/// a DISTINCT CE event type so the SEV-1 cross-tenant alert is no
/// longer poisoned by ordinary cache misses or tombstone reads.
///
/// Severity matrix (per WI §11 + ADR-0028):
/// - [`MissReason::NeverExisted`] → `info!` + [`crate::audit::REAPI_READ_MISS`].
///   The conflated NeverExisted/CrossTenantMasked arm cannot be
///   disambiguated at the read-side without a side-channel oracle;
///   the S-09 chain consumer reclassifies to `cross_tenant_attempt`
///   (SEV-1) using the offline global digest index.
/// - [`MissReason::Tombstoned`] → `info!` +
///   [`crate::audit::REAPI_TOMBSTONED_READ_ATTEMPT`]. Legitimate
///   post-GC read.
/// - [`MissReason::R2OrphanRow`] → `error!` +
///   [`crate::audit::REAPI_R2_ORPHAN_DETECTED`]. SEV-2 maps to the
///   GC reconcile signal.
pub(super) fn emit_read_miss_audit(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    reason: MissReason,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id = deterministic_audit_id(client_request_id, storage_ctx.tenant_id(), digest);
    let envelope = match reason {
        // Codex round-4 P2 fix: emit the low-severity `read_miss`
        // event type for the conflated NeverExisted/CrossTenantMasked
        // arm. The S-09 chain consumer reclassifies to
        // `cross_tenant_attempt` (SEV-1) using the offline global
        // digest index when applicable; the read-side handler can't
        // disambiguate without a side-channel oracle and therefore
        // does NOT trip the SEV-1 alert here.
        MissReason::NeverExisted => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_READ_MISS,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
        MissReason::Tombstoned => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_TOMBSTONED_READ_ATTEMPT,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
        MissReason::R2OrphanRow => crate::audit::AuditEnvelope {
            specversion: "1.0",
            id: envelope_id,
            source: format!("corelink://tenant/{}", storage_ctx.tenant_id()),
            r#type: crate::audit::REAPI_R2_ORPHAN_DETECTED,
            subject: canonical.clone(),
            datacontenttype: "application/json",
            data: crate::audit::AuditEnvelopeData {
                tenant_id: storage_ctx.tenant_id(),
                principal_id,
                digest: canonical.clone(),
                size_bytes: 0,
                region: region_str(region),
                request_id: client_request_id.to_owned(),
            },
        },
    };
    let json = envelope.to_json().unwrap_or_default();
    let event_type = envelope.r#type;
    match reason {
        MissReason::NeverExisted => {
            // Severity is `info!` — the conflated NeverExisted /
            // CrossTenantMasked arm is high-volume by definition
            // (every Bazel/Buck2 cache-miss probe surfaces here). The
            // S-09 chain consumer reclassifies to `warn!` /
            // `cross_tenant_attempt` SEV-1 only when its global digest
            // index confirms cross-tenant ownership; the read-side
            // handler stays silent on the alert lane (codex round-4
            // P2 fix).
            tracing::info!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "never_existed_or_cross_tenant",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                envelope = %json,
                "CAS read miss (uniform 404 per ADR-0028; classification deferred to S-09 chain consumer)"
            );
        }
        MissReason::Tombstoned => {
            tracing::info!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "tombstoned",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                envelope = %json,
                "CAS read of tombstoned blob (uniform 404)"
            );
        }
        MissReason::R2OrphanRow => {
            tracing::error!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "r2_orphan_row",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                envelope = %json,
                "CAS read encountered R2 orphan row (GC reconcile pending)"
            );
        }
    }
}
