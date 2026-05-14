//! Stripe error taxonomy mapped from API responses.
//!
//! Every arm is `#[non_exhaustive]` at the enum level to permit
//! additive growth (e.g. new Stripe error codes).

use thiserror::Error;

/// Canonical Stripe error taxonomy. Mapped from the `error.type` /
/// `error.code` fields of Stripe's JSON error envelope plus transport
/// failure modes from `reqwest`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StripeError {
    /// HTTP 429 — rate limited. Caller should back off; the retry
    /// layer respects this automatically up to the max-retries budget.
    #[error("Stripe rate limited: {message}")]
    RateLimited {
        /// Human-readable message echoed from Stripe.
        message: String,
        /// Suggested retry delay (seconds) from `Retry-After` header.
        retry_after_seconds: Option<u64>,
    },

    /// HTTP 401 — API key invalid / revoked / expired.
    #[error("Stripe authentication failed: {0}")]
    Authentication(String),

    /// HTTP 402 — card declined (Stripe `card_error.code`).
    #[error("Stripe card declined: {code}: {message}")]
    CardDeclined {
        /// Stripe decline code (e.g. `card_declined`, `insufficient_funds`).
        code: String,
        /// Human-readable message.
        message: String,
    },

    /// HTTP 400 — invalid request parameters.
    #[error("Stripe invalid request: {0}")]
    InvalidRequest(String),

    /// Transport / TLS / DNS / connection failure (before HTTP layer).
    #[error("Stripe API connection error: {0}")]
    ApiConnection(String),

    /// Idempotency-Key collision: same key, different parameters.
    /// Stripe rejects with HTTP 400 + `idempotency_error` type.
    #[error("Stripe idempotency conflict: {0}")]
    Idempotency(String),

    /// Catch-all for unmapped Stripe API errors.
    #[error("Stripe API error [{http_status}] {code}: {message}")]
    Generic {
        /// HTTP status code.
        http_status: u16,
        /// Stripe error code (or empty string if absent).
        code: String,
        /// Human-readable message.
        message: String,
    },
}

/// Webhook signature verification error.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WebhookVerifyError {
    /// `Stripe-Signature` header missing or malformed.
    #[error("malformed Stripe-Signature header: {0}")]
    MalformedHeader(String),

    /// Timestamp older than tolerance (replay protection).
    #[error("timestamp outside tolerance window: skew={skew_seconds}s")]
    ReplayWindowExceeded {
        /// Skew between now and signed timestamp, in seconds.
        skew_seconds: i64,
    },

    /// Timestamp future-dated beyond tolerance (clock skew the other way).
    #[error("timestamp future-dated: skew={skew_seconds}s")]
    FutureDated {
        /// Negative skew (timestamp > now).
        skew_seconds: i64,
    },

    /// HMAC-SHA256 signature mismatch.
    #[error("HMAC signature mismatch")]
    SignatureMismatch,

    /// Hex decode failure on signature.
    #[error("hex decode failure: {0}")]
    HexDecode(String),
}
