//! Property tests for the rate-limit JSON/body and HTTP-header mirror contract.
//!
//! This is kept separate from the circuit-breaker properties because it owns a
//! distinct wire-format responsibility: every 429 body must mirror the
//! canonical headers, including frozen vendor URLs and JSON escaping.

use corelink_rate_headers::{
    canonical_kind_list, RateLimitErrorBody, RateLimitHeaderBuilder, RateLimitPolicy,
    XRateLimitTypeKind, DOCS_URL, ERROR_CODE_RATE_LIMIT_EXCEEDED, RETRY_AFTER_HARD_CEILING_SECS,
    TIER_UPGRADE_URL,
};
use proptest::prelude::*;

fn kind_strategy() -> impl proptest::strategy::Strategy<Value = XRateLimitTypeKind> {
    let kinds = canonical_kind_list();
    (0_usize..kinds.len()).prop_map(move |i| kinds[i])
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: super::proptest_cases(),
        .. ProptestConfig::default()
    })]

    /// Body and headers are ALWAYS consistent on every 429.
    #[test]
    fn prop_rate_limit_body_always_consistent_with_headers(
        limit in 0_u64..1_000_000,
        remaining in 0_u64..1_000_000,
        reset_secs in 0_u64..1_000_000,
        retry_after in 0_u64..(RETRY_AFTER_HARD_CEILING_SECS + 1_000),
        kind in kind_strategy(),
        tier in proptest::sample::select(vec![
            String::new(),
            String::from("free"),
            String::from("solo"),
            String::from("team"),
            String::from("business"),
            String::from("enterprise"),
        ]),
        reset_utc in "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z",
        request_id in "[0-9A-HJKMNP-TV-Z]{0,26}",
        message in ".{0,128}",
    ) {
        let h = RateLimitHeaderBuilder::build_with_vendor(
            limit,
            remaining,
            reset_secs,
            vec![RateLimitPolicy::new(limit.max(1), 60)],
            retry_after,
            kind,
            tier.clone(),
            reset_utc.clone(),
        );
        let body = RateLimitErrorBody::from_headers(
            &h,
            message.clone(),
            request_id.clone(),
        );

        // INV-BODY-HEADER-MIRROR-1: discriminator matches.
        prop_assert_eq!(body.kind, h.x_rate_limit_type);
        prop_assert_eq!(body.kind.as_str(), h.render_x_rate_limit_type());

        // INV-BODY-HEADER-MIRROR-2: retry-after matches.
        prop_assert_eq!(body.retry_after_seconds, h.retry_after_secs);
        prop_assert_eq!(
            body.retry_after_seconds.to_string(),
            h.render_retry_after()
        );

        // INV-BODY-HEADER-MIRROR-3: RFC 9331 limit / remaining / reset.
        prop_assert_eq!(body.limit, h.limit);
        prop_assert_eq!(body.remaining, h.remaining);
        prop_assert_eq!(body.reset_seconds, h.reset_secs);

        // INV-BODY-HEADER-MIRROR-4: vendor fields mirror exactly.
        prop_assert_eq!(&body.tier, &h.corelink_tier);
        prop_assert_eq!(&body.reset_utc, &h.corelink_quota_reset_utc);

        // INV-BODY-FROZEN-URLS: tier_upgrade_url + docs_url canonical.
        prop_assert_eq!(body.tier_upgrade_url, TIER_UPGRADE_URL);
        prop_assert_eq!(body.docs_url, DOCS_URL);
        prop_assert_eq!(
            h.corelink_tier_upgrade_url,
            "https://corelink-docs.humangr.com/pricing",
        );

        // INV-BODY-STABLE-CODE: at GA there is exactly one stable code.
        prop_assert_eq!(body.code, ERROR_CODE_RATE_LIMIT_EXCEEDED);

        // INV-BODY-RENDER-WELL-FORMED: JSON envelope balanced.
        let json = body.render_json();
        let envelope_prefix = "{\"error\":{";
        let envelope_suffix = "}}";
        prop_assert!(
            json.starts_with(envelope_prefix),
            "json missing envelope prefix"
        );
        prop_assert!(
            json.ends_with(envelope_suffix),
            "json missing envelope suffix"
        );
        // No raw control characters leaked into the output (every
        // < 0x20 char MUST have been escaped per RFC 8259).
        for byte in json.bytes() {
            prop_assert!(
                byte >= 0x20,
                "raw control byte {byte:#x} leaked into JSON",
            );
        }
    }
}
