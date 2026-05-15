//! `corelink audit verify-ndjson` — offline re-verify of an
//! NDJSON envelope produced by `GET /v1/audit/export` (WI-S09-008).
//!
//! Wave-17 closes the customer-CLI re-verify AC of WI-S09-008. The
//! Wave-15.3 server endpoint streams one NDJSON line per audit row
//! plus a trailing `{"manifest": <ExportManifest>}` line. The
//! `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header
//! advertises the chain head observed at export time as a 64-char
//! BLAKE3 hex string.
//!
//! This module re-verifies the NDJSON dump offline:
//!
//! 1. Parse each `{event, proof}` row.
//! 2. Recompute the chain link at each row via
//!    [`link_chain_hash`] over (`event.prev_hash`, `event`); compare
//!    constant-time against `proof.link_hash`.
//! 3. Walk the chain forward — the *next* row's `event.prev_hash`
//!    MUST equal the current row's `proof.link_hash`.
//! 4. After the last row, assert the final `proof.link_hash` equals
//!    the customer-supplied `--chain-head-anchor` AND the manifest
//!    line's `chain_head_at_export`.
//!
//! Any divergence aborts with a structured JSON error
//! (`{verified, events_verified, error: { line, observed, expected, kind }}`)
//! and exit code `1`. Happy path emits `{verified: true, ...}` and
//! exit code `0`. Fail-CLOSED — we'd rather refuse to acknowledge an
//! export than rubber-stamp a tampered one.
//!
//! ## Why a separate module from `commands::audit::run_verify`
//!
//! `commands::audit::run_verify` already re-verifies a **JSON-LD**
//! export (a single JSON document with `manifest` + `events: [...]`).
//! The server's `/v1/audit/export` endpoint emits a **streaming NDJSON**
//! envelope — one row per line + trailing manifest line. The two
//! formats share the underlying `InclusionProof` primitive but the
//! parse surface differs; this module handles the NDJSON shape.

use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use corelink_audit_chain::{
    hashes_eq_ct, link_chain_hash, AuditEvent, ChainHash, ExportManifest, InclusionProof,
};

use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Structured outcome of `corelink audit verify-ndjson` surfaced to
/// stdout (text + JSON).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VerifyNdjsonOutcome {
    /// On-disk path of the NDJSON file that was verified.
    pub file_path: String,
    /// Total NDJSON lines that parsed as `{event, proof}` rows.
    pub events_verified: u64,
    /// Customer-supplied chain head anchor (lowercase 64-char hex).
    pub expected_chain_head_anchor: String,
    /// Manifest `chain_head_at_export` recovered from the trailing
    /// `{"manifest": ...}` line (lowercase 64-char hex).
    pub manifest_chain_head: String,
    /// Final BLAKE3 link hash observed after walking the chain
    /// (lowercase 64-char hex). MUST equal both
    /// `expected_chain_head_anchor` and `manifest_chain_head` for the
    /// chain to be considered intact.
    pub final_observed_chain_head: String,
    /// `true` iff every link recomputed, every sibling-walk continuity
    /// check passed, AND the final hash matched both the manifest +
    /// the customer-supplied anchor.
    pub verified: bool,
}

impl fmt::Display for VerifyNdjsonOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "NDJSON audit-export verify:")?;
        writeln!(f, "  Path:                {}", self.file_path)?;
        writeln!(f, "  Events verified:     {}", self.events_verified)?;
        writeln!(f, "  Expected anchor:     {}", self.expected_chain_head_anchor)?;
        writeln!(f, "  Manifest head:       {}", self.manifest_chain_head)?;
        writeln!(f, "  Observed final:      {}", self.final_observed_chain_head)?;
        write!(
            f,
            "  Status:              {}",
            if self.verified { "OK" } else { "FAIL" }
        )
    }
}

/// Run `corelink audit verify-ndjson --ndjson <path> --chain-head-anchor <hex>`.
///
/// Streams the NDJSON file, recomputes every chain link, walks
/// continuity forward, asserts the manifest + customer-supplied
/// anchor match the recomputed final hash. Fail-CLOSED on ANY
/// divergence (CTRL-AUDIT-001).
///
/// # Errors
///
/// Returns [`CliError`] on:
/// - I/O failure reading `path`.
/// - Malformed proof / event / manifest JSON.
/// - Manifest line missing entirely (NDJSON shape contract violated).
/// - Recomputed link hash diverging from claimed `proof.link_hash`.
/// - Continuity check failure (event N+1's `prev_hash` not equal to
///   event N's `link_hash`).
/// - Final observed chain head not matching the customer-supplied
///   `expected_anchor_hex` (case-insensitive 64-char BLAKE3 hex).
/// - Manifest `chain_head_at_export` disagreeing with either the
///   computed final hash or the customer-supplied anchor.
pub fn run_verify_ndjson(
    path: &Path,
    expected_anchor_hex: &str,
    output_fmt: OutputFormat,
) -> Result<VerifyNdjsonOutcome, CliError> {
    let expected_anchor = parse_anchor_hex(expected_anchor_hex)?;

    let body = fs::read_to_string(path).map_err(CliError::Io)?;
    let (rows, manifest) = parse_ndjson_envelope(&body)?;

    // Walk the chain. For the FIRST row, `event.prev_hash` is the
    // chain anchor at the start of the window. For each subsequent
    // row, `event.prev_hash` MUST equal the previous row's recomputed
    // link hash (continuity).
    //
    // For every row, the recomputed BLAKE3 link MUST equal the
    // claimed `proof.link_hash` (per-row integrity).
    let mut prev_link: Option<ChainHash> = None;
    let mut last_link: Option<ChainHash> = None;
    for (idx, row) in rows.iter().enumerate() {
        let line_number = idx.saturating_add(1);
        // Continuity check (skipped for the first row).
        if let Some(prev) = prev_link {
            if !chain_hashes_eq_ct(&prev, &row.event.prev_hash) {
                return Err(structured_chain_break(
                    line_number,
                    &row.event.prev_hash.to_hex(),
                    &prev.to_hex(),
                    "continuity",
                    path,
                ));
            }
        }
        // Recompute link hash from canonical bytes; compare CT.
        let recomputed = link_chain_hash(&row.event.prev_hash, &row.event)
            .map_err(|e| CliError::Other(format!("link recompute failed at line {line_number}: {e}")))?;
        if !chain_hashes_eq_ct(&recomputed, &row.proof.link_hash) {
            return Err(structured_chain_break(
                line_number,
                &row.proof.link_hash.to_hex(),
                &recomputed.to_hex(),
                "link_recompute",
                path,
            ));
        }
        prev_link = Some(recomputed);
        last_link = Some(recomputed);
    }

    // Manifest + customer anchor cross-check.
    let final_link = last_link.unwrap_or(ChainHash::genesis());
    let manifest_head_hex = manifest.chain_head_at_export.to_hex();
    if !chain_hashes_eq_ct(&manifest.chain_head_at_export, &final_link) {
        return Err(CliError::Other(format!(
            "audit verify-ndjson: manifest chain_head_at_export ({}) disagrees with recomputed final hash ({}) — export envelope is internally inconsistent",
            manifest_head_hex,
            final_link.to_hex()
        )));
    }
    if !chain_hashes_eq_ct(&expected_anchor, &final_link) {
        return Err(CliError::Other(format!(
            "audit verify-ndjson: customer-supplied --chain-head-anchor ({}) does not match recomputed final hash ({}) — the chunk may be tampered or the wrong anchor was supplied; expected the value from response header X-CoreLink-Audit-Export-Chain-Head-Anchor",
            expected_anchor.to_hex(),
            final_link.to_hex()
        )));
    }

    let outcome = VerifyNdjsonOutcome {
        file_path: path.display().to_string(),
        events_verified: rows.len() as u64,
        expected_chain_head_anchor: expected_anchor.to_hex(),
        manifest_chain_head: manifest_head_hex,
        final_observed_chain_head: final_link.to_hex(),
        verified: true,
    };
    let fmt = Formatter::new(output_fmt);
    fmt.emit(&outcome).map_err(CliError::Json)?;
    Ok(outcome)
}

/// Parse a 64-char lowercase / mixed-case hex BLAKE3 anchor into a
/// [`ChainHash`]. Trims surrounding whitespace.
fn parse_anchor_hex(hex_str: &str) -> Result<ChainHash, CliError> {
    let trimmed = hex_str.trim();
    if trimmed.len() != 64 {
        return Err(CliError::Other(format!(
            "audit verify-ndjson: --chain-head-anchor must be 64 hex chars (BLAKE3-256); got {} chars",
            trimmed.len()
        )));
    }
    let bytes = hex::decode(trimmed)
        .map_err(|e| CliError::Other(format!("audit verify-ndjson: --chain-head-anchor hex decode failed: {e}")))?;
    if bytes.len() != 32 {
        return Err(CliError::Other(format!(
            "audit verify-ndjson: --chain-head-anchor decoded to {} bytes; expected 32",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(ChainHash(out))
}

/// One parsed NDJSON row carrying `{event, proof}`.
#[derive(Debug, Deserialize)]
struct RowEnvelope {
    event: AuditEvent,
    proof: InclusionProof,
}

/// Trailing NDJSON line carrying the manifest envelope.
#[derive(Debug, Deserialize)]
struct ManifestEnvelope {
    manifest: ExportManifest,
}

/// Parse the NDJSON envelope: every line except the last MUST be a
/// `{event, proof}` row; the LAST non-empty line MUST be the
/// `{"manifest": <ExportManifest>}` footer per the
/// `apps/server/src/routes/audit_export.rs` wire shape.
///
/// Returns the row vector + parsed manifest, or a structured error
/// pinpointing the malformed line.
fn parse_ndjson_envelope(body: &str) -> Result<(Vec<RowEnvelope>, ExportManifest), CliError> {
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return Err(CliError::Other(
            "audit verify-ndjson: NDJSON file is empty; expected at least the {\"manifest\": ...} footer line".into(),
        ));
    }
    let footer_idx = lines.len().saturating_sub(1);
    let footer_line = lines.get(footer_idx).ok_or_else(|| {
        CliError::Other("audit verify-ndjson: failed to locate footer line".into())
    })?;
    let manifest_env: ManifestEnvelope = serde_json::from_str(footer_line).map_err(|e| {
        CliError::Other(format!(
            "audit verify-ndjson: failed to parse trailing manifest line (line {}): {e} — the wire contract requires the final non-empty line to be the JSON object {{\"manifest\": <ExportManifest>}}",
            footer_idx.saturating_add(1)
        ))
    })?;
    let mut rows: Vec<RowEnvelope> = Vec::with_capacity(footer_idx);
    for (i, line) in lines.iter().take(footer_idx).enumerate() {
        let row: RowEnvelope = serde_json::from_str(line).map_err(|e| {
            CliError::Other(format!(
                "audit verify-ndjson: malformed proof JSON at line {}: {e}",
                i.saturating_add(1)
            ))
        })?;
        rows.push(row);
    }
    Ok((rows, manifest_env.manifest))
}

/// Constant-time compare of two [`ChainHash`] values. Re-exports the
/// canonical `corelink_audit_chain::hashes_eq_ct` discipline (the
/// verifier MUST NOT short-circuit on the first differing byte; a
/// side-channel here would let a tampered chunk leak which byte to
/// flip).
fn chain_hashes_eq_ct(a: &ChainHash, b: &ChainHash) -> bool {
    hashes_eq_ct(a, b)
}

/// Build a structured `CliError::Other` carrying the canonical
/// chain-break diagnostic shape (line + observed vs expected hash +
/// failure kind + file path). The string itself is JSON-shaped so
/// downstream SIEM / Drata wrappers can parse it directly.
fn structured_chain_break(
    line: usize,
    observed_hex: &str,
    expected_hex: &str,
    kind: &str,
    path: &Path,
) -> CliError {
    // Build a JSON object as the error string. We don't use
    // `serde_json::json!` here because we want the error text to be
    // human-readable + machine-parseable simultaneously without
    // escaping cost.
    let chunk = path.display().to_string();
    let body = format!(
        "Chain break at chunk {chunk} event line #{line}: observed hash {observed_hex}, expected {expected_hex} — chunk may be tampered (kind={kind}). Structured: {{\"verified\":false,\"file\":\"{chunk}\",\"line\":{line},\"observed\":\"{observed_hex}\",\"expected\":\"{expected_hex}\",\"kind\":\"{kind}\"}}"
    );
    CliError::Other(body)
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
    use crate::commands::audit::build_fixture_exporter;
    use corelink_audit_chain::{AuditExporter, ExportWindow};
    use tempfile::TempDir;
    use uuid::Uuid;

    /// Build a deterministic NDJSON export envelope (the same wire
    /// shape `apps/server/src/routes/audit_export.rs` emits): one
    /// `{event, proof}` JSON per line, terminated by a single
    /// `{"manifest": ...}` line.
    fn build_ndjson_fixture(event_count: u32) -> (TempDir, std::path::PathBuf, String) {
        let tenant = Uuid::now_v7();
        let exporter = build_fixture_exporter(tenant, event_count, 1_000).unwrap();
        let window = ExportWindow::new(0, 10_000).unwrap();
        let result = exporter.export_window(&tenant.to_string(), window).unwrap();

        let mut body = String::new();
        for row in &result.rows {
            let line = serde_json::to_string(&serde_json::json!({
                "event": row.event,
                "proof": row.proof,
            }))
            .unwrap();
            body.push_str(&line);
            body.push('\n');
        }
        let manifest_line = serde_json::to_string(&serde_json::json!({
            "manifest": result.manifest,
        }))
        .unwrap();
        body.push_str(&manifest_line);

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("export.ndjson");
        fs::write(&path, body).unwrap();
        let anchor = result.manifest.chain_head_at_export.to_hex();
        (tmp, path, anchor)
    }

    /// Happy path — 10 events, recomputed chain matches the manifest
    /// + the customer-supplied anchor.
    #[test]
    fn happy_path_ten_event_round_trip_verifies() {
        let (tmp, path, anchor) = build_ndjson_fixture(10);
        let outcome = run_verify_ndjson(&path, &anchor, OutputFormat::Json).unwrap();
        assert!(outcome.verified);
        assert_eq!(outcome.events_verified, 10);
        assert_eq!(outcome.expected_chain_head_anchor, anchor);
        assert_eq!(outcome.manifest_chain_head, anchor);
        assert_eq!(outcome.final_observed_chain_head, anchor);
        drop(tmp);
    }

    /// 1-byte flip in event #3 — the recompute path catches it at
    /// the first link recomputation that diverges (line 3) OR at the
    /// continuity check on line 4 (whichever fires first; the
    /// recompute fires first because the tampered event's link hash
    /// is computed BEFORE we check continuity against the next row).
    #[test]
    fn tampered_event_one_byte_flip_in_chunk_three_fails_closed() {
        let (tmp, path, anchor) = build_ndjson_fixture(5);
        let body = fs::read_to_string(&path).unwrap();
        // Find the third NDJSON row and flip a digit inside the
        // `fixture_seq` data payload to make the canonical bytes
        // diverge from what the proof line hash was computed over.
        let mut lines: Vec<String> = body.lines().map(str::to_owned).collect();
        // lines[0..5] are rows, lines[5] is the manifest.
        let target = &mut lines[2];
        // Replace `"fixture_seq":2` with `"fixture_seq":7` (a 1-digit
        // flip in the JSON body — distinct from line 0/1/3/4's seqs).
        let tampered = target.replace("\"fixture_seq\":2", "\"fixture_seq\":7");
        assert_ne!(tampered, *target, "fixture must include a fixture_seq=2 to flip");
        *target = tampered;
        fs::write(&path, lines.join("\n")).unwrap();
        let err = run_verify_ndjson(&path, &anchor, OutputFormat::Json).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Chain break") && (msg.contains("line #3") || msg.contains("line #4")),
            "tampered chunk #3 must surface a structured chain break: {msg}"
        );
        drop(tmp);
    }

    /// Wrong anchor — the chain itself is intact but the customer
    /// supplied the wrong `--chain-head-anchor`.
    #[test]
    fn wrong_anchor_rejected_with_actionable_error() {
        let (tmp, path, _anchor) = build_ndjson_fixture(3);
        let wrong = "0".repeat(64);
        let err = run_verify_ndjson(&path, &wrong, OutputFormat::Json).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("--chain-head-anchor") && msg.contains("does not match"),
            "wrong-anchor error must reference the flag: {msg}"
        );
        drop(tmp);
    }

    /// Empty NDJSON — fails-CLOSED with a clear error (we won't
    /// silently rubber-stamp a zero-byte export).
    #[test]
    fn empty_ndjson_rejected_clearly() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("empty.ndjson");
        fs::write(&path, b"").unwrap();
        let err = run_verify_ndjson(&path, &"a".repeat(64), OutputFormat::Json).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("empty"), "empty-file error must mention emptiness: {msg}");
        drop(tmp);
    }

    /// Malformed proof JSON on a row line — surfaces a structured
    /// parse error pinpointing the line number.
    #[test]
    fn malformed_proof_json_pinpoints_line() {
        let (tmp, path, anchor) = build_ndjson_fixture(3);
        let body = fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = body.lines().map(str::to_owned).collect();
        // Corrupt row #2 (zero-indexed line 1).
        lines[1] = "{\"event\": not_valid_json,,,".to_string();
        fs::write(&path, lines.join("\n")).unwrap();
        let err = run_verify_ndjson(&path, &anchor, OutputFormat::Json).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("malformed proof JSON") && msg.contains("line 2"),
            "malformed-JSON error must pinpoint line: {msg}"
        );
        drop(tmp);
    }

    /// Mismatched chain-head — the manifest line itself has been
    /// tampered (anchor flipped); we surface the manifest disagrees
    /// with the recomputed final hash.
    #[test]
    fn mismatched_chain_head_in_manifest_rejected() {
        let (tmp, path, anchor) = build_ndjson_fixture(4);
        let body = fs::read_to_string(&path).unwrap();
        // Flip the manifest's chain_head_at_export by replacing the
        // first hex char of the anchor inside the manifest line.
        let original = format!("\"chain_head_at_export\":\"{anchor}\"");
        let flipped_anchor: String = if let Some(rest) = anchor.strip_prefix('0') {
            format!("1{rest}")
        } else if let Some(rest) = anchor.strip_prefix(|c: char| c != '0') {
            format!("0{rest}")
        } else {
            // Defensive fallback — anchor is empty, which never
            // happens for a 64-char BLAKE3 hex string.
            "0".repeat(64)
        };
        let tampered = format!("\"chain_head_at_export\":\"{flipped_anchor}\"");
        let new_body = body.replace(&original, &tampered);
        assert_ne!(new_body, body, "fixture must contain the manifest anchor field to flip");
        fs::write(&path, new_body).unwrap();
        let err = run_verify_ndjson(&path, &anchor, OutputFormat::Json).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("manifest chain_head_at_export") && msg.contains("disagrees"),
            "tampered-manifest error must call out the manifest disagreement: {msg}"
        );
        drop(tmp);
    }
}
