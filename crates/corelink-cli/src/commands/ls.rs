//! `corelink ls` — list CAS/AC entries for a tenant (WI-S15-001).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// A single CAS/AC entry returned by the ls endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LsEntry {
    /// Content-addressed digest (BLAKE3).
    pub digest: String,
    /// Blob size in bytes.
    pub size_bytes: u64,
    /// Pseudonymised tenant prefix.
    pub tenant_prefix: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

impl fmt::Display for LsEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:<72}  {:>12} bytes  {}  {}",
            self.digest, self.size_bytes, self.tenant_prefix, self.created_at
        )
    }
}

/// Response wrapper for the ls endpoint.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LsResponse {
    /// List of entries.
    pub entries: Vec<LsEntry>,
    /// Pagination cursor for the next page (`None` = last page).
    pub next_cursor: Option<String>,
    /// Total matching count (estimated).
    pub total_count: u64,
}

impl fmt::Display for LsResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:<72}  {:>12}  {:>12}  CREATED_AT", "DIGEST", "SIZE", "TENANT")?;
        writeln!(f, "{}", "-".repeat(120))?;
        for e in &self.entries {
            writeln!(f, "{e}")?;
        }
        write!(
            f,
            "({} total{})",
            self.total_count,
            if self.next_cursor.is_some() { "; more pages available" } else { "" }
        )
    }
}

/// Run `corelink ls --tenant <id> [--prefix <p>] [--limit n] [--cursor c]`.
pub async fn run(
    client: &CorelinkClient,
    tenant: &str,
    prefix: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let mut path = format!("/v1/cas/list?tenant={tenant}");
    if let Some(p) = prefix {
        path.push_str(&format!("&prefix={p}"));
    }
    if let Some(l) = limit {
        path.push_str(&format!("&limit={l}"));
    }
    if let Some(c) = cursor {
        path.push_str(&format!("&cursor={c}"));
    }

    let raw = client.get_json(&path).await?;

    // Attempt to parse as LsResponse; fall back to raw JSON display.
    match serde_json::from_value::<LsResponse>(raw.clone()) {
        Ok(resp) => {
            let fmt = Formatter::new(format);
            fmt.emit(&resp).map_err(CliError::Json)?;
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
    fn ls_entry_display_contains_digest() {
        let e = LsEntry {
            digest: "blake3:abc123".to_owned(),
            size_bytes: 1024,
            tenant_prefix: "acme".to_owned(),
            created_at: "2026-05-14T00:00:00Z".to_owned(),
        };
        let s = format!("{e}");
        assert!(s.contains("blake3:abc123"));
        assert!(s.contains("1024"));
    }

    #[test]
    fn ls_response_serialises_to_json() {
        let r = LsResponse {
            entries: vec![],
            next_cursor: None,
            total_count: 0,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"entries\""));
        assert!(json.contains("\"total_count\""));
    }
}
