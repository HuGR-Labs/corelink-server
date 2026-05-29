//! `POST /v8/artifacts/events` telemetry endpoint types.
//!
//! Turbo sends build-event telemetry to the remote cache host.  CoreLink
//! accepts the request body without parsing it and returns HTTP 200.  No
//! state is mutated and no audit event is emitted — telemetry data is not
//! CoreLink's to own.

/// Request envelope for `POST /v8/artifacts/events`.
///
/// The body is accepted as raw bytes; no schema is enforced.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TurboEventsRequest {
    /// Raw body bytes from the Turbo client (may be empty; never parsed).
    pub body: Vec<u8>,
    /// Caller principal (already authenticated upstream).
    pub principal: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl TurboEventsRequest {
    /// Construct a [`TurboEventsRequest`].
    #[must_use]
    pub fn new(body: impl Into<Vec<u8>>, principal: impl Into<String>, at_unix_ms: u64) -> Self {
        Self {
            body: body.into(),
            principal: principal.into(),
            at_unix_ms,
        }
    }
}

/// Response for `POST /v8/artifacts/events` — always succeeds (200 OK, empty
/// body).  Turbo does not gate on the response body.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TurboEventsResponse;

impl TurboEventsResponse {
    /// Construct the unit response.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for TurboEventsResponse {
    fn default() -> Self {
        Self::new()
    }
}
