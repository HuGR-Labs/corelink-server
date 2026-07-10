//! Networked audit surface — `corelink audit export` (production) and
//! `corelink audit tail`, wired to the live container route
//! `GET /v1/audit/:tenant/export` (see
//! `crates/corelink-container/src/routes/audit_export/`).
//!
//! This is a **binary-only** module (not part of the fuzzable `lib`),
//! because it depends on [`crate::client::CorelinkClient`]. The offline /
//! fixture / pure-verify surface stays in [`crate::commands::audit`],
//! which IS mirrored into the lib.
//!
//! The route streams an NDJSON envelope (`{event, proof}` per row + a
//! trailing `{"manifest": …}` line) and publishes the BLAKE3 chain-head
//! anchor in the `X-CoreLink-Audit-Export-Chain-Head-Anchor` response
//! header. `audit export` persists the bytes content-addressed (so the
//! customer can re-verify with `audit verify-ndjson --ndjson <file>
//! --chain-head-anchor <hdr>`); `audit tail` decodes the window and
//! prints the most recent (optionally filtered) events.

use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};
use crate::verify_ndjson::{parse_ndjson_envelope_public, RowEnvelopePub};

/// Response header advertising the BLAKE3 chain-head anchor at export
/// time. Mirrors
/// `routes/audit_export/types.rs::HEADER_CHAIN_HEAD_ANCHOR`.
const HEADER_CHAIN_HEAD_ANCHOR: &str = "x-corelink-audit-export-chain-head-anchor";

/// Default tail window: the trailing hour.
const DEFAULT_TAIL_WINDOW_MS: u64 = 3_600_000;
/// Default number of events `audit tail` prints.
const DEFAULT_TAIL_LIMIT: usize = 50;

/// Outcome of a production `audit export`.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct ExportProdOutcome {
    /// Tenant whose window was exported.
    pub tenant_id: String,
    /// Inclusive lower bound (Unix epoch ms).
    pub since_ms: u64,
    /// Exclusive upper bound (Unix epoch ms).
    pub until_ms: u64,
    /// Bytes written to disk.
    pub bytes_written: u64,
    /// On-disk path of the NDJSON export.
    pub file_path: String,
    /// BLAKE3-256 of the file contents (hex).
    pub file_blake3: String,
    /// Chain-head anchor from the response header (needed for
    /// `verify-ndjson`), if the server supplied it.
    pub chain_head_anchor: Option<String>,
}

impl fmt::Display for ExportProdOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Audit export written (NDJSON):")?;
        writeln!(f, "  Tenant:       {}", self.tenant_id)?;
        writeln!(
            f,
            "  Window:       [{}, {}) Unix ms",
            self.since_ms, self.until_ms
        )?;
        writeln!(f, "  Bytes:        {}", self.bytes_written)?;
        writeln!(f, "  Path:         {}", self.file_path)?;
        writeln!(f, "  BLAKE3:       {}", self.file_blake3)?;
        match &self.chain_head_anchor {
            Some(a) => write!(
                f,
                "  Chain anchor: {a}\n  Verify with:  corelink audit verify-ndjson --ndjson {} --chain-head-anchor {a}",
                self.file_path
            ),
            None => write!(
                f,
                "  Chain anchor: (server did not emit {HEADER_CHAIN_HEAD_ANCHOR}; cannot auto-verify)"
            ),
        }
    }
}

fn blake3_hex(bytes: &[u8]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(bytes);
    hex::encode(h.finalize().as_bytes())
}

/// Content-addressed export filename (mirrors the offline exporter's
/// naming convention with a `.ndjson` extension).
fn export_filename(tenant_id: &str, since_ms: u64, until_ms: u64, file_hash: &str) -> String {
    let tprefix: String = tenant_id.chars().take(8).collect();
    let hprefix: String = file_hash.chars().take(32).collect();
    format!("corelink-audit-{tprefix}-{since_ms}-{until_ms}-{hprefix}.ndjson")
}

/// Run the production `audit export`: fetch the NDJSON window from the
/// live route and persist it content-addressed under `out_dir`.
pub async fn run_export_production(
    client: &CorelinkClient,
    tenant: &str,
    since_ms: u64,
    until_ms: u64,
    out_dir: &Path,
    output_fmt: OutputFormat,
) -> Result<(), CliError> {
    if until_ms <= since_ms {
        return Err(CliError::Other(format!(
            "audit export: --until ({until_ms}) must be greater than --since ({since_ms})"
        )));
    }
    let path = format!("/v1/audit/{tenant}/export?from={since_ms}&to={until_ms}");
    let (bytes, anchor) = client
        .get_bytes_with_header(&path, HEADER_CHAIN_HEAD_ANCHOR)
        .await?;

    let file_hash = blake3_hex(&bytes);
    let file_name = export_filename(tenant, since_ms, until_ms, &file_hash);
    if !out_dir.as_os_str().is_empty() {
        std::fs::create_dir_all(out_dir).map_err(CliError::Io)?;
    }
    let file_path = out_dir.join(&file_name);
    std::fs::write(&file_path, &bytes).map_err(CliError::Io)?;

    let outcome = ExportProdOutcome {
        tenant_id: tenant.to_owned(),
        since_ms,
        until_ms,
        bytes_written: bytes.len() as u64,
        file_path: file_path.display().to_string(),
        file_blake3: file_hash,
        chain_head_anchor: anchor,
    };
    Formatter::new(output_fmt)
        .emit(&outcome)
        .map_err(CliError::Json)?;
    Ok(())
}

/// A single tail line, serialisable for `--output=json`.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct TailEvent {
    /// Monotonic audit sequence number.
    pub sequence_number: u64,
    /// Event time (Unix epoch ms).
    pub time_ms: u64,
    /// CloudEvents `type` (e.g. `corelink.observability.export_ready`).
    pub event_type: String,
    /// Event subject.
    pub subject: String,
    /// Event source.
    pub source: String,
}

impl fmt::Display for TailEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:>8}  {:>14}  {:<44}  {}",
            self.sequence_number, self.time_ms, self.event_type, self.subject
        )
    }
}

/// Newtype so we can give `Vec<TailEvent>` a `Display` for text output.
#[derive(Debug, Serialize)]
pub struct TailEvents(pub Vec<TailEvent>);

impl fmt::Display for TailEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return write!(f, "(no matching audit events in window)");
        }
        writeln!(f, "{:>8}  {:>14}  {:<44}  SUBJECT", "SEQ", "TIME_MS", "EVENT_TYPE")?;
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

/// Parse a `--filter key=value` predicate.
fn parse_filter(raw: &str) -> Result<(String, String), CliError> {
    match raw.split_once('=') {
        Some((k, v)) if !k.trim().is_empty() => Ok((k.trim().to_owned(), v.trim().to_owned())),
        _ => Err(CliError::Other(format!(
            "audit tail: --filter must be `key=value` (e.g. event_type=corelink.observability.export_ready); got {raw:?}"
        ))),
    }
}

/// Extract a filterable string field from an audit event.
fn event_field(event: &corelink_audit_chain::AuditEvent, key: &str) -> Option<String> {
    match key {
        "event_type" => Some(event.event_type.clone()),
        "subject" => Some(event.subject.subject().to_owned()),
        "source" => Some(event.source.clone()),
        "region" => Some(event.region.as_str().to_owned()),
        "tenant_id" => Some(event.tenant_id.to_string()),
        "sequence_number" | "seq" => Some(event.sequence_number.to_string()),
        "id" => Some(event.id.to_string()),
        _ => None,
    }
}

/// Apply an optional `(key, value)` filter + trailing `limit` to parsed
/// rows, returning the projected [`TailEvent`] list (chronological).
fn project_tail(
    rows: &[RowEnvelopePub],
    filter: Option<&(String, String)>,
    limit: usize,
) -> Result<Vec<TailEvent>, CliError> {
    // Validate the filter key up-front (against the first row if any) so
    // a typo'd key fails loudly rather than silently matching nothing.
    if let (Some((k, _)), Some(first)) = (filter, rows.first()) {
        if event_field(&first.event, k).is_none() {
            return Err(CliError::Other(format!(
                "audit tail: unknown --filter key {k:?}; supported: event_type, subject, source, region, tenant_id, sequence_number, id"
            )));
        }
    }
    let mut matched: Vec<TailEvent> = rows
        .iter()
        .filter(|r| match filter {
            None => true,
            Some((k, v)) => event_field(&r.event, k).as_deref() == Some(v.as_str()),
        })
        .map(|r| TailEvent {
            sequence_number: r.event.sequence_number,
            time_ms: r.event.time_ms,
            event_type: r.event.event_type.clone(),
            subject: r.event.subject.subject().to_owned(),
            source: r.event.source.clone(),
        })
        .collect();
    // Keep the trailing `limit` (most recent by position in the chain).
    if matched.len() > limit {
        matched.drain(0..matched.len() - limit);
    }
    Ok(matched)
}

/// Current wall-clock time in Unix epoch ms.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Run `corelink audit tail` — pull a recent audit window from the live
/// export route and print the trailing (optionally filtered) events.
///
/// NOTE: this is a windowed *pull*, not a live follow — true streaming
/// (`-f`) would need a server-side SSE/websocket surface that does not
/// exist yet. The default window is the trailing hour.
pub async fn run_tail(
    client: &CorelinkClient,
    tenant: &str,
    filter: Option<&str>,
    since_ms: Option<u64>,
    limit: Option<usize>,
    output_fmt: OutputFormat,
) -> Result<(), CliError> {
    let until = now_ms();
    let since = since_ms.unwrap_or_else(|| until.saturating_sub(DEFAULT_TAIL_WINDOW_MS));
    let limit = limit.unwrap_or(DEFAULT_TAIL_LIMIT);
    let parsed_filter = match filter {
        Some(f) => Some(parse_filter(f)?),
        None => None,
    };

    let path = format!("/v1/audit/{tenant}/export?from={since}&to={until}");
    let (bytes, _anchor) = client
        .get_bytes_with_header(&path, HEADER_CHAIN_HEAD_ANCHOR)
        .await?;
    let body = std::str::from_utf8(&bytes)
        .map_err(|e| CliError::Other(format!("audit tail: response not UTF-8: {e}")))?;
    let (rows, _manifest) = parse_ndjson_envelope_public(body)?;
    let events = project_tail(&rows, parsed_filter.as_ref(), limit)?;

    Formatter::new(output_fmt)
        .emit(&TailEvents(events))
        .map_err(CliError::Json)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use corelink_audit_chain::{AuditEvent, AuditEventKind, ChainHash, InclusionProof};

    fn mk_row(seq: u64, event_type_kind: AuditEventKind) -> RowEnvelopePub {
        let event = AuditEvent::new(
            event_type_kind,
            "corelink/region/iad",
            uuid::Uuid::now_v7(),
            1_000 + seq,
            uuid::Uuid::now_v7(),
            corelink_analytics::Region::Iad,
            seq,
            ChainHash::genesis(),
            serde_json::json!({"seq": seq}),
        );
        RowEnvelopePub {
            event,
            proof: InclusionProof {
                prev_hash: ChainHash::genesis(),
                link_hash: ChainHash::genesis(),
                siblings: vec![],
            },
        }
    }

    #[test]
    fn parse_filter_splits_key_value() {
        let (k, v) = parse_filter("event_type=corelink.cas.put").unwrap();
        assert_eq!(k, "event_type");
        assert_eq!(v, "corelink.cas.put");
    }

    #[test]
    fn parse_filter_rejects_malformed() {
        assert!(parse_filter("no-equals").is_err());
        assert!(parse_filter("=value").is_err());
    }

    #[test]
    fn export_filename_is_content_addressed() {
        let name = export_filename(
            "00000000-0000-0000-0000-000000000001",
            10,
            20,
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        );
        assert!(name.starts_with("corelink-audit-00000000-10-20-"));
        assert!(name.ends_with(".ndjson"));
    }

    #[test]
    fn project_tail_applies_limit_keeping_most_recent() {
        let rows: Vec<RowEnvelopePub> = (0..5).map(|i| mk_row(i, AuditEventKind::CasPut)).collect();
        let out = project_tail(&rows, None, 2).unwrap();
        assert_eq!(out.len(), 2);
        // Trailing two by chain position → seq 3 and 4.
        assert_eq!(out[0].sequence_number, 3);
        assert_eq!(out[1].sequence_number, 4);
    }

    #[test]
    fn project_tail_filters_by_event_type() {
        let rows: Vec<RowEnvelopePub> = vec![
            mk_row(0, AuditEventKind::CasPut),
            mk_row(1, AuditEventKind::CasGet),
            mk_row(2, AuditEventKind::CasPut),
        ];
        let put_type = rows[0].event.event_type.clone();
        let filter = ("event_type".to_owned(), put_type);
        let out = project_tail(&rows, Some(&filter), 50).unwrap();
        assert_eq!(out.len(), 2, "only CasPut rows should match");
    }

    #[test]
    fn project_tail_rejects_unknown_filter_key() {
        let rows = vec![mk_row(0, AuditEventKind::CasPut)];
        let filter = ("nope".to_owned(), "x".to_owned());
        assert!(project_tail(&rows, Some(&filter), 50).is_err());
    }
}
