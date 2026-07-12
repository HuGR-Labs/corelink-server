//! `corelink cas get` / `corelink cas export` — the sales-FAQ escape-hatch
//! commands (`marketing/sales/FAQ-MASTER.md` M4 / P34: "the content-
//! addressed nature of CAS is itself the escape hatch").
//!
//! - `cas get <digest> [-o <file>]` — download one blob by digest, with
//!   the same client-side integrity verify as `corelink get`.
//! - `cas export --tenant <id|me> --output <dest>` — bulk-download every
//!   blob in the tenant CAS to a **local directory** (`<dir>/<digest>`)
//!   plus an index. An `s3://` destination is a flagged gap (needs an S3
//!   client + credentials — not wired here).
//!
//! Also hosts [`upload_dir`], the shared local-directory → CAS uploader
//! reused by `corelink import` and `corelink ci mirror`.

use std::fmt;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::client::CorelinkClient;
use crate::commands::ls::LsResponse;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Strip a `sha256:` / `blake3:` scheme prefix, returning the bare hex
/// digest the CAS routes address by.
#[must_use]
pub fn normalize_digest(digest: &str) -> &str {
    digest
        .strip_prefix("sha256:")
        .or_else(|| digest.strip_prefix("blake3:"))
        .unwrap_or(digest)
}

/// A resolved `cas export` / `import` destination or source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Location {
    /// A local filesystem directory.
    LocalDir(PathBuf),
    /// An `s3://bucket/prefix` URI (not wired — flagged).
    S3(String),
    /// Some other URI scheme (`gs://`, `http://`, …) — not wired.
    OtherUri(String),
}

/// Classify a destination/source string into a [`Location`].
#[must_use]
pub fn classify_location(raw: &str) -> Location {
    if let Some(rest) = raw.strip_prefix("s3://") {
        return Location::S3(rest.to_owned());
    }
    if let Some((scheme, _)) = raw.split_once("://") {
        // Any other explicit URI scheme is a not-wired remote target.
        let _ = scheme;
        return Location::OtherUri(raw.to_owned());
    }
    Location::LocalDir(PathBuf::from(raw))
}

/// Run `corelink cas get <digest> [-o <file>]`. Delegates to the shared
/// [`crate::commands::get`] path after normalising the digest scheme.
pub async fn run_get(
    client: &CorelinkClient,
    digest: &str,
    output: Option<PathBuf>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let bare = normalize_digest(digest).to_owned();
    crate::commands::get::run(client, &bare, output, format).await
}

/// Summary of a bulk CAS export.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct ExportSummary {
    /// Tenant exported.
    pub tenant_id: String,
    /// Output directory.
    pub output_dir: String,
    /// Number of blobs downloaded.
    pub blobs_written: u64,
    /// Total bytes downloaded.
    pub bytes_written: u64,
}

impl fmt::Display for ExportSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cas export: {} blob(s), {} bytes → {} (tenant {})",
            self.blobs_written, self.bytes_written, self.output_dir, self.tenant_id
        )
    }
}

/// Run `corelink cas export --tenant <id|me> --output <dest>`.
pub async fn run_export(
    client: &CorelinkClient,
    tenant_arg: &str,
    dest: &str,
    format: OutputFormat,
) -> Result<(), CliError> {
    // Resolve the effective tenant. `me` (or an empty arg) means the
    // caller's own tenant, which is the only tenant CAS reads are keyed
    // to (per-tenant HMAC) — so exporting another tenant is impossible.
    let self_tenant = client.tenant_id().ok_or_else(|| {
        CliError::Other(
            "cas export: tenant_id not set — run `corelink login`/`corelink whoami` first."
                .to_owned(),
        )
    })?;
    let tenant = if tenant_arg.is_empty() || tenant_arg == "me" {
        self_tenant.to_owned()
    } else if tenant_arg == self_tenant {
        tenant_arg.to_owned()
    } else {
        return Err(CliError::Other(format!(
            "cas export: can only export your own tenant ({self_tenant}); requested {tenant_arg:?}. \
             CAS is per-tenant HMAC-keyed — cross-tenant export is not possible."
        )));
    };

    let out_dir = match classify_location(dest) {
        Location::LocalDir(p) => p,
        Location::S3(_) | Location::OtherUri(_) => {
            return Err(CliError::Other(format!(
                "cas export: destination {dest:?} is a remote URI — only a local directory is wired \
                 today. Direct-to-S3/GCS streaming needs an object-store client + credentials \
                 (flagged gap)."
            )));
        }
    };
    std::fs::create_dir_all(&out_dir).map_err(CliError::Io)?;

    // Page through the CAS listing, downloading each blob.
    let mut cursor: Option<String> = None;
    let mut blobs_written = 0u64;
    let mut bytes_written = 0u64;
    loop {
        let mut path = format!("/v1/cas/list?tenant={tenant}&limit=500");
        if let Some(c) = &cursor {
            path.push_str(&format!("&cursor={c}"));
        }
        let raw = client.get_json(&path).await?;
        let page: LsResponse = serde_json::from_value(raw).map_err(CliError::Json)?;
        for entry in &page.entries {
            let bare = normalize_digest(&entry.digest);
            let data = client.cas_get(bare).await?;
            let blob_path = out_dir.join(bare);
            std::fs::write(&blob_path, &data).map_err(CliError::Io)?;
            blobs_written += 1;
            bytes_written += data.len() as u64;
        }
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    let summary = ExportSummary {
        tenant_id: tenant,
        output_dir: out_dir.display().to_string(),
        blobs_written,
        bytes_written,
    };
    Formatter::new(format)
        .emit(&summary)
        .map_err(CliError::Json)?;
    Ok(())
}

/// Summary of a local-directory → CAS upload.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct UploadSummary {
    /// Number of files uploaded.
    pub files_uploaded: u64,
    /// Total bytes uploaded.
    pub bytes_uploaded: u64,
}

impl fmt::Display for UploadSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "uploaded {} file(s), {} bytes to CoreLink CAS",
            self.files_uploaded, self.bytes_uploaded
        )
    }
}

/// Recursively collect regular files under `dir` (deterministic order).
pub fn collect_files(dir: &Path) -> Result<Vec<PathBuf>, CliError> {
    let mut out = Vec::new();
    collect_files_into(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_files_into(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), CliError> {
    for entry in std::fs::read_dir(dir).map_err(CliError::Io)? {
        let entry = entry.map_err(CliError::Io)?;
        let path = entry.path();
        let ft = entry.file_type().map_err(CliError::Io)?;
        if ft.is_dir() {
            collect_files_into(&path, out)?;
        } else if ft.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Upload every regular file under `dir` into the tenant CAS, keyed by
/// the SHA-256 of its contents. Shared by `import` and `ci mirror`.
pub async fn upload_dir(client: &CorelinkClient, dir: &Path) -> Result<UploadSummary, CliError> {
    if !dir.is_dir() {
        return Err(CliError::Other(format!(
            "upload: {} is not a directory",
            dir.display()
        )));
    }
    let files = collect_files(dir)?;
    let mut files_uploaded = 0u64;
    let mut bytes_uploaded = 0u64;
    for f in &files {
        let data = std::fs::read(f).map_err(CliError::Io)?;
        let sha = {
            let mut h = Sha256::new();
            h.update(&data);
            hex::encode(h.finalize())
        };
        let len = data.len() as u64;
        client.cas_put(&sha, Bytes::from(data)).await?;
        files_uploaded += 1;
        bytes_uploaded += len;
    }
    Ok(UploadSummary {
        files_uploaded,
        bytes_uploaded,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn normalize_digest_strips_schemes() {
        assert_eq!(normalize_digest("sha256:abc"), "abc");
        assert_eq!(normalize_digest("blake3:def"), "def");
        assert_eq!(normalize_digest("bareabc"), "bareabc");
    }

    #[test]
    fn classify_location_distinguishes_local_and_remote() {
        assert_eq!(
            classify_location("./out"),
            Location::LocalDir(PathBuf::from("./out"))
        );
        assert_eq!(
            classify_location("s3://bucket/prefix"),
            Location::S3("bucket/prefix".to_owned())
        );
        assert!(matches!(
            classify_location("gs://bucket"),
            Location::OtherUri(_)
        ));
    }

    #[test]
    fn collect_files_walks_recursively() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), b"a").unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("b.bin"), b"bb").unwrap();
        let files = collect_files(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
    }
}
