//! End-to-end integration test for the compiled `billing-reconcile-run`
//! binary — exercises the real `read_input → parse → run_reconcile_pass →
//! write_report → exit-code` glue that lives in
//! `src/bin/billing-reconcile-run.rs`.
//!
//! The unit/property suites cover the pure reconcile logic in the library;
//! this suite locks the thin I/O bin layer (CLI parse, file/stdin read,
//! report write, the 0/1/2 exit-code contract) that those cannot reach. It
//! runs the COMPILED bin via the `CARGO_BIN_EXE_*` path cargo injects into
//! integration tests, so it asserts genuine process behavior rather than
//! re-calling internal functions.
//!
//! Exit-code contract under test (from the bin's module doc):
//! - `0` — clean pass, prints `BILLING_RECONCILE_CLEAN`.
//! - `1` — drift escalated to / past the `--fail-on` floor, prints the
//!   per-tenant `BILLING_RECONCILE_DRIFT_DETECTED::<tenant>::<sev>` marker.
//! - `2` — the pass errored (malformed input), prints
//!   `BILLING_RECONCILE_ERROR` (fail-CLOSED — never a silent clean).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed these primitives; failures should panic loudly."
)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

/// Absolute path to the compiled bin (cargo provides this to integration
/// tests). Hyphens in the bin name are preserved verbatim in the env key.
const BIN: &str = env!("CARGO_BIN_EXE_billing-reconcile-run");

/// A canonical, stable tenant id used in the input documents + the
/// expected per-tenant drift marker.
const TENANT: &str = "018f9b1e-0000-7000-8000-000000000001";

/// A clean three-layer-totals input: all three layers agree exactly, so
/// the canonical reconcile pass escalates nothing (max_severity = clean).
fn clean_input_json() -> String {
    format!(
        r#"{{
            "billing_period": "2026-05",
            "snapshots": [
                {{
                    "tenant_id": "{TENANT}",
                    "layer1": {{ "total_qty": 1000, "record_count": 1 }},
                    "layer2": {{ "total_qty": 1000, "record_count": 1 }},
                    "layer3": {{ "total_qty": 1000, "record_count": 1 }}
                }}
            ]
        }}"#
    )
}

/// A drift-past-floor input: Layer 1 + Layer 2 agree at 100, Layer 3
/// (Stripe billed) diverges to 95 = 5% drift → SEV-1 (usage != billed),
/// which is well past the default `sev3` fail-on floor.
fn drift_input_json() -> String {
    format!(
        r#"{{
            "billing_period": "2026-05",
            "snapshots": [
                {{
                    "tenant_id": "{TENANT}",
                    "layer1": {{ "total_qty": 100, "record_count": 1 }},
                    "layer2": {{ "total_qty": 100, "record_count": 1 }},
                    "layer3": {{ "total_qty": 95, "record_count": 1 }}
                }}
            ]
        }}"#
    )
}

/// Per-test unique scratch dir under cargo's integration-test tmp dir
/// (`CARGO_TARGET_TMPDIR`) — no external `tempfile` dep, auto-cleaned by
/// `cargo clean`, isolated per test by a process-local counter.
fn unique_dir() -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("brr-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// CLEAN input file → exit 0 + `BILLING_RECONCILE_CLEAN` on stdout + a
/// report file written with the expected clean contents.
///
/// Kills:
/// - `read_input_bytes` returning a fixed `Ok(vec![0])` / `Ok(vec![1])` /
///   `Ok(vec![])`: those wrong bytes fail to parse → the bin exits 2, not
///   0, so this assertion fails on the mutant.
/// - the `p != "-"` match-guard flipped to `==`: with `==` a real
///   `--input <file>` path falls through to the stdin arm, reading the
///   (null) stdin → parse error → exit 2, not 0.
/// - `write_report` short-circuited to `Ok(())`: the report file is then
///   never written / lacks the clean body, so the file-content assert fails.
#[test]
fn clean_input_exits_zero_and_writes_clean_report() {
    let dir = unique_dir();
    let input = dir.join("input.json");
    let report = dir.join("report.json");
    std::fs::write(&input, clean_input_json()).expect("write input");

    let out = Command::new(BIN)
        .arg("--input")
        .arg(&input)
        .arg("--report")
        .arg(&report)
        // Null stdin: if the file-read guard mis-routes to stdin, the read
        // yields empty → parse failure (rather than hanging on a tty).
        .stdin(Stdio::null())
        .output()
        .expect("spawn billing-reconcile-run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "clean input must exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("BILLING_RECONCILE_CLEAN"),
        "clean pass must print the clean marker; stdout={stdout}"
    );

    // The report artifact must exist AND carry the clean body — a
    // `write_report → Ok(())` mutant leaves this missing/empty.
    let body = std::fs::read_to_string(&report).expect("report file must be written");
    assert!(
        body.contains("\"max_severity\": \"clean\""),
        "report must record max_severity=clean; body={body}"
    );
    assert!(
        body.contains("\"escalation_count\": 0"),
        "clean report must record zero escalations; body={body}"
    );
    assert!(
        body.contains("\"billing_period\": \"2026-05\""),
        "report must echo the billing period; body={body}"
    );
}

/// DRIFT-past-floor input → exit 1 + the per-tenant
/// `BILLING_RECONCILE_DRIFT_DETECTED::<tenant>::sev1` marker on stdout + a
/// report file recording the escalation.
///
/// Kills:
/// - `run` short-circuited to `Ok(<default ExitCode>)` (= SUCCESS): the
///   bin would exit 0, not 1.
/// - `main` short-circuited to its default `ExitCode`: same — exit 0, not 1.
/// - the per-tenant marker guard `o.severity >= Sev3` flipped to `<`: with
///   `<` the SEV-1 tenant no longer prints its drift marker (clean tenants
///   would instead), so the stdout marker assert fails.
#[test]
fn drift_past_floor_exits_one_and_marks_tenant() {
    let dir = unique_dir();
    let input = dir.join("input.json");
    let report = dir.join("report.json");
    std::fs::write(&input, drift_input_json()).expect("write input");

    let out = Command::new(BIN)
        .arg("--input")
        .arg(&input)
        .arg("--report")
        .arg(&report)
        .stdin(Stdio::null())
        .output()
        .expect("spawn billing-reconcile-run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "drift past floor must exit 1; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains(&format!("BILLING_RECONCILE_DRIFT_DETECTED::{TENANT}::sev1")),
        "the SEV-1 tenant must print its per-tenant drift marker; stdout={stdout}"
    );

    let body = std::fs::read_to_string(&report).expect("report file must be written");
    assert!(
        body.contains("\"max_severity\": \"sev1\""),
        "report must record max_severity=sev1; body={body}"
    );
    assert!(
        body.contains("\"escalation_count\": 1"),
        "drift report must record one escalation; body={body}"
    );
}

/// MALFORMED input → exit 2 + `BILLING_RECONCILE_ERROR` on stderr (fail
/// CLOSED — an unparseable input is NEVER reported as a clean pass).
///
/// Kills the `read_input_bytes → Ok(vec![..])` family from the other
/// direction (a fixed-byte read would change the parse-failure path) and
/// pins the distinct error exit code (2, not 0 or 1).
#[test]
fn malformed_input_exits_two_fail_closed() {
    let dir = unique_dir();
    let input = dir.join("input.json");
    std::fs::write(&input, b"{ not valid json").expect("write input");

    let out = Command::new(BIN)
        .arg("--input")
        .arg(&input)
        .stdin(Stdio::null())
        .output()
        .expect("spawn billing-reconcile-run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "malformed input must fail-closed at exit 2; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stderr.contains("BILLING_RECONCILE_ERROR"),
        "malformed input must print the error marker; stderr={stderr}"
    );
    assert!(
        !stdout.contains("BILLING_RECONCILE_CLEAN"),
        "a parse error must NEVER report a clean pass; stdout={stdout}"
    );
}

/// EMPTY input read from stdin (`--input -`) → exit 2: an empty byte
/// stream is not a valid `ReconcileRunInput`, so the bin fails closed.
/// This also exercises the stdin read path + the `p != "-"` guard in its
/// true (stdin) branch.
#[test]
fn empty_stdin_input_exits_two() {
    let out = Command::new(BIN)
        .arg("--input")
        .arg("-")
        .stdin(Stdio::null())
        .output()
        .expect("spawn billing-reconcile-run");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "empty stdin input must fail-closed at exit 2; stderr={stderr}"
    );
    assert!(
        stderr.contains("BILLING_RECONCILE_ERROR"),
        "empty input must print the error marker; stderr={stderr}"
    );
}

/// CLEAN input piped via STDIN with `--input -` → exit 0 + the clean
/// marker. The bin must route `-` to the stdin read branch.
///
/// Kills the `read_input_bytes` match guard (`p != "-"` at :99) forced to
/// `true`: with the guard always-true, `--input -` would take the FILE
/// branch and `std::fs::read("-")` fails (no such file) → exit 2, not 0.
#[test]
fn clean_input_piped_via_stdin_dash_exits_zero() {
    let mut child = Command::new(BIN)
        .arg("--input")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn billing-reconcile-run");
    {
        let mut stdin = child.stdin.take().expect("child stdin handle");
        stdin
            .write_all(clean_input_json().as_bytes())
            .expect("write child stdin");
    } // drop closes stdin so the child's read_to_end completes (no hang)
    let out = child.wait_with_output().expect("wait for child");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "clean stdin input via `--input -` must exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("BILLING_RECONCILE_CLEAN"),
        "clean stdin pass must print the clean marker; stdout={stdout}"
    );
}

/// `--help` → exit 0 + the usage line on stdout (NOT the error marker).
///
/// Kills the `main` match guard (`msg == "help"` at :170) forced to
/// `false`: with the guard always-false the help string falls through to
/// the fail-CLOSED error arm → `BILLING_RECONCILE_ERROR` on stderr +
/// exit 2, not 0.
#[test]
fn help_flag_exits_zero_with_usage() {
    let out = Command::new(BIN)
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("spawn billing-reconcile-run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "--help must exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("usage: billing-reconcile-run"),
        "--help must print the usage line; stdout={stdout}"
    );
    assert!(
        !stderr.contains("BILLING_RECONCILE_ERROR"),
        "--help must NOT be treated as a run error; stderr={stderr}"
    );
}
