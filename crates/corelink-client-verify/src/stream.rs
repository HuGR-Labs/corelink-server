//! Async stream verify: incremental BLAKE3 update while passing chunks
//! through to the consumer.
//!
//! BLAKE3 is incremental: feed bytes via `Hasher::update(&[u8])`, then
//! call `finalize()` once at end-of-stream. We expose this as a
//! `futures::Stream<Item = Result<Bytes, StreamVerifyError>>` so the
//! SDK can drive an `AsyncRead` into the verifier and receive the
//! same chunks back to forward to the caller, with the verify result
//! delivered as the FINAL item: a `Mismatch` error appears at
//! end-of-stream when finalization fails the constant-time compare.
//!
//! # Detection latency
//!
//! BLAKE3 cannot prove a mismatch until the last byte has been hashed.
//! Per WI-S02-003 §2 (cycle 9 SEAL clarification), per-chunk Merkle
//! proofs are deferred to S-05 multipart (WI-S05-005); for S-02 the
//! detection latency is end-of-stream. The memory-bound benefit
//! (incremental hash, no need to buffer the full body) holds
//! independently of the detection-latency tradeoff.
//!
//! # Memory bound
//!
//! Per WI §10.3.3 the SDK allocates one chunk at a time; a 5 MiB blob
//! verifies with peak ≤ 2 MiB total overhead (chunk + hasher state).
//! We use a 64 KiB chunk size (matches Tokio default + Cloudflare R2
//! recommended part-size for streamed reads) — see
//! [`STREAM_CHUNK_BYTES`].

use bytes::{Bytes, BytesMut};
use futures::stream::Stream;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::digest::Digest;
use crate::error::{StreamVerifyError, VerifyError};
use crate::verifier::ClientVerifier;

/// Chunk size used by [`ClientVerifier::verify_stream`]. 64 KiB matches
/// the Tokio `AsyncRead` default + Cloudflare R2 streamed-read
/// recommendation. Each in-flight chunk plus the BLAKE3 hasher state
/// keeps peak memory < 2 MiB even for 5 MiB blobs.
pub const STREAM_CHUNK_BYTES: usize = 64 * 1024;

impl ClientVerifier {
    /// Stream-aware verify. Drives `reader` to end-of-stream, hashing
    /// incrementally with BLAKE3; yields each successfully-read chunk
    /// as a `Bytes` for the SDK to forward to the application. The
    /// final chunk is yielded BEFORE the end-of-stream verify; the
    /// verify outcome is then surfaced as either:
    /// - the stream terminating with a final `Err(Mismatch)` item, OR
    /// - the stream terminating with no further items (verify ok).
    ///
    /// # Streaming contract — consumer responsibility
    ///
    /// **The chunks this stream yields are NOT integrity-checked
    /// until the stream terminates cleanly.** A consumer that streams
    /// chunks straight to a destination would expose tampered bytes
    /// if `Err(Mismatch)` lands at end-of-stream. The canonical
    /// pattern is therefore:
    ///
    /// 1. Stage every yielded chunk to a *reversible* buffer
    ///    (tempfile + atomic rename, in-memory `Vec`, etc).
    /// 2. Drive the stream to completion.
    /// 3. ONLY commit the staged buffer to the consumer's
    ///    destination after the stream ends without an `Err` item.
    ///
    /// `examples/verify_stream.rs` shows the staging dance end-to-end.
    /// SDK consumers that can afford to buffer the entire body should
    /// prefer the sync [`ClientVerifier::verify`], which does not have
    /// this footgun.
    ///
    /// # Default-on contract
    ///
    /// On a default-on verifier (`config.enabled == true`) the body is
    /// hashed and compared. On a `disabled()` verifier the stream
    /// terminates with `Err(StreamVerifyError::Mismatch(VerifyDisabled))`
    /// without driving the reader (consistent with the sync `verify`
    /// short-circuit).
    pub fn verify_stream<R>(
        &self,
        reader: R,
        expected: Digest,
    ) -> impl Stream<Item = Result<Bytes, StreamVerifyError>>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        verify_stream_impl(self.config().enabled(), reader, expected)
    }
}

/// Internal helper so the stream construction does not depend on `Self`
/// for lifetime reasons (the future captures `reader` by value).
fn verify_stream_impl<R>(
    enabled: bool,
    reader: R,
    expected: Digest,
) -> impl Stream<Item = Result<Bytes, StreamVerifyError>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    State::new(enabled, reader, expected).into_stream()
}

/// State machine for stream verify. Each `poll_next` either:
/// - yields the next read chunk (hashing it on the way),
/// - yields the EOF outcome (`Ok` ends; `Mismatch` errors), or
/// - propagates an upstream I/O error.
struct State<R: AsyncRead + Unpin + Send + 'static> {
    enabled: bool,
    reader: Option<R>,
    hasher: blake3::Hasher,
    expected: Digest,
    done: bool,
}

impl<R: AsyncRead + Unpin + Send + 'static> State<R> {
    fn new(enabled: bool, reader: R, expected: Digest) -> Self {
        Self {
            enabled,
            reader: Some(reader),
            hasher: blake3::Hasher::new(),
            expected,
            done: false,
        }
    }

    fn into_stream(self) -> impl Stream<Item = Result<Bytes, StreamVerifyError>> {
        // We use `futures::stream::unfold` so the entire state machine
        // is captured in the unfold closure's state without us having
        // to hand-write `Pin<Box<dyn Stream>>`.
        futures::stream::unfold(self, |mut s| async move {
            if s.done {
                return None;
            }
            // Disabled verifier short-circuits without consuming the reader.
            if !s.enabled {
                s.done = true;
                return Some((
                    Err(StreamVerifyError::Mismatch(VerifyError::VerifyDisabled)),
                    s,
                ));
            }
            let mut buf = BytesMut::with_capacity(STREAM_CHUNK_BYTES);
            buf.resize(STREAM_CHUNK_BYTES, 0u8);
            let Some(reader) = s.reader.as_mut() else {
                // Unreachable: `done` is set the only time we drop the reader.
                s.done = true;
                return None;
            };
            match reader.read(&mut buf).await {
                Ok(0) => {
                    // EOF — finalize hasher, compare in constant time.
                    s.done = true;
                    s.reader = None;
                    let computed_arr = *s.hasher.finalize().as_bytes();
                    // CRITICAL: same constant-time compare as the sync
                    // path. Comparing raw 32-byte arrays via `subtle`
                    // (no early return on first differing byte) keeps
                    // the timing channel sealed for the stream-feature
                    // consumer. Hex-string `==` would have been
                    // short-circuiting and would leak partial-match
                    // information through observable timing.
                    use subtle::ConstantTimeEq;
                    let matches = bool::from(computed_arr.ct_eq(s.expected.as_bytes()));
                    if matches {
                        // Ok end-of-stream → stream terminates cleanly
                        // (no extra item).
                        None
                    } else {
                        let computed_hex = hex::encode(computed_arr);
                        Some((
                            Err(StreamVerifyError::Mismatch(VerifyError::DigestMismatch {
                                expected: s.expected.to_hex(),
                                computed: computed_hex,
                            })),
                            s,
                        ))
                    }
                }
                Ok(n) => {
                    buf.truncate(n);
                    let chunk = buf.freeze();
                    s.hasher.update(&chunk);
                    Some((Ok(chunk), s))
                }
                Err(e) => {
                    s.done = true;
                    s.reader = None;
                    Some((Err(StreamVerifyError::Io(e)), s))
                }
            }
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code; panic on assertion failure is the contract"
)]
mod tests {
    use super::*;
    use crate::config::VerifyConfig;
    use futures::StreamExt;

    #[tokio::test]
    async fn stream_happy_path_yields_all_chunks_and_terminates_clean() {
        let body = vec![0xA5u8; 5 * 1024 * 1024]; // 5 MiB
        let expected = Digest::compute(&body);
        let cursor = std::io::Cursor::new(body.clone());
        let v = ClientVerifier::default_on();
        let stream = v.verify_stream(cursor, expected);
        tokio::pin!(stream);
        let mut total = 0usize;
        let mut last_err: Option<StreamVerifyError> = None;
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => total += bytes.len(),
                Err(e) => last_err = Some(e),
            }
        }
        assert_eq!(total, body.len());
        assert!(last_err.is_none(), "happy path: no error item at EOF");
    }

    #[tokio::test]
    async fn stream_mismatch_detected_at_end_of_stream() {
        let body = vec![0xA5u8; 256 * 1024];
        // Expected digest is intentionally for a different body.
        let wrong = Digest::compute(b"other");
        let cursor = std::io::Cursor::new(body.clone());
        let v = ClientVerifier::default_on();
        let stream = v.verify_stream(cursor, wrong);
        tokio::pin!(stream);

        // Consume all chunks; expect them all to come through OK
        // (BLAKE3 cannot rule out a match until end-of-stream).
        let mut chunk_count = 0usize;
        let mut total = 0usize;
        let mut tail_err: Option<StreamVerifyError> = None;
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => {
                    chunk_count += 1;
                    total += bytes.len();
                }
                Err(e) => tail_err = Some(e),
            }
        }
        assert!(chunk_count >= 1);
        assert_eq!(total, body.len());
        let tail = tail_err.expect("expected mismatch err at EOF");
        match tail {
            StreamVerifyError::Mismatch(VerifyError::DigestMismatch { expected, computed }) => {
                assert_eq!(expected, wrong.to_hex());
                assert_eq!(computed, Digest::compute(&body).to_hex());
            }
            other => panic!("expected DigestMismatch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stream_disabled_short_circuits_with_verify_disabled() {
        let body = vec![1u8; 1024];
        let expected = Digest::compute(&body);
        let cursor = std::io::Cursor::new(body);
        let v = ClientVerifier::new(VerifyConfig::disabled_silent());
        let stream = v.verify_stream(cursor, expected);
        tokio::pin!(stream);
        let first = stream.next().await.expect("at least one item");
        let err = first.expect_err("must err");
        match err {
            StreamVerifyError::Mismatch(VerifyError::VerifyDisabled) => {}
            other => panic!("expected VerifyDisabled, got {other:?}"),
        }
        // No further items.
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn stream_propagates_upstream_io_error() {
        struct ErrReader;
        impl AsyncRead for ErrReader {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
                _buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Err(std::io::Error::other("synthetic")))
            }
        }
        let v = ClientVerifier::default_on();
        let stream = v.verify_stream(ErrReader, Digest::compute(b""));
        tokio::pin!(stream);
        let item = stream.next().await.expect("first item");
        match item.expect_err("io error") {
            StreamVerifyError::Io(_) => {}
            other => panic!("expected Io, got {other:?}"),
        }
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn stream_chunk_bytes_constant_matches_doc() {
        assert_eq!(STREAM_CHUNK_BYTES, 64 * 1024);
    }
}
