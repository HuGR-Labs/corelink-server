//! `corelink stat` — existence + size for a digest (WI-S15-001).
//!
//! There is NO `/v1/cas/stat/…` route; the only per-object CAS surface is
//! `/v1/cas/<tenant>/<blake3>`. `stat` therefore issues a `HEAD` against that
//! route (axum's `get(handle_read)` also serves HEAD) using the logged-in
//! tenant — 200 (+ `content-length` when present) ⇒ present, 404 ⇒ absent.
//! Creation time / age / region are NOT exposed on a HEAD response, so they
//! are reported as `n/a` rather than invented.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Metadata derived from a CAS `HEAD` probe.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StatResult {
    /// BLAKE3 digest (bare hex, scheme prefix stripped).
    pub digest: String,
    /// Whether the blob exists in the tenant CAS.
    pub exists: bool,
    /// Blob size in bytes, when the server reports `content-length` on HEAD.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Tenant the digest was looked up under.
    pub tenant_id: String,
}

impl fmt::Display for StatResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Digest:  {}", self.digest)?;
        writeln!(f, "Exists:  {}", self.exists)?;
        let size = self
            .size_bytes
            .map(|n| format!("{n} bytes"))
            .unwrap_or_else(|| "n/a".to_owned());
        writeln!(f, "Size:    {size}")?;
        write!(f, "Tenant:  {}", self.tenant_id)
    }
}

/// Run `corelink stat <digest>`.
pub async fn run(
    client: &CorelinkClient,
    digest: &str,
    format: OutputFormat,
) -> Result<(), CliError> {
    // Strip any `blake3:`/`sha256:` scheme prefix the user may have supplied.
    let bare = digest
        .trim_start_matches("blake3:")
        .trim_start_matches("sha256:");

    let tenant = client
        .tenant_id()
        .ok_or_else(|| {
            CliError::Other(
                "stat: tenant_id not set — run `corelink login`/`corelink whoami` first."
                    .to_owned(),
            )
        })?
        .to_owned();

    let head = client.cas_head(bare).await?;

    let result = StatResult {
        digest: bare.to_owned(),
        exists: head.exists,
        size_bytes: head.size_bytes,
        tenant_id: tenant,
    };

    let fmt = Formatter::new(format);
    fmt.emit(&result).map_err(CliError::Json)?;

    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    #[test]
    fn stat_result_display_present_with_size() {
        let r = StatResult {
            digest: "abc".to_owned(),
            exists: true,
            size_bytes: Some(4096),
            tenant_id: "t-abc".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("4096 bytes"));
        assert!(s.contains("Exists:  true"));
        assert!(s.contains("t-abc"));
    }

    #[test]
    fn stat_result_display_absent_shows_na_size() {
        let r = StatResult {
            digest: "abc".to_owned(),
            exists: false,
            size_bytes: None,
            tenant_id: "t-abc".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("Exists:  false"));
        assert!(s.contains("Size:    n/a"));
    }

    #[test]
    fn stat_result_serialises() {
        let r = StatResult {
            digest: "d".to_owned(),
            exists: true,
            size_bytes: Some(10),
            tenant_id: "t".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"exists\""));
        assert!(json.contains("\"size_bytes\""));
        assert!(json.contains("\"tenant_id\""));
    }

    #[test]
    fn stat_result_omits_size_when_absent() {
        let r = StatResult {
            digest: "d".to_owned(),
            exists: false,
            size_bytes: None,
            tenant_id: "t".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("\"size_bytes\""));
    }
}
