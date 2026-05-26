//! `corelink stat` — show metadata for a digest (WI-S15-001).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Metadata returned by the stat endpoint.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StatResult {
    /// BLAKE3 digest.
    pub digest: String,
    /// Blob size in bytes.
    pub size_bytes: u64,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// Age in human-readable form.
    pub age: String,
    /// Storage region.
    pub region: String,
    /// Pseudonymised tenant ID.
    pub tenant_id_pseudonym: String,
}

impl fmt::Display for StatResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Digest:     {}", self.digest)?;
        writeln!(f, "Size:       {} bytes", self.size_bytes)?;
        writeln!(f, "Created:    {}", self.created_at)?;
        writeln!(f, "Age:        {}", self.age)?;
        writeln!(f, "Region:     {}", self.region)?;
        write!(f, "Tenant:     {}", self.tenant_id_pseudonym)
    }
}

/// Run `corelink stat <digest>`.
pub async fn run(
    client: &CorelinkClient,
    digest: &str,
    format: OutputFormat,
) -> Result<(), CliError> {
    let path = format!("/v1/cas/stat/{digest}");
    let raw = client.get_json(&path).await?;

    match serde_json::from_value::<StatResult>(raw.clone()) {
        Ok(stat) => {
            let fmt = Formatter::new(format);
            fmt.emit(&stat).map_err(CliError::Json)?;
        }
        Err(_) => {
            let fmt = Formatter::new(format);
            fmt.emit_json_value(&raw).map_err(CliError::Json)?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn stat_result_display_contains_fields() {
        let r = StatResult {
            digest: "blake3:abc".to_owned(),
            size_bytes: 4096,
            created_at: "2026-05-14T00:00:00Z".to_owned(),
            age: "2 days".to_owned(),
            region: "wnam".to_owned(),
            tenant_id_pseudonym: "t-abc***".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("4096 bytes"));
        assert!(s.contains("wnam"));
    }

    #[test]
    fn stat_result_serialises() {
        let r = StatResult {
            digest: "d".to_owned(),
            size_bytes: 0,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            age: "0s".to_owned(),
            region: "enam".to_owned(),
            tenant_id_pseudonym: "t-***".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"region\""));
    }
}
