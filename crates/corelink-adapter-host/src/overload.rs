//! Shared load-shed response semantics for the cache adapter surfaces.
//!
//! The container's PAT verifier bounds concurrent Argon2id work with a small
//! permit pool (global + per-tenant). When a request cannot enter that pool
//! within the bounded wait the verifier **sheds** it: it gives up BEFORE
//! deciding anything about the credential and returns
//! `VerifyError::Backend("pat verifier overloaded")`.
//!
//! Every adapter surface (cargo / brew / npm / pip) maps that shed onto a
//! dedicated `503 Service Unavailable` variant carrying
//! [`SHED_RETRY_AFTER_SECS`], for two reasons:
//!
//! 1. **It is not an auth failure.** Surfacing a shed as `401` tells a client
//!    holding a perfectly valid PAT that its credential is bad — the live
//!    defect this module exists to close. `503 + Retry-After` is the honest,
//!    retryable answer, and it keeps the shed uniform across row existence
//!    (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM` — see the module header of
//!    `crates/corelink-container/src/adapter_pat.rs`).
//! 2. **The shed converges fast.** Once the first in-flight verify completes it
//!    populates the verifier's `SecretMatchMemo`, and every later request for
//!    that PAT takes the warm path, which consumes NO permit at all. A one
//!    second back-off is therefore enough to clear a cold burst.

/// `Retry-After` (seconds) attached to every adapter shed response.
///
/// Mirrors the house convention's floor — `corelink_ratelimit::config::
/// DEFAULT_RETRY_AFTER_FLOOR_SECS` (RFC 6585 §4 minimum granularity: 1 s, so a
/// hot retry loop cannot busy-spin on `Retry-After: 0`). Duplicated as a const
/// rather than imported because `corelink-adapter-host` deliberately does not
/// depend on `corelink-ratelimit`; the value is the RFC floor, not a tunable.
///
/// One second (rather than the quota paths' "wait for the cycle to reset") is
/// correct here because a permit-pool shed is a sub-second condition: it clears
/// as soon as an in-flight Argon2id finishes and the memo is populated.
pub const SHED_RETRY_AFTER_SECS: u64 = 1;
