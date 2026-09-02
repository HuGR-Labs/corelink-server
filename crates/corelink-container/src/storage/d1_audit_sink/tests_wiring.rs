//! The property: the DEPLOYED builder seam never degrades quietly.
//!
//! `build_r2_cas_handler_from_env` / `build_r2_ac_handler_from_env` construct
//! their audit sink through these functions, and the outcome is binary — a
//! DURABLE `D1AuditOutboxSink`, or a loud `Err` that makes the route mount the
//! fail-CLOSED 503 handler. What must never happen is the third outcome: a
//! silent fallback to the volatile `InMemoryAuditSink`, which would lose the
//! pre-mutation audit row on restart AND make the handler's `AuditFailed`
//! guard dead code in production. These tests pin both halves of that binary.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::stub_env;
use super::*;

#[test]
fn deployed_cas_builder_yields_durable_non_inmemory_sink() {
    // This is EXACTLY the construction `build_r2_cas_handler_from_env`
    // performs (`cas_audit_sink_from_d1(D1HttpClient::new(&env))`), so it
    // proves the deployed builder wires a DURABLE sink when D1 env present.
    let sink = cas_audit_sink_from_d1(D1HttpClient::new(&stub_env())).expect("durable sink builds");
    let dbg = format!("{sink:?}");
    assert!(
        dbg.contains("D1AuditOutboxSink"),
        "must be the durable D1 sink, got: {dbg}"
    );
    assert!(
        !dbg.contains("InMemory"),
        "must NOT be the volatile in-memory sink, got: {dbg}"
    );
}

#[test]
fn deployed_ac_builder_yields_durable_non_inmemory_sink() {
    let sink = ac_audit_sink_from_d1(D1HttpClient::new(&stub_env())).expect("durable sink builds");
    let dbg = format!("{sink:?}");
    assert!(dbg.contains("D1AuditOutboxSink"), "got: {dbg}");
    assert!(!dbg.contains("InMemory"), "got: {dbg}");
}

#[test]
fn cas_sink_fails_closed_when_d1_client_unavailable() {
    // Storage creds present but the durable sink cannot be constructed →
    // REFUSE (Err), never a silent in-memory fallback. The builder maps
    // this to Some(Err(..)) → the route mounts the fail-CLOSED 503 handler.
    let err = cas_audit_sink_from_d1(Err("reqwest tls unavailable".to_owned()))
        .expect_err("must fail closed");
    assert!(
        err.contains("durable audit sink unavailable"),
        "loud fail-closed message, got: {err}"
    );
}

#[test]
fn ac_sink_fails_closed_when_d1_client_unavailable() {
    let err = ac_audit_sink_from_d1(Err("reqwest tls unavailable".to_owned()))
        .expect_err("must fail closed");
    assert!(err.contains("durable audit sink unavailable"), "got: {err}");
}
