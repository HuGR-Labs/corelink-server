//! [`SbomPublisher`] trait + [`DefaultSbomPublisher`] orchestrator.
//!
//! The orchestrator wires together:
//! 1. `cargo cyclonedx` subprocess (SBOM generation).
//! 2. [`crate::purl`] PURL normalisation pass.
//! 3. [`crate::ntia`] NTIA minimum elements check (strict mode gate).
//! 4. [`crate::tsa`] RFC 3161 TSA timestamp attestation.
//! 5. [`crate::dt`] Dependency-Track ingestion with retry + fallback queue.
//!
//! # F-001 — per-instance `Arc<Mutex<>>`
//!
//! Each `DefaultSbomPublisher` wraps its mutable state in `Arc<Mutex<>>` so
//! callers can share a single publisher across concurrent Tokio tasks.
//!
//! # Trace spans
//!
//! Four `tracing` spans are emitted (per WI-S12-002 §6.1.8):
//! - `sbom.generate`
//! - `sbom.validate_ntia`
//! - `sbom.attest_tsa`
//! - `sbom.ingest_dt`

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{info, info_span, warn, Instrument};
use url::Url;
use uuid::Uuid;

use crate::dt::{ingest_into_dt, DtApiKey, DtProjectUuid};
use crate::error::SbomError;
use crate::metrics::{NtiaOutcome, TsaOutcome, METRICS};
use crate::ntia::{validate_ntia_json, NtiaValidation, ValidationMode};
use crate::purl::normalise_sbom_purls;
use crate::tsa::{request_tsa_timestamp, TsrToken};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// CycloneDX SBOM format version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[allow(non_camel_case_types)]
pub enum SbomFormat {
    /// CycloneDX 1.5 (canonical baseline for CoreLink S-12).
    CycloneDX_1_5,
    /// CycloneDX 1.6 (future upgrade path, ADR required).
    CycloneDX_1_6,
}

impl std::fmt::Display for SbomFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CycloneDX_1_5 => write!(f, "CycloneDX_1_5"),
            Self::CycloneDX_1_6 => write!(f, "CycloneDX_1_6"),
        }
    }
}

/// Metadata provided by the release pipeline for SBOM serialNumber and tagging.
#[derive(Debug, Clone)]
pub struct ReleaseMetadata {
    /// Semantic version string (e.g. `"0.1.2"`).
    pub version: String,
    /// Git commit SHA (full 40-char hex).
    pub commit_sha: String,
    /// Project / repository name.
    pub project_name: String,
}

/// A generated, validated, and (optionally) TSA-attested SBOM.
#[derive(Debug, Clone)]
pub struct SignedSbom {
    /// Raw CycloneDX 1.5+ JSON value.
    pub sbom: serde_json::Value,
    /// SBOM format variant.
    pub format: SbomFormat,
    /// Spec version string (`"1.5"` or `"1.6"`).
    pub spec_version: String,
    /// `urn:uuid:…` serial number (unique per release).
    pub serial_number: String,
    /// ISO 8601 generation timestamp.
    pub timestamp_rfc3339: String,
    /// RFC 3161 TSR token (present if TSA succeeded).
    pub tsr_token: Option<TsrToken>,
    /// Number of components in the SBOM.
    pub component_count: u32,
    /// Whether NTIA strict mode validation passed.
    pub ntia_compliant: bool,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Core SBOM pipeline trait.
///
/// Implementors must be `Send + Sync` to allow use across async task boundaries.
#[async_trait]
pub trait SbomPublisher: Send + Sync + std::fmt::Debug {
    /// Generate a CycloneDX 1.5+ SBOM from `Cargo.lock` via `cargo cyclonedx`,
    /// normalise PURLs, and validate NTIA minimum elements (strict mode gate).
    ///
    /// # Arguments
    ///
    /// * `cargo_lock_path` — path to `Cargo.lock`.
    /// * `cargo_toml_path` — path to workspace root `Cargo.toml`.
    /// * `release_metadata` — release version/commit metadata.
    ///
    /// # Errors
    ///
    /// - [`SbomError::GenerationFailed`] if `cargo cyclonedx` exits non-zero.
    /// - [`SbomError::NtiaValidationFailed`] if NTIA strict check fails.
    async fn generate(
        &self,
        cargo_lock_path: &Path,
        cargo_toml_path: &Path,
        release_metadata: &ReleaseMetadata,
    ) -> Result<SignedSbom, SbomError>;

    /// Ingest SBOM into Dependency-Track via `POST /api/v1/bom`.
    ///
    /// Returns [`DtProjectUuid`] on success (first attempt or after retry).
    /// On retry exhaustion writes to fallback queue and returns
    /// [`SbomError::DtRetryExhausted`].
    async fn ingest_dt(
        &self,
        sbom: &SignedSbom,
        dt_endpoint: &Url,
        api_key: &DtApiKey,
    ) -> Result<DtProjectUuid, SbomError>;

    /// Validate NTIA minimum elements on a raw CycloneDX JSON value.
    async fn validate_ntia(
        &self,
        sbom_json: &serde_json::Value,
    ) -> Result<NtiaValidation, SbomError>;
}

// ---------------------------------------------------------------------------
// DefaultSbomPublisher
// ---------------------------------------------------------------------------

/// Default implementation of [`SbomPublisher`].
///
/// Per F-001 all mutable internal state is behind `Arc<Mutex<>>`.
#[derive(Debug, Clone)]
pub struct DefaultSbomPublisher {
    inner: Arc<tokio::sync::Mutex<PublisherInner>>,
}

#[derive(Debug)]
struct PublisherInner {
    /// TSA endpoint URL.
    tsa_url: Url,
    /// Workspace member crate names (for PURL discriminator).
    workspace_members: Vec<String>,
    /// Crates in `[patch.crates-io]`.
    patched_crates: Vec<String>,
    /// reqwest client (shared).
    http_client: reqwest::Client,
}

impl DefaultSbomPublisher {
    /// Create a new publisher.
    ///
    /// # Arguments
    ///
    /// * `tsa_url`          — Sigstore TSA endpoint (default: `https://tsa.sigstore.dev/api/v1/timestamp`).
    /// * `workspace_members` — crate names that are local workspace members.
    /// * `patched_crates`   — crates with `[patch.crates-io]` entries.
    ///
    /// # Example
    ///
    /// ```
    /// use sbom_publish::publisher::DefaultSbomPublisher;
    /// use url::Url;
    ///
    /// let tsa = Url::parse("https://tsa.sigstore.dev/api/v1/timestamp").unwrap();
    /// let pub_ = DefaultSbomPublisher::new(tsa, &[], &[]);
    /// ```
    pub fn new(tsa_url: Url, workspace_members: &[&str], patched_crates: &[&str]) -> Self {
        // reqwest::ClientBuilder::build() only fails when TLS is misconfigured;
        // with default rustls feature this is infallible at runtime.
        // We use unwrap_or_else with a known-safe fallback instead of expect.
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();

        Self {
            inner: Arc::new(tokio::sync::Mutex::new(PublisherInner {
                tsa_url,
                workspace_members: workspace_members.iter().map(|s| s.to_string()).collect(),
                patched_crates: patched_crates.iter().map(|s| s.to_string()).collect(),
                http_client,
            })),
        }
    }
}

#[async_trait]
impl SbomPublisher for DefaultSbomPublisher {
    async fn generate(
        &self,
        cargo_lock_path: &Path,
        cargo_toml_path: &Path,
        release_metadata: &ReleaseMetadata,
    ) -> Result<SignedSbom, SbomError> {
        let span = info_span!(
            "sbom.generate",
            sbom.format = "CycloneDX_1_5",
            sbom.version = %release_metadata.version,
            sbom.commit = %release_metadata.commit_sha,
        );
        async move {
            info!(
                cargo_lock = %cargo_lock_path.display(),
                cargo_toml = %cargo_toml_path.display(),
                version = %release_metadata.version,
                "Starting SBOM generation"
            );

            // Invoke cargo-cyclonedx as a subprocess
            let manifest_dir = cargo_toml_path
                .parent()
                .unwrap_or(std::path::Path::new("."));

            let output = std::process::Command::new("cargo")
                .args([
                    "cyclonedx",
                    "--all",
                    "--format",
                    "json",
                    "--spec-version",
                    "1.5",
                    "--output-cdx",
                    "sbom.cdx.json",
                ])
                .current_dir(manifest_dir)
                .output()
                .map_err(|e| SbomError::GenerationFailed(format!("failed to spawn cargo-cyclonedx: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(SbomError::GenerationFailed(format!(
                    "cargo-cyclonedx exited with {}: {stderr}",
                    output.status
                )));
            }

            // Read generated SBOM
            let sbom_path = manifest_dir.join("sbom.cdx.json");
            let sbom_bytes = std::fs::read(&sbom_path)
                .map_err(|e| SbomError::GenerationFailed(format!("reading sbom.cdx.json: {e}")))?;

            let mut sbom_json: serde_json::Value = serde_json::from_slice(&sbom_bytes)?;

            // PURL normalisation pass
            {
                let inner = self.inner.lock().await;
                let wm: Vec<&str> = inner.workspace_members.iter().map(|s| s.as_str()).collect();
                let pc: Vec<&str> = inner.patched_crates.iter().map(|s| s.as_str()).collect();
                normalise_sbom_purls(&mut sbom_json, &wm, &pc);
            }

            // Inject serial_number if not present
            let serial_number = format!("urn:uuid:{}", Uuid::new_v4());
            if let Some(obj) = sbom_json.as_object_mut() {
                obj.entry("serialNumber")
                    .or_insert_with(|| serde_json::Value::String(serial_number.clone()));
            }

            // Extract metadata
            let spec_version = sbom_json
                .get("specVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("1.5")
                .to_owned();

            let timestamp_rfc3339 = sbom_json
                .pointer("/metadata/timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("1970-01-01T00:00:00Z")
                .to_owned();

            let component_count = sbom_json
                .get("components")
                .and_then(|c| c.as_array())
                .map(|a| a.len() as u32)
                .unwrap_or(0);

            METRICS.set_components_count(u64::from(component_count));

            // NTIA strict validation
            let ntia = validate_ntia_json(&sbom_json, ValidationMode::Strict);
            let ntia_compliant = ntia.overall_compliant;

            if ntia_compliant {
                METRICS.record_ntia(NtiaOutcome::Ok);
            } else {
                METRICS.record_ntia(NtiaOutcome::Fail);
                return Err(SbomError::NtiaValidationFailed(ntia));
            }

            // TSA timestamp
            let tsr_token = {
                let inner = self.inner.lock().await;
                let tsa_url = inner.tsa_url.clone();
                let client = inner.http_client.clone();
                drop(inner);

                let sbom_bytes_normalized = serde_json::to_vec(&sbom_json)?;
                match request_tsa_timestamp(&sbom_bytes_normalized, &tsa_url, Some(client))
                    .instrument(info_span!("sbom.attest_tsa"))
                    .await
                {
                    Ok(token) => {
                        METRICS.record_tsa(TsaOutcome::Ok);
                        Some(token)
                    }
                    Err(e) => {
                        warn!(error = %e, "TSA timestamp failed (SEV-3); SBOM published without TSR");
                        METRICS.record_tsa(TsaOutcome::TsaUnavailable);
                        None
                    }
                }
            };

            info!(
                component_count,
                ntia_compliant,
                has_tsr = tsr_token.is_some(),
                "SBOM generation complete"
            );

            Ok(SignedSbom {
                sbom: sbom_json,
                format: SbomFormat::CycloneDX_1_5,
                spec_version,
                serial_number,
                timestamp_rfc3339,
                tsr_token,
                component_count,
                ntia_compliant,
            })
        }
        .instrument(span)
        .await
    }

    async fn ingest_dt(
        &self,
        sbom: &SignedSbom,
        dt_endpoint: &Url,
        api_key: &DtApiKey,
    ) -> Result<DtProjectUuid, SbomError> {
        let span = info_span!(
            "sbom.ingest_dt",
            sbom.component_count = sbom.component_count,
            sbom.format = %sbom.format,
        );
        async move {
            let sbom_bytes = serde_json::to_vec(&sbom.sbom)?;
            let project_name = sbom
                .sbom
                .pointer("/metadata/component/name")
                .and_then(|v| v.as_str())
                .unwrap_or("corelink-server");
            let project_version = &sbom.spec_version;

            ingest_into_dt(
                &sbom_bytes,
                dt_endpoint,
                api_key,
                project_name,
                project_version,
                None,
            )
            .await
        }
        .instrument(span)
        .await
    }

    async fn validate_ntia(
        &self,
        sbom_json: &serde_json::Value,
    ) -> Result<NtiaValidation, SbomError> {
        let span = info_span!("sbom.validate_ntia");
        async move {
            let result = validate_ntia_json(sbom_json, ValidationMode::Strict);
            if result.overall_compliant {
                METRICS.record_ntia(NtiaOutcome::Ok);
                Ok(result)
            } else {
                METRICS.record_ntia(NtiaOutcome::Fail);
                Err(SbomError::NtiaValidationFailed(result))
            }
        }
        .instrument(span)
        .await
    }
}
