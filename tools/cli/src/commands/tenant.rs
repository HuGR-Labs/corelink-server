//! `corelink tenant export` / `corelink tenant verify-export` —
//! customer data-portability / offboarding (GDPR exit; see
//! `apps/docs/docs/how-to/leave-corelink.mdx`).
//!
//! # What this builds today (and the flagged gap)
//!
//! The offboarding how-to advertises a single `.tar.zst` archive
//! containing every CAS blob, the audit-chain slice, the RBAC roster and
//! the DPA receipt. There is **no server-side bulk-export endpoint** that
//! streams that archive today — the reachable, real surfaces are the
//! audit-export route (`GET /v1/audit/:tenant/export`) and the CAS
//! listing (`GET /v1/cas/:tenant`).
//!
//! So `tenant export` assembles a **content-addressed JSON bundle** from
//! those real surfaces: the audit-chain NDJSON slice + the CAS blob index
//! (digests + sizes), each SHA-256-addressed, with a bundle-level hash
//! over the component digests. `tenant verify-export` re-reads the bundle
//! and re-checks every component hash + the bundle hash (byte-for-byte
//! integrity, offline). The blob **bytes** themselves, the RBAC roster
//! and the DPA receipt remain a flagged gap pending the server bulk-export
//! endpoint (blobs are individually retrievable via `corelink cas get
//! <digest>` in the meantime).

use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Bundle schema tag (bump on breaking format changes).
const BUNDLE_SCHEMA: &str = "corelink.tenant-export/v1";

/// Header on the audit-export route carrying the BLAKE3 chain-head
/// anchor (recorded in the bundle metadata for provenance).
const HEADER_CHAIN_HEAD_ANCHOR: &str = "x-corelink-audit-export-chain-head-anchor";

/// Descriptor for one bundle component (its content hash + size).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct Component {
    /// SHA-256 (hex) of the component payload bytes.
    pub sha256: String,
    /// Payload size in bytes.
    pub size_bytes: u64,
}

/// A content-addressed tenant-export bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TenantExportBundle {
    /// Schema tag.
    pub schema: String,
    /// Exported tenant id.
    pub tenant_id: String,
    /// Bundle creation time (Unix epoch ms).
    pub generated_at_ms: u64,
    /// Producing CLI version.
    pub generator: String,
    /// BLAKE3 chain-head anchor from the audit export, when the server
    /// supplied it (provenance for the audit slice).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_chain_head_anchor: Option<String>,
    /// Raw audit-chain NDJSON slice (one `{event, proof}` per line + a
    /// trailing manifest line, verbatim from the export route).
    pub audit_chain_ndjson: String,
    /// CAS blob index (the `GET /v1/cas/:tenant` response, verbatim).
    pub cas_index: serde_json::Value,
    /// Component descriptors, keyed by name.
    pub components: std::collections::BTreeMap<String, Component>,
    /// SHA-256 (hex) over the canonical component-digest summary.
    pub bundle_sha256: String,
}

impl fmt::Display for TenantExportBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Tenant export bundle:")?;
        writeln!(f, "  Schema:        {}", self.schema)?;
        writeln!(f, "  Tenant:        {}", self.tenant_id)?;
        writeln!(f, "  Generated at:  {} Unix ms", self.generated_at_ms)?;
        for (name, c) in &self.components {
            writeln!(
                f,
                "  Component:     {name} ({} bytes, sha256={})",
                c.size_bytes, c.sha256
            )?;
        }
        write!(f, "  Bundle sha256: {}", self.bundle_sha256)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Canonical summary string the bundle hash is computed over. Stable +
/// order-independent (components are a `BTreeMap`).
fn bundle_summary(
    tenant_id: &str,
    generated_at_ms: u64,
    components: &std::collections::BTreeMap<String, Component>,
) -> String {
    let mut s =
        format!("schema:{BUNDLE_SCHEMA}\ntenant:{tenant_id}\ngenerated_at:{generated_at_ms}\n");
    for (name, c) in components {
        s.push_str(&format!("component:{name}:{}:{}\n", c.sha256, c.size_bytes));
    }
    s
}

/// Build a bundle from already-fetched payloads (pure; no I/O).
#[must_use]
pub fn build_bundle(
    tenant_id: &str,
    generated_at_ms: u64,
    audit_chain_ndjson: String,
    cas_index: serde_json::Value,
    audit_chain_head_anchor: Option<String>,
) -> TenantExportBundle {
    let audit_bytes = audit_chain_ndjson.as_bytes();
    // Canonical JSON bytes for the CAS index so the hash is reproducible.
    let cas_bytes = serde_json::to_vec(&cas_index).unwrap_or_default();

    let mut components = std::collections::BTreeMap::new();
    components.insert(
        "audit_chain_ndjson".to_owned(),
        Component {
            sha256: sha256_hex(audit_bytes),
            size_bytes: audit_bytes.len() as u64,
        },
    );
    components.insert(
        "cas_index".to_owned(),
        Component {
            sha256: sha256_hex(&cas_bytes),
            size_bytes: cas_bytes.len() as u64,
        },
    );

    let summary = bundle_summary(tenant_id, generated_at_ms, &components);
    let bundle_sha256 = sha256_hex(summary.as_bytes());

    TenantExportBundle {
        schema: BUNDLE_SCHEMA.to_owned(),
        tenant_id: tenant_id.to_owned(),
        generated_at_ms,
        generator: format!("corelink-cli/{}", env!("CARGO_PKG_VERSION")),
        audit_chain_head_anchor,
        audit_chain_ndjson,
        cas_index,
        components,
        bundle_sha256,
    }
}

/// Outcome of `tenant verify-export`.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VerifyReport {
    /// Verified bundle path.
    pub archive: String,
    /// `true` iff every component hash + the bundle hash matched.
    pub ok: bool,
    /// Human-readable mismatch descriptions (empty on success).
    pub mismatches: Vec<String>,
}

impl fmt::Display for VerifyReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Verify export:")?;
        writeln!(f, "  Archive: {}", self.archive)?;
        writeln!(f, "  Status:  {}", if self.ok { "OK" } else { "FAIL" })?;
        if self.mismatches.is_empty() {
            write!(f, "  Integrity: all component + bundle hashes match")
        } else {
            for m in &self.mismatches {
                writeln!(f, "  Mismatch: {m}")?;
            }
            write!(f, "  ({} mismatch(es))", self.mismatches.len())
        }
    }
}

/// Re-derive and check every hash in a bundle (pure; no I/O).
#[must_use]
pub fn verify_bundle(bundle: &TenantExportBundle, archive_label: &str) -> VerifyReport {
    let mut mismatches: Vec<String> = Vec::new();

    // Recompute component payloads.
    let audit_actual = sha256_hex(bundle.audit_chain_ndjson.as_bytes());
    let cas_actual = sha256_hex(&serde_json::to_vec(&bundle.cas_index).unwrap_or_default());

    if let Some(c) = bundle.components.get("audit_chain_ndjson") {
        if c.sha256 != audit_actual {
            mismatches.push(format!(
                "audit_chain_ndjson sha256 {} != recomputed {audit_actual}",
                c.sha256
            ));
        }
    } else {
        mismatches.push("missing component descriptor: audit_chain_ndjson".to_owned());
    }
    if let Some(c) = bundle.components.get("cas_index") {
        if c.sha256 != cas_actual {
            mismatches.push(format!(
                "cas_index sha256 {} != recomputed {cas_actual}",
                c.sha256
            ));
        }
    } else {
        mismatches.push("missing component descriptor: cas_index".to_owned());
    }

    // Recompute bundle hash.
    let summary = bundle_summary(
        &bundle.tenant_id,
        bundle.generated_at_ms,
        &bundle.components,
    );
    let bundle_actual = sha256_hex(summary.as_bytes());
    if bundle_actual != bundle.bundle_sha256 {
        mismatches.push(format!(
            "bundle_sha256 {} != recomputed {bundle_actual}",
            bundle.bundle_sha256
        ));
    }

    VerifyReport {
        archive: archive_label.to_owned(),
        ok: mismatches.is_empty(),
        mismatches,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Run `corelink tenant export --tenant-id <id> --output <path>`.
///
/// Fetches the full-history audit slice + the CAS index from the live
/// routes, assembles the content-addressed bundle, and writes it to
/// `output` as JSON.
pub async fn run_export(
    client: &CorelinkClient,
    tenant: &str,
    output: &Path,
    output_fmt: OutputFormat,
) -> Result<(), CliError> {
    let generated_at = now_ms();

    // 1. Audit-chain slice over [0, now).
    let audit_path = format!("/v1/audit/{tenant}/export?from=0&to={generated_at}");
    let (audit_bytes, anchor) = client
        .get_bytes_with_header(&audit_path, HEADER_CHAIN_HEAD_ANCHOR)
        .await?;
    let audit_ndjson = String::from_utf8(audit_bytes.to_vec())
        .map_err(|e| CliError::Other(format!("tenant export: audit slice not UTF-8: {e}")))?;

    // 2. CAS blob index. Tenant is a PATH segment on the real list route
    //    (`routes/cas.rs`, `CAS_LIST_ROUTE = "/v1/cas/{tenant}"`); the old
    //    `/v1/cas/list?tenant=` shape 403'd (axum matched `{tenant}="list"`).
    let cas_index = client.get_json(&format!("/v1/cas/{tenant}")).await?;

    // 3. Assemble + persist the bundle.
    let bundle = build_bundle(tenant, generated_at, audit_ndjson, cas_index, anchor);
    let json = serde_json::to_vec_pretty(&bundle).map_err(CliError::Json)?;
    std::fs::write(output, &json).map_err(CliError::Io)?;

    Formatter::new(output_fmt)
        .emit(&bundle)
        .map_err(CliError::Json)?;
    eprintln!(
        "note: bundle is a content-addressed JSON manifest (audit-chain slice + CAS index). \
         Blob bytes are retrievable via `corelink cas get <digest>`; a single-file .tar.zst \
         with all blob bytes + RBAC roster + DPA receipt needs the server bulk-export endpoint \
         (not yet available)."
    );
    Ok(())
}

/// Run `corelink tenant verify-export --archive <path>`.
pub fn run_verify_export(archive: &Path, output_fmt: OutputFormat) -> Result<(), CliError> {
    let raw = std::fs::read(archive).map_err(CliError::Io)?;
    let bundle: TenantExportBundle = serde_json::from_slice(&raw).map_err(CliError::Json)?;
    if bundle.schema != BUNDLE_SCHEMA {
        return Err(CliError::Other(format!(
            "tenant verify-export: unexpected schema {:?} (expected {BUNDLE_SCHEMA})",
            bundle.schema
        )));
    }
    let report = verify_bundle(&bundle, &archive.display().to_string());
    Formatter::new(output_fmt)
        .emit(&report)
        .map_err(CliError::Json)?;
    if !report.ok {
        return Err(CliError::Other(
            "tenant verify-export: bundle integrity check FAILED (see mismatches above)".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sample_bundle() -> TenantExportBundle {
        build_bundle(
            "tenant-xyz",
            1_700_000_000_000,
            "{\"event\":1}\n{\"manifest\":true}\n".to_owned(),
            serde_json::json!({"blobs": [{"hash": "abc", "size": 10, "created_at": "2026-07-01T00:00:00Z"}], "next_cursor": null}),
            Some("deadbeef".repeat(8)),
        )
    }

    #[test]
    fn build_then_verify_roundtrips_ok() {
        let b = sample_bundle();
        let report = verify_bundle(&b, "mem");
        assert!(
            report.ok,
            "fresh bundle must verify: {:?}",
            report.mismatches
        );
        assert!(report.mismatches.is_empty());
    }

    #[test]
    fn bundle_serialises_and_deserialises() {
        let b = sample_bundle();
        let json = serde_json::to_vec(&b).unwrap();
        let back: TenantExportBundle = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.tenant_id, "tenant-xyz");
        assert!(verify_bundle(&back, "mem").ok);
    }

    #[test]
    fn tampered_audit_payload_is_detected() {
        let mut b = sample_bundle();
        b.audit_chain_ndjson.push_str("tampered\n");
        let report = verify_bundle(&b, "mem");
        assert!(!report.ok, "tampered payload must fail verify");
        assert!(report
            .mismatches
            .iter()
            .any(|m| m.contains("audit_chain_ndjson")));
    }

    #[test]
    fn tampered_bundle_hash_is_detected() {
        let mut b = sample_bundle();
        b.bundle_sha256 = "00".repeat(32);
        let report = verify_bundle(&b, "mem");
        assert!(!report.ok);
        assert!(report
            .mismatches
            .iter()
            .any(|m| m.contains("bundle_sha256")));
    }

    #[test]
    fn components_cover_audit_and_cas() {
        let b = sample_bundle();
        assert!(b.components.contains_key("audit_chain_ndjson"));
        assert!(b.components.contains_key("cas_index"));
    }

    /// Kills the `run_export -> Ok(())` whole-body mutant: `tenant export`
    /// MUST fetch the audit slice + the CAS index (tenant as a PATH segment)
    /// and WRITE the assembled bundle to `output`. A no-op body writes no
    /// file, so reading the bundle back fails.
    #[tokio::test]
    async fn run_export_writes_the_assembled_bundle() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Audit-chain slice (query params vary — match on path only).
        Mock::given(method("GET"))
            .and(path("/v1/audit/t-test/export"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("{\"event\":1}\n{\"manifest\":true}\n"),
            )
            .mount(&server)
            .await;
        // CAS blob index (tenant is a PATH segment, not `/v1/cas/list`).
        Mock::given(method("GET"))
            .and(path("/v1/cas/t-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "blobs": [{"hash": "abc", "size": 10, "created_at": "2026-07-01T00:00:00Z"}],
                "next_cursor": null
            })))
            .mount(&server)
            .await;
        let client = CorelinkClient::for_test(server.uri(), Some("t-test".to_owned()));

        let out = tempfile::tempdir().unwrap();
        let dest = out.path().join("bundle.json");
        run_export(&client, "t-test", &dest, OutputFormat::Json)
            .await
            .expect("tenant export must succeed against the real routes");

        // The bundle file must exist and round-trip as a valid bundle.
        let raw = std::fs::read(&dest).expect("bundle file must be written");
        let bundle: TenantExportBundle = serde_json::from_slice(&raw).expect("valid bundle JSON");
        assert_eq!(bundle.tenant_id, "t-test");
        assert!(
            verify_bundle(&bundle, "written").ok,
            "written bundle must verify"
        );
    }
}
