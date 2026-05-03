//! RFC 9331 RateLimit + RateLimit-Policy header builder + the canonical
//! 5-arm `XRateLimitTypeKind` response-code taxonomy + `RateLimitHeaders`
//! payload (the typed shape the production Tower middleware writes onto
//! every 429 response — and informationally onto 200 responses for
//! customer SDK pre-emptive backoff).
//!
//! ## RFC 9331 IETF stable canonical
//!
//! Per sprint contract §14.s08.5, the canonical IETF format is
//! `RateLimit: limit=N, remaining=M, reset=S` (where `S` is
//! seconds-until-refill); the policy disclosure header is
//! `RateLimit-Policy: <limit>;w=<window>` (multiple policies allowed via
//! comma-separation, e.g. `100;w=60, 1000;w=3600`). Legacy custom
//! `X-RateLimit-*` headers are **NOT emitted** to avoid customer SDK
//! ambiguity; vendors (Bazel / Buck2 / Stripe / GitHub) consume RFC 9331
//! directly so adopting the IETF canonical = customer SDK works without
//! per-vendor adaptation.
//!
//! ## 5-arm `XRateLimitTypeKind` taxonomy
//!
//! Per sprint contract §5 R-S08-8, every 429 response carries a typed
//! discriminator (`X-Rate-Limit-Type`) signalling which camada layer
//! denied the request:
//!
//! - `tenant_quota` (camada 1; per-tenant DO; WI-S08-001) — within-plan
//!   429; SLI failure (bug nosso; counted in SLO-AVAIL-CAS-GET
//!   denominator per sprint contract §7.10.s08.1).
//! - `per_ip` (camada 2; CF edge; WI-S08-002) — adversarial IP flood;
//!   legitimate edge defense; NOT in SLI denominator.
//! - `per_pat` (camada 3; PAT misuse; WI-S08-003) — compromised
//!   credential signal; legitimate; NOT in SLI denominator.
//! - `over_quota` (camada 3; storage / bandwidth 100% boundary;
//!   WI-S08-003) — tenant exceeded plan; legitimate over-plan; NOT in
//!   SLI denominator (sprint contract §7.10.s08.1 critical SLI
//!   correctness fix).
//! - `global_circuit_open` (camada 0; THIS WI camada-0 emergency) —
//!   system-wide overload; 429 all requests; SLI failure (counted in
//!   SLI denominator EXCEPT when trip reason is ManualOverride; sprint
//!   contract §7.10.s08.1 + Lote 10.8bis P1-3 ManualOverride exclusion
//!   for planned drills).
//!
//! ## SLI distinction critical (sprint contract §7.10.s08.1)
//!
//! [`XRateLimitTypeKind::counts_against_sli`] returns the canonical
//! within-quota vs over-quota distinction the SLO numerator depends on.
//! Without this, error budget exhausts on legitimate over-plan paths +
//! obscures real failures (`tenant_quota` 429s being counted as bug
//! nosso). The corresponding metrics emit through
//! [`crate::metrics::CircuitMetricsObserver::record_within_quota`]
//! (counts in SLI) vs `record_over_quota` (legitimate; NOT counted).

/// Canonical 5-arm response-code taxonomy per sprint contract §5 R-S08-8.
///
/// Carried in the `X-Rate-Limit-Type` HTTP header on every 429 response;
/// the tower middleware in production wiring derives the kind from the
/// camada that denied the request.
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs (e.g.
/// per-region scoping; cross-region federation deferred S-14 per sprint
/// contract §10 anti-scope).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum XRateLimitTypeKind {
    /// Camada 1; per-tenant DO; WI-S08-001 within-plan 429.
    TenantQuota,
    /// Camada 2; CF edge; WI-S08-002 adversarial IP flood.
    PerIp,
    /// Camada 3; per-PAT misuse signal; WI-S08-003.
    PerPat,
    /// Camada 3; storage / bandwidth 100% boundary; WI-S08-003.
    OverQuota,
    /// Camada 0; this WI emergency global circuit open.
    GlobalCircuitOpen,
}

impl XRateLimitTypeKind {
    /// Canonical `X-Rate-Limit-Type` header string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TenantQuota => "tenant_quota",
            Self::PerIp => "per_ip",
            Self::PerPat => "per_pat",
            Self::OverQuota => "over_quota",
            Self::GlobalCircuitOpen => "global_circuit_open",
        }
    }

    /// SLI distinction predicate (sprint contract §7.10.s08.1):
    /// returns `true` when this 429 type counts in the
    /// SLO-AVAIL-CAS-GET denominator (within-quota; bug nosso) and
    /// `false` when it is legitimate over-plan / edge defense (NOT
    /// counted; per sprint contract §7.10.s08.1 SLI correctness fix).
    ///
    /// `is_manual_override` (per Lote 10.8bis P1-3 R5 fix) excludes
    /// planned-drill ManualOverride trips from the denominator — those
    /// are intentional load-shed actions, not system overload bugs.
    /// Callers MUST pass `true` only when the underlying trip reason is
    /// `ManualOverride`.
    #[must_use]
    pub const fn counts_against_sli(
        self,
        is_manual_override: bool,
    ) -> bool {
        match self {
            Self::TenantQuota => true,
            Self::GlobalCircuitOpen => !is_manual_override,
            Self::PerIp | Self::PerPat | Self::OverQuota => false,
        }
    }
}

impl core::fmt::Display for XRateLimitTypeKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 5-arm enumeration for cross-component regression tests +
/// dashboard widget configuration. The order mirrors sprint contract
/// §5 R-S08-8 textual listing.
#[must_use]
pub const fn canonical_kind_list() -> [XRateLimitTypeKind; 5] {
    [
        XRateLimitTypeKind::TenantQuota,
        XRateLimitTypeKind::PerIp,
        XRateLimitTypeKind::PerPat,
        XRateLimitTypeKind::OverQuota,
        XRateLimitTypeKind::GlobalCircuitOpen,
    ]
}

/// One RFC 9331 `RateLimit-Policy` policy entry.
///
/// Format: `<limit>;w=<window>` (e.g. `100;w=60` = 100 requests per 60s
/// window). Multiple policies serialise as comma-separated tuples per
/// RFC 9331 §3 (e.g. `100;w=60, 1000;w=3600`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitPolicy {
    /// Token-bucket capacity ceiling (the `<limit>` field).
    pub limit: u64,
    /// Refill window in seconds (the `w=<window>` parameter).
    pub window_secs: u64,
}

impl RateLimitPolicy {
    /// Construct a policy with explicit fields.
    #[must_use]
    pub const fn new(limit: u64, window_secs: u64) -> Self {
        Self { limit, window_secs }
    }

    /// Render as the canonical RFC 9331 textual entry
    /// (`<limit>;w=<window>`).
    #[must_use]
    pub fn render(&self) -> String {
        format!("{};w={}", self.limit, self.window_secs)
    }
}

/// Typed RFC 9331 + Retry-After + X-Rate-Limit-Type header payload.
///
/// The production Tower middleware writes these onto the `http::Response`
/// builder; the in-memory crate exposes the typed shape so tests can
/// assert against fields rather than string-grep across the rendered
/// header bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RateLimitHeaders {
    /// RFC 9331 `RateLimit: limit=…` field.
    pub limit: u64,
    /// RFC 9331 `RateLimit: remaining=…` field. Per the IETF draft this
    /// is the integer floor of available tokens at the boundary.
    pub remaining: u64,
    /// RFC 9331 `RateLimit: reset=…` field — seconds-until-refill.
    pub reset_secs: u64,
    /// RFC 9331 `RateLimit-Policy` policies (≥ 1; production wiring
    /// emits one entry per active camada the caller is subject to).
    pub policies: Vec<RateLimitPolicy>,
    /// RFC 6585 §4 `Retry-After` (always present per sprint contract
    /// §5 R-S08-9).
    pub retry_after_secs: u64,
    /// CoreLink-specific `X-Rate-Limit-Type` discriminator (5-arm).
    pub x_rate_limit_type: XRateLimitTypeKind,
}

impl RateLimitHeaders {
    /// Render the canonical `RateLimit:` value (no header name; the
    /// production wiring prepends `RateLimit:` via the `http::Response`
    /// builder).
    ///
    /// Format: `limit=N, remaining=M, reset=S` (RFC 9331 §2 stable).
    #[must_use]
    pub fn render_rate_limit(&self) -> String {
        format!(
            "limit={}, remaining={}, reset={}",
            self.limit, self.remaining, self.reset_secs
        )
    }

    /// Render the canonical `RateLimit-Policy:` value (no header name).
    ///
    /// Format: `<limit>;w=<window>, <limit>;w=<window>, …` (RFC 9331 §3).
    /// Returns the empty string when `policies` is empty (the production
    /// wiring SHOULD always emit ≥ 1 policy; the empty case is a
    /// degraded fallback for the carrier-doesn't-know-the-policy edge).
    #[must_use]
    pub fn render_rate_limit_policy(&self) -> String {
        let mut out = String::new();
        let mut first = true;
        for p in &self.policies {
            if !first {
                out.push_str(", ");
            }
            first = false;
            out.push_str(&p.render());
        }
        out
    }

    /// Render the `X-Rate-Limit-Type` value.
    #[must_use]
    pub const fn render_x_rate_limit_type(&self) -> &'static str {
        self.x_rate_limit_type.as_str()
    }

    /// Render the `Retry-After` value (RFC 6585 §4 seconds form).
    #[must_use]
    pub fn render_retry_after(&self) -> String {
        self.retry_after_secs.to_string()
    }
}

/// RFC 9331 + Retry-After header builder.
///
/// Composes [`RateLimitHeaders`] from per-camada bucket state +
/// per-request decision context. The canonical entry is
/// [`RateLimitHeaderBuilder::for_decision`] which renders the full
/// 5-tuple typed payload. The render-stage helpers
/// ([`RateLimitHeaders::render_rate_limit`] etc.) ship the canonical
/// strings the production Tower middleware writes.
///
/// Per WI §6.1.4 Retry-After is type-specific:
///
/// - `tenant_quota`: from WI-S08-001 RateLimitResult.retry_after_seconds
///   (canonical `(amount_needed - tokens_remaining) / refill_rate`).
/// - `per_ip`: 60s canonical (CF Ruleset mitigation_timeout).
/// - `per_pat`: from WI-S08-003 PatRateResult.retry_after_seconds.
/// - `over_quota` (storage): days-until-month-reset (canonical per
///   ADR-0020 FROZEN; emitted by `corelink-quota-cas`).
/// - `over_quota` (bandwidth): seconds-until-next-month-1st-UTC.
/// - `global_circuit_open`: 60s canonical (hysteresis sample interval).
#[derive(Clone, Copy, Debug, Default)]
pub struct RateLimitHeaderBuilder;

/// Canonical Retry-After for the `per_ip` camada (CF Ruleset
/// mitigation_timeout per WI §6.1.4).
pub const PER_IP_RETRY_AFTER_SECS: u64 = 60;

/// Canonical Retry-After for the `global_circuit_open` camada (hysteresis
/// sample interval per WI §6.1.4 + the canonical 2min HalfOpen dwell
/// floor — 60s is the SAMPLE interval, NOT the dwell; the SDK SHOULD
/// retry at 60s where it has another chance to land in HalfOpen sample).
pub const GLOBAL_CIRCUIT_RETRY_AFTER_SECS: u64 = 60;

/// Hard upper bound on Retry-After across all camadas (sprint contract
/// §6 DoD: < 30 days). Headers builder clamps so `over_quota` storage
/// never emits > 30 days even if the days-until-month-reset arithmetic
/// glitches at the month boundary.
pub const RETRY_AFTER_HARD_CEILING_SECS: u64 = 30 * 24 * 60 * 60;

impl RateLimitHeaderBuilder {
    /// Build a typed [`RateLimitHeaders`] payload from the per-camada
    /// decision context.
    ///
    /// `limit` is the bucket capacity (RFC 9331 `limit=`).
    /// `remaining` is `available_tokens.floor()` (RFC 9331 `remaining=`).
    /// `reset_secs` is seconds-until-refill (RFC 9331 `reset=`).
    /// `policies` is one entry per active camada (≥ 1).
    /// `retry_after_secs` is the type-specific value per WI §6.1.4
    /// (clamped to [`RETRY_AFTER_HARD_CEILING_SECS`]).
    /// `kind` is the 5-arm taxonomy discriminator.
    #[must_use]
    pub fn build(
        limit: u64,
        remaining: u64,
        reset_secs: u64,
        policies: Vec<RateLimitPolicy>,
        retry_after_secs: u64,
        kind: XRateLimitTypeKind,
    ) -> RateLimitHeaders {
        let clamped_retry_after =
            retry_after_secs.min(RETRY_AFTER_HARD_CEILING_SECS);
        // remaining is structurally clamped to ≤ limit per RFC 9331 §2.
        let clamped_remaining = remaining.min(limit);
        RateLimitHeaders {
            limit,
            remaining: clamped_remaining,
            reset_secs,
            policies,
            retry_after_secs: clamped_retry_after,
            x_rate_limit_type: kind,
        }
    }

    /// Convenience for the global-circuit `429 GlobalCircuitOpen` arm:
    /// limit + remaining are both 0 (system-wide reject); reset = 60s
    /// (canonical hysteresis sample interval); retry_after = 60s.
    #[must_use]
    pub fn for_global_circuit_open(
        policies: Vec<RateLimitPolicy>,
    ) -> RateLimitHeaders {
        Self::build(
            0,
            0,
            GLOBAL_CIRCUIT_RETRY_AFTER_SECS,
            policies,
            GLOBAL_CIRCUIT_RETRY_AFTER_SECS,
            XRateLimitTypeKind::GlobalCircuitOpen,
        )
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_has_unique_canonical_string() {
        let v = canonical_kind_list();
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(set.insert(k.as_str()));
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn canonical_strings_match_sprint_contract_taxonomy() {
        assert_eq!(XRateLimitTypeKind::TenantQuota.as_str(), "tenant_quota");
        assert_eq!(XRateLimitTypeKind::PerIp.as_str(), "per_ip");
        assert_eq!(XRateLimitTypeKind::PerPat.as_str(), "per_pat");
        assert_eq!(XRateLimitTypeKind::OverQuota.as_str(), "over_quota");
        assert_eq!(
            XRateLimitTypeKind::GlobalCircuitOpen.as_str(),
            "global_circuit_open"
        );
    }

    #[test]
    fn sli_distinction_per_kind_canonical_5_arm() {
        // Within-quota 429 (counted in SLI denominator):
        assert!(XRateLimitTypeKind::TenantQuota.counts_against_sli(false));
        assert!(XRateLimitTypeKind::TenantQuota.counts_against_sli(true));
        // Over-quota / edge / PAT (NOT counted):
        assert!(!XRateLimitTypeKind::PerIp.counts_against_sli(false));
        assert!(!XRateLimitTypeKind::PerPat.counts_against_sli(false));
        assert!(!XRateLimitTypeKind::OverQuota.counts_against_sli(false));
        // GlobalCircuitOpen: counted UNLESS ManualOverride (Lote 10.8bis
        // P1-3 R5 planned-drill exclusion):
        assert!(
            XRateLimitTypeKind::GlobalCircuitOpen.counts_against_sli(false)
        );
        assert!(
            !XRateLimitTypeKind::GlobalCircuitOpen.counts_against_sli(true)
        );
    }

    #[test]
    fn render_rate_limit_canonical_format() {
        let h = RateLimitHeaderBuilder::build(
            100,
            42,
            60,
            vec![RateLimitPolicy::new(100, 60)],
            5,
            XRateLimitTypeKind::TenantQuota,
        );
        assert_eq!(h.render_rate_limit(), "limit=100, remaining=42, reset=60");
    }

    #[test]
    fn render_rate_limit_policy_single() {
        let h = RateLimitHeaderBuilder::build(
            100,
            0,
            60,
            vec![RateLimitPolicy::new(100, 60)],
            60,
            XRateLimitTypeKind::TenantQuota,
        );
        assert_eq!(h.render_rate_limit_policy(), "100;w=60");
    }

    #[test]
    fn render_rate_limit_policy_multiple() {
        let h = RateLimitHeaderBuilder::build(
            100,
            0,
            60,
            vec![
                RateLimitPolicy::new(100, 60),
                RateLimitPolicy::new(1000, 3600),
            ],
            60,
            XRateLimitTypeKind::TenantQuota,
        );
        assert_eq!(
            h.render_rate_limit_policy(),
            "100;w=60, 1000;w=3600"
        );
    }

    #[test]
    fn render_x_rate_limit_type_returns_canonical() {
        let h = RateLimitHeaderBuilder::build(
            100,
            0,
            60,
            vec![],
            60,
            XRateLimitTypeKind::PerIp,
        );
        assert_eq!(h.render_x_rate_limit_type(), "per_ip");
    }

    #[test]
    fn render_retry_after_seconds_form() {
        let h = RateLimitHeaderBuilder::build(
            100,
            0,
            60,
            vec![],
            42,
            XRateLimitTypeKind::TenantQuota,
        );
        assert_eq!(h.render_retry_after(), "42");
    }

    #[test]
    fn build_clamps_retry_after_to_hard_ceiling() {
        let h = RateLimitHeaderBuilder::build(
            100,
            0,
            60,
            vec![],
            u64::MAX,
            XRateLimitTypeKind::OverQuota,
        );
        assert_eq!(h.retry_after_secs, RETRY_AFTER_HARD_CEILING_SECS);
    }

    #[test]
    fn build_clamps_remaining_to_limit() {
        let h = RateLimitHeaderBuilder::build(
            100,
            500,
            60,
            vec![],
            5,
            XRateLimitTypeKind::TenantQuota,
        );
        assert_eq!(h.remaining, 100);
    }

    #[test]
    fn for_global_circuit_open_canonical_60s() {
        let h = RateLimitHeaderBuilder::for_global_circuit_open(vec![
            RateLimitPolicy::new(100, 60),
        ]);
        assert_eq!(h.limit, 0);
        assert_eq!(h.remaining, 0);
        assert_eq!(h.reset_secs, 60);
        assert_eq!(h.retry_after_secs, 60);
        assert_eq!(h.x_rate_limit_type, XRateLimitTypeKind::GlobalCircuitOpen);
    }

    #[test]
    fn rate_limit_policy_render_format() {
        let p = RateLimitPolicy::new(200, 60);
        assert_eq!(p.render(), "200;w=60");
    }

    #[test]
    fn empty_policies_render_to_empty_string() {
        let h = RateLimitHeaderBuilder::build(
            100,
            0,
            60,
            vec![],
            5,
            XRateLimitTypeKind::TenantQuota,
        );
        assert_eq!(h.render_rate_limit_policy(), "");
    }

    #[test]
    fn display_kind_matches_as_str() {
        assert_eq!(
            format!("{}", XRateLimitTypeKind::TenantQuota),
            "tenant_quota"
        );
    }
}
