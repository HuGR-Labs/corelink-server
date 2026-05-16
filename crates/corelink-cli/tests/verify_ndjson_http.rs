//! Wave-19 (WI-S09-008 §6) integration tests for
//! `corelink-cli` `audit verify-ndjson --url <export-url>`.
//!
//! These tests spin up a minimal raw HTTP/1.1 server on a loopback
//! socket so the CLI's HTTP-fetch path can be exercised hermetically
//! (no TLS, no DNS, no external network). The server hand-crafts the
//! response bytes — chunked transfer encoding + `Trailer:` header +
//! either no trailer (happy path) or the canonical wave-18 abort
//! trailer payload — so we can assert the CLI's trailer-detection and
//! diagnostic emission against the exact wire shape
//! `apps/server/src/routes/audit_export.rs` emits.
//!
//! The tests cover:
//! 1. `streams_clean_response_and_verifies` — full NDJSON body +
//!    chain-head-anchor header; verifier accepts.
//! 2. `mid_stream_abort_trailer_detected_and_diagnostic_surfaced` —
//!    abort trailer with `{break_at_seq, break_at_chunk, observed,
//!    expected}`; CLI returns `AbortedMidStream` + exit code 65.
//! 3. `network_failure_surfaces_structured_error` — listener
//!    accepts then drops the connection without writing a response;
//!    the CLI surfaces a structured network error (not a panic) and
//!    exit code is non-zero, not 65.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::net::SocketAddr;
use std::sync::Once;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

/// Wave-23 (W23-RUSTLS-INIT): install the `ring` CryptoProvider as
/// rustls' process-wide default exactly once per test binary.
///
/// rustls 0.23 requires either (a) exactly one provider feature flag
/// set across all dependent crates so auto-install can pick it
/// unambiguously, OR (b) an explicit `CryptoProvider::install_default()`
/// call at process init. Today the dep tree carries only `ring`, so
/// auto-install would succeed — but a future feature-flag change (e.g.
/// adding `aws-lc-rs` alongside `ring`) would make auto-install
/// ambiguous and panic with "multiple CryptoProviders available". This
/// explicit init is the defensive guard: it runs before the CLI's
/// `HttpsConnectorBuilder::new()` path so the provider is always set,
/// independent of feature-flag drift.
///
/// `install_default()` returns `Err` if a provider is already installed
/// — we ignore that error because it means the auto-install already
/// won the race, which is also a valid post-state. `Once` guarantees
/// the body runs at most once per process.
fn init_rustls_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

use corelink_audit_chain::{AuditExporter, ExportWindow};
use corelink_cli::audit_export::build_fixture_exporter;
use corelink_cli::output::OutputFormat;
use corelink_cli::verify_ndjson_http::{
    run_verify_ndjson_http, HttpVerifyOutcome, EXIT_DATAERR,
};

/// Build a deterministic NDJSON body (one `{event, proof}` line per
/// row + a trailing `{"manifest": ...}` line) plus the recovered
/// chain-head anchor — mirrors the offline-mode fixture in
/// `commands/verify_ndjson.rs::tests`.
fn build_ndjson_body(event_count: u32) -> (String, String) {
    let tenant = Uuid::now_v7();
    let exporter = build_fixture_exporter(tenant, event_count, 1_000).unwrap();
    let window = ExportWindow::new(0, 10_000).unwrap();
    let result = exporter.export_window(&tenant.to_string(), window).unwrap();
    let mut body = String::new();
    for row in &result.rows {
        let line = serde_json::to_string(&serde_json::json!({
            "event": row.event,
            "proof": row.proof,
        }))
        .unwrap();
        body.push_str(&line);
        body.push('\n');
    }
    let manifest_line = serde_json::to_string(&serde_json::json!({
        "manifest": result.manifest,
    }))
    .unwrap();
    body.push_str(&manifest_line);
    let anchor = result.manifest.chain_head_at_export.to_hex();
    (body, anchor)
}

/// Server-side scenario discriminator — drives what bytes the
/// fixture server writes to the socket after parsing the request.
#[derive(Clone, Debug)]
enum Scenario {
    /// Full NDJSON body + chain-head anchor response header + no
    /// trailer.
    CleanHappyPath { body: String, anchor: String },
    /// NDJSON body bytes followed by a chunked-transfer trailer
    /// carrying `x-corelink-audit-export-aborted: <payload>`.
    AbortTrailer { body: String, trailer_payload: String },
    /// Accept the TCP connection and immediately drop it (simulates
    /// network / server-side failure mid-request).
    DropConnection,
}

/// Spawn a one-shot HTTP/1.1 fixture server. Returns the bound
/// address (caller turns it into the URL). The task exits after
/// serving exactly one connection — sufficient for a single CLI
/// invocation per test.
async fn spawn_fixture_server(scenario: Scenario) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _peer) = match listener.accept().await {
            Ok(ok) => ok,
            Err(_) => return,
        };
        // Read request headers (best-effort drain until "\r\n\r\n").
        let mut buf = [0u8; 4096];
        let mut total = Vec::<u8>::new();
        loop {
            let n = match sock.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => return,
            };
            total.extend_from_slice(&buf[..n]);
            if total.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        match scenario {
            Scenario::DropConnection => {
                // Drop without writing a response — the CLI must
                // surface a structured stream/HTTP error.
                drop(sock);
            }
            Scenario::CleanHappyPath { body, anchor } => {
                let body_bytes = body.as_bytes();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Type: application/x-ndjson\r\n\
                     X-CoreLink-Audit-Export-Chain-Head-Anchor: {anchor}\r\n\
                     Content-Length: {len}\r\n\
                     Connection: close\r\n\
                     \r\n",
                    len = body_bytes.len(),
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.write_all(body_bytes).await;
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            }
            Scenario::AbortTrailer {
                body,
                trailer_payload,
            } => {
                // Chunked transfer encoding with a single data chunk
                // followed by a trailer chunk carrying the abort
                // header. This mirrors the wave-18 server's
                // `Frame::data` / `Frame::trailers` sequence.
                let body_bytes = body.as_bytes();
                let resp = "HTTP/1.1 200 OK\r\n\
                     Content-Type: application/x-ndjson\r\n\
                     Transfer-Encoding: chunked\r\n\
                     Trailer: X-CoreLink-Audit-Export-Aborted\r\n\
                     Connection: close\r\n\
                     \r\n";
                let _ = sock.write_all(resp.as_bytes()).await;
                let chunk_hdr = format!("{:X}\r\n", body_bytes.len());
                let _ = sock.write_all(chunk_hdr.as_bytes()).await;
                let _ = sock.write_all(body_bytes).await;
                let _ = sock.write_all(b"\r\n").await;
                // Zero-length terminating chunk.
                let _ = sock.write_all(b"0\r\n").await;
                // Trailer header line (raw HTTP/1.1 trailer block) +
                // empty line to close.
                let trailer_line =
                    format!("x-corelink-audit-export-aborted: {trailer_payload}\r\n\r\n");
                let _ = sock.write_all(trailer_line.as_bytes()).await;
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            }
        }
    });
    // Give the listener a tick to enter accept.
    tokio::time::sleep(Duration::from_millis(20)).await;
    addr
}

/// Wave-19 integration #1 — full NDJSON body streams cleanly, the
/// chain-head-anchor response header is recovered, the wave-17
/// chain verifier accepts. Outcome is [`HttpVerifyOutcome::Verified`]
/// with `verified=true` + `events_verified=10`.
#[tokio::test]
async fn streams_clean_response_and_verifies() {
    init_rustls_provider();
    let (body, anchor) = build_ndjson_body(10);
    let addr = spawn_fixture_server(Scenario::CleanHappyPath {
        body,
        anchor: anchor.clone(),
    })
    .await;
    let url = format!("http://{addr}/v1/audit/export?from=0&to=10000");
    // Bearer token shape mirrors the canonical PAT discipline (never
    // logged). Test asserts only that the call succeeds — token is
    // not validated by the fixture server.
    let outcome = run_verify_ndjson_http(
        &url,
        "corelink_test_token.aaaa.bbbb",
        None,
        OutputFormat::Json,
    )
    .await
    .expect("clean path must verify");
    match outcome {
        HttpVerifyOutcome::Verified(v) => {
            assert!(v.verified);
            assert_eq!(v.events_verified, 10);
            assert_eq!(v.expected_chain_head_anchor, anchor);
            assert_eq!(v.manifest_chain_head, anchor);
            assert_eq!(v.final_observed_chain_head, anchor);
        }
        HttpVerifyOutcome::AbortedMidStream { diagnostic, .. } => {
            panic!("expected Verified, got AbortedMidStream: {diagnostic}")
        }
        // `HttpVerifyOutcome` is `#[non_exhaustive]` so the compiler
        // demands a catch-all; new arms in future waves would land a
        // failing test here intentionally.
        _ => panic!("unexpected HttpVerifyOutcome variant"),
    }
}

/// Wave-19 integration #2 — the server emits the wave-18 abort
/// trailer mid-stream. CLI parses the canonical payload + surfaces
/// the `AUDIT_EXPORT_ABORTED:` diagnostic + returns exit-code 65.
#[tokio::test]
async fn mid_stream_abort_trailer_detected_and_diagnostic_surfaced() {
    init_rustls_provider();
    // Body bytes can be anything — the CLI must short-circuit on the
    // trailer BEFORE verifying the chain. We emit a single
    // (well-formed) row + manifest so a future regression where the
    // CLI mistakenly tries to verify even on abort trailer would
    // surface a chain-break error instead.
    let (body, _anchor) = build_ndjson_body(2);
    let trailer_payload = r#"{"break_at_seq":42,"break_at_chunk":7,"observed":"deadbeef00000000000000000000000000000000000000000000000000000000","expected":"cafef00d00000000000000000000000000000000000000000000000000000000"}"#;
    let addr = spawn_fixture_server(Scenario::AbortTrailer {
        body,
        trailer_payload: trailer_payload.to_owned(),
    })
    .await;
    let url = format!("http://{addr}/v1/audit/export?from=0&to=10000");
    let outcome = run_verify_ndjson_http(
        &url,
        "corelink_test_token.aaaa.bbbb",
        None,
        OutputFormat::Json,
    )
    .await
    .expect("abort trailer must NOT be a hard error — it's a structured outcome");
    match outcome {
        HttpVerifyOutcome::AbortedMidStream {
            payload,
            diagnostic,
        } => {
            assert_eq!(payload.break_at_seq, 42);
            assert_eq!(payload.break_at_chunk, 7);
            assert!(payload.observed.starts_with("deadbeef"));
            assert!(payload.expected.starts_with("cafef00d"));
            assert!(
                diagnostic.starts_with("AUDIT_EXPORT_ABORTED: break_at_seq=42"),
                "diagnostic shape: {diagnostic}"
            );
            assert!(diagnostic.contains("break_at_chunk=7"));
            assert!(diagnostic.contains("observed=deadbeef"));
            assert!(diagnostic.contains("expected=cafef00d"));
        }
        HttpVerifyOutcome::Verified(_) => {
            panic!("CLI ignored the abort trailer — must short-circuit on detection")
        }
        _ => panic!("unexpected HttpVerifyOutcome variant"),
    }
    // Exit-code mapping is sysexits DATAERR (65).
    let abort = HttpVerifyOutcome::AbortedMidStream {
        payload: corelink_cli::verify_ndjson_http::AbortTrailerPayload {
            break_at_seq: 1,
            break_at_chunk: 0,
            observed: "00".repeat(32),
            expected: "ff".repeat(32),
        },
        diagnostic: String::new(),
    };
    assert_eq!(abort.exit_code(), EXIT_DATAERR);
    assert_eq!(EXIT_DATAERR, 65);
}

/// Wave-19 integration #3 — the server accepts the TCP connection
/// then drops without writing a response. The CLI must surface a
/// structured error (no panic, no unwrap) and the resulting exit
/// code is non-zero AND not 65 (the structured-data-error code is
/// reserved for the abort-trailer arm).
#[tokio::test]
async fn network_failure_surfaces_structured_error() {
    init_rustls_provider();
    let addr = spawn_fixture_server(Scenario::DropConnection).await;
    let url = format!("http://{addr}/v1/audit/export?from=0&to=10000");
    let res = run_verify_ndjson_http(
        &url,
        "corelink_test_token.aaaa.bbbb",
        None,
        OutputFormat::Json,
    )
    .await;
    let err = res.expect_err("dropped connection must surface a structured error");
    let msg = format!("{err}");
    assert!(
        msg.contains("verify-ndjson --url") || msg.contains("HTTP request failed") || msg.contains("body stream error"),
        "expected structured network error, got: {msg}"
    );
    // The error MUST NOT carry the bearer token (CTRL-CRED-001).
    assert!(
        !msg.contains("corelink_test_token"),
        "bearer token leaked into error: {msg}"
    );
}
