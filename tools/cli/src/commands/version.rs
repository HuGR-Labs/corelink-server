//! `corelink version` — print version + git rev + SLSA attestation link (WI-S15-001).

use std::fmt;

use serde::Serialize;

use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Version information.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct VersionInfo {
    /// SemVer version from `Cargo.toml`.
    pub version: String,
    /// Git commit SHA (set at build time via env var `GIT_COMMIT_SHA`).
    pub git_rev: String,
    /// Build timestamp (deterministic via `SOURCE_DATE_EPOCH`).
    pub build_timestamp: String,
    /// SLSA attestation link (S-12 supply chain alignment).
    pub slsa_attestation: String,
    /// Target triple this binary was compiled for.
    pub target_triple: String,
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "corelink {}", self.version)?;
        writeln!(f, "  git rev:      {}", self.git_rev)?;
        writeln!(f, "  built:        {}", self.build_timestamp)?;
        writeln!(f, "  target:       {}", self.target_triple)?;
        write!(f, "  attestation:  {}", self.slsa_attestation)
    }
}

/// Run `corelink version`.
pub fn run(format: OutputFormat) -> Result<(), CliError> {
    let info = build_version_info();
    let fmt = Formatter::new(format);
    fmt.emit(&info).map_err(CliError::Json)?;
    Ok(())
}

/// Build version info from compile-time env vars.
#[must_use]
pub fn build_version_info() -> VersionInfo {
    let version = env!("CARGO_PKG_VERSION").to_owned();
    let git_rev = option_env!("GIT_COMMIT_SHA")
        .unwrap_or("unknown")
        .to_owned();
    let build_timestamp = option_env!("SOURCE_DATE_EPOCH")
        .map(|epoch| format!("epoch:{epoch}"))
        .unwrap_or_else(|| "unknown".to_owned());
    let target_triple = option_env!("TARGET")
        .unwrap_or(std::env::consts::ARCH)
        .to_owned();
    let slsa_attestation = format!(
        "https://corelink.humangr.com/attestations/cli/{version}/{git_rev}/slsa3.json"
    );

    VersionInfo {
        version,
        git_rev,
        build_timestamp,
        slsa_attestation,
        target_triple,
    }
}

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn version_info_has_semver() {
        let info = build_version_info();
        // Version should be in X.Y.Z format.
        assert!(info.version.contains('.'), "version '{}'", info.version);
    }

    #[test]
    fn version_info_has_slsa_link() {
        let info = build_version_info();
        assert!(info.slsa_attestation.starts_with("https://corelink.humangr.com/attestations/cli/"));
    }

    #[test]
    fn version_info_display_contains_version() {
        let info = build_version_info();
        let s = format!("{info}");
        assert!(s.contains("corelink"));
        assert!(s.contains("git rev:"));
    }

    #[test]
    fn version_info_serialises() {
        let info = build_version_info();
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"version\""));
        assert!(json.contains("\"slsa_attestation\""));
    }
}
