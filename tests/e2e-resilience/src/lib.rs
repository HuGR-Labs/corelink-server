//! `e2e-resilience` — R3-6 end-to-end resilience harness.
//!
//! Composes the canonical pure-logic orchestrators from
//! [`corelink_ratelimit`] (camada 1 per-tenant token bucket) and
//! [`corelink_rate_headers::circuit`] (camada 0 global circuit breaker)
//! under a **logical clock** + **seeded RNG** so the suite is
//! byte-for-byte reproducible across runs.
//!
//! # Scenarios pinned (see `tests/scenarios.rs`)
//!
//! 1. **Rate-limit per tenant** — burst 110% over a per-second quota →
//!    10% rejected with `429 Too Many Requests` + per-deny audit emit
//!    BEFORE state mutation (fail-CLOSED envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 2. **Token-bucket refill** — after quota exhaust, advance the logical
//!    clock 1s, request succeeds (RFC 6585 §4 Retry-After contract holds).
//! 3. **Circuit breaker opens** — 5 consecutive 5xx responses (under
//!    tight test thresholds with `min_observations: 5` so the rolling
//!    window is saturated by the burst) → multi-signal trip → state
//!    Open → subsequent [`check`](corelink_rate_headers::GlobalCircuitBreaker::check)
//!    returns `Reject` fast (no upstream traversal) which the production
//!    Tower layer maps to 503.
//! 4. **Half-open probe** — after the canonical `HALFOPEN_DWELL_MS`
//!    (2 min) logical wait + signal recovery, the breaker transitions
//!    Open → HalfOpen. Probe success → HalfOpen → Closed. Probe failure
//!    → HalfOpen → Open (immediate revert).
//! 5. **Back-pressure** — the in-test [`BackPressureQueue`] rejects when
//!    queue depth > threshold with `503 Service Unavailable` +
//!    `Retry-After` (mirrors the production Tower bounded mpsc layer).
//! 6. **Audit-fail-CLOSED at the breaker layer** — every state
//!    transition emits the canonical audit record BEFORE the state
//!    mutation visible to the next caller; pinned by a failing
//!    [`FailingCircuitAuditSink`] that aborts the decision arm.
//!
//! # Charter constraints
//!
//! - `#![forbid(unsafe_code)]`
//! - No `unwrap` / `expect` / `panic` outside `#[cfg(test)]` (enforced
//!   crate-strict via `Cargo.toml` lints).
//! - No `tokio` in `src/` (the orchestrators are sync; the suite drives
//!   them in-process).
//! - Logical clock only — never `SystemTime::now()`.
//! - Seeded RNG only — the breaker's HalfOpen 10% sampler is
//!   deterministic (`observation_id mod 10`); the back-pressure queue
//!   draws no randomness.
//! - Audit fail-CLOSED ordering enforced + tested (scenario 6).
//!
//! [`FailingCircuitAuditSink`]:
//! corelink_rate_headers::FailingCircuitAuditSink

#![forbid(unsafe_code)]

use std::sync::Mutex;

use thiserror::Error;

/// Logical clock seeded at the `t0_ms` instant; advances monotonically
/// via [`LogicalClock::advance_ms`]. The harness uses this in lieu of
/// `SystemTime::now()` so scenarios are byte-for-byte reproducible.
#[derive(Debug)]
pub struct LogicalClock {
    inner: Mutex<u64>,
}

impl LogicalClock {
    /// Construct a fresh clock pinned at `t0_ms` (Unix-epoch ms).
    #[must_use]
    pub fn new(t0_ms: u64) -> Self {
        Self {
            inner: Mutex::new(t0_ms),
        }
    }

    /// Read the current logical instant.
    ///
    /// # Errors
    ///
    /// Returns [`ResilienceError::ClockPoisoned`] when the inner mutex
    /// is poisoned (only reachable on a panic in a prior `now_ms` /
    /// `advance_ms` call — impossible in production because this crate
    /// has no `panic!` sites).
    pub fn now_ms(&self) -> Result<u64, ResilienceError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| ResilienceError::ClockPoisoned)?;
        Ok(*g)
    }

    /// Advance the clock by `delta_ms` and return the new instant.
    ///
    /// Saturates on `u64` overflow (the Unix epoch + 2^64 ms is several
    /// hundred million years into the future; saturation is the
    /// canonical defensive behaviour).
    ///
    /// # Errors
    ///
    /// Returns [`ResilienceError::ClockPoisoned`] on inner mutex
    /// poisoning.
    pub fn advance_ms(&self, delta_ms: u64) -> Result<u64, ResilienceError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| ResilienceError::ClockPoisoned)?;
        *g = g.saturating_add(delta_ms);
        Ok(*g)
    }
}

/// Canonical HTTP status discriminator used by the harness for the
/// scenario assertions. The production Tower layer renders the same
/// mapping; we mirror it here as a typed value so tests assert on the
/// canonical mapping rather than on a raw `u16`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HttpStatus {
    /// `200 OK` — request admitted by the camada under test.
    Ok,
    /// `429 Too Many Requests` — rate-limit camada (camada 1 / camada
    /// 0 reject path; RFC 6585 §4).
    TooManyRequests,
    /// `503 Service Unavailable` — back-pressure reject OR camada-0
    /// circuit Open / HalfOpen fast-reject (the canonical Tower layer
    /// maps both circuit `Reject` arms to 503).
    ServiceUnavailable,
}

impl HttpStatus {
    /// Canonical numeric code for cross-protocol regression assertions.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        match self {
            Self::Ok => 200,
            Self::TooManyRequests => 429,
            Self::ServiceUnavailable => 503,
        }
    }
}

/// Per-request response shape rendered by the harness. Composes a
/// `status` arm with the canonical `retry_after_secs` (RFC 6585 §4)
/// when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResilienceResponse {
    /// HTTP status discriminator.
    pub status: HttpStatus,
    /// Retry-After value (seconds; only present on `429` / `503` arms).
    pub retry_after_secs: Option<u64>,
}

impl ResilienceResponse {
    /// `200 OK` constructor — no Retry-After header on the admit arm.
    #[must_use]
    pub const fn ok() -> Self {
        Self {
            status: HttpStatus::Ok,
            retry_after_secs: None,
        }
    }

    /// `429 Too Many Requests` constructor.
    #[must_use]
    pub const fn too_many_requests(retry_after_secs: u64) -> Self {
        Self {
            status: HttpStatus::TooManyRequests,
            retry_after_secs: Some(retry_after_secs),
        }
    }

    /// `503 Service Unavailable` constructor (back-pressure /
    /// circuit Open).
    #[must_use]
    pub const fn service_unavailable(retry_after_secs: u64) -> Self {
        Self {
            status: HttpStatus::ServiceUnavailable,
            retry_after_secs: Some(retry_after_secs),
        }
    }
}

/// Minimal back-pressure adjudicator — bounded FIFO queue with audit
/// emission on every reject BEFORE the response is rendered. Mirrors
/// the canonical fail-CLOSED envelope: if audit fails the request
/// surfaces a typed error (NOT a 503) so the caller's circuit
/// observability is preserved.
///
/// Production wiring uses a bounded `mpsc::channel` + a Tower
/// `ConcurrencyLimit` layer; the algorithmic invariant is identical:
/// queue depth > threshold → reject with `Retry-After` AND emit the
/// rejection audit BEFORE rendering the response.
#[derive(Debug)]
pub struct BackPressureQueue {
    capacity: usize,
    depth: Mutex<usize>,
    rejects: Mutex<Vec<BackPressureRejectRecord>>,
    audit_should_fail: Mutex<bool>,
}

impl BackPressureQueue {
    /// Construct a fresh queue with the canonical capacity ceiling.
    /// `capacity` is the **inclusive** maximum depth; the
    /// `capacity + 1`th admit attempt rejects.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            depth: Mutex::new(0),
            rejects: Mutex::new(Vec::new()),
            audit_should_fail: Mutex::new(false),
        }
    }

    /// Snapshot the current queue depth (for invariant assertions).
    ///
    /// # Errors
    ///
    /// Returns [`ResilienceError::BackPressurePoisoned`] on inner
    /// mutex poisoning.
    pub fn depth(&self) -> Result<usize, ResilienceError> {
        let g = self
            .depth
            .lock()
            .map_err(|_| ResilienceError::BackPressurePoisoned)?;
        Ok(*g)
    }

    /// Snapshot all reject audit records (chronological order).
    ///
    /// # Errors
    ///
    /// Returns [`ResilienceError::BackPressurePoisoned`] on inner
    /// mutex poisoning.
    pub fn audit_snapshot(&self) -> Result<Vec<BackPressureRejectRecord>, ResilienceError> {
        let g = self
            .rejects
            .lock()
            .map_err(|_| ResilienceError::BackPressurePoisoned)?;
        Ok(g.clone())
    }

    /// Force the next audit emit to fail (drives scenario 6: audit
    /// fail-CLOSED ordering at the back-pressure boundary).
    ///
    /// # Errors
    ///
    /// Returns [`ResilienceError::BackPressurePoisoned`] on inner
    /// mutex poisoning.
    pub fn arm_audit_failure(&self) -> Result<(), ResilienceError> {
        let mut g = self
            .audit_should_fail
            .lock()
            .map_err(|_| ResilienceError::BackPressurePoisoned)?;
        *g = true;
        Ok(())
    }

    /// Attempt to admit one request. Returns:
    /// - `Ok(ResilienceResponse::ok())` when `depth < capacity` (admit
    ///   path increments depth).
    /// - `Ok(ResilienceResponse::service_unavailable(_))` when
    ///   `depth >= capacity` (reject path; audit emitted BEFORE the
    ///   response is rendered).
    /// - `Err(ResilienceError::AuditFailed)` if the armed audit
    ///   failure tripped the fail-CLOSED envelope on the reject path.
    ///
    /// # Errors
    ///
    /// Surface as [`ResilienceError`].
    pub fn try_admit(
        &self,
        request_id: u64,
        now_ms: u64,
        retry_after_secs: u64,
    ) -> Result<ResilienceResponse, ResilienceError> {
        let mut g = self
            .depth
            .lock()
            .map_err(|_| ResilienceError::BackPressurePoisoned)?;
        if *g < self.capacity {
            *g = g.saturating_add(1);
            return Ok(ResilienceResponse::ok());
        }
        // Reject path — audit emit BEFORE rendering the response.
        drop(g);
        {
            let mut fail_flag = self
                .audit_should_fail
                .lock()
                .map_err(|_| ResilienceError::BackPressurePoisoned)?;
            if *fail_flag {
                *fail_flag = false;
                return Err(ResilienceError::AuditFailed);
            }
        }
        let record = BackPressureRejectRecord {
            request_id,
            now_ms,
            retry_after_secs,
            event_type: "corelink.backpressure.rejected",
        };
        let mut records = self
            .rejects
            .lock()
            .map_err(|_| ResilienceError::BackPressurePoisoned)?;
        records.push(record);
        Ok(ResilienceResponse::service_unavailable(retry_after_secs))
    }

    /// Drain one admitted request from the queue (mirrors handler
    /// completion). No-op when depth is 0.
    ///
    /// # Errors
    ///
    /// Returns [`ResilienceError::BackPressurePoisoned`] on inner
    /// mutex poisoning.
    pub fn drain_one(&self) -> Result<(), ResilienceError> {
        let mut g = self
            .depth
            .lock()
            .map_err(|_| ResilienceError::BackPressurePoisoned)?;
        if *g > 0 {
            *g -= 1;
        }
        Ok(())
    }
}

/// Audit record emitted on every back-pressure reject (mirrors the
/// canonical fail-CLOSED audit-before-mutation envelope).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackPressureRejectRecord {
    /// Monotonic per-instance request id.
    pub request_id: u64,
    /// Logical-clock instant the reject fired.
    pub now_ms: u64,
    /// Retry-After value surfaced to the caller.
    pub retry_after_secs: u64,
    /// Canonical CloudEvents `type` discriminator string.
    pub event_type: &'static str,
}

/// Typed harness-level errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ResilienceError {
    /// The logical clock's inner mutex was poisoned.
    #[error("logical clock mutex poisoned")]
    ClockPoisoned,
    /// A back-pressure queue mutex was poisoned.
    #[error("back-pressure queue mutex poisoned")]
    BackPressurePoisoned,
    /// The audit sink failed and the fail-CLOSED envelope aborted the
    /// request.
    #[error("audit sink failed; fail-CLOSED envelope aborted")]
    AuditFailed,
}
