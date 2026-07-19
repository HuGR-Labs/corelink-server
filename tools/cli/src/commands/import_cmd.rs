//! `corelink import <source>` — bulk pre-warm the tenant CAS from an
//! existing store (sales-FAQ M2: "pre-warm via the bulk-upload CLI —
//! content-addressed, so dedup happens for free").
//!
//! **Wired today:** a local directory source — every file is uploaded
//! into CoreLink CAS keyed by its BLAKE3 (dedup is automatic; re-import
//! is idempotent because identical content maps to the same digest).
//!
//! **Flagged gap:** `s3://…` (and other object-store URIs) — direct
//! ingestion from S3/GCS needs an object-store client + credentials that
//! are out of scope for this crate. The command fails with a clear
//! pointer rather than pretending.

use std::path::PathBuf;

use crate::client::CorelinkClient;
use crate::commands::cas::{self, Location};
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Resolve an import source to a local directory, or a structured error
/// naming the flagged gap. Pure (no I/O).
pub fn plan_import(source: &str) -> Result<PathBuf, CliError> {
    match cas::classify_location(source) {
        Location::LocalDir(p) => Ok(p),
        Location::S3(_) => Err(CliError::Other(format!(
            "import: {source:?} is an S3 URI — direct S3 ingestion needs an object-store client + \
             credentials (flagged gap). Sync the bucket locally and `corelink import <dir>`, or \
             pipe individual objects through `corelink put`."
        ))),
        Location::OtherUri(_) => Err(CliError::Other(format!(
            "import: {source:?} is a remote URI — only a local directory source is wired today."
        ))),
    }
}

/// Run `corelink import <source>`.
pub async fn run(
    client: &CorelinkClient,
    source: &str,
    format: OutputFormat,
) -> Result<(), CliError> {
    let dir = plan_import(source)?;
    let summary = cas::upload_dir(client, &dir).await?;
    Formatter::new(format)
        .emit(&summary)
        .map_err(CliError::Json)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn plan_import_accepts_local_dir() {
        assert_eq!(plan_import("./cache").unwrap(), PathBuf::from("./cache"));
    }

    #[test]
    fn plan_import_flags_s3() {
        let err = plan_import("s3://bucket/prefix").unwrap_err();
        assert!(format!("{err}").contains("S3"));
    }

    #[test]
    fn plan_import_flags_other_uri() {
        assert!(plan_import("gs://bucket/x").is_err());
    }
}
