//! `ByteStream::Read` chunked-stream helpers + Drop-time fallback audit
//! envelope. Extracted from the monolithic `handler.rs` per Wave 33
//! Stream A2.1c file-size discipline so [`super::bytestream`] stays
//! within the L2.10 hard cap.
//!
//! The stream is wrapped in a [`ReadCompleteAuditGuard`] Drop guard so
//! the `corelink.cas.read_completed` envelope fires regardless of the
//! completion path (success → terminal `None`; cancellation → guard
//! `Drop`). The guard checks the running `bytes_sent` atomic at emit
//! time so partial reads audit accurately (codex round-1 P2(a) fix).

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_replication::region_resolver::Region;
use tonic::Status;
use uuid::Uuid;

use crate::proto::bytestream::ReadResponse;

use super::audit_emit::emit_read_completed_audit_post_stream;
use super::helpers::region_str;
use super::READ_CHUNK_SIZE_BYTES;

/// Audit emission tail bundle. Captured by
/// [`chunked_read_stream_with_audit`] so the read_completed envelope is
/// emitted **post-stream-completion**, with the actually-delivered
/// `bytes_sent` count — partial reads (`read_offset`/`read_limit`) and
/// client disconnects therefore audit accurately, not as full reads.
/// Codex round-1 P2(a) fix.
pub(super) struct ReadAuditTail {
    pub(super) tenant_id: Uuid,
    pub(super) principal_id: Uuid,
    pub(super) region: Region,
    pub(super) client_request_id: String,
    pub(super) digest: Digest,
    /// blob_meta-recorded full body size (from D1 row).
    pub(super) size_bytes: u64,
    /// Length of the payload we actually slice into chunks (post
    /// `read_offset` + `read_limit`). Travels into the audit envelope
    /// alongside `size_bytes` so SIEM can spot partial-read patterns.
    pub(super) payload_len: u64,
}

/// Build a `ReadStream` of `ReadResponse` chunks at `READ_CHUNK_SIZE_BYTES`
/// granularity AND emit the `corelink.cas.read_completed` audit
/// envelope after the LAST chunk has been polled (so a client
/// disconnect MID-stream audits as a partial / aborted read, not a
/// successful full read).
///
/// The audit emission is wrapped via `futures::stream::unfold` so the
/// terminal "emit the audit envelope" branch fires when the underlying
/// chunk iterator has yielded its last frame. If the stream is dropped
/// before completion (client cancellation), the inner state's `Drop`
/// path emits a structured `tracing::warn!` for SRE forensics.
///
/// Empty body case: REAPI v2 §"reading from CAS" allows reading the
/// canonical empty blob (BLAKE3 of `b""`); we emit a single empty
/// `ReadResponse{data: []}` followed by end-of-stream so clients see
/// a non-empty stream they can collect with the same code path.
pub(super) fn chunked_read_stream_with_audit(
    body: Bytes,
    audit: ReadAuditTail,
) -> futures::stream::BoxStream<'static, Result<ReadResponse, Status>> {
    use futures::StreamExt;
    let chunks = chunk_bytes_for_read(body, READ_CHUNK_SIZE_BYTES);
    let total_chunks = chunks.len();

    // Convert chunks to `Vec<u8>` once so the iterator is `'static`.
    let frames: Vec<ReadResponse> = chunks
        .into_iter()
        .map(|c| ReadResponse { data: c.to_vec() })
        .collect();

    // State for the unfold: (frames_remaining_iter, bytes_sent, audit).
    let bytes_sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let bytes_sent_for_unfold = std::sync::Arc::clone(&bytes_sent);

    let frames_iter = frames.into_iter();
    let unfold_state = (frames_iter, bytes_sent_for_unfold);

    let stream = futures::stream::unfold(unfold_state, |(mut iter, bytes_sent)| async move {
        match iter.next() {
            Some(frame) => {
                bytes_sent.fetch_add(frame.data.len() as u64, std::sync::atomic::Ordering::AcqRel);
                Some((Ok(frame), (iter, bytes_sent)))
            }
            None => None,
        }
    });

    // Wrap in a `ReadCompleteAudit` Drop guard so the audit envelope is
    // emitted regardless of completion path (success → terminal None;
    // cancellation → guard Drop). The guard checks `bytes_sent` at
    // emission time so partial reads are audited correctly.
    let guard = ReadCompleteAuditGuard {
        audit,
        bytes_sent,
        total_chunks,
        emitted: false,
    };
    let stream_with_guard = futures::stream::unfold(
        (
            Box::pin(stream)
                as std::pin::Pin<
                    Box<dyn futures::Stream<Item = Result<ReadResponse, Status>> + Send>,
                >,
            guard,
        ),
        |(mut s, mut guard)| async move {
            match s.as_mut().next().await {
                Some(item) => Some((item, (s, guard))),
                None => {
                    // Terminal branch — emit the read_completed envelope
                    // with the actual bytes_sent count.
                    guard.emit_completion();
                    None
                }
            }
        },
    );
    stream_with_guard.boxed()
}

/// Drop-time fallback: client cancelled the stream before the last
/// chunk was polled. Emit a warn-line (no info-level "completed")
/// so SIEM can distinguish abandoned vs completed reads.
struct ReadCompleteAuditGuard {
    audit: ReadAuditTail,
    bytes_sent: std::sync::Arc<std::sync::atomic::AtomicU64>,
    total_chunks: usize,
    emitted: bool,
}

impl ReadCompleteAuditGuard {
    fn emit_completion(&mut self) {
        if self.emitted {
            return;
        }
        self.emitted = true;
        let bytes_sent = self.bytes_sent.load(std::sync::atomic::Ordering::Acquire);
        emit_read_completed_audit_post_stream(
            self.audit.tenant_id,
            self.audit.principal_id,
            self.audit.region,
            &self.audit.client_request_id,
            &self.audit.digest,
            self.audit.size_bytes,
            self.audit.payload_len,
            bytes_sent,
            self.total_chunks,
        );
    }
}

impl Drop for ReadCompleteAuditGuard {
    fn drop(&mut self) {
        if self.emitted {
            return;
        }
        let bytes_sent = self.bytes_sent.load(std::sync::atomic::Ordering::Acquire);
        // Cancellation path — partial / aborted read. Distinct event
        // type so dashboards can graph cancellation rate separately.
        tracing::warn!(
            target: "corelink.audit",
            event_type = "corelink.cas.read_aborted",
            tenant = %self.audit.tenant_id,
            principal = %self.audit.principal_id,
            region = %region_str(self.audit.region),
            digest = %self.audit.digest.to_hex(),
            size_bytes = self.audit.size_bytes,
            payload_len = self.audit.payload_len,
            bytes_sent = bytes_sent,
            "CAS read aborted (client cancelled stream before completion)"
        );
    }
}

/// Split a `Bytes` body into a `Vec<Bytes>` of chunks at `chunk_size`
/// granularity. Empty body → `vec![Bytes::new()]` (one empty frame for
/// REAPI conformance — clients expect at least one ReadResponse).
fn chunk_bytes_for_read(body: Bytes, chunk_size: usize) -> Vec<Bytes> {
    if chunk_size == 0 {
        return vec![body];
    }
    if body.is_empty() {
        return vec![Bytes::new()];
    }
    let mut out = Vec::with_capacity(body.len().div_ceil(chunk_size));
    let mut start = 0usize;
    while start < body.len() {
        let end = start.saturating_add(chunk_size).min(body.len());
        out.push(body.slice(start..end));
        start = end;
    }
    out
}
