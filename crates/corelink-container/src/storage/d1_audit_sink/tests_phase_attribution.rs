//! The property: the blocking D1-over-HTTP audit write is ATTRIBUTED, not left
//! in the unattributed residue.
//!
//! `write_blocking` is the one choke point every SYNC CAS/AC/signup
//! `emit`/`append` routes through, and it opens a `PhaseScope` for
//! `Phase::Audit` (`oaudit`) around the `block_in_place`/`block_on` call. Since
//! `PhaseScope`'s `Drop` records on EVERY exit path, a FAILING D1 call proves
//! the wrap is in place just as well as a succeeding one — which is what lets
//! this be a test at all.
//!
//! This is the seam that keeps `oaudit` off the `oother` residue on the
//! container's own `Server-Timing` header; see `origin_timing.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::stub_env;
use super::*;

/// `write_blocking` — the ONE choke point every CAS/AC/signup
/// `emit`/`append` call routes through — must attribute its blocking
/// D1-over-HTTP call to `Phase::Audit` regardless of whether the call
/// itself succeeds or fails: `PhaseScope`'s `Drop` records on EVERY exit
/// path (see `origin_timing.rs`), so a network failure here proves the
/// wrap is in place just as well as a success would.
///
/// Driven through the real `origin_timing_layer` axum middleware — the
/// only public way to scope a ledger from outside `origin_timing.rs`
/// (its `LEDGER` task-local is private to that module) — and reads the
/// raw ledger via [`crate::origin_timing::current_ledger`] rather than
/// the rendered `Server-Timing` header, so it needs no
/// `CORELINK_ORIGIN_TIMING_DETAIL=on` (the header gate does not affect
/// what the ledger itself records — see `origin_timing.rs`'s doc on
/// `detail_phases_enabled`).
///
/// Gated behind `#[ignore]` — like `storage_r2_round_trip` — because
/// `D1HttpClient::new` hardcodes `query_url` to the real
/// `api.cloudflare.com`, so this test needs outbound network reachability
/// (not live credentials: a 401 from a reachable-but-wrong-token request
/// still proves the `block_in_place` call ran and was timed). Run
/// manually with:
///
/// ```bash
/// cargo test -p corelink-server d1_audit_write_blocking_records_oaudit_phase -- --ignored
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires outbound network reachability to api.cloudflare.com"]
async fn d1_audit_write_blocking_records_oaudit_phase() {
    let sink = cas_audit_sink_from_d1(D1HttpClient::new(&stub_env())).expect("durable sink builds");
    let captured: std::sync::Arc<
        std::sync::Mutex<Option<std::sync::Arc<crate::origin_timing::PhaseLedger>>>,
    > = std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured_in_handler = std::sync::Arc::clone(&captured);

    let app = axum::Router::new()
        .route(
            "/x",
            axum::routing::get(move || {
                let sink = std::sync::Arc::clone(&sink);
                let captured = std::sync::Arc::clone(&captured_in_handler);
                async move {
                    let event = corelink_handler_cas::AuditEvent::new(
                        corelink_handler_cas::AuditEventKind::ReadAttempted,
                        "tenant-x",
                        "deadbeef00000000000000000000000000000000000000000000000000000001",
                        "caller@tenant-x",
                        1,
                    );
                    let _ = corelink_handler_cas::AuditSink::emit(&*sink, event);
                    *captured.lock().expect("mutex not poisoned") =
                        crate::origin_timing::current_ledger();
                    axum::http::StatusCode::OK
                }
            }),
        )
        .layer(axum::middleware::from_fn(
            crate::origin_timing::origin_timing_layer,
        ));

    let _resp = tower::ServiceExt::oneshot(
        app,
        axum::http::Request::builder()
            .uri("/x")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    let ledger = captured
        .lock()
        .expect("mutex not poisoned")
        .clone()
        .expect("current_ledger() must see the scope origin_timing_layer opened");
    assert!(
        ledger.micros(crate::origin_timing::Phase::Audit).is_some(),
        "the blocking D1 write in write_blocking must be attributed to \
         Phase::Audit even when the D1 call itself errors"
    );
}
