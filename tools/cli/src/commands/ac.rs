//! `corelink ac` — action cache operations.
//!
//! Subcommands:
//! - `corelink ac put <digest> <result>` — store a result blob keyed by digest.
//! - `corelink ac get <digest>`           — retrieve a result blob.
//!
//! API paths: `PUT/GET /v1/ac/<tenant>/<digest>`

use std::fmt;
use std::path::PathBuf;

use bytes::Bytes;
use serde::Serialize;

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Output for a successful `ac put`.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct AcPutResult {
    /// The action digest key.
    pub digest: String,
    /// Bytes stored.
    pub bytes_stored: u64,
    /// Server-echoed hash (if provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_hash: Option<String>,
}

impl fmt::Display for AcPutResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AC stored {} bytes under digest: {}", self.bytes_stored, self.digest)
    }
}

/// Output for a successful `ac get`.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct AcGetResult {
    /// The action digest key.
    pub digest: String,
    /// Bytes retrieved.
    pub bytes_retrieved: u64,
    /// Destination: `"-"` for stdout, else file path.
    pub destination: String,
}

impl fmt::Display for AcGetResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AC retrieved {} bytes for digest {} → {}",
            self.bytes_retrieved, self.digest, self.destination
        )
    }
}

/// Run `corelink ac put <digest> <result>`.
///
/// `result_path` is the path to a file whose contents are stored under `digest`.
pub async fn run_put(
    client: &CorelinkClient,
    digest: &str,
    result_path: &PathBuf,
    format: OutputFormat,
) -> Result<(), CliError> {
    let data = std::fs::read(result_path)?;
    let bytes_stored = data.len() as u64;
    let resp = client.ac_put(digest, Bytes::from(data)).await?;
    let result = AcPutResult {
        digest: digest.to_owned(),
        bytes_stored,
        server_hash: resp.hash,
    };
    let fmt = Formatter::new(format);
    fmt.emit(&result).map_err(CliError::Json)?;
    Ok(())
}

/// Run `corelink ac get <digest> [-o <file>]`.
pub async fn run_get(
    client: &CorelinkClient,
    digest: &str,
    output_path: Option<PathBuf>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let data = client.ac_get(digest).await?;
    let bytes_retrieved = data.len() as u64;

    let destination = match &output_path {
        Some(p) => {
            std::fs::write(p, &data)?;
            p.display().to_string()
        }
        None => {
            use std::io::Write as _;
            std::io::stdout().write_all(&data)?;
            "-".to_owned()
        }
    };

    // Only emit summary when writing to a file (not stdout which has binary data).
    if output_path.is_some() {
        let result = AcGetResult {
            digest: digest.to_owned(),
            bytes_retrieved,
            destination,
        };
        let fmt = Formatter::new(format);
        fmt.emit(&result).map_err(CliError::Json)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;

    #[test]
    fn ac_put_result_display() {
        let r = AcPutResult {
            digest: "sha256:abcd".to_owned(),
            bytes_stored: 128,
            server_hash: None,
        };
        let s = format!("{r}");
        assert!(s.contains("128 bytes"));
        assert!(s.contains("sha256:abcd"));
    }

    #[test]
    fn ac_get_result_display() {
        let r = AcGetResult {
            digest: "sha256:1234".to_owned(),
            bytes_retrieved: 256,
            destination: "/tmp/out".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("256 bytes"));
        assert!(s.contains("/tmp/out"));
    }

    #[test]
    fn ac_put_result_serialises() {
        let r = AcPutResult {
            digest: "d".to_owned(),
            bytes_stored: 1,
            server_hash: Some("h".to_owned()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"digest\""));
        assert!(json.contains("\"server_hash\""));
    }

    #[test]
    fn ac_get_result_serialises() {
        let r = AcGetResult {
            digest: "d".to_owned(),
            bytes_retrieved: 10,
            destination: "-".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"bytes_retrieved\""));
    }
}
