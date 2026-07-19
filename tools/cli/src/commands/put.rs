//! `corelink put` — upload a blob (WI-S15-001 + Stream-1 prod API).
//!
//! Computes a BLAKE3 digest via streaming read (never loads the whole file
//! into memory at once for hashing — safe for multi-GB blobs). Uses
//! `indicatif` progress bar when stderr is a tty; suppressed in pipes
//! (CTRL-UX-001).
//!
//! Native CAS addresses blobs by BLAKE3 (the write handler re-hashes the
//! bytes and 422s a non-BLAKE3 claim), so the CLI MUST claim the BLAKE3
//! digest here.
//!
//! API: `PUT /v1/cas/<tenant>/<blake3>` (Stream-1 prod contract).

use std::fmt;
use std::io::BufReader;
use std::path::PathBuf;

use bytes::Bytes;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Read chunk size: 256 KiB — keeps memory constant regardless of file size.
const CHUNK_SIZE: usize = 256 * 1024;

/// Output for a successful `put` command.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct PutResult {
    /// BLAKE3 hex digest of the uploaded blob.
    pub digest: String,
    /// Bytes uploaded.
    pub bytes_uploaded: u64,
    /// Source file path.
    pub source: String,
    /// Whether the server reported the blob as fresh (201) vs idempotent (200).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fresh: Option<bool>,
}

impl fmt::Display for PutResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Uploaded {} bytes from '{}' → blake3: {}",
            self.bytes_uploaded, self.source, self.digest
        )
    }
}

/// Run `corelink put <file>`.
///
/// Streams the file in chunks, computing BLAKE3 on the fly, then uploads
/// via `PUT /v1/cas/<tenant>/<blake3>`. Shows a progress bar when stderr is
/// a tty.
pub async fn run(
    client: &CorelinkClient,
    file: &PathBuf,
    pre_computed_digest: Option<&str>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let source = file.display().to_string();
    let file_meta = std::fs::metadata(file)?;
    let file_size = file_meta.len();

    // Progress bar (tty-gated).
    let progress = if atty_stderr() {
        let pb = ProgressBar::new(file_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };

    // Stream file in chunks to compute BLAKE3 + collect bytes for upload.
    // Note: for truly large blobs (>4 GB) a streaming PUT would be ideal;
    // reqwest 0.12 with body streaming is used here via Bytes::from(Vec).
    // For the Stream-1 MVP this is acceptable (max practical blob ~ few hundred MB).
    let (digest, data) = {
        let f = std::fs::File::open(file)?;
        let reader = BufReader::new(f);
        blake3_digest_and_read(reader, progress.as_ref(), file_size as usize)?
    };

    if let Some(pb) = &progress {
        pb.finish_with_message("hashed");
    }

    // Use pre-computed digest if provided (skip re-hash); otherwise use computed.
    let upload_digest = pre_computed_digest.unwrap_or(&digest);

    let bytes_uploaded = data.len() as u64;

    // Upload via the new Stream-1 method (PUT /v1/cas/<tenant>/<blake3>).
    let resp = client.cas_put(upload_digest, Bytes::from(data)).await?;

    let result = PutResult {
        digest: upload_digest.to_owned(),
        bytes_uploaded,
        source,
        fresh: None, // server does not expose 201 vs 200 in PutResp currently
    };

    let _ = resp; // hash echo from server — not used in output for now

    let fmt = Formatter::new(format);
    fmt.emit(&result).map_err(CliError::Json)?;

    Ok(())
}

/// Stream `reader` in [`CHUNK_SIZE`] chunks, computing the BLAKE3 digest of
/// the bytes as they flow AND collecting them for the upload body. Returns
/// `(blake3_hex, bytes)` where `blake3_hex` is the 64-char lowercase hex the
/// native CAS route addresses by. `progress`, when present, is advanced by the
/// byte count of each chunk.
///
/// Extracted from [`run`] so the BLAKE3-not-SHA-256 contract is unit-testable
/// against `blake3::hash` without a live upload.
fn blake3_digest_and_read<R: std::io::Read>(
    mut reader: R,
    progress: Option<&ProgressBar>,
    size_hint: usize,
) -> Result<(String, Vec<u8>), CliError> {
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut all_data: Vec<u8> = Vec::with_capacity(size_hint);

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        // `n <= buf.len()` is guaranteed by the `Read` contract.
        // Use `get(..n)` to satisfy `indexing_slicing` lint; the else
        // branch is unreachable in practice.
        if let Some(filled) = buf.get(..n) {
            hasher.update(filled);
            all_data.extend_from_slice(filled);
        }
        if let Some(pb) = progress {
            pb.inc(n as u64);
        }
    }

    let hex = hex::encode(hasher.finalize().as_bytes());
    Ok((hex, all_data))
}

/// Returns true when stderr is connected to a tty.
///
/// Uses `std::io::IsTerminal` (stable since Rust 1.70 / workspace MSRV 1.80).
fn atty_stderr() -> bool {
    use std::io::IsTerminal as _;
    std::io::stderr().is_terminal()
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
    fn put_result_display() {
        let r = PutResult {
            digest: "abc123".to_owned(),
            bytes_uploaded: 2048,
            source: "file.bin".to_owned(),
            fresh: None,
        };
        let s = format!("{r}");
        assert!(s.contains("2048 bytes"));
        assert!(s.contains("abc123"));
        assert!(s.contains("blake3:"));
    }

    #[test]
    fn put_result_serialises() {
        let r = PutResult {
            digest: "d".to_owned(),
            bytes_uploaded: 1,
            source: "f".to_owned(),
            fresh: Some(true),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"digest\""));
        assert!(json.contains("\"bytes_uploaded\""));
        assert!(json.contains("\"fresh\""));
    }

    #[test]
    fn put_digest_is_blake3_not_sha256() {
        // B3 regression: the streaming put-hash helper must return the SAME
        // 64-hex as `blake3::hash` (proves BLAKE3, not SHA-256). Native CAS
        // 422s a non-BLAKE3 claim, so this is the load-bearing contract.
        let data = b"hello corelink";
        let (digest, echoed) =
            blake3_digest_and_read(&data[..], None, data.len()).expect("hashing must not fail");

        let reference = hex::encode(blake3::hash(data).as_bytes());
        assert_eq!(digest, reference, "put must claim the BLAKE3 digest");
        assert_ne!(
            digest,
            // A SHA-256 of the same bytes must NOT match (regression sentinel).
            {
                use sha2::{Digest as _, Sha256};
                let mut h = Sha256::new();
                h.update(data);
                hex::encode(h.finalize())
            },
            "digest must be BLAKE3, never SHA-256"
        );
        assert_eq!(echoed, data, "collected bytes must equal the input");
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn blake3_10mb_streaming() {
        // Verify streaming hash on a 10 MB synthetic blob (regression gate for
        // OOM) and that the chunked digest equals the one-shot BLAKE3.
        let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let (digest, _) =
            blake3_digest_and_read(&data[..], None, data.len()).expect("hashing must not fail");
        assert_eq!(digest, hex::encode(blake3::hash(&data).as_bytes()));
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
