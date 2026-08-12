//! End-to-end integration test for the compiled `runner-aggregate-run` binary —
//! exercises the real `read stdin → parse → aggregate_runner_usage → write
//! stdout → exit-code` glue in `src/bin/runner-aggregate-run.rs`.
//!
//! The lib unit suite covers the pure aggregation logic; this suite locks the
//! thin I/O bin layer (stdin read, JSON parse, stdout write, the success/failure
//! exit-code contract) that the unit tests cannot reach. It runs the COMPILED
//! bin via the `CARGO_BIN_EXE_*` path cargo injects into integration tests, so it
//! asserts genuine process behavior.
//!
//! Exit-code contract under test:
//! - `0` (success) — valid input; the `RunnerAggregateOutput` JSON is on stdout.
//! - non-zero (failure) — malformed input JSON or a hash-chain break; nothing
//!   is written to stdout (fail-CLOSED — never a silent empty success).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration tests are allowed these primitives; failures should panic loudly."
)]

use std::io::Write;
use std::process::{Command, Stdio};

/// Absolute path to the compiled bin (cargo injects this for integration tests).
const BIN: &str = env!("CARGO_BIN_EXE_runner-aggregate-run");

/// Run the bin with `stdin`, returning `(exit_code, stdout, stderr)`.
fn run(stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn runner-aggregate-run");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A Starter tenant that ran 240 vCPU-h (100 included) → 140 vCPU-h over at
/// $0.20 = $28.00 = 2_800_000 millicents.
fn starter_240h_input() -> String {
    r#"{
        "billing_period": "2026-08",
        "period_start_ms": 1000,
        "period_end_ms": 2000,
        "now_ms": 1500,
        "staged": [
            {
                "tenant_id": "00000000-0000-0000-0000-000000000001",
                "region": "iad",
                "qty_vcpu_seconds": 864000,
                "idem_key_hex": "0101010101010101010101010101010101010101010101010101010101010101",
                "time_ms": 1
            }
        ],
        "tenant_tiers": { "00000000-0000-0000-0000-000000000001": "runner_starter" },
        "prior_chain_heads": {}
    }"#
    .to_string()
}

#[test]
fn valid_input_exits_zero_and_emits_the_shadow_charge() {
    let (code, stdout, _stderr) = run(&starter_240h_input());
    assert_eq!(code, 0, "valid input must exit 0; stdout={stdout}");

    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert_eq!(v["counters"].as_array().unwrap().len(), 1);
    let line = &v["shadow_ledger"][0];
    assert_eq!(
        line["shadow_charge_millicents"].as_u64().unwrap(),
        2_800_000
    );
    assert_eq!(line["shadow_charge_cents"].as_u64().unwrap(), 2800);
    assert_eq!(line["overage_vcpu_hours"].as_str().unwrap(), "140.000000");
    assert_eq!(v["total_shadow_millicents"].as_u64().unwrap(), 2_800_000);
}

#[test]
fn malformed_input_json_exits_non_zero_and_writes_no_stdout() {
    // This is the load-bearing assertion that kills the `main -> Default::default`
    // mutant: a broken input MUST NOT return a success exit code.
    let (code, stdout, stderr) = run("{ this is not valid json");
    assert_ne!(
        code, 0,
        "malformed input must fail-CLOSED with a non-zero exit"
    );
    assert!(
        stdout.trim().is_empty(),
        "no output document on a failed parse; stdout={stdout}"
    );
    assert!(
        stderr.contains("invalid RunnerAggregateInput JSON"),
        "the error names the cause; stderr={stderr}"
    );
}

#[test]
fn empty_staged_still_succeeds_with_empty_ledger() {
    let input = r#"{
        "billing_period": "2026-08",
        "period_start_ms": 1000,
        "period_end_ms": 2000,
        "now_ms": 1500,
        "staged": []
    }"#;
    let (code, stdout, _stderr) = run(input);
    assert_eq!(code, 0, "empty-but-valid input is a clean success");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert_eq!(v["counters"].as_array().unwrap().len(), 0);
    assert_eq!(v["total_shadow_millicents"].as_u64().unwrap(), 0);
}
