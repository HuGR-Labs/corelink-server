//! `corelink put` — upload a blob (WI-S15-001).
//!
//! Computes BLAKE3 digest if `--digest` not provided. Outputs computed digest.

use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Output for a successful `put` command.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct PutResult {
    /// BLAKE3 digest of the uploaded blob.
    pub digest: String,
    /// Bytes uploaded.
    pub bytes_uploaded: u64,
    /// Source file path.
    pub source: String,
}

impl fmt::Display for PutResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Uploaded {} bytes from '{}' → digest: {}",
            self.bytes_uploaded, self.source, self.digest
        )
    }
}

/// Run `corelink put <file> [--digest <d>]`.
pub async fn run(
    client: &CorelinkClient,
    file: &PathBuf,
    pre_computed_digest: Option<&str>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let data = std::fs::read(file)?;
    let bytes_uploaded = data.len() as u64;

    let digest = if let Some(d) = pre_computed_digest {
        d.to_owned()
    } else {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&data);
        hasher.finalize().to_hex().to_string()
    };

    let source = file.display().to_string();
    client
        .post_bytes("/v1/cas/upload", bytes::Bytes::from(data))
        .await?;

    let result = PutResult {
        digest,
        bytes_uploaded,
        source,
    };

    let fmt = Formatter::new(format);
    fmt.emit(&result).map_err(CliError::Json)?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn put_result_display() {
        let r = PutResult {
            digest: "abc123".to_owned(),
            bytes_uploaded: 2048,
            source: "file.bin".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("2048 bytes"));
        assert!(s.contains("abc123"));
    }

    #[test]
    fn put_result_serialises() {
        let r = PutResult {
            digest: "d".to_owned(),
            bytes_uploaded: 1,
            source: "f".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"digest\""));
        assert!(json.contains("\"bytes_uploaded\""));
    }
}
