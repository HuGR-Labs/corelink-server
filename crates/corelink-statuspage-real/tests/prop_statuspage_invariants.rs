//! Property tests for `corelink-statuspage-real` pure-logic surfaces.
//!
//! WI-PROPTEST-FU-W33-002 closure for `corelink-statuspage-real`.
//!
//! Coverage map (each test pins a named invariant from `src/`):
//!
//! - `prop_inv_redact_api_key_never_leaks_prefix` (CTRL-PRIV-001) —
//!   `redact_api_key(k)` MUST NOT contain any prefix of `k` longer than
//!   the trailing 4 codepoints. The audit envelope carries only the
//!   redacted form; a regression that bleeds prefix bytes would leak
//!   the Statuspage `OAuth` credential into the audit trail.
//! - `prop_inv_dsr_completion_report_serde_roundtrip` — `serde_json`
//!   round-trip preserves the canonical 24h-rolling DSR completion
//!   payload byte-for-byte. The wire format is the canonical audit-
//!   envelope shape; a serde drift would silently corrupt the audit
//!   chain.
//! - `prop_inv_rate_limiter_denied_does_not_advance_window` —
//!   Atlassian Statuspage allows 1 publish per 5 min per metric. A
//!   `DenyBackoff` decision MUST NOT mutate `last_allowed` (otherwise
//!   the limiter would extend the quota indefinitely under sustained
//!   load).
//! - `prop_inv_retry_policy_classification_total_and_monotone` —
//!   `RetryPolicy::decide` MUST return one of `{Success, Retry,
//!   GiveUpAuth, GiveUp}` for every possible HTTP status, AND
//!   `backoff_for` MUST be monotone non-decreasing up to the
//!   `max_backoff` cap (exponential schedule; never regresses).

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as proptest failures by design"
)]

use std::time::Duration;

use corelink_statuspage_real::{
    redact_api_key, DsrCompletionReport, RateLimitDecision, RetryDecision, RetryPolicy,
    StatuspageRateLimiter,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 256 for
/// the PR gate; CI nightly + the `PROPTEST_CASES=256` stress run in §3
/// of the WI override.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

const RATE_LIMIT_WINDOW_MS: u64 = 5 * 60 * 1_000;

proptest! {
    #![proptest_config(ProptestConfig { cases: proptest_cases(), .. ProptestConfig::default() })]

    /// CTRL-PRIV-001 NEVER-LEAK invariant: the redacted form MUST NOT
    /// contain any prefix of the input key longer than the trailing 4
    /// codepoints (the canonical fingerprint window). Defends against a
    /// regression where the redactor accidentally echoes leading bytes
    /// into the audit envelope.
    #[test]
    fn prop_inv_redact_api_key_never_leaks_prefix(
        // Restrict to printable ASCII (Statuspage keys are ASCII; the
        // `chars().rev().take(4)` path also guards UTF-8 boundaries).
        key in "[ -~]{0,128}",
    ) {
        let redacted = redact_api_key(&key);
        let trimmed = key.trim();

        // Always begins with the canonical marker; never contains the
        // raw key with the `OAuth ` prefix dropped.
        prop_assert!(
            redacted.starts_with("OAuth "),
            "redacted MUST start with 'OAuth ': '{redacted}'"
        );

        // Empty / whitespace-only key ⇒ canonical "OAuth ***" with no
        // fingerprint — nothing to inspect further.
        if trimmed.is_empty() {
            prop_assert_eq!(redacted.as_str(), "OAuth ***");
            return Ok(());
        }

        // The canonical fingerprint is the last ≤4 codepoints of the
        // trimmed key. The redacted output MUST end with that tail and
        // MUST NOT contain any longer prefix slice of the trimmed key.
        let tail: String = trimmed.chars().rev().take(4).collect::<String>()
            .chars()
            .rev()
            .collect();
        prop_assert!(
            redacted.ends_with(&tail),
            "redacted MUST end with the ≤4-char fingerprint '{tail}': '{redacted}'"
        );

        // For any key whose codepoint length > 4, the leading bytes
        // (everything BEFORE the last 4 codepoints) MUST NOT appear in
        // the redacted output. We assemble that leading slice
        // explicitly (codepoint-safe).
        let total_cps = trimmed.chars().count();
        if total_cps > 4 {
            let leading_cp_len = total_cps - 4;
            let leading: String =
                trimmed.chars().take(leading_cp_len).collect();
            // Skip degenerate cases where `leading` is a substring of
            // the canonical marker `OAuth ` (e.g. leading == "O" or
            // "OA" if the user happens to seed those bytes — proptest
            // can hit that).
            if !leading.is_empty() && !"OAuth ***".contains(leading.as_str()) {
                prop_assert!(
                    !redacted.contains(leading.as_str()),
                    "redacted MUST NOT echo leading bytes '{leading}': '{redacted}'"
                );
            }
        }

        // The redacted output's length is bounded — `OAuth ***` (9
        // bytes) + at most the byte-length of the last 4 codepoints
        // (≤ 16 bytes for any UTF-8 codepoint).
        prop_assert!(
            redacted.len() <= 9 + 16,
            "redacted length must be bounded: '{redacted}'"
        );
    }

    /// INV-STATUSPAGE-WIRE-ROUNDTRIP: serde_json round-trip MUST
    /// preserve every field of the canonical 24h DsrCompletionReport.
    /// The audit envelope carries this payload; a serde drift would
    /// silently corrupt the audit chain (CTRL-AUDIT-001 by reference).
    #[test]
    fn prop_inv_dsr_completion_report_serde_roundtrip(
        window_start in 0u64..=1_000_000_000u64,
        verified_complete in 0u64..=1_000_000u64,
        verified_partial in 0u64..=1_000_000u64,
        sla_breached in 0u64..=1_000_000u64,
        p95 in 0u64..=7_200u64,
    ) {
        let window_end = window_start.saturating_add(86_400);
        let original = DsrCompletionReport::new(
            window_start,
            window_end,
            verified_complete,
            verified_partial,
            sla_breached,
            p95,
        ).expect("canonical 24h window + p95 in range");

        let json = serde_json::to_string(&original).expect("serialize");
        let restored: DsrCompletionReport =
            serde_json::from_str(&json).expect("deserialize");

        // Explicit field-by-field equality — NOT matches! (S-08 P1-1).
        prop_assert_eq!(restored.window_start_unix_s, original.window_start_unix_s);
        prop_assert_eq!(restored.window_end_unix_s, original.window_end_unix_s);
        prop_assert_eq!(
            restored.verified_complete_count,
            original.verified_complete_count
        );
        prop_assert_eq!(
            restored.verified_partial_count,
            original.verified_partial_count
        );
        prop_assert_eq!(restored.sla_breached_count, original.sla_breached_count);
        prop_assert_eq!(restored.p95_resolution_hours, original.p95_resolution_hours);

        // The wire body for Statuspage Public-Metric must use the
        // window END as the timestamp and the p95 as the value (per
        // Atlassian Statuspage API v1; canonical observation).
        let body = restored.to_metric_body();
        let data = body.get("data").expect("data key present");
        prop_assert_eq!(
            data.get("timestamp").and_then(|v| v.as_u64()),
            Some(window_end)
        );
        prop_assert_eq!(
            data.get("value").and_then(|v| v.as_u64()),
            Some(p95)
        );
    }

    /// INV-STATUSPAGE-RATELIMIT-NO-DRIFT: a `DenyBackoff` decision MUST
    /// NOT advance `last_allowed`. Concretely: after a first `Allow`
    /// records `t0`, any subsequent decide(t1) within the window MUST
    /// keep `last_allowed_ms == Some(t0)`. The next `Allow` ONLY fires
    /// at-or-after `t0 + RATE_LIMIT_WINDOW_MS`.
    #[test]
    fn prop_inv_rate_limiter_denied_does_not_advance_window(
        page in "[a-z0-9]{1,8}",
        metric in "[a-z0-9]{1,8}",
        t0 in 0u64..=u64::MAX / 4,
        // Random offsets within the window — these MUST all deny without
        // advancing last_allowed.
        offsets in proptest::collection::vec(
            1u64..RATE_LIMIT_WINDOW_MS, 0..8,
        ),
    ) {
        let limiter = StatuspageRateLimiter::new();
        let first = limiter.decide(&page, &metric, t0);
        prop_assert_eq!(first, RateLimitDecision::Allow);
        prop_assert_eq!(
            limiter.last_allowed_ms(&page, &metric),
            Some(t0)
        );

        for offset in &offsets {
            let t = t0.saturating_add(*offset);
            let d = limiter.decide(&page, &metric, t);
            // Must be DenyBackoff. Field-bind, not matches! (S-08).
            match d {
                RateLimitDecision::DenyBackoff { retry_after, jitter } => {
                    // retry_after must be in (0, WINDOW]; jitter MUST be
                    // exactly retry_after / 8 (deterministic helper).
                    let ra_ms = retry_after.as_millis() as u64;
                    let j_ms = jitter.as_millis() as u64;
                    prop_assert!(
                        ra_ms > 0 && ra_ms <= RATE_LIMIT_WINDOW_MS,
                        "retry_after out of range: {ra_ms}"
                    );
                    prop_assert_eq!(j_ms, ra_ms / 8);
                }
                other => prop_assert!(
                    false,
                    "expected DenyBackoff, got {other:?}"
                ),
            }
            // Window MUST NOT advance.
            prop_assert_eq!(
                limiter.last_allowed_ms(&page, &metric),
                Some(t0),
                "denied decision MUST NOT advance last_allowed"
            );
        }

        // Exactly at the boundary the next decide is allowed AND
        // advances the window.
        let boundary = t0.saturating_add(RATE_LIMIT_WINDOW_MS);
        let after = limiter.decide(&page, &metric, boundary);
        prop_assert_eq!(after, RateLimitDecision::Allow);
        prop_assert_eq!(
            limiter.last_allowed_ms(&page, &metric),
            Some(boundary)
        );
    }

    /// INV-STATUSPAGE-RETRY-TOTAL-AND-MONOTONE: `RetryPolicy::decide`
    /// is total over all u16 HTTP statuses (every input maps to exactly
    /// one canonical variant) AND `backoff_for(attempt)` is monotone
    /// non-decreasing in `attempt` up to the `max_backoff` cap.
    #[test]
    fn prop_inv_retry_policy_classification_total_and_monotone(
        status_seed in any::<u16>(),
        attempt in 0u32..=10u32,
    ) {
        let p = RetryPolicy::wave16_default();

        // Totality: every status maps to one of the four canonical
        // variants (the compiler-level totality is enforced by the
        // match in `decide`; we re-verify at runtime + exercise both
        // the `Some(status)` and `None` (transport-error) arms).
        let s_decision = p.decide(Some(status_seed), attempt);
        match (status_seed, attempt < p.max_retries) {
            (s, _) if (200..300).contains(&s) => {
                prop_assert_eq!(s_decision, RetryDecision::Success);
            }
            (401, _) | (403, _) => {
                prop_assert_eq!(s_decision, RetryDecision::GiveUpAuth);
            }
            (429, true) | (500..=599, true) => {
                match s_decision {
                    RetryDecision::Retry { delay } => {
                        prop_assert!(delay >= Duration::ZERO);
                        prop_assert!(delay <= p.max_backoff);
                    }
                    other => prop_assert!(
                        false,
                        "expected Retry, got {other:?}"
                    ),
                }
            }
            (429, false) | (500..=599, false) => {
                prop_assert_eq!(s_decision, RetryDecision::GiveUp);
            }
            _ => {
                // Other 4xx + 1xx + 3xx + edge u16s all collapse to
                // GiveUp (permanent reject).
                prop_assert_eq!(s_decision, RetryDecision::GiveUp);
            }
        }

        // Transport error (status==None) arm: retries while attempt <
        // max_retries, then GiveUp.
        let none_decision = p.decide(None, attempt);
        if attempt < p.max_retries {
            match none_decision {
                RetryDecision::Retry { delay } => {
                    prop_assert!(delay <= p.max_backoff);
                }
                other => prop_assert!(
                    false,
                    "transport error must retry, got {other:?}"
                ),
            }
        } else {
            prop_assert_eq!(none_decision, RetryDecision::GiveUp);
        }

        // Backoff schedule: monotone non-decreasing under the cap. We
        // compare `attempt` vs `attempt+1` to pin the property.
        let b0 = p.backoff_for(attempt);
        let b1 = p.backoff_for(attempt.saturating_add(1));
        prop_assert!(
            b1 >= b0 || b0 == p.max_backoff,
            "backoff must be monotone (or saturated at cap): \
             b({attempt})={b0:?}, b({})={b1:?}",
            attempt + 1,
        );
        prop_assert!(
            b0 <= p.max_backoff,
            "backoff must be capped at max_backoff"
        );
    }
}
