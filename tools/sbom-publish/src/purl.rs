#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
//! PURL normalisation for CycloneDX SBOMs.
//!
//! # Normalisation rules
//!
//! 1. All component PURLs emitted by `cargo-cyclonedx` use the `pkg:cargo/<name>@<version>`
//!    ecosystem prefix.
//! 2. Dependency-Track interprets `pkg:cargo/…` correctly; however some older DT versions and
//!    third-party scanners require `pkg:crates/<name>@<version>`.  This module emits **both**:
//!    - primary PURL: `pkg:cargo/<name>@<version>` (canonical)
//!    - DT alias: `pkg:crates/<name>@<version>`
//! 3. **Workspace member discriminator**: components whose name matches a workspace-local crate
//!    receive a `?vcs_url=https://github.com/HumanGuardrail/corelink-server` qualifier on the
//!    primary PURL to prevent confusion with a hypothetical malicious crate published on
//!    crates.io with the same name.
//! 4. **Patched deps** (`[patch.crates-io]`): a `cdx:patched_locally` property is injected
//!    when the component name appears in the supplied patch list.

/// Canonical VCS URL for workspace member discriminator.
pub const WORKSPACE_VCS_URL: &str = "https://github.com/HumanGuardrail/corelink-server";

/// Normalise a single PURL string from cargo-cyclonedx output.
///
/// Returns `(primary_purl, dt_alias_purl)`.
///
/// # Arguments
///
/// * `raw_purl` — PURL string from `components[*].purl`.
/// * `is_workspace_member` — if `true`, append `?vcs_url=…` discriminator.
///
/// # Example
///
/// ```
/// use sbom_publish::purl::normalise_purl;
///
/// let (primary, alias) = normalise_purl("pkg:cargo/serde@1.0.0", false);
/// assert_eq!(primary, "pkg:cargo/serde@1.0.0");
/// assert_eq!(alias, "pkg:crates/serde@1.0.0");
/// ```
pub fn normalise_purl(raw_purl: &str, is_workspace_member: bool) -> (String, String) {
    // Strip existing qualifiers for reconstruction
    let base = raw_purl.split('?').next().unwrap_or(raw_purl);

    let primary = if is_workspace_member {
        format!("{}?vcs_url={}", base, WORKSPACE_VCS_URL)
    } else {
        base.to_owned()
    };

    // Build DT alias: replace `pkg:cargo/` with `pkg:crates/`
    let dt_alias = if let Some(rest) = base.strip_prefix("pkg:cargo/") {
        format!("pkg:crates/{rest}")
    } else {
        base.to_owned()
    };

    (primary, dt_alias)
}

/// Apply PURL normalisation to a mutable CycloneDX JSON document in-place.
///
/// For each component, replaces `purl` with the normalised primary PURL and
/// appends `"dt_purl_alias"` as a custom property for DT compatibility.
///
/// # Arguments
///
/// * `sbom_json` — mutable CycloneDX 1.5+ JSON value.
/// * `workspace_members` — set of crate names that are workspace-local.
/// * `patched_crates` — set of crate names with `[patch.crates-io]` entries.
pub fn normalise_sbom_purls(
    sbom_json: &mut serde_json::Value,
    workspace_members: &[&str],
    patched_crates: &[&str],
) {
    let Some(components) = sbom_json
        .get_mut("components")
        .and_then(|c| c.as_array_mut())
    else {
        return;
    };

    for component in components.iter_mut() {
        let name = component
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_owned();

        let is_workspace = workspace_members.contains(&name.as_str());
        let is_patched = patched_crates.contains(&name.as_str());

        if let Some(purl_val) = component
            .get("purl")
            .and_then(|p| p.as_str())
            .map(|s| s.to_owned())
        {
            let (primary, alias) = normalise_purl(&purl_val, is_workspace);
            if let Some(obj) = component.as_object_mut() {
                obj.insert("purl".to_owned(), serde_json::Value::String(primary));
                // Store DT alias as custom property
                let props = obj
                    .entry("properties".to_owned())
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                if let Some(arr) = props.as_array_mut() {
                    arr.push(serde_json::json!({
                        "name": "dt:purl_alias",
                        "value": alias
                    }));
                    if is_patched {
                        arr.push(serde_json::json!({
                            "name": "cdx:patched_locally",
                            "value": "true"
                        }));
                    }
                }
            }
        }
    }
}

/// Validate that a PURL string is syntactically valid (basic check).
///
/// Returns `true` if the PURL starts with `pkg:` and contains a name segment.
///
/// # Example
///
/// ```
/// use sbom_publish::purl::is_valid_purl;
///
/// assert!(is_valid_purl("pkg:cargo/serde@1.0.0"));
/// assert!(!is_valid_purl("not-a-purl"));
/// ```
pub fn is_valid_purl(purl: &str) -> bool {
    if !purl.starts_with("pkg:") {
        return false;
    }
    // Must have at least `pkg:<type>/<name>`
    let after_scheme = &purl["pkg:".len()..];
    let mut parts = after_scheme.splitn(2, '/');
    let pkg_type = parts.next().unwrap_or("");
    let name_part = parts.next().unwrap_or("");
    !pkg_type.is_empty() && !name_part.is_empty()
}
