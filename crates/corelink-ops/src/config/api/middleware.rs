//! Middleware for the config admin API (WI-S13-001).
//!
//! Currently ships:
//! - [`mfa_freshness`]: MFA session freshness guard (CTRL-AUTH-010).

pub mod mfa_freshness;
