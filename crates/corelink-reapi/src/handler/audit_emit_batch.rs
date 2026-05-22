//! Slot-aware batch audit emitters + the FindMissingBlobs batch-summary
//! line. Extracted from the monolithic `handler.rs` per Wave 33 Stream
//! A2.1c file-size discipline so the non-slot emitters (used by the
//! streaming + HTTP read paths) live in [`super::audit_emit`] and the
//! `*_at_slot` variants (used by the batch handlers) live here.
//!
//! The `slot_idx` token is mixed into the deterministic audit `id`
//! derivation per codex round-3 P2 SEAL fix — per-slot duplicates of the
//! SAME digest in a single `BatchReadBlobs` request emit distinct CE
//! `id`s so `audit_outbox` PK collisions are structurally impossible.

use corelink_hash::Digest;
use corelink_worker::{Region, TenantCtx as StorageTenantCtx};
use uuid::Uuid;

use crate::read::MissReason;

use super::helpers::region_str;

/// Slot-aware variant of
/// [`super::audit_emit::deterministic_audit_id`] for batch handlers
/// (codex round-3 P2 SEAL fix). Mixes a `slot_idx` token into the hashed
/// material so per-slot duplicates of the same digest within a single
/// request emit distinct CE `id`s — preventing `audit_outbox` PK
/// collisions when a Bazel client batches the same digest twice in a
/// `BatchReadBlobs` request. The non-slot variant remains the canonical
/// id for non-batch handlers (ByteStream::Read, HTTP read), which only
/// see at most one event per (request_id, digest) pair.
pub(super) fn deterministic_audit_id_at_slot(
    client_request_id: &str,
    tenant: Uuid,
    digest: &Digest,
    slot_idx: usize,
) -> Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"corelink-reapi-audit-id-v2-slot\0");
    hasher.update(client_request_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(tenant.as_bytes());
    hasher.update(b"\0");
    hasher.update(digest.to_hex().as_bytes());
    hasher.update(b"\0");
    hasher.update(&(slot_idx as u64).to_le_bytes());
    let out = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&out.as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0F) | 0x70;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    Uuid::from_bytes(bytes)
}

/// Slot-aware variant of
/// [`super::audit_emit::emit_read_completed_audit_post_stream`] for batch
/// handlers (codex round-3 P2 SEAL fix). The audit envelope `id` mixes
/// the `slot_idx` so per-slot duplicates of the same digest in a single
/// `BatchReadBlobs` request do NOT collide on the `audit_outbox` PK.
#[allow(
    clippy::too_many_arguments,
    reason = "audit emitter aggregates many forensic fields; bundling adds noise without reuse"
)]
pub(super) fn emit_read_completed_audit_post_stream_at_slot(
    tenant_id: Uuid,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    size_bytes: u64,
    payload_len: u64,
    bytes_sent: u64,
    total_chunks: usize,
    slot_idx: usize,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id =
        deterministic_audit_id_at_slot(client_request_id, tenant_id, digest, slot_idx);
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
            slot_idx = slot_idx,
            envelope = %json,
            "CAS read completed (batch slot)"
        );
    }
}

/// Slot-aware variant of [`super::audit_emit::emit_read_miss_audit`] for
/// batch handlers (codex round-3 P2 SEAL fix). The audit envelope `id`
/// mixes the `slot_idx` to keep per-slot CE ids unique even when the same
/// digest appears multiple times in the request.
pub(super) fn emit_read_miss_audit_at_slot(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    digest: &Digest,
    reason: MissReason,
    slot_idx: usize,
) {
    let canonical = format!("blake3:{}", digest.to_hex());
    let envelope_id = deterministic_audit_id_at_slot(
        client_request_id,
        storage_ctx.tenant_id(),
        digest,
        slot_idx,
    );
    let envelope = match reason {
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
            tracing::info!(
                target: "corelink.audit",
                event_type = event_type,
                reason = "never_existed_or_cross_tenant",
                tenant = %storage_ctx.tenant_id(),
                principal = %principal_id,
                region = %region_str(region),
                digest = %digest.to_hex(),
                slot_idx = slot_idx,
                envelope = %json,
                "CAS batch-read miss (uniform 404 per ADR-0028; classification deferred to S-09 chain consumer)"
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
                slot_idx = slot_idx,
                envelope = %json,
                "CAS batch-read of tombstoned blob (uniform 404)"
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
                slot_idx = slot_idx,
                envelope = %json,
                "CAS batch-read encountered R2 orphan row (GC reconcile pending)"
            );
        }
    }
}

/// Emit a single batch-summary audit line for a `FindMissingBlobs`
/// invocation. WI-S02-002 §6.1.5 mandates batch-level audit (NOT
/// per-digest CE envelopes) so the audit chain is not flooded by
/// 1000-digest cache-miss probes.
///
/// The line goes to `corelink.audit` at `info!` level; SRE
/// dashboards can trend `(missing / total)` ratio per tenant for
/// regression monitoring.
#[allow(
    clippy::too_many_arguments,
    reason = "audit emitter aggregates many forensic fields; bundling adds noise without reuse"
)]
pub(super) fn emit_find_missing_batch_audit(
    storage_ctx: &StorageTenantCtx,
    principal_id: Uuid,
    region: Region,
    client_request_id: &str,
    requested: usize,
    missing: usize,
    d1_lookups: usize,
) {
    tracing::info!(
        target: "corelink.audit",
        event_type = "corelink.cas.find_missing_batch",
        tenant = %storage_ctx.tenant_id(),
        principal = %principal_id,
        region = %region_str(region),
        request_id = %client_request_id,
        requested = requested,
        missing = missing,
        d1_lookups = d1_lookups,
        "FindMissingBlobs batch processed"
    );
}
