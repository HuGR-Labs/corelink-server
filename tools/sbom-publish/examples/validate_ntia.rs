//! Example: validate NTIA minimum elements for an existing sbom.cdx.json.
//!
//! # Usage
//!
//! ```sh
//! # Strict mode (CI gate — exits 2 on failure):
//! cargo run --example validate_ntia -- /tmp/sbom.cdx.json
//!
//! # Auditor mode (detailed report, no exit 2):
//! cargo run --example validate_ntia -- /tmp/sbom.cdx.json --auditor
//! ```

#![forbid(unsafe_code)]
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::expect_used)]

use sbom_publish::ntia::{validate_ntia_json, ValidationMode};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        eprintln!("Usage: validate_ntia <sbom.cdx.json> [--auditor]");
        std::process::exit(1);
    });
    let auditor = args.any(|a| a == "--auditor");
    let mode = if auditor { ValidationMode::Auditor } else { ValidationMode::Strict };

    let sbom_bytes = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("Cannot read {path}: {e}");
        std::process::exit(1);
    });

    let sbom_json: serde_json::Value = serde_json::from_slice(&sbom_bytes).unwrap_or_else(|e| {
        eprintln!("Invalid JSON: {e}");
        std::process::exit(1);
    });

    let result = validate_ntia_json(&sbom_json, mode);
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "overall_compliant":               result.overall_compliant,
        "author_present":                  result.author_present,
        "timestamp_present":               result.timestamp_present,
        "component_name_present":          result.component_name_present,
        "component_version_present":       result.component_version_present,
        "component_supplier_present":      result.component_supplier_present,
        "component_unique_id_present":     result.component_unique_id_present,
        "dependency_relationships_present":result.dependency_relationships_present,
    })).expect("serialise"));

    if !result.overall_compliant && !auditor {
        eprintln!("NTIA strict mode: validation FAILED");
        std::process::exit(2);
    }
}
