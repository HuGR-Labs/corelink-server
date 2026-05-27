//! `corelink audit verify-ndjson --url <export-url>` — wave-19 HTTP-aware
//! re-verify of a `GET /v1/audit/export` response.
//!
//! Wave-17 shipped offline (`--ndjson <file>`); wave-18 shipped the wire
//! format with the `x-corelink-audit-export-aborted` trailer carrying
//! `{break_at_seq, break_at_chunk, observed, expected}` JSON on the
//! mid-stream chain-break arm. This module lifts the wave-18
//! `cli_compat_diagnostic_from_trailer` helper (see
//! `apps/server/tests/audit_export.rs`) into a production CLI path:
//!
//! 1. Issue a streaming GET against the export URL with the customer
//!    Bearer PAT.
//! 2. Read the response frame-by-frame via `http_body::Body::frame()`
//!    — data frames accumulate into the NDJSON body, the trailer frame
//!    carries the abort signal (if any).
//! 3. On trailer detection: parse the canonical JSON payload, print
//!    the diagnostic to **stderr**, and exit with **sysexits DATAERR
//!    (65)**. The verifier is short-circuited (the body is already
//!    cryptographically broken per the server's mid-stream re-check).
//! 4. On no-trailer (happy path): run the wave-17 NDJSON verifier on
//!    the body bytes against the `--chain-head-anchor` published by
//!    the server in the `X-CoreLink-Audit-Export-Chain-Head-Anchor`
//!    response header.
//!
//! ## Why not `reqwest`?
//!
//! `reqwest::Response::trailers()` is not part of the public API in
//! `reqwest = "0.12"` — the response body is gated as `pub(crate)`.
//! We use `hyper` + `hyper-util` + `hyper-rustls` directly so we can
//! read HTTP/1.1 trailers via `http_body::Body::poll_frame`.
//!
//! ## Charter compliance
//!
//! - `#![forbid(unsafe_code)]` inherited from the crate.
//! - No `unwrap` / `expect` / `panic` — every fallible op surfaces via
//!   `?` into [`crate::error::CliError`].
//! - Bearer token is NEVER logged or printed; tracing emits mask any
//!   `authorization` header derived value (`tracing::debug!` lines
//!   only log the URL path + status, never the bearer).

use std::pin::Pin;
use std::str::FromStr;

use bytes::Bytes;
use http::header::HeaderName;
use http::{HeaderValue, Request};
use http_body::{Body, Frame};
use http_body_util::Empty;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client as LegacyClient;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};

use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};
// Reach the offline verifier via the crate-root alias. Both the
// binary (`main.rs`) and the lib (`lib.rs`) expose this module at
// `crate::verify_ndjson` so the path resolves uniformly.
use crate::verify_ndjson::{
    parse_ndjson_envelope_public, run_verify_chain_public, VerifyNdjsonOutcome,
};

/// HTTP trailer name (lowercase wire form per HTTP/2 + HTTP/1.1
/// canonical lowercase) carrying the wave-18 mid-stream abort signal.
///
/// Mirrors `apps/server/src/routes/audit_export.rs::HEADER_EXPORT_ABORTED`.
/// We re-declare it here so the CLI crate stays decoupled from the
/// server crate (no compile-time dep cycle).
pub const HEADER_EXPORT_ABORTED: &str = "x-corelink-audit-export-aborted";

/// Response header advertising the BLAKE3 chain-head anchor at export
/// time. The verifier cross-checks the recomputed final hash against
/// this value (fail-CLOSED on mismatch).
pub const HEADER_CHAIN_HEAD_ANCHOR: &str = "x-corelink-audit-export-chain-head-anchor";

/// Sysexits canonical `DATAERR` exit code — surfaced on the abort-
/// trailer arm so wrapping scripts (Drata, SIEM, automated re-export
/// loops) can distinguish "data integrity broken on the wire" from
/// generic network failure (any other non-zero exit).
pub const EXIT_DATAERR: i32 = 65;

/// Canonical payload of the `x-corelink-audit-export-aborted` trailer
/// per wave-18 server contract. Mirrors the JSON object the route
/// emits via `Frame::trailers`.
///
/// Intentionally **not** `#[non_exhaustive]` — the wire shape is
/// pinned by the server contract (4 fields) and downstream consumers
/// (tests, SIEM wrappers, the canonical diagnostic formatter) need
/// struct-literal construction.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AbortTrailerPayload {
    /// Audit-event seq at which the server detected the break.
    pub break_at_seq: u64,
    /// R2 chunk index at which the server detected the break.
    pub break_at_chunk: u64,
    /// Observed BLAKE3 hash (64 lowercase hex).
    pub observed: String,
    /// Expected BLAKE3 hash (64 lowercase hex).
    pub expected: String,
}

/// Structured outcome of `verify-ndjson --url` — exposed for tests +
/// JSON output. The `Display` impl is intentionally human-readable
/// and never includes the Bearer (CTRL-CRED-001).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HttpVerifyOutcome {
    /// Happy path — the response streamed cleanly, no abort trailer,
    /// and the wave-17 chain verifier accepted the body against the
    /// recovered chain-head anchor.
    Verified(VerifyNdjsonOutcome),
    /// Wave-18 abort trailer detected mid-stream. The verifier is
    /// short-circuited; the diagnostic surfaces the canonical
    /// `{break_at_seq, break_at_chunk, observed, expected}` shape +
    /// the human-readable line.
    AbortedMidStream {
        /// The parsed trailer payload (machine-readable).
        payload: AbortTrailerPayload,
        /// The exact diagnostic string printed to stderr.
        diagnostic: String,
    },
}

impl HttpVerifyOutcome {
    /// Map the outcome to a sysexits-style process exit code:
    /// - [`HttpVerifyOutcome::Verified`] => `0`
    /// - [`HttpVerifyOutcome::AbortedMidStream`] => [`EXIT_DATAERR`]
    ///
    /// The binary path translates `CliError::AuditExportAborted` to
    /// the same code in `main.rs`; this helper exists for lib
    /// consumers (integration tests + downstream SDKs that wrap the
    /// `run_verify_ndjson_http` call).
    #[allow(dead_code, reason = "binary path uses CliError::AuditExportAborted; this helper is for lib consumers")]
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Verified(_) => 0,
            Self::AbortedMidStream { .. } => EXIT_DATAERR,
        }
    }
}

impl std::fmt::Display for HttpVerifyOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verified(v) => write!(f, "{v}"),
            Self::AbortedMidStream { diagnostic, .. } => write!(f, "{diagnostic}"),
        }
    }
}

/// Run `corelink audit verify-ndjson --url <URL> --bearer <PAT>
/// [--chain-head-anchor <HEX>]`.
///
/// Streams the response, reads trailers, surfaces the canonical
/// diagnostic on abort, otherwise runs the wave-17 chain verifier.
///
/// The `--chain-head-anchor` is optional on the HTTP path: when
/// omitted, the verifier reads it from the
/// `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header
/// emitted by the server at export time. When supplied, the CLI ALSO
/// cross-checks the header matches (defence-in-depth — a tampered
/// response header alone CANNOT mask a body mismatch).
///
/// # Errors
///
/// Returns [`CliError`] on:
/// - URL parse failure / scheme other than `http`/`https`.
/// - Network / TLS / DNS failure.
/// - HTTP status ≠ 200 (the server's reject arms are 4xx/5xx + audit
///   row — the customer-CLI surfaces the status code to stderr).
/// - Trailer JSON parse failure (malformed wire contract).
/// - Chain verifier rejection (delegates to the wave-17 verifier;
///   surfaces the same structured chain-break error).
/// - Missing `X-CoreLink-Audit-Export-Chain-Head-Anchor` header
///   AND missing `--chain-head-anchor` flag (verifier needs at least
///   one source of truth for the anchor).
pub async fn run_verify_ndjson_http(
    url: &str,
    bearer: &str,
    chain_head_anchor_override: Option<&str>,
    output_fmt: OutputFormat,
) -> Result<HttpVerifyOutcome, CliError> {
    // 1. Build the HTTPS-capable hyper client.
    //    `webpki-tokio` ships Mozilla CA roots without ring NSS deps;
    //    `rustls-native-certs` falls back to OS trust store.
    //    Order: native roots first, webpki fallback.
    let tls_builder = HttpsConnectorBuilder::new();
    let native_or_webpki = match tls_builder.with_native_roots() {
        Ok(b) => b.https_or_http().enable_http1().build(),
        Err(e) => {
            tracing::debug!(
                target = "corelink_cli::verify_ndjson_http",
                "native roots load failed ({e}); falling back to webpki bundled roots"
            );
            HttpsConnectorBuilder::new()
                .with_webpki_roots()
                .https_or_http()
                .enable_http1()
                .build()
        }
    };
    let client: LegacyClient<_, Empty<Bytes>> =
        LegacyClient::builder(TokioExecutor::new()).build(native_or_webpki);

    // 2. Build the request. Bearer is set as an HTTP header — header
    //    values are NOT logged anywhere (charter CTRL-CRED-001).
    let uri = http::Uri::from_str(url)
        .map_err(|e| CliError::Other(format!("verify-ndjson --url: invalid URL: {e}")))?;
    let scheme = uri.scheme_str().unwrap_or_default();
    if scheme != "http" && scheme != "https" {
        return Err(CliError::Other(format!(
            "verify-ndjson --url: URL must be http or https; got scheme {scheme:?}"
        )));
    }
    let bearer_header = HeaderValue::from_str(&format!("Bearer {bearer}")).map_err(|_| {
        CliError::Other(
            "verify-ndjson --url: --bearer contains invalid HTTP-header bytes".to_owned(),
        )
    })?;
    let mut req_builder = Request::builder()
        .method(http::Method::GET)
        .uri(&uri)
        .header(http::header::ACCEPT, "application/x-ndjson");
    if let Some(headers) = req_builder.headers_mut() {
        headers.insert(http::header::AUTHORIZATION, bearer_header);
    }
    let req = req_builder
        .body(Empty::<Bytes>::new())
        .map_err(|e| CliError::Other(format!("verify-ndjson --url: request build failed: {e}")))?;

    tracing::debug!(
        target = "corelink_cli::verify_ndjson_http",
        url = %uri,
        "issuing verify-ndjson GET"
    );

    // 3. Issue + collect body + trailers frame-by-frame.
    let resp = client
        .request(req)
        .await
        .map_err(|e| CliError::Other(format!("verify-ndjson --url: HTTP request failed: {e}")))?;
    let status = resp.status();
    let resp_headers = resp.headers().clone();
    if !status.is_success() {
        return Err(CliError::Other(format!(
            "verify-ndjson --url: server returned HTTP {status}; expected 200"
        )));
    }

    let body = resp.into_body();
    let collected = collect_body_with_trailers(body).await?;

    // 4. Trailer-first: if the abort trailer is present, surface the
    //    canonical diagnostic and short-circuit.
    let trailer_name = match HeaderName::from_static_lower(HEADER_EXPORT_ABORTED) {
        Some(n) => n,
        None => {
            return Err(CliError::Other(
                "verify-ndjson --url: trailer name canonicalisation failed".to_owned(),
            ));
        }
    };
    if let Some(trailer_value) = collected.trailers.get(&trailer_name) {
        let payload_str = trailer_value.to_str().map_err(|e| {
            CliError::Other(format!(
                "verify-ndjson --url: abort trailer not ASCII: {e} — wire contract requires canonical JSON"
            ))
        })?;
        let payload: AbortTrailerPayload = serde_json::from_str(payload_str).map_err(|e| {
            CliError::Other(format!(
                "verify-ndjson --url: abort trailer JSON parse failed: {e}"
            ))
        })?;
        let diagnostic = format_abort_diagnostic(&payload);
        eprintln!("{diagnostic}");
        return Ok(HttpVerifyOutcome::AbortedMidStream {
            payload,
            diagnostic,
        });
    }

    // 5. Happy path — resolve the chain-head anchor (header first;
    //    override flag wins if BOTH supplied + match).
    let header_anchor = resp_headers
        .get(HEADER_CHAIN_HEAD_ANCHOR)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned());
    let resolved_anchor = match (chain_head_anchor_override, header_anchor.as_deref()) {
        (Some(flag), Some(header)) => {
            if !ascii_eq_ct(flag.trim(), header.trim()) {
                return Err(CliError::Other(format!(
                    "verify-ndjson --url: --chain-head-anchor ({flag}) disagrees with response header X-CoreLink-Audit-Export-Chain-Head-Anchor ({header}); refusing to verify under ambiguous anchors"
                )));
            }
            flag.trim().to_owned()
        }
        (Some(flag), None) => flag.trim().to_owned(),
        (None, Some(header)) => header.to_owned(),
        (None, None) => {
            return Err(CliError::Other(
                "verify-ndjson --url: response did not carry X-CoreLink-Audit-Export-Chain-Head-Anchor and --chain-head-anchor was not supplied — cannot verify".to_owned(),
            ));
        }
    };

    // 6. Body bytes → NDJSON envelope → wave-17 chain verifier.
    let body_str = std::str::from_utf8(&collected.bytes).map_err(|e| {
        CliError::Other(format!(
            "verify-ndjson --url: response body is not UTF-8: {e}"
        ))
    })?;
    let (rows, manifest) = parse_ndjson_envelope_public(body_str)?;
    let outcome = run_verify_chain_public(
        &rows,
        &manifest,
        &resolved_anchor,
        // Synthetic "source label" for the verifier diagnostics. We
        // prefer the URL (already non-secret) over a generic "<http>"
        // so chain-break errors pinpoint which export they came from.
        url,
        output_fmt,
    )?;
    Ok(HttpVerifyOutcome::Verified(outcome))
}

/// Format the canonical wave-18 diagnostic line emitted on stderr +
/// embedded in [`HttpVerifyOutcome::AbortedMidStream`]. Shape pinned
/// in the integration tests + wave-18 helper
/// `cli_compat_diagnostic_from_trailer` (see
/// `apps/server/tests/audit_export.rs`).
#[must_use]
pub fn format_abort_diagnostic(p: &AbortTrailerPayload) -> String {
    format!(
        "AUDIT_EXPORT_ABORTED: break_at_seq={} break_at_chunk={} observed={} expected={}",
        p.break_at_seq, p.break_at_chunk, p.observed, p.expected,
    )
}

/// Constant-time ASCII equality for the anchor header / flag cross-
/// check. Both inputs are 64-char lowercase BLAKE3 hex on the happy
/// path; we still constant-time the compare so a side-channel CANNOT
/// reveal where the strings differ (mirrors `corelink_audit_chain::
/// hashes_eq_ct`'s discipline).
fn ascii_eq_ct(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes().iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

/// Output of [`collect_body_with_trailers`] — the full body bytes +
/// any trailer header map the server emitted at end-of-stream.
struct BodyAndTrailers {
    bytes: Vec<u8>,
    trailers: http::HeaderMap,
}

/// Drain a streaming hyper body frame-by-frame, accumulating data
/// frames into a contiguous `Vec<u8>` + capturing the (possibly
/// multiple) trailer frames into a single `HeaderMap`.
///
/// Caps the body at 64 MiB so a tampered server cannot drain CLI
/// memory; the wave-18 server's export endpoint emits manifests for
/// windows that comfortably fit (a single 1k-event window is < 1MiB).
async fn collect_body_with_trailers<B>(body: B) -> Result<BodyAndTrailers, CliError>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
{
    const MAX_BYTES: usize = 64 * 1024 * 1024;

    let mut acc = Vec::<u8>::new();
    let mut trailers = http::HeaderMap::new();
    let mut pinned = Box::pin(body);
    while let Some(frame_res) = poll_next_frame(&mut pinned).await {
        let frame: Frame<Bytes> = frame_res?;
        if frame.is_data() {
            if let Ok(data) = frame.into_data() {
                if acc.len().saturating_add(data.len()) > MAX_BYTES {
                    return Err(CliError::Other(format!(
                        "verify-ndjson --url: response body exceeded {MAX_BYTES} byte cap"
                    )));
                }
                acc.extend_from_slice(&data);
            }
        } else if frame.is_trailers() {
            if let Ok(t) = frame.into_trailers() {
                trailers.extend(t);
            }
        }
    }
    Ok(BodyAndTrailers {
        bytes: acc,
        trailers,
    })
}

/// Tiny `frame()` shim — `http_body::Body::frame()` is on the trait
/// but requires a `Pin<&mut Self>` and an async cradle; this helper
/// keeps the call-site tidy + surfaces frame errors as `CliError`.
async fn poll_next_frame<B>(
    body: &mut Pin<Box<B>>,
) -> Option<Result<Frame<Bytes>, CliError>>
where
    B: Body<Data = Bytes> + ?Sized,
    B::Error: std::fmt::Display,
{
    use std::future::poll_fn;
    use std::task::Poll;
    let frame = poll_fn(|cx| match body.as_mut().poll_frame(cx) {
        Poll::Pending => Poll::Pending,
        Poll::Ready(opt) => Poll::Ready(opt),
    })
    .await;
    frame.map(|res| {
        res.map_err(|e| CliError::Other(format!("verify-ndjson --url: body stream error: {e}")))
    })
}

/// `HeaderName` lacks a const-fn lowercase constructor; we wrap the
/// fallible path so callers do not propagate the unrelated
/// `InvalidHeaderName` error.
trait HeaderNameLowerExt {
    fn from_static_lower(s: &str) -> Option<HeaderName>;
}

impl HeaderNameLowerExt for HeaderName {
    fn from_static_lower(s: &str) -> Option<HeaderName> {
        HeaderName::from_bytes(s.as_bytes()).ok()
    }
}

/// Emit the outcome in the requested output format. Pure I/O surface
/// — does not touch the network. Public so `main.rs` can re-emit
/// after the async run completes (lib consumers + future
/// machine-readable JSON output also use this).
///
/// # Errors
///
/// Returns [`CliError::Json`] if JSON serialisation fails (should be
/// unreachable — the outcome shape is pure POD).
#[allow(dead_code, reason = "exposed for lib + future binary use; binary today emits inside run_verify_ndjson_http")]
pub fn emit_outcome(outcome: &HttpVerifyOutcome, fmt: OutputFormat) -> Result<(), CliError> {
    let f = Formatter::new(fmt);
    match outcome {
        HttpVerifyOutcome::Verified(v) => f.emit(v).map_err(CliError::Json),
        HttpVerifyOutcome::AbortedMidStream { .. } => {
            // Already printed to stderr in run_verify_ndjson_http; we
            // also emit a compact JSON summary on stdout so machine
            // consumers can parse a structured object regardless of
            // exit code.
            f.emit(outcome).map_err(CliError::Json)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    /// Wave-19 unit — the canonical trailer payload parses + the
    /// diagnostic string round-trips the wave-18 wire shape verbatim.
    #[test]
    fn trailer_payload_parses_and_diagnostic_round_trips() {
        let payload_json = r#"{"break_at_seq":42,"break_at_chunk":3,"observed":"deadbeef","expected":"cafef00d"}"#;
        let p: AbortTrailerPayload = serde_json::from_str(payload_json).unwrap();
        assert_eq!(p.break_at_seq, 42);
        assert_eq!(p.break_at_chunk, 3);
        assert_eq!(p.observed, "deadbeef");
        assert_eq!(p.expected, "cafef00d");

        let diag = format_abort_diagnostic(&p);
        assert_eq!(
            diag,
            "AUDIT_EXPORT_ABORTED: break_at_seq=42 break_at_chunk=3 observed=deadbeef expected=cafef00d"
        );
    }

    /// Wave-19 unit — exit-code mapping.
    #[test]
    fn exit_code_mapping_dataerr_on_abort_and_zero_on_verified() {
        let abort = HttpVerifyOutcome::AbortedMidStream {
            payload: AbortTrailerPayload {
                break_at_seq: 1,
                break_at_chunk: 0,
                observed: "00".repeat(32),
                expected: "ff".repeat(32),
            },
            diagnostic: "AUDIT_EXPORT_ABORTED: ...".to_owned(),
        };
        assert_eq!(abort.exit_code(), EXIT_DATAERR);
        assert_eq!(EXIT_DATAERR, 65);
    }

    /// Wave-19 — `ascii_eq_ct` rejects mismatched lengths + flips at
    /// any position without short-circuiting.
    #[test]
    fn ascii_eq_ct_rejects_mismatched_lengths_and_any_flip() {
        assert!(ascii_eq_ct("abc", "abc"));
        assert!(!ascii_eq_ct("abc", "abcd"));
        assert!(!ascii_eq_ct("abcd", "abce"));
        assert!(!ascii_eq_ct("xbcd", "abcd"));
    }

    /// Wave-19 — malformed trailer JSON yields a structured error
    /// (not a panic / unwrap).
    #[test]
    fn malformed_trailer_payload_surfaces_structured_error() {
        let bad = r#"{"break_at_seq":"not-a-number"}"#;
        let res: Result<AbortTrailerPayload, _> = serde_json::from_str(bad);
        assert!(res.is_err(), "non-numeric break_at_seq must fail");
    }
}
