//! `corelink get` — download a blob by SHA-256 digest (WI-S15-001 + Stream-1).
//!
//! Client-verify default-on: SHA-256 of downloaded bytes is compared against
//! the requested digest (CTRL-CAS-002 + INV-CAS-INTEGRITY).
//!
//! API: `GET /v1/cas/<tenant>/<sha256>` (Stream-1 prod contract).

use std::fmt;
use std::path::PathBuf;

use serde::Serialize;
use sha2::{Digest as _, Sha256};

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
///
/// Uses `GET /v1/cas/<tenant>/<sha256>` via the Stream-1 client method.
/// Performs client-side SHA-256 verify against the requested digest.
pub async fn run(
    client: &CorelinkClient,
    digest: &str,
    output_path: Option<PathBuf>,
    format: OutputFormat,
) -> Result<(), CliError> {
    // Strip any `sha256:` prefix the user may have supplied.
    let bare_digest = digest.trim_start_matches("sha256:");

    let data = client.cas_get(bare_digest).await?;

    // Client-verify: SHA-256 the downloaded bytes against the requested digest.
    let computed = {
        let mut hasher = Sha256::new();
        hasher.update(&data);
        hex::encode(hasher.finalize())
    };
    if computed != bare_digest {
        return Err(CliError::Other(format!(
            "Client verify FAILED: expected sha256:{bare_digest}, got sha256:{computed}. \
             Possible data corruption (INV-CAS-INTEGRITY)."
        )));
    }

    let bytes_written = data.len() as u64;
    let destination = match &output_path {
        Some(p) => {
            std::fs::write(p, &data)?;
            p.display().to_string()
        }
        None => {
            // Write raw bytes to stdout.
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
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::format_in_format_args
)]
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

    #[test]
    fn sha256_prefix_strip() {
        let digest = "sha256:abcdef0123456789";
        let bare = digest.trim_start_matches("sha256:");
        assert_eq!(bare, "abcdef0123456789");
    }
}
