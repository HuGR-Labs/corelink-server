//! `corelink ci mirror --from <src> --to <sink>` — the sales-FAQ M5
//! "mirror, not cut-over" migration helper (write to your existing cache
//! AND CoreLink in parallel before flipping primary).
//!
//! **Wired today:** a one-shot mirror of a **local cache directory**
//! (`--from <dir> --to corelink`) — every blob under the directory is
//! copied into CoreLink CAS (content-addressed, so re-runs are
//! idempotent and only new blobs cost bandwidth). This is the honest
//! subset of the mirror story: point it at your `bazel-remote` /
//! `buildbuddy` on-disk cache dir and it fills CoreLink.
//!
//! **Flagged gap:** the *live sidecar* form (`--from bazel-remote` as a
//! running service, or `--bes_backend` BES ingestion) needs a
//! long-running daemon that speaks the source cache's wire protocol —
//! that bridge is not implemented here and the command says so rather
//! than pretending.

use std::path::PathBuf;

use crate::client::CorelinkClient;
use crate::commands::cas::{self, Location};
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// The only wired mirror sink.
const SINK_CORELINK: &str = "corelink";

/// Source-cache service keywords that imply the (unwired) live-sidecar
/// bridge rather than a local directory.
const LIVE_SERVICE_KEYWORDS: [&str; 4] = ["bazel-remote", "buildbuddy", "nativelink", "bes"];

/// Validate `--to` and resolve `--from` to a local mirror directory, or a
/// structured error naming the flagged gap. Pure (no I/O).
pub fn plan_mirror(from: &str, to: &str) -> Result<PathBuf, CliError> {
    if !to.eq_ignore_ascii_case(SINK_CORELINK) {
        return Err(CliError::Other(format!(
            "ci mirror: --to {to:?} is not supported; the only wired sink is `corelink`."
        )));
    }
    if LIVE_SERVICE_KEYWORDS
        .iter()
        .any(|k| from.eq_ignore_ascii_case(k))
    {
        return Err(CliError::Other(format!(
            "ci mirror: --from {from:?} names a live cache service — the streaming sidecar bridge \
             (source-protocol client + BES ingestion) is not implemented here (flagged gap). \
             Point --from at the source cache's on-disk directory for a one-shot mirror instead."
        )));
    }
    match cas::classify_location(from) {
        Location::LocalDir(p) => Ok(p),
        Location::S3(_) | Location::OtherUri(_) => Err(CliError::Other(format!(
            "ci mirror: --from {from:?} is a remote URI — only a local cache directory is wired \
             today (flagged gap)."
        ))),
    }
}

/// Run `corelink ci mirror --from <src> --to <sink>`.
pub async fn run(
    client: &CorelinkClient,
    from: &str,
    to: &str,
    format: OutputFormat,
) -> Result<(), CliError> {
    let dir = plan_mirror(from, to)?;
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
    fn plan_mirror_accepts_local_dir_to_corelink() {
        assert_eq!(
            plan_mirror("/var/cache/bazel", "corelink").unwrap(),
            PathBuf::from("/var/cache/bazel")
        );
        // sink is case-insensitive.
        assert!(plan_mirror("/tmp/c", "CoreLink").is_ok());
    }

    #[test]
    fn plan_mirror_rejects_unknown_sink() {
        let err = plan_mirror("/tmp/c", "buildbuddy").unwrap_err();
        assert!(format!("{err}").contains("only wired sink"));
    }

    #[test]
    fn plan_mirror_flags_live_service_source() {
        let err = plan_mirror("bazel-remote", "corelink").unwrap_err();
        assert!(format!("{err}").contains("sidecar"));
    }

    #[test]
    fn plan_mirror_flags_remote_source_uri() {
        assert!(plan_mirror("s3://bucket", "corelink").is_err());
    }
}
