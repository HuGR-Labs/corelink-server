//! `corelink runbook-drill record` subcommand — WI-S17-003.
//!
//! Builds a validated [`DrillRecord`](corelink_runbook_tracker::DrillRecord)
//! using the pure-logic tracker library and emits it as JSON on stdout.
//!
//! The JSON payload is the canonical wire form for the D1 `runbook_drills`
//! insert API (migration 0033). It can be piped to `wrangler d1 execute` or
//! POSTed to the admin runbook-drill endpoint:
//!
//! ```bash
//! corelink runbook-drill record \
//!     --runbook-id RB-FM-051 \
//!     --executor op_gschneiter \
//!     --duration-seconds 720 \
//!     --expected-seconds 600 \
//!     --evidence https://r2.example/evidence-runbooks/cast-1.cast \
//!   | curl -X POST https://corelink-api.humangr.com/admin/runbook-drill \
//!         -H "Authorization: Bearer $CORELINK_PAT" -d @-
//! ```
//!
//! The subcommand is fully offline: no network, no PAT required. The host
//! adapter (CF Worker / admin API) is responsible for D1 persistence and
//! post-mortem trigger emission on `outcome == drift_flagged`.

use corelink_runbook_tracker::{DrillRecord, RunbookId, TrackerError};
use uuid::Uuid;

use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Build + emit a drill record. Returns `Ok` on validation success; the
/// outcome (`pass` / `fail` / `drift_flagged`) is computed canonical from
/// `--success` and the drift ratio (`duration / expected > 2.0`).
#[allow(clippy::too_many_arguments)]
pub fn run(
    runbook_id: &str,
    executor: &str,
    duration_seconds: i64,
    expected_seconds: i64,
    evidence: &str,
    success: bool,
    executed_at: Option<i64>,
    notes: Option<&str>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let rb = RunbookId::new(runbook_id).map_err(map_tracker_err)?;
    let drill_id = Uuid::new_v4().to_string();
    let now = executed_at.unwrap_or_else(unix_now_secs);
    let record = DrillRecord::new(
        drill_id,
        rb,
        executor,
        now,
        duration_seconds,
        expected_seconds,
        success,
        evidence,
        notes.map(str::to_owned),
    )
    .map_err(map_tracker_err)?;

    Formatter::new(format).emit(&DrillEmit(record))?;
    Ok(())
}

/// Display wrapper for [`DrillRecord`] (text form = single-line summary).
#[derive(Debug, serde::Serialize)]
#[serde(transparent)]
struct DrillEmit(DrillRecord);

impl std::fmt::Display for DrillEmit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "drill_id={} runbook={} executor={} duration={}s expected={}s outcome={} evidence={}",
            self.0.drill_id,
            self.0.runbook_id,
            self.0.executor,
            self.0.duration_seconds,
            self.0.expected_seconds,
            self.0.outcome.as_label(),
            self.0.evidence_url,
        )
    }
}

fn map_tracker_err(e: TrackerError) -> CliError {
    CliError::Other(format!("runbook-drill: {e}"))
}

fn unix_now_secs() -> i64 {
    // System time fallback to 0 on the (impossible) `before UNIX_EPOCH` clock
    // — keeps the CLI deterministic and lints clean.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
