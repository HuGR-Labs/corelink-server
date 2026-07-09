//! `corelink audit export` + `corelink audit verify` — tamper-proof audit
//! log export for SOC 2 / GDPR / LGPD evidence (WI-R-PREP-AUDIT-EXPORT).
//!
//! Customer-facing CLI surface over the [`corelink_audit_chain::AuditExporter`] trait. The
//! production wiring (CF Worker `GET /v1/audit/export/window`) is
//! deferred to a follow-on WI; this lane ships the pure-logic CLI +
//! in-memory fake transport so customers can validate the format
//! contract + the proof-verify path against fixture data today.
//!
//! ## Output content-addressing
//!
//! The output filename includes BLAKE3-128 prefix of the file contents
//! (truncated to 32 hex chars) + the export UTC timestamp:
//!
//! ```text
//! corelink-audit-{tenant_prefix}-{since}-{until}-{blake3_prefix}.{ext}
//! ```
//!
//! Customers committing the file to git or attaching it to a Drata
//! evidence ticket get verifiable provenance for free.
//!
//! ## Three serialization formats
//!
//! - **`json-ld`** (default, canonical): one JSON document with `@context`
//!   pointing at the CloudEvents JSON-LD context + CoreLink audit
//!   extensions. Manifest at top, rows in `events` array, each row
//!   carries its full inclusion proof. Schema-on-read so any SIEM /
//!   compliance tool can ingest.
//! - **`csv`** (flat): one row per audit event; proof is dropped (the
//!   compliance reviewer who wants a spreadsheet doesn't need the
//!   Merkle path). For chain-integrity, pair with a `json-ld` export.
//! - **`parquet`** (analytical): emits a parquet-targeted JSON
//!   container — schema + rows — that the existing
//!   `crates/corelink-audit-export-worker` (deferred) wraps in a true
//!   Apache Parquet file. Stripping the Arrow dependency from the CLI
//!   binary keeps the CLI < 10 MiB; the JSON sidecar is the
//!   schema-stable handoff to the worker.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use corelink_audit_chain::{
    verify_export_result, AuditExporter, ExportManifest, ExportResult, ExportWindow,
    ExportedAuditEvent, InMemoryAuditExporter,
};

use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Supported export serialization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[non_exhaustive]
pub enum ExportFormat {
    /// JSON-LD with CloudEvents 1.0 context + CoreLink audit extensions
    /// (canonical; default).
    #[clap(name = "json-ld")]
    JsonLd,
    /// Flat CSV for spreadsheet review (drops Merkle proofs).
    Csv,
    /// Parquet-targeted JSON container (schema + rows).
    Parquet,
}

impl ExportFormat {
    /// File extension corresponding to the format.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::JsonLd => "jsonld",
            Self::Csv => "csv",
            Self::Parquet => "parquet.json",
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JsonLd => write!(f, "json-ld"),
            Self::Csv => write!(f, "csv"),
            Self::Parquet => write!(f, "parquet"),
        }
    }
}

/// Outcome of an `audit export` invocation, surfaced to stdout/JSON.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ExportOutcome {
    /// Tenant id whose audit log was exported.
    pub tenant_id: String,
    /// Inclusive lower bound, Unix epoch ms.
    pub since_ms: u64,
    /// Exclusive upper bound, Unix epoch ms.
    pub until_ms: u64,
    /// Number of events exported.
    pub event_count: u64,
    /// BLAKE3-256 of the on-disk file contents (hex).
    pub file_blake3: String,
    /// Format used.
    pub format: String,
    /// On-disk path of the export.
    pub file_path: String,
    /// Whether `--include-merkle-proofs` was set.
    pub merkle_proofs_included: bool,
    /// Whether `--verify` re-validated every proof pre-write.
    pub verified_before_write: bool,
    /// Chain head observed at export time (hex).
    pub chain_head_at_export: String,
}

impl fmt::Display for ExportOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Export written:")?;
        writeln!(f, "  Tenant:         {}", self.tenant_id)?;
        writeln!(
            f,
            "  Window:         [{}, {}) Unix ms",
            self.since_ms, self.until_ms
        )?;
        writeln!(f, "  Events:         {}", self.event_count)?;
        writeln!(f, "  Format:         {}", self.format)?;
        writeln!(f, "  Path:           {}", self.file_path)?;
        writeln!(f, "  BLAKE3:         {}", self.file_blake3)?;
        writeln!(f, "  Merkle proofs:  {}", self.merkle_proofs_included)?;
        writeln!(f, "  Pre-verified:   {}", self.verified_before_write)?;
        write!(f, "  Chain head:     {}", self.chain_head_at_export)
    }
}

/// Run the `audit export` CLI surface against an injectable
/// [`AuditExporter`] (production wiring injects the CF Worker client;
/// tests inject the in-memory fake).
///
/// # Errors
///
/// Returns [`CliError`] when JSON / I/O fails or the source data fails
/// the pre-write `--verify` pass.
#[allow(clippy::too_many_arguments)]
pub fn run_export<E: AuditExporter + ?Sized>(
    exporter: &E,
    tenant_id: &str,
    since_ms: u64,
    until_ms: u64,
    format: ExportFormat,
    output_dir: &Path,
    include_merkle_proofs: bool,
    verify: bool,
    output_fmt: OutputFormat,
) -> Result<ExportOutcome, CliError> {
    let window = ExportWindow::new(since_ms, until_ms)
        .map_err(|e| CliError::Other(format!("invalid window: {e}")))?;
    let result = exporter
        .export_window(tenant_id, window)
        .map_err(|e| CliError::Other(format!("export_window failed: {e}")))?;

    // `--verify` re-validates every proof BEFORE we write the file.
    // CTRL-AUDIT-001 fail-CLOSED — we'd rather refuse to write a
    // potentially-tampered export than ship one to a compliance
    // reviewer.
    if verify {
        verify_export_result(&result).map_err(|e| {
            CliError::Other(format!(
                "audit export pre-write verify failed (CTRL-AUDIT-001 fail-CLOSED): {e}"
            ))
        })?;
    }

    let body = serialize_export(&result, format, include_merkle_proofs)?;
    let file_hash = blake3_hex_of(&body);
    let file_name = build_filename(tenant_id, since_ms, until_ms, format, &file_hash);
    let file_path = output_dir.join(&file_name);

    // Ensure parent dir exists; ignore AlreadyExists.
    if let Some(parent) = file_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(CliError::Io)?;
        }
    }
    let mut f = fs::File::create(&file_path).map_err(CliError::Io)?;
    f.write_all(&body).map_err(CliError::Io)?;
    f.sync_all().map_err(CliError::Io)?;

    let outcome = ExportOutcome {
        tenant_id: tenant_id.to_owned(),
        since_ms,
        until_ms,
        event_count: result.manifest.event_count,
        file_blake3: file_hash,
        format: format.to_string(),
        file_path: file_path.display().to_string(),
        merkle_proofs_included: include_merkle_proofs,
        verified_before_write: verify,
        chain_head_at_export: result.manifest.chain_head_at_export.to_hex(),
    };
    let fmt = Formatter::new(output_fmt);
    fmt.emit(&outcome).map_err(CliError::Json)?;
    Ok(outcome)
}

/// Wraps the export body in the chosen serialization format.
fn serialize_export(
    result: &ExportResult,
    format: ExportFormat,
    include_proofs: bool,
) -> Result<Vec<u8>, CliError> {
    match format {
        ExportFormat::JsonLd => render_json_ld(result, include_proofs),
        ExportFormat::Csv => Ok(render_csv(result)),
        ExportFormat::Parquet => render_parquet_container(result, include_proofs),
    }
}

#[derive(Serialize)]
struct JsonLdEnvelope<'a> {
    #[serde(rename = "@context")]
    context: Vec<&'static str>,
    #[serde(rename = "@type")]
    type_: &'static str,
    manifest: &'a ExportManifest,
    events: Vec<JsonLdEventRow<'a>>,
}

#[derive(Serialize)]
struct JsonLdEventRow<'a> {
    #[serde(rename = "@type")]
    type_: &'static str,
    event: &'a corelink_audit_chain::AuditEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof: Option<&'a corelink_audit_chain::InclusionProof>,
}

fn render_json_ld(result: &ExportResult, include_proofs: bool) -> Result<Vec<u8>, CliError> {
    let rows: Vec<JsonLdEventRow<'_>> = result
        .rows
        .iter()
        .map(|r| JsonLdEventRow {
            type_: "CoreLinkAuditEvent",
            event: &r.event,
            proof: if include_proofs { Some(&r.proof) } else { None },
        })
        .collect();
    let env = JsonLdEnvelope {
        context: vec![
            "https://cloudevents.io/jsonld/context",
            "https://humanguardrail.github.io/corelink-specs/jsonld/audit-export-v1.jsonld",
        ],
        type_: "CoreLinkAuditExport",
        manifest: &result.manifest,
        events: rows,
    };
    let body = serde_json::to_vec_pretty(&env).map_err(CliError::Json)?;
    Ok(body)
}

fn render_csv(result: &ExportResult) -> Vec<u8> {
    let mut out = String::new();
    // Header: 10 canonical columns aligned to AuditEvent CloudEvents 1.0 +
    // chain link slots.
    out.push_str(
        "sequence_number,time_ms,tenant_id,region,subject,event_type,id,source,prev_hash,link_hash\n",
    );
    for row in &result.rows {
        let e = &row.event;
        // CSV-escape each cell: wrap in double-quotes + double internal
        // quotes (RFC 4180).
        let cells: [String; 10] = [
            e.sequence_number.to_string(),
            e.time_ms.to_string(),
            e.tenant_id.to_string(),
            e.region.as_str().to_owned(),
            e.subject.subject().to_owned(),
            e.event_type.clone(),
            e.id.to_string(),
            e.source.clone(),
            e.prev_hash.to_hex(),
            row.proof.link_hash.to_hex(),
        ];
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            csv_escape_into(&mut out, cell);
        }
        out.push('\n');
    }
    out.into_bytes()
}

fn csv_escape_into(buf: &mut String, cell: &str) {
    let needs_quote = cell.contains(',') || cell.contains('"') || cell.contains('\n');
    if needs_quote {
        buf.push('"');
        for ch in cell.chars() {
            if ch == '"' {
                buf.push('"');
            }
            buf.push(ch);
        }
        buf.push('"');
    } else {
        buf.push_str(cell);
    }
}

#[derive(Serialize)]
struct ParquetContainer<'a> {
    schema_version: u32,
    parquet_target: &'static str,
    schema: ParquetSchema,
    manifest: &'a ExportManifest,
    rows: Vec<ParquetRow<'a>>,
}

#[derive(Serialize)]
struct ParquetSchema {
    columns: Vec<ParquetColumn>,
}

#[derive(Serialize)]
struct ParquetColumn {
    name: &'static str,
    parquet_type: &'static str,
    repetition: &'static str,
}

#[derive(Serialize)]
struct ParquetRow<'a> {
    sequence_number: u64,
    time_ms: u64,
    tenant_id: String,
    region: &'a str,
    subject: &'static str,
    event_type: &'a str,
    id: String,
    source: &'a str,
    prev_hash: String,
    link_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof_siblings: Option<&'a [corelink_audit_chain::ProofSibling]>,
    data_json: String,
}

fn render_parquet_container(
    result: &ExportResult,
    include_proofs: bool,
) -> Result<Vec<u8>, CliError> {
    let rows: Result<Vec<ParquetRow<'_>>, CliError> = result
        .rows
        .iter()
        .map(|r| -> Result<ParquetRow<'_>, CliError> {
            Ok(ParquetRow {
                sequence_number: r.event.sequence_number,
                time_ms: r.event.time_ms,
                tenant_id: r.event.tenant_id.to_string(),
                region: r.event.region.as_str(),
                subject: r.event.subject.subject(),
                event_type: &r.event.event_type,
                id: r.event.id.to_string(),
                source: &r.event.source,
                prev_hash: r.event.prev_hash.to_hex(),
                link_hash: r.proof.link_hash.to_hex(),
                proof_siblings: if include_proofs {
                    Some(&r.proof.siblings)
                } else {
                    None
                },
                data_json: serde_json::to_string(&r.event.data).map_err(CliError::Json)?,
            })
        })
        .collect();
    let container = ParquetContainer {
        schema_version: 1,
        parquet_target: "corelink-audit-export-v1",
        schema: ParquetSchema {
            columns: vec![
                ParquetColumn {
                    name: "sequence_number",
                    parquet_type: "INT64",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "time_ms",
                    parquet_type: "INT64",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "tenant_id",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "region",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "subject",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "event_type",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "id",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "source",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "prev_hash",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "link_hash",
                    parquet_type: "BYTE_ARRAY(UTF8)",
                    repetition: "REQUIRED",
                },
                ParquetColumn {
                    name: "proof_siblings",
                    parquet_type: "BYTE_ARRAY(JSON)",
                    repetition: "OPTIONAL",
                },
                ParquetColumn {
                    name: "data_json",
                    parquet_type: "BYTE_ARRAY(JSON)",
                    repetition: "REQUIRED",
                },
            ],
        },
        manifest: &result.manifest,
        rows: rows?,
    };
    serde_json::to_vec_pretty(&container).map_err(CliError::Json)
}

fn blake3_hex_of(bytes: &[u8]) -> String {
    let mut h = Hasher::new();
    h.update(bytes);
    let digest = h.finalize();
    hex::encode(digest.as_bytes())
}

fn build_filename(
    tenant_id: &str,
    since_ms: u64,
    until_ms: u64,
    format: ExportFormat,
    file_hash_hex: &str,
) -> String {
    // Take 8-char tenant prefix for filesystem-readable name.
    let tprefix: String = tenant_id.chars().take(8).collect();
    // Take 32-char BLAKE3 prefix for content addressing.
    let hash_prefix: String = file_hash_hex.chars().take(32).collect();
    format!(
        "corelink-audit-{tprefix}-{since_ms}-{until_ms}-{hash_prefix}.{ext}",
        ext = format.extension()
    )
}

/// Outcome of `corelink audit verify <export>`.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VerifyOutcome {
    /// On-disk path of the verified export.
    pub file_path: String,
    /// BLAKE3 of the file contents (matches `file_blake3` in the export
    /// outcome).
    pub file_blake3: String,
    /// Number of events whose proofs were re-validated.
    pub events_verified: u64,
    /// Chain head observed at export (from manifest).
    pub chain_head_at_export: String,
    /// `true` if every proof checked out.
    pub ok: bool,
}

impl fmt::Display for VerifyOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Verify result:")?;
        writeln!(f, "  Path:           {}", self.file_path)?;
        writeln!(f, "  BLAKE3:         {}", self.file_blake3)?;
        writeln!(f, "  Events:         {}", self.events_verified)?;
        writeln!(f, "  Chain head:     {}", self.chain_head_at_export)?;
        write!(
            f,
            "  Status:         {}",
            if self.ok { "OK" } else { "FAIL" }
        )
    }
}

/// Run `corelink audit verify <path>` — re-decode a JSON-LD export and
/// re-validate every inclusion proof against the manifest anchor.
///
/// # Errors
///
/// Returns [`CliError`] when the file is missing, not JSON-LD, or any
/// proof fails to verify.
pub fn run_verify(path: &Path, output_fmt: OutputFormat) -> Result<VerifyOutcome, CliError> {
    let body = fs::read(path).map_err(CliError::Io)?;
    let file_hash = blake3_hex_of(&body);
    let parsed: JsonLdEnvelopeOwned = serde_json::from_slice(&body).map_err(CliError::Json)?;

    let rows: Vec<ExportedAuditEvent> = parsed
        .events
        .into_iter()
        .map(|r| -> Result<ExportedAuditEvent, CliError> {
            let proof = r.proof.ok_or_else(|| {
                CliError::Other(
                    "audit verify: export missing inclusion proofs (re-export with --include-merkle-proofs)"
                        .to_owned(),
                )
            })?;
            Ok(ExportedAuditEvent {
                event: r.event,
                proof,
            })
        })
        .collect::<Result<_, _>>()?;
    let event_count = rows.len() as u64;
    let result = ExportResult {
        manifest: parsed.manifest.clone(),
        rows,
    };
    let outcome_ok = verify_export_result(&result).is_ok();
    let outcome = VerifyOutcome {
        file_path: path.display().to_string(),
        file_blake3: file_hash,
        events_verified: event_count,
        chain_head_at_export: parsed.manifest.chain_head_at_export.to_hex(),
        ok: outcome_ok,
    };
    let fmt = Formatter::new(output_fmt);
    fmt.emit(&outcome).map_err(CliError::Json)?;
    if !outcome.ok {
        return Err(CliError::Other(
            "audit verify failed: at least one Merkle proof did not match the chain anchor".into(),
        ));
    }
    Ok(outcome)
}

#[derive(Deserialize)]
struct JsonLdEnvelopeOwned {
    manifest: ExportManifest,
    events: Vec<JsonLdEventRowOwned>,
    // Other JSON-LD fields (@context, @type) are ignored on read.
}

#[derive(Deserialize)]
struct JsonLdEventRowOwned {
    event: corelink_audit_chain::AuditEvent,
    #[serde(default)]
    proof: Option<corelink_audit_chain::InclusionProof>,
}

/// Build an [`InMemoryAuditExporter`] populated with a deterministic
/// fixture chain. Helper for tests + the `--fixture` CLI flag that
/// exercises the pipeline offline (no real backend).
///
/// `event_count` is capped at `u32::MAX` defensively; production-scale
/// inputs (TB-class) flow through the streaming endpoint, not the
/// fixture.
///
/// # Errors
///
/// Returns [`CliError`] when chain construction or seeding fails.
pub fn build_fixture_exporter(
    tenant_id: uuid::Uuid,
    event_count: u32,
    start_time_ms: u64,
) -> Result<InMemoryAuditExporter, CliError> {
    use corelink_analytics::Region;
    use corelink_audit_chain::{AuditEvent, AuditEventKind, ChainHash, HashChainBuilder};

    let mut exporter = InMemoryAuditExporter::new();
    let mut builder = HashChainBuilder::new();
    let mut prev = ChainHash::genesis();
    for i in 0..u64::from(event_count) {
        let event = AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            uuid::Uuid::now_v7(),
            start_time_ms.saturating_add(i),
            tenant_id,
            Region::Iad,
            i,
            prev,
            serde_json::json!({"fixture_seq": i}),
        );
        prev = builder
            .append(&event)
            .map_err(|e| CliError::Other(format!("fixture chain append failed: {e}")))?;
        exporter
            .append_event(event)
            .map_err(|e| CliError::Other(format!("fixture exporter append failed: {e}")))?;
    }
    let _ = (prev, start_time_ms); // silence unused warnings if reordered later
    Ok(exporter)
}

/// Build a default output directory (cwd) if none supplied.
#[must_use]
pub fn default_output_dir() -> PathBuf {
    PathBuf::from(".")
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
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn export_format_extensions_pinned() {
        assert_eq!(ExportFormat::JsonLd.extension(), "jsonld");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
        assert_eq!(ExportFormat::Parquet.extension(), "parquet.json");
    }

    #[test]
    fn build_filename_contains_hash_prefix_and_ext() {
        let name = build_filename(
            "00000000-0000-0000-0000-000000000001",
            100,
            200,
            ExportFormat::JsonLd,
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        );
        assert!(name.starts_with("corelink-audit-00000000-100-200-"));
        assert!(name.ends_with(".jsonld"));
        // 32-char hash prefix embedded.
        assert!(name.contains("deadbeefdeadbeefdeadbeefdeadbeef"));
    }

    #[test]
    fn csv_escape_handles_comma_and_quote_and_newline() {
        let mut buf = String::new();
        csv_escape_into(&mut buf, "no-special");
        assert_eq!(buf, "no-special");

        let mut buf = String::new();
        csv_escape_into(&mut buf, "has,comma");
        assert_eq!(buf, "\"has,comma\"");

        let mut buf = String::new();
        csv_escape_into(&mut buf, "has\"quote");
        assert_eq!(buf, "\"has\"\"quote\"");

        let mut buf = String::new();
        csv_escape_into(&mut buf, "has\nnewline");
        assert_eq!(buf, "\"has\nnewline\"");
    }

    fn write_and_read(format: ExportFormat) -> (TempDir, ExportOutcome) {
        let tenant = Uuid::now_v7();
        let exporter = build_fixture_exporter(tenant, 5, 1_000).unwrap();
        let tmp = TempDir::new().unwrap();
        let outcome = run_export(
            &exporter,
            &tenant.to_string(),
            0,
            10_000,
            format,
            tmp.path(),
            true,
            true,
            OutputFormat::Json,
        )
        .unwrap();
        (tmp, outcome)
    }

    #[test]
    fn export_jsonld_writes_file_with_content_hash() {
        let (tmp, outcome) = write_and_read(ExportFormat::JsonLd);
        assert_eq!(outcome.event_count, 5);
        assert!(outcome.merkle_proofs_included);
        assert!(outcome.verified_before_write);
        // File exists.
        let path = std::path::Path::new(&outcome.file_path);
        assert!(path.exists());
        // BLAKE3 in filename matches body hash (first 32 hex chars).
        let body = fs::read(path).unwrap();
        let computed = blake3_hex_of(&body);
        assert_eq!(computed, outcome.file_blake3);
        assert!(outcome
            .file_path
            .contains(&computed.chars().take(32).collect::<String>()));
        drop(tmp);
    }

    #[test]
    fn export_csv_has_canonical_header_row() {
        let (tmp, outcome) = write_and_read(ExportFormat::Csv);
        let body = fs::read_to_string(&outcome.file_path).unwrap();
        let first_line = body.lines().next().unwrap();
        assert_eq!(
            first_line,
            "sequence_number,time_ms,tenant_id,region,subject,event_type,id,source,prev_hash,link_hash"
        );
        // 5 events + 1 header = 6 lines.
        assert_eq!(body.lines().count(), 6);
        drop(tmp);
    }

    #[test]
    fn export_parquet_container_carries_schema_and_rows() {
        let (tmp, outcome) = write_and_read(ExportFormat::Parquet);
        let body = fs::read(&outcome.file_path).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["parquet_target"], "corelink-audit-export-v1");
        assert_eq!(v["rows"].as_array().unwrap().len(), 5);
        // Schema has 12 columns.
        assert_eq!(v["schema"]["columns"].as_array().unwrap().len(), 12);
        drop(tmp);
    }

    #[test]
    fn export_verify_roundtrip_passes() {
        let (tmp, outcome) = write_and_read(ExportFormat::JsonLd);
        let path = std::path::Path::new(&outcome.file_path);
        let v = run_verify(path, OutputFormat::Json).unwrap();
        assert!(v.ok);
        assert_eq!(v.events_verified, 5);
        assert_eq!(v.file_blake3, outcome.file_blake3);
        drop(tmp);
    }

    #[test]
    fn export_verify_rejects_tampered_file() {
        let (tmp, outcome) = write_and_read(ExportFormat::JsonLd);
        let path = std::path::Path::new(&outcome.file_path);
        let mut body = fs::read(path).unwrap();
        // Find the first occurrence of "fixture_seq" and corrupt the
        // integer that follows.
        if let Some(pos) = body.windows(13).position(|w| w == b"\"fixture_seq\"") {
            // Walk forward to the digit.
            let mut i = pos + 13;
            while i < body.len() && !body[i].is_ascii_digit() {
                i += 1;
            }
            if i < body.len() {
                // Flip the digit so the canonical bytes change.
                body[i] = if body[i] == b'9' { b'0' } else { body[i] + 1 };
            }
        }
        fs::write(path, &body).unwrap();
        let err = run_verify(path, OutputFormat::Json).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("audit verify failed") || msg.contains("Merkle"));
        drop(tmp);
    }

    #[test]
    fn export_window_validates_bounds() {
        let tenant = Uuid::now_v7();
        let exporter = build_fixture_exporter(tenant, 1, 0).unwrap();
        let tmp = TempDir::new().unwrap();
        let err = run_export(
            &exporter,
            &tenant.to_string(),
            500,
            100, // inverted
            ExportFormat::JsonLd,
            tmp.path(),
            true,
            true,
            OutputFormat::Json,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("invalid window"));
        drop(tmp);
    }

    #[test]
    fn verify_rejects_export_without_proofs() {
        // Build an export without proofs, then try to verify it.
        let tenant = Uuid::now_v7();
        let exporter = build_fixture_exporter(tenant, 3, 1_000).unwrap();
        let tmp = TempDir::new().unwrap();
        let outcome = run_export(
            &exporter,
            &tenant.to_string(),
            0,
            10_000,
            ExportFormat::JsonLd,
            tmp.path(),
            false, // include_merkle_proofs OFF
            false,
            OutputFormat::Json,
        )
        .unwrap();
        let path = std::path::Path::new(&outcome.file_path);
        let err = run_verify(path, OutputFormat::Json).unwrap_err();
        assert!(format!("{err}").contains("missing inclusion proofs"));
        drop(tmp);
    }

    #[test]
    fn empty_window_yields_zero_events_and_succeeds() {
        let tenant = Uuid::now_v7();
        let exporter = build_fixture_exporter(tenant, 0, 0).unwrap();
        let tmp = TempDir::new().unwrap();
        let outcome = run_export(
            &exporter,
            &tenant.to_string(),
            0,
            10_000,
            ExportFormat::JsonLd,
            tmp.path(),
            true,
            true,
            OutputFormat::Json,
        )
        .unwrap();
        assert_eq!(outcome.event_count, 0);
        drop(tmp);
    }
}
