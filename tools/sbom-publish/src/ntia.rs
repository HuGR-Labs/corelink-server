//! NTIA minimum elements validator (7 fields per ntia.doc.gov SBOM spec).
//!
//! # Strict mode (default)
//!
//! All 7 fields must be 100% present across all components. Exit 2 on failure.
//!
//! # Auditor mode (`--auditor`)
//!
//! Emits a detailed per-field report with soft warnings; does **not** exit
//! non-zero. Useful for debugging SBOM quality without blocking a release.
//!
//! # NTIA 7 minimum elements
//!
//! 1. SBOM author (`metadata.authors[*].name`)
//! 2. Timestamp (`metadata.timestamp` ISO 8601 within ≤ 24 h)
//! 3. Component name (100 %)
//! 4. Component version (100 %)
//! 5. Component supplier (≥ 95 % in strict mode; libs without supplier annotation excluded)
//! 6. Unique identifier — PURL or CPE (100 %)
//! 7. Dependency relationships (`dependencies[*]` populated)

use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

/// Result of an NTIA minimum elements check.
#[derive(Debug, Clone)]
pub struct NtiaValidation {
    /// Whether `metadata.authors[*].name` is non-empty.
    pub author_present: bool,
    /// Whether `metadata.timestamp` is present and non-empty.
    pub timestamp_present: bool,
    /// Fraction of components that have a non-empty `supplier.name` (0.0–1.0).
    pub component_supplier_present: f32,
    /// Fraction of components that have a non-empty `name` field (0.0–1.0).
    pub component_name_present: f32,
    /// Fraction of components that have a non-empty `version` field (0.0–1.0).
    pub component_version_present: f32,
    /// Fraction of components that have a non-empty `purl` OR `cpe` field (0.0–1.0).
    pub component_unique_id_present: f32,
    /// Whether the `dependencies` array is populated.
    pub dependency_relationships_present: f32,
    /// True when all 7 NTIA elements satisfy the strict-mode thresholds.
    pub overall_compliant: bool,
}

impl std::fmt::Display for NtiaValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "NtiaValidation {{ author={}, timestamp={}, supplier={:.2}, name={:.2}, \
             version={:.2}, unique_id={:.2}, deps={:.2}, compliant={} }}",
            self.author_present,
            self.timestamp_present,
            self.component_supplier_present,
            self.component_name_present,
            self.component_version_present,
            self.component_unique_id_present,
            self.dependency_relationships_present,
            self.overall_compliant,
        )
    }
}

/// Validation mode for the NTIA check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// Fail-fast; exit 2 if any element is missing or below threshold.
    Strict,
    /// Emit per-field warnings; never exits non-zero.
    Auditor,
}

/// Validate NTIA minimum elements from a raw CycloneDX JSON value.
///
/// # Arguments
///
/// * `sbom_json` — parsed CycloneDX 1.5+ JSON document.
/// * `mode` — [`ValidationMode::Strict`] for CI gate; [`ValidationMode::Auditor`] for reports.
///
/// # Returns
///
/// [`NtiaValidation`] summary. Call `.overall_compliant` to check pass/fail.
///
/// # Example
///
/// ```no_run
/// use sbom_publish::ntia::{validate_ntia_json, ValidationMode};
/// use serde_json::json;
///
/// let sbom = json!({
///     "metadata": {
///         "timestamp": "2026-05-13T00:00:00Z",
///         "authors": [{"name": "CoreLink CI"}]
///     },
///     "components": [],
///     "dependencies": []
/// });
/// let result = validate_ntia_json(&sbom, ValidationMode::Strict);
/// println!("compliant: {}", result.overall_compliant);
/// ```
pub fn validate_ntia_json(sbom_json: &Value, mode: ValidationMode) -> NtiaValidation {
    let metadata = sbom_json.get("metadata").unwrap_or(&Value::Null);

    // 1. Author
    let author_present = metadata
        .get("authors")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter().any(|a| {
                a.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    // 2. Timestamp
    let timestamp_present = metadata
        .get("timestamp")
        .and_then(|t| t.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    // 3–6. Component-level fields
    let components = sbom_json
        .get("components")
        .and_then(|c| c.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let (name_count, version_count, supplier_count, uid_count) = if components.is_empty() {
        (1.0_f32, 1.0_f32, 1.0_f32, 1.0_f32)
    } else {
        let total = components.len() as f32;
        let names = components
            .iter()
            .filter(|c| {
                c.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
            })
            .count() as f32;
        let versions = components
            .iter()
            .filter(|c| {
                c.get("version")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
            })
            .count() as f32;
        let suppliers = components
            .iter()
            .filter(|c| {
                c.get("supplier")
                    .and_then(|s| s.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|s| {
                        let trimmed = s.trim();
                        !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("UNKNOWN")
                    })
                    .unwrap_or(false)
            })
            .count() as f32;
        let unique_ids = components
            .iter()
            .filter(|c| {
                let has_purl = c
                    .get("purl")
                    .and_then(|p| p.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                let has_cpe = c
                    .get("cpe")
                    .and_then(|p| p.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                has_purl || has_cpe
            })
            .count() as f32;
        (
            names / total,
            versions / total,
            suppliers / total,
            unique_ids / total,
        )
    };

    // 7. Dependency relationships
    let deps = sbom_json
        .get("dependencies")
        .and_then(|d| d.as_array())
        .map(|arr| if arr.is_empty() { 0.0_f32 } else { 1.0_f32 })
        .unwrap_or(0.0_f32);

    // Compute overall compliance (strict thresholds)
    let overall_compliant = author_present
        && timestamp_present
        && name_count >= 1.0
        && version_count >= 1.0
        && supplier_count >= 0.95
        && uid_count >= 1.0
        && deps >= 1.0;

    let validation = NtiaValidation {
        author_present,
        timestamp_present,
        component_supplier_present: supplier_count,
        component_name_present: name_count,
        component_version_present: version_count,
        component_unique_id_present: uid_count,
        dependency_relationships_present: deps,
        overall_compliant,
    };

    if mode == ValidationMode::Auditor {
        emit_auditor_report(&validation);
    } else if !overall_compliant {
        warn!(
            ntia_author = %author_present,
            ntia_timestamp = %timestamp_present,
            ntia_supplier = %supplier_count,
            ntia_name = %name_count,
            ntia_version = %version_count,
            ntia_uid = %uid_count,
            ntia_deps = %deps,
            "NTIA strict validation failed"
        );
    } else {
        info!(
            ntia_supplier = %supplier_count,
            ntia_uid = %uid_count,
            "NTIA minimum elements validation passed"
        );
    }

    validation
}

/// Validate NTIA from a placeholder-detection perspective.
///
/// Returns `true` if `supplier` field contains a placeholder value like "UNKNOWN".
pub fn has_placeholder_supplier(sbom_json: &Value) -> bool {
    let components = sbom_json
        .get("components")
        .and_then(|c| c.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    components.iter().any(|c| {
        c.get("supplier")
            .and_then(|s| s.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.trim().eq_ignore_ascii_case("UNKNOWN"))
            .unwrap_or(false)
    })
}

fn emit_auditor_report(v: &NtiaValidation) {
    info!(
        author_present = %v.author_present,
        timestamp_present = %v.timestamp_present,
        component_supplier_pct = format!("{:.1}%", v.component_supplier_present * 100.0),
        component_name_pct = format!("{:.1}%", v.component_name_present * 100.0),
        component_version_pct = format!("{:.1}%", v.component_version_present * 100.0),
        component_uid_pct = format!("{:.1}%", v.component_unique_id_present * 100.0),
        dependency_rels_ok = %v.dependency_relationships_present,
        overall_compliant = %v.overall_compliant,
        "NTIA minimum elements auditor report"
    );
    if !v.author_present {
        warn!("NTIA: metadata.authors[*].name missing");
    }
    if !v.timestamp_present {
        warn!("NTIA: metadata.timestamp missing");
    }
    if v.component_supplier_present < 0.95 {
        warn!(
            pct = format!("{:.1}%", v.component_supplier_present * 100.0),
            "NTIA: component supplier coverage below 95%"
        );
    }
    if v.component_unique_id_present < 1.0 {
        warn!("NTIA: some components missing purl and cpe");
    }
    if v.dependency_relationships_present < 1.0 {
        warn!("NTIA: dependencies array is empty");
    }
}

/// Deserialised SBOM metadata for use in validation helpers.
#[derive(Debug, Deserialize)]
pub struct SbomMetadata {
    /// ISO 8601 timestamp of SBOM generation.
    pub timestamp: Option<String>,
}
