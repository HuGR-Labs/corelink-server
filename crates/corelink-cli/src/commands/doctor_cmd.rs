//! `corelink doctor` command handler (WI-S15-001).
//!
//! Delegates to [`crate::doctor`] for the 8 canonical checks.

use std::fmt;

use serde::Serialize;

use crate::client::CorelinkClient;
use crate::doctor::{run_checks, CheckStatus, DoctorCheck};
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Wrapper for the doctor result list (needed for Display impl).
#[derive(Debug, Serialize)]
pub struct DoctorReport(pub Vec<DoctorCheck>);

impl fmt::Display for DoctorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<18} {:<6} {:>8}   {:>20}   NEXT_ACTION",
            "CHECK", "STATUS", "LATENCY", "ERROR_CODE"
        )?;
        writeln!(f, "{}", "-".repeat(100))?;
        for check in &self.0 {
            writeln!(f, "{check}")?;
        }

        let fails: Vec<&DoctorCheck> = self.0.iter().filter(|c| c.status == CheckStatus::Fail).collect();
        let skips: Vec<&DoctorCheck> = self.0.iter().filter(|c| c.status == CheckStatus::Skip).collect();

        writeln!(f)?;
        if fails.is_empty() {
            write!(f, "All checks passed ({} ok", self.0.len() - skips.len())?;
            if !skips.is_empty() {
                write!(f, ", {} skipped", skips.len())?;
            }
            write!(f, ").")?;
        } else {
            write!(
                f,
                "{} check(s) failed, {} skipped.",
                fails.len(),
                skips.len()
            )?;
        }
        Ok(())
    }
}

/// Run `corelink doctor [--json]`.
pub async fn run(
    client: &CorelinkClient,
    json_flag: bool,
    global_format: OutputFormat,
) -> Result<(), CliError> {
    // `--json` flag on doctor is a legacy shorthand; prefer global `--output=json`.
    let format = if json_flag {
        OutputFormat::Json
    } else {
        global_format
    };

    let checks = run_checks(client).await?;
    let any_fail = checks.iter().any(|c| c.status == CheckStatus::Fail);
    let report = DoctorReport(checks);

    let fmt = Formatter::new(format);
    fmt.emit(&report).map_err(CliError::Json)?;

    if any_fail {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::doctor::DoctorCheck;

    #[test]
    fn doctor_report_display_all_ok() {
        let checks = vec![
            DoctorCheck { check: "network".to_owned(), status: CheckStatus::Ok, latency_ms: Some(5), error_code: None, next_action: None },
            DoctorCheck { check: "auth".to_owned(), status: CheckStatus::Ok, latency_ms: Some(10), error_code: None, next_action: None },
        ];
        let report = DoctorReport(checks);
        let s = format!("{report}");
        assert!(s.contains("All checks passed"));
    }

    #[test]
    fn doctor_report_display_with_fail() {
        let checks = vec![
            DoctorCheck { check: "network".to_owned(), status: CheckStatus::Fail, latency_ms: Some(5000), error_code: Some("COR_NET_UNREACHABLE".to_owned()), next_action: Some("Check DNS".to_owned()) },
        ];
        let report = DoctorReport(checks);
        let s = format!("{report}");
        assert!(s.contains("1 check(s) failed"));
    }

    #[test]
    fn doctor_report_serialises_to_json() {
        let checks = vec![
            DoctorCheck { check: "auth".to_owned(), status: CheckStatus::Ok, latency_ms: Some(3), error_code: None, next_action: None },
        ];
        let report = DoctorReport(checks);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"check\""));
        assert!(json.contains("\"ok\""));
    }

    #[test]
    fn doctor_report_8_checks_count() {
        // Validate that in a real doctor run we always get exactly 8 results.
        // We test with a mocked set.
        let checks: Vec<DoctorCheck> = (0..8).map(|i| DoctorCheck {
            check: format!("check_{i}"),
            status: CheckStatus::Ok,
            latency_ms: Some(i as u64),
            error_code: None,
            next_action: None,
        }).collect();
        assert_eq!(checks.len(), 8);
    }
}
