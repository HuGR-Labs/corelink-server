//! `POST /v8/artifacts/status` static endpoint types.
//!
//! Turbo queries `/v8/artifacts/status` to check whether remote caching is
//! enabled.  CoreLink always returns `{"status":"enabled"}`.

use serde::Serialize;

/// Request envelope for `POST /v8/artifacts/status`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TurboStatusRequest {
    /// Caller principal (already authenticated upstream).
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl TurboStatusRequest {
    /// Construct a [`TurboStatusRequest`].
    #[must_use]
    pub fn new(principal: impl Into<String>, at_unix_ms: u64) -> Self {
        Self {
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// Response for `POST /v8/artifacts/status`.
///
/// Always serialises to `{"status":"enabled"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct TurboStatusPayload {
    /// Cache status.  Always `"enabled"` in CoreLink's implementation.
    pub status: String,
}

impl TurboStatusPayload {
    /// Construct the static `{"status":"enabled"}` response body.
    #[must_use]
    pub fn enabled() -> Self {
        Self {
            status: "enabled".into(),
        }
    }
}
