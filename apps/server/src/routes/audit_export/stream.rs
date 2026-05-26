//! Wave-19 true async page-by-page streaming generator + R2-list pager
//! abstraction + mid-stream chain-break / trailer machinery for the
//! `/v1/audit/export` route.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Verbatim move of `R2ListPager` trait, `InMemoryR2ListPager`,
//! `build_audit_export_async_stream`, `emit_mid_stream_break_audit`,
//! `abort_trailer_frame`, `mid_stream_abort_trailer_value`, and
//! `export_row_buffer_bytes`.

#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::sync::Arc;

use async_stream::stream;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use bytes::Bytes;
use corelink_audit_chain::{verify_inclusion_proof, ChainHash, ExportedAuditEvent};
use futures::Stream;
use http_body::Frame;
use uuid::Uuid;

use super::audit_sink::ExportAuditSink;
use super::types::{
    ExportAuditRow, DEFAULT_EXPORT_ROW_BUFFER_BYTES, ENV_EXPORT_ROW_BUFFER_BYTES,
    EVENT_TYPE_VERIFY_FAILED, EXIT_STATUS_VERIFY_FAILED_MID_STREAM, HEADER_EXPORT_ABORTED,
    R2_LIST_PAGE_SIZE,
};

/// Wave-19 — paginated R2 list source the audit-export streaming
/// generator drives one page at a time. The trait surface decouples
/// the route from the underlying R2 binding so:
///
/// - Native unit + integration tests pass [`InMemoryR2ListPager`]
///   (no network).
/// - Production wires a CF Worker R2 binding adapter implementing
///   `next_page` against the real
///   `GET /accounts/{account_id}/r2/buckets/{bucket}/objects?prefix=...&cursor=...`
///   CF API v4 paginated walk (wave-17 + wave-18 pattern lifted into
///   the customer-facing export path).
///
/// The trait is async + sealed by `Send + Sync` so the generator can
/// `.await` page boundaries inside a `tokio::spawn`-free body stream
/// (charter forbids `tokio::spawn` in src; the generator runs on the
/// axum body-poll task).
///
/// # Page contract
///
/// `next_page` returns at most [`R2_LIST_PAGE_SIZE`] rows per call.
/// `None` signals end-of-stream (the generator then flushes the
/// trailing manifest line and closes the body). Errors are surfaced
/// as an empty page (production wiring fail-CLOSED) — the daily-verify
/// retention cron is the redundancy net so an export aborting on R2
/// list 5xx is acceptable per the audit doc §4 wave-19 row.
#[async_trait::async_trait]
pub trait R2ListPager: Send + Sync + core::fmt::Debug {
    /// Fetch the next page of rows. `None` signals end-of-stream.
    /// The returned `Vec<ExportedAuditEvent>` length MUST be in
    /// `0..=R2_LIST_PAGE_SIZE` — over-budget pages are an
    /// implementation bug (the property test pins this).
    async fn next_page(&mut self) -> Option<Vec<ExportedAuditEvent>>;
}

/// In-memory [`R2ListPager`] fake. Slices a `Vec<ExportedAuditEvent>`
/// into pages of at most `page_size` rows. Used by every unit +
/// integration test in this module so the wave-19 streaming generator
/// can be driven without a real R2 binding.
#[derive(Debug)]
pub struct InMemoryR2ListPager {
    /// Remaining rows; consumed front-to-back via `split_off`.
    rows: Vec<ExportedAuditEvent>,
    /// Per-page key budget (defaults to [`R2_LIST_PAGE_SIZE`]).
    pub(super) page_size: usize,
    /// Whether `next_page` has been polled at least once after the
    /// final page was returned — used to honour the `None`-terminator
    /// contract.
    exhausted: bool,
}

impl InMemoryR2ListPager {
    /// Construct from a row buffer + page size. A `page_size` of `0`
    /// is silently clamped to [`R2_LIST_PAGE_SIZE`] so a misconfigured
    /// caller cannot construct a pager that never makes progress
    /// (fail-CLOSED — better to serve at the default than infinite
    /// loop the export task).
    #[must_use]
    pub fn with_rows(rows: Vec<ExportedAuditEvent>, page_size: usize) -> Self {
        let page_size = if page_size == 0 {
            R2_LIST_PAGE_SIZE
        } else {
            page_size
        };
        Self {
            rows,
            page_size,
            exhausted: false,
        }
    }
}

#[async_trait::async_trait]
impl R2ListPager for InMemoryR2ListPager {
    async fn next_page(&mut self) -> Option<Vec<ExportedAuditEvent>> {
        if self.exhausted {
            return None;
        }
        if self.rows.is_empty() {
            // First poll on an empty source still returns `Some(vec![])`
            // so the generator emits the manifest line on the
            // happy-empty-range path. Subsequent polls return `None`.
            self.exhausted = true;
            return Some(Vec::new());
        }
        let take = self.page_size.min(self.rows.len());
        // `split_off(take)` returns the TAIL — we want the HEAD, so
        // swap the two slices.
        let tail = self.rows.split_off(take);
        let head = core::mem::replace(&mut self.rows, tail);
        if self.rows.is_empty() {
            self.exhausted = true;
        }
        Some(head)
    }
}

/// Wave-19 true async page-by-page generator backing the audit-export
/// streaming response body. Replaces the wave-18
/// pre-materialized `Vec<Frame<Bytes>>` plan.
///
/// The generator:
///
/// 1. Polls [`R2ListPager::next_page`] for one page of rows.
/// 2. For each row in the page: runs `verify_inclusion_proof` against
///    the manifest anchor. On the FIRST verify failure (or row
///    serialize failure) it:
///    - emits the SEV-0 mid-stream-break audit row via
///      [`emit_mid_stream_break_audit`] (audit-anchor-BEFORE-trailer
///      invariant per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`),
///    - yields the canonical `Frame::trailers` carrying
///      [`HEADER_EXPORT_ABORTED`], then closes the stream (no further
///      frames).
/// 3. On a clean page: yields one `Frame::data` per row containing
///    `<row-json>\n` bytes. The `\n` suffix mirrors the wave-16
///    monolithic wire shape byte-for-byte.
/// 4. After the final page (`next_page` returns `None`): yields the
///    trailing manifest line as a single `Frame::data` (no `\n`
///    suffix — matches the wave-16 wire shape).
///
/// # Memory bound
///
/// At any point, exactly ONE page of rows + ONE row's serialized
/// bytes are live; the generator parks on `yield` and never reads
/// ahead. The CF Worker R2 binding adapter that fills the pager in
/// production allocates one CF API v4 response body per page; that
/// body is dropped before the next page is fetched. Per-row
/// serialized bytes are bounded by the customer's audit event
/// shape; the [`ENV_EXPORT_ROW_BUFFER_BYTES`] env-var caps the
/// soft-allocation hint (the row buffer `Vec::with_capacity`).
///
/// # Back-pressure
///
/// `async_stream::stream!` expands to a `Stream` whose
/// `poll_next` parks the generator on each `yield`. axum's
/// `StreamBody` polls one frame at a time; the generator therefore
/// fetches the next page ONLY when the consumer (the wire) is
/// ready for it. There is no buffer-ahead of multiple pages.
// Wave-20 (A-P3-01 closure): the 9-arg signature is a deliberate compromise
// between (a) one private context struct that would couple the stream-builder
// to a particular wiring shape and (b) the current explicit-arg surface that
// keeps the function call-site self-documenting at the route handler boundary.
// A `StreamBuildContext { ... }` refactor is tracked as a follow-on cleanup
// (cosmetic only; no behavioral change).
#[allow(clippy::too_many_arguments, reason = "trailing-payload + audit sink fan-in")]
pub fn build_audit_export_async_stream(
    mut pager: Box<dyn R2ListPager>,
    manifest_line: String,
    anchor_head: ChainHash,
    audit_sink: Arc<dyn ExportAuditSink>,
    authenticated_tenant: Uuid,
    from_ms: u64,
    to_ms: u64,
    bytes_written: u64,
    events_written: u64,
) -> impl Stream<Item = Result<Frame<Bytes>, Infallible>> + Send {
    let row_capacity_hint = export_row_buffer_bytes();
    stream! {
        // Global row index across pages — used as `break_at_chunk` so
        // the customer-CLI diagnostic reads the same as wave-18 (chunk
        // numbering is contiguous across page boundaries).
        let mut global_idx: u64 = 0;
        loop {
            let Some(page) = pager.next_page().await else {
                // End-of-stream. Yield the trailing manifest line and
                // close the body. The `\n`-suffix discipline (rows
                // append `\n`; manifest does NOT) is wave-18 carried
                // through verbatim.
                yield Ok(Frame::data(Bytes::from(manifest_line)));
                return;
            };
            for row in &page {
                let row_serialized = match serde_json::to_string(row) {
                    Ok(s) => s,
                    Err(_) => {
                        // Serialize-failure path — audit-anchor-BEFORE-trailer.
                        // Wave-20 (A-P1-03): on audit-emit failure we
                        // force-close the body WITHOUT yielding the
                        // trailer (see `emit_mid_stream_break_audit`
                        // doc-comment for the trade-off note).
                        if emit_mid_stream_break_audit(
                            &audit_sink,
                            authenticated_tenant,
                            row.event.sequence_number,
                            global_idx,
                            "serialize_failed",
                            "row-serialize-failed",
                            from_ms,
                            to_ms,
                            bytes_written,
                            events_written,
                        )
                        .is_err()
                        {
                            tracing::error!(
                                target: "corelink.audit.export",
                                event = "audit_export_mid_stream_emit_failed",
                                severity = "SEV-0",
                                tenant = %authenticated_tenant,
                                break_at_seq = row.event.sequence_number,
                                break_at_chunk = global_idx,
                                observed = "serialize_failed",
                                expected = "row-serialize-failed",
                                "audit-sink failed during mid-stream serialize-failure emit; \
                                 force-closing body without abort trailer (wave-20 A-P1-03 \
                                 trade-off: missing trailer is louder than missing anchor)"
                            );
                            return;
                        }
                        yield Ok(abort_trailer_frame(
                            row.event.sequence_number,
                            global_idx,
                            "serialize_failed",
                            "row-serialize-failed",
                        ));
                        return;
                    }
                };
                let verified = verify_inclusion_proof(row, &anchor_head).unwrap_or_default();
                if !verified {
                    // Chain-break path — audit-anchor-BEFORE-trailer
                    // (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER; the wave-19
                    // proptest pins this ordering at 10k iter).
                    // Wave-20 (A-P1-03): on audit-emit failure we force-
                    // close the body WITHOUT yielding the trailer (see
                    // `emit_mid_stream_break_audit` doc-comment).
                    let observed_hex = row.proof.link_hash.to_hex();
                    let expected_hex = anchor_head.to_hex();
                    if emit_mid_stream_break_audit(
                        &audit_sink,
                        authenticated_tenant,
                        row.event.sequence_number,
                        global_idx,
                        &observed_hex,
                        &expected_hex,
                        from_ms,
                        to_ms,
                        bytes_written,
                        events_written,
                    )
                    .is_err()
                    {
                        tracing::error!(
                            target: "corelink.audit.export",
                            event = "audit_export_mid_stream_emit_failed",
                            severity = "SEV-0",
                            tenant = %authenticated_tenant,
                            break_at_seq = row.event.sequence_number,
                            break_at_chunk = global_idx,
                            observed = %observed_hex,
                            expected = %expected_hex,
                            "audit-sink failed during mid-stream chain-break emit; \
                             force-closing body without abort trailer (wave-20 A-P1-03 \
                             trade-off: missing trailer is louder than missing anchor)"
                        );
                        return;
                    }
                    yield Ok(abort_trailer_frame(
                        row.event.sequence_number,
                        global_idx,
                        &observed_hex,
                        &expected_hex,
                    ));
                    return;
                }
                // Happy-row: yield `<row-json>\n` as a single data
                // frame. Row buffer capacity hint comes from
                // `EXPORT_ROW_BUFFER_BYTES` (clamped to the row size
                // + 1 if smaller so we never allocate less than we
                // need).
                let capacity = row_capacity_hint.max(row_serialized.len() + 1);
                let mut buf: Vec<u8> = Vec::with_capacity(capacity);
                buf.extend_from_slice(row_serialized.as_bytes());
                buf.push(b'\n');
                yield Ok(Frame::data(Bytes::from(buf)));
                global_idx = global_idx.saturating_add(1);
            }
        }
    }
}

/// Parse [`ENV_EXPORT_ROW_BUFFER_BYTES`] returning the canonical
/// default on absence / malformed input. The value is the
/// `Vec::with_capacity` hint for each row's body buffer; the actual
/// allocation grows to fit the row if it exceeds the hint.
#[must_use]
fn export_row_buffer_bytes() -> usize {
    match std::env::var(ENV_EXPORT_ROW_BUFFER_BYTES) {
        Ok(v) => v.trim().parse::<usize>().unwrap_or(DEFAULT_EXPORT_ROW_BUFFER_BYTES),
        Err(_) => DEFAULT_EXPORT_ROW_BUFFER_BYTES,
    }
}

/// Emit the SEV-0 mid-stream chain-break audit row. The structured
/// `{break_at_seq, break_at_chunk, observed, expected}` payload is
/// carried on the wave-19 [`ExportAuditRow::payload`] field;
/// `exit_status` is the stable [`EXIT_STATUS_VERIFY_FAILED_MID_STREAM`]
/// enum (no colon-prefix encoding).
///
/// Wave-20 — returns `Result<(), &'static str>` (A-P1-03 closure). The
/// route handler has already flushed response headers by the time the
/// async stream generator polls a row, so an audit-emit failure cannot
/// be surfaced as a 503. The caller (the `stream!` generator inside
/// [`build_audit_export_async_stream`]) handles `Err` by:
///   1. emitting a `tracing::error!` at SEV-0 carrying the would-be
///      audit row + sink-error string,
///   2. force-closing the body WITHOUT yielding the
///      `Frame::trailers` abort trailer — the customer-CLI sees a
///      truncated body (the loudest possible signal short of a 503).
///
/// The trade-off is documented in the module-level "Wave-20 emit-
/// discipline lift" doc-block: a silent abort-trailer with a missing
/// audit row would let the security team's detect surface miss the
/// event entirely; a truncated body forces the CLI verifier to flag
/// a chain-broken export.
#[allow(clippy::too_many_arguments, reason = "audit row shape")]
fn emit_mid_stream_break_audit(
    sink: &Arc<dyn ExportAuditSink>,
    authenticated_tenant: Uuid,
    break_at_seq: u64,
    break_at_chunk: u64,
    observed_hex: &str,
    expected_hex: &str,
    from_ms: u64,
    to_ms: u64,
    bytes_written: u64,
    events_written: u64,
) -> Result<(), &'static str> {
    let payload = serde_json::json!({
        "break_at_seq": break_at_seq,
        "break_at_chunk": break_at_chunk,
        "observed": observed_hex,
        "expected": expected_hex,
    });
    sink.emit(ExportAuditRow {
        event_type: EVENT_TYPE_VERIFY_FAILED.to_string(),
        authenticated_tenant: Some(authenticated_tenant),
        attempted_tenant: None,
        from_ms,
        to_ms,
        bytes_written,
        events_written,
        exit_status: EXIT_STATUS_VERIFY_FAILED_MID_STREAM.to_string(),
        payload: Some(payload),
    })
}

/// Build the canonical mid-stream abort HTTP-trailer frame. Payload
/// is a single ASCII JSON object value placed in the
/// `X-CoreLink-Audit-Export-Aborted` trailer header — the
/// customer-CLI parses it for the operator diagnostic. We expose the
/// shape through `mid_stream_abort_trailer_value` so the wave-18
/// integration test can assert the canonical payload byte-for-byte
/// without re-implementing the formatter.
fn abort_trailer_frame(
    break_at_seq: u64,
    break_at_chunk: u64,
    observed_hex: &str,
    expected_hex: &str,
) -> Frame<Bytes> {
    let mut trailers = HeaderMap::new();
    let value = mid_stream_abort_trailer_value(
        break_at_seq,
        break_at_chunk,
        observed_hex,
        expected_hex,
    );
    if let (Ok(name), Ok(val)) = (
        HeaderName::from_bytes(HEADER_EXPORT_ABORTED.as_bytes()),
        HeaderValue::from_str(&value),
    ) {
        trailers.insert(name, val);
    }
    Frame::trailers(trailers)
}

/// Canonical mid-stream abort-trailer payload encoder. Public for
/// the wave-18 integration test + the customer-CLI compatibility
/// test (both assert the exact wire shape).
#[must_use]
pub fn mid_stream_abort_trailer_value(
    break_at_seq: u64,
    break_at_chunk: u64,
    observed_hex: &str,
    expected_hex: &str,
) -> String {
    format!(
        "{{\"break_at_seq\":{break_at_seq},\"break_at_chunk\":{break_at_chunk},\"observed\":\"{observed_hex}\",\"expected\":\"{expected_hex}\"}}"
    )
}
