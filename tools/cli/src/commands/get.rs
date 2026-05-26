//! `corelink get` — download a blob by digest (WI-S15-001).
//!
//! Client-verify default-on: BLAKE3 hash of downloaded bytes is compared
//! against the requested digest (CTRL-CAS-002 + INV-CAS-INTEGRITY).

use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Output for a successful `get` command.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct GetResult {
    /// Digest that was downloaded.
    pub digest: String,
    /// Bytes written.
    pub bytes_written: u64,
    /// Destination: `"-"` for stdout, else file path.
    pub destination: String,
    /// Client-verify result: always `"ok"` (fail exits with error).
    pub client_verify: String,
}

impl fmt::Display for GetResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Downloaded {} bytes to '{}' (verify: {})",
            self.bytes_written, self.destination, self.client_verify
        )
    }
}

/// Run `corelink get <digest> [-o <file>]`.
pub async fn run(
    client: &CorelinkClient,
    digest: &str,
    output_path: Option<PathBuf>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let path = format!("/v1/cas/download/{digest}");
    let data = client.get_bytes(&path).await?;

    // Client-verify: BLAKE3 the downloaded bytes against the requested digest.
    let computed = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&data);
        hasher.finalize().to_hex().to_string()
    };
    // The digest may be bare BLAKE3 hex or prefixed with "blake3:".
    let expected = digest.trim_start_matches("blake3:");
    if computed != expected {
        return Err(CliError::Other(format!(
            "Client verify FAILED: expected digest {expected}, got {computed}. Possible data corruption (INV-CAS-INTEGRITY)."
        )));
    }

    let bytes_written = data.len() as u64;
    let destination = match &output_path {
        Some(p) => {
            std::fs::write(p, &data)?;
            p.display().to_string()
        }
        None => {
            // Write raw bytes to stdout via eprintln avoidance.
            use std::io::Write as _;
            std::io::stdout().write_all(&data)?;
            "-".to_owned()
        }
    };

    let result = GetResult {
        digest: digest.to_owned(),
        bytes_written,
        destination,
        client_verify: "ok".to_owned(),
    };

    if output_path.is_some() {
        // Only emit the summary when writing to a file, not stdout (binary data).
        let fmt = Formatter::new(format);
        fmt.emit(&result).map_err(CliError::Json)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn get_result_display() {
        let r = GetResult {
            digest: "abc".to_owned(),
            bytes_written: 512,
            destination: "/tmp/out".to_owned(),
            client_verify: "ok".to_owned(),
        };
        let s = format!("{r}");
        assert!(s.contains("512 bytes"));
        assert!(s.contains("verify: ok"));
    }

    #[test]
    fn get_result_serialises() {
        let r = GetResult {
            digest: "abc".to_owned(),
            bytes_written: 100,
            destination: "-".to_owned(),
            client_verify: "ok".to_owned(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"client_verify\""));
    }
}
