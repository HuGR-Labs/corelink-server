//! Stream verify with the canonical staging-then-commit pattern.
//!
//! Run via `cargo run --features stream --example verify_stream`.
//!
//! `verify_stream` yields each chunk BEFORE the BLAKE3 finalize at
//! end-of-stream. A naive consumer that streamed chunks straight into
//! a destination would expose tampered bytes if a `Mismatch` arrived
//! at the end. The canonical fix is to stage the chunks somewhere
//! reversible (in-memory `Vec` or a tempfile + atomic rename) and
//! COMMIT only after the stream terminates cleanly.

#![allow(clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]

use corelink_client_verify::{ClientVerifier, Digest, StreamVerifyError, VerifyError};
use futures::StreamExt;

#[tokio::main]
async fn main() {
    let body = vec![0xA5u8; 5 * 1024 * 1024];
    let expected = Digest::compute(&body);
    let cursor = std::io::Cursor::new(body.clone());

    let v = ClientVerifier::default_on();
    let stream = v.verify_stream(cursor, expected);
    tokio::pin!(stream);

    // Staging buffer: holds chunks until verify completes cleanly.
    // In a real SDK this would typically be a tempfile so we don't
    // pay the full-body memory cost; the staging *step* is what
    // matters, not the storage choice.
    let mut staging: Vec<u8> = Vec::with_capacity(body.len());
    let mut chunk_count = 0usize;
    let mut tail_err: Option<StreamVerifyError> = None;

    while let Some(item) = stream.next().await {
        match item {
            Ok(chunk) => {
                staging.extend_from_slice(&chunk);
                chunk_count += 1;
            }
            Err(e) => {
                tail_err = Some(e);
                break;
            }
        }
    }

    match tail_err {
        None => {
            // Clean termination → COMMIT staging.
            println!(
                "stream verify ok: {} bytes across {chunk_count} chunks; digest = {expected}; \
                 committed {} bytes",
                staging.len(),
                staging.len(),
            );
        }
        Some(StreamVerifyError::Mismatch(VerifyError::DigestMismatch { expected, computed })) => {
            // ABORT staging — tampered bytes never reach destination.
            eprintln!(
                "stream verify FAILED: digest mismatch (expected {expected}, computed {computed}); \
                 dropping {} bytes of staged data",
                staging.len()
            );
            drop(staging);
            std::process::exit(1);
        }
        Some(other) => {
            eprintln!("stream verify failed: {other}; dropping staging");
            drop(staging);
            std::process::exit(1);
        }
    }
}
